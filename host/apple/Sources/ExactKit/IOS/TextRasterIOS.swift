// UIKit paragraph layers consume pixels from the same workers and paint routine
// as AppKit. Native views, inline links and accessibility keep their identities.
// @ref LLP 1044.000 §6 S5
#if os(iOS) || os(tvOS)
import UIKit
import CoreText

final class TextRasterizer {
    private final class Work {
        weak var node: NodeView?
        let key: TextRasterKey
        let namespace: Int
        let diagnosticNode: UInt32
        /// The node's incarnation when the job was issued (LLP 1068 §4.9).
        let incarnation: UInt64
        let group = DispatchGroup()
        weak var operation: Operation?
        private let lock = NSLock()
        private enum State {
            case queued, running, abandoned, finished(TextRasterImage?), consumed
        }
        private var state = State.queued
        init(_ node: NodeView, key: TextRasterKey, namespace: Int) {
            self.node = node; self.key = key; self.namespace = namespace; incarnation = node.incarnation
            diagnosticNode = node.id
            group.enter()
        }
        /// Claim and finish one result; cancellation can only win before the claim.
        func perform(_ render: () -> TextRasterImage?) {
            lock.lock()
            guard case .queued = state else { lock.unlock(); return }
            state = .running
            lock.unlock()
            let result = render()
            lock.lock()
            state = .finished(result)
            lock.unlock()
        }
        var wasAbandoned: Bool {
            lock.lock(); defer { lock.unlock() }
            if case .abandoned = state { return true }
            return false
        }
        /// A completed mailbox transfers ownership once, even when settlement beats the queued notification.
        func consume() -> TextRasterImage? {
            precondition(group.wait(timeout: .now()) == .success)
            lock.lock(); defer { lock.unlock() }
            switch state {
            case .finished(let result): state = .consumed; return result
            case .queued, .abandoned: state = .consumed; return nil
            case .running: preconditionFailure("Text work completes before consumption")
            case .consumed: preconditionFailure("Text work is consumed once")
            }
        }
        /// Drop a job no worker has begun: true if it will never render.
        func abandon() -> Bool {
            lock.lock()
            let unstarted: Bool
            switch state {
            case .queued: state = .abandoned; unstarted = true
            case .abandoned: unstarted = true
            default: unstarted = false
            }
            lock.unlock()
            if unstarted { operation?.cancel() }
            return unstarted
        }
    }
    private var working: [Work] = []
    /// A worker per two cores, from two to four; twice that many jobs in
    /// flight, so a worker finishing one finds the next already queued.
    static let concurrency = max(2, min(4, ProcessInfo.processInfo.activeProcessorCount / 2))
    private static let maxInFlight = concurrency * 2
    private static let maximumBytes: CGFloat = 16 * 1024 * 1024
    /// A tall paragraph's band a worker paints ahead of the scroll: room
    /// for two ports at 3x on the iPhone Duo's open display, so the band is
    /// ready before the port leaves the one shown.
    private static let leadBytes: CGFloat = 48 * 1024 * 1024
    /// Every tall paragraph's band, shown or being painted, takes its bytes
    /// beyond `maximumBytes` from this one allowance, so they do not grow
    /// with the number of tall paragraphs near the port: a band left
    /// without any is as tall as the main thread's.
    private static let leadRoom = leadBytes - maximumBytes
    /// The paragraphs that have had a band, weakly.
    private let banded = NSHashTable<NodeView>.weakObjects()
    /// The most text (UTF-16 units) a changed paragraph paints on the main
    /// thread to replace its old words at once: a paragraph's worth. A
    /// longer one, which a short box may clip to a few of its lines, keeps
    /// the worker path, as every line of it is laid out to paint.
    private static let urgentWords = 1_000
    var inFlight: Int { working.count }
    var hasRoom: Bool { working.count < Self.maxInFlight }
    /// A worker's slot freed, whatever became of its pixels: a pass that
    /// found none free can go on, and when no pass waits, a paragraph
    /// whose `display()` the rasterizer refused gets its pass now.
    var slotFreed: (() -> Void)?

    /// Pixels by what they paint. Rows repeat small labels (a reaction's
    /// emoji and count, a relative time), and a list that turns back shows
    /// what it just left: those take the pixels another view already paid
    /// for. Least recently used first, bounded by the bytes each holds (its
    /// pixels or glyph values, and its key's text); only small entries, so
    /// what stays beyond the views that show it is a few megabytes.
    /// Kept by `TextRasterKey.anyRoom`, with the key it was painted for: a
    /// view of another size, or the same view after its padding changed,
    /// takes pixels that serve it (`TextRasterKey.serves`).
    private struct Kept {
        let image: TextRasterImage
        let key: TextRasterKey
        let bytes: Int
        var used: UInt64
    }
    private var kept: [TextRasterKey: Kept] = [:]
    private var keptBytes = 0
    private var keptClock: UInt64 = 0
    private var keptNamespace = 0
    private static let keptLimit = 4 * 1024 * 1024
    private static let keptEntryLimit = 64 * 1024
    private func keptImage(_ key: TextRasterKey, namespace: Int) -> TextRasterImage? {
        if namespace != keptNamespace { kept.removeAll(); keptBytes = 0; keptNamespace = namespace; return nil }
        guard var hit = kept[key.anyRoom], hit.key.serves(key, ink: hit.image.ink, shadows: hit.image.shadows) else { return nil }
        keptClock += 1; hit.used = keptClock; kept[key.anyRoom] = hit
        return hit.image
    }
    private func sharedImage(_ image: TextRasterImage?, for key: TextRasterKey, namespace: Int) -> TextRasterImage? {
        // A late worker cannot switch the cache back to an earlier font namespace
        guard key.clip == nil, namespace == keptNamespace else { return image }
        guard let shared = keptImage(key, namespace: namespace) else { return image }
        // A live shared mask wins at publication; otherwise matching glyph values take the worker's mask
        if case .coverage(let prepared)? = image?.content, let drawing = shared.drawing,
           let coverage = prepared.reusing(for: drawing) {
            return shared.replacingInk(with: .coverage(coverage))
        }
        return shared
    }
    private func keep(_ image: TextRasterImage?, for key: TextRasterKey, namespace: Int) {
        let any = key.anyRoom
        guard var image, namespace == keptNamespace else { return }
        // Strip only the cache's value copy; the mailbox keeps the mask until a view takes it
        if let drawing = image.drawing { image = image.replacingInk(with: .drawn(drawing)) }
        let ink: Int
        switch image.content {
        case .pixels(let pixels): ink = pixels.bytesPerRow * pixels.height
        case .drawn(let drawing): ink = drawing.bytes
        case .coverage(let coverage): ink = coverage.bytes
        }
        // The key holds the paragraph's text, however little of it shows
        let text = key.spec.runs.reduce(0) { $0 + $1.text.utf8.count }
        let bytes = ink + (image.cast.map { $0.bytesPerRow * $0.height } ?? 0) + text
        guard bytes <= Self.keptEntryLimit else { return }
        // A newer image replaces one that cannot serve its room (a narrower view's, its ink cut)
        if let old = kept[any] {
            guard !old.key.serves(key, ink: old.image.ink, shadows: old.image.shadows) else { return }
            kept.removeValue(forKey: any); keptBytes -= old.bytes
        }
        keptClock += 1
        kept[any] = Kept(image: image, key: key, bytes: bytes, used: keptClock)
        keptBytes += bytes
        guard keptBytes > Self.keptLimit else { return }
        for (key, entry) in kept.sorted(by: { $0.value.used < $1.value.used }) {
            kept.removeValue(forKey: key); keptBytes -= entry.bytes
            if keptBytes <= Self.keptLimit * 3 / 4 { break }
        }
    }

