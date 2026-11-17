import Foundation
import CExact

struct RasterSourceKey: Hashable {
    let name: String
    let resolver: ObjectIdentifier
}
/// One decode of a source: the core's entry, less the source.
struct RasterDecodeKey: Hashable {
    let width: Int, height: Int, variant: UInt32
}

final class RasterSource: RasterSourceOwner, @unchecked Sendable {
    let id: UInt64
    let key: RasterSourceKey
    var name: String { key.name }
    weak var resolver: AssetResolver?
    // Mutable preparation facts are accessed only under the backend lock.
    var users = 0
    var inspecting = false
    var metadata: RasterMetadata?
    var input: RasterInput?
    /// A remote source's downloaded body, or why not, for a worker to read
    /// the header of (`RasterBackend.inspectOne`).
    var fetched: Result<RasterInput, Error>?
    var cancellation = RasterCancellation()
    var failure: String?
    /// Its download failed in a way asking again may mend: not before this
    /// (its server's `Retry-After`, or the failure's time) (`RasterRetry`).
    var transient: DispatchTime?
    /// A view that fills the screen shows it: its download goes first.
    var urgent = false
    /// The decodes whose last attempt failed in ImageIO or Core Graphics over
    /// bytes whose size it read (`RasterFailure.decode`): a busy system does
    /// that, so their views ask again before the `error`
    /// (`RasterLoader.declineDelays`). One per decode, as two views can ask
    /// for two sizes and one can land while the other is declined.
    var declined: Set<RasterDecodeKey> = []
    /// An SVG's device pixels per CSS pixel by decode, which the core's work
    /// does not carry (LLP 1011.001 D3): the worker draws the scene at the
    /// plan's size over this. Under the backend lock.
    var svgScales: [RasterDecodeKey: Double] = [:]
    init(id: UInt64, name: String, resolver: AssetResolver) {
        self.id = id; key = RasterSourceKey(name: name, resolver: ObjectIdentifier(resolver)); self.resolver = resolver
    }
}
/// What admission answered: a source, a cap that frees as inspections and
/// views finish (the caller waits), or a refusal no retry changes.
enum RasterAcquire {
    case source(RasterSource)
    case busy
    case refused
}
final class WeakRasterSource {
    weak var value: RasterSource?
    init(_ value: RasterSource) { self.value = value }
}

/// The backend holds bounded metadata/descriptors only. A worker obtains its
/// own bytes after admission. Sources never retain a resolver or encoded blob.
final class RasterBackend: @unchecked Sendable {
    let id: UInt64
    let lock = NSLock()
    weak var loader: RasterLoader?
    var sources: [UInt64: WeakRasterSource] = [:]
    var byName: [RasterSourceKey: UInt64] = [:]
    private var cold: [RasterSource] = []
    static let defaultSourceLimit = 1152
    let sourceLimit: Int
    let metadataLimit: Int
    var serial: UInt64 = 0
    var stopped = false
    var paused = false
    var wakePending = false
    var metadataReads = 0
    var decoded = 0
    var refusals = 0
    #if DEBUG
    /// Decodes still to fail as ImageIO's do on a busy system.
    var testDeclines = 0
    #endif
    private var budgetNotice: [UInt64] = []
    init(id: UInt64, sourceLimit: Int = defaultSourceLimit, metadataLimit: Int = 64) {
        self.id = id; self.sourceLimit = sourceLimit; self.metadataLimit = metadataLimit
    }

