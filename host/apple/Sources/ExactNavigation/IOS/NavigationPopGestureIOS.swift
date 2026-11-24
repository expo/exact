#if os(iOS) || os(tvOS)
import ExactKit
import UIKit

extension NavigationHost {
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        // The first finger of each gesture replaces the last gesture's; a second finger does not.
        if gestureRecognizer.numberOfTouches == 0 { notePopTouchDown(gestureRecognizer, at: touch.location(in: gestureRecognizer.view)) }
        return true
    }
    func notePopTouchDown(_ gestureRecognizer: UIGestureRecognizer, at point: CGPoint) { popTouchDown.setObject(NSValue(cgPoint: point), forKey: gestureRecognizer) }

    /// The swipe's start: its first touch's point, else (no touch seen) its translation's origin.
    func popStart(_ pan: UIPanGestureRecognizer, in view: UIView) -> CGPoint {
        if let down = popTouchDown.object(forKey: pan)?.cgPointValue { return down }
        let location = pan.location(in: view), delta = pan.translation(in: view)
        return CGPoint(x: location.x - delta.x, y: location.y - delta.y)
    }

    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        guard let pan = gestureRecognizer as? UIPanGestureRecognizer, let view = pan.view else {
            return popMayBegin(gestureRecognizer, from: nil, in: nil, velocity: .zero)
        }
        // A pan's velocity at shouldBegin is often zero (a finger that
        // started slowly, or one sample in): read that way every back swipe
        // is refused as not horizontal. Its travel so far says the direction.
        return popShouldBegin(pan, in: view, velocity: NavigationRules.popDirection(velocity: pan.velocity(in: view), travel: pan.translation(in: view)))
    }
    /// `shouldBegin` for a pan at `velocity`: from where its finger landed.
    func popShouldBegin(_ pan: UIPanGestureRecognizer, in view: UIView, velocity: CGPoint) -> Bool {
        popMayBegin(pan, from: popStart(pan, in: view), in: view, velocity: velocity)
    }

    /// Whether a pop recognizer's swipe, starting at `start` in `view` (a
    /// pan's) with `velocity`, may pop: the showing stack's (a tab's, or a
    /// sheet's over it), by its own depth, with an enabled back control in
    /// its active route (D1), and not a pan a `swiperight` node or a canvas
    /// owns, nor more vertical than horizontal.
    func popMayBegin(_ gestureRecognizer: UIGestureRecognizer, from start: CGPoint?, in view: UIView?, velocity: CGPoint) -> Bool {
        // tvOS has no interactive pop gesture.
        #if os(tvOS)
        let owner: UINavigationController? = nil, toList = false
        #else
        let pops = { (nav: UINavigationController) -> Bool in
            if nav.interactivePopGestureRecognizer === gestureRecognizer { return true }
            if #available(iOS 26.0, *) { return nav.interactiveContentPopGestureRecognizer === gestureRecognizer }
            return false
        }
        let owner = (allNavigations + [more].compactMap { $0 }).first(where: pops)
        // Let More's swipe back to its list from a tab's root or a plain tab begin: it is UIKit's own, and no route pops.
        // On More's list alone there is nothing to pop.
        let toList = owner != nil && owner === more && (more?.topViewController is PlainTab || (navigation === more && more?.stackRoutes.count == 1))
        #endif
        // Of a split's two stacks under one finger, the one that pops (`SplitIOS.popDepth`)
        let column = owner.flatMap { nav in modalNavigation == nil ? split?.popDepth(nav) : nil }
        if let owner, owner !== navigation, !toList, column == nil { return false }
        let depth = column ?? (toList ? owner : navigation)?.viewControllers.count ?? 0
        let goes = toList || back != nil
        guard NavigationRules.popMayBegin(depth: depth, changing: changing, modalActive: presenter.modalHost.inTransition,
                                          hasBackControl: goes,
                                          contextPreviewActive: !presenter.chrome.ids("contextTarget").isEmpty) else {
            if depth > 1, !changing, !presenter.modalHost.inTransition, !goes {
                presenter.session?.log("back gesture refused: \(backRefusal)")
            }
            return false
        }
        guard let start, let view else { return true }
        var overSwipeRight = false
        var hit = view.hitTest(start, with: nil)
        if CanvasInputs.owns(hit) { return false }
        while let current = hit {
            // A rightward pan is a `swiperight` node's, and a `swipeleft`
            // node's while its swipe is caught left of rest, on its way back.
            if let node = current as? NodeView, node.handlers.contains("swiperight")
                || (node.handlers.contains("swipeleft") && node.translate.x < -0.5) { overSwipeRight = true; break }
            if current === view { break }
            hit = current.superview
        }
        return NavigationRules.panMayBegin(startX: start.x, overSwipeRight: overSwipeRight, velocity: velocity)
    }

    /// UIKit's push or pop on `nav`, while it runs: its coordinator presents
    /// nothing (a sheet's has a presentation style; a rotation's is not the
    /// stack's) and moves the stack's top controller in or out. The top is
    /// the destination until a cancelled swipe's rollback ends, then the
    /// source again in its `didShow`.
    func pushOrPop(on nav: UINavigationController) -> UIViewControllerTransitionCoordinator? {
        guard let transition = nav.transitionCoordinator, transition.presentationStyle == .none else { return nil }
        let top = nav.topViewController
        return transition.viewController(forKey: .to) === top || transition.viewController(forKey: .from) === top ? transition : nil
    }
}
#endif
