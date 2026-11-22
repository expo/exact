// An icon path on a node (LLP 1121 D10), drawn by ExactCore’s IconPath.
#if os(macOS)
import AppKit
#else
import UIKit
#endif
import ExactCore

extension NodeView {
    func iconPathImage(size: CGSize, crop: CGRect? = nil) -> IconImage? {
        guard let commands = props["symbolPath"] else { return nil }
        return IconPath.image(commands, viewBox: props["symbolViewBox"] ?? "", size: size, crop: crop,
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
        // Set the leaf's frame only where the glyph is placed: an interim frame is a change to the image view's
        // geometry, for which UIKit sets its contents again, in every layout
        leaf.clipsToBounds = !listSymbol
        let fit = listSymbol ? "none" : style["object_fit"]?.string ?? "fill"
        var shown = image
        if props["symbolPath"] != nil {
            let box = IconPath.viewBox(props["symbolViewBox"] ?? "")
            let rect = IconPath.placed(viewBox: box, em: number("font_size", PageFacts.defaultRootFontSize), fit: fit,
                                       position: style["object_position"]?.array, in: content)
            let visible = listSymbol ? rect : rect.intersection(content)
            // Keep the full placement transform even when cover needs only a sliver of it.
            let crop = visible.isEmpty ? .zero : visible.offsetBy(dx: -rect.minX, dy: -rect.minY)
            let state = SymbolEffectState.of(leaf)
            if state.pathTemplateGeometry?.size != rect.size || state.pathTemplateGeometry?.crop != crop {
                state.pathTemplateGeometry = (rect.size, crop)
                state.pathTemplate = iconPathImage(size: rect.size, crop: crop)
            }
            shown = state.pathTemplate
            leaf.frame = visible.isEmpty ? .zero : visible
            leaf.contentMode = .scaleToFill
        } else {
            let rect = IconPath.placed(natural: image?.size ?? .zero, fit: fit,
                                       position: style["object_position"]?.array, in: content)
            leaf.frame = rect
            leaf.contentMode = .scaleAspectFit
        }
        inkSymbolView(shown)
        if (listSymbol || content.contains(leaf.frame)), cornerRadii(in: bounds).allSatisfy({ $0 == 0 }) {
            if leaf.layer.mask != nil { leaf.layer.mask = nil }; return
        }
        var transform = CGAffineTransform(translationX: -content.minX, y: -content.minY)
        let mask = CAShapeLayer()
        mask.frame = content.offsetBy(dx: -leaf.frame.minX, dy: -leaf.frame.minY)
        mask.masksToBounds = true
        mask.path = roundedPath(in: bounds).cgPath.copy(using: &transform)
        leaf.layer.mask = mask
    }
}
#endif
