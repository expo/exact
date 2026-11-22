// @ref LLP 1035.001 D1/D6 — the web's hide-and-inert rule, as input and
// accessibility see it: a hidden or inert node (authored, a route's, behind a
// dialog) takes no input and leaves the accessibility tree. Every plan has it;
// navigation feeds it the routes and panels it hides.
#if os(macOS)
import AppKit

package final class InputGates {
    private unowned let presenter: Presenter
    /// Each node's gates as last applied: hidden, route-inert, `inert`, `aria-hidden`.
    private var gates: [UInt32: [Bool]] = [:]
    /// The ops that can change what hides or disables a node: a frame, a
    /// paint or a presentation value (every frame of an animation) cannot,
    /// and each write posts an accessibility notification.
    private static let gating: Set<BatchOp.Kind> = [.create, .props, .style, .children, .roots]
    init(_ presenter: Presenter) { self.presenter = presenter }

    func reset() { gates.removeAll() }

    /// The nodes a batch can have changed the gates of, and the subtrees
    /// whose ancestry it changed (`reparented`, and the roots it names).
    package func touched(by batch: Batch, reparented: Set<UInt32>) -> (touched: Set<UInt32>, subtrees: Set<UInt32>) {
        var touched = Set<UInt32>(), subtrees = reparented
        for op in batch.ops {
            if let id = op.nodeID {
                if op.op == .destroy { gates.removeValue(forKey: id) }
                else if Self.gating.contains(op.op) { touched.insert(id) }
            }
            // A children op names retained siblings too. Only changed ancestry
            // can change their inherited gates; a list append must not revisit
            // every paragraph and cell already mounted in the window.
            if op.op == .roots { subtrees.formUnion(op.ids) }
        }
        return (touched, subtrees)
    }

    /// Apply the gates of `touched` and of everything under `subtrees`, and
    /// of `placeholder`, a view out of its authored ancestors' views (the
    /// split pane's), when one of those ancestors is in `subtrees`.
    package func settle(touched: Set<UInt32>, subtrees: Set<UInt32>, placeholder: NSView?) {
        var touched = touched, subtrees = subtrees
        // Reuse the presenter's ancestor inert gate for focus/input. AppKit's
        // accessibility subtree is suppressed as HTML inert suppresses it.
        for id in touched {
            guard let node = presenter.views[id] else { continue }
            let gate = [node.isHidden, node.routeInert, node.props["inert"] == "true", node.props["accessibilityElementsHidden"] == "true"]
            if let old = gates[id], old != gate { subtrees.insert(id) }
            gates[id] = gate
        }
        func descendants(_ view: NSView) {
            if let node = view as? NodeView { touched.insert(node.id) }
            for child in view.subviews { descendants(child) }
        }
        for id in subtrees { if let node = presenter.views[id] { descendants(node) } }
        // The placeholder the split's pane shows is out of its route's views: its gates follow its authored ancestors'.
        if let lifted = placeholder, !subtrees.isEmpty,
           presenter.logicalAncestry(of: lifted).contains(where: { ($0 as? NodeView).map { subtrees.contains($0.id) } ?? false }) {
            descendants(lifted)
        }
        for id in touched {
            guard let node = presenter.views[id] else { continue }
            gates[id] = [node.isHidden, node.routeInert, node.props["inert"] == "true", node.props["accessibilityElementsHidden"] == "true"]
        }
        refresh(touched.compactMap { presenter.views[$0] })
    }

    /// A prepared node's gates, read where it stands under live ancestors (LLP 1072 T13).
    func prepared(_ node: NodeView) {
        gates[node.id] = [node.isHidden, node.routeInert, node.props["inert"] == "true", node.props["accessibilityElementsHidden"] == "true"]
        refresh([node])
    }

    /// Top-layer ownership changes the same native input/AX gates as inert.
    /// The focus leaves a node input no longer reaches (a hidden view, an
    /// inert one: authored, a route's, behind a dialog) and a control that
    /// became disabled (HTML's focus fixup rule), whose place Tab goes on
    /// from (`Presenter.tabStart`). `aria-hidden` takes a node out of the
    /// accessibility tree only, and the focus stays, as in Chrome.
    func refresh(_ nodes: [NodeView]) {
        // Nothing below changes what makes a node inert: the ancestors' answers are asked once, as a hit test asks them.
        let outermost = NodeView.hitInert == nil
        if outermost { NodeView.hitInert = [:] }
        defer { if outermost { NodeView.hitInert = nil } }
        for node in nodes {
            let inert = node.inert
            let unreachable = inert || node.isHiddenOrHasHiddenAncestor
            // Read before a field is disabled below, which can end its editing.
            let window = node.window, responder = window?.firstResponder as? NSView
            let focused = responder.map { $0 === node || $0.isDescendant(of: node) || node.field?.currentEditor() === $0 } ?? false
            // The AX getter can traverse the legacy unsupported-attribute path
            // for non-elements; writing this property is cheaper than querying it.
            // Each write posts a notification, changed or not.
            let hidden = node.accessibilityInert || node.isHiddenOrHasHiddenAncestor || node.accessibilityHiddenByProp
            if node.accessibilityHiddenWritten != hidden { node.setAccessibilityHidden(hidden) }
            let enabled = !node.disabled && !inert
            if let field = node.field, field.isEnabled != enabled { field.isEnabled = enabled }
            let editable = enabled && node.props["editable"] != "false"
            if let area = node.textArea, area.isEditable != editable { area.isEditable = editable }
            guard focused, unreachable || node.formDisabled, let window else { continue }
            if !unreachable {
                presenter.tabStart = presenter.keyView(of: node)
                presenter.keyViewLoopStale = true
            }
            // A field that was disabled may have ended its editing already.
            if window.firstResponder === responder { window.makeFirstResponder(nil) }
        }
    }
}

extension Presenter {
    /// The view over `view` in the authored tree: an open popover's authored
    /// parent, and for a subtree the split's pane holds, its home in the
    /// page; else its superview. What inerts or hides a node from
    /// assistive technology is read up it.
    func authoredSuperview(of view: NSView) -> NSView? {
        if let cell = view as? DataTableCell { return cell.home }
        if let pane = navigation.stackSplit.detailPane, view.superview === pane, navigation.stackSplit.active, let node = view as? NodeView {
            return blocks.parent(of: node)
        }
        return menus.parent(of: view)
    }
}
#endif
