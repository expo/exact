// Where UIKit grows a menu out of a node and returns it there: a long
// press's context menu (`ContextMenusIOS`) and a custom invoker's pull-down
// (`MenuButton`) take the node itself as their preview, so UIKit hides it
// while its menu shows, as it hides a stock button whose menu it presents.
#if os(iOS)
import UIKit

enum LiftTarget {
    /// `view`'s outline in its own coordinates where its corners are rounded,
    /// else nil. One radius, round or Apple's continuous, is UIKit's own
    /// rounded rectangle, which UIKit keeps through the lift's morph: UIKit
    /// applies any other path only after the morph's first frames, which show
    /// the node square (measured, iOS 27.1). One radius within the tolerance the
    /// layer's own (`applyBoxLayer`) has.
    static func rounded(_ view: UIView) -> UIBezierPath? {
        guard let node = view as? NodeView else {
            return view.layer.cornerRadius > 0 ? UIBezierPath(roundedRect: view.bounds, cornerRadius: view.layer.cornerRadius) : nil
        }
        let corners = node.cornerSizes(in: node.bounds), shape = node.style["corner_shape"]
        if let corner = corners.first, corner.width > 0, corners.allSatisfy({ abs($0.width - corner.width) < 0.01 && abs($0.height - corner.width) < 0.01 }),
           shape == nil || CornerShape(shape)?.isAppleContinuous == true {
            return UIBezierPath(roundedRect: node.bounds, cornerRadius: corner.width)
        }
        return corners.contains(where: { $0.width > 0 }) ? node.roundedPath(in: node.bounds) : nil
    }

    /// The view UIKit highlights, lifts and returns a preview over: the view of
    /// the controller that holds `view` (its route's, a sheet's, or the root
    /// controller's in an app with no routes), which the bars stay above.
    /// UIKit's own is the view's superview, and an ancestor that clips, such as
    /// a strip that scrolls sideways, then cuts the preview, its corners and its
    /// shadow as it grows (as it cuts a collection view's cell). The superview
    /// where no controller holds it; nil outside a window.
    static func container(of view: UIView) -> UIView? {
        guard view.window != nil, let parent = view.superview else { return nil }
        return sequence(first: parent, next: { $0.superview }).first(where: { $0.next is UIViewController }) ?? parent
    }

    /// Where UIKit highlights, lifts and returns `view` itself: over
    /// `container(of:)`, centered on the view, with the view's transform, as
    /// UIKit's own target for a view carries it (`UITargetedPreview(view:parameters:)`),
    /// and each ancestor's scale and rotation; converting the center takes the
    /// translations.
    static func target(_ view: UIView) -> UIPreviewTarget? {
        guard let container = container(of: view), let parent = view.superview else { return nil }
        var transform = view.transform
        for ancestor in sequence(first: parent, next: { $0.superview }) {
            if ancestor === container { break }
            let t = ancestor.transform
            transform = transform.concatenating(CGAffineTransform(a: t.a, b: t.b, c: t.c, d: t.d, tx: 0, ty: 0))
        }
        transform.tx = 0
        transform.ty = 0
        return UIPreviewTarget(container: container, center: view.convert(CGPoint(x: view.bounds.midX, y: view.bounds.midY), to: container), transform: transform)
    }

    /// `view` as its own preview, clipped to its corners, over nothing (a
    /// glass face is see-through); nil (UIKit's default) once it has left
    /// the window or is hidden.
    static func preview(of view: UIView) -> UITargetedPreview? {
        guard !view.isHidden, let target = target(view) else { return nil }
        let parameters = UIPreviewParameters()
        parameters.visiblePath = rounded(view)
        parameters.backgroundColor = .clear
        return UITargetedPreview(view: view, parameters: parameters, target: target)
    }
}
#endif
