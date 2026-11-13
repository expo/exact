#if os(iOS) || os(tvOS)
import UIKit

/// Pixels a view's subtree was painted into: premultiplied RGBA, rows
/// top-down, `width * 4` bytes per row, owned by the context.
package struct Bitmap {
    let context: CGContext
    package let width: Int
    package let height: Int
    package var bytes: UnsafeMutableRawPointer? { context.data }
    package var bytesPerRow: Int { context.bytesPerRow }
    /// Empty pixels for the module to fill (a readback).
    package static func blank(width: Int, height: Int) -> Bitmap? {
        guard width > 0, height > 0, let space = CGColorSpace(name: CGColorSpace.sRGB),
              let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue)
        else { return nil }
        return Bitmap(context: ctx, width: width, height: height)
    }
    package var image: UIImage? { context.makeImage().map { UIImage(cgImage: $0) } }
}

/// A view's subtree as pixels (LLP 1014 D3).
package enum Capture {
    /// A capture is drawing: its draws are not repaints (D4 b).
    nonisolated(unsafe) static var capturing = false
    /// Guest pictures when this turn chooses arm snapshots; their remote
    /// platform views are hidden while the nodes draw these (LLP 1020 D4/D6).
    nonisolated(unsafe) static var web: [UInt32: ExactWebImage] = [:]
    /// EXACT_CAPTURE=cpu: the Core Graphics capture even where Metal is
    /// present — the measure's baseline, and the fixture's oracle.
    package static let cpu = ProcessInfo.processInfo.environment["EXACT_CAPTURE"] == "cpu"
    /// The subtree painted at `scale`: premultiplied RGBA, rows top-down,
    /// transparent where nothing painted.
    package static func bitmap(of view: UIView, scale: CGFloat) -> Bitmap? {
        let w = Int((view.bounds.width * scale).rounded()), h = Int((view.bounds.height * scale).rounded())
        guard let bitmap = Bitmap.blank(width: w, height: h) else { return nil }
        // The GPU, where there is one (`Shadow`); Core Graphics otherwise.
        if !cpu, let shadow = Shadow.shared {
            capturing = true
            let ok = shadow.render(view, scale: scale, into: bitmap)
            capturing = false
            if ok {
                // EXACT_CAPTURE_DUMP=<dir>: this render and the CPU one of the
                // same frame, as PNGs, to compare the two by eye.
                if let dir = ProcessInfo.processInfo.environment["EXACT_CAPTURE_DUMP"], dumped < 4 {
                    dumped += 1
                    try? bitmap.image?.pngData()?.write(to: URL(fileURLWithPath: dir).appendingPathComponent("gpu-\(dumped).png"))
                    if let cpuBitmap = Bitmap.blank(width: w, height: h) { draw(view, scale: scale, into: cpuBitmap); try? cpuBitmap.image?.pngData()?.write(to: URL(fileURLWithPath: dir).appendingPathComponent("cpu-\(dumped).png")) }
                }
                return bitmap
            }
        }
        draw(view, scale: scale, into: bitmap)
        return bitmap
    }
    nonisolated(unsafe) static var dumped = 0

    /// The CPU capture: Core Graphics rasterizes the subtree into `bitmap`.
    static func draw(_ view: UIView, scale: CGFloat, into bitmap: Bitmap) {
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        let h = bitmap.height
        let ctx = bitmap.context
        // UIKit's geometry — y down from the top — into a context whose y
        // is up from the bottom: the first row in memory is then the top.
        ctx.translateBy(x: 0, y: CGFloat(h))
        ctx.scaleBy(x: scale, y: -scale)
        // A canvas chooses its child's opacity. The filter still belongs
        // to the captured pixels, even when CSS opacity is ignored here.
        // A placed canvas child is drawn as its writers asked (`asAsked`).
        let rootOpacity = rootOpacity(of: view.layer)
        let elsewhere = (view.layer as? NodeLayer)?.drawnElsewhere ?? false
        (view.layer as? NodeLayer)?.drawnElsewhere = false
        // A canvas nested under this one that is painted through its own
        // surface: its picture comes by readback (its draw), not from its
        // overlay's views, which the render would paint regardless of their
        // alpha — so those are hidden for the duration.
        var hidden: [UIView] = []
        func hide(_ v: UIView) {
            for s in v.subviews {
                if let n = s as? NodeView, n.placement != nil, !n.isHidden { n.isHidden = true; hidden.append(n); continue }
                // A nested canvas paints its readback in `draw` (LLP 1014):
                // drop the layer's cached picture so the render calls `draw`
                // instead of copying what it drew last time (its placements
                // are read there too, so the cards of a deck in the sky move).
                if let n = s as? NodeView, n.kind == "canvas" { n.layer.contents = nil; n.setNeedsDisplay() }
                if let n = s as? NodeView, let o = n.overlay, o.alpha == 0, !o.isHidden { o.isHidden = true; hidden.append(o); continue }
                hide(s)
            }
        }
        hide(view)
        capturing = true
        UIGraphicsPushContext(ctx)
        // A filtered box shows as its picture (`BoxFilter`), drawn here as
        // the box is, shown and opaque (a placed canvas child's is hidden).
        if let picture = (view as? NodeView)?.filterPicture {
            let (hidden, opacity) = (picture.isHidden, picture.opacity)
            (picture.isHidden, picture.opacity) = (false, 1)
            picture.render(in: ctx)
            (picture.isHidden, picture.opacity) = (hidden, opacity)
        } else {
            let copy = paintOrderLayer(of: view, root: view.layer)
            copy.opacity = rootOpacity
            copy.render(in: ctx)
        }
        UIGraphicsPopContext()
        capturing = false
        for o in hidden { o.isHidden = false }
        (view.layer as? NodeLayer)?.drawnElsewhere = elsewhere
    }

    /// The canvas owns CSS opacity; feedback and the filter stay in its texture.
    static func rootOpacity(of layer: CALayer) -> Float {
        let model = (layer as? NodeLayer)?.ownOpacity ?? layer.opacity
        let node = layer.delegate as? NodeView, base = node?.ownOpacity ?? model
        let factor = node?.filterOpacityFactor ?? 1
        // A fully transparent CSS root has no ratio to undo. Capture its
        // unattenuated contents, as for an unfiltered transparent root.
        guard base > 0 else { return factor }
        func animatesOpacity(_ animation: CAAnimation) -> Bool {
            if let group = animation as? CAAnimationGroup { return group.animations?.contains(where: animatesOpacity) == true }
            return (animation as? CAPropertyAnimation)?.keyPath == "opacity"
        }
        let animated = (layer.animationKeys() ?? []).contains { layer.animation(forKey: $0).map(animatesOpacity) == true }
        guard animated, model > 0 else { return factor }
        // Placed children composite at 1. Apply their presentation delta to
        // the retained model before removing CSS opacity from the texture.
        let delta = (layer.presentation()?.opacity ?? layer.opacity) - layer.opacity
        // A batch can capture before the new filter factor reaches the
        // layer. Normalize the feedback against its retained model first.
        return (model + delta) / model * factor
    }
}

#endif
