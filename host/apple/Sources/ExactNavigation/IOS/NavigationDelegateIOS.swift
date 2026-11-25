// @ref LLP 1075.003 §3.5 — the delegate slot Exact keeps on each navigation
// controller it builds, and forwards.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// The delegate Exact gives each navigation controller it builds: Exact's
/// own handling first, then the app's delegate, which is asked first for a
/// transition's animator and receives straight anything Exact does not
/// implement (LLP 1075.003 §3.5). Each call goes to the app's delegate it
/// came to, even when Exact's handling retires the stack. A pop UIKit
/// commits reaches the router at Exact's `willShow`, before the app's; a
/// swipe, in its coordinator's completion, after both have heard `didShow`
/// (LLP 1035.001 D1, D2).
final class NavigationDelegateProxy: ForwardingDelegate, UINavigationControllerDelegate {
    weak var host: NavigationHost?
    var app: UINavigationControllerDelegate? { appDelegate as? UINavigationControllerDelegate }

    func navigationController(_ nav: UINavigationController, willShow controller: UIViewController, animated: Bool) {
        let app = self.app
        // The bar as the route coming into view wants it, alongside UIKit's
        // transition: a cancelled pop calls this again for the source (§9.10).
        host?.showBar(nav, for: controller as? RouteController, animated: animated)
        host?.followFloatingBar(nav, for: controller, animated: animated)
        host?.navigationController(nav, willShow: controller, animated: animated)
        app?.navigationController?(nav, willShow: controller, animated: animated)
    }

    func navigationController(_ nav: UINavigationController, didShow controller: UIViewController, animated: Bool) {
        let app = self.app
        // Restoring a hidden bar changes the route's safe area. Lay out its
        // container before reporting that cover to the kernel.
        nav.view.setNeedsLayout()
        if nav.viewIfLoaded?.window != nil {
            // That layout can start the bar's own update UIKit deferred to
            // the transition's end (iPadOS applies a push's items only then):
            // started inside the end, it takes the bar's touches and never
            // gives them back (UIKit gives them back when a transition ends,
            // and this one already has), so Back and a drawn title take no
            // taps. What this layout took is given back at once, before the
            // host and the app hear of the route: a bar either of them turns
            // off stays off. (An update UIKit animates takes taps a moment
            // early; one it leaves to a later turn is not this layout's.)
            let bar = nav.navigationBar, touches = bar.isUserInteractionEnabled
            nav.view.layoutIfNeeded()
            if touches, !bar.isUserInteractionEnabled { bar.isUserInteractionEnabled = true }
        }
        host?.followFloatingBar(nav, for: controller, animated: false) // (where it settled: a cancelled pop's source included)
        host?.navigationController(nav, didShow: controller, animated: animated)
        app?.navigationController?(nav, didShow: controller, animated: animated)
    }

    /// The app's animator first (LLP 1075.003 §3.5), then the one the
    /// route's own part asks for (LLP 1013.001), else UIKit's.
    func navigationController(_ nav: UINavigationController, animationControllerFor operation: UINavigationController.Operation,
                              from: UIViewController, to: UIViewController) -> UIViewControllerAnimatedTransitioning? {
        app?.navigationController?(nav, animationControllerFor: operation, from: from, to: to)
            ?? host?.routeTransition(nav, operation: operation, from: from, to: to)
    }
}
#endif