    /// Let go of the pixels no view shows.
    func dropKept() { kept.removeAll(); keptBytes = 0 }

    /// What the paragraph's pixels hold: its words and style, the box they
    /// are laid out in (its width breaks and aligns the lines), the scale,
    /// and where its ink is kept (`TextRasterKey.room`), all in the box's own
    /// space, never where the box is in its view (`NodeView.placeTextRaster`).
    /// The view's bounds are that room, and a tall view's paragraph is
    /// painted in bands of the view's rows.
    /// `urgent`: the band is painted on the main thread now, so it is no
    /// taller than it must be; a worker's band leads the scroll instead.
    private func key(_ node: NodeView, urgent: () -> Bool) -> TextRasterKey {
        let scale = max(1, node.window?.screen.scale ?? node.traitCollection.displayScale)
        let box = node.paragraphBox(), room = node.textRasterRoom(box)
        var key = TextRasterKey(spec: node.paragraphSpec(), size: box.size, box: CGRect(origin: .zero, size: box.size), scale: scale, room: room)
        // Whole ordinary paragraphs, viewport bands for tall ones. Never allocate
        // to an unbreakable line's advance; the shared painter clips its ink.
        if room.height > 4096 || room.width * room.height * scale * scale * 4 > Self.maximumBytes {
            // `band` and `keeps` measure from the view's top left.
            let inView = { (r: CGRect) in r.offsetBy(dx: -room.minX, dy: -room.minY) }
            let port = (node.presenter?.textPreparationRect(node) ?? node.bounds).offsetBy(dx: -box.minX, dy: -box.minY)
            let down = node.textMore.bandTravel.follow(port)
            // Kept until the port nears its edge ahead; then a worker paints the next.
            if let old = node.textRasterKey, var clip = old.clip, clip.minY <= port.minY, clip.maxY >= port.maxY,
               TextRasterJob.keeps(inView(clip), port: inView(port), down: down, height: room.height) || urgent() {
                // Its rows, across the view as it is now
                let columns = TextRasterJob.bandColumns(key.spec, width: room.width).offsetBy(dx: room.minX, dy: 0)
                clip.origin.x = columns.minX; clip.size.width = columns.width
                var kept = key
                kept.clip = clip
                if kept == old || node.textRasterServes(kept) { return kept }
            }
            let now = urgent()
            let lead = now ? 0 : max(0, Self.leadRoom - leadTaken(besides: node))
            key.clip = TextRasterJob.band(key.spec, width: room.width, port: inView(port), scale: scale,
                                          maximumBytes: Self.maximumBytes + lead, down: now ? nil : down)
                .offsetBy(dx: room.minX, dy: room.minY)
            banded.add(node)
        }
        return key
    }

    /// The allowance other paragraphs' bands hold, counted as a band is
    /// sized, four bytes a pixel: the pixels each shows, which stay up while
    /// new ones are asked for (`invalidateText` clears the key and keeps
    /// them), and each job still to paint, whatever its paragraph asks for
    /// now (one begun runs to its end). `node`'s own is the band its new one
    /// replaces.
    private func leadTaken(besides node: NodeView) -> CGFloat {
        func beyond(_ width: CGFloat, _ height: CGFloat, _ scale: CGFloat) -> CGFloat {
            max(0, width * height * scale * scale * 4 - Self.maximumBytes)
        }
        let shown = banded.allObjects.reduce(0) { taken, other in
            guard other !== node, other.textRaster != nil else { return taken }
            return taken + beyond(other.textRasterFrame.width, other.textRasterFrame.height, other.textRasterScale)
        }
        return working.reduce(shown) { taken, work in
            guard work.node !== node, !work.wasAbandoned, let clip = work.key.clip else { return taken }
            return taken + beyond(clip.width, clip.height, work.key.scale)
        }
    }

