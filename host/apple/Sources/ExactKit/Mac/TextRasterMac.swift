// Paragraph text rasterized off the main thread. @ref LLP 1044 F4, LLP 1008 §3
//
// AppKit paints a layer-backed view only where it is visible, so a paragraph
// scrolling into view is painted in strips: a backing buffer, a display-list
// replay and a texture upload per paragraph per frame, on the main thread, in
// the frame that has to move — half of what that thread did during a scroll.
// Here a paragraph's text is drawn once, whole, on a worker, into a surface the
// view's layer shows. The main thread mounts the row; the pixels arrive behind it,
// while the row is still a screen away. Scrolling mounted text is compositing.
//
// `NodeView.draw` still paints text for everything this declines: a
// selection, a capture, a decorated text box, a paragraph
// taller than a screen, and one too small to repay a surface (unless HDR).
#if os(macOS)
import AppKit
import CoreText
import IOSurface

final class TextRasterizer {
    /// Points — about a screenful. A taller paragraph is never seen whole, and a
    /// bitmap is all or nothing: a 245-line code block was one 33 MB surface,
    /// more than every other text pixel in the document together. AppKit's
    /// strips back only what is on screen.
    static let maxHeight: CGFloat = 1024
    /// Pixels. A surface has a fixed cost — an IOSurface, and its registration
    /// with the render server when a layer first shows it — that a few glyphs
    /// never repay: a table of 32 narrow cells made one per cell as each row
    /// mounted and scrolled at 31–35 fps, where AppKit drawing the cells into
    /// their layers' own backing stores made 51–58. Smaller text draws.
    static let minPixels: CGFloat = 16384
    // Keep italic overhang and ink outside tight line boxes, but never size
    // a surface to an unbreakable line's potentially unbounded advance.
    static let maxInkOverflow = TextRasterJob.maxInkOverflow

    // A slice limits admission rate, not outstanding work. Keep no backlog:
    // the next pump tries the still-nearby paragraphs again when a worker is
    // free, so navigation and resize cannot queue obsolete document pixels.
    private var active = 0
    private static let maxConcurrent = 2
    /// A freed slot resumes admission even when its pixels were discarded.
    var slotFreed: (() -> Void)?

    /// Completed surfaces are immutable and may back several layers. Keep only
    /// small paragraphs, charging their IOSurface allocations and key text.
    /// Ordinary rasters start at 64 KiB of pixels; 256 KiB leaves room for labels
    /// and allocation alignment without retaining whole document paragraphs.
    private struct Kept {
        let image: TextRasterImage
        let bytes: Int
        var used: UInt64
    }
    private var kept: [TextRasterKey: Kept] = [:]
    private var keptBytes = 0
    private var keptClock: UInt64 = 0
    private weak var keptEngine: TextEngine?
    private var keptNamespace = -1
    private static let keptLimit = 4 * 1024 * 1024
    private static let keptEntryLimit = 256 * 1024

    private func keptImage(_ key: TextRasterKey, engine: TextEngine) -> TextRasterImage? {
        if keptEngine !== engine || keptNamespace != engine.namespace {
            kept.removeAll(); keptBytes = 0
            keptEngine = engine; keptNamespace = engine.namespace
        }
        guard var hit = kept[key] else { return nil }
        keptClock += 1
        hit.used = keptClock; kept[key] = hit
        return hit.image
    }

    private func keep(_ image: TextRasterImage?, for key: TextRasterKey, engine: TextEngine, namespace: Int) -> TextRasterImage? {
        // A late worker cannot repopulate an obsolete font catalog or line scale.
        guard let image, keptEngine === engine, namespace == keptNamespace, namespace == engine.namespace else { return image }
        if let shared = keptImage(key, engine: engine) { return shared }
        let text = key.spec.runs.reduce(0) { $0 + $1.text.utf8.count }
        let bytes = image.surface.allocationSize + (image.cast?.allocationSize ?? 0) + text
        guard bytes <= Self.keptEntryLimit else { return image }
        keptClock += 1
        kept[key] = Kept(image: image, bytes: bytes, used: keptClock)
        keptBytes += bytes
        if keptBytes > Self.keptLimit {
            for (key, entry) in kept.sorted(by: { $0.value.used < $1.value.used }) {
                kept.removeValue(forKey: key); keptBytes -= entry.bytes
                if keptBytes <= Self.keptLimit * 3 / 4 { break }
            }
        }
        return image
    }

