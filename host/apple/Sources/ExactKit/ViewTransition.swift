#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

// @ref LLP 1013.003 — `startViewTransition()`: the batch lands inside a cross-fade of the whole window.
extension ExactSession {
    /// The web's default for the root's cross-fade (CSS View Transitions 1, the user agent stylesheet).
    static let crossFadeSeconds = 0.25

    /// Applies a batch that names `startViewTransition()` inside a cross-fade of the whole window, as UIKit's
    /// `UIView.transition(with: window, options: .transitionCrossDissolve)`, its own navigation unanimated.
    /// `closing`: the dialogs the batch's `close(id)` commands name, which are delivered after it.
    func crossFade(_ fades: Bool, closing: [String], _ apply: @escaping () -> Void) {
        guard fades, !ExactEnv.agentFreezes, let window = presenter.root.window else { return apply() }
        presenter.navigation.crossFading = true
        defer { presenter.navigation.crossFading = false }
        #if os(macOS)
        let fade = CATransition()
        fade.type = .fade
        fade.duration = Self.crossFadeSeconds
        window.contentView?.layer?.add(fade, forKey: kCATransition)
        apply()
        #else
        guard let root = window.rootViewController, root.presentedViewController != nil else {
            // Inside the block the batch's own frame changes would animate too.
            return UIView.transition(with: window, duration: Self.crossFadeSeconds, options: .transitionCrossDissolve) { UIView.performWithoutAnimation(apply) }
        }
        // Under a presentation the batch lands outside the block, which would animate a sheet's own content; the
        // dissolve starts from what the display last showed, and the batch is not on the display yet. A sheet or
        // dialog the batch closes slides out with its own dismissal while the controller beneath it fades; when
        // none leaves, the whole window fades, sheets and all.
        var chain = [root]
        while let next = chain[chain.count - 1].presentedViewController { chain.append(next) }
        let dialogs = closing.compactMap { id in presenter.views.values.first { $0.props["id"] == id } }
            .filter { presenter.dialogs.isOpen($0) }.flatMap { presenter.dialogs.controllers(holding: $0) }
        apply()
        let leaving = chain.indices.dropFirst().first { i in
            let sheet = chain[i]
            return sheet.isBeingDismissed || sheet.presentingViewController == nil || presenter.navigation.retires(sheet) || dialogs.contains { $0 === sheet }
        }
        let over: UIView = leaving.map { chain[$0 - 1].view } ?? window
        UIView.transition(with: over, duration: Self.crossFadeSeconds, options: .transitionCrossDissolve, animations: {})
        #endif
    }
}