    /// Pixels for `node`'s paragraph. `urgent`: what shows has none, so they
    /// are painted now — taken from its worker if that has finished or is
    /// running, else rendered here. `now`: what shows is stale, and is
    /// painted now too (a presented colour). Otherwise a worker job at
    /// `priority`, if there is room for one (false when there is not).
    /// `urgent` and what shows are asked only of a paragraph whose pixels
    /// are not already its words', and `urgent` also of a tall one whose
    /// port nears its band's edge (`key`): they walk its clipping ancestors,
    /// and a batch's display pass asks again of every paragraph it gave new
    /// words.
    @discardableResult
    func ensure(_ node: NodeView, urgent isUrgent: @autoclosure () -> Bool, now: Bool = false, priority: Operation.QueuePriority = .normal) -> Bool {
        guard node.canRasterText else { return true }
        // Asked once, and only when needed: it walks the clipping ancestors.
        lazy var urgent = isUrgent()
        let key = key(node, urgent: { urgent })
        // Pixels that hold what `key`'s would are its own (`TextRasterKey.serves`).
        if node.textRasterKey != key, node.textRasterServes(key) { node.adoptTextRaster(key) }
        if node.textRasterKey == key && node.textRasterReady { return true }
        let visible = node.presenter?.textUrgentRect(node) ?? .zero
        // A whole paragraph's pixels of other words are none of this text's:
        // an urgent paint replaces them now, rather than a worker a frame
        // later. A tall paragraph's bands, a long text and a shadowed one
        // (each line a blurred layer) keep the worker path.
        let otherWords = key.clip == nil && node.textRasterSpec != key.spec
            && key.spec.shadow == nil && !key.spec.runs.contains { $0.shadow != nil }
            && key.spec.runs.reduce(0) { $0 + $1.text.utf16.count } <= Self.urgentWords
        let missingPixels = node.textRaster == nil || !node.textRasterFrame.contains(visible) || now || otherWords
        if node.textRasterKey == key && (node.textRasterReady || !urgent || !missingPixels) { return true }
        let firstPixels = urgent && missingPixels
        let pending = working.last { $0.node === node }
        guard let engine = node.text else { return true }
        if key.clip == nil, let image = keptImage(key, namespace: engine.namespace) {
            if let pending { _ = pending.abandon() }
            node.textRasterKey = key; node.textRasterReady = false; node.textRasterFailed = false
            node.showTextRaster(image, for: key)
            return true
        }
        // A finished worker for this key shows only if the node still asks
        // for it: the key a batch's change cleared is asked for again first
        if firstPixels, let pending, pending.key == key {
            node.textRasterKey = key; node.textRasterReady = false
            if take(pending), node.textRasterReady { return true }
        }
        guard firstPixels || hasRoom else { return false }
        guard let job = job(node, key: key, engine: engine) else { return true }
        // A job for the paragraph's previous text or box paints nothing now.
        if let pending { _ = pending.abandon() }
        node.textRasterKey = key; node.textRasterReady = false; node.textRasterFailed = false
        if firstPixels, urgentBatch != nil {
            urgentBatch?.append(UrgentPaint(node: node, key: key, job: job, namespace: engine.namespace))
        } else if firstPixels {
            let post = Presenter.signposts.beginInterval("text-raster-urgent"), phase = HostPhases.shared.begin(.text)
            defer { HostPhases.shared.end(phase) }
            let image = job.renderDiagnosed(node: node.id)
            node.showTextRaster(image, for: key)
            if key.clip == nil { keep(image, for: key, namespace: engine.namespace) }
            Presenter.signposts.endInterval("text-raster-urgent", post)
        } else {
            let work = Work(node, key: key, namespace: engine.namespace)
            working.append(work)
            work.group.notify(queue: .main) { [weak self] in self?.publish(work) }
            let operation = BlockOperation {
                work.perform {
                    let post = Presenter.signposts.beginInterval("text-raster-worker")
                    defer { Presenter.signposts.endInterval("text-raster-worker", post) }
                    return job.renderPrepared(node: work.diagnosticNode)
                }
            }
            operation.queuePriority = priority
            // Also when abandoned unstarted: the mailbox always closes.
            operation.completionBlock = { work.group.leave() }
            work.operation = operation
            RegionTextExecutor.queue.addOperation(operation)
        }
        return true
    }
    /// The job that paints `key`'s pixels of `node`'s paragraph. For a band,
    /// it lays out only the band's lines: laying out all of a long paragraph's
    /// lines would make each band cost as much as the whole paragraph.
    func job(_ node: NodeView, key: TextRasterKey, engine: TextEngine) -> TextRasterJob? {
        let measured = engine.measuredBreaks(key.spec, width: key.box.width)
        let paragraph = measured == nil ? node.paragraphLayout() : nil
        guard let geometry = measured ?? paragraph.map(LineGeometry.init) else { return nil }
        let source = paragraph?.shape?.attributed ?? engine.attributed(key.spec)
        let glyphs = engine.measuredGlyphs(key.spec, width: key.box.width)
        // The measured glyphs' fast path pairs each of its lines with a baseline, so it takes them all
        let lines = glyphs == nil ? Self.lines(geometry, in: key) : (geometry.ranges, geometry.baselines)
        return TextRasterJob(source: source.copy() as! NSAttributedString, ranges: lines.0, baselines: lines.1,
            flush: key.spec.align == 1 ? 0.5 : key.spec.align == 2 ? 1 : 0, whiteSpace: key.spec.whiteSpace, justifies: key.spec.align == 3, insets: paragraph?.insets ?? LineInsets(key.spec, source: source),
            box: key.box, size: key.size, scale: key.scale, clip: key.clip, room: key.room,
            ellipsis: key.spec.ellipsis, crop: true, clamped: geometry.clamped, shadow: key.spec.hdrShadow.map(TextRunShadow.init),
            antialiased: key.spec.antialiased,
            measuredGlyphs: glyphs,
            measuredColor: TextEngine.color(key.spec.color).cgColor)
    }

    /// A band's lines: those whose baselines lie within reach of its rows.
    /// The reach adds the ink a job keeps past its box (`maxInkOverflow`), the
    /// largest step between baselines (a line with an inline image), four
    /// times the largest font size (a glyph whose ink reaches past its line)
    /// and how far the shadows in its pixels are cast up and down
    /// (`Spec.shadowRows`), however far that is.
    static func lines(_ geometry: LineGeometry, in key: TextRasterKey) -> ([CFRange], [CGFloat]) {
        let baselines = geometry.baselines
        guard let clip = key.clip, geometry.clamped == nil, baselines.count == geometry.ranges.count, baselines.count > 1 else {
            return (geometry.ranges, baselines)
        }
        var gap: CGFloat = 0
        for i in 1..<baselines.count { gap = max(gap, baselines[i] - baselines[i - 1]) }
        let size = key.spec.runs.map(\.size).max() ?? 0
        // A line above the band reaches it with a shadow cast down, one below with a shadow cast up.
        let reach = TextRasterJob.maxInkOverflow + gap + 4 * size, shadows = key.spec.shadowRows
        let low = clip.minY - reach - shadows.down, high = clip.maxY + reach + shadows.up
        guard let first = baselines.firstIndex(where: { $0 >= low }) else { return ([], []) }
        let end = baselines[first...].firstIndex(where: { $0 > high }) ?? baselines.count
        return (Array(geometry.ranges[first..<end]), Array(baselines[first..<end]))
    }

    private struct UrgentPaint {
        let node: NodeView
        let key: TextRasterKey
        let job: TextRasterJob
        let namespace: Int
        var diagnosticNode: UInt32 { node.id }
    }
    /// The paragraphs `ensureUrgent` prepares together, while it runs.
    private var urgentBatch: [UrgentPaint]?

    /// Start urgent preparation serially to limit contention on shared glyph caches.
    private static let urgentAlone: CFTimeInterval = 0.002

