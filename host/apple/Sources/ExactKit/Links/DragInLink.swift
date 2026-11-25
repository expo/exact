// @ref LLP 1094.001 D4 — a drag into the app on UIKit is a linked capability,
// `drag_in`: the `ExactDragIn` module, which an app's composition links when
// its plan hears `drop`, `dragenter` or `dragleave`, and installs here.
// Without it no node takes a drag from another app, and such a plan is
// refused at boot (`Unlinked("drag_in")`). The Mac takes drops in its core
// (FileDragMac.swift).
#if os(iOS)
import UIKit

/// What the core asks of the capability.
package protocol DragInCapability: AnyObject {
    /// A presenter's drop target.
    func target(_ presenter: Presenter) -> DragInTarget
}

/// A presenter's drop target.
package protocol DragInTarget: AnyObject {
    /// After a batch: the window's drop interaction follows whether a node
    /// hears a drag's events, and each field's own text drop whether its
    /// node declares `drop` (D2).
    func sync()
    func reset()
    /// The drop point VoiceOver offers on `view` (D4): its own node's, or
    /// its nearest authored ancestor's that declares `drop`; none, rather
    /// than UIKit's own, where that node takes no drop now.
    func dropPoints(of view: NodeView) -> [UIAccessibilityLocationDescriptor]?
    /// The agent's `tap <target> drop|dragenter|dragleave` over `view` (D6).
    func agentDrag(_ req: [String: Any], over view: NodeView) -> [String: Any]
}

/// UIKit offers an element only the drop interactions on it or in its
/// subtree, and the one drop interaction is the window's: each element asks
/// the tracker for its node's drop point. A control's view, a paragraph's
/// words and a reader's atom ask beside their drag sources (DragOutLink.swift).
extension NodeView {
    package override var accessibilityDropPointDescriptors: [UIAccessibilityLocationDescriptor]? {
        get { super.accessibilityDropPointDescriptors ?? presenter?.dragIn?.dropPoints(of: self) }
        set { super.accessibilityDropPointDescriptors = newValue }
    }
}

extension NativeButtonIOS {
    package override var accessibilityDropPointDescriptors: [UIAccessibilityLocationDescriptor]? {
        get { super.accessibilityDropPointDescriptors ?? owner.flatMap { $0.presenter?.dragIn?.dropPoints(of: $0) } }
        set { super.accessibilityDropPointDescriptors = newValue }
    }
}

/// The button laid over a custom invoker is the element VoiceOver visits there.
extension InvokerButton {
    override var accessibilityDropPointDescriptors: [UIAccessibilityLocationDescriptor]? {
        get { super.accessibilityDropPointDescriptors ?? invoker.flatMap { $0.presenter?.dragIn?.dropPoints(of: $0) } }
        set { super.accessibilityDropPointDescriptors = newValue }
    }
}

/// The drag-in module, installed by the composition.
package enum DragInLink {
    package static var installed: DragInCapability?
}
#endif
