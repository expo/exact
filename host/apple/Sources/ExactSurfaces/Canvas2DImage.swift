// @ref LLP 1056 D9 — Canvas 2D images and pixels on Apple, and the
// presenter's canvases. An image handle is the string an `image` node's
// `src` takes: an app asset, or an http(s) URL. The runner names the handles
// its draws asked for (the batch's `canvasImages`); the host decodes each
// off the main thread and answers `exact_canvas_image`, which redraws the
// canvases that asked. `putImageData` writes raw backing pixels: no
// transform, clip, alpha, compositing or shadow.
import CoreGraphics
import CoreText
import Foundation
import ImageIO
import QuartzCore
import ExactKit

/// No implicit animations: new pixels appear with the batch that drew them.
private final class Instant: NSObject, CALayerDelegate {
    static let shared = Instant()
    func action(for layer: CALayer, forKey event: String) -> CAAction? { NSNull() }
}

extension Canvas2DReplayer {
    /// `drawImage`: `image sx sy sw sh dx dy dw dh`, the source already
    /// clipped to the image; the destination in user space.
    func drawImage(_ c: CGContext, _ n: [Double]) {
        guard let src = imageSources[UInt32(n[0])], let image = env?.canvasImage(src) else { return }
        let (sx, sy, sw, sh) = (n[1], n[2], n[3], n[4])
        let dest = CGRect(x: n[5], y: n[6], width: n[7], height: n[8])
        guard sw > 0, sh > 0 else { return }
        let (kx, ky) = (dest.width / sw, dest.height / sh)
        let whole = CGRect(x: dest.minX - sx * kx, y: dest.minY - sy * ky,
                           width: CGFloat(image.width) * kx, height: CGFloat(image.height) * ky)
        render(c) { c in
            c.concatenate(state.author)
            c.clip(to: dest)
            // Upright in the y-down space.
            c.translateBy(x: 0, y: whole.minY + whole.maxY)
            c.scaleBy(x: 1, y: -1)
            c.draw(image, in: whole)
        }
    }

    /// `putImageData`: `x y w h` in backing pixels, then w × h RGBA pixels.
    func putImageData(_ c: CGContext, _ n: [Double], _ count: Int) {
        let (x, y, w, h) = (Int(n[0]), Int(n[1]), Int(n[2]), Int(n[3]))
        guard w > 0, h > 0, count >= 4 + w * h else { return }
        var bytes = [UInt8](repeating: 0, count: w * h * 4)
        for i in 0..<(w * h) {
            let v = UInt32(n[4 + i])
            bytes[i * 4] = UInt8(v >> 24); bytes[i * 4 + 1] = UInt8((v >> 16) & 0xff)
            bytes[i * 4 + 2] = UInt8((v >> 8) & 0xff); bytes[i * 4 + 3] = UInt8(v & 0xff)
        }
        guard let image = space.image(bytes, width: w, height: h) else { return }
        c.saveGState()
        c.resetClip()
        c.concatenate(c.ctm.inverted())
        c.setBlendMode(.copy)
        c.setAlpha(1)
        c.setShadow(offset: .zero, blur: 0, color: nil)
        c.interpolationQuality = .none
        c.draw(image, in: CGRect(x: x, y: height - y - h, width: w, height: h))
        c.restoreGState()
    }
}

