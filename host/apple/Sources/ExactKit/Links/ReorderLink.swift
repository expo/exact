// @ref LLP 1047.001 D10 — reordering a list's rows (LLP 1041 §8.5, LLP
// 1094) is the lists family's: a reorder grip or group selects it, so
// `ExactCollections` installs a presenter's reorders here, and Drag
// (`ExactDrag`) holds none of it. Without the family a presenter holds
// `NoReorders`: no grip takes a gesture, a key or an action.
#if os(macOS)
import AppKit
#else
import UIKit
#endif

package protocol NativeListReorder: AnyObject {
    func queueDrop(before: UInt32?)
    func cancel()
}

/// What the core asks of a presenter's reorders.
package protocol ReorderHosting: AnyObject {
    /// Whether a List row is lifted, its contact unsettled.
    var lifting: Bool { get }
    /// Whether `view` is the lifted row's wrapper.
    func lifts(_ view: UInt32) -> Bool
    /// Whether a reorder across a `reorderGroup`'s lists is under way.
    var groupActive: Bool { get }
    /// Cancel the active drag from the list with this authored id (LLP 1094 D8).
    func cancel(_ id: String)
    func nativeMovement(grip: NodeView, list: UInt32, finished: @escaping () -> Void) -> NativeListReorder?
    /// A batch's reorder ops, after it applies.
    func observe(_ ops: [BatchOp])
    /// The presenter's tree goes: every reorder ends.
    func abandon()
    /// Whether `view` is a grouped grip that takes the keys (LLP 1094 D9).
    func takesKeys(_ view: NodeView) -> Bool
    /// A key on a grouped grip; true when the reorder took it.
    func key(_ view: NodeView, _ name: String) -> Bool
    #if os(macOS)
    /// A grouped grip's custom actions; nil when `view` is none.
    func actions(_ view: NodeView) -> [NSAccessibilityCustomAction]?
    /// The mouse reorder, first in the mouse chain.
    var mouse: MouseDrag { get }
    #else
    /// A grouped grip's custom actions; nil when `view` is none.
    func actions(_ view: NodeView) -> [UIAccessibilityCustomAction]?
    /// A node's reorder handle, after its props changed.
    func updateGesture(_ view: NodeView)
    /// Whether a reorder recognizer on `view` begins; nil when `gesture` is
    /// not a reorder's.
    func shouldBegin(_ view: NodeView, _ gesture: UIGestureRecognizer) -> Bool?
    #endif
}

extension ReorderHosting {
    package func nativeMovement(grip: NodeView, list: UInt32, finished: @escaping () -> Void) -> NativeListReorder? { nil }
}

/// No reorders.
package final class NoReorders: ReorderHosting {
    package init() {}
    package var lifting: Bool { false }
    package func lifts(_ view: UInt32) -> Bool { false }
    package var groupActive: Bool { false }
    package func observe(_ ops: [BatchOp]) {}
    package func cancel(_ id: String) {}
    package func abandon() {}
    package func takesKeys(_ view: NodeView) -> Bool { false }
    package func key(_ view: NodeView, _ name: String) -> Bool { false }
    #if os(macOS)
    package func actions(_ view: NodeView) -> [NSAccessibilityCustomAction]? { nil }
    package let mouse: MouseDrag = NoMouseDrag()
    #else
    package func actions(_ view: NodeView) -> [UIAccessibilityCustomAction]? { nil }
    package func updateGesture(_ view: NodeView) {}
    package func shouldBegin(_ view: NodeView, _ gesture: UIGestureRecognizer) -> Bool? { nil }
    #endif
}

/// The slot is set before any session is made, and read on the main thread.
package enum ReorderLink {
    /// A presenter's reorders.
    package static var make: ((Presenter) -> any ReorderHosting)?
}

#if os(macOS)
extension NodeView {
    /// A grouped grip's reorder actions (LLP 1094 D9).
    package override func accessibilityCustomActions() -> [NSAccessibilityCustomAction]? {
        // Reading an unset NSView custom-actions property during an AX request
        // makes AppKit report kAXErrorFailure even when the handler succeeds.
        let assigned = hasAssignedCustomActions ? super.accessibilityCustomActions() : nil
        return appendingActions(to: presenter?.reorders.actions(self) ?? assigned)
    }
    package override func setAccessibilityCustomActions(_ actions: [NSAccessibilityCustomAction]?) {
        hasAssignedCustomActions = actions != nil
        super.setAccessibilityCustomActions(actions)
    }
}
#else
extension NodeView {
    /// A grouped grip's reorder actions (LLP 1094 D9).
    package override var accessibilityCustomActions: [UIAccessibilityCustomAction]? {
        get { appendingActions(to: presenter?.reorders.actions(self) ?? super.accessibilityCustomActions) }
        set { super.accessibilityCustomActions = newValue }
    }
}
#endif
