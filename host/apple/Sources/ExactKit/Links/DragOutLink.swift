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
    /// Whether a press on `node` would arm a drag (D8), which a first click
    /// in a window that is not key may do alone (D5, "From an inactive window").
    func arms(_ node: NodeView, in presenter: Presenter) -> Bool
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

#if os(macOS)
/// AppKit's source of a drag a node of the app began: the drop side finds
/// the drag's `OwnDrag` through the `draggingSource` it reports.
package protocol OwnDragSource: AnyObject {
    var own: OwnDrag? { get }
}
#endif

/// A drag a node of the app began, as the app's own drop side meets it
/// (LLP 1094.000 D16): the source's `dragstart` goes before the drop side's
/// first event of the drag, and its `dragend` after the drop side's last,
/// each of its drops delivered or failed and every leave it owes sent. UIKit
/// carries it as the drag session's `localContext`, AppKit as its source.
package final class OwnDrag {
    private var start: (() -> Void)?
    private var end: ((Bool) -> Void)?
    /// The drops of the drag still loading, which hold its `dragend`.
    private var loading = 0
    /// What each drop side that follows the drag sends when it ends, as a
    /// leave it would send after a delay.
    private var owed: [ObjectIdentifier: () -> Void] = [:]
    /// Whether the app's own drop of the drag was delivered: the outcome then,
    /// over what the platform reports, as the platform's comes before the
    /// drop's items arrive.
    private var delivered: Bool?

    /// `start` runs on the main queue, after the platform's callback that
    /// began the drag returns, unless the drop side runs it first.
    package init(start: @escaping () -> Void) {
        self.start = start
        DispatchQueue.main.async { [weak self] in self?.starting() }
    }

    /// The source's `dragstart`, once: the drop side runs it before its own
    /// first event of the drag, after answering the platform's question.
    package func starting() {
        guard let start else { return }
        self.start = nil
        start()
    }

    /// `side`, which follows the drag, sends `finish` when it ends, or
    /// nothing once it calls `owes(nil, by:)`.
    package func owes(_ finish: (() -> Void)?, by side: AnyObject) { owed[ObjectIdentifier(side)] = finish }

    /// A drop of the drag loads its items until `landed`.
    package func loads() { loading += 1 }

    package func landed(delivered: Bool) {
        self.delivered = delivered
        loading -= 1
        if loading == 0, let end { self.end = nil; end(delivered) }
    }

    /// The app's own drop of the drag resolved at once.
    package func dropped(delivered: Bool) { self.delivered = delivered }

    /// The platform ended the drag, a destination taking it or not
    /// (`taken`): `end` hears the outcome once each drop side has sent what
    /// it owes and no drop of the drag still loads. Its `dragstart` goes
    /// first if it has not.
    package func ended(taken: Bool, _ end: @escaping (Bool) -> Void) {
        starting()
        let owed = self.owed.values
        self.owed = [:]
        owed.forEach { $0() }
        if loading == 0 { end(delivered ?? taken) } else { self.end = end }
    }
}
