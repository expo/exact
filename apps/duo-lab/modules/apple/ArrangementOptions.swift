#if os(iOS)
import UIKit

/// The Arrangement screen's Options, set as a UIKit app sets them: title display, bar minimization
/// and the safe-area adjustment on the route's navigation item, and the bar's content scroll view
/// with `setContentScrollView(_:for:)`. Its route carries them as `data-*` words; each pane is an
/// `arrangement-pane` scroller.
final class ArrangementOptions {
    private var routes: [String: ExactRoute] = [:]
    private var panes: [String: ExactElement] = [:]
    /// Stacks whose bar starts as a plain UINavigationController's does, without large titles, as in
    /// the reference app: Automatic shows an inline title until Large turns large titles on
    private let stacks = NSHashTable<UINavigationController>.weakObjects()

    func update(_ route: ExactRoute) {
        guard let display = route.data[.titleDisplay] else { return }
        routes[route.key] = route
        guard #available(iOS 27.1, *) else { return }
        if let navigation = route.navigation?.controller, !stacks.contains(navigation) {
            stacks.add(navigation)
            navigation.navigationBar.prefersLargeTitles = false
        }
        let item = route.controller.navigationItem
        switch display {
        case "large":
            route.navigation?.controller.navigationBar.prefersLargeTitles = true
            item.largeTitleDisplayMode = .always
        case "inline": item.largeTitleDisplayMode = .never
        default: item.largeTitleDisplayMode = .automatic
        }
        item.navigationBarMinimization.minimizationBehavior = switch route.data[.barMinimization] {
        case "never": .never
        case "on-scroll-down": .onScrollDown
        case "on-scroll-up": .onScrollUp
        default: .automatic
        }
        item.navigationBarMinimization.safeAreaAdjustment = switch route.data[.safeAreaAdjustment] {
        case "enabled": .enabled
        case "disabled": .disabled
        default: .automatic
        }
        select(route)
    }

    func remove(_ route: ExactRoute) {
        if routes[route.key] === route { routes[route.key] = nil }
    }

    func update(_ element: ExactElement) {
        guard element.hatch == .arrangementPane else { return }
        element.reusable = true
        panes[element.id] = element
        for route in routes.values where route.data[.contentScroll] == element.id { select(route) }
    }

    func remove(_ element: ExactElement) {
        if element.hatch == .arrangementPane, panes[element.id] === element { panes[element.id] = nil }
    }

    /// Select the named pane's scroller for the bar, or the automatic selection when none is named
    private func select(_ route: ExactRoute) {
        let id = route.data[.contentScroll] ?? ""
        let scroll = id.isEmpty ? nil : panes[id]?.scrollView
        // Select a named pane once its scroller arrives
        if !id.isEmpty, scroll == nil { return }
        route.controller.setContentScrollView(scroll, for: .top)
    }
}
#endif
