// @ref LLP 1123 — `-exact-background-extension="auto"` on an image: where
// the image meets a safe area (a split's sidebar or inspector, the iPhone
// Duo's side strip), that area shows the image's mirrored, blurred edges, as
// UIKit's background extension view (iOS 26) and SwiftUI's
// `backgroundExtensionEffect()` draw them (`ExtensionView`). The image stays
// where it is laid out. The extension is the image's own bottom subview,
// grown past the image's box to the route's edges on the sides that meet its
// safe area, and it draws the picture and its mirror, so no pixel is shared
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
        let kept = (held as? ExtensionView).flatMap { $0.laidOut == boxLayout ? $0.sides : nil } ?? []
        guard let (frame, sides) = extensionFrame(keeping: kept) else {
            // Inside UIKit's animation, the safe areas are already its end's
            // while the image's layout for that end has yet to land in it: the
            // extension stays, to move there, and goes at a placement outside
            // an animation that finds the image meeting no safe area.
            if held != nil, UIView.inheritedAnimationDuration > 0 { return }
            return removeBackgroundExtension()
        }
        let view = held as? ExtensionView ?? ExtensionView()
        (view.sides, view.laidOut) = (sides, boxLayout)
        if held is ExtensionView { return placeExtension(view, frame, image) }
        Self.extensions.setObject(view, forKey: self)
        UIView.performWithoutAnimation { placeExtension(view, frame, image) }
    }

    /// Where the kernel last laid the image's box out, which only its layout moves.
    private var boxLayout: CGRect { grid?.layout ?? frame }

    /// The extension at `frame`, in the image's coordinates, drawing the
    /// image's box as the image shows it: its corners, its content box and
    /// its picture fitted by `object-fit` there, an animated image's frame
    /// now, and a `-exact-tint-color` through the picture's alpha
    /// (`applyImageLayer`, `RasterGeometry.draw`).
    private func placeExtension(_ view: ExtensionView, _ frame: CGRect, _ image: RasterImage) {
        if view.superview !== self || subviews.first !== view { insertSubview(view, at: 0) }
        view.frame = frame
        // The extension draws the picture: the image's own layer is not drawn too.
        if imageLayer?.isHidden == false { imageLayer?.isHidden = true }
        let shown = bounds.offsetBy(dx: -frame.minX, dy: -frame.minY)
        let inner = contentBox(), fit = style["object_fit"]?.string ?? "fill"
        let drawn = RasterGeometry.rect(natural: image.naturalSize, content: inner, fit: fit).offsetBy(dx: -inner.minX, dy: -inner.minY)
        let corners = cornerRadii(in: bounds).contains { $0 > 0 } ? roundedPath(in: CGRect(origin: .zero, size: bounds.size)).cgPath : nil
        view.place(shown, inner: inner, drawn: drawn, pixels: AnimatedRasters.shared.frame(for: self) ?? image.image,
                   tint: channels("tint_color").map { TextEngine.color($0).cgColor }, corners: corners)
    }

    /// The extension goes when the image leaves the window, is destroyed or
    /// is parked, or meets no safe area; the image draws its picture again.
    func removeBackgroundExtension() {
        guard let view = Self.extensions.object(forKey: self) else { return }
        view.removeFromSuperview()
        Self.extensions.removeObject(forKey: self)
        imageLayer?.isHidden = false
    }

    /// The extension's frame in the image's coordinates, and the sides it
    /// reaches: the image's box grown to the route's edges on each side where
    /// it meets the route's safe area, and on each of `kept`, the sides it
    /// reached before, while the kernel has not laid the box out again. UIKit
    /// gives a route its new size and safe area before the kernel lays the box
    /// out for them: judged against them, the old box met no safe area on a
    /// side it is about to fill again, the side went blank between the box
    /// and the route's edge, and came back sliding in from that edge as the
    /// new layout landed (folding the iPhone Duo). Only the box's own layout
    /// takes a side away. Nil where it reaches none.
    private func extensionFrame(keeping kept: UIRectEdge) -> (CGRect, UIRectEdge)? {
        guard let route = sequence(first: self as UIView, next: \.superview).first(where: { $0.next is RouteController }) else { return nil }
        let edges = route.convert(route.bounds, to: self)
        let safe = route.convert(route.bounds.inset(by: route.safeAreaInsets), to: self)
        var frame = bounds, sides: UIRectEdge = []
        if (kept.contains(.left) || abs(frame.minX - safe.minX) < 1 && safe.minX > edges.minX), edges.minX < frame.minX {
            frame.size.width += frame.minX - edges.minX; frame.origin.x = edges.minX; sides.insert(.left)
        }
        if (kept.contains(.right) || abs(frame.maxX - safe.maxX) < 1 && safe.maxX < edges.maxX), edges.maxX > frame.maxX {
            frame.size.width = edges.maxX - frame.minX; sides.insert(.right)
        }
        if (kept.contains(.top) || abs(frame.minY - safe.minY) < 1 && safe.minY > edges.minY), edges.minY < frame.minY {
            frame.size.height += frame.minY - edges.minY; frame.origin.y = edges.minY; sides.insert(.top)
        }
        if (kept.contains(.bottom) || abs(frame.maxY - safe.maxY) < 1 && safe.maxY < edges.maxY), edges.maxY > frame.maxY {
            frame.size.height = edges.maxY - frame.minY; sides.insert(.bottom)
        }
        return sides.isEmpty ? nil : (frame, sides)
    }
}

#endif
