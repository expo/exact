// @ref LLP 1075.003 §9.10 — UIKit reads each history row from the route's own item.
#if os(iOS)
import CExact.BackHistory
import ExactKit
import UIKit

struct BackHistory {
    var title: String?
    var mode: UINavigationItem.BackButtonDisplayMode = .default

    private static func usable(_ label: String?) -> String? {
        guard let label, !label.unicodeScalars.allSatisfy({ exact_history_blank_scalar($0.value) != 0 }) else { return nil }
        return label
    }

    init(authored: String?, heading: String?, head: String?) {
        if authored == "" { mode = .minimal }
        else { title = authored }
        if Self.usable(heading) == nil, Self.usable(authored) == nil, let head = Self.usable(head) {
            title = head
            if mode == .default { mode = .generic }
        }
    }
}

extension NavigationHost {
    @discardableResult
    func projectBack(_ route: RouteController, before successor: RouteController?, heading: String?, in nav: UINavigationController) -> Bool {
        guard controllers[route.node.id] === route, presenter.views[route.node.id] === route.node else { return false }
        let control = successor.flatMap { next in
            barShows(nav) ? container?.props["navigationBack"].flatMap { name in
                presenter.carrying("id").first { $0.props["id"] == name && presenter.authored($0, inside: next.node) }
            } : nil
        }
        control?.rejectNativeActiveProperties(on: "UIKit back button")
        let desired = BackHistory(authored: control.map { HeaderShape.Item($0).title },
                                  heading: heading, head: route.node.props["headTitle"])
        let old = route.backHistory
        // Leave UIKit's item alone until there is a title or mode to project.
        let titleChanged = old?.title != desired.title
        let modeChanged = (old?.mode ?? .default) != desired.mode
        guard titleChanged || modeChanged else { return false }
        route.backHistory = desired
        if titleChanged { route.navigationItem.backButtonTitle = desired.title }
        if modeChanged { route.navigationItem.backButtonDisplayMode = desired.mode }
        route.projected = nil
        return true
    }

    /// A nested detail controller and UIKit's More items are native neighbors,
    /// with no authored successor control of their own.
    func reconcileBackHistory(in nav: UINavigationController) {
        guard stacks[ObjectIdentifier(nav)] != nil else { return }
        let items = nav.viewControllers
        for (index, item) in items.enumerated() {
            guard let route = item as? RouteController, let source = route.projectedSource else { continue }
            let successor = items.indices.contains(index + 1) ? items[index + 1] as? RouteController : nil
            if projectBack(route, before: successor, heading: source.bar == .hidden ? nil : source.title, in: nav) {
                hatchRoute(route, in: nav, source: source, scroll: contentScrollView(of: route))
            }
        }
    }
}
#endif