    private func begin(_ node: NodeView, key: TextRasterKey) {
        node.textRasterKey = key
        node.textRasterReady = false
        node.textRasterFailed = false
        node.textRasterPending = false
    }

    private func reuse(_ node: NodeView, key: TextRasterKey, deferOffscreen: Bool = false) -> Bool {
        guard let engine = node.text, let image = keptImage(key, engine: engine) else { return false }
        begin(node, key: key)
        node.showTextRaster(image.surface, for: key, frame: image.frame, cast: image.cast, deferOffscreen: deferOffscreen)
        return true
    }

    private func key(_ node: NodeView) -> TextRasterKey {
        TextRasterKey(spec: node.paragraphSpec(), size: node.bounds.size,
                      box: node.paragraphBox(), scale: node.pixelScale ?? 2)
    }

    private func prepare(_ node: NodeView, key: TextRasterKey) -> TextRasterJob? {
        guard let engine = node.text else { return nil }
        let measured = engine.measuredBreaks(key.spec, width: key.box.width)
        let paragraph = measured == nil ? node.paragraphLayout() : nil
        guard let geometry = measured ?? paragraph.map(LineGeometry.init) else { return nil }
        begin(node, key: key)
        let source = paragraph?.shape?.attributed ?? engine.attributed(key.spec)
        return TextRasterJob(source: source.copy() as! NSAttributedString,
                   ranges: geometry.ranges, baselines: geometry.baselines,
                   flush: key.spec.align == 1 ? 0.5 : key.spec.align == 2 ? 1 : 0, whiteSpace: key.spec.whiteSpace, justifies: key.spec.align == 3, insets: paragraph?.insets ?? LineInsets(key.spec, source: source),
                   box: key.box, size: key.size, scale: key.scale, lineScale: engine.lineScale, ellipsis: key.spec.ellipsis, clamped: geometry.clamped,
                   shadow: key.spec.hdrShadow.map(TextRunShadow.init), antialiased: key.spec.antialiased,
                   measuredGlyphs: key.spec.color.count == 4 ? engine.measuredGlyphs(key.spec, width: key.box.width) : nil,
                   measuredColor: TextEngine.color(key.spec.color).cgColor)
    }

    /// Only first pixels may rasterize synchronously. A replacement uses the
    /// visible batch below, or a worker if an AppKit display pass gets here first.
    @discardableResult
    func ensure(_ node: NodeView, urgent: Bool) -> Bool {
        if node.textRasterFailed, node.textRasterKey != nil { return true }
        guard node.rastersText else { return true }
        let key = key(node)
        if node.textRasterKey == key, node.textRasterReady || !urgent || node.textRaster != nil {
            if node.textRasterReady && node.textRasterPending { node.presentTextRaster() }
            return true
        }
        if reuse(node, key: key, deferOffscreen: !urgent) { return true }
        guard let engine = node.text else { node.dropTextRaster(); return true }
        let namespace = engine.namespace
        let firstPixels = urgent && node.textRaster == nil && !node.textRasterUsesStrips
        guard firstPixels || active < Self.maxConcurrent else { return false }
        guard let job = prepare(node, key: key) else { node.dropTextRaster(); return true }
        if firstPixels {
            let lines = job.measuredGlyphs == nil ? node.text?.rasterLines(key.spec, ranges: job.ranges, width: key.box.width).1 : nil
            let image = keep(Self.render(job, firstPixels: true, lines: lines), for: key, engine: engine, namespace: namespace)
            node.showTextRaster(image?.surface, for: key, frame: image?.frame, cast: image?.cast)
            return true
        }
        if urgent { node.useTextStrips() }
        active += 1
        RegionTextExecutor.queue.addOperation { [weak self, weak node] in
            let image = Self.render(job)
            DispatchQueue.main.async { [weak self, weak node, weak engine] in
                self?.active -= 1
                if let self, let engine, engine.namespace == namespace {
                    let image = self.keep(image, for: key, engine: engine, namespace: namespace)
                    if node?.text === engine {
                        node?.showTextRaster(image?.surface, for: key, frame: image?.frame, cast: image?.cast, deferOffscreen: true)
                    }
                } else if let node, node.textRasterKey == key, !node.textRasterReady {
                    node.invalidateText()
                }
                self?.slotFreed?()
            }
        }
        return true
    }

