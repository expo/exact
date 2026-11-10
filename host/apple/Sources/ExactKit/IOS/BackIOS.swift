// @ref LLP 1115 D5; LLP 1035.001 D1 — the platform's Back on iOS and tvOS:
// how it goes from the selected route (the authored control, the root's
// `navigate`, or the router's own pop), and the agent's `back`, which takes
// it as a person does, More's Back to its list included (LLP 1075.003 §3.7).
#if os(iOS) || os(tvOS)
import UIKit

extension NavigationHost {
    func invokeBack(from source: NodeView) {
        guard presenter.session?.view?.window != nil,
              presenter.views[source.id] === source, routeIDs.contains(source.id),
              source.props["navigationKey"] == selectedKey else { return }
        goBack()
    }

    /// Whether the platform's Back goes from the selected route (LLP 1115 D5).
    var canInvokeBack: Bool { back != nil }

    /// The agent's `back` from a pushed route (LLP 1116.006 D4): UIKit's
    /// Back button, ending where its pop ends (`goBack`), as tvOS's Menu
    /// does; over a tab's root or a plain tab that More shows, More's Back
    /// to its list (`moreListBack`). nil when the stack in front has nothing
    /// beneath its top (a sheet's own root is the sheet's); "" when it went,
    /// else why not.
    func agentBack() -> String? {
        if let more = moreListBack {
            more.popViewController(animated: !ExactEnv.agentFreezes)
            return ""
        }
        guard (navigation?.viewControllers.count ?? 0) > 1 else { return nil }
        guard back != nil else { return backRefusal }
        goBack()
        return ""
    }

    /// More, while it shows a tab's root or a plain tab over its list with
    /// nothing presented over it (iOS): its Back pops to the list. No router
    /// visit is beneath that root, so `back` has no way there; the pop is
    /// UIKit's own, as More's Back button and its swipe are, and the router
    /// keeps the tab's root.
    var moreListBack: UINavigationController? {
        #if os(iOS)
        guard let more, presentedNavigations.isEmpty, more.viewIfLoaded?.window != nil, more.transitionCoordinator == nil,
              more.viewControllers.count == 2, more.topViewController is PlainTab || more.stackRoutes.count == 1 else { return nil }
        return more
        #else
        return nil
        #endif
    }

    /// The platform's Back, completed: the authored control pressed, else
    /// the root's `navigate` with the location beneath (LLP 1115 D5).
    func goBack() {
        switch back {
        case .press(let id)?: presenter.press(id)
        case .navigate(let location)?:
            guard let root = container, let session = presenter.session else { return }
            session.navigate(location, at: root)
        case .pop?:
            guard let key = selectedRoute?.props["navigationKey"], let session = presenter.session else { return }
            session.hostBack(key)
        case nil: break
        }
    }

    /// D1: resolved at use, by HTML id, among live enabled press controls —
    /// never captured at a gesture's start (`NavigationBack.control`).
    private var backControl: NodeView? {
        guard let route = selectedRoute else { return nil }
        let routes = routeIDs.compactMap { presenter.views[$0] }
        return NavigationBack.control(named: container?.props["navigationBack"], among: presenter.carrying("id"),
                                           id: \.id, htmlID: { $0.props["id"] }, pressable: { $0.handlers.contains("press") }, disabled: \.disabled,
                                           inActiveRoute: { $0 === route || $0.isDescendant(of: route) },
                                           inAnyRoute: { control in routes.contains { control === $0 || control.isDescendant(of: $0) } })
    }

    private var selectedRoute: NodeView? {
        guard let key = selectedKey else { return nil }
        return routeIDs.compactMap({ presenter.views[$0] }).first(where: { $0.props["navigationKey"] == key })
    }

    /// How Back goes from the selected route, resolved at use as its
    /// control is (`NavigationBack.way`, LLP 1115 D5).
    var back: NavigationBack.Way? {
        guard let root = container, let route = selectedRoute else { return nil }
        return NavigationBack.way(control: backControl?.id, declared: declaresBack(route), hearsNavigate: root.handlers.contains("navigate"),
                                    beneath: { [presenter] in route.props["navigationKey"].flatMap { presenter.session?.runtime.locationBeneath($0) } })
    }

    /// Whether a route holds an element named by the root's `navigationBack`.
    private func declaresBack(_ route: NodeView) -> Bool {
        guard let name = container?.props["navigationBack"] else { return false }
        return presenter.carrying("id").contains { $0.props["id"] == name && ($0 === route || $0.isDescendant(of: route)) }
    }

    /// Why Back does not go from the selected route, for the journal.
    var backRefusal: String {
        if let route = selectedRoute, declaresBack(route) { return "its navigationBack control is disabled or has no press handler" }
        return "no visit beneath the active route"
    }
}

#if os(tvOS)
extension NavigationHost {
    /// Whether the Siri Remote's Menu goes back: a route to pop and a Back control.
    var menuGoesBack: Bool { (navigation?.viewControllers.count ?? 0) > 1 && canInvokeBack }
    func menuBack() {
        guard menuGoesBack, !changing else { return }
        goBack()
    }
}
#endif
#endif