/// A presenter's 2D canvases, by view id: the `canvas2d` op, the decoded
/// image handles they draw, and cleanup when a view goes. The bitmap shows
/// in a sublayer below the view's children, framed to the content box.
final class Canvas2DHost: Canvas2DEnv {
    /// The replayers, by view id: touched only on `replay`. A canvas that is
    /// recorded (`Canvas2DRecord.swift`) has none.
    private var replayers: [UInt32: Canvas2DReplayer] = [:]
    /// Each canvas's tracker and kept lists: touched only on `replay`.
    private var kept: [UInt32: Canvas2DKept] = [:]
    /// Each recorded canvas's recording layer, over its bitmap.
    private var recorders: [UInt32: Canvas2DRecordLayer] = [:]
    /// The canvases the GPU module draws (`Canvas2DGpu.swift`): touched only
    /// on `replay`.
    private var gpus: [UInt32: Canvas2DGpuCanvas] = [:]
    /// CanvasesHost whose fresh bitmap has had no lists yet (replay queue).
    private var undrawn: Set<UInt32> = []
    private var completed: [UInt32: UInt64] = [:]
    /// Tests (and the parity smoke, EXACT_CANVAS_RECORD=always) record every
    /// canvas the policy allows, animating or not.
    static var recordAlways = ProcessInfo.processInfo.environment["EXACT_CANVAS_RECORD"] == "always"
    private var layers: [UInt32: CALayer] = [:]
    /// Where each bitmap shows (`place`). A bitmap drawn at a new size before
    /// its view lays out is stretched into the old box.
    private var placements: [UInt32: (box: CGRect, radii: [CGSize])] = [:]
    /// Replay runs off the main thread, in order (LLP 1056 §8 stage 4, as
    /// built): a list row's canvas cost 130 ms/s of an iPhone's main thread
    /// in a fling. The row's box, mask and scale apply with its batch; its
    /// pixels land when the replay ends, a frame later at most, and the
    /// agent's settle waits for them (`loadingCount`).
    ///
    /// Between captures a canvas has at most one replay waiting behind the
    /// one running. Lists arriving while one waits join it, and a
    /// fresh bitmap's lists replace it, since a new bitmap discards what was
    /// drawn before. While one waits the runner holds the canvas's frame
    /// request (`onHeld`), as a browser's animation frame waits for the last
    /// one to present: an animation drops frames instead of queueing them,
    /// and the newest replay done always shows.
    #if DEBUG
    var beforeCaptureCopy: (() -> Void)?
    var beforeCaptureEncode: (() -> Void)?
    #endif
    private let replay: DispatchQueue
    init(replay: DispatchQueue = DispatchQueue(label: "exact.canvas2d.replay", qos: .userInitiated)) {
        self.replay = replay
    }
    /// Replays dispatched and not yet shown (`loadingCount`); each one's end,
    /// shown or dropped, tells `onReplayEnded`.
    private var pending = 0 { didSet { if pending < oldValue { onReplayEnded?() } } }
    /// A replay ended: what the session counts as outstanding may have cleared.
    var onReplayEnded: (() -> Void)?
    /// Each canvas's latest arrival, and the newest one shown: an older
    /// replay's pixels are never shown over a newer one's.
    private var sequence: [UInt32: Int] = [:]
    private var shown: [UInt32: Int] = [:]
    /// Sequence numbers run across every canvas and never restart, and each
    /// canvas remembers the first one of its present life: a replay still
    /// running when its view was retired never shows on the view that
    /// reuses its id (found by the GPU stress test, 2026-09-29).
    private var nextSeq = 0
    private var born: [UInt32: Int] = [:]
    /// Each canvas's lifetime as the main thread last applied it: a
    /// retirement removes only what that lifetime made on the replay queue,
    /// since a replay already queued may have made the next lifetime's
    /// canvas before the retirement's cleanup runs (found by the stress
    /// test).
    private var lifetimes: [UInt32: UInt64] = [:]
    /// Each canvas's replay not yet started, under `lock` (the replay queue
    /// takes it).
    private var waiting: [UInt32: Canvas2DJob] = [:]
    private let lock = NSLock()
    /// A canvas's replay is waiting (true) or has started (false): the
    /// runner holds its frame request meanwhile (`exact_canvas_held`).
    var onHeld: ((UInt32, Bool) -> Void)?
    /// Lists that could not be read, for `logs`.
    var errors: [String] = []
    /// The views' real scale differs from what the canvases were drawn at.
    var onScale: ((CGFloat) -> Void)?
    private var reported: CGFloat = 0
    /// The session's text engine (LLP 1056 D8).
    var textEngine: (() -> TextEngine?)?
    /// An app asset's bytes.
    var assetBytes: ((String) -> Data?)?
    /// A handle decoded (its image) or not (nil): the session tells the runtime.
    var onImage: ((String, CGImage?) -> Void)?
    private var images: [String: CGImage] = [:]
    private var loading: Set<String> = []

