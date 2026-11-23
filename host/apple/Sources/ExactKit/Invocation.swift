#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// What pressing a button does to an element it names, as HTML's button
/// activation resolves it, for the commands Exact runs: the popover commands,
/// `show-modal` and `close`. A `commandfor` that names an element governs and
/// nothing falls back, even when its `command` cannot act on that element;
/// otherwise `popovertarget` acts by `popovertargetaction`. Keywords match
/// ASCII case-insensitively and untrimmed; an invalid `popovertargetaction`
/// toggles. (HTML: the button element's activation behaviour, and the popover
/// target attribute activation behaviour.)
package struct Invocation {
    package enum Action { case show, hide, toggle, showModal, close }
    package let target: NodeView
    package let action: Action

    /// Whether it can put its target up (`show`, `toggle`, `show-modal`)
    package var opens: Bool { action != .hide && action != .close }
    /// Whether it opens a popover, as a header's menu button does:
    /// `show-modal` on a `dialog` that is also a popover is no menu
    package var opensPopover: Bool { action == .show || action == .toggle }
    /// Whether it closes `target` while that shows modally or as a popover:
    /// HTML's dialog `close` closes a modal dialog, and `hide-popover` a popover.
    func closes(_ target: NodeView, modally: Bool) -> Bool {
        self.target === target && action == (modally && target.props["semanticTag"] == "dialog" ? .close : .hide)
    }

    /// What an `id` names in the session: a view, or an element that has
    /// none (a run of a paragraph), which no command acts on.
    enum Named { case view(NodeView), element }

    /// `nil` when `source` is not a button or invokes nothing. `element`
    /// finds what an `id` names, `nil` when nothing does.
    init?(of source: NodeView, element: (String) -> Named?) {
        guard source.isButton else { return nil }
        if let name = source.props["commandfor"], !name.isEmpty, let named = element(name) {
            guard case .view(let commanded) = named else { return nil }
            let popover = commanded.props["popover"] != nil, dialog = commanded.props["semanticTag"] == "dialog"
            switch source.props["command"].map(Self.asciiLowercased) {
            case "show-popover" where popover: action = .show
            case "hide-popover" where popover: action = .hide
            case "toggle-popover" where popover: action = .toggle
            case "show-modal" where dialog: action = .showModal
            case "close" where dialog: action = .close
            default: return nil
            }
            target = commanded
            return
        }
        guard let name = source.props["popovertarget"], !name.isEmpty, case .view(let popover)? = element(name),
              popover.props["popover"] != nil else { return nil }
        switch source.props["popovertargetaction"].map(Self.asciiLowercased) {
        case "show": action = .show
        case "hide": action = .hide
        default: action = .toggle
        }
        target = popover
    }

    fileprivate static func asciiLowercased(_ s: String) -> String {
        String(String.UnicodeScalarView(s.unicodeScalars.map { (65...90).contains($0.value) ? Unicode.Scalar($0.value + 32)! : $0 }))
    }
}

extension Presenter {
    /// What `source` invokes now, resolving names among this session's views
    /// (the lowest id among several with one `id`, as `aria-*` references do)
    /// and then its paragraphs' named runs.
    package func invocation(of source: NodeView) -> Invocation? {
        Invocation(of: source, element: invocationElement)
    }

    #if !os(macOS)
    /// Contact can precede the press handler that mounts a named popover.
    /// Resolved targets retain their menu or overlay's existing feedback.
    func showsPopoverPress(of source: NodeView) -> Bool {
        if let invocation = invocation(of: source) {
            return invocation.opensPopover && !menus.pullsDown(invocation.target)
        }
        guard source.isButton else { return false }
        if let name = source.props["commandfor"], !name.isEmpty {
            if invocationElement(name) != nil { return false }
            if ["show-popover", "toggle-popover"].contains(source.props["command"].map(Invocation.asciiLowercased) ?? "") { return true }
        }
        guard let name = source.props["popovertarget"], !name.isEmpty, invocationElement(name) == nil else { return false }
        return source.props["popovertargetaction"].map(Invocation.asciiLowercased) != "hide"
    }
    #endif

    private func invocationElement(_ name: String) -> Invocation.Named? {
        if let view = chrome.named[name]?.min().flatMap({ views[$0] }) { return .view(view) }
        let run = inlineOwners.values.contains { location in
            guard let runs = views[location.owner]?.inlineText, runs.indices.contains(location.index) else { return false }
            return runs[location.index].props["id"] == name
        }
        return run ? .element : nil
    }
}


