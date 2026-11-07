// @ref LLP 1123 — `-exact-background-extension="auto"` on an image: where
// the image meets a safe area (a split's sidebar or inspector, the iPhone
// Duo's side strip), UIKit's background extension view (iOS 26) fills that
// area with the image's mirrored, blurred edges, as SwiftUI's
// `backgroundExtensionEffect()` does. The image stays where it is laid out.
// As in a hand-built UIKit screen, the picture is the extension view's
// content: the view is the image's own bottom subview, grown past the
// image's box to the route's edges on the sides that meet its safe area, and
// it draws the picture and its mirror as one surface, so no pixel is shared
// by the picture's edge and the mirror's. Being in the image's subtree, it
// moves, fades and hides with the image and its ancestors, inside whatever
// animation moves them (a column's, a press's).
#if os(iOS) || os(tvOS)
import UIKit

extension NodeView {
    /// PROTO (safe-area design r6 §3): `-exact-safe-area="extend"` (the old property's `auto` until migrated).
    var extendsBackground: Bool {
        props["backgroundExtension"] == "auto" || (props["safeArea"]?.split(separator: ",").contains { $0.split(separator: " ").first == "extend" } ?? false)
    }

    /// Each image's extension view, out of `NodeView`'s stored properties,
    /// which every view pays for.
    private static let extensions = NSMapTable<NodeView, UIView>.weakToStrongObjects()

    /// The image's extension view placed, filled and sized for where the
    /// image is now; removed where it has nothing to fill. A new one shows
    /// where it goes at once; one already shown resizes in the animation it
    /// is placed in (a split's column coming or going), as the image does,
    /// rather than growing from nothing or jumping to its end.
    func syncBackgroundExtension() {
        let held = Self.extensions.object(forKey: self)
        // A flight shows the whole image on its own layer (LLP 1013.000 D4).
        guard #available(iOS 26.0, tvOS 26.0, *), kind == "image", extendsBackground, !cssVisibilityHidden,
              flightLook == nil, window != nil, let image = raster?.image else { return removeBackgroundExtension() }
        if let route = ViewPresentation.controller(for: self) as? RouteController, route.safeAreaNeedsLayout { return }
        guard let frame = extensionFrame() else {
            // Inside UIKit's animation, the safe areas are already its end's
            // while the image's layout for that end has yet to land in it: the
            // extension stays, to move there, and goes at a placement outside
            // an animation that finds the image meeting no safe area.
            if held != nil, UIView.inheritedAnimationDuration > 0 { return }
            return removeBackgroundExtension()
        }
        if let view = held as? UIBackgroundExtensionView { return placeExtension(view, frame, image) }
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
        Self.extensions.setObject(view, forKey: self)
        UIView.performWithoutAnimation { placeExtension(view, frame, image) }
    }

    @available(iOS 26.0, tvOS 26.0, *)
    private func placeExtension(_ view: UIBackgroundExtensionView, _ frame: CGRect, _ image: RasterImage) {
        if view.superview !== self || subviews.first !== view { insertSubview(view, at: 0) }
        view.frame = frame
        // The extension draws the picture: the image's own layer is not drawn too.
        if imageLayer?.isHidden == false { imageLayer?.isHidden = true }
        guard let content = view.contentView, let box = content.layer.sublayers?.first, let picture = box.sublayers?.first else { return }
        // The image's box, in the extension view, holds the picture as the image shows it.
        let shown = bounds.offsetBy(dx: -frame.minX, dy: -frame.minY)
        content.frame = shown
        // The extension mirrors the image with its corners: nothing of the
        // picture shows past the image's rounded corners.
        if cornerRadii(in: bounds).contains(where: { $0 > 0 }) {
            let corners = content.layer.mask as? CAShapeLayer ?? CAShapeLayer()
            CATransaction.begin(); CATransaction.setDisableActions(true)
            corners.path = roundedPath(in: shown.offsetBy(dx: -shown.minX, dy: -shown.minY)).cgPath
            CATransaction.commit()
            if content.layer.mask !== corners { content.layer.mask = corners }
        } else if content.layer.mask != nil { content.layer.mask = nil }
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

    /// The extension goes when the image leaves the window, is destroyed or
    /// is parked, or meets no safe area; the image draws its picture again.
    func removeBackgroundExtension() {
        guard let view = Self.extensions.object(forKey: self) else { return }
        view.removeFromSuperview()
        Self.extensions.removeObject(forKey: self)
        imageLayer?.isHidden = false
    }

    /// The extension's frame in the image's coordinates: the image's box,
    /// grown to the route's edges on each side where it meets the route's
    /// safe area. Nil where it meets none.
    private func extensionFrame() -> CGRect? {
        guard let route = sequence(first: self as UIView, next: \.superview).first(where: { $0.next is RouteController }) else { return nil }
        let edges = route.convert(route.bounds, to: self)
        let safe = route.convert(route.bounds.inset(by: route.safeAreaInsets), to: self)
        var frame = bounds
        if abs(frame.minX - safe.minX) < 1, safe.minX > edges.minX { frame.size.width += frame.minX - edges.minX; frame.origin.x = edges.minX }
        if abs(frame.maxX - safe.maxX) < 1, safe.maxX < edges.maxX { frame.size.width = edges.maxX - frame.minX }
        if abs(frame.minY - safe.minY) < 1, safe.minY > edges.minY { frame.size.height += frame.minY - edges.minY; frame.origin.y = edges.minY }
        if abs(frame.maxY - safe.maxY) < 1, safe.maxY < edges.maxY { frame.size.height = edges.maxY - frame.minY }
        return frame == bounds ? nil : frame
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
