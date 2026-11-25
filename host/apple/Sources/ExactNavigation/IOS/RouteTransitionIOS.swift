// @ref LLP 1013.001 D2, D5, D6 — a route's own entrance and exit on UIKit:
// an animator in place of UIKit's push, pop, presentation or dismissal,
// where the route the change adds or removes declares the part. The
// arriving route is its live view, the leaving one the still `freeze` laid
// over it; only the owner moves (`RoutePart`), the other rests in the
// transition's container, and UIKit ignores input to both until the part
// ends. The edge swipe keeps UIKit's interactive pop (D5).
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// Plays one part on the route that owns the change.
final class RouteTransition: NSObject, UIViewControllerAnimatedTransitioning {
    let part: RoutePart
    /// The owner arrives (a push, a presentation) rather than leaves.
    let arriving: Bool

    init(part: RoutePart, arriving: Bool) {
        self.part = part
        self.arriving = arriving
    }

    func transitionDuration(using context: UIViewControllerContextTransitioning?) -> TimeInterval {
        part.plays ? part.seconds : 0
    }

    func animateTransition(using context: UIViewControllerContextTransitioning) {
        let container = context.containerView
        let from = context.view(forKey: .from)
        // A presentation over the full screen leaves the presenting view
        // where it is, and a dismissal's `to` is that view: neither is added.
        if let to = context.view(forKey: .to), let controller = context.viewController(forKey: .to) {
            to.frame = context.finalFrame(for: controller)
            if arriving || from == nil { container.addSubview(to) } else if let from { container.insertSubview(to, belowSubview: from) }
        }
        let owner = arriving ? context.view(forKey: .to) : from
        guard let owner, part.plays else {
            context.completeTransition(!context.transitionWasCancelled)
            return
        }
        CATransaction.begin()
        CATransaction.setCompletionBlock { [weak owner] in
            owner?.layer.removeAnimation(forKey: Self.key)
            context.completeTransition(!context.transitionWasCancelled)
        }
        owner.layer.add(part.animation(), forKey: Self.key)
        CATransaction.commit()
    }

    private static let key = "exact.route-transition"
}

/// A full-screen route's presentation and dismissal by its parts. UIKit
/// holds a transitioning delegate weakly: its controller keeps this.
final class RoutePresenting: NSObject, UIViewControllerTransitioningDelegate {
    let entrance: RoutePart?, exit: RoutePart?

    init?(entrance: RoutePart?, exit: RoutePart?) {
        guard entrance != nil || exit != nil else { return nil }
        self.entrance = entrance
        self.exit = exit
    }

    func animationController(forPresented presented: UIViewController, presenting: UIViewController,
                             source: UIViewController) -> UIViewControllerAnimatedTransitioning? {
        entrance.map { RouteTransition(part: $0, arriving: true) }
    }

    func animationController(forDismissed dismissed: UIViewController) -> UIViewControllerAnimatedTransitioning? {
        exit.map { RouteTransition(part: $0, arriving: false) }
    }
}

extension NavigationHost {
    /// The animator for a push or a pop of one route whose owner declares
    /// the part, or nil for UIKit's own: its push, its pop, and the swipe.
    func routeTransition(_ nav: UINavigationController, operation: UINavigationController.Operation,
                         from: UIViewController, to: UIViewController) -> UIViewControllerAnimatedTransitioning? {
        switch operation {
        case .push:
            guard let route = pushedRoute(to),
                  let part = presenter.session?.routePart(route.node, exit: false, size: nav.view.bounds.size) else { return nil }
            return RouteTransition(part: part, arriving: true)
        case .pop:
            #if os(iOS)
            if nav.interactivePopGestureRecognizer?.state == .began { return nil }
            #endif
            guard let part = pushedRoute(from)?.exitPart else { return nil }
            return RouteTransition(part: part, arriving: false)
        default:
            return nil
        }
    }

    /// Keep the exit the top route of `routes` declares: the route is gone
    /// from the tree by the time UIKit asks for the pop's animator. A
    /// stack's first route is never popped, but for a split's detail column,
    /// which a collapsed split pops off the list's.
    func keepExit(_ routes: [RouteController], in nav: UINavigationController?) {
        guard let top = routes.last else { return }
        let size = nav?.view.bounds.size ?? top.view.bounds.size
        let pops = routes.count > 1 || (nav != nil && nav === split?.detail)
        top.exitPart = pops ? presenter.session?.routePart(top.node, exit: true, size: size) : nil
    }
}
#endif
