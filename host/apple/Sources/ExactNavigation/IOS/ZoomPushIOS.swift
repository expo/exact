// @ref LLP 1035.001, the zoom transition on a push: a route pushed with a
// `navigationSource` arrives by UIKit's zoom from the source element in the
// route beneath, as a full-screen presentation does (ModalIOS `present`),
// and lands on its own element with the source's id (an image, as drawn).
// iOS 18's `preferredTransition` applies to a push as to a presentation
// (UIKit, "UIViewController.Transition.zoom"); the swipe back is the
// zoom's own, which `transitionEnded` dispatches as any interactive pop.
#if os(iOS)
import UIKit
import ExactKit

extension NavigationHost {
    /// Give `top`, the route about to be pushed over `beneath`, its zoom
    /// transition when it names a `navigationSource`; nothing otherwise, and
    /// nothing before iOS 18. `pushed` is the controller UIKit pushes: the
    /// route, or a collapsed split's detail column with the route on top.
    func prepareZoomPush(_ top: RouteController, beneath: RouteController?, on pushed: UIViewController? = nil) {
        let pushed = pushed ?? top
        guard #available(iOS 18.0, *), let beneath, top.node.props["navigationSource"] != nil,
              pushed.preferredTransition == nil else { return }
        let route = top.node, preceding = beneath.node
        let options = UIViewController.Transition.ZoomOptions()
        let landing = ZoomLanding()
        // The zoom's own swipe back pops the route (`transitionEnded`
        // dispatches it); it begins only where an edge swipe may.
        options.interactiveDismissShouldBegin = { [weak self, weak pushed] context in
            guard let self, let pushed, context.willBegin else { return false }
            return landing.dismissal(begins: zoomDismissalBegins(pushed))
        }
        // A collapsed split's base can hold two landings, its own and its column's
        let appeared = top.appeared
        top.appeared = { appeared?(); landing.appeared() }
        let resolve: (UIView, NodeView?) -> CGRect? = { [weak self, weak route] zoomed, sourceView in
            guard let self, let route, let name = route.props["navigationSource"],
                  let target = presenter.views.values.filter({
                      $0.props["id"] == name && $0.window != nil && $0.isDescendant(of: route)
                  }).min(by: { $0.id < $1.id }) else { return nil }
            let source = sourceView.flatMap(ModalHost.zoomImage)
            return ModalHost.zoomAlignment(target: target, fallbackNatural: source?.raster?.image.naturalSize, in: zoomed)
        }
        options.alignmentRectProvider = { context in
            landing.answer(context.zoomedViewController.viewIfLoaded.flatMap { resolve($0, context.sourceView as? NodeView) })
        }
        pushed.preferredTransition = .zoom(options: options, sourceViewProvider: { [weak self, weak route, weak preceding, weak pushed] _ in
            guard let self, let route, let preceding, presenter.views[preceding.id] === preceding,
                  let name = route.props["navigationSource"] else { return nil }
            // A split's detail column zooms from its base's source only while it holds just that base, should UIKit
            // play a transition it took before the base changed or a route was pushed over it
            if let split, pushed === split.detail, split.detailRoutes.count != 1 || split.detailRoutes.first?.node !== route { return nil }
            return presenter.views.values.filter {
                $0.props["id"] == name && $0.window != nil && ($0 === preceding || $0.isDescendant(of: preceding))
            }.min(by: { $0.id < $1.id })
        })
    }

    /// Whether the zoom's drag-down dismissal of `pushed` may begin: where
    /// an edge swipe may, the platform's Back going and nothing moving. A
    /// collapsed split's detail column, which N0 pops whole, only while it
    /// holds just its base, as N0's back swipe (LLP 1127 D5.3): with more,
    /// the pop would take them all, past the top route's own Back. The
    /// column's base itself never, as N1 cannot pop it: N0's pop of the
    /// column is its dismissal.
    func zoomDismissalBegins(_ pushed: UIViewController) -> Bool {
        guard back != nil, !changing, !presenter.modalHost.inTransition else { return false }
        guard let split else { return true }
        if pushed === split.detail { return (split.popDepth(split.list) ?? 0) > 1 }
        return split.detail.viewControllers.first !== pushed
    }
}
#endif