    func canvasFont(_ f: Canvas2DFont) -> CTFont? { textEngine?()?.canvasText.font(f) }
    func canvasImage(_ src: String) -> CGImage? { images[src] }
    /// Handles being decoded and replays not yet shown: the agent's settle
    /// waits for them.
    var loadingCount: Int { loading.count + pending }

    /// Decode the handles the runner asked for, off the main thread, within
    /// an `img`'s bounds: a remote body is fetched as one is, spooled to a
    /// file and stopped past its byte limit, and an image over its pixel
    /// limit is refused before it is decoded.
    func load(_ srcs: [String]) {
        for src in srcs where images[src] == nil && !loading.contains(src) {
            loading.insert(src)
            let remote = URL(string: src).flatMap { $0.scheme == "http" || $0.scheme == "https" ? $0 : nil }
            let local = remote == nil ? assetBytes?(src.hasPrefix("/") ? String(src.dropFirst()) : src) : nil
            DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                var bytes = local
                if let remote {
                    let done = DispatchSemaphore(value: 0)
                    RasterInput.fetch(remote, cancellation: RasterCancellation()) { result in
                        bytes = try? result.get().bytes()
                        done.signal()
                    }
                    done.wait()
                }
                let image = bytes.flatMap { CGImageSourceCreateWithData($0 as CFData, nil) }
                    .flatMap { source -> CGImage? in
                        guard let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
                              let w = properties[kCGImagePropertyPixelWidth] as? NSNumber,
                              let h = properties[kCGImagePropertyPixelHeight] as? NSNumber,
                              w.intValue > 0, h.intValue > 0,
                              w.intValue <= RasterMetadata.pixelLimit / h.intValue else { return nil }
                        // Full-size transformed pixels make both drawImage and
                        // the dimensions reported to the runner honor EXIF.
                        return CGImageSourceCreateThumbnailAtIndex(source, 0, [
                            kCGImageSourceCreateThumbnailFromImageAlways: true,
                            kCGImageSourceCreateThumbnailWithTransform: true,
                            kCGImageSourceThumbnailMaxPixelSize: max(w.intValue, h.intValue),
                            kCGImageSourceShouldCacheImmediately: true,
                        ] as CFDictionary)
                    }
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.loading.remove(src)
                    if let image { self.images[src] = image }
                    self.onImage?(src, image)
                }
            }
        }
    }

    func apply(_ id: UInt32, _ payload: [String: Any], layer parent: CALayer?) {
        guard let parent else { return }
        let num = { (key: String) -> Double in (payload[key] as? NSNumber)?.doubleValue ?? 0 }
        let lifetime = UInt64(num("lifetime")), generation = UInt32(num("generation"))
        lifetimes[id] = lifetime
        let fresh = (payload["fresh"] as? NSNumber)?.boolValue == true
        let (w, h, scale) = (Int(num("w")), Int(num("h")), num("scale"))
        let lists: [Data?] = payload["lists"] as? [Data] ?? []
        let layer = layers[id] ?? {
            let l = CALayer(); l.delegate = Instant.shared; l.contentsGravity = .resize
            l.magnificationFilter = .linear; l.minificationFilter = .linear
            layers[id] = l; return l
        }()
        if layer.superlayer !== parent {
            #if os(macOS)
            // Replaced content paints over its box: above the box's own
            // sublayers where AppKit's box is layers (`BoxLayerMac.swift`).
            if let node = parent.delegate as? NodeView, let top = node.boxSublayersTop { parent.insertSublayer(layer, above: top) }
            else { parent.insertSublayer(layer, at: 0) }
            #else
            parent.insertSublayer(layer, at: 0)
            #endif
        }
        if let placed = placements[id] { Self.place(layer, placed.box, placed.radii) }
        layer.contentsScale = max(1, scale)
        let actual = parent.contentsScale
        if (payload["stretch"] as? NSNumber)?.boolValue != true, actual >= 1, abs(actual - scale) > 0.01, actual != reported {
            reported = actual
            onScale?(actual)
        }
        // What the replay reads, resolved here: the decoded images, and the
        // fonts its text sets (the text engine is the main thread's).
        // Earlier fonts belong to the replayer's current and saved states.
        var fonts: [Canvas2DFont: CTFont] = [:]
        for case let data? in lists {
            for f in Canvas2DReplayer.fonts(in: data) where fonts[f] == nil { fonts[f] = canvasFont(f) }
        }
        nextSeq += 1
        let seq = nextSeq
        if sequence[id] == nil { born[id] = seq }
        sequence[id] = seq
        let job = Canvas2DJob(fresh: fresh, w: w, h: h, scale: scale, lifetime: lifetime, generation: generation,
                              lists: lists, images: images, fonts: fonts, seq: seq)
        job.drawing = (payload["seq"] as? NSNumber)?.uint64Value ?? 0
        // The draw asked for the next frame: it animates.
        job.animating = (payload["animating"] as? NSNumber)?.boolValue == true || Canvas2DHost.recordAlways
        job.stretch = (payload["stretch"] as? NSNumber)?.boolValue == true
        job.space = Canvas2DSpace(p3: (payload["p3"] as? NSNumber)?.boolValue == true, float16: (payload["float16"] as? NSNumber)?.boolValue == true)
        lock.lock()
        let joined = waiting[id]?.absorb(job) ?? false
        if !joined { waiting[id] = job }
        lock.unlock()
        Canvas2DStats.arrival(joined: joined)
        if joined { return }
        pending += 1
        onHeld?(id, true)
        replay.async { [weak self] in self?.run(id, job) }
    }

    /// A queued snapshot separates replay jobs on this serial queue.
    private func run(_ id: UInt32, _ job: Canvas2DJob) {
        lock.lock()
        if waiting[id] === job { waiting.removeValue(forKey: id) }
        let cancelled = job.cancelled
        lock.unlock()
        DispatchQueue.main.async { [weak self] in
            // Released unless another replay has come to wait since.
            guard let self else { return }
            self.lock.lock(); let still = self.waiting[id] != nil; self.lock.unlock()
            if !still { self.onHeld?(id, false) }
        }
        guard !cancelled else {
            DispatchQueue.main.async { [weak self] in self?.pending -= 1 }
            return
        }
        if job.fresh {
            completed[id] = nil
            replayers[id] = nil
            let t = Canvas2DReplayer(trackingWidth: job.w, height: job.h, scale: job.scale, lifetime: job.lifetime, generation: job.generation)
            t.space = job.space
            kept[id] = Canvas2DKept(t)
        }
        guard let k = kept[id], k.tracker.lifetime == job.lifetime, k.tracker.generation == job.generation else {
            DispatchQueue.main.async { [weak self] in self?.pending -= 1 }
            return
        }
        defer { if job.drawing > 0 { completed[id] = job.drawing } }
        let env = Canvas2DSnapshot(images: job.images, fonts: job.fonts)
        var unreadable = 0
        let lists = job.lists.compactMap { $0 }
        unreadable += job.lists.count - lists.count
        // The GPU (LLP 1056 §8.5): a canvas is given to the module when its
        // bitmap is made, if it animates then (or always, for the parity
        // smoke), and stays with it until its next fresh bitmap.
        // Decided at the first lists a fresh bitmap gets (a fresh bitmap's
        // first job often carries none: its draw has not run yet), when
        // nothing drawn before has to be carried over.
        if job.fresh { gpus[id] = nil; undrawn.insert(id) }
        if undrawn.contains(id), !lists.isEmpty {
            undrawn.remove(id)
            // The module's surfaces are 8-bit: a `float16` canvas stays with Core Graphics.
            if Canvas2DGpuModule.mode == .always || job.animating, !job.space.float16, let m = Canvas2DGpuModule.shared,
               let g = Canvas2DGpuCanvas(module: m, width: job.w, height: job.h, scale: job.scale, lifetime: job.lifetime, generation: job.generation,
                                         p3: job.space.p3) {
                gpus[id] = g
                replayers[id] = nil
            }
        }
        if let g = gpus[id], g.lifetime == job.lifetime, g.generation == job.generation {
            var fonts = k.tracker.resolvedFonts
            fonts.merge(job.fonts) { _, new in new }
            let before = k.tracker.snapshot()
            k.tracker.env = env
            for data in lists { k.tracker.trackFonts(data) }
            k.tracker.env = nil
            let gpuEnv = Canvas2DSnapshot(images: job.images, fonts: fonts)
            let t0 = CFAbsoluteTimeGetCurrent()
            let (surface, bad) = g.replay(lists, env: gpuEnv)
            let ms = (CFAbsoluteTimeGetCurrent() - t0) * 1000
            if bad { unreadable += 1 }
            var contents: Any? = surface
            if surface == nil {
                // Core Graphics continues from the GPU's last pixels: the kept
                // lists do not hold them, so they start again at a cover.
                // The GPU keeps its own drawing state; what the tracker
                // follows (fonts and saved levels) carries over.
                gpus[id] = nil
                Canvas2DGpuModule.log("canvas gpu: canvas \(id) failed; Core Graphics continues from its last pixels")
                let r = Canvas2DReplayer(width: job.w, height: job.h, scale: job.scale, lifetime: job.lifetime, generation: job.generation, space: job.space)
                r.continue(from: g.pixels(), state: before)
                r.env = gpuEnv
                for data in lists where !r.apply(data) { unreadable += 1 }
                r.env = nil
                withExtendedLifetime(gpuEnv) {}
                replayers[id] = r
                k.drop()
                k.recording = nil
                contents = r.image()
            }
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.pending -= 1
                for _ in 0..<unreadable { self.errors.append("canvas \(id): unreadable list") }
                guard let contents, self.sequence[id] != nil, job.seq >= self.born[id] ?? .max, job.seq > self.shown[id] ?? 0,
                      let layer = self.layers[id] else { return }
                self.shown[id] = job.seq
                layer.contents = contents
                self.recorders[id]?.hide()
                Canvas2DStats.shown(gpu: surface != nil, recorded: false, ms: ms)
            }
            return
        }
        k.tracker.env = env
        for data in lists { k.keep(data) }
        k.tracker.env = nil
        let keptEnv = Canvas2DSnapshot(images: job.images, fonts: k.fonts)
        // The policy (LLP 1056 §8.4): recorded while it animates, from a
        // cover, drawing nothing a recording draws differently, within the
        // bound, at the display's own scale.
        let record = job.animating && !job.stretch && !job.space.float16 && k.start != nil && !k.refused && k.bounded && job.w > 0 && job.h > 0
        var contents: Any?, recording: Canvas2DRecordLayer.Frame?
        if record, let start = k.start {
            replayers[id] = nil
            recording = .init(lists: k.lists, start: start, env: keptEnv, width: job.w, height: job.h, scale: job.scale, space: job.space)
            k.recording = recording
        } else {
            let r: Canvas2DReplayer
            if let existing = replayers[id], k.recording == nil {
                r = existing
                r.env = env
                for data in lists where !r.apply(data) { unreadable += 1 }
            } else {
                // A new bitmap: from the kept lists when it was recorded (or
                // it is fresh), which carry every list since their start.
                r = Canvas2DReplayer(width: job.w, height: job.h, scale: job.scale, lifetime: job.lifetime, generation: job.generation, space: job.space)
                r.env = env
                if let start = k.start {
                    r.env = keptEnv
                    r.restore(start)
                    for data in k.lists where !r.apply(data) { unreadable += 1 }
                } else {
                    for data in lists where !r.apply(data) { unreadable += 1 }
                }
                replayers[id] = r
                k.recording = nil
            }
            r.env = nil
            contents = r.image()
        }
        // Kept only while it may be recorded: a canvas that does not animate,
        // or kept past the bound, starts again at its next cover.
        if !k.bounded || (!job.animating && k.recording == nil) { k.drop() }
        withExtendedLifetime(env) {}
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.pending -= 1
            for _ in 0..<unreadable { self.errors.append("canvas \(id): unreadable list") }
            // The newest replay done of a canvas still mounted shows: the
            // bitmap, or the recording.
            guard self.sequence[id] != nil, job.seq >= self.born[id] ?? .max, job.seq > self.shown[id] ?? 0,
                  let layer = self.layers[id] else { return }
            self.shown[id] = job.seq
            layer.contents = contents
            Canvas2DStats.shown(gpu: false, recorded: recording != nil, ms: 0)
            if let recording {
                let r = self.recorders[id] ?? { let r = Canvas2DRecordLayer(); layer.addSublayer(r); self.recorders[id] = r; return r }()
                r.show(recording, in: layer.bounds.size)
            } else {
                self.recorders[id]?.hide()
            }
        }
    }

    /// A capture (LLP 1056 D9, as amended for #188): the bitmap of `id` the
    /// presenter keeps, as a PNG, made on the replay queue after every replay
    /// already dispatched. The replay queue waits while `snapshot` copies on
    /// the runtime owner and marks the job; only then may it encode. `done`
    /// hears the PNG on main, nil when copying or encoding was refused.
    func capture(_ id: UInt32, lifetime: UInt64, generation: UInt32, sequence: UInt64, width: Int, height: Int, snapshot: @escaping (() -> Bool) -> Bool = { $0() }, done: @escaping (Data?) -> Void) {
        // A later resize must not replace lists the capture is waiting to read.
        lock.lock(); waiting.removeValue(forKey: id); lock.unlock()
        replay.async { [weak self] in
            #if DEBUG
            self?.beforeCaptureCopy?()
            #endif
            var image: CGImage?
            let copied = snapshot {
                image = self?.captureImage(id, lifetime: lifetime, generation: generation, sequence: sequence, width: width, height: height)
                return image != nil
            }
            #if DEBUG
            if copied { self?.beforeCaptureEncode?() }
            #endif
            let png = copied ? image.flatMap(canvas2DPNG) : nil
            DispatchQueue.main.async { done(png) }
        }
    }

    /// What shows for `id`: the GPU's surface, the Core Graphics bitmap, or,
    /// for a canvas Core Animation records, its kept lists replayed into a
    /// bitmap, which is what the recording draws. An unavailable generation refuses.
    private func captureImage(_ id: UInt32, lifetime: UInt64, generation: UInt32, sequence: UInt64, width: Int, height: Int) -> CGImage? {
        guard sequence > 0, (completed[id] ?? 0) >= sequence else { return nil }
        let current = { (l: UInt64, g: UInt32) in l == lifetime && g == generation }
        var image: CGImage?
        if let g = gpus[id], current(g.lifetime, g.generation), let surface = g.current {
            image = canvas2DImage(of: surface, p3: g.p3)
        } else if let r = replayers[id], current(r.lifetime, r.generation) {
            image = r.image()
        } else if let k = kept[id], let frame = k.recording, current(k.tracker.lifetime, k.tracker.generation) {
            let r = Canvas2DReplayer(width: width, height: height, scale: frame.scale, lifetime: lifetime,
                                     generation: generation, space: frame.space)
            r.env = frame.env
            image = withExtendedLifetime(frame) {
                r.restore(frame.start)
                for data in frame.lists { _ = r.apply(data) }
                return r.image()
            }
        }
        return image
    }

    /// An agent's picture waits for the replays already dispatched to show.
    func waitForReplays(timeout: TimeInterval = 1) {
        let end = Date(timeIntervalSinceNow: timeout)
        while pending > 0 && Date() < end { RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.002)) }
    }

    func place(_ id: UInt32, in box: CGRect, radii: [CGSize]) {
        if let placed = placements[id], placed.box == box, placed.radii == radii { return }
        placements[id] = (box, radii)
        guard let layer = layers[id] else { return }
        CATransaction.begin(); CATransaction.setDisableActions(true)
        Self.place(layer, box, radii)
        recorders[id]?.bounds = CGRect(origin: .zero, size: box.size)
        CATransaction.commit()
    }

    /// Frames the bitmap's layer at the content box and clips it to the
    /// content edge's curve, as the web clips replaced content.
    private static func place(_ layer: CALayer, _ box: CGRect, _ radii: [CGSize]) {
        layer.frame = box
        let bounds = CGRect(origin: .zero, size: box.size)
        if radii.contains(where: { $0.width > 0 && $0.height > 0 }) {
            let mask = (layer.mask as? CAShapeLayer) ?? CAShapeLayer()
            mask.path = BorderPaint.roundedRect(bounds, BorderPaint.reduced(radii, in: bounds))
            layer.mask = mask
        } else {
            layer.mask = nil
        }
    }

    func forget(_ id: UInt32) {
        // Every retired view is forgotten here; only a canvas has a replayer.
        placements.removeValue(forKey: id)
        guard sequence.removeValue(forKey: id) != nil else { return }
        shown.removeValue(forKey: id)
        born.removeValue(forKey: id)
        lock.lock(); waiting.removeValue(forKey: id)?.cancelled = true; lock.unlock()
        recorders.removeValue(forKey: id)
        let retired = lifetimes.removeValue(forKey: id)
        replay.async { [weak self] in
            guard let self else { return }
            if self.replayers[id]?.lifetime == retired { self.replayers.removeValue(forKey: id) }
            if self.kept[id]?.tracker.lifetime == retired { self.kept.removeValue(forKey: id) }
            if self.gpus[id]?.lifetime == retired { self.gpus.removeValue(forKey: id) }
            if self.kept[id] == nil { self.undrawn.remove(id); self.completed.removeValue(forKey: id) }
        }
        layers.removeValue(forKey: id)?.removeFromSuperlayer()
    }
}