    /// `ensure(urgent: true)` for each of `nodes`. Prepare results in turn
    /// for `urgentAlone`, then the rest at once, the main thread among the
    /// workers. Glyph plans defer painting until main-thread publication,
    /// after reuse is checked; pixel jobs paint during preparation. This
    /// threshold bounds serial preparation, not publication.
    func ensureUrgent(_ nodes: [NodeView], now: Bool = false) {
        urgentBatch = []
        for node in nodes { ensure(node, urgent: true, now: now) }
        let batch = urgentBatch ?? []
        urgentBatch = nil
        guard !batch.isEmpty else { return }
        let post = Presenter.signposts.beginInterval("text-raster-urgent"), phase = HostPhases.shared.begin(.text)
        defer { Presenter.signposts.endInterval("text-raster-urgent", post); HostPhases.shared.end(phase) }
        var images = [TextRasterImage?](repeating: nil, count: batch.count)
        images.withUnsafeMutableBufferPointer { slots in
            let start = CACurrentMediaTime()
            var next = 0
            while next < batch.count && CACurrentMediaTime() - start < Self.urgentAlone {
                slots[next] = batch[next].job.renderDiagnosed(node: batch[next].diagnosticNode)
                next += 1
            }
            let first = next
            guard first < batch.count else { return }
            let identifiers = batch.map(\.diagnosticNode)
            DispatchQueue.concurrentPerform(iterations: batch.count - first) {
                slots[first + $0] = batch[first + $0].job.renderDiagnosed(node: identifiers[first + $0])
            }
        }
        for (paint, image) in zip(batch, images) { publishUrgent(paint, image: image) }
    }

    private func publishUrgent(_ paint: UrgentPaint, image rendered: TextRasterImage?) {
        let image = sharedImage(rendered, for: paint.key, namespace: paint.namespace)
        paint.node.showTextRaster(image, for: paint.key)
        if paint.key.clip == nil { keep(image, for: paint.key, namespace: paint.namespace) }
    }

    /// A worker's pixels for what shows now: its finished result, or the
    /// running one waited for (it has the paragraph partly painted; the main
    /// thread would start over). False when it has not begun: it is dropped
    /// and the caller paints.
    private func take(_ work: Work) -> Bool {
        guard !work.wasAbandoned else { return false }
        if work.group.wait(timeout: .now()) != .success {
            if work.abandon() { return false }
            let post = Presenter.signposts.beginInterval("text-raster-wait")
            let diagnostic = TextRenderMetrics.shared.begin()
            let done = work.group.wait(timeout: .now() + .milliseconds(8)) == .success
            TextRenderMetrics.shared.end(diagnostic, kind: .workerWait, node: work.diagnosticNode, detail: done ? 1 : 0)
            Presenter.signposts.endInterval("text-raster-wait", post)
            guard done else { return false }
        }
        publish(work)
        return true
    }
    private func publish(_ work: Work) {
        guard let index = working.firstIndex(where: { $0 === work }) else { return }
        // Only a completed worker's mailbox is read, including by agent settlement.
        guard work.group.wait(timeout: .now()) == .success else { return }
        working.remove(at: index)
        defer { slotFreed?() }
        // Dropped unstarted: no pixels, and no failure either.
        guard !work.wasAbandoned else { return }
        let image = sharedImage(work.consume(), for: work.key, namespace: work.namespace)
        if work.key.clip == nil { keep(image, for: work.key, namespace: work.namespace) }
        guard let node = work.node, node.incarnation == work.incarnation else { return }
        node.showTextRaster(image, for: work.key)
    }
    /// Jobs whose paragraph left, or whose text or box changed since, or
    /// that the list has already carried past (`passed`), and that no worker
    /// has begun, are dropped: their pixels would never show.
    func abandonStale(passed: (NodeView) -> Bool) {
        for work in working {
            guard let node = work.node, node.textRasterKey == work.key else { _ = work.abandon(); continue }
            // A passed paragraph owes a job again if the list comes back.
            if passed(node), work.abandon() { node.textRasterKey = nil }
        }
    }

    /// A synchronous agent screenshot must observe the requested appearance,
    /// not a previous accepted raster. Rendering stays on the workers; their
    /// mailboxes can be published here without draining the main queue reentrantly.
    func settleVisible(_ nodes: [NodeView]) {
        let deadline = CACurrentMediaTime() + 1
        repeat {
            for node in nodes where node.canRasterText { ensure(node, urgent: false) }
            let batch = working
            guard !batch.isEmpty else { return }
            for work in batch {
                let left = max(0, deadline - CACurrentMediaTime())
                if work.group.wait(timeout: .now() + left) == .success { publish(work) }
            }
            if nodes.allSatisfy({ !$0.canRasterText || ($0.textRasterReady && $0.textRasterKey != nil) }) { return }
        } while CACurrentMediaTime() < deadline
    }

}

