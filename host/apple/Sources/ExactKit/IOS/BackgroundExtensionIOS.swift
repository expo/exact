// @ref LLP 1123 — `-exact-background-extension="auto"` on an image: where
// the image meets a safe area (a split's sidebar or inspector, the iPhone
// Duo's side strip), UIKit's background extension view (iOS 26) fills that
// area with the image's mirrored, blurred edges, as SwiftUI's
// `backgroundExtensionEffect()` does. The image stays where it is laid out;
// the extension is drawn around it, in the nearest scroll view or route.
#if os(iOS) || os(tvOS)
import UIKit

extension NodeView {
    /// Each image's extension view, out of `NodeView`'s stored properties,
    /// which every view pays for.
    private static let extensions = NSMapTable<NodeView, UIView>.weakToStrongObjects()

    /// The image's extension view placed, filled and sized for where the
    /// image is now; removed where it has nothing to fill. A new one shows
    /// where it goes at once; one already shown moves and resizes in the
    /// animation it is placed in (a split's column coming or going), as the
    /// image does, rather than growing from nothing or jumping to its end.
    func syncBackgroundExtension() {
        let held = Self.extensions.object(forKey: self)
        guard #available(iOS 26.0, tvOS 26.0, *), kind == "image", props["backgroundExtension"] == "auto", !cssVisibilityHidden,
              window != nil, let image = raster?.image else { return removeBackgroundExtension() }
        guard let place = extensionPlace() else {
            // Inside UIKit's animation, the safe areas are already its end's
            // while the image's layout for that end has yet to land in it: the
            // extension stays, to move there, and goes at a placement outside
            // an animation that finds the image meeting no safe area.
            if held != nil, UIView.inheritedAnimationDuration > 0 { return }
            return removeBackgroundExtension()
        }
        if let view = held as? UIBackgroundExtensionView { return placeExtension(view, place, image) }
        let view = UIBackgroundExtensionView()
        view.automaticallyPlacesContentView = false
        view.isUserInteractionEnabled = false
        view.accessibilityElementsHidden = true
        let content = UIView()
        content.clipsToBounds = true
        // The image's content box, which clips its picture, and the picture.
        let box = ExtensionLayer(owner: content)
        box.masksToBounds = true
        box.addSublayer(ExtensionLayer(owner: content))
        content.layer.addSublayer(box)
        view.contentView = content
        // What the extension draws: four bands around the image's box.
        let outside = ExtensionLayer(owner: view)
        for _ in 0..<4 {
            let band = ExtensionLayer(owner: view)
            band.backgroundColor = UIColor.black.cgColor
            outside.addSublayer(band)
        }
        view.layer.mask = outside
        Self.extensions.setObject(view, forKey: self)
        UIView.performWithoutAnimation { placeExtension(view, place, image) }
    }

    @available(iOS 26.0, tvOS 26.0, *)
    private func placeExtension(_ view: UIBackgroundExtensionView, _ place: (host: UIView, below: UIView, frame: CGRect, image: CGRect), _ image: RasterImage) {
        if view.superview !== place.host || place.host.subviews.firstIndex(of: view) != place.host.subviews.firstIndex(of: place.below).map({ $0 - 1 }) {
            place.host.insertSubview(view, belowSubview: place.below)
        }
        view.frame = place.frame
        // Outside the image's subtree, the extension takes the opacity and the
        // hiding of the image and of each ancestor between it and the host.
        let between = sequence(first: self as UIView, next: \.superview).prefix { $0 !== place.host }
        view.alpha = between.reduce(1) { $0 * $1.alpha }
        view.isHidden = between.contains { $0.isHidden }
        guard let content = view.contentView, let box = content.layer.sublayers?.first, let picture = box.sublayers?.first,
              let outside = view.layer.mask, let bands = outside.sublayers, bands.count == 4 else { return }
        let shown = place.image.offsetBy(dx: -place.frame.minX, dy: -place.frame.minY)
        content.frame = shown
        // The extension mirrors the image with its corners, and draws only
        // outside the image's box, which the image draws: nothing of the copy
        // shows behind the image's corners or through the image as it fades.
        if cornerRadii(in: bounds).contains(where: { $0 > 0 }) {
            let corners = content.layer.mask as? CAShapeLayer ?? CAShapeLayer()
            CATransaction.begin(); CATransaction.setDisableActions(true)
            corners.path = roundedPath(in: shown.offsetBy(dx: -shown.minX, dy: -shown.minY)).cgPath
            CATransaction.commit()
            if content.layer.mask !== corners { content.layer.mask = corners }
        } else if content.layer.mask != nil { content.layer.mask = nil }
        let size = place.frame.size
        outside.frame = CGRect(origin: .zero, size: size)
        bands[0].frame = CGRect(x: 0, y: 0, width: max(0, shown.minX), height: size.height)
        bands[1].frame = CGRect(x: shown.maxX, y: 0, width: max(0, size.width - shown.maxX), height: size.height)
        bands[2].frame = CGRect(x: shown.minX, y: 0, width: shown.width, height: max(0, shown.minY))
        bands[3].frame = CGRect(x: shown.minX, y: shown.maxY, width: shown.width, height: max(0, size.height - shown.maxY))
        // The pixels as the image shows them (`applyImageLayer`, `draw(_:)`):
        // an animated image's frame now, fitted by its `object-fit` in its
        // content box, which clips them, and with a `-exact-tint-color` the
        // tint through their alpha (`RasterGeometry.draw`).
        let inner = contentBox(), fit = style["object_fit"]?.string ?? "fill"
        box.frame = inner
        picture.frame = RasterGeometry.rect(natural: image.naturalSize, content: inner, fit: fit).offsetBy(dx: -inner.minX, dy: -inner.minY)
        let pixels = AnimatedRasters.shared.frame(for: self) ?? image.image
        if let tint = channels("tint_color").map({ TextEngine.color($0).cgColor }) {
            let alpha = picture.mask ?? ExtensionLayer(owner: content)
            alpha.frame = picture.bounds
            if (alpha.contents as AnyObject?) !== pixels { alpha.contents = pixels }
            picture.mask = alpha
            picture.contents = nil
            picture.backgroundColor = tint
        } else {
            picture.mask = nil
            picture.backgroundColor = nil
            if (picture.contents as AnyObject?) !== pixels { picture.contents = pixels }
        }
    }