/// One canvas's replay not yet started: its lists in order, with what they
/// read.
private final class Canvas2DJob {
    var cancelled = false
    var fresh: Bool
    var w: Int, h: Int, scale: Double, lifetime: UInt64, generation: UInt32
    var lists: [Data?]
    var images: [String: CGImage]
    var fonts: [Canvas2DFont: CTFont]
    var seq: Int
    var drawing: UInt64 = 0
    /// The draw asked for the next frame.
    var animating = false
    /// An explicit bitmap, stretched to the box.
    var stretch = false
    var space = Canvas2DSpace.srgb8
    init(fresh: Bool, w: Int, h: Int, scale: Double, lifetime: UInt64, generation: UInt32, lists: [Data?],
         images: [String: CGImage], fonts: [Canvas2DFont: CTFont], seq: Int) {
        self.fresh = fresh; self.w = w; self.h = h; self.scale = scale; self.lifetime = lifetime
        self.generation = generation; self.lists = lists; self.images = images; self.fonts = fonts; self.seq = seq
    }

    /// Take a later arrival into this replay; always true. A fresh bitmap's
    /// lists supersede everything before them.
    func absorb(_ later: Canvas2DJob) -> Bool {
        if later.fresh {
            fresh = true; w = later.w; h = later.h; scale = later.scale
            lifetime = later.lifetime; generation = later.generation; lists = later.lists
            fonts = later.fonts
        } else {
            lists += later.lists
            fonts.merge(later.fonts) { _, new in new }
        }
        images = later.images
        seq = later.seq
        drawing = later.drawing
        animating = later.animating
        stretch = later.stretch
        space = later.space
        return true
    }
}

/// What one replay reads off the main thread: the images and fonts as they
/// were when its lists arrived.
final class Canvas2DSnapshot: Canvas2DEnv {
    let images: [String: CGImage]
    let fonts: [Canvas2DFont: CTFont]
    init(images: [String: CGImage], fonts: [Canvas2DFont: CTFont]) { self.images = images; self.fonts = fonts }
    func canvasFont(_ f: Canvas2DFont) -> CTFont? { fonts[f] }
    func canvasImage(_ src: String) -> CGImage? { images[src] }
}

