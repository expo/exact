// @ref LLP 1094.000 D3, D4 — a node as a drag source to other apps is a
// linked capability, `drag_out`: the `ExactDragOut` module, which an app's
// composition links when its plan has a `draggable`, and installs here.
// Without it no node lifts, and a plan with a `draggable` is refused at boot
// (`Unlinked("drag_out")`).
#if os(iOS)
import UIKit

/// What the core asks of the capability.
package protocol DragOutCapability: AnyObject {
    /// A presenter's drag sources.
    func sources(_ presenter: Presenter) -> DragOutSources
}

/// A presenter's drag sources: one drag interaction on each draggable node,
/// which the node pool parks and lends with the node's view, as it does a
/// context menu's (`ContextMenuHost`).
package protocol DragOutSources: AnyObject {
    /// After a batch.
    func sync()
    func reset()
    /// Whether `interaction`, or `recognizer` on `view`, is one of the
    /// sources', which the node pool parks with the view. A drag in flight
    /// needs neither: it outlives its interaction and its view.
    func owns(_ interaction: UIInteraction, on view: UIView) -> Bool
    func owns(_ recognizer: UIGestureRecognizer, on view: UIView) -> Bool
    /// `view` parks in the node pool, its interaction still on it.
    func parked(_ view: NodeView)
    /// A parked `view` leaves the pool for good.
    func dropped(_ view: NodeView)
}

/// The drag-out module, installed by the composition.
package enum DragOutLink {
    package static var installed: DragOutCapability?
}
#endif
