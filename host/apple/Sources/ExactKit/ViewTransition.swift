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
    func crossFade(_ fades: Bool, _ apply: @escaping () -> Void) {
        guard fades, !ExactEnv.agentFreezes, let window = presenter.root.window else { return apply() }
        #if os(iOS) || os(tvOS) || os(macOS)
        presenter.navigation.crossFading = true
        defer { presenter.navigation.crossFading = false }
        #endif
        #if os(iOS) || os(tvOS)
        guard let root = window.rootViewController, root.presentedViewController != nil else {
            // Inside the block the batch's own frame changes would animate too.
            return UIView.transition(with: window, duration: Self.crossFadeSeconds, options: .transitionCrossDissolve) { UIView.performWithoutAnimation(apply) }
        }
        // Under a presentation the batch lands outside the block, which would animate a sheet's own content, and
        // what fades is chosen once the batch's commands are delivered, since `close(id)` is one of them. A
        // presentation that leaves slides out with its own dismissal while the controller beneath it cross-fades
        // from its snapshot; when none leaves, the whole window does, sheets and all.
        var chain = [root]
        while let next = chain[chain.count - 1].presentedViewController { chain.append(next) }
        let pages = chain.map { ($0, $0.view.snapshotView(afterScreenUpdates: false)) }
        let whole = window.snapshotView(afterScreenUpdates: false)
        apply()
        crossFadeEnd = { [weak self, weak window] in
            guard let self, let window else { return }
            let leaving = pages.indices.dropFirst().first { i in
                let sheet = pages[i].0
                return sheet.isBeingDismissed || sheet.presentingViewController == nil || presenter.navigation.retires(sheet)
            }
            var old = whole, over: UIView? = window
            if let leaving { (old, over) = (pages[leaving - 1].1, pages[leaving - 1].0.viewIfLoaded) }
            guard let old, let over, over === window || over.window === window else { return }
            old.frame = over.bounds
            over.addSubview(old)
            UIView.transition(with: over, duration: Self.crossFadeSeconds, options: .transitionCrossDissolve, animations: { old.removeFromSuperview() })
        }
        #elseif os(macOS)
        let fade = CATransition()
        fade.type = .fade
        fade.duration = Self.crossFadeSeconds
        window.contentView?.layer?.add(fade, forKey: kCATransition)
        apply()
        #else
        apply()
        #endif
    }
}
