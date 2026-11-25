#if os(iOS)
import UIKit

// LLP 1057.001 §8 on UIKit: a node's `swipe` is UIKit's discrete swipe, a
// `UISwipeGestureRecognizer` in the node's `swipe-direction`. It has no
// delegate, so it hears the touches of every view inside the node, a field's
// included, and UIKit arbitrates it with the recognizers on the touch's path.
extension NodeView {
    /// Makes the recognizer the node's `swipe` and `swipe-direction` ask for,
    /// again when the direction changes, and removes it when either goes.
    func updateDiscreteSwipe() {
        let direction = handlers.contains("swipe") ? Self.swipeDirections[props["swipeDirection"] ?? ""] : nil
        guard discreteSwipe?.direction != direction else { return }
        if let old = discreteSwipe { removeGestureRecognizer(old) }
        discreteSwipe = direction.map { direction in
            let gesture = UISwipeGestureRecognizer(target: self, action: #selector(discreteSwiped))
            gesture.direction = direction
            addGestureRecognizer(gesture)
            return gesture
        }
    }
    @objc func discreteSwiped() {
        guard let session = presenter?.session else { return }
        session.applyInput(session.runtime.swipe(id, .discrete, now: session.now()), at: id)
    }
    static let swipeDirections: [String: UISwipeGestureRecognizer.Direction] = ["left": .left, "right": .right, "up": .up, "down": .down]
}
#endif
