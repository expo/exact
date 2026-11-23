// Badges and tvOS dots are drawn as images so UIKit owns the bar item's focus.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

#if os(tvOS)
struct BarDotFace: Equatable {
    /// A dot is a bar icon. Bound each bitmap dimension even for oversized authored boxes.
    private static let pixelLimit: CGFloat = 256

    let rect: CGRect
    let size: CGSize
    let color: [Double]
    let corners: [CGSize]
    let opacity: CGFloat
    let tintShare: Double?

    init?(_ button: NodeView) {
        guard button.kind == "button", let presenter = button.presenter else { return nil }
        let children = presenter.flats.children(of: button.id) ?? button.childNodes.map(\.id)
        let shown = children.filter { id in
            guard let style = presenter.flats.leaves[id]?.style ?? presenter.views[id]?.style else { return false }
            return style["display"]?.string != "none" && !cssVisibilityHides(style)
        }
        guard shown.count == 1, let id = shown.first else { return nil }
        let leaf = presenter.flats.leaves[id], node = presenter.views[id]
        guard (presenter.flats.children(of: id) ?? node?.childNodes.map(\.id) ?? []).isEmpty,
              let style = leaf?.style ?? node?.style, leaf != nil || node?.kind == "view",
              let width = style["width"]?.number, let height = style["height"]?.number, width > 0, height > 0,
              let fill = style["background_color"] else { return nil }
        let host = node ?? button
        let left = max(0, button.number("padding_left")), right = max(0, button.number("padding_right"))
        let totalWidth = left + width + right
        guard totalWidth.isFinite, height.isFinite else { return nil }
        rect = CGRect(x: left, y: 0, width: width, height: height)
        size = CGSize(width: totalWidth, height: height)
        let tint = fill.namesTint ? host.carriedTint : nil
        guard let color = fill.channels(dark: host.drawsDark, contrast: host.drawsHighContrast, elevated: host.drawsElevated, tint: tint) else { return nil }
        self.color = color
        corners = BorderPaint.reduced(BorderPaint.radii(style, in: rect), in: rect)
        opacity = leaf.map { CGFloat($0.opacity) } ?? node?.alpha ?? 1
        tintShare = fill.tintAlpha
    }

    var source: String { "\(rect):\(size):\(color):\(corners):\(opacity):\(tintShare ?? -1)" }

    func image(dimmed: Bool) -> UIImage {
        let ink = TextEngine.color(color)
        let format = UIGraphicsImageRendererFormat.default()
        let side = Self.pixelLimit / format.scale
        let scale = min(1, side / max(size.width, size.height))
        let pixel = 1 / format.scale
        let output = CGSize(width: max(pixel, min(side, size.width * scale)),
                            height: max(pixel, min(side, size.height * scale)))
        return UIGraphicsImageRenderer(size: output, format: format).image { context in
            context.cgContext.scaleBy(x: scale, y: scale)
            context.cgContext.setAlpha(opacity)
            let fill = dimmed ? tintShare.map { ink.dimmedTint(share: $0) } ?? ink : ink
            fill.setFill()
            UIBezierPath(cgPath: BorderPaint.roundedRect(rect, corners, shape: nil)).fill()
        }.withRenderingMode(.alwaysOriginal)
    }
}
#endif

/// UIKit keeps a bar item's original bitmap unchanged when its tint dims. This view hears the
/// bar's effective tint changes so only faces containing tint ink are baked again.
final class BarFaceTintView: UIView {
    var changed: (() -> Void)?
    override func tintColorDidChange() { super.tintColorDidChange(); changed?() }
}

extension NavigationHost {
    func retintBarFaces(in nav: UINavigationController) {
        let dimmed = nav.navigationBar.tintAdjustmentMode == .dimmed
        for route in nav.viewControllers.compactMap({ $0 as? RouteController }) {
            for made in route.barItemsMade.values {
                if let face = made.item.badge, face.shares.contains(where: { $0 != nil }) {
                    made.bar.image = face.image(dimmed: dimmed)
                }
                #if os(tvOS)
                if let dot = made.item.dot, dot.tintShare != nil { made.bar.image = dot.image(dimmed: dimmed) }
                #endif
            }
        }
    }
    /// Whether two item lists are the same items, in order.
    static func same(_ a: [UIBarButtonItem]?, _ b: [UIBarButtonItem]?) -> Bool {
        (a ?? []).elementsEqual(b ?? []) { $0 === $1 }
    }

    /// The item a header's control is drawn as in a bar that shows, if it is one.
    func shownBarItem(_ id: UInt32) -> UIBarButtonItem? {
        controllers.values.lazy.filter { $0.viewIfLoaded?.window != nil }.compactMap { $0.barItemsMade[id]?.bar }.first
    }
}
#endif
