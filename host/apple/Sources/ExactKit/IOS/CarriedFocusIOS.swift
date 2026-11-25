// A `portal` mover's views move to their destination as themselves (LLP
// 1013.002 D5), and UIKit resigns a first responder whose view leaves its
// superview, which AppKit does not. So a batch that moves the focused node
// or an ancestor to another parent carries the focus across: that node's
// blur, focus and edit commit wait while the batch applies, and as it ends
// the same responder takes the focus back, with its selection, if it is still
// in the window and nothing else took the focus. Otherwise the blur and
// commit it held are delivered: one loss, before any other node's focus, and
// no replay of UIKit's intermediate callbacks. tvOS's focus engine moves its
// own focus (QUEUE).
#if os(iOS)
import ExactCore
import UIKit

struct CarriedFocus {
    let id: UInt32
    weak var node: NodeView?
    weak var responder: UIView?
    let selection: UITextRange?
    /// UIKit resigned it while the batch applied.
    var resigned = false
    /// The edit commit its resigning asked for.
    var commit: (value: String, change: Bool)?
}

extension Presenter {
    /// The focus a batch carries when it moves the focused node's view or an
    /// ancestor's to another parent.
    func carryFocus(_ batch: Batch) {
        carriedFocus = nil
        guard let id = focusEvents.focused, let node = textHost(id), node.window != nil,
              let responder = [node.field, node.textArea, node].compactMap({ $0 }).first(where: \.isFirstResponder) else { return }
        var ancestors: [ObjectIdentifier: UIView?] = [:]
        var at: UIView? = node
        while let view = at { ancestors[ObjectIdentifier(view)] = view.superview; at = view.superview }
        let moves = batch.ops.contains { op in
            op.op == .children && op.ids.contains { child in
                guard let view = views[child], let was = ancestors[ObjectIdentifier(view)] else { return false }
                return was !== views[op.id]?.container
            }
        }
        guard moves else { return }
        carriedFocus = CarriedFocus(id: id, node: node, responder: responder, selection: (responder as? UITextInput)?.selectedTextRange)
    }

    /// Whether `id`'s focus callback waits for the batch's end (`carryFocus`).
    func carriesFocus(_ id: UInt32) -> Bool { carriedFocus?.id == id }

    /// The batch has applied: the carried focus returns, or its loss is said.
    func landFocus() {
        guard let carried = carriedFocus else { return }
        if carried.resigned, focusEvents.focused == carried.id, let node = carried.node,
           views[carried.id] === node || textHost(carried.id) === node,
           let responder = carried.responder, responder.window != nil, responder.isDescendant(of: node),
           FirstResponder.current.map({ $0 === session?.view }) ?? true, responder.becomeFirstResponder() {
            if let selection = carried.selection, let input = responder as? UITextInput { input.selectedTextRange = selection }
            carriedFocus = nil
            return
        }
        releaseCarriedFocus()
    }

    /// What the carried focus held, delivered: its commit, then its blur if
    /// UIKit took the focus from it. Before another node's focus, in the
    /// web's order.
    func releaseCarriedFocus() {
        guard let carried = carriedFocus else { return }
        carriedFocus = nil
        if let commit = carried.commit { commitEdit(carried.id, commit.value, change: commit.change) }
        if carried.resigned { blur(carried.id) }
    }
}
#endif