    // One visible replacement set at a time, on the existing raster workers.
    // The main thread reads results only after both workers leave the group.
    private final class Replacement {
        let group = DispatchGroup()
        let lock = NSLock()
        var images: [Int: TextRasterImage] = [:]
        var published = false
        func put(_ image: TextRasterImage?, at index: Int) {
            lock.lock(); defer { lock.unlock() }
            images[index] = image
        }
    }
    private var replacing = false

    private struct Paint {
        let node: NodeView
        let key: TextRasterKey
        let job: TextRasterJob
        let engine: TextEngine
        let namespace: Int
    }

    /// New geometry and its pixels become visible in one transaction. Spend
    /// at most a quarter frame waiting; on timeout AppKit draws the new-width
    /// visible strips in this display pass. No stale line breaks cross a frame.
    /// A live resize skips full rasters when at least half their pixels are hidden.
    @discardableResult
    func replaceVisible(_ nodes: [NodeView], wait: TimeInterval) -> Bool {
        var jobs: [Paint] = []
        var deferred = false
        // Only a paragraph that has pixels to replace, asked first: most near the port are drawn by AppKit, and
        // `canRasterText` is the costly question.
        for node in nodes where (node.textRaster != nil || node.textRasterUsesStrips) && node.needsTextRaster && node.canRasterText {
            let key = key(node)
            if node.textRasterKey == key {
                if node.textRasterReady && node.textRasterPending { node.presentTextRaster() }
                continue
            }
            let visible = node.presenter?.textIsVisible(node) == true
            guard visible else { continue }
            if reuse(node, key: key) { continue }
            let port = node.presenter?.textScrollportRect(node) ?? node.visibleRect
            if replacing || active > 0 || (node.inLiveResize && port.height < node.bounds.height * 0.5) {
                node.useTextStrips()
                // Busy workers request the next pass when they finish.
                // Occluded live-resize text still needs a later frame.
                deferred = deferred || active == 0
                continue
            }
            if let engine = node.text, let job = prepare(node, key: key) {
                jobs.append(Paint(node: node, key: key, job: job, engine: engine, namespace: engine.namespace))
            }
        }
        guard !jobs.isEmpty else { return deferred }
        replacing = true
        let result = Replacement()
        let work = jobs
        let workers = min(Self.maxConcurrent, jobs.count)
        active += workers
        // Two bounded lanes, no job per paragraph queued behind old widths.
        for lane in 0..<workers {
            result.group.enter()
            RegionTextExecutor.queue.addOperation {
                for index in stride(from: lane, to: work.count, by: workers) {
                    result.put(Self.render(work[index].job), at: index)
                }
                result.group.leave()
            }
        }
        let publish = { [weak self] in
            guard !result.published else { return }
            result.published = true
            self?.active -= workers
            self?.replacing = false
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            for (index, entry) in jobs.enumerated() {
                guard entry.node.text === entry.engine, entry.engine.namespace == entry.namespace else {
                    if entry.node.textRasterKey == entry.key, !entry.node.textRasterReady { entry.node.invalidateText() }
                    continue
                }
                let image = self?.keep(result.images[index], for: entry.key, engine: entry.engine, namespace: entry.namespace)
                entry.node.showTextRaster(image?.surface, for: entry.key, frame: image?.frame, cast: image?.cast)
            }
            CATransaction.commit()
            self?.slotFreed?()
        }
        if result.group.wait(timeout: .now() + max(0, wait)) == .success {
            publish()
        } else {
            for entry in jobs { entry.node.useTextStrips() }
            result.group.notify(queue: .main, execute: publish)
        }
        return deferred
    }

    /// The same paint `TextEngine.draw` makes — a y-down context, one
    /// `CTLineDraw` per line, baselines rounded to points — into an sRGB
    /// IOSurface. A surface is what the render server composites: a CGImage
    /// would be converted and copied for it on the main thread, at commit.
    /// Workers create their own lines from source and ranges.
    private static func render(_ job: TextRasterJob, firstPixels: Bool = false, lines: [CTLine]? = nil) -> TextRasterImage? {
        assert(!Thread.isMainThread || firstPixels, "replacement rasterization belongs to workers")
        return job.render(lines: lines)
    }
}

