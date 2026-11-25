// What a menu row shows in UIKit's menu (LLP 1021 D3): its title, subtitle
// and image, read from the row's nodes, which a closed menu never lays out.

#if os(iOS) || os(tvOS)
import UIKit

extension MenuHost {
    /// A row's item image: its symbol, custom or native (LLP 1069.011.000
    /// D5), else its `img` — a provider's own icon — once that has loaded.
    /// The menu reads what the hidden row already holds; opening it never
    /// fetches, and an image still loading is no image until the next open.
    func image(of row: NodeView) -> UIImage? {
        guard row.isButton else { return nil }
        if let symbol = row.face?.symbol { return FaceSymbol.image(symbol) }
        guard !row.isNativeButton, let img = Self.firstImage(in: row) else { return nil }
        if let symbol = img.image, img.imageSource?.hasPrefix("symbol:") == true { return symbol }
        return img.raster.map { Self.rowImage($0.image.image) }
    }
    /// A menu row draws an image at its own size: fit it in the row's icon
    /// box, keeping its ratio.
    static func rowImage(_ bitmap: CGImage) -> UIImage {
        let side: CGFloat = 24, natural = CGSize(width: bitmap.width, height: bitmap.height)
        let scale = min(side / max(natural.width, 1), side / max(natural.height, 1))
        let size = CGSize(width: natural.width * scale, height: natural.height * scale)
        return UIGraphicsImageRenderer(size: size).image { _ in
            UIImage(cgImage: bitmap).draw(in: CGRect(origin: .zero, size: size))
        }.withRenderingMode(.alwaysOriginal)
    }
    private static func firstImage(in view: UIView) -> NodeView? {
        for case let node as NodeView in (view as? NodeView)?.container.subviews ?? view.subviews {
            if node.kind == "image" { return node }
            if let found = firstImage(in: node) { return found }
        }
        return nil
    }

    package func title(of v: NodeView) -> String {
        if v.kind == "text" { return v.paragraphSpec().runs.map(\.text).joined() }
        // A native button's children are its face, not views: its title, else its label.
        if v.isNativeButton { return v.face?.shown ?? "" }
        // A custom button whose face fits shows it too: a symbol-only row its
        // label (LLP 1069.011.000 D5); other content keeps its text.
        if v.isButton, let face = v.face, face.fits, let shown = face.shown { return shown }
        // Its accessible name: an `aria-hidden` child (a submenu row's `›`,
        // which the platform menu draws itself) is not part of it.
        return v.container.subviews
            .compactMap { $0 as? NodeView }
            .filter { $0.props["accessibilityElementsHidden"] != "true" }
            .map(title(of:))
            .filter { !$0.isEmpty }
            .joined(separator: " ")
    }
}
#endif