extension Presenter {
    /// HTML chooses the first button, whether or not it names a target.
    /// Inline links and native controls own their activation before that button.
    func activationButton(from source: NodeView, inline id: UInt32? = nil) -> NodeView? {
        if let id {
            var run = inlineText(id)
            while let current = run {
                if !(current.props["href"] ?? "").isEmpty { return nil }
                run = inlineText(current.parent)
            }
        }
        guard !source.inert else { return nil }
        var at: KeyPlatformView? = source
        while let view = at {
            if let node = view as? NodeView {
                if node.disabled || node.authoredInert || !(node.props["href"] ?? "").isEmpty { return nil }
                if node.isButton { return node }
                if node.kind == "control" || node.field != nil || node.textArea != nil { return nil }
            }
            #if os(macOS)
            at = eventParent(of: view)
            #else
            at = listenerParent(of: view) ?? view.superview
            #endif
        }
        return nil
    }

    func dispatchPressHandler(_ id: UInt32, from original: NodeView?) {
        if let original, original.isButton, !original.handlers.contains("press") {
            if let listener = buttonPressListener(above: original) { onPress?(listener.id) }
        } else { onPress?(id) }
    }

    /// A button's click reaches a handler above it even when the button owns
    /// its invocation, through a `portal` mover's declaration (LLP 1013.002
    /// D1). Invocation ownership stays with the originating button.
    func buttonPressListener(above source: NodeView, at point: CGPoint? = nil) -> NodeView? {
        var child: KeyPlatformView = source, crossed = false, declared = false
        func parent(of view: KeyPlatformView) -> KeyPlatformView? {
            if let parent = declaredParent(of: view) { declared = true; return parent }
            #if os(macOS)
            return eventParent(of: view)
            #else
            return listenerParent(of: view)
            #endif
        }
        while let view = parent(of: child) {
            crossed = crossed || view !== child.superview
            child = view
            guard let node = view as? NodeView else { continue }
            // Past a declaration, inertness is the shown tree's, which the hit read (LLP 1013.002 D1).
            guard !node.disabled, declared || !node.authoredInert else { return nil }
            if node.handlers.contains("press") {
                if let point, !crossed, !node.bounds.contains(node.local(point)) { return nil }
                return node
            }
        }
        return nil
    }
}

extension ExactSession {
    /// Applies a batch a person's input caused: a press, an edit, a key, a
    /// pointer's hover, press or drag. The batch carries when it was marked,
    /// so on iOS a glass it adds to a group in the window materializes, as
    /// UIKit's does when an app sets it in an animation. What `apply` lands
    /// before it (a published reply or tick) and a timer due after it carry
    /// none: their glass is drawn at once. So do events the app's own script
    /// can cause as well as a person: focus, a selection, a scroll, a
    /// dialog's close and a popover's `toggle`. Beside a staged batch the
    /// input on `id` is admitted as `admit` says (LLP 1072 T13).
    // Input without a touch hit test still installs ownership before its runner call.
    package func applyInput(_ input: @autoclosure () -> Batch, at id: UInt32? = nil) {
        guard admits(id) else { return }
        showBudgetWarning(nil)
        #if os(iOS)
        presenter.navigation.finishBootOwner()
        #endif
        if clock == nil { followOffset() } // (the input reads the date: after a sleep, a press can come before any frame)
        var batch = input()
        batch.input = CACurrentMediaTime()
        apply(batch)
    }
}

#if os(iOS) || os(tvOS)
import UIKit

extension Presenter {
    /// A hatch's act on an authored element, as the DOM's: `click()` presses
    /// it as a tap does, `focus()` and `blur()` follow the focus rules. Each
    /// is queued (`ElementHatches.later`) and runs on a later turn. False when
    /// refused.
    package func hatchAct(_ id: UInt32, _ action: UInt32) -> Bool {
        guard let node = views[id] else { return false }
        switch action {
        case 0:
            guard node.handlers.contains("press"), !node.disabled else { return false }
            // Still the node it was when asked: a reload restarts node ids.
            return elements.later(id, "click") { [weak self, weak node] in
                if let self, let node, self.views[id] === node { self.session?.log("hatch \(node.props["hatch"].map { "element \($0) #\(id)" } ?? "node #\(id)"): click (delivery: hatch)"); self.press(id) }
            }
        case 1:
            return elements.later(id, "focus") { [weak self, weak node] in
                if let self, let node, self.views[id] === node { self.focusNode(node) }
            }
        case 2:
            return elements.later(id, "blur") { [weak self, weak node] in
                if let self, let node, self.views[id] === node { _ = (node.textArea ?? node.field ?? node).resignFirstResponder() }
            }
        default: return false
        }
        return true
    }
}
#endif
