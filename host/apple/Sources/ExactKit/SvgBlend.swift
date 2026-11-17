// An SVG owns the backdrop for blends between its elements. UIKit has no
// public layer blend, so SVG islands composite in sRGB with Core Graphics
// (LLP 1055.000 D2, D19).
import CoreGraphics
import QuartzCore

#if os(iOS) || os(tvOS)
final class SvgBlend {
    static func requested(_ value: Any) -> Bool {
        if let d = value as? [String: Any] {
            // Only scene elements have numeric ids. Text chunks also use
            // "bl", for dominant-baseline rather than a blend mode.
            if d["id"] is NSNumber, let mode = d["bl"] as? NSNumber, mode.intValue != 0 { return true }
            return d.values.contains { requested($0) }
        }
        return (value as? [Any])?.contains { requested($0) } ?? false
    }

    private static var saidUnavailable = false
    static func unavailable() {
        guard !saidUnavailable else { return }
        saidUnavailable = true
        FileHandle.standardError.write(Data("exact style-undrawn: mix-blend-mode could not composite its SVG picture; the SVG draws unblended\n".utf8))
    }

    struct Style { let mode: Int; let isolate: Bool }
    var styles: [ObjectIdentifier: Style] = [:]
    private static let modes: [CGBlendMode] = [
        .normal, .multiply, .screen, .overlay, .darken, .lighten, .colorDodge,
        .colorBurn, .hardLight, .softLight, .difference, .exclusion, .hue,
        .saturation, .color, .luminosity,
    ]

    func contains(_ layer: CALayer) -> Bool {
        (styles[ObjectIdentifier(layer)]?.mode ?? 0) != 0
            || (layer.sublayers ?? []).contains { contains($0) }
    }

    /// Local coordinates to the parent's, including the bounds origin.
    private func placed(_ layer: CALayer) -> CGAffineTransform {
        CGAffineTransform(translationX: -layer.bounds.minX - layer.anchorPoint.x * layer.bounds.width,
                          y: -layer.bounds.minY - layer.anchorPoint.y * layer.bounds.height)
            .concatenating(layer.affineTransform())
            .concatenating(CGAffineTransform(translationX: layer.position.x, y: layer.position.y))
    }

    func bounds(_ layer: CALayer) -> CGRect {
        if layer.isHidden { return .null }
        var rect = layer.contents == nil ? CGRect.null : layer.bounds
        if let shape = layer as? CAShapeLayer, let path = shape.path {
            rect = path.boundingBoxOfPath
            // Square caps extend independently of the join's miter limit.
            if shape.strokeColor != nil { rect = rect.insetBy(dx: -shape.lineWidth * max(sqrt(2), shape.miterLimit) / 2, dy: -shape.lineWidth * max(sqrt(2), shape.miterLimit) / 2) }
        }
        let sub = CATransform3DGetAffineTransform(layer.sublayerTransform)
        for child in layer.sublayers ?? [] { rect = rect.union(bounds(child).applying(placed(child)).applying(sub)) }
        if layer.shadowOpacity > 0, !rect.isNull {
            // Keep the Gaussian tail for both CALayer.render and the nested
            // group's Core Graphics shadow, before the filter/viewport clip.
            let spread = layer.shadowRadius * 3
            let source = layer.shadowPath?.boundingBoxOfPath ?? rect
            let shadow = source.offsetBy(dx: layer.shadowOffset.width, dy: layer.shadowOffset.height)
                .insetBy(dx: -spread, dy: -spread)
            rect = rect.union(shadow)
        }
        if layer.masksToBounds { rect = rect.intersection(layer.bounds) }
        return rect
    }

    /// A bitmap cannot keep a render-server animation moving.
    func canCapture(_ layer: CALayer) -> Bool {
        (layer.animationKeys() ?? []).isEmpty
            && CATransform3DIsAffine(layer.transform) && CATransform3DIsAffine(layer.sublayerTransform)
            && (layer.mask.map { canCapture($0) } ?? true)
            && (layer.sublayers ?? []).allSatisfy { canCapture($0) }
    }