    func acquire(_ name: String, resolver: AssetResolver) -> RasterAcquire {
        RasterWorkers.shared.start()
        var evicted: [RasterSource] = []
        lock.lock(); defer { lock.unlock(); retire(evicted) }
        guard !stopped, name.utf8.count <= (name.hasPrefix("data:") ? RasterInput.dataLimit : 4096) else { return .refused }
        pruneLocked()
        let key = RasterSourceKey(name: name, resolver: ObjectIdentifier(resolver))
        if let known = byName[key], let source = sources[known]?.value, source.resolver === resolver,
           !source.cancellation.isCancelled {
            // A retained private spool reuses its validated metadata and pixels,
            // even for no-store: HTML §4.8.4.3.1's list of available images
            // ignores HTTP cache freshness within a document.
            // https://html.spec.whatwg.org/multipage/images.html#list-of-available-images
            cold.removeAll { $0 === source }
            source.users += 1
            // Released before its header was read: a worker reads it now.
            if source.users == 1 && source.metadata == nil && source.failure == nil {
                exact_raster_session_control(self.id, 5)
            }
            return .source(source)
        }
        var live = sources.values.compactMap(\.value)
        // Cold metadata is a cache, never what keeps a shown image out at the
        // source cap: its identity goes first (a painted lease keeps pixels).
        if live.count >= sourceLimit {
            evicted = evictColdLocked(live.count - sourceLimit + 1)
            live = sources.values.compactMap(\.value)
        }
        guard live.count < sourceLimit,
              pendingMetadataLocked() < metadataLimit else {
            refusals += 1; return .busy
        }
        serial += 1
        let source = RasterSource(id: serial, name: name, resolver: resolver)
        source.users = 1; sources[serial] = WeakRasterSource(source); byName[key] = serial
        // A worker waiting on the gate wakes to read this source's metadata.
        exact_raster_session_control(id, 5)
        return .source(source)
    }
    func release(_ id: UInt64) {
        lock.lock()
        let source = sources[id]?.value
        var cancellation: RasterCancellation?
        if let source {
            source.users = max(0, source.users - 1)
            if source.users == 0 && source.inspecting { cancellation = source.cancellation }
            if source.users == 0 && source.metadata != nil && !cold.contains(where: { $0 === source }) { cold.append(source) }
        }
        // Detach strong cold owners and their cache identities under lock;
        // destroy them after unlocking. A painted lease keeps its pixels.
        let evicted = evictColdLocked(max(0, cold.count - RasterLoader.coldSources))
        pruneLocked()
        lock.unlock(); cancellation?.cancel()
        retire(evicted)
        withExtendedLifetime(source) {}
        // A freed source may admit an image waiting at a cap.
        wake()
    }
    private func pruneLocked() {
        sources = sources.filter { $0.value.value != nil }
        byName = byName.filter { sources[$0.value]?.value != nil }
    }
    private func pendingMetadataLocked() -> Int {
        sources.values.compactMap(\.value).filter { $0.metadata == nil && $0.failure == nil }.count
    }
    private func retire(_ sources: [RasterSource]) {
        for source in sources { exact_raster_session_retire_generation(id, source.id) }
    }
    private func evictColdLocked(_ count: Int, keepingPrivateSpools: Bool = false) -> [RasterSource] {
        guard count > 0 else { return [] }
        var evicted: [RasterSource] = [], retained: [RasterSource] = []
        for source in cold {
            if evicted.count < count && source.users == 0 &&
                (!keepingPrivateSpools || source.input?.rechecksMetadata != false) {
                sources.removeValue(forKey: source.id)
                if byName[source.key] == source.id { byName.removeValue(forKey: source.key) }
                evicted.append(source)
            } else { retained.append(source) }
        }
        cold = retained
        return evicted
    }
    func invalidate(_ name: String) {
        lock.lock(); byName = byName.filter { $0.key.name != name }; lock.unlock()
    }
    /// The scale an SVG's decode draws at, for the worker (`svgScales`).
    func noteScale(_ source: RasterSource, _ key: RasterDecodeKey, _ scale: Double) {
        lock.lock(); source.svgScales[key] = scale; lock.unlock()
    }
    func metadata(_ source: RasterSource) -> (RasterMetadata?, String?) {
        lock.lock(); defer { lock.unlock() }; return (source.metadata, source.failure)
    }
    func declined(_ source: RasterSource, _ key: RasterDecodeKey) -> Bool {
        lock.lock(); defer { lock.unlock() }; return source.declined.contains(key)
    }
    /// Why `source` failed, if it did, and the earliest it may be asked
    /// again if asking again may mend it: read together.
    func retry(_ source: RasterSource) -> (failure: String?, after: DispatchTime?) {
        lock.lock(); defer { lock.unlock() }; return (source.failure, source.failure == nil ? nil : source.transient)
    }
    /// Asks again for `source`, whose download failed in a way asking again
    /// may mend: a worker downloads it anew. Whether it was still failed
    /// (another view's retry may have asked already).
    func reset(_ source: RasterSource) -> Bool {
        lock.lock()
        guard source.failure != nil, source.transient != nil, !source.inspecting else { lock.unlock(); return false }
        // Within the metadata cap, as a first ask is (`acquire`): else it
        // waits, its attempt not spent (`askAgain`).
        let pending = pendingMetadataLocked()
        guard pending < metadataLimit else { lock.unlock(); return false }
        source.failure = nil; source.transient = nil; source.fetched = nil; source.metadata = nil; source.input = nil
        // Asked again as its views are now (`urge`), not as they were.
        source.urgent = false
        lock.unlock()
        exact_raster_session_control(id, 5); wake()
        return true
    }
    /// A view that fills the screen shows `source`: its download goes first,
    /// one under way too.
    func urge(_ source: RasterSource) {
        lock.lock(); let was = source.urgent; source.urgent = true; let running = source.inspecting; lock.unlock()
        if !was, running, let url = RasterInput.remote(source.name) { RasterInput.promote(url) }
    }
    func inspectOne() -> Bool {
        lock.lock()
        let source = stopped || paused ? nil : sources.values.compactMap(\.value).filter {
            $0.users > 0 && !$0.inspecting && $0.metadata == nil && $0.failure == nil
        }.min { $0.id < $1.id }
        source?.inspecting = true
        let cancellation = source?.cancellation
        let fetched = source?.fetched
        source?.fetched = nil
        lock.unlock()
        guard let source, let cancellation else { return false }
        // A remote source downloads on URLSession's queue, never on a worker,
        // and stays `inspecting` (so its release cancels it) until its body
        // is here: the workers go on reading headers and decoding meanwhile.
        if fetched == nil, let url = RasterInput.remote(source.name) {
            // Asked as the download starts, so a view that came to fill the screen meanwhile counts.
            let urgent = { [weak self, weak source] () -> Bool in
                guard let self, let source else { return false }
                self.lock.lock(); defer { self.lock.unlock() }; return source.urgent
            }
            RasterInput.fetch(url, urgent: urgent, cancellation: cancellation) { [weak self] result in self?.downloaded(source, cancellation, result) }
            return true
        }
        // A package's picture is asked of its implementation, on the main thread.
        if fetched == nil, let pictures = loader?.pictures, pictures.serving(source.name) != nil {
            DispatchQueue.main.async { [weak self] in
                guard let self, let loader = self.loader else { return }
                loader.askPicture(source.name) { [weak self] result in self?.downloaded(source, cancellation, result) }
            }
            return true
        }
        var metadata: RasterMetadata?, input: RasterInput?, failure: String?, transient: DispatchTime?
        do {
            guard let resolver = source.resolver else { throw RasterFailure.decode }
            input = try fetched?.get() ?? RasterInput.open(source.name, resolver: resolver, cancellation: cancellation)
            metadata = try input!.metadata()
        } catch {
            // A body that came with a status that is not 2xx and is not an image fails as that status.
            let error = input?.failure(error) ?? error
            // (A link the reason quotes loses its query: a CDN's signature.)
            failure = RasterRetry.redacted(error is RasterFailure || error is RasterHTTPStatus || error is RasterEmptyResponse
                ? String(describing: error) : error.localizedDescription, source: source.name)
            // A download's failure, or its status's: a header that fails to read fails again.
            if RasterInput.remote(source.name) != nil, input == nil || error is RasterHTTPStatus, RasterRetry.transient(error) {
                transient = (error as? RasterHTTPStatus)?.retryAfter ?? .now()
            }
        }
        lock.lock()
        if cancellation.isCancelled {
            source.cancellation = RasterCancellation()
        } else {
            source.metadata = metadata; source.input = input; source.failure = failure; source.transient = transient
        }
        source.inspecting = false
        metadataReads += 1
        if failure != nil { refusals += 1 }
        pruneLocked()
        lock.unlock()
        wake()
        return true
    }
    /// A remote source's body arrived: a worker reads its header next. One
    /// let go of meanwhile is asked for again from the start, and the room
    /// it held at the metadata cap may admit a waiting image.
    private func downloaded(_ source: RasterSource, _ cancellation: RasterCancellation, _ result: Result<RasterInput, Error>) {
        lock.lock()
        if cancellation.isCancelled { source.cancellation = RasterCancellation() } else { source.fetched = result }
        source.inspecting = false
        lock.unlock()
        exact_raster_session_control(id, 5)
        wake()
    }
    func decode(_ work: ExactRasterWork) {
        // Pixels are made on a worker only: no pass on main, a mount pass included, decodes.
        assert(!Thread.isMainThread, "a raster decode on the main thread")
        let charge = NativeRasterCharge(work.charge)
        lock.lock()
        let source = sources[work.source]?.value
        let metadata = source?.metadata
        let input = source?.input
        let stopped = self.stopped
        lock.unlock()
        guard !stopped, exact_raster_is_cancelled(work.permit) == 0,
              let source, let metadata, let input, let resolver = source.resolver else {
            exact_raster_fail(work.permit); return
        }
        let key = RasterDecodeKey(width: Int(work.width), height: Int(work.height), variant: work.variant)
        do {
            // Decode scope drops encoded data, source and staging before complete.
            let image: RasterImage = try autoreleasepool {
                // An SVG's plan is the work's own size (the box's, under `fill`), at the scale the request noted.
                var plan = try RasterDecodePlan(metadata: metadata, maxPixel: Int(max(work.width, work.height)), variant: work.variant,
                                                aspect: metadata.svg ? (Int(work.width), Int(work.height)) : nil)
                guard plan.width == work.width, plan.height == work.height else { throw RasterFailure.reservation }
                if metadata.svg { lock.lock(); plan.scale = source.svgScales[key] ?? 1; lock.unlock() }
                if let name = input.catalog, exact_raster_is_cancelled(work.permit) == 0,
                   let image = RasterImage.catalog(name, metadata: metadata, plan: plan, charge: charge, sourceOwner: source) {
                    return image
                }
                if let picture = input.decoded {
                    return try RasterImage.picture(picture, metadata: metadata, plan: plan, charge: charge, sourceOwner: source)
                }
                return try withExtendedLifetime(resolver) {
                    try input.withBytes { bytes, file in
                        guard exact_raster_is_cancelled(work.permit) == 0 else { throw RasterFailure.decode }
                        #if DEBUG
                        lock.lock(); let decline = testDeclines > 0; testDeclines = max(0, testDeclines - 1); lock.unlock()
                        if decline { throw RasterFailure.decode }
                        #endif
                        return try RasterImage.decode(bytes, metadata: metadata, plan: plan, charge: charge, sourceOwner: source,
                                                      file: file, recheckMetadata: input.rechecksMetadata)
                    }
                }
            }
            let owner = UInt64(UInt(bitPattern: Unmanaged.passRetained(image).toOpaque()))
            _ = exact_raster_complete(work.permit, owner, { value in
                if let pointer = UnsafeRawPointer(bitPattern: UInt(value)) {
                    Unmanaged<RasterImage>.fromOpaque(pointer).release()
                }
            }, UInt64(image.residentBytes))
            lock.lock(); decoded += 1; source.declined.remove(key); lock.unlock()
        } catch {
            // Set before the core's failure is visible to the pass that reads both.
            lock.lock(); refusals += 1
            if error as? RasterFailure == .decode { source.declined.insert(key) } else { source.declined.remove(key) }
            lock.unlock()
            exact_raster_fail(work.permit)
        }
        wake()
    }
    func wake() {
        lock.lock()
        guard !wakePending, !stopped else { lock.unlock(); return }
        wakePending = true; lock.unlock()
        // Only an ID/weak backend crosses to main; no image is captured here.
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            lock.lock(); wakePending = false; let loader = self.loader; lock.unlock()
            loader?.reconcile(.wake)
        }
    }
    func notifyBudgetChange() {
        let s = exact_raster_stats(id)
        let signature = [s.waiting_budget, s.resident_bytes + s.reserved_bytes]
        lock.lock(); pruneLocked()
        let changed = signature != budgetNotice
        budgetNotice = signature
        lock.unlock()
        if changed && s.waiting_budget > 0 { wake() }
    }
    func reset(stop: Bool) {
        lock.lock(); stopped = stop
        let owners = sources.values.compactMap(\.value)
        let cancellations = owners.map(\.cancellation)
        let evicted = cold; cold.removeAll()
        sources.removeAll(); byName.removeAll(); lock.unlock()
        cancellations.forEach { $0.cancel() }
        withExtendedLifetime(evicted) {}
        withExtendedLifetime(owners) {}
    }
    func setPaused(_ paused: Bool) {
        lock.lock(); self.paused = paused
        // Asked again on resume as its views are then (`urge`).
        if paused { sources.values.compactMap(\.value).forEach { $0.urgent = false } }
        let cancellations = paused ? sources.values.compactMap(\.value).filter(\.inspecting).map(\.cancellation) : []
        lock.unlock(); cancellations.forEach { $0.cancel() }
    }
    func trim(keeping bytes: UInt64 = 0) {
        lock.lock()
        // A mutable file needs a fresh identity after rest so a cache hit
        // cannot hide a replacement's pixels and natural size.
        let evicted = evictColdLocked(cold.count, keepingPrivateSpools: bytes > 0)
        // Another size may still be live: its file's unused sizes must also
        // go, so their next decode checks the file for replacement.
        let mutable = sources.values.compactMap(\.value).filter { $0.input?.rechecksMetadata == true }.map(\.id)
        lock.unlock()
        retire(evicted)
        for generation in mutable { exact_raster_session_trim_generation(id, generation) }
        exact_raster_session_trim(id, bytes)
        withExtendedLifetime(evicted) {}
        wake()
    }
}