extension NodeView {
    var canRasterText: Bool { rastersText && !rollsNumerals }
    /// What a paragraph's raster needs of its structure, whether or not its
    /// numerals roll instead.
    private var rastersText: Bool {
        if textRasterFailed && textRasterKey != nil { return false }
        // `text-overflow: ellipsis` truncates in the raster job, as `draw(_:)`
        // does (LLP 1053 G5): a label stretched across a row rasters its text,
        // not a backing store of the row's width. A `line-clamp`'s last line
        // is made again from the range it broke at (`LineGeometry.clamped`).
        guard isParagraph && flowShapes.isEmpty && columnRecord == nil && !Capture.capturing && window != nil && backgroundClip != "text"
            && bounds.width > 0 && bounds.height > 0 else { return false }
        return canvasAbove == nil && !paintsVisibleInlineRun
    }
    /// A rolling paragraph is drawn again when it changes: its new text
    /// may move it between the capability and the raster (`Presenter.touched`).
    var hasContentTransition: Bool { (style["content_transition"]?.string ?? "none") != "none" }
    /// LLP 1077 D15: the paragraph's numerals roll per glyph, drawn by the
    /// Numerals capability (SwiftUI's `Text`) rather than as a raster
    /// (`NumeralLink`), when that draws what Exact's painter would:
    /// - its structure is one a raster could hold (`rastersText`);
    /// - its layout is one whole line of text, uncut by `text-overflow` or
    ///   `line-clamp`, with no image in it (LLP 1011.002) and no white space
    ///   it keeps (`white-space: pre`, `pre-wrap`, or at its end), which
    ///   `Text` places otherwise, and what is painted, the glyphs and the
    ///   runs' backgrounds on the painted baseline, lies within the box;
    /// - its paint is what `Text` draws from an attributed string (font,
    ///   colour, kerning, underline, line-through) and the backgrounds
    ///   `showNumerals` paints: no shadow, stroke or inline opacity group, nothing hidden.
    /// Any other paragraph is drawn as any other; where it rasters, the
    /// raster rolls whole.
    var rollsNumerals: Bool {
        guard NumeralLink.installed != nil, hasContentTransition, rastersText, !cssVisibilityHidden else { return false }
        let spec = paragraphSpec()
        guard spec.shadow == nil, !spec.runs.contains(where: { $0.box != nil || $0.hidden || $0.stroke != nil || $0.shadow != nil || !$0.opacity.isEmpty }),
              let p = paragraphLayout(), p.lines.count <= 1 else { return false }
        guard let first = p.lines.first else { return true }
        let box = paragraphBox()
        let line = spec.ellipsis ? p.ellipsized(0, spec: spec, width: box.width) : first
        guard line === first, p.clampedRange == nil, !spec.preserves, CTLineGetTrailingWhitespaceWidth(first) == 0 else { return false }
        // The line's ink, where `TextEngine.draw` puts it: the raster keeps
        // ink past the box, this view's own drawing does not.
        let scale = max(1, window?.screen.scale ?? traitCollection.displayScale)
        let x = box.minX + p.origin(0, align: spec.align, width: box.width)
        let baseline = TextEngine.paintedLine(p.baselines[0], top: box.minY, scale: scale, lineScale: p.lineScale)
        let glyphs = CTLineGetBoundsWithOptions(first, .useGlyphPathBounds)
        var ink = glyphs.isEmpty ? CGRect.null : CGRect(x: x + glyphs.minX, y: baseline - glyphs.maxY, width: glyphs.width, height: glyphs.height)
        for fill in TextLinePaint.backgrounds(first, at: CGPoint(x: x, y: baseline)) { ink = ink.union(fill.rect) }
        return ink.isNull || bounds.contains(ink)
    }
    /// The paragraph as the Numerals capability draws it: its line at the
    /// line's place and baseline. SwiftUI's
    /// `Text` reads no inline box's background, so those are painted here,
    /// under it, for the text as it now reads; they do not roll.
    func showNumerals(_ ctx: CGContext) {
        guard let engine = text, let capability = NumeralLink.installed else { return }
        dropTextRaster()
        let spec = paragraphSpec(), box = paragraphBox(), mode = style["content_transition"]?.string ?? "numeric"
        let shown = engine.attributed(spec)
        guard let p = paragraphLayout(), let line = p.lines.first else {
            return capability.show(self, attributed: shown, box: contentBox(), baseline: nil, align: spec.align, mode: mode)
        }
        TextEngine.draw(p, spec: spec, in: box, context: ctx, pass: .backgrounds)
        let baseline = TextEngine.paintedLine(p.baselines[0], top: box.minY, scale: TextEngine.deviceScale(ctx), lineScale: p.lineScale)
        let width = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
        let lineBox = CGRect(x: box.minX + p.origin(0, align: spec.align, width: box.width), y: box.minY,
                             width: max(1, width), height: max(1, p.height, baseline - box.minY))
        capability.show(self, attributed: shown, box: lineBox, baseline: baseline - box.minY, align: spec.align, mode: mode)
    }
    /// The whole paragraph's pixels are up for its current text and box: a
    /// refresh has nothing to do for it until a change clears its key
    /// (`invalidateText`, `textRasterGeometryChanged`, `dropTextRaster`).
    var textRasterSettled: Bool { textRasterReady && textRasterWhole }
    func textRasterGeometryChanged() {
        // Laid out to nothing (a text that became a collapsed space, as
        // stopwatch's status line did on Reset, LLP 1116.005 D4), it paints
        // nothing. Neither the pump nor UIKit's `draw(_:)`, which lets go of
        // a raster that no longer applies, visits an empty box: the old ink
        // goes here, or it stays on screen beside the text that replaced it.
        if bounds.isEmpty {
            if textRaster != nil || textRasterLayer != nil { dropTextRaster() }
            return
        }
        // A box that moves keeps its pixels, which move with it; only one of
        // another size, or whose pixels the view's new bounds cut otherwise
        // (`TextRasterKey.serves`), is painted again.
        placeTextRaster()
        let box = paragraphBox()
        if var key = textRasterKey, key.size == box.size {
            let room = textRasterRoom(box)
            if key.room == room { return }
            key.room = room
            if textRasterServes(key) { adoptTextRaster(key); return }
        }
        textRasterKey = nil
        presenter?.requestTextPublication()
    }
    /// The view's bounds in its text's box's space: where the paragraph's
    /// ink is kept (`TextRasterKey.room`).
    func textRasterRoom(_ box: CGRect) -> CGRect { bounds.offsetBy(dx: -box.minX, dy: -box.minY) }
    /// Whether the pixels shown are exactly `key`'s (`TextRasterKey.serves`).
    func textRasterServes(_ key: TextRasterKey) -> Bool {
        guard textRasterReady, let shown = textRasterKey else { return false }
        return shown.serves(key, ink: textMore.rasterInk.ink, shadows: textMore.rasterInk.shadows)
    }
    /// Takes `key` for the pixels shown, which serve it: they answer for its
    /// room too (`placeTextRaster`).
    func adoptTextRaster(_ key: TextRasterKey) {
        textRasterKey = key
        textMore.rasterRoom = key.room ?? key.box
        placeTextRaster()
    }
    func showTextRaster(_ result: TextRasterImage?, for key: TextRasterKey) {
        guard presenter?.views[id] === self, textRasterKey == key, !textRasterReady else { return }
        guard let result else {
            dropTextRaster()
            textRasterKey = key; textRasterFailed = true
            // Retrying display through UIKit supplies the allocation fallback.
            setNeedsDisplay()
            return
        }
        let diagnostic = TextRenderMetrics.shared.begin()
        let coverage: TextCoverage?
        var reusedCoverage = false
        if let drawing = result.drawing {
            if let held = drawing.liveCoverage {
                coverage = held
                reusedCoverage = true
            } else if let old = textRasterLayer?.delegate as? CoverageImageView,
               let held = old.coverage?.reusing(for: drawing) {
                coverage = held
                reusedCoverage = true
            } else if case .coverage(let prepared) = result.content {
                coverage = prepared
            } else {
                let paint = TextRenderMetrics.shared.begin(), phase = HostPhases.shared.begin(.text)
                coverage = drawing.coverage()
                HostPhases.shared.end(phase)
                TextRenderMetrics.shared.end(paint, kind: .coveragePaint, node: id,
                    width: drawing.pixelWidth, height: drawing.pixelHeight, bytes: coverage?.bytes ?? 0)
            }
            guard coverage != nil else { showTextRaster(nil, for: key); return }
            assert(coverage?.drawing === drawing)
            drawing.liveCoverage = coverage
        } else if case .coverage(let held) = result.content { coverage = held }
        else { coverage = nil }
        let shown = textRasterSpec
        textRaster = result.drawing.map(TextInk.drawn) ?? result.content
        textRasterScale = key.scale; textRasterSpec = key.spec
        textMore.rasterCovered = result.covered; textMore.rasterBand = key.clip != nil; textMore.rasterInk = (result.ink, result.shadows)
        textMore.rasterRoom = key.room ?? key.box
        textRasterReady = true; textRasterFailed = false
        if (coverage != nil) != (textRasterLayer?.delegate is CoverageImageView), let old = textRasterLayer {
            old.dropTextCast()
            if let view = old.delegate as? CoverageImageView { view.removeFromSuperview() } else { old.removeFromSuperlayer() }
            textRasterLayer = nil
        }
        let ink: CALayer
        let imageView: CoverageImageView?
        if coverage != nil {
            let view = (textRasterLayer?.delegate as? CoverageImageView) ?? CoverageImageView()
            imageView = view; ink = view.layer
        } else {
            imageView = nil; ink = textRasterLayer ?? InkLayer()
        }
        hostText(ink)
        // The box follows UIKit's resize; the cropped ink rectangle changes atomically with its image
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        ink.frame = result.frame
        ink.contentsScale = key.scale
        CATransaction.commit()
        textBoxLayer?.show(from: shown, to: key.spec)
        let roll = NumeralRoll.roll(ink, node: self)
        UIView.performWithoutAnimation {
            if let coverage, let imageView {
                imageView.show(coverage, scale: key.scale)
            } else if case .pixels(let pixels) = result.content { ink.contents = pixels }
        }
        TextShadowLayer.apply(key.spec.hdrShadow == nil ? key.spec.shadow : nil, to: ink)
        ink.applyTextRange(headroom: result.headroom, limit: style["dynamic_range_limit"]?.string)
        ink.applyTextCast(result.cast, headroom: result.castHeadroom, limit: style["dynamic_range_limit"]?.string, rolling: roll)
        textRasterLayer = ink
        presenter?.boxFilters.changed(self)
        switch result.content {
        case .pixels(let pixels):
            TextRenderMetrics.shared.end(diagnostic, kind: .publishPixels, node: id, width: pixels.width, height: pixels.height,
                bytes: pixels.bytesPerRow * pixels.height, detail: Int(result.diagnosticFallback))
        case .drawn, .coverage:
            TextRenderMetrics.shared.end(diagnostic, kind: .publishSingleColor, node: id,
                width: coverage?.mask.width ?? 0, height: coverage?.mask.height ?? 0, bytes: coverage?.bytes ?? 0, detail: reusedCoverage ? 1 : 0)
        }
    }
    /// Release a raster still off-window a turn after any within-batch move; the next display
    /// repaints it when the view returns.
    func dropTextRasterOffWindow() {
        guard window == nil, textRaster != nil else { return }
        dropTextRaster()
        layer.setNeedsDisplay()
    }

