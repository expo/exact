// @ref LLP 1094.000 D3, D4, D5 — a node as a drag source to other apps is a
// linked capability, `drag_out`: the `ExactDragOut` module, which an app's
// composition links when its plan has a `draggable`, and installs here.
// Without it no node lifts, and a plan with a `draggable` is refused at boot
// (`Unlinked("drag_out")`).
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

/// What the core asks of the capability.
package protocol DragOutCapability: AnyObject {
    #if os(iOS)
    /// A presenter's drag sources.
    func sources(_ presenter: Presenter) -> DragOutSources
    #elseif os(macOS)
    /// A presenter's mouse drag out of the app, last in the mouse chain's
    /// rank on one node (D8).
    func mouseDrag(_ presenter: Presenter) -> MouseRecognizer
    #endif
}

#if os(iOS)
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
    /// The drags an assistive technology offers on an element (D9): `view`
    /// itself when `isElement`, else the view that holds an element UIKit
    /// makes no view for (a paragraph's words, a link, a reader's atom). Nil
    /// where UIKit's own offer the same.
    func descriptors(of view: UIView, isElement: Bool) -> [UIAccessibilityLocationDescriptor]?
}

extension NodeView {
    /// Whether a touch `hitTest` brings to this paragraph goes to the
    /// selection surface its selectable container's paragraphs share
    /// (`hitTest`'s `.touches` branch), whose selection then has the contact.
    package var touchSelectsText: Bool { isParagraph && presenter?.textSelection.routesToSurface(self) == true }
}

/// UIKit offers an element only the drag interactions on it or in its
/// subtree (Document: `UIAccessibility.h:445-450`), and a node's is on the
/// node's view: an element inside the node, its text or a button in it, asks
/// the node's sources (D9).
extension NodeView {
    package override var accessibilityDragSourceDescriptors: [UIAccessibilityLocationDescriptor]? {
        get { super.accessibilityDragSourceDescriptors ?? presenter?.dragOut?.descriptors(of: self, isElement: true) }
        set { super.accessibilityDragSourceDescriptors = newValue }
    }
}

extension NativeButtonIOS {
    package override var accessibilityDragSourceDescriptors: [UIAccessibilityLocationDescriptor]? {
        get { super.accessibilityDragSourceDescriptors ?? owner?.presenter?.dragOut?.descriptors(of: self, isElement: true) }
        set { super.accessibilityDragSourceDescriptors = newValue }
    }
}

/// The button laid over a custom invoker is the element VoiceOver visits there.
extension InvokerButton {
    override var accessibilityDragSourceDescriptors: [UIAccessibilityLocationDescriptor]? {
        get { super.accessibilityDragSourceDescriptors ?? invoker?.presenter?.dragOut?.descriptors(of: self, isElement: true) }
        set { super.accessibilityDragSourceDescriptors = newValue }
    }
}
#endif

/// The drag-out module, installed by the composition.
package enum DragOutLink {
    package static var installed: DragOutCapability?
}
