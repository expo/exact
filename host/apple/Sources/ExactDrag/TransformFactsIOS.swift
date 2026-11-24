// A transform drag's contact and geometry facts on UIKit (`TransformGeometry`).
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// The photo handle's one contact (LLP 1057.001 §4): the binding's pan, now up
/// to two fingers, and a `UIPinchGestureRecognizer`, simultaneous with it —
/// the one built-in simultaneity (precedence rule 6). Both feed the same paired
/// `TransformDragHold`. Every sample is the pair anchored at the contact's
/// focal point; the anchor moves whenever a recognizer starts or stops or the
/// finger count changes, which also absorbs UIKit's centroid jump.
final class TransformContact: TransformPinchContact {
    #if os(tvOS)
    // tvOS has no pinch; nothing makes a contact there.
    let pinch: UIGestureRecognizer
    #else
    let pinch: UIPinchGestureRecognizer
    #endif
    var pinchRecognizer: UIGestureRecognizer { pinch }
    var panning = false, pinching = false
    /// Where the pan's finger touched down, in window points, while no other
    /// finger has joined it: a touch that arrives while another is still down
    /// clears it, whether the recognizer's `numberOfTouches` counts that one
    /// yet or not.
    private(set) var panDown: CGPoint?
    private let fingers = NSHashTable<UITouch>.weakObjects()
    func panReceived(_ touch: UITouch, at point: CGPoint) {
        let joined = fingers.allObjects.contains { $0 !== touch && [.began, .moved, .stationary].contains($0.phase) }
        panDown = joined ? nil : point
        fingers.add(touch)
    }
    /// The pair's value, the focal point (window points) and pinch scale at the anchor.
    var anchor: (value: TransformDragPosition, focal: CGPoint, scale: CGFloat, touches: Int)?
    #if os(tvOS)
    init(_ pinch: UIGestureRecognizer) { self.pinch = pinch }
    #else
    init(_ pinch: UIPinchGestureRecognizer) { self.pinch = pinch }
    #endif
}

extension NodeView {
    /// The core keeps a handle's contact untyped (`transformContact`).
    var contact: TransformContact? {
        get { transformContact as? TransformContact }
        set { transformContact = newValue }
    }
}

extension Presenter {
    func transformFacts(_ binding: TransformDragBinding) -> TransformGeometryFacts? {
        guard let handle = views[binding.id], let targetID = binding.target, let clipID = binding.clip,
              let target = views[targetID], let clip = views[clipID], target.superview === clip,
              let window = clip.window, handle.window === window, target.window === window,
              SwipeInput.allows(handle), SwipeInput.allows(target), SwipeInput.allows(clip) else { return nil }
        var ancestor: UIView? = handle
        while let view = ancestor {
            if view !== target {
                if !view.transform.isIdentity || !view.layer.affineTransform().isIdentity
                    || !(view.layer.presentation()?.affineTransform().isIdentity ?? true) { return nil }
                if (view.layer.animationKeys() ?? []).contains(where: { $0.contains("transform") || $0.contains("position") || $0.contains("bounds") }) { return nil }
            } else if target.rotate != 0 || !target.contextTransform.isIdentity { return nil }
            ancestor = view.superview
        }
        // UIView.frame is undefined under a nonidentity transform. Bounds and
        // center retain the untransformed layout box in its parent's coordinates.
        let frame = CGRect(x: target.center.x - target.bounds.width / 2, y: target.center.y - target.bounds.height / 2,
            width: target.bounds.width, height: target.bounds.height)
        return TransformGeometryFacts(targetBounds: target.bounds, targetFrame: frame,
            clipBounds: clip.bounds, windowOrigin: clip.convert(.zero, to: window), supportedAncestors: true)
    }
}
#endif
