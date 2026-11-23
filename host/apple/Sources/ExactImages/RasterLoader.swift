import Foundation
import ExactKit
import CExact
import ExactCore
#if os(macOS)
import AppKit
#else
import UIKit
#endif

final class NativeRasterCharge: RasterBackingCharge, @unchecked Sendable {
    let id: UInt64
    init(_ id: UInt64) { self.id = id }
    deinit { exact_raster_charge_release(id) }
}

/// View ownership pins the core lease; the provider separately owns its charge.
final class NativeRasterLease: ImageLease {
    let id: UInt64
    let decoded: RasterImage
    var image: any ImageBitmap { decoded }
    init?(_ ready: ExactRasterReady) {
        guard ready.lease != 0, let pointer = UnsafeRawPointer(bitPattern: UInt(ready.payload)) else { return nil }
        id = ready.lease
        decoded = Unmanaged<RasterImage>.fromOpaque(pointer).takeUnretainedValue()
    }
    deinit { exact_raster_lease_release(id) }
}

/// Main-thread view interests. Replacement keeps the old lease/natural size
/// until this exact generation accepts a new lease; metadata alone never paints.
final class RasterLoader: ImageLoader {
    /// An image waiting at the source or metadata cap. Weak: it keeps no
    /// removed view, resolver generation or session alive while it waits.
    private final class DeferredInterest {
        weak var view: NodeView?
        weak var resolver: AssetResolver?
        let generation: Int
        let source: String
        init(view: NodeView, resolver: AssetResolver, source: String) {
            self.view = view; self.resolver = resolver; generation = view.loadGeneration; self.source = source
        }
    }
    private struct Interest {
        weak var view: NodeView?
        let generation: Int
        let source: RasterSource
        var request: UInt64 = 0
        var variant = RasterVariant.own8
        var requestedPixel = 0
        /// The decode its request asked for.
        var decode: RasterDecodeKey?
        var offeredPixel = 0
        var failure: String?
        var delivered = false
        /// Its `load` or `error` went out: once per source, as `<img>`'s.
        var announced = false
        /// The core's request queue was full: asked again on the next pass.
        var admissionDeferred = false
        /// Declined decodes since it last painted, and the delay it waits out
        /// before asking again: its token, which a reset or a new wait retires.
        var declines = 0
        var waiting: Int?
        /// Off screen, its picture let go of or its decode not asked for
        /// (`park`): asked for again once its view is on screen.
        var parked = false
        /// Its download failed in a way asking again may mend, its `error`
        /// held while it is asked again on screen (`hold`): the retries this
        /// showing, the wait's token, and whether it waits off screen.
        var held = false
        var retries = 0
        var retryWaiting: Int?
        var offscreen = false
        /// A retry's download is under way: still watched, so leaving the
        /// screen meanwhile starts the count over.
        var retrying = false
        /// A first download whose box could fill the screen, off screen:
        /// watched, so a scroll that brings it on screen hurries it (`urge`).
        var watching = false
        /// An SVG under `object-fit: fill`: the box's pixel size its plan
        /// took, a different picture once either axis moves (LLP 1011.001 D1).
        var aspect: [Int] = []
    }
    /// How long a view waits after each declined decode before asking again;
    /// past the last, the decline is its `error`, as a page's `<img>` errs
    /// for a file and never for a busy machine. ImageIO says only that a
    /// decode failed; a JPEG or PNG it cannot decode fails its header read
    /// first (macOS 27), so a file that fails here anyway errs 1.75 s late,
    /// inside the agent's 3 s bound.
    static let declineDelays: [Double] = [0.25, 0.5, 1]
    /// Eight viewport-sized RGBA bitmaps, bounded to 32–192 MiB. Start at
    /// the floor until the owning view has geometry, never a process screen.
    static let minimumBudget: UInt64 = 32 * 1024 * 1024
    static func viewportBudget(size: CGSize, scale: CGFloat) -> UInt64 {
        let pixels = size.width * size.height * scale * scale
        guard pixels.isFinite, pixels >= 0 else { return minimumBudget }
        return UInt64(min(192 * 1024 * 1024, max(CGFloat(minimumBudget), pixels * 4 * 8)))
    }
    /// Sources no view shows whose metadata stays for the cache's keys.
    static let coldSources = 256
    private(set) var budget: UInt64
    let id: UInt64
    private let backend: RasterBackend
    /// Package pictures admitted and revoked (PackagePictures.swift).
    let pictures = PackagePictures()
    private var interests: [UInt32: Interest] = [:]
    private var deferred: [UInt32: DeferredInterest] = [:]
    /// The source each view's last refusal before a fetch named: its `error`
    /// goes out once, though every props op asks for that source again.
    private var refused: [UInt32: String] = [:]
    /// Package pictures refused admission, by view: asked again after each
    /// commit, since an implementation bound or a permission granted since
    /// may let one in (LLP 1133.019 D2).
    private var refusedPictures: [UInt32: (view: () -> NodeView?, source: String, resolver: AssetResolver)] = [:]
    private var deferredCursor: UInt32 = 0
    /// The views parked off screen (`park`), for the scroll check.
    private var parked = Set<UInt32>()
    #if os(macOS)
    private var scrollObserver: NSObjectProtocol?
    #else
    private final class ScrollWatch {
        weak var scroller: UIScrollView?
        let observation: NSKeyValueObservation
        init(_ scroller: UIScrollView, _ observation: NSKeyValueObservation) { self.scroller = scroller; self.observation = observation }
    }
    private var scrollWatches: [ObjectIdentifier: ScrollWatch] = [:]
    #endif
    private var paused = false
    private var destroyed = false
    private var pressure: DispatchSourceMemoryPressure?
    #if DEBUG
    private(set) var testReconciliations = 0
    /// Each `load` (nil) or `error` (its reason) a pass announced, by view.
    private(set) var testAnnounced: [(view: UInt32, failure: String?)] = []
    /// Stands in for the core's request: a request id, or 0 and its refusal.
    var testRequest: ((ExactRasterDemand) -> (request: UInt64, refusal: UInt64?))?
    /// The next `count` decodes fail as ImageIO's do on a busy system.
    func testDecline(next count: Int) { backend.lock.lock(); backend.testDeclines = count; backend.lock.unlock() }
    /// Whether the core holds the picture a view asked for ready, not yet taken.
    func testReady(_ view: UInt32) -> Bool {
        guard let request = interests[view]?.request, request != 0 else { return false }
        return exact_raster_status(id, request) == 4
    }
    #endif
    init(budget: UInt64 = RasterLoader.minimumBudget,
         sourceLimit: Int = 1152, metadataLimit: Int = 64) {
        self.budget = max(Self.minimumBudget, budget)
        id = exact_raster_session_create(self.budget)
        backend = RasterBackend(id: id, sourceLimit: sourceLimit, metadataLimit: metadataLimit); backend.loader = self
        RasterWorkers.shared.add(backend)
        let pressure = DispatchSource.makeMemoryPressureSource(eventMask: [.warning, .critical], queue: .global(qos: .utility))
        pressure.setEventHandler { [weak backend] in backend?.trim() }
        pressure.resume(); self.pressure = pressure
    }
    deinit { shutdown() }
    func fit(size: CGSize, scale: CGFloat) {
        let next = Self.viewportBudget(size: size, scale: scale)
        guard !destroyed, next != budget else { return }
        budget = next
        exact_raster_session_budget(id, next)
    }
    /// Reconsider decode resolution even when the boxes kept their point
    /// sizes, and HDR against the display (LLP 1100 D9).
    func displayChanged() {
        for interest in Array(interests.values) {
            if let view = interest.view { resized(view); rangeChanged(view) }
        }
    }
    /// Re-plan an HDR picture whose decode no longer matches its node's
    /// limit or display (LLP 1100 D8, D9).
    func rangeChanged(_ view: NodeView) {
        guard let interest = interests[view.id], interest.request != 0 || interest.delivered,
              let metadata = backend.metadata(interest.source).0, metadata.hdr else { return }
        if Self.wantsHDR(view, metadata) != (interest.variant == RasterVariant.hdr) { retry(view.id) }
    }
    @discardableResult func load(_ view: NodeView, source: String, resolver: AssetResolver) -> Bool {
        func refuse(_ reason: String) {
            if refused[view.id] != source { refused[view.id] = source; Self.announce([(view, nil, reason)]) }
        }
        if source.hasPrefix("data:"), source.utf8.count > RasterInput.dataLimit {
            view.presenter?.session?.log("image refused: a data: source is over \(RasterInput.dataLimit) bytes (LLP 1011 §2)")
            refuse("a data: source over \(RasterInput.dataLimit) bytes"); return false
        }
        refusedPictures[view.id] = nil
        if let session = view.presenter?.session, let refusal = pictures.admit(source, session: session) {
            view.presenter?.session?.log("image refused: \(sourceWithoutQuery(source)): \(refusal) (LLP 1133.019 D2)")
            refusedPictures[view.id] = ({ [weak view] in view }, source, resolver)
            refuse(refusal); return false
        }
        let retained = interests[view.id] != nil || deferred[view.id] != nil
        guard !destroyed, retained || interests.count + deferred.count < 1024 else {
            view.presenter?.session?.log("image deferred: raster subscriber limit")
            refuse("raster subscriber limit"); return false
        }
        switch backend.acquire(source, resolver: resolver) {
        case .source(let record):
            cancel(view.id)
            interests[view.id] = Interest(view: view, generation: view.loadGeneration, source: record)
            timings.asked(view.id); timings.admitted(view.id)
        case .busy:
            // The caps free as inspections land and views go; each wakes a
            // pass that admits it (`admitDeferred`), with no prop change.
            cancel(view.id)
            deferred[view.id] = DeferredInterest(view: view, resolver: resolver, source: source)
            timings.asked(view.id)
            view.presenter?.session?.log("image queued: raster metadata/source admission")
        case .refused:
            view.presenter?.session?.log("image deferred: invalid raster source or stopped loader")
            refuse("invalid raster source"); return false
        }
        refused.removeValue(forKey: view.id)
        // Initial props run before Presenter registers the new view. Reconcile
        // after the batch, when identity checks can distinguish it from removal:
        // at its end, before the transaction commits, so a picture the core
        // already holds shows with its row (`mountPass`); and on the next turn,
        // as before, for one that must still be read or decoded.
        if !paused {
            if let presenter = view.presenter, presenter.applying {
                if mounting.isEmpty { presenter.afterBatch { [weak self] in self?.mountPass() } }
                mounting.insert(view.id)
            }
            backend.wake()
        }
        return true
    }
    /// After a batch that mounted images: a reconcile of the views it loaded, whose
    /// pictures go on their layers in the batch's transaction, the row and its
    /// photo in one frame.
    /// It only takes what the core holds ready: a request it makes is decoded
    /// on a worker as any other, nothing here waits, and the sizes and events
    /// it lands go out on the next main turn, as the asynchronous pass's do
    /// (LLP 1011, "Events").
    func mountPass() {
        reconcile(.mount)
    }
    /// At the end of a boot's apply: the pictures already decoded go on their
    /// layers in the boot's own transaction, its first frame, instead of on the
    /// wake that would bring them a turn later. It takes only what the core
    /// holds ready, so the first frame never waits for a decode; it asks for
    /// nothing, and what it lands is reported on the next turn (LLP 1011).
    func bootPass() {
        reconcile(.boot)
    }
    /// The views a batch under way asked to load: its mount pass's.
    private var mounting = Set<UInt32>()
    func cancel(_ view: UInt32) {
        timings.retired(view)
        deferred.removeValue(forKey: view)
        refusedPictures.removeValue(forKey: view)
        refused.removeValue(forKey: view)
        unpark(view)
        guard let interest = interests.removeValue(forKey: view) else { return }
        if interest.request != 0 { exact_raster_cancel(id, interest.request) }
        backend.release(interest.source.id)
    }
    func invalidate(_ source: String) { backend.invalidate(source) }
    var hasRefusedPictures: Bool { !refusedPictures.isEmpty }
    /// Each refused package picture whose admission now passes loads. Main
    /// thread, after a commit applied.
    func readmitPictures() {
        for (id, refusal) in refusedPictures {
            guard let view = refusal.view(), let session = view.presenter?.session else { refusedPictures[id] = nil; continue }
            guard pictures.admit(refusal.source, session: session, count: false) == nil else { continue }
            refused[id] = nil
            load(view, source: refusal.source, resolver: refusal.resolver)
        }
    }
    /// Ask the implementation for package picture `name` at the size of a
    /// box that shows it. Main thread.
    func askPicture(_ name: String, done: @escaping (Result<RasterInput, Error>) -> Void) {
        let view = interests.values.first { $0.source.name == name && $0.view != nil }?.view
        guard let view, let session = view.presenter?.session else { return done(.failure(RasterFailure.unresolved)) }
        let scale = Self.displayScale(view)
        // Before layout, the platform's own thumbnail size.
        let box = view.bounds.size == .zero ? CGSize(width: 512, height: 512) : view.bounds.size
        pictures.ask(name, session: session, width: max(1, Int((box.width * scale).rounded(.up))),
                     height: max(1, Int((box.height * scale).rounded(.up))),
                     fit: view.style["object_fit"]?.string ?? "fill", done: done)
    }
    /// The implementation took package picture `source` back (LLP 1133.019
    /// D2): every view showing it is emptied and fires `error`, its loads
    /// and cache entries go, and a reply in flight is dropped.
    func revokePicture(_ source: String) {
        pictures.revoke(source)
        backend.invalidate(source)
        var gone: [(view: NodeView, generation: Int?, failure: String?)] = []
        for (id, interest) in interests where interest.source.name == source {
            if let view = interest.view { view.withdrawPicture(); gone.append((view, nil, "revoked")) }
            cancel(id)
            refused[id] = source
        }
        Self.announce(gone)
    }
    /// The implementation says package picture `source` changed: each view
    /// showing it loads it again.
    func pictureChanged(_ source: String) {
        backend.invalidate(source)
        for (id, interest) in interests where interest.source.name == source {
            guard let view = interest.view, let resolver = interest.source.resolver else { continue }
            cancel(id)
            load(view, source: source, resolver: resolver)
        }
    }
    /// When each image arrived, and which arrived late (LLP 1145 D2).
    let timings = RasterTimings()
    /// A presented frame (the frame sampler's): each loading image whose box
    /// shows starts its blank time.
    func sampleBlank() {
        guard HatchDiagnosticsLink.available else { return }
        let loading = interests.filter { !$0.value.delivered && ($0.value.failure == nil || $0.value.held) }
        guard !loading.isEmpty || !deferred.isEmpty else { return }
        // A decode that finished waits here for the main thread to deliver it:
        // its `decoded` is the first frame that saw it ready.
        for (key, i) in loading where i.request != 0 && exact_raster_status(id, i.request) == 4 { timings.decoded(key) }
        timings.showing(loading.compactMap { key, i in i.view.map(Self.mayShow) == true ? key : nil }
            + deferred.compactMap { key, d in d.view.map(Self.mayShow) == true ? key : nil })
    }
    /// Images on screen still loading (the agent's `clock` and `screenshot`
    /// wait for them).
    var loadingOnScreen: Int {
        // (One held for a retry is still loading.)
        interests.values.filter { i in !i.delivered && (i.failure == nil || i.held) && i.view.map(Self.mayShow) == true }.count
            + deferred.values.filter { $0.view.map(Self.mayShow) == true }.count
    }
    /// A fragment follows its route's pictures until their first load or error,
    /// including ones above the port. Resolution changes and parking keep that outcome.
    /// A picture waiting to be on screen (parked, or a retry held off screen)
    /// asks for nothing meanwhile, so it is not loading.
    func loading(inside root: NodeView) -> Bool {
        interests.values.contains { item in
            guard let view = item.view, view.loadGeneration == item.generation else { return false }
            return !item.announced && !item.parked && !(item.held && item.offscreen) && view.isDescendant(of: root)
        } || deferred.values.contains { item in
            guard let view = item.view, view.loadGeneration == item.generation else { return false }
            return view.isDescendant(of: root)
        }
    }
    /// A loading image's box is on screen. An image sized on one axis
    /// (`width=96`, no height) has an empty box until its natural size
    /// lands, so an empty side counts as one point.
    private static func mayShow(_ view: NodeView) -> Bool {
        guard view.window != nil else { return false }
        #if !os(macOS)
        if view.drawnByBar { return true }
        #endif
        let probe = CGRect(origin: view.bounds.origin, size: CGSize(width: max(1, view.bounds.width), height: max(1, view.bounds.height)))
        var root: MediaPlatformView = view
        while let parent = root.superview {
            if root.isHidden { return false }
            root = parent
        }
        var clipped = view.convert(probe, to: root).intersection(root.bounds)
        var ancestor = view.superview
        while let current = ancestor, !clipped.isEmpty {
            #if os(macOS)
            let clips = current is NSClipView || current.clipsToBounds || current.layer?.masksToBounds == true
            #else
            let clips = current.clipsToBounds
            #endif
            if clips { clipped = clipped.intersection(current.convert(current.bounds, to: root)) }
            ancestor = current.superview
        }
        return !clipped.isEmpty
    }
    /// Rest keeps cold pictures within three eighths of the eight-viewport
    /// budget and drops cold mutable-file decodes. Memory pressure drops all cold metadata.
    func trimCold(keeping bytes: UInt64? = nil) { backend.trim(keeping: bytes ?? budget * 3 / 8) }
    func resized(_ view: NodeView) {
        guard let interest = interests[view.id] else { return }
        // Laid out to fill the screen while it loads (a viewer's picture): its download goes first.
        if !interest.delivered, Self.fillsScreen(view) { backend.urge(interest.source) }
        // A held picture laid out on or off screen: a pass sees it (`hold`).
        if interest.held || interest.retrying { backend.wake(); return }
        // A first download laid out large enough to fill the screen: a pass
        // watches it off screen, or hurries it on (`watching`).
        if !interest.delivered, !interest.watching, interest.offeredPixel == 0, Self.fillsScreen(view, anywhere: true) { backend.wake() }
        // Laid out on screen, a parked view asks for its picture.
        if interest.parked { if Self.mayShow(view) { backend.wake() }; return }
        guard interest.offeredPixel > 0 else { return }
        let metadata = backend.metadata(interest.source).0
        let pixel = requestedPixel(view, metadata)
        if let rendering = view.style["image_rendering"]?.string, rendering != "auto", pixel != interest.offeredPixel { retry(view.id); return }
        // Avoid re-decoding for every one-pixel live resize or small rounding.
        if pixel > interest.offeredPixel * 5 / 4 || pixel * 5 / 4 < interest.offeredPixel { retry(view.id); return }
        // An SVG filling its box is drawn at the box's ratio: either axis past the band is another picture.
        if let aspect = metadata.flatMap({ Self.svgAspect(view, $0) }), interest.aspect.count == 2 {
            let moved = zip([aspect.0, aspect.1], interest.aspect).contains { now, was in now > was * 5 / 4 || now * 5 / 4 < was }
            if moved { retry(view.id) }
        }
    }
    /// The view's device pixels per point.
    private static func displayScale(_ view: NodeView) -> CGFloat {
        #if os(macOS)
        return view.pixelScale ?? 1
        #else
        return view.traitCollection.displayScale
        #endif
    }
    /// For an SVG under `object-fit: fill` (CSS's initial value), the box's
    /// pixel size, which is the concrete object size the picture is drawn at
    /// (LLP 1011.001 D1); nil for a bitmap or another fit.
    private static func svgAspect(_ view: NodeView, _ metadata: RasterMetadata) -> (Int, Int)? {
        #if os(iOS) || os(tvOS)
        let box = view.buttonRasterView == nil ? view.bounds.size : metadata.naturalSize
        #else
        let box = view.bounds.size
        #endif
        guard metadata.svg, (view.style["object_fit"]?.string ?? "fill") == "fill", box.width > 0, box.height > 0 else { return nil }
        let scale = displayScale(view)
        return (max(1, Int((box.width * scale).rounded())), max(1, Int((box.height * scale).rounded())))
    }
    /// The decoded image's longest side: enough device pixels for the box
    /// at its object-fit (`cover` and `fill` scale the image until both axes
    /// fill the box, so the longer one overflows it), as a browser decodes
    /// for the size it paints. Before the metadata, the box's longest side.
    private func requestedPixel(_ view: NodeView, _ metadata: RasterMetadata?) -> Int {
        if let rendering = view.style["image_rendering"]?.string, rendering != "auto", let metadata, !metadata.svg {
            return max(metadata.sourceWidth, metadata.sourceHeight)
        }
        #if os(macOS)
        let scale = view.pixelScale ?? 1
        #else
        let scale = view.traitCollection.displayScale
        #endif
        #if os(iOS) || os(tvOS)
        // A native face displays the image at its natural dimensions, even
        // before metadata has enlarged the button's image-free layout box.
        if view.buttonRasterView != nil, let natural = metadata?.naturalSize {
            return max(64, min(4096, Int(ceil(max(natural.width, natural.height) * scale))))
        }
        #endif
        let box = view.bounds.size
        var longest = max(box.width, box.height)
        if let natural = metadata?.naturalSize, natural.width > 0, natural.height > 0, box.width > 0, box.height > 0 {
            let x = box.width / natural.width, y = box.height / natural.height
            let factor: CGFloat
            switch view.style["object_fit"]?.string ?? "fill" {
            case "contain": factor = min(x, y)
            case "scale-down": factor = min(1, min(x, y))
            case "none": factor = 1
            default: factor = max(x, y)
            }
            longest = max(natural.width, natural.height) * factor
        }
        return max(64, min(4096, Int(ceil(longest * scale))))
    }
    private func retry(_ view: UInt32) {
        guard var item = interests[view] else { return }
        if item.request != 0 { exact_raster_cancel(id, item.request) }
        item.request = 0; item.requestedPixel = 0; item.failure = nil; item.delivered = false
        item.admissionDeferred = false; item.waiting = nil
        if item.held || item.retrying || item.watching { unwatch(view, &item) }
        interests[view] = item
        reconcile()
    }
    /// Ask again for a decode ImageIO declined, unless the wait was retired
    /// (a reset, a new source or view under the same id, another wait).
    private func decodeAgain(_ view: UInt32, token: Int) {
        guard var item = interests[view], item.waiting == token else { return }
        item.waiting = nil; interests[view] = item
        reconcile()
    }
    /// A picture whose download failed in a way asking again may mend, its
    /// `error` held (LLP 1011 §4, the same rules as the Linux host's): on
    /// screen it is asked again after the next of `RasterRetry.steps`, never
    /// before its server's `Retry-After`; off screen it waits, its count
    /// from nothing, until it shows. Whether it is still held; false once
    /// its retries are spent, or its server's wait is past what a picture
    /// waits: its `error` goes out.
    private func hold(_ viewID: UInt32, _ item: inout Interest, _ view: NodeView) -> Bool {
        let (failure, after) = backend.retry(item.source)
        guard let failure else {
            // Another view of the URL asked again: this one joins that
            // download, with no retry of its own counted.
            item.held = false; item.failure = nil; item.retryWaiting = nil; item.offscreen = false; item.retrying = true
            interests[viewID] = item; backend.wake()
            return true
        }
        item.failure = failure
        guard let after else { return false }
        // Watched, so leaving the screen or coming back is seen (`scrolled`).
        if !parked.contains(viewID) { watch(viewID, view) }
        defer { interests[viewID] = item }
        guard Self.mayShow(view) else {
            item.retryWaiting = nil; item.retries = 0; item.offscreen = true
            return true
        }
        item.offscreen = false
        if item.retryWaiting != nil { return true }
        let now = DispatchTime.now()
        let wait = after > now ? Double(after.uptimeNanoseconds - now.uptimeNanoseconds) / 1e9 : 0
        guard item.retries < RasterRetry.steps.count else { return false }
        // Further off than a picture waits: no more this showing.
        guard wait <= RasterRetry.most else { return false }
        let delay = max(RasterRetry.steps[item.retries], wait)
        nextWait += 1; item.retryWaiting = nextWait
        let token = nextWait
        view.presenter?.session?.log("image queued: \(failure); asked again in \(Int(delay * 1000)) ms")
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in self?.askAgain(viewID, token: token) }
        return true
    }
    /// A held picture's wait is out: asked again if it is still on screen
    /// (the next pass sees it left otherwise), unless the wait was retired.
    private func askAgain(_ viewID: UInt32, token: Int) {
        guard var item = interests[viewID], item.held, item.retryWaiting == token, let view = item.view else { return }
        item.retryWaiting = nil
        if Self.mayShow(view), backend.reset(item.source) {
            item.retries += 1; item.held = false; item.failure = nil; item.retrying = true
        }
        interests[viewID] = item
        reconcile()
    }
    /// Its retry tracking or its watch for the screen ends: the watch goes,
    /// unless it is parked for real (`park`).
    private func unwatch(_ viewID: UInt32, _ item: inout Interest) {
        item.held = false; item.retryWaiting = nil; item.retrying = false; item.watching = false
        if !item.parked { unpark(viewID) }
    }
    private func watch(_ viewID: UInt32, _ view: NodeView) {
        #if os(macOS)
        if parked.isEmpty { watchScrolls() }
        #else
        watchScrolls(above: view)
        #endif
        parked.insert(viewID)
    }
    /// A box on screen that fills it (`RasterRetry.fills`); `anywhere`, one
    /// that would if it were on screen.
    private static func fillsScreen(_ view: NodeView, anywhere: Bool = false) -> Bool {
        #if os(macOS)
        guard let root = view.window?.contentView else { return false }
        #else
        guard let root = view.window else { return false }
        #endif
        return (anywhere || mayShow(view)) && RasterRetry.fills(view.convert(view.bounds, to: root), window: root.bounds)
    }
    private var nextWait = 0
    /// Who asks for a pass. The wake (`RasterBackend.wake`) runs one on its own
    /// main turn; a mount pass runs in a batch's epilogue, over the views that
    /// batch loaded; a direct pass (a retry, a resize, a display change) may run
    /// inside a batch or a layout. The wake and mount passes take a cache hit as
    /// they ask for it; a direct pass leaves it to the wake, as before. A boot
    /// pass (`bootPass`) only takes what is ready.
    enum Pass { case wake, mount, direct, boot }
    func reconcile(_ pass: Pass = .direct) {
        #if DEBUG
        testReconciliations += 1
        #endif
        // A mount pass takes its batch's views even when it then does nothing:
        // left behind, they would keep every later batch from asking for one. A
        // wake runs between batches, never inside one, so views still listed then
        // are a pass that never came (an epilogue not run), and it lets them go.
        let batch = pass == .mount ? mounting : []
        if pass == .wake || pass == .mount { mounting.removeAll() }
        guard !destroyed, !paused else { return }
        // Decoded pictures put on their layers: a late frame's `image` (LLP 1144 D2).
        let began = HostPhases.shared.begin(.image)
        defer { HostPhases.shared.end(began) }
        if pass != .boot { admitDeferred() }
        // Every image this turn lands reports its natural size together,
        // after the loop: one layout for them all, not one each.
        var landed: [(view: NodeView, generation: Int, size: CGSize)] = []
        var settled: [(view: NodeView, generation: Int?, failure: String?)] = []
        // Whether this pass gave the core's queue room: a picture taken, a
        // request let go of.
        var progress = false
        defer {
            // A mount pass runs inside the batch's epilogue, a boot pass at the
            // boot's end: what it landed is reported, and its `load`s sent, on
            // the next turn.
            if pass == .mount || pass == .boot {
                if !landed.isEmpty || !settled.isEmpty {
                    DispatchQueue.main.async { [weak self, landed, settled] in self?.publish(landed, settled) }
                }
            } else { publish(landed, settled) }
            // The interests a full queue refused ask again on the next pass.
            // A decode landing wakes one, but the last may already have
            // landed, so a pass that made room wakes one itself.
            if progress && interests.values.contains(where: \.admissionDeferred) { backend.wake() }
        }
        // Each source's first outcome, its `load` or its `error`.
        func settle(_ item: inout Interest, _ view: NodeView) {
            // Its `load` or `error`: retry tracking ends.
            if item.retrying || item.held || item.watching { unwatch(view.id, &item) }
            if !item.announced { item.announced = true; settled.append((view, item.generation, item.failure)) }
        }
        // The picture the core holds ready for the item's request, on its view:
        // false when there is none to take, the request not ready.
        func take(_ viewID: UInt32, _ item: inout Interest, _ view: NodeView) -> Bool {
            guard let lease = NativeRasterLease(exact_raster_take_ready(id, item.request)) else { return false }
            item.delivered = true; item.declines = 0
            if item.retrying || item.watching { unwatch(viewID, &item) }
            timings.decoded(viewID) // if no frame saw it ready first (`sampleBlank`)
            if let size = view.acceptRaster(lease, generation: item.generation) { landed.append((view, item.generation, size)); settle(&item, view) }
            timings.delivered(viewID)
            interests[viewID] = item
            return true
        }
        let ids = pass == .mount ? batch.filter { interests[$0] != nil } : Array(interests.keys)
        for viewID in ids {
            guard var item = interests[viewID], let view = item.view,
                  view.loadGeneration == item.generation, view.presenter?.views[viewID] === view else { cancel(viewID); continue }
            // A boot pass takes a ready picture and leaves everything else as it is.
            if pass == .boot {
                if !item.delivered, item.failure == nil, item.request != 0,
                   exact_raster_status(id, item.request) == 4, take(viewID, &item, view) { progress = true }
                continue
            }
            if item.parked {
                guard Self.mayShow(view) else { continue }
                item.parked = false; interests[viewID] = item; unpark(viewID)
            }
            if item.held {
                if hold(viewID, &item, view) { continue }
                item.held = false; unpark(viewID); settle(&item, view); interests[viewID] = item
                view.presenter?.session?.log("image deferred: \(item.failure ?? "")"); continue
            }
            // A retry under way whose picture left the screen: the count starts over.
            if item.retrying, !Self.mayShow(view) { item.retries = 0; interests[viewID] = item }
            if item.failure != nil || item.delivered || item.waiting != nil { continue }
            if Self.fillsScreen(view) { backend.urge(item.source) }
            // A first download that could fill the screen, off screen: a scroll may bring it on.
            else if !item.watching, item.request == 0, !Self.mayShow(view), Self.fillsScreen(view, anywhere: true),
                    RasterInput.remote(item.source.name) != nil, backend.metadata(item.source).0 == nil,
                    backend.metadata(item.source).1 == nil {
                item.watching = true; watch(viewID, view); interests[viewID] = item
            }
            if item.request != 0 {
                let state = exact_raster_status(id, item.request)
                if state == 4, take(viewID, &item, view) {
                    progress = true
                } else if state == 115, item.declines < Self.declineDelays.count,
                          let decode = item.decode, backend.declined(item.source, decode) {
                    // Cancelled now, so the core forgets the failed decode once
                    // every view of it has let go, and the next request decodes.
                    let delay = Self.declineDelays[item.declines]
                    exact_raster_cancel(id, item.request); item.request = 0; item.requestedPixel = 0; progress = true
                    nextWait += 1
                    item.declines += 1; item.waiting = nextWait; interests[viewID] = item
                    view.presenter?.session?.log("image queued: ImageIO declined a decode; again in \(Int(delay * 1000)) ms")
                    let token = nextWait
                    DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
                        self?.decodeAgain(viewID, token: token)
                    }
                    continue
                } else if state >= 100 {
                    item.failure = Self.refusal(UInt64(state - 100)); settle(&item, view)
                    // A failed decode is let go of, its error kept: held, it
                    // is the core's answer to the next view that asks for it.
                    if state == 115 { exact_raster_cancel(id, item.request); item.request = 0; progress = true }
                    interests[viewID] = item
                    view.presenter?.session?.log("image deferred: \(item.failure!)")
                }
                if state != 2 { continue }
                // Waiting on the budget. The decodes in flight hold
                // reservations `available` does not count: once they land
                // it may fit, at the size it asked for.
                let plan = backend.metadata(item.source).0.flatMap {
                    try? RasterDecodePlan(metadata: $0, maxPixel: item.requestedPixel, variant: item.variant, aspect: Self.svgAspect(view, $0))
                }
                if let plan, plan.peakBytes <= available() { continue }
                // The pictures views hold leave it no room. Off screen it
                // holds no place in the queue and takes no lower resolution:
                // it asks again on screen. On screen, the pictures held off
                // screen make room, and the core retries it as they go.
                if !Self.mayShow(view) { park(viewID, &item, view); progress = true; continue }
                if let plan, release(plan.peakBytes - available()) > 0 { progress = true; continue }
                if item.requestedPixel <= 1 { continue }
                // Nothing off screen to let go of: re-plan against the room
                // left. A worker's changed-budget notice, never a layout
                // callback, brought it here.
                exact_raster_cancel(id, item.request); item.request = 0; progress = true
                interests[viewID] = item
            }
            let (metadata, failure) = backend.metadata(item.source)
            if let failure {
                item.failure = failure
                // Asking again may mend it: its `error` waits (`hold`).
                if !item.announced, backend.retry(item.source).after != nil {
                    item.held = true; item.retrying = false
                    if hold(viewID, &item, view) { continue }
                    item.held = false
                }
                if item.retrying { item.retrying = false }
                unpark(viewID)
                settle(&item, view); interests[viewID] = item
                view.presenter?.session?.log("image deferred: \(failure)"); continue
            }
            guard let metadata else { continue }
            if item.watching { unwatch(viewID, &item); interests[viewID] = item }
            let offered = requestedPixel(view, metadata)
            var pixel = item.requestedPixel > 0 ? min(offered, item.requestedPixel) : offered
            var available = available()
            let hdr = Self.wantsHDR(view, metadata)
            let aspect = Self.svgAspect(view, metadata)
            var plan = Self.fit(metadata, pixel: pixel, available: available, hdr: hdr, aspect: aspect)
            if let need = plan?.peakBytes, need > available {
                // The pictures views hold leave it no room at its size: off
                // screen it waits for its turn on screen; on screen the
                // pictures held off screen make room, and it is planned again.
                guard Self.mayShow(view) else { park(viewID, &item, view); continue }
                if release(need - available) > 0 {
                    available = self.available(); plan = Self.fit(metadata, pixel: pixel, available: available, hdr: hdr, aspect: aspect)
                }
            }
            if plan == nil || plan!.peakBytes > available {
                // Aligned rows make cost a staircase: find the last side that
                // fits, preserving the colour ladder at each candidate size.
                var low = 1, high = pixel
                while low < high {
                    let middle = low + (high - low + 1) / 2
                    if let candidate = Self.fit(metadata, pixel: middle, available: available, hdr: hdr, aspect: aspect),
                       candidate.peakBytes <= available { low = middle } else { high = middle - 1 }
                }
                pixel = low; plan = Self.fit(metadata, pixel: pixel, available: available, hdr: hdr, aspect: aspect)
            }
            guard var plan else { item.failure = "decode plan too large"; settle(&item, view); interests[viewID] = item; continue }
            if pixel < offered, item.offeredPixel != offered || item.requestedPixel != pixel {
                view.presenter?.session?.log("image resolution reduced for budget: \(offered) to \(pixel) pixels on the longest side")
            }
            if metadata.svg { plan.scale = Double(Self.displayScale(view)) }
            item.aspect = aspect.map { [$0.0, $0.1] } ?? []
            var demand = ExactRasterDemand()
            demand.view = UInt64(viewID); demand.view_generation = UInt64(item.generation)
            demand.source = item.source.id; demand.generation = item.source.id
            demand.width = UInt32(plan.width); demand.height = UInt32(plan.height)
            demand.priority = Self.mayShow(view) ? 0 : 1
            demand.natural_width = UInt32(metadata.naturalSize.width); demand.natural_height = UInt32(metadata.naturalSize.height)
            demand.encoded_bytes = UInt64(metadata.encodedBytes); demand.header_bytes = UInt64(metadata.headerBytes)
            demand.stride = UInt64(plan.stride); demand.scratch_bytes = UInt64(plan.scratchBytes)
            demand.variant = plan.variant
            #if DEBUG
            let submitted = testRequest?(demand)
            item.request = submitted?.request ?? exact_raster_request(id, demand)
            #else
            item.request = exact_raster_request(id, demand)
            #endif
            if item.request != 0 { timings.requested(viewID) }
            item.requestedPixel = pixel; item.offeredPixel = offered
            item.variant = plan.variant
            item.decode = RasterDecodeKey(width: plan.width, height: plan.height, variant: plan.variant)
            if metadata.svg, let key = item.decode { backend.noteScale(item.source, key, plan.scale) }
            if item.request == 0 {
                #if DEBUG
                let refusal = submitted?.refusal ?? exact_raster_stats(id).last_refusal
                #else
                let refusal = exact_raster_stats(id).last_refusal
                #endif
                if Self.transientRequestRefusal(refusal) {
                    if !item.admissionDeferred { view.presenter?.session?.log("image queued: raster request admission") }
                    item.admissionDeferred = true
                } else {
                    item.failure = Self.refusal(refusal); settle(&item, view)
                    view.presenter?.session?.log("image deferred: \(item.failure!)")
                }
            } else {
                item.admissionDeferred = false
            }
            interests[viewID] = item
            // A cache hit has no worker completion to wake delivery: the wake and
            // mount passes take it now. A direct pass, which may run inside a batch
            // or a layout, and any other settled state (a refusal) leave it to the
            // next wake, as a worker's completion would.
            guard item.request != 0 else { continue }
            let state = exact_raster_status(id, item.request)
            if state == 4, pass != .direct, take(viewID, &item, view) { progress = true } else if state >= 4 { backend.wake() }
        }
    }
    /// The view lets go of its picture and its request while off screen,
    /// keeping its interest. The pixels it held go cold, for the core to
    /// evict when a decode needs their room, and `reconcile` asks for them
    /// again once the view is on screen (`scrolled`, `resized`). Returns
    /// the bytes the picture held.
    @discardableResult private func park(_ viewID: UInt32, _ item: inout Interest, _ view: NodeView) -> Int {
        let bytes = (view.raster as? NativeRasterLease)?.decoded.residentBytes ?? 0
        if item.request != 0 { exact_raster_cancel(id, item.request) }
        item.request = 0; item.requestedPixel = 0; item.delivered = false
        item.admissionDeferred = false; item.waiting = nil; item.parked = true
        interests[viewID] = item
        if view.raster != nil {
            // The sublayer's contents and a player's frames hold the pixels too.
            view.raster = nil; view.applyImageLayer(); AnimatedRasters.shared.attach(view)
            #if os(macOS)
            view.needsDisplay = true
            #else
            view.setNeedsDisplay()
            #endif
        }
        #if os(macOS)
        if parked.isEmpty { watchScrolls() }
        #else
        watchScrolls(above: view)
        #endif
        parked.insert(viewID)
        return bytes
    }
    private func unpark(_ viewID: UInt32) {
        guard parked.remove(viewID) != nil, parked.isEmpty else { return }
        stopWatchingScrolls()
    }
    /// Pictures held by views off screen, let go of until `bytes` are
    /// covered, the view farthest from the window first: a view on screen
    /// waits on the budget they fill. Returns the bytes let go of.
    private func release(_ bytes: Int) -> Int {
        guard bytes > 0 else { return 0 }
        var held: [(distance: CGFloat, id: UInt32)] = []
        for (viewID, item) in interests where item.delivered {
            // A flying picture is a flight's stand-in until it lands.
            guard let view = item.view, view.raster != nil, view.flightLook == nil, !Self.mayShow(view) else { continue }
            held.append((Self.distance(view), viewID))
        }
        held.sort { $0.distance > $1.distance }
        var freed = 0
        for (_, viewID) in held where freed < bytes {
            guard var item = interests[viewID], let view = item.view else { continue }
            freed += park(viewID, &item, view)
        }
        return freed
    }
    /// How far a view's box is from the window, in points: 0 inside it.
    private static func distance(_ view: NodeView) -> CGFloat {
        #if os(macOS)
        guard let root = view.window?.contentView else { return .greatestFiniteMagnitude }
        #else
        guard let root = view.window else { return .greatestFiniteMagnitude }
        #endif
        let box = view.convert(view.bounds, to: root), port = root.bounds
        return max(0, box.minX - port.maxX, port.minX - box.maxX) + max(0, box.minY - port.maxY, port.minY - box.maxY)
    }
    /// A scroll container moved, or the bar marked a picture it draws
    /// (`NodeView.drawnByBar`): a parked view may show now, and a pass asks
    /// for its picture. Only the parked views are looked at.
    func visibilityChanged() {
        guard !parked.isEmpty else { return }
        // A held picture may have left the screen or come back (`hold`).
        if parked.contains(where: { interests[$0].map { $0.held || $0.retrying || $0.watching } == true }) { backend.wake(); return }
        for viewID in parked where interests[viewID]?.view.map(Self.mayShow) == true { backend.wake(); return }
    }
    #if os(macOS)
    /// Every clip view that posts its bounds (the viewport's, a node's
    /// scroller's), while a view is parked; none otherwise.
    private func watchScrolls() {
        scrollObserver = NotificationCenter.default.addObserver(forName: NSView.boundsDidChangeNotification, object: nil, queue: .main) { [weak self] _ in self?.visibilityChanged() }
    }
    private func stopWatchingScrolls() {
        if let scrollObserver { NotificationCenter.default.removeObserver(scrollObserver) }
        scrollObserver = nil
    }
    #else
    /// Each scroll view above a parked view (the viewport, a node's, a
    /// grouped list's), while one is parked; none otherwise.
    private func watchScrolls(above view: NodeView) {
        scrollWatches = scrollWatches.filter { $0.value.scroller != nil }
        var ancestor = view.superview
        while let current = ancestor {
            if let scroller = current as? UIScrollView, scrollWatches[ObjectIdentifier(scroller)] == nil {
                scrollWatches[ObjectIdentifier(scroller)] = ScrollWatch(scroller, scroller.observe(\.contentOffset) { [weak self] _, _ in self?.visibilityChanged() })
            }
            ancestor = current.superview
        }
    }
    private func stopWatchingScrolls() { scrollWatches.removeAll() }
    #endif
    /// The plan for a picture at `pixel` (LLP 1100 D7): HDR if wanted and it
    /// fits, else its own variant, else for a deep picture 8 bits before any
    /// resolution is given up.
    private static func fit(_ metadata: RasterMetadata, pixel: Int, available: Int, hdr: Bool, aspect: (Int, Int)? = nil) -> RasterDecodePlan? {
        if hdr, let shown = try? RasterDecodePlan(metadata: metadata, maxPixel: pixel, variant: RasterVariant.hdr, aspect: aspect),
           shown.peakBytes <= available { return shown }
        let full = try? RasterDecodePlan(metadata: metadata, maxPixel: pixel, aspect: aspect)
        if let full, full.peakBytes <= available || !full.deep { return full }
        return (try? RasterDecodePlan(metadata: metadata, maxPixel: pixel, variant: RasterVariant.reduced8, aspect: aspect)) ?? full
    }
    private static func wantsHDR(_ view: NodeView, _ metadata: RasterMetadata) -> Bool {
        metadata.hdr && DisplayRange.showsHDR(view, limit: view.style["dynamic_range_limit"]?.string)
    }
    /// The metadata and source caps are an admission queue, not a refusal.
    /// Taken in turn from after the last admitted view, so one source that
    /// stays busy does not hold back the rest.
    private func admitDeferred() {
        let keys = deferred.keys.sorted()
        for viewID in keys.filter({ $0 > deferredCursor }) + keys.filter({ $0 <= deferredCursor }) {
            guard let item = deferred[viewID], let view = item.view,
                  view.loadGeneration == item.generation, let resolver = item.resolver else {
                deferred.removeValue(forKey: viewID); continue
            }
            switch backend.acquire(item.source, resolver: resolver) {
            case .source(let source):
                deferred.removeValue(forKey: viewID)
                interests[viewID] = Interest(view: view, generation: item.generation, source: source)
                timings.admitted(viewID)
                deferredCursor = viewID
            case .busy:
                continue
            case .refused:
                deferred.removeValue(forKey: viewID)
                view.presenter?.session?.log("image deferred: invalid raster source or stopped loader")
                Self.announce([(view, item.generation, "invalid raster source")])
            }
        }
    }
    /// What a decode can have without lowering its resolution: the budget
    /// less the pixels views hold (pinned, retiring). Cold pixels are
    /// evicted before a decode waits (LLP 1010 §6.3), and a reservation is a
    /// decode in flight, released as it lands; the core queues a request
    /// until they make room.
    private func available() -> Int {
        let used = exact_raster_stats(id)
        let held = used.resident_bytes - min(used.resident_bytes, used.cold_bytes)
        return Int(budget - min(budget, held))
    }
    /// HTML `<img>`'s `load`, or its `error` with the reason (LLP 1011 §4),
    /// to a node that hears it: on the next main turn, after the sizes this
    /// turn reported and the batch that set the source, and only while the
    /// view still shows that load (`generation`; nil for a source refused
    /// before it had one).
    /// The natural sizes a pass landed, then each source's first `load` or `error`.
    private func publish(_ landed: [(view: NodeView, generation: Int, size: CGSize)], _ settled: [(view: NodeView, generation: Int?, failure: String?)]) {
        report(landed); Self.announce(settled)
        let presenter = landed.lazy.compactMap { $0.view.presenter }.first ?? settled.lazy.compactMap { $0.view.presenter }.first
        presenter?.flushFragment()
        #if DEBUG
        testAnnounced = Array((testAnnounced + settled.map { ($0.view.id, $0.failure) }).suffix(64))
        #endif
    }
    private static func announce(_ events: [(view: NodeView, generation: Int?, failure: String?)]) {
        for (view, generation, failure) in events {
            DispatchQueue.main.async { [weak view] in
                guard let view, generation.map({ $0 == view.loadGeneration }) ?? true,
                      let session = view.presenter?.session, view.presenter?.views[view.id] === view,
                      view.handlers.contains(failure == nil ? "load" : "error") else { return }
                session.apply(failure.map { session.runtime.media(view.id, event: "error", payload: $0, now: session.now()) }
                              ?? session.runtime.load(view.id, now: session.now()))
            }
        }
    }
    private func report(_ landed: [(view: NodeView, generation: Int, size: CGSize)]) {
        // The first still shown: a view gone by a deferred report has no presenter.
        guard let presenter = landed.lazy.compactMap({ $0.view.presenter }).first else { return }
        let sizes = landed.compactMap { item -> (UInt32, CGSize?)? in
            guard item.view.loadGeneration == item.generation, presenter.views[item.view.id] === item.view else { return nil }
            #if os(iOS) || os(tvOS)
            return (item.view.buttonRasterView ?? item.view.id, item.size)
            #else
            return (item.view.id, item.size)
            #endif
        }
        if !sizes.isEmpty { presenter.onIntrinsic?(sizes) }
    }
    func setPaused(_ paused: Bool) {
        guard self.paused != paused, !destroyed else { return }
        self.paused = paused
        backend.setPaused(paused)
        exact_raster_session_control(id, paused ? 1 : 2)
        for key in interests.keys {
            interests[key]?.request = 0; interests[key]?.failure = nil; interests[key]?.delivered = false
            interests[key]?.admissionDeferred = false; interests[key]?.waiting = nil
            // Paused, a picture is off screen: its count starts over.
            if var item = interests[key] {
                if item.held || item.retrying || item.watching { unwatch(key, &item) }
                item.retries = 0; interests[key] = item
            }
        }
        if !paused { reconcile() }
    }
    func reset() {
        interests.removeAll(); deferred.removeAll(); refused.removeAll(); refusedPictures.removeAll(); deferredCursor = 0
        parked.removeAll(); mounting.removeAll(); stopWatchingScrolls(); timings.reset()
        #if DEBUG
        testAnnounced.removeAll()
        #endif
        backend.reset(stop: false); exact_raster_session_control(id, 0)
    }
    func shutdown() {
        guard !destroyed else { return }; destroyed = true
        pressure?.cancel(); pressure = nil
        interests.removeAll(); deferred.removeAll(); parked.removeAll(); stopWatchingScrolls(); timings.reset()
        backend.reset(stop: true); exact_raster_session_control(id, 3)
    }
    var encodedHTTPCache: Any { RasterInput.httpCacheUsage }
    var diagnostics: [String: Any] {
        let s = exact_raster_stats(id)
        backend.lock.lock(); defer { backend.lock.unlock() }
        return ["residentBytes": s.resident_bytes, "reservedBytes": s.reserved_bytes, "peakBytes": s.peak_bytes,
            "pinnedBytes": s.pinned_bytes, "coldBytes": s.cold_bytes, "retiringBytes": s.retiring_bytes,
            "running": s.process_running, "sessionRunning": s.running, "ready": s.ready, "deliveryCells": s.delivery_cells,
            "pending": s.pending_jobs, "subscribers": s.subscribers, "coldEntries": s.cold_entries,
            "sources": backend.sources.count, "metadataReads": backend.metadataReads, "decoded": backend.decoded,
            "cancelled": s.cancelled, "sourceLimit": backend.sourceLimit,
            "refusals": backend.refusals, "viewInterests": interests.count, "lastRefusal": Self.refusal(s.last_refusal),
            "images": interests.map { view, interest -> [String: Any] in
                ["view": view, "source": String(interest.source.id), "offeredPixel": interest.offeredPixel,
                 "admittedPixel": interest.requestedPixel, "status": exact_raster_status(id, interest.request),
                 "resolutionFallback": interest.requestedPixel > 0 && interest.requestedPixel < interest.offeredPixel ? "budget" : NSNull(),
                 "failure": interest.failure ?? "", "declines": interest.declines, "parked": interest.parked,
                 "held": interest.held, "retries": interest.retries, "retrying": interest.retrying, "pixels": [interest.view?.raster?.image.image.width ?? 0, interest.view?.raster?.image.image.height ?? 0],
                 "color": Self.colorFacts(interest), "timing": timings.stamps(view) ?? NSNull()]
            },
            "lateImages": timings.summary,
            "deferred": interests.values.filter { $0.admissionDeferred || $0.failure != nil || ($0.request != 0 && exact_raster_status(id, $0.request) == 2) }.count,
            "deferredAdmission": deferred.count, "parked": interests.values.filter(\.parked).count,
            "watched": parked.count,
            "scope": "Exact-owned RGBA storage plus conservative thumbnail/conversion reservation; ImageIO internals excluded"]
    }
    /// What the agent sees of a picture's storage (LLP 1100 D12).
    private static func colorFacts(_ interest: Interest) -> [String: Any] {
        let names: [UInt32: String] = [RasterVariant.srgb8: "srgb8", RasterVariant.own8: "own8",
                                       RasterVariant.deep: "deep", RasterVariant.hdr: "hdr", RasterVariant.reduced8: "reduced8"]
        var facts: [String: Any] = ["variant": names[interest.variant] ?? String(interest.variant),
                                    "fallback": interest.variant == RasterVariant.reduced8 ? "budget" : NSNull()]
        if let image = interest.view?.raster?.image.image {
            facts["space"] = image.colorSpace.map(colorSpaceName) ?? "none"
            facts["bitsPerComponent"] = image.bitsPerComponent
            facts["bytesPerPixel"] = image.bitsPerPixel / 8
            if let raster = interest.view?.raster?.image, raster.isHDR { facts["headroom"] = raster.headroom }
            if let layer = interest.view?.imageContentLayer {
                var shown = layer.dynamicRangeFacts
                shown["current"] = (layer.contents as AnyObject?) === image
                facts["layer"] = shown
            }
        }
        return facts
    }
    private static func refusal(_ code: UInt64) -> String {
        let names = ["none", "overflow", "invalid dimensions", "encoded limit", "header limit", "source pixels",
            "too large", "budget", "queue full", "subscriber limit", "conflicting metadata", "paused", "shutdown",
            "stale", "actual exceeds reservation", "decode failed"]
        return code < names.count ? names[Int(code)] : "unknown refusal"
    }
    /// A full request queue is temporary: every decode that lands or fails
    /// wakes reconciliation, which asks again, and a pass that makes room
    /// wakes one itself. Any other refusal stands.
    static func transientRequestRefusal(_ code: UInt64) -> Bool { code == 8 }
}
