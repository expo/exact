// @ref LLP 1047.001 D10 — iOS's selection of non-editable text (LLP 1115.000
// D11.6) is a family the composition selects by `user-select`: on iOS only
// text its author made selectable selects (`textSelectable`), so a plan that
// never says `text` or `all` reaches none of it. Without the family the
// presenter holds `NoTextSelection`: a touch installs no interaction and
// nothing is selected, which is all such a plan has. `ExactTextSelection.install()`
// sets the slot. The Mac's selection is AppKit's document text, selectable
// unsaid (LLP 1115 D8), so it stays in the core there.
#if os(iOS)
import UIKit

/// What the core asks of a presenter's text selection.
package protocol TextSelecting: AnyObject {
    /// A touch hit-tested to `node`: selectable text takes its interaction
    /// now, in time for this touch.
    func touched(_ node: NodeView)
    /// The view that takes a touch hit-tested to `node`: the surface a
    /// selectable container's paragraphs share, else nil.
    func surface(for node: NodeView) -> UIView?
    /// Whether a touch hit-tested to `node` would go to such a surface, as
    /// `touched` would install it, installing nothing.
    func routesToSurface(_ node: NodeView) -> Bool
    /// The node is leaving (destroyed, or parked for another row).
    func remove(_ node: NodeView)
    /// A batch was applied.
    func batchEnded()
    /// Whether text of `node` is selected.
    func hasSelection(in node: NodeView) -> Bool
    /// Copy: `node`'s selected text to the pasteboard. False with nothing selected.
    @discardableResult
    func copy(_ node: NodeView) -> Bool
    /// Whether `gesture` is one of the text interaction's on `paragraph`,
    /// which waits for the long press of the menu above `paragraph` to fail.
    func interactionHolds(_ gesture: UIGestureRecognizer, on paragraph: NodeView) -> Bool
    #if EXACT_AGENT
    /// Nothing selected any more.
    func clear()
    /// The agent's `tap <id> dblclick` or `contextmenu` on selectable text,
    /// `point` in the window: the word there is selected, as a double tap
    /// or a long press selects it, and the reply's `selection` says what
    /// (`text`, and `start` and `end` in the node's own text). Nil when
    /// `hit` is not selectable text, has no word there, or a long press
    /// belongs to its own or a row's menu.
    func agentSelect(_ hit: UIView, at point: CGPoint, event: String) -> [String: Any]?
    #endif
}

/// Nothing selects.
package final class NoTextSelection: TextSelecting {
    package init() {}
    package func touched(_ node: NodeView) {}
    package func surface(for node: NodeView) -> UIView? { nil }
    package func routesToSurface(_ node: NodeView) -> Bool { false }
    package func remove(_ node: NodeView) {}
    package func batchEnded() {}
    package func hasSelection(in node: NodeView) -> Bool { false }
    package func copy(_ node: NodeView) -> Bool { false }
    package func interactionHolds(_ gesture: UIGestureRecognizer, on paragraph: NodeView) -> Bool { false }
    #if EXACT_AGENT
    package func clear() {}
    package func agentSelect(_ hit: UIView, at point: CGPoint, event: String) -> [String: Any]? { nil }
    #endif
}

/// The view a selectable container's paragraphs share, behind their ink:
/// hit testing sends their text touches to it, and the pointer events of a
/// touch on it are its paragraph's.
package protocol SharedSelectionSurface: UIView {
    /// The container whose paragraphs share it.
    var owner: NodeView? { get }
    /// The paragraph under `touch`.
    func paragraph(under touch: UITouch) -> NodeView?
}

extension SharedSelectionSurface {
    /// Send the pointer events of touches on the surface to its paragraphs'
    /// listeners, holding back none of the touches.
    package func observePointers() {
        let pointer = PointerRecognizer(target: nil, action: nil)
        pointer.selection = self
        pointer.cancelsTouchesInView = false
        pointer.delaysTouchesBegan = false
        pointer.delaysTouchesEnded = false
        addGestureRecognizer(pointer)
    }
}

/// The slot is set before any session is made, and read on the main thread.
package enum TextSelectionLink {
    /// A presenter's text selection.
    package static var make: ((Presenter) -> any TextSelecting)?
}
#endif
