// @ref LLP 1100 D12. Core Animation composites in a floating-point bitmap,
// including supplied frames at the layers the render server normally draws.
// On iOS 27 render(in:) still clips native extended-range gradients and omits shadows.
import AVFoundation
import CoreImage
import QuartzCore
#if os(iOS) || os(tvOS)
import UIKit
#else
import AppKit
#endif

private struct SamplePicture {
    let layer: CALayer
    let area: CGRect
    let image: CGImage?
    let refusal: String
    let replacesSubviews: Bool
    let clip: Bool
}

extension Agent {
    func sample(_ req: [String: Any]) -> [String: Any] {
        _ = settleForPicture()
        if let now = session.clock {
            _ = session.canvases.waitUntilReady()
            session.canvases.settle(now: now)
        }
        presenter.canvas2d.waitForReplays()
        let requested = req["points"] as? [[Any]] ?? []
        let points = requested.compactMap { p -> CGPoint? in
            guard p.count == 2, let x = (p[0] as? NSNumber)?.doubleValue, let y = (p[1] as? NSNumber)?.doubleValue,
                  x.isFinite, y.isFinite else { return nil }
            return CGPoint(x: x, y: y)
        }
        guard !points.isEmpty, points.count == requested.count else { return ["malformed": true, "error": "sample needs pairs of finite viewport coordinates"] }

        // Snapshot providers can run the main loop. Finish them before borrowing any live layer.
        var guests: [UInt32: ExactWebImage] = [:]
        var stableGuests = false
        for _ in 0..<3 {
            let commit = session.appliedBatches
            guests = session.webviews.snapshots().merging(session.natives.snapshots()) { web, _ in web }
            #if targetEnvironment(simulator) && (os(iOS) || os(tvOS))
            // WebKit's simulator snapshot flushes remote text. UIKit can then draw
            // that text at backing scale, including the guest's transparent pixels.
            for id in Array(guests.keys) {
                guard let web = presenter.views[id]?.web, !web.bounds.isEmpty else { continue }
                let format = UIGraphicsImageRendererFormat()
                format.scale = web.window?.screen.scale ?? web.traitCollection.displayScale
                format.opaque = false
                format.preferredRange = .extended
                var complete = false
                let image = UIGraphicsImageRenderer(bounds: web.bounds, format: format).image { _ in
                    complete = web.drawHierarchy(in: web.bounds, afterScreenUpdates: true)
                }
                guests[id] = complete ? image : nil
            }
            #endif
            if session.appliedBatches == commit { stableGuests = true; break }
        }
        if !stableGuests { guests.removeAll() }
        #if os(iOS) || os(tvOS)
        let view = presenter.viewport
        let size = view.pageSize
        let scale = max(1, view.window?.screen.scale ?? view.traitCollection.displayScale)
        let layer: CALayer
        var mapping = CGAffineTransform.identity
        let area = view.window.map { view.convert(view.pageBounds, to: $0) } ?? CGRect(origin: .zero, size: size)
        let backing = CGAffineTransform(translationX: area.minX, y: area.minY)
            .scaledBy(x: area.width / max(1, size.width), y: area.height / max(1, size.height))
        if presenter.surfaces(includingRetiring: true).contains(where: { $0.kind != .document }), let window = view.window {
            layer = window.layer
            if area.width > 0, area.height > 0 { mapping = mapping.scaledBy(x: size.width / area.width, y: size.height / area.height) }
            mapping = mapping.translatedBy(x: -area.minX, y: -area.minY)
        } else {
            layer = presenter.documentHost.layer
            mapping = mapping.translatedBy(x: view.bounds.minX - view.pageBounds.minX, y: view.bounds.minY - view.pageBounds.minY)
        }
        #else
        let view = presenter.navigation.stackSplit.agentRoot
        if view.layer == nil {
            view.wantsLayer = true
            view.layoutSubtreeIfNeeded()
            view.displayIfNeeded()
        }
        guard let layer = view.layer else { return ["error": "the session's view has no layer"] }
        let area = presenter.navigation.stackSplit.agentCaptureArea
        let size = area.size
        let scale = view.window?.backingScaleFactor ?? max(1, layer.contentsScale)
        let backing = CGAffineTransform(translationX: area.minX, y: area.minY)
        var mapping = CGAffineTransform(translationX: -area.minX, y: -area.minY)
        if presenter.navigation.stackSplit.active, !view.isFlipped {
            mapping = mapping.translatedBy(x: 0, y: view.bounds.height).scaledBy(x: 1, y: -1)
        }
        #endif
        if req["strict"] as? Bool == true, points.contains(where: {
            $0.x < 0 || $0.y < 0 || $0.x >= size.width || $0.y >= size.height
        }) { return ["error": "the sample lies outside the viewport"] }
        guard size.width > 0, size.height > 0 else { return ["error": "the viewport has no pixels"] }
        let bounds = CGRect(origin: .zero, size: size).applying(backing)
        guard bounds.width > 0, bounds.height > 0 else { return ["error": "the viewport has no backing pixels"] }
        let pixels = points.map {
            let p = $0.applying(backing)
            return CGPoint(x: min(ceil(bounds.maxX * scale) - 1, max(floor(bounds.minX * scale), floor(p.x * scale))),
                           y: min(ceil(bounds.maxY * scale) - 1, max(floor(bounds.minY * scale), floor(p.y * scale))))
        }
        let box = pixels.reduce(CGRect.null) { $0.union(CGRect(origin: $1, size: CGSize(width: 1, height: 1))) }
        let (w, h) = (Int(box.width), Int(box.height))
        guard let extended = CGColorSpace(name: CGColorSpace.extendedSRGB), let linear = CGColorSpace(name: CGColorSpace.extendedLinearSRGB),
              let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 32, bytesPerRow: w * 16, space: extended,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.floatComponents.rawValue
                                      | CGBitmapInfo.byteOrder32Little.rawValue),
              let data = ctx.data else { return ["error": "no extended-range context for the view"] }
        ctx.translateBy(x: -box.minX, y: box.maxY)
        ctx.scaleBy(x: scale, y: -scale)
        ctx.concatenate(backing)
        ctx.concatenate(mapping)
        CATransaction.flush()
        let pictures = samplePictures(in: layer, guests: guests, stable: stableGuests)
        let offsets = pixels.map { (Int($0.y - box.minY) * w + Int($0.x - box.minX)) * 4 }
        func read() -> [[Float]] {
            memset(data, 0, ctx.bytesPerRow * h)
            layer.render(in: ctx)
            let px = data.assumingMemoryBound(to: Float.self)
            return offsets.map { i in (0..<4).map { px[i + $0] } }
        }
        CATransaction.begin(); CATransaction.setDisableActions(true)
        var restore: [() -> Void] = []
        defer { for undo in restore.reversed() { undo() }; CATransaction.commit() }
        // render(in:) visits siblings in array order. The compositor also uses zPosition.
        func order(_ parent: CALayer) {
            guard !parent.isHidden, parent.opacity > 0 else { return }
            if let mask = parent.mask { order(mask) }
            guard let children = parent.sublayers else { return }
            if zip(children, children.dropFirst()).contains(where: { $0.zPosition > $1.zPosition }) {
                parent.sublayers = children.enumerated().sorted {
                    ($0.element.zPosition, $0.offset) < ($1.element.zPosition, $1.offset)
                }.map(\.element)
                restore.append { parent.sublayers = children }
            }
            children.forEach(order)
        }
        order(layer)
        var unknown: [(CALayer, String)] = []
        for picture in pictures {
            let source = picture.layer
            if picture.replacesSubviews {
                let background = source.backgroundColor, contents = source.contents, border = source.borderWidth
                source.backgroundColor = nil; source.contents = nil; source.borderWidth = 0
                restore.append { source.backgroundColor = background; source.contents = contents; source.borderWidth = border }
                let visible = (source.sublayers ?? []).filter { !$0.isHidden }
                for child in visible { child.isHidden = true }
                restore.append { for child in visible { child.isHidden = false } }
            }
            let stand = CALayer()
            stand.frame = picture.area
            stand.contents = picture.image
            if picture.image == nil {
                stand.backgroundColor = CGColor(gray: 1, alpha: 1)
                unknown.append((stand, picture.refusal))
            }
            if picture.clip {
                let clip = CALayer()
                clip.frame = source.bounds
                clip.bounds = source.bounds
                clip.masksToBounds = true
                clip.addSublayer(stand)
                source.insertSublayer(clip, at: 0)
                restore.append { clip.removeFromSuperlayer() }
            } else {
                source.insertSublayer(stand, at: 0)
                restore.append { stand.removeFromSuperlayer() }
            }
        }
        let values = read()
        // A missing picture only matters where it contributes. The same compositor
        // resolves masks, rounded clips and occlusion for both the probe and the pixel.
        for (stand, reason) in unknown {
            for alpha in [CGFloat(0), 1] {
                stand.backgroundColor = CGColor(gray: 0, alpha: alpha)
                if read() != values { return ["error": reason] }
            }
            stand.backgroundColor = CGColor(gray: 1, alpha: 1)
        }
        // Apple blends SDR layers in encoded sRGB. Convert after blending so half
        // black over red has the displayed intensity while P3 and HDR remain extended.
        var result: [[Double]] = []
        for px in values {
            let a = CGFloat(px[3])
            let rgba = px.prefix(3).map { a > 0 ? CGFloat($0) / a : 0 } + [a]
            let converted = CGColor(colorSpace: extended, components: rgba)?.converted(to: linear, intent: .defaultIntent, options: nil)
            guard let components = converted?.components else { return ["error": "could not convert the sample to linear sRGB"] }
            result.append(components.map { Agent.r4(Double($0)) })
        }
        return ["space": "extended-linear-srgb", "values": result]
    }

    private func samplePictures(in root: CALayer, guests: [UInt32: ExactWebImage], stable: Bool) -> [SamplePicture] {
        var owners: [ObjectIdentifier: NodeView] = [:]
        var guestLayers: [ObjectIdentifier: NodeView] = [:]
        func guest(_ view: PaintView, _ node: NodeView) {
            if let layer = view.layer as CALayer? { guestLayers[ObjectIdentifier(layer)] = node }
        }
        for node in presenter.views.values {
            if let layer = node.layer as CALayer? { owners[ObjectIdentifier(layer)] = node }
            if let view = node.web { guest(view, node) }
        }
        for (id, view) in session.natives.snapshotViews {
            if let node = presenter.views[id] { guest(view, node) }
        }
        var pictures: [SamplePicture] = []
        func walk(_ layer: CALayer, owner: NodeView?) {
            guard !layer.isHidden, layer.opacity > 0 else { return }
            let key = ObjectIdentifier(layer)
            let node = owners[key] ?? owner
            var image: CGImage?, reason: String?, area = layer.bounds, replaces = false, clip = false
            if let guest = guestLayers[key] {
                #if os(macOS)
                image = guests[guest.id]?.cgImage(forProposedRect: nil, context: nil, hints: nil)
                #else
                image = guests[guest.id]?.cgImage
                #endif
                reason = stable ? "the hosted view returned no snapshot" : "the page changed during the hosted snapshot"
                replaces = true
            } else if let metal = layer as? CAMetalLayer {
                #if os(macOS)
                let canvas = node?.metal?.layer
                #else
                let canvas = node?.metal?.metalLayer
                #endif
                if canvas === metal, let node { image = node.canvases?.sampledFrame(of: node) }
                reason = "the Metal layer has no readable presented frame"
            } else if let player = layer as? AVPlayerLayer, player.isReadyForDisplay {
                if #available(iOS 27, macOS 27, tvOS 27, *) {
                    image = player.displayedReadOnlyPixelBuffer()?.withUnsafeBuffer { Self.videoPicture($0) }
                } else { image = Self.videoPicture(player.displayedPixelBuffer()) }
                clip = true
                area = player.videoRect
                reason = "AVPlayerLayer did not provide the displayed frame (playing, protected or unavailable)"
            } else if let display = layer as? AVSampleBufferDisplayLayer {
                clip = true
                if #available(iOS 17.4, tvOS 17.4, macOS 14.4, *), display.isReadyForDisplay {
                    reason = "the sample-buffer renderer did not provide the displayed frame"
                    image = Self.videoPicture(display.sampleBufferRenderer.displayedPixelBuffer())
                    if let image, display.videoGravity != .resize {
                        let w = CGFloat(image.width), h = CGFloat(image.height)
                        let ratio = display.videoGravity == .resizeAspectFill ? max(area.width / w, area.height / h) : min(area.width / w, area.height / h)
                        area = CGRect(x: area.midX - w * ratio / 2, y: area.midY - h * ratio / 2, width: w * ratio, height: h * ratio)
                    }
                } else if #unavailable(iOS 17.4, tvOS 17.4, macOS 14.4) {
                    reason = "this OS does not expose the displayed sample buffer"
                }
            } else if let preview = layer as? AVCaptureVideoPreviewLayer {
                if #available(tvOS 17, *) {
                    let rect = preview.layerRectConverted(fromMetadataOutputRect: CGRect(x: 0, y: 0, width: 1, height: 1))
                    if !rect.isEmpty { area = area.intersection(rect) }
                }
                reason = "the live camera preview does not expose its displayed frame"
            }
            #if os(macOS)
            if layer is CAOpenGLLayer { reason = "the OpenGL layer does not expose its displayed frame" }
            #else
            if layer is CAEAGLLayer { reason = "the OpenGL layer does not expose its displayed frame" }
            #endif
            if let reason, !area.isEmpty {
                let owner = node.map { $0.props["testId"] ?? "\($0.kind) #\($0.id)" } ?? "the session"
                pictures.append(SamplePicture(layer: layer, area: area, image: image, refusal: "sample \(owner): \(reason)", replacesSubviews: replaces, clip: clip))
            }
            if let mask = layer.mask { walk(mask, owner: node) }
            if !replaces { for child in layer.sublayers ?? [] { walk(child, owner: node) } }
        }
        walk(root, owner: nil)
        return pictures
    }

    private static let videoContext = CIContext()

    private static func videoPicture(_ buffer: CVPixelBuffer?) -> CGImage? {
        guard let buffer else { return nil }
        let image = CIImage(cvPixelBuffer: buffer, options: [.applyOrientationProperty: true])
        return videoContext.createCGImage(image, from: image.extent, format: .RGBAh, colorSpace: CGColorSpace(name: CGColorSpace.extendedLinearSRGB))
    }

    static func r4(_ v: Double) -> Double { (v * 10_000).rounded() / 10_000 }
}