    func dropTextRaster() {
        textRasterLayer?.dropTextCast()
        if let view = textRasterLayer?.delegate as? CoverageImageView { view.removeFromSuperview() }
        else { textRasterLayer?.removeFromSuperlayer() }
        textRasterLayer = nil
        removeTextBox()
        textRaster = nil; textRasterKey = nil; textRasterReady = false; textRasterFailed = false; textRasterSpec = nil
    }

    /// Only row recycling keeps the empty UIKit hierarchy; eviction and off-window release still drop it.
    func parkTextRaster() {
        guard let ink = textRasterLayer, let view = ink.delegate as? CoverageImageView else { return dropTextRaster() }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        textBoxLayer?.reset()
        ink.removeAllAnimations()
        NumeralRoll.forget(ink)
        ink.dropTextCast()
        TextShadowLayer.apply(nil, to: ink)
        ink.applyTextRange(headroom: 0, limit: nil)
        view.clear()
        textRaster = nil; textRasterKey = nil; textRasterReady = false; textRasterFailed = false; textRasterSpec = nil
        textMore.rasterCovered = .null; textMore.rasterRoom = .null; textMore.rasterBand = false
        textMore.rasterInk = (.null, .null); textMore.rasterFrame = .zero; textMore.rasterScale = 1
        mountTextBox()
        CATransaction.commit()
        presenter?.boxFilters.changed(self)
    }

    /// The paragraph's ink in the lines layer inside its text's box
    /// (`makeTextBox`, `placeTextRaster`), the box made now if it has none.
    /// The box is inside a vibrancy view when the ink draws in a system colour
    /// over a blur (`VibrancyIOS.swift`).
    func hostText(_ ink: CALayer) {
        let made = textBoxLayer == nil
        if made { CATransaction.begin(); CATransaction.setDisableActions(true) }
        defer { if made { CATransaction.commit() } }
        let box = textBoxLayer ?? makeTextBox()
        if let view = ink.delegate as? CoverageImageView, let lines = textMore.rasterView?.lines {
            if view.superview !== lines { lines.addSubview(view) }
        } else if let lines = box.lines, ink.superlayer !== lines { lines.addSublayer(ink) }
        textRasterLayer = ink
        if box.superlayer == nil, syncVibrancy() == nil { mountTextBox() }
        placeTextRaster()
    }

    /// The box, placed and sized as the paragraph's content box, moving and
    /// resizing in UIKit's animation (`BoxPartLayer`), with its lines at its
    /// origin, which never move in it. A separate lines layer keeps fade
    /// snapshots in box-local coordinates while the box moves and resizes;
    /// a transition on the box can retain its pre-resize geometry.
    private func makeTextBox() -> TextBoxLayer {
        let view = TextBoxView(), box = view.box
        textMore.rasterView = view
        textBoxLayer = box
        return box
    }

    /// The text's box above the box's border (`applyBoxLayer`), under everything else.
    func mountTextBox() {
        if let view = specs?.rasterView {
            if view.superview !== self { insertSubview(view, at: 0) }
            insertBoxSublayer(view.layer)
        }
    }

    func removeTextBox() {
        if let view = specs?.rasterView { view.removeFromSuperview() } else { textBoxLayer?.removeFromSuperlayer() }
        specs?.rasterView = nil
        textBoxLayer = nil
    }