extension NodeView {
    /// Every canvas's overlay (`NodeView.init`): with none, no view is
    /// painted through a canvas, and `canvasAbove`, which the text pass asks
    /// of each paragraph near the port, takes no walk to the window.
    static let canvasOverlays = NSHashTable<FlippedView>.weakObjects()
    /// The canvas this node is painted through, if any: the nearest canvas
    /// above whose overlay holds it.
    package var canvasAbove: NodeView? {
        guard Self.canvasOverlays.count > 0 else { return nil }
        var v: NSView = self
        while let s = v.superview {
            if let c = s as? NodeView, c.overlay === v { return c }
            v = s
        }
        return nil
    }

    /// Whether this paragraph's text is a rasterized surface rather than
    /// something `draw` paints. Asked by AppKit through `wantsUpdateLayer`.
    var rastersText: Bool { canRasterText && !textRasterUsesStrips }

    var canRasterText: Bool {
        if textRasterFailed, textRasterKey != nil { return false }
        // Cheapest first: this is asked of every visible paragraph on every
        // text refresh, and the ancestor walk and the paragraph's spec are
        // the costly questions.
        // `text-overflow: ellipsis` truncates in the raster job, as `draw(_:)`
        // does, and a `line-clamp`'s last line is made again from the range
        // it broke at (`LineGeometry.clamped`), as on iOS (873cec46e): a
        // clamped or ellipsized label no longer paints on the main thread.
        guard kind == "text", isParagraph, flowShapes.isEmpty, columnRecord == nil, !hasBoxPaint, !Capture.capturing, backgroundClip != "text",
              bounds.width > 0, bounds.height > 0, bounds.height <= TextRasterizer.maxHeight,
              window != nil, readerParagraph == nil, let presenter else { return false }
        // Canvas snapshots must share our surfaces, not AppKit's backing
        // stores: borrowing those can blank the live label after scrolling.
        guard !textIsSmall || canvasAbove != nil else { return false }
        if presenter.selection.isActive, let selected = presenter.selection.range(self), selected.length > 0 { return false }
        if presenter.session?.regions.owns(self) == true { return false }
        return !paintsVisibleInlineRun
    }

    /// Too few pixels to repay a surface. Such text draws whole rather than in
    /// bands admitted as it scrolls: bands, and their visit per frame, are for
    /// paragraphs too large to paint at once. HDR text is never small: only a
    /// raster's layers follow `dynamic-range-limit` (LLP 1100 D8).
    var textIsSmall: Bool {
        // The page's scale (the window's backing, the page zoom), not
        // `pixelScale`: this is a threshold asked of every visible paragraph
        // on every text refresh, where the walk to the window that
        // `pixelScale` takes was the costliest question.
        let scale = window.map { $0.backingScaleFactor * (presenter?.viewport.magnification ?? 1) } ?? 2
        guard bounds.width * bounds.height * scale * scale < TextRasterizer.minPixels else { return false }
        return !(isParagraph && paragraphHeadroom(drawing: true) > 1)
    }

    /// Whether the pump still owes this paragraph pixels.
    var needsTextRaster: Bool {
        if textRasterFailed, textRasterKey != nil { return false }
        return !textRasterReady || textRasterKey == nil || textRasterPending
    }

    /// The box a raster was painted for is gone — the layer would stretch its
    /// surface to whatever the paragraph is now. Retire the key so the pump
    /// asks a worker for pixels at the new geometry. AppKit's own dirty flag
    /// cannot be that record: a paragraph resized while it is off screen is
    /// not redrawn there, and an `ensure` that finds every worker busy
    /// declines after `updateLayer` has already cleared the flag. The old
    /// pixels stay up until the new ones replace them, as `invalidateText`
    /// leaves them for a changed paragraph.
    func textRasterGeometryChanged() {
        // A worker will replace accepted pixels only while this node remains
        // eligible. Crossing to draw must retire both the surface and any
        // overflow/shadow sublayer before AppKit chooses its display path.
        if textRaster != nil, !canRasterText {
            dropTextRaster()
            needsDisplay = true
            return
        }
        if let key = textRasterKey, key.size == bounds.size && key.box == paragraphBox() { return }
        textRasterKey = nil
        textRasterPending = false
        // Position the accepted surface at its original dimensions immediately;
        // the node's new frame must never stretch old glyphs.
        if textRaster != nil { presentTextRaster() }
    }

    func useTextStrips() {
        textRasterUsesStrips = true
        needsDisplay = true
    }