private final class WeakRasterBackend {
    weak var value: RasterBackend?
    init(_ value: RasterBackend) { self.value = value }
}

/// Exactly two process workers do both bounded metadata inspection and pixel
/// decode. The gate's wait does not demand UI frames or retain Runtime objects.
/// While any list travels fast (`travelling`), only the first decodes: a
/// second decode in flight doubles ImageIO's buffers at the peak, and at
/// that speed most rows pass before either lands.
final class RasterWorkers: @unchecked Sendable {
    static let shared = RasterWorkers()
    private let lock = NSLock()
    private var backends: [UInt64: WeakRasterBackend] = [:]
    private var cursor: UInt64 = 0
    private var fast = Set<ObjectIdentifier>()
    /// Whether `owner` (a scroll pump) has a list travelling fast.
    func travelling(_ owner: AnyObject, _ value: Bool) {
        lock.lock()
        if value { fast.insert(ObjectIdentifier(owner)) } else { fast.remove(ObjectIdentifier(owner)) }
        lock.unlock()
    }
    var single: Bool { lock.lock(); defer { lock.unlock() }; return !fast.isEmpty }
    private var started = false
    /// The workers start with the process's first image source: an app that
    /// shows no picture keeps no decode threads.
    func start() {
        lock.lock(); let first = !started; started = true; lock.unlock()
        guard first else { return }
        for i in 0..<2 { Thread.detachNewThread { [self] in run(i) } }
    }
    func add(_ backend: RasterBackend) { lock.lock(); backends[backend.id] = WeakRasterBackend(backend); lock.unlock() }
    private func backend(_ id: UInt64) -> RasterBackend? {
        lock.lock(); defer { lock.unlock() }; return backends[id]?.value
    }
    private func metadataTurn() -> Bool {
        lock.lock()
        backends = backends.filter { $0.value.value != nil }
        let all = backends.keys.sorted()
        let order = all.filter { $0 > cursor } + all.filter { $0 <= cursor }
        let candidates = order.compactMap { backends[$0]?.value }
        if let first = candidates.first { cursor = first.id }
        lock.unlock()
        for item in candidates {
            item.notifyBudgetChange()
            if item.inspectOne() { return true }
        }
        return false
    }
    private func run(_ index: Int) {
        var metadataFirst = false
        // Whether this worker decoded or inspected since it last went idle.
        var worked = false
        while true {
            autoreleasepool {
                // The second worker only reads metadata while one decodes.
                if index > 0 && single {
                    if metadataTurn() { worked = true } else { Thread.sleep(forTimeInterval: 0.005) }
                    return
                }
                // Each decode earns a metadata turn, even under continuous
                // ready-metadata traffic from another session.
                if metadataFirst { metadataFirst = false; if metadataTurn() { worked = true; return } }
                let work = exact_raster_next_decode(0, 0)
                if work.permit != 0 { metadataFirst = true; worked = true; deliver(work); return }
                // Read before looking: a source acquired while this worker
                // looks wakes the gate after it, and the wait returns at once.
                let since = exact_raster_wake_sequence()
                if metadataTurn() { worked = true; return }
                // Going idle: the pages ImageIO's frames reached (up to 64 KB
                // in euxy's launch) are dead until the next decode.
                if worked { worked = false; releaseStackBelowCaller() }
                // Work wakes the gate: a decode admitted or freed, a source
                // acquired, a session resumed. The timeout is a backstop, long
                // enough not to be a poll, so an idle app's workers sleep.
                let waited = exact_raster_next_decode(1000, since)
                if waited.permit != 0 { metadataFirst = true; worked = true; deliver(waited) }
            }
        }
    }
    private func deliver(_ work: ExactRasterWork) {
        if let backend = backend(work.session) { backend.decode(work) }
        else { exact_raster_fail(work.permit); exact_raster_charge_release(work.charge) }
    }
}
