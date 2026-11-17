// An icon path on a node (LLP 1121 D10), drawn by ExactCore’s IconPath.
#if os(macOS)
import AppKit
#else
import UIKit
#endif
import ExactCore

extension NodeView {
    /// An icon path's image for this node `side` points square, when it has
    /// one (`symbolPath`), filled by its `symbolFillRule`.
    func iconPathImage(side: CGFloat) -> IconImage? {
        guard let commands = props["symbolPath"] else { return nil }
        return IconPath.image(commands, viewBox: props["symbolViewBox"] ?? "", size: CGSize(width: side, height: side),
                              rule: IconPath.rule(props["symbolFillRule"]))
    }
}

#if os(iOS) || os(tvOS)
extension NodeView {
    func inkedPathImage(_ image: UIImage, color: UIColor) -> UIImage? {
        guard let mask = image.cgImage,
              let ctx = CGContext(data: nil, width: mask.width, height: mask.height, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        let rect = CGRect(x: 0, y: 0, width: mask.width, height: mask.height)
        ctx.clip(to: rect, mask: mask)
        ctx.setFillColor(color.cgColor); ctx.fill(rect)
        return ctx.makeImage().map { UIImage(cgImage: $0, scale: image.scale, orientation: image.imageOrientation).withRenderingMode(.alwaysOriginal) }
    }

    func layoutSymbol() {
        guard let leaf = symbolView else { return }
        let content = contentBox()
        leaf.frame = content; leaf.clipsToBounds = !listSymbol // a list cell's image is its own size, centred, as UIKit's
        switch listSymbol ? "none" : style["object_fit"]?.string ?? "fill" {
        case "cover": leaf.contentMode = .scaleAspectFill
        case "none": leaf.contentMode = .center
        case "scale-down":
            let size = image?.size ?? .zero
            leaf.contentMode = size.width <= content.width && size.height <= content.height ? .center : .scaleAspectFit
        // `contain`, and `fill`: a symbol is a vector glyph, which keeps its
        // aspect in a box of another shape as an SVG's default
        // `preserveAspectRatio` (`xMidYMid meet`) keeps it, and as the web
        // host's symbol mask draws it (LLP 1011 §4).
        default: leaf.contentMode = .scaleAspectFit
        }
        // An icon path is a vector: drawn again at the box it fills, not its em square stretched (UIKit scales a bitmap).
        var shown = image
        if props["symbolPath"] != nil, let image {
            let side = max(content.width, content.height)
            // (Centred, it is its em square again, whatever the box was before.)
            if leaf.contentMode == .center { shown = image }
            else if side > 0, side != image.size.width {
                let state = SymbolEffectState.of(leaf)
                if state.pathTemplate?.size.width != side { state.pathTemplate = iconPathImage(side: side) }
                shown = state.pathTemplate
            }
        }
        inkSymbolView(shown)
        // A square box clips nothing the content box does not.
        if cornerRadii(in: bounds).allSatisfy({ $0 == 0 }) { if leaf.layer.mask != nil { leaf.layer.mask = nil }; return }
        let path = roundedPath(in: bounds).cgPath
        var transform = CGAffineTransform(translationX: -content.minX, y: -content.minY)
        let mask = CAShapeLayer(); mask.path = path.copy(using: &transform); leaf.layer.mask = mask
    }
}
#endif