    /// The extension is a view beside the image, outside its subtree: it
    /// goes when the image leaves the window, is destroyed or is parked.
    func removeBackgroundExtension() {
        Self.extensions.object(forKey: self)?.removeFromSuperview()
        Self.extensions.removeObject(forKey: self)
    }

    /// Where the extension goes: the nearest scroll view, else the route's
    /// view, behind the image's ancestor there; its frame the image's, grown
    /// to the route's edges on each side where the image meets the route's
    /// safe area. Nil where it meets none.
    private func extensionPlace() -> (host: UIView, below: UIView, frame: CGRect, image: CGRect)? {
        let ancestors = Array(sequence(first: self as UIView, next: \.superview))
        guard let route = ancestors.first(where: { $0.next is RouteController }),
              let host = ancestors.first(where: { $0 is UIScrollView && $0.isDescendant(of: route) }) ?? Optional(route),
              let below = ancestors.first(where: { $0.superview === host }) else { return nil }
        let image = convert(bounds, to: host)
        let edges = route.convert(route.bounds, to: host)
        let safe = route.convert(route.bounds.inset(by: route.safeAreaInsets), to: host)
        var frame = image
        if abs(image.minX - safe.minX) < 1, safe.minX > edges.minX { frame.size.width += frame.minX - edges.minX; frame.origin.x = edges.minX }
        if abs(image.maxX - safe.maxX) < 1, safe.maxX < edges.maxX { frame.size.width = edges.maxX - frame.minX }
        if abs(image.minY - safe.minY) < 1, safe.minY > edges.minY { frame.size.height += frame.minY - edges.minY; frame.origin.y = edges.minY }
        if abs(image.maxY - safe.maxY) < 1, safe.maxY < edges.maxY { frame.size.height = edges.maxY - frame.minY }
        return frame == image ? nil : (host, below, frame, image)
    }
}

extension Presenter {
    /// `view`'s opacity or display changed: the extensions of the images in
    /// it follow, as they are outside the images' subtrees.
    func compositingChanged(_ view: UIView) {
        for image in carrying("backgroundExtension") where image.isDescendant(of: view) { image.syncBackgroundExtension() }
    }
}

/// A layer of an image's extension (`syncBackgroundExtension`) that moves
/// and resizes in the animation its view's frame is set in, as that view
/// does; its other changes show at once. Core Animation asks a view for its
/// own layer's actions only, never a sublayer's or a mask's: `owner` is asked.
private final class ExtensionLayer: CALayer {
    weak var owner: UIView?
    init(owner: UIView) {
        self.owner = owner
        super.init()
    }
    override init(layer: Any) {
        owner = (layer as? ExtensionLayer)?.owner
        super.init(layer: layer)
    }
    required init?(coder: NSCoder) { nil }
    override func action(forKey event: String) -> (any CAAction)? {
        guard event == "bounds" || event == "position", let owner else { return NSNull() }
        return owner.action(for: self, forKey: event) ?? NSNull()
    }
}
#endif