    func bitmap(_ layer: CALayer, rect: CGRect, w: Int, h: Int, placing: Bool = false) -> CGContext? {
        guard canCapture(layer), rect.minX.isFinite, rect.minY.isFinite,
              rect.width.isFinite, rect.height.isFinite, rect.width > 0, rect.height > 0,
              w > 0, h > 0, w <= SvgIsland.cap / h,
              let space = CGColorSpace(name: CGColorSpace.sRGB),
              let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                                  space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        ctx.translateBy(x: 0, y: CGFloat(h)); ctx.scaleBy(x: CGFloat(w) / rect.width, y: -CGFloat(h) / rect.height)
        ctx.translateBy(x: -rect.minX, y: -rect.minY)
        if placing { ctx.concatenate(placed(layer)) }
        guard draw(layer, in: ctx) else { return nil }
        return ctx
    }

    /// Draw in the layer's local coordinates. Normal groups share the current
    /// backdrop; opacity, masks and isolation establish a separate group.
    func draw(_ layer: CALayer, in ctx: CGContext) -> Bool {
        guard !layer.isHidden, layer.opacity > 0 else { return true }
        let style = styles[ObjectIdentifier(layer)]
        let mode = style?.mode ?? 0
        let nested = (layer.sublayers ?? []).contains { contains($0) }
        ctx.saveGState()
        defer { ctx.restoreGState() }
        ctx.setBlendMode(Self.modes[min(max(mode, 0), Self.modes.count - 1)])
        if layer.masksToBounds { ctx.clip(to: layer.bounds) }
        if let mask = layer.mask {
            let rect = ctx.boundingBoxOfClipPath.integral
            let k = sqrt(abs(ctx.ctm.a * ctx.ctm.d - ctx.ctm.b * ctx.ctm.c))
            guard let e = SvgIsland.extent(rect, k: max(k, 0.01), seen: nil) else { return false }
            guard let image = bitmap(mask, rect: e.rect, w: e.w, h: e.h, placing: true)?.makeImage() else { return false }
            // CGImage's rows run opposite to this top-down user space.
            let flip = CGAffineTransform(translationX: 0, y: e.rect.minY + e.rect.maxY).scaledBy(x: 1, y: -1)
            ctx.concatenate(flip)
            ctx.clip(to: e.rect, mask: image)
            ctx.concatenate(flip)
        }
        if !nested {
            // Apply root coverage once. A neutral receiver prevents render(in:)
            // from multiplying the context's alpha or mask again.
            let opacity = layer.opacity, mask = layer.mask
            ctx.setAlpha(CGFloat(opacity))
            CATransaction.begin(); CATransaction.setDisableActions(true)
            layer.opacity = 1; layer.mask = nil
            defer { layer.opacity = opacity; layer.mask = mask; CATransaction.commit() }
            ctx.beginTransparencyLayer(auxiliaryInfo: nil)
            layer.render(in: ctx)
            ctx.endTransparencyLayer()
            return true
        }
        let group = mode != 0 || style?.isolate == true || layer.opacity < 1 || layer.mask != nil || layer.shadowOpacity > 0
        if group {
            ctx.setAlpha(CGFloat(layer.opacity))
            if layer.shadowOpacity > 0, let color = layer.shadowColor?.copy(alpha: CGFloat(layer.shadowOpacity)) {
                // Quartz shadows use base-space pixels, unaffected by the CTM.
                // Its linear part includes the bitmap's Y flip, device scale,
                // viewBox and every enclosing SVG transform.
                let t = ctx.ctm
                let x = hypot(t.a, t.b), y = hypot(t.c, t.d)
                if layer.shadowRadius > 0 {
                    // A scalar Quartz blur cannot represent an elliptical blur.
                    guard abs(x - y) <= max(x, y) * 1e-6,
                          abs(t.a * t.c + t.b * t.d) <= x * y * 1e-6 else { return false }
                }
                let offset = layer.shadowOffset.applying(t)
                ctx.setShadow(offset: offset, blur: layer.shadowRadius * 2 * x, color: color)
            }
            ctx.beginTransparencyLayer(auxiliaryInfo: nil)
        }
        var complete = true
        ctx.concatenate(CATransform3DGetAffineTransform(layer.sublayerTransform))
        for child in layer.sublayers ?? [] {
            ctx.saveGState()
            ctx.concatenate(placed(child))
            complete = draw(child, in: ctx)
            ctx.restoreGState()
            if !complete { break }
        }
        if group { ctx.endTransparencyLayer() }
        return complete
    }
}

#endif
