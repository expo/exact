#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

extension ExactSession {
    /// `startViewTransition(seconds)`: the batch lands inside a cross-fade of the whole window, as UIKit's
    /// `UIView.transition(with: window, options: .transitionCrossDissolve)`, its own navigation unanimated.
    func crossFade(_ seconds: Double?, _ apply: @escaping () -> Void) {
        guard let seconds, !ExactEnv.agentFreezes, let window = presenter.root.window else { return apply() }
        #if os(iOS) || os(tvOS)
        presenter.navigation.crossFading = true
        defer { presenter.navigation.crossFading = false }
        // Inside the block the batch's own frame changes would animate too.
        let still = { UIView.performWithoutAnimation(apply) }
        guard let root = window.rootViewController, root.presentedViewController != nil,
              let page = root.view.snapshotView(afterScreenUpdates: false),
              let whole = window.snapshotView(afterScreenUpdates: false) else {
            return UIView.transition(with: window, duration: seconds, options: .transitionCrossDissolve, animations: still)
        }
        // Under a presentation the batch lands outside the block, which would animate the sheet's own content.
        // One it closes slides out while the page beneath cross-fades from its snapshot (the sheet's container
        // is the window's, outside the page); one that stays fades with the rest of the window.
        apply()
        let (old, over) = presenter.navigation.closesPresentation ? (page, root.view!) : (whole, window as UIView)
        old.frame = over.bounds
        over.addSubview(old)
        UIView.transition(with: over, duration: seconds, options: .transitionCrossDissolve, animations: { old.removeFromSuperview() })
        #elseif os(macOS)
        let fade = CATransition()
        fade.type = .fade
        fade.duration = seconds
        window.contentView?.layer?.add(fade, forKey: "exact.viewTransition")
        apply()
        #else
        apply()
        #endif
    }
}