    func showTextRaster(_ image: IOSurface?, for key: TextRasterKey, frame: CGRect? = nil, cast: IOSurface? = nil, deferOffscreen: Bool = false) {
        // An urgent paint can overtake its worker. Keep the accepted surface
        // instead of committing identical pixels again when that worker ends.
        guard textRasterKey == key, !textRasterReady else { return }
        guard let image else {
            dropTextRaster()
            textRasterKey = key
            textRasterFailed = true
            needsDisplay = true
            // A synchronous failure can occur inside updateLayer, whose dirty
            // flag AppKit is about to clear. Ask again after that display pass,
            // now through draw. A late successful worker can still replace it.
            DispatchQueue.main.async { [weak self] in
                guard let self, self.textRasterFailed, self.textRasterKey == key else { return }
                self.needsDisplay = true
            }
            return
        }
        textRasterFailed = false
        textRasterUsesStrips = false
        textRaster = image
        textRasterCast = cast
        textRasterScale = key.scale
        textRasterFrame = frame ?? CGRect(origin: .zero, size: key.size)
        textRasterReady = true
        guard rastersText else { return }
        if deferOffscreen, let presenter, !presenter.textIsVisible(self) {
            textRasterPending = true
            presenter.requestTextPublication()
        } else { presentTextRaster() }
    }

    /// Fitting ink uses the view's contents. Overflow ink needs a positioned
    /// sublayer so it can escape the layout box, subject to authored clipping.
    func presentTextRaster() {
        guard !textRasterUsesStrips, let layer, let surface = textRaster else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        // A `text-shadow` is cast by a sublayer of its own: the view's layer
        // would cast its box too (LLP 1077 D3).
        let shadow = textRasterKey.flatMap { $0.spec.hdrShadow == nil ? $0.spec.shadow : nil }
        // HDR ink too needs its own layer, for its range (LLP 1100 D8).
        let headroom = TextRasterJob.headroom(of: surface)
        if textRasterFrame == CGRect(origin: .zero, size: bounds.size), shadow == nil, headroom <= 1, textRasterCast == nil {
            textRasterOverflowLayer?.dropTextCast(); textRasterOverflowLayer?.removeFromSuperlayer()
            textRasterOverflowLayer = nil
            layer.contentsScale = textRasterScale
            layer.contentsGravity = .resize
            layer.contents = surface
            layer.applyTextRange(headroom: headroom, limit: style["dynamic_range_limit"]?.string)
        } else {
            layer.contents = nil
            layer.applyTextRange(headroom: 0, limit: style["dynamic_range_limit"]?.string)
            let ink = textRasterOverflowLayer ?? CALayer()
            if ink.superlayer == nil { layer.addSublayer(ink) }
            textRasterOverflowLayer = ink
            ink.frame = textRasterFrame
            ink.contentsScale = textRasterScale
            ink.contentsGravity = .resize
            ink.contents = surface
            TextShadowLayer.apply(shadow, to: ink)
            ink.applyTextRange(headroom: headroom, limit: style["dynamic_range_limit"]?.string)
            ink.applyTextCast(textRasterCast, headroom: textRasterCast.map(TextRasterJob.headroom) ?? 0, limit: style["dynamic_range_limit"]?.string)
        }
        textRasterPending = false
        CATransaction.commit()
        presenter?.boxFilters.changed(self)
        repaintThrough()
    }

    func dropTextRaster() {
        textRasterOverflowLayer?.dropTextCast(); textRasterOverflowLayer?.removeFromSuperlayer()
        textRasterOverflowLayer = nil
        if textRaster != nil { layer?.contents = nil }
        layer?.setValue(nil, forKey: CALayer.textHeadroomKey)
        textRaster = nil; textRasterCast = nil
        textRasterKey = nil
        textRasterReady = false
        textRasterFailed = false
        textRasterPending = false
        textRasterUsesStrips = false
    }
}

nonisolated(unsafe) private var textRasterCastKey: UInt8 = 0

extension NodeView {
    /// An HDR `text-shadow`'s own surface (`TextRasterImage.cast`).
    var textRasterCast: IOSurface? {
        get { objc_getAssociatedObject(self, &textRasterCastKey) as? IOSurface }
        set { objc_setAssociatedObject(self, &textRasterCastKey, newValue, .OBJC_ASSOCIATION_RETAIN_NONATOMIC) }
    }
}
#endif