    /// Where the paragraph's box is drawn in this view: its content box
    /// placed on the device's pixel grid as a box of its own is
    /// (`PixelGrid.place`, from this view's absolute origin), where a label's
    /// frame lies, so its glyphs are as crisp as the box's own edges.
    func textPlace(scale: CGFloat) -> CGPoint {
        let origin = grid?.origin ?? .zero
        return PixelGrid.place(paragraphBox(), in: origin, scale: origin.scale > 0 ? CGFloat(origin.scale) : scale).drawn.origin
    }

    /// The paragraph's pixels at its box (`textPlace`), as a label is at
    /// its frame: the text's box layer, the ink inside it where its pixels
    /// lie in the box. Moved inside an animation UIKit has open, the box
    /// moves with it (`BoxPartLayer`); what it holds changes only with the
    /// pixels (`showTextRaster`).
    func placeTextRaster() {
        guard let box = textBoxLayer, let specs else { return }
        let at = textPlace(scale: textRasterScale), size = paragraphBox().size
        if box.position != at { box.position = at }
        if box.bounds.size != size { box.bounds = CGRect(origin: .zero, size: size) }
        if let lines = specs.rasterView?.lines, lines.bounds.size != size { lines.frame = CGRect(origin: .zero, size: size) }
        // What the pixels answer for in this view: the room they were painted
        // for (the view's bounds then, `TextRasterKey.room`) and the ink past
        // it; a band, only its rows. Bounds that grew since are not theirs
        // until they are painted again or shown to serve them.
        let covered = specs.rasterCovered.offsetBy(dx: at.x, dy: at.y), room = specs.rasterRoom.offsetBy(dx: at.x, dy: at.y)
        let rows = specs.rasterBand ? CGRect(x: room.minX, y: covered.minY, width: room.width, height: covered.height) : room
        textRasterFrame = covered.isNull ? .zero : covered.union(rows)
    }
}

/// A paragraph's worker-rendered pixels (`showTextRaster`): its changes show
/// at once, as under `setDisableActions`, in whatever transaction is open.
class InkLayer: CALayer {
    override func action(forKey event: String) -> (any CAAction)? { nil }
}

/// One refresh's clip geometry: each ancestor's accumulated clips (at no
/// reach, at the next frames' travel, and at the lead's) and its offset in
/// the viewport's space, found once per ancestor rather than once per
/// paragraph beneath it. Valid only while no frame or offset changes.
final class TextClips {
    private struct Entry {
        /// Local-to-viewport translation; nil under a transform (convert).
        var shift: CGPoint?
        /// Clips at or above this view per reach, in viewport space; `.null` if hidden.
        var clips: [CGRect]
    }
    private var memo: [ObjectIdentifier: Entry] = [:]
    unowned let viewport: UIView
    /// No reach, the travel the next frames will show, and the lead's.
    let reaches: [CGFloat]
    var soon: CGFloat { reaches[1] }
    init(_ viewport: UIView, soon: CGFloat, reach: CGFloat) {
        self.viewport = viewport; reaches = [0, soon, reach]
    }

    private func entry(_ view: UIView?) -> Entry {
        guard let view else { return Entry(shift: nil, clips: reaches.map { _ in .infinite }) }
        if let known = memo[ObjectIdentifier(view)] { return known }
        var result = entry(view.superview)
        // The document's mount stands in the viewport's place (`DocumentMountIOS.swift`).
        result.shift = view === viewport || view is DocumentMount ? .zero : shift(view, above: result.shift)
        if view.isHidden || view.alpha == 0 {
            result.clips = reaches.map { _ in .null }
        } else if !result.clips[0].isNull && (view.clipsToBounds || view is UIWindow) {
            let box = rect(view.bounds, of: view, shift: result.shift)
            result.clips = zip(result.clips, reaches).map { $0.intersection(box.insetBy(dx: -$1, dy: -$1)) }
        }
        memo[ObjectIdentifier(view)] = result
        return result
    }
    private func rect(_ r: CGRect, of view: UIView, shift: CGPoint?) -> CGRect {
        shift.map { r.offsetBy(dx: $0.x, dy: $0.y) } ?? view.convert(r, to: viewport)
    }
    private func shift(_ view: UIView, above: CGPoint?) -> CGPoint? {
        guard let s = above, view.transform.isIdentity, CATransform3DIsIdentity(view.layer.transform) else { return nil }
        return CGPoint(x: s.x + view.frame.minX - view.bounds.minX, y: s.y + view.frame.minY - view.bounds.minY)
    }
    /// `node`'s bounds in viewport space.
    func frame(_ node: NodeView) -> CGRect {
        rect(node.bounds, of: node, shift: shift(node, above: entry(node.superview).shift))
    }
    /// What `Presenter.textBand` finds by walking, for this reach or none;
    /// nil for any other reach, and outside the viewport or under a
    /// transform, where clips meet as their bounding boxes in the
    /// viewport's space (a rotated parent's covers a corner its own bounds
    /// do not) and only the walk is exact.
    func band(_ node: NodeView, reach: CGFloat) -> CGRect? {
        guard let index = reaches.firstIndex(of: reach) else { return nil }
        if node.isHidden || node.alpha == 0 { return .zero }
        let above = entry(node.superview)
        let clip = above.clips[index]
        guard !clip.isNull else { return .zero }
        if clip.isInfinite { return node.bounds }
        guard let own = shift(node, above: above.shift) else { return nil }
        let band = rect(node.bounds, of: node, shift: own).intersection(clip)
        guard !band.isNull else { return .zero }
        return band.offsetBy(dx: -own.x, dy: -own.y).intersection(node.bounds)
    }
}

extension Presenter {
    /// Every clipping ancestor participates; an inner scroller can itself sit
    /// outside an outer viewport. All geometry stays in the paragraph's space.
    private func textBand(_ node: NodeView, reach: CGFloat) -> CGRect {
        guard node.window != nil else { return .zero }
        if let band = textClips?.band(node, reach: reach) { return band }
        var result = node.bounds
        var ancestor: UIView? = node
        while let view = ancestor {
            if view.isHidden || view.alpha == 0 { return .zero }
            if view !== node && (view.clipsToBounds || view is UIWindow) {
                result = result.intersection(node.convert(view.bounds, from: view).insetBy(dx: -reach, dy: -reach))
            }
            ancestor = view.superview
        }
        return result.isNull ? .zero : result
    }
    func textScrollportRect(_ node: NodeView) -> CGRect { textBand(node, reach: 0) }
    /// What a refresh paints now rather than on a worker: what shows, and
    /// during a refresh what the next frames' travel will show.
    func textUrgentRect(_ node: NodeView) -> CGRect { textBand(node, reach: textClips?.soon ?? 0) }
    func textIsVisible(_ node: NodeView) -> Bool { !textScrollportRect(node).isEmpty }
    func textPreparationRect(_ node: NodeView) -> CGRect {
        let visible = textScrollportRect(node)
        return visible.isEmpty ? textBand(node, reach: viewport.bounds.height) : visible
    }

