#if os(iOS) || os(tvOS)
import UIKit

extension MenuHost {
    /// A visible native cell replaces its host-hidden authored section; CSS hiding still withdraws the invoker.
    func groupedProjectionShows(_ source: NodeView, behind view: UIView) -> Bool {
        guard let node = view as? NodeView, node.hiddenByHost, node.style["display"]?.string != "none",
              let lists = presenter?.groupedLists, lists.hides(node),
              case .view(let drawn, let port)? = lists.shown(source) else { return false }
        return drawn.window != nil && !drawn.isHidden && !port.isHidden
    }

    /// Whether `view` is in the confirmation UIKit shows: its alert or action
    /// sheet is presented over every presentation, so a presentation that
    /// covers the page the dialog is in leaves it reachable (HTML's topmost
    /// modal dialog).
    func confirmationShows(_ view: UIView) -> Bool {
        guard let owner = confirmation, !owner.finishing, let pop = owner.popover else { return false }
        return view.isDescendant(of: pop)
    }

    /// An alertdialog-shaped popover has one or more actions and at most one
    /// hide-only cancel: a chooser ("Open in…") is one action per choice.
    /// Retain this owner until native dismissal completes; ids alone are not
    /// enough because a restart can reuse them for unrelated controls.
    #if os(tvOS)
    // tvOS has no popovers; the sheet keeps the adaptive delegate only.
    typealias ConfirmationDelegate = UIAdaptivePresentationControllerDelegate
    #else
    typealias ConfirmationDelegate = UIPopoverPresentationControllerDelegate
    #endif
    /// The alert's preferred action, drawn bold and taking a hardware
    /// Return: the Mac's default button (`AlertsMac.defaultRow`, LLP 1115.000
    /// D7.3), the one `filled` action that is not destructive, when exactly
    /// one is; emphasis never picks among several. A prompt's Return is its
    /// first action that is not destructive (`PromptField.returned`), so
    /// there the filled action is preferred only when it is that one: the
    /// action drawn bold is always the one Return presses.
    static func preferredAction(_ actions: [NodeView], cancel: NodeView? = nil, prompt: Bool) -> Int? {
        let destructive = actions.map { $0.props["destructive"] == "true" }
        let filled = actions.indices.filter { !destructive[$0] && actions[$0].face?.style == "filled" }
        // A filled cancel is one of the candidates on the Mac, and never the default.
        guard filled.count == 1, prompt || cancel?.face?.style != "filled" else { return nil }
        return !prompt || destructive.firstIndex(of: false) == filled[0] ? filled[0] : nil
    }
    /// LLP 1115 D6, by the HIG: a confirmation — explanatory text, a
    /// cancel, at most three buttons — is an alert, centred, its Cancel
    /// kept (React Native's `Alert.alert`, SwiftUI's `.alert`). Anything
    /// else is an action sheet: UIKit's unanchored one on a compact screen
    /// when it has a cancel to show (UIKit draws no cancel in a popover), else a
    /// popover, which a tap outside dismisses. An alert has no outside (LLP
    /// 1115.000 D13.4), so a `dialog` of the alert's shape whose author wrote
    /// `closedby="any"` (`outside`) is the dismissible form too.
    /// Something can be chosen: an action that is choosable, or none at
    /// all, the one-button alert whose button is its cancel.
    static func choosable(_ actions: [NodeView], _ test: (NodeView) -> Bool) -> Bool {
        actions.isEmpty || actions.contains(where: test)
    }
    static func style(texts: Int, actions: Int, cancels: Int, compact: Bool, outside: Bool) -> Confirmation.Style {
        if !outside, ConfirmationShape.isAlert(texts: texts, actions: actions, cancels: cancels) { return .alert }
        #if os(tvOS)
        return .sheet
        #else
        return cancels > 0 && compact ? .sheet : .popover
        #endif
    }
    final class Confirmation: NSObject, ConfirmationDelegate {
        /// How it shows (LLP 1115 D6): `alert`, the platform's centred
        /// alert, for a confirmation (a message and a cancel, at most three
        /// buttons); `sheet`, UIKit's unanchored action sheet on a compact
        /// screen (its Cancel drawn; iOS 27 places it mid-screen), for a chooser with a cancel; `popover`, the sheet as a
        /// popover at its invoker, where a tap outside is its only way out
        /// (no cancel) or the screen is regular width.
        enum Style: String { case alert, sheet, popover }
        weak var host: MenuHost?
        weak var source: NodeView?
        let invoked: Bool
        let anchored: Bool
        /// Opened by `show-modal`, not as a popover: HTML's popover commands
        /// leave it open, and its dialog `close` closes only this mode
        let modal: Bool
        weak var popover: NodeView?
        let route: String?
        let style: Style
        let alert: UIAlertController
        var finishing = false
        var dismissing = false
        var chosen: NodeView?
        /// Ended by the person's close request (Escape, Back, a tap
        /// outside), which fires `cancel` at a modal dialog before its
        /// `close`, as HTML's does. The cancel's button is its authored
        /// `command="close"`, which fires `close` alone.
        var closeRequest = false
        /// An action as presented: the node, the title the sheet shows for
        /// it, and whether it could be chosen.
        final class Presented {
            weak var node: NodeView?
            let title: String
            /// A prompt's follows its action as the fields are typed in
            /// (`followPrompt`), as a hand-built rename's Save does.
            var enabled: Bool
            init(_ node: NodeView, title: String, enabled: Bool) { self.node = node; self.title = title; self.enabled = enabled }
        }
        let actions: [Presented]
        /// The alert's buttons for `actions`, in order.
        var buttons: [UIAlertAction] = []
        /// The hide-only cancel as presented, and the alert's button for it.
        let cancel: Presented?
        var cancelButton: UIAlertAction?
        /// A prompt's text fields (`PromptFieldIOS.swift`).
        var fields: [PromptField] = []
        init(host: MenuHost, source: NodeView?, popover: NodeView, modal: Bool, actions: [Presented], cancel: Presented?, style: Style, title: String?, message: String?) {
            self.host = host; self.source = source; self.popover = popover; self.modal = modal; self.actions = actions; self.cancel = cancel; self.style = style
            invoked = source.map { host.opens($0, popover) } ?? false
            anchored = source != nil && style == .popover
            // Own unanchored presentations by their dialog's route
            route = host.presenter?.navigation.routeKey(containing: (anchored ? source : nil) ?? popover)
            alert = UIAlertController(title: title, message: message, preferredStyle: style == .alert ? .alert : .actionSheet)
        }
        func rejectActiveProperties() {
            for entry in actions + [cancel].compactMap({ $0 }) { entry.node?.rejectNativeActiveProperties(on: "UIAlertAction") }
        }
        func observeDisappearance() {
            let lifetime = ConfirmationLifetime(frame: .zero)
            lifetime.owner = self
            alert.view.addSubview(lifetime)
        }
        func owns(_ node: NodeView) -> Bool { actions.contains { $0.node === node } }
        func adaptivePresentationStyle(for controller: UIPresentationController) -> UIModalPresentationStyle { .none }
        #if !os(tvOS)
        func popoverPresentationControllerDidDismissPopover(_ controller: UIPopoverPresentationController) {
            host?.cancelled(self, closeRequest: true)
        }
        #endif
        func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
            host?.cancelled(self, closeRequest: true)
        }
    }

}
private final class ConfirmationLifetime: UIView {
    weak var owner: MenuHost.Confirmation?
    private var mounted = false
    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window != nil { mounted = true; return }
        guard mounted else { return }
        DispatchQueue.main.async { [weak self] in
            guard let owner = self?.owner, owner.alert.presentingViewController == nil, !owner.finishing else { return }
            owner.host?.cancelled(owner, closeRequest: false)
        }
    }
}

#endif
