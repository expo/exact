// Navigation as the core reaches it on the Mac (`NavigationLinkMac.swift`):
// the hosts' conformances, and the queries the window's bar asks of routes,
// which read the family's own state.
#if os(macOS)
import AppKit
import ExactKit

extension NavigationHost: NavigationHosting {
    var stackSplit: any StackSplitting { split }
    func keepsForExit(_ id: UInt32) -> Bool { transitions.keeps(id) }
    func makeRouteChrome() -> (any SheetChrome)? { RouteChrome() }
    func adoptRouteSheet(_ entry: DialogHost.Entry, _ sheet: DialogSheet) { presenter.dialogs.adopt(entry, sheet) }
    func releaseRouteSheet(_ entry: DialogHost.Entry) { presenter.dialogs.release(entry) }
    func dialogSheetHeader(_ dialog: NodeView) -> (any SheetHeading)? { SheetHeader.read(dialog: dialog) }
    func syncDialogSheet(_ entry: DialogHost.Entry) { presenter.dialogs.project(entry, SheetHeader.read(dialog: entry.dialog)) }
    #if EXACT_AGENT
    var paintedModalObservation: [String: Any]? {
        paintedModal.map { ["route": Int($0.id), "presentation": "painted", "kept": keptSheets[$0.id] ?? NSNull()] }
    }
    #endif
}

extension SplitHost: StackSplitting {
    var shownSplit: (root: NodeView, list: NodeView, detail: NodeView?, routes: [NodeView])? {
        shown.map { ($0.root, $0.list, $0.detail, $0.routes) }
    }
    var window: NSWindow? { controller?.view.window }
    var table: UInt32? { shown?.table }
    var detailPane: NSView? { controller?.detailPane }
    var listDivider: (view: NSSplitView, index: Int)? { controller.map { ($0.splitView, $0.listDivider) } }
}

extension SidebarHost: SidebarHosting {}

extension ExactSidebarController: WindowSplitting {
    package func dismantle() { release() }
}

extension BackSwipe: BackSwiping {
    func scroll(_ event: NSEvent) -> Bool { scroll(event, enabled: NSEvent.isSwipeTrackingFromScrollEventsEnabled && !ExactEnv.agentMode) }
}

extension Presenter {
    /// The built-in navigation, for the family's own code.
    var navigationHost: NavigationHost { navigation as! NavigationHost }
    var sidebarHost: SidebarHost { sidebar as! SidebarHost }
}

extension NavigationHost {
    /// The route whose header the window's bar shows, with its navigation
    /// root: the route the root shows, or under a modal route the route it
    /// stands over (`NavigationRules.segments`); and whether that route is
    /// pushed, so the bar's Back is its way back. Read from what the last
    /// batch showed (`selection`).
    var headerRoute: (root: NodeView, route: NodeView, pushed: Bool)? {
        // A stack that splits: one of its two routes owns the window.
        if let routes = splitHeaderRoutes { return (routes.root, routes.owner, splitPushed(routes.owner)) }
        return windowRoute
    }

    /// Whether a split's route is pushed, so the bar's Back stands for its
    /// header's Back control: the list never is, and a detail only above
    /// where its pane begins, the base detail having no Back (LLP 1115.002
    /// D4). Both headers of a split read it, whichever owns the window.
    func splitPushed(_ route: NodeView) -> Bool {
        route !== split.shown?.list && windowRoute?.pushed == true
    }

    /// The route under any modal or fullscreen segments: the window's
    /// toolbar still belongs to it while a cover is selected. A split's
    /// base detail is not pushed.
    var windowRoute: (root: NodeView, route: NodeView, pushed: Bool)? {
        guard let selection, let root = presenter.views[selection.root], selection.stack.indices.contains(selection.at) else { return nil }
        let stack = selection.stack[...selection.at].compactMap { presenter.views[$0] }
        guard stack.count == selection.at + 1,
              let base = NavigationRules.segments(presentations: stack.map { $0.props["navigationPresentation"] }).first,
              let route = stack[base].last else { return nil }
        return (root, route, base.count - 1 > split.base)
    }

    /// @ref LLP 1115.002 D4, §5 Q10 — the two headers of a window that shows
    /// a list beside its detail. One route owns the window: its heading is
    /// the window's title and its whole header is the bar's. The other is
    /// beside it: its heading stays the first line of its pane, and its
    /// buttons are the bar's over its own column, after the separator that
    /// follows the divider. Which is which is `SplitHost.listOwns`.
    var splitHeaderRoutes: (root: NodeView, owner: NodeView, beside: NodeView?)? {
        guard split.active, let shown = split.shown else { return nil }
        guard let detail = shown.detail else { return (shown.root, shown.list, nil) }
        return split.listOwns ? (shown.root, shown.list, detail) : (shown.root, detail, shown.list)
    }
}
#endif