    /// Before the frame commits (the scroll callback, a rescue): a paragraph
    /// that shows without pixels for what shows gets them now, from its
    /// worker if that has finished or is running, else painted here — the
    /// reader never sees it blank (LLP 1050.000 D1). A settled paragraph
    /// costs one flag; the rest, their frame against the port. True while
    /// some paragraph lacks pixels: the pump's refresh has work then.
    @discardableResult
    func paintVisibleText() -> Bool {
        guard !applying else { return true }
        let port = viewport.bounds
        let culls = viewport.clipsToBounds
        let presentations = surfaces(includingRetiring: true).filter { $0.kind != .document }
        func presented(_ node: UIView) -> Bool { presentations.contains { node.isDescendant(of: $0.view) } }
        var clips: TextClips?
        defer { textClips = nil }
        var owed = false
        // A paragraph that never rasters (it truncates as it paints, say)
        // is out before its geometry, which is most of a scan's cost.
        func unsettled(_ node: NodeView) -> Bool { !node.textRasterSettled && !node.bounds.isEmpty && node.canRasterText }
        textIndex.visit(textViews, reach: 0, viewport: viewport) { node in
            guard unsettled(node) else { return true }
            owed = true
            let c = clips ?? TextClips(viewport, soon: 0, reach: 0)
            clips = c; textClips = c
            if culls && !presented(node) && !c.frame(node).intersects(port) { return true }
            let visible = textScrollportRect(node)
            guard !visible.isEmpty, node.textRaster == nil || !node.textRasterFrame.contains(visible) else { return true }
            textRasters.ensure(node, urgent: true)
            return true
        }
        // The refresh has work while a paragraph within its least lead
        // (a viewport) lacks pixels; one farther waits until it is nearer.
        if !owed { textIndex.visit(textViews, reach: port.height, viewport: viewport) { node in owed = unsettled(node); return !owed } }
        return owed
    }

    /// Paint motion's colour reaches the screen with its value (LLP 1062
    /// D6): a paragraph whose presented colour a batch changed is painted
    /// as the batch ends, where it shows, not on a worker a frame later.
    /// The paragraphs a batch gave new words (`changedText`) show them in
    /// the batch's frame, where they show (LLP 1116.005 D4), as UIKit draws a
    /// label's new text in the commit that sets it, never the old words a
    /// frame longer while a worker paints.
    func paintChangedText() {
        guard !changedText.isEmpty else { return }
        let ids = changedText
        changedText = []
        // Each clipping ancestor's geometry once, not once per paragraph
        // (a log's hundred rows, each given its neighbour's words).
        textClips = TextClips(viewport, soon: 0, reach: 0)
        defer { textClips = nil }
        textRasters.ensureUrgent(ids.compactMap { id in
            guard let node = views[id], node.canRasterText, !node.bounds.isEmpty, textIsVisible(node) else { return nil }
            return node
        })
    }

    func paintPresentedText() {
        guard !presentedText.isEmpty else { return }
        let ids = presentedText
        presentedText = []
        textRasters.ensureUrgent(ids.compactMap { id in
            guard let node = views[id], node.canRasterText, !node.bounds.isEmpty, textIsVisible(node) else { return nil }
            return node
        }, now: true)
    }

    /// Called after layout/scroll returns. What shows without pixels is
    /// painted (as `paintVisibleText`); built paragraphs within the lead get
    /// worker jobs, nearest first, what the next frames of travel
    /// (`velocity`, points a second, over `interval`) will show before the
    /// rest. At speed, what the list has carried past gets none, and its
    /// unstarted jobs are dropped. Each ancestor's clip is found once
    /// (`TextClips`), each paragraph's distance from the viewport once.
    /// True while jobs wait for room or time.
    @discardableResult
    func refreshVisibleText(deadline: TimeInterval? = nil, velocity: CGFloat = 0, interval: TimeInterval = 1.0 / 60) -> Bool {
        guard !applying else { return true }
        let port = viewport.bounds
        let soon = min(port.height, abs(velocity) * CGFloat(interval) * 2)
        // A quarter second of travel, at least a viewport.
        let reach = max(port.height, abs(velocity) / 4)
        let clips = TextClips(viewport, soon: 0, reach: reach)
        textClips = clips
        defer { textClips = nil }
        // More than two viewports a second: what is behind will not show.
        let fast = abs(velocity) > port.height * 2
        func passed(_ r: CGRect) -> Bool { fast && (velocity > 0 ? r.maxY <= port.minY : r.minY >= port.maxY) }

        // The viewport clips its content: a paragraph farther than the reach
        // from it has no band, so its distance alone rules it out.
        let culls = viewport.clipsToBounds
        let presentations = surfaces(includingRetiring: true).filter { $0.kind != .document }
        func presented(_ node: UIView) -> Bool { presentations.contains { node.isDescendant(of: $0.view) } }
        if textRasters.inFlight > 0 { textRasters.abandonStale { !presented($0) && passed(clips.frame($0)) } }
        var ranked: [(distance: CGFloat, node: NodeView)] = []
        textIndex.visit(textViews, reach: reach, viewport: viewport) { node in
            guard !node.textRasterSettled && !node.bounds.isEmpty && node.canRasterText else { return true }
            let r = clips.frame(node)
            let distance = presented(node) ? 0 : max(0, port.minY - r.maxY, r.minY - port.maxY)
            guard !(culls && distance > reach), (presented(node) || !passed(r)), !textBand(node, reach: reach).isEmpty else { return true }
            ranked.append((distance, node))
            return true
        }
        ranked.sort { $0.distance == $1.distance ? $0.node.id < $1.node.id : $0.distance < $1.distance }
        var deferred = false
        for (distance, node) in ranked {
            let visible = textScrollportRect(node)
            if !visible.isEmpty && (node.textRaster == nil || !node.textRasterFrame.contains(visible)) {
                textRasters.ensure(node, urgent: true)
                continue
            }
            guard !node.textRasterSettled else { continue }
            if !textRasters.hasRoom || deadline.map({ CACurrentMediaTime() >= $0 }) == true { deferred = true; continue }
            let priority: Operation.QueuePriority = distance == 0 ? .veryHigh : distance <= soon ? .high : .normal
            if !textRasters.ensure(node, urgent: false, priority: priority) { deferred = true }
        }
        return deferred
    }
}
#endif
