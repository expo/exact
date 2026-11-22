// @ref LLP 1038 D6; LLP 1035.001 D1/D6 — the web's hide-and-inert rule.
#if os(macOS)
import AppKit
import ExactKit

final class NavigationHost {
    unowned let presenter: Presenter
    private var refused: [UInt32: String] = [:]
    /// The tabpanels a root last hid or showed: one it stops naming shows again.
    private var panels: Set<UInt32> = []
    /// Each root's stack as last shown, its keys through its selected route.
    private var shown: [UInt32: [String]] = [:]
    init(presenter: Presenter) { self.presenter = presenter }
    /// LLP 1080.001 D3: a route or tabpanel this host hid (the web's
    /// hide-and-inert rule), which it inerts with it.
    func hides(_ node: NodeView) -> Bool { node.routeInert && (panels.contains(node.id) || node.props["navigationKey"] != nil) }
    /// The first navigation root, the stack that holds the route it shows
    /// and that route's place in it, as of the last batch: what the
    /// platform's Back leaves (BackMac.swift).
    private(set) var selection: (root: UInt32, stack: [UInt32], at: Int)?
    /// Whether a route mounted under that root, in any of its stacks, shows
    /// the platform's bar (does not say `navigationBar="none"`): what gives
    /// the window its toolbar (ToolbarMac.swift).
    private(set) var barRoute = false
    /// The first navigation root as of the last batch, its tabs as this
    /// host's views name them (`NavigationTabs`) and the tab whose stack
    /// holds the route it shows: what the sidebar reads (SidebarMac.swift).
    private(set) var root: (node: NodeView, tabs: NavigationTabs?, at: Int?)?
    /// Whether that root has no routes, in any of its stacks (BackMac.swift `routerRoot`).
    private(set) var routeless = false
    /// Whether the app's route table declares one location, asked of the
    /// runtime once a session (BackMac.swift `backPossible`).
    var oneLocation: Bool {
        if let asked = oneLocationAsked { return asked }
        guard let asked = presenter.session?.runtime.routeOneLocation() else { return false }
        oneLocationAsked = asked
        return asked
    }
    private var oneLocationAsked: Bool?
    /// @ref LLP 1115.002 D7.1 — the ground under each modal route that
    /// paints none of its own, by the route's id (`ModalGround`).
    private var grounds: [UInt32: ModalGround] = [:]
    /// @ref LLP 1115.002 D7.4 — the modal routes that are sheets as of the
    /// last batch, bottom to top, with what each sheet shows of its header
    /// (RouteSheetMac.swift), and why a route that is not one stays painted.
    private(set) var sheets: [(base: NodeView, route: NodeView, header: SheetHeader?)] = []
    var keptSheets: [UInt32: String] = [:]
    /// A batch that can have changed a header came since the sheets last showed theirs.
    var sheetsRead = false
    /// @ref LLP 1115.002 D4 — the selected stack as a list beside its detail (SplitMac.swift).
    private(set) lazy var split = SplitHost(presenter)
    /// @ref LLP 1013.001 — a route's own entrance and exit (RouteTransitionMac.swift).
    private(set) lazy var transitions = RouteTransitions(presenter: presenter)
    var crossFading = false
    /// The modal route the root shows painted in the window, on its ground.
    var paintedModal: NodeView? {
        guard let selection, selection.at > 0, selection.stack.indices.contains(selection.at), let route = presenter.views[selection.stack[selection.at]],
              route.props["navigationPresentation"] == "modal", !sheets.contains(where: { $0.route === route }) else { return nil }
        return route
    }
    /// What is shown, read again with no batch: a sheet left, a window came.
    func resync() { project(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil), reparented: [], concerned: true) }
    func sync(_ batch: Batch, reparented: Set<UInt32> = []) { project(batch, reparented: reparented, concerned: nil) }
    /// A stack's routes as authored, a route in its sheet's window among
    /// them. What else has left `node`'s views for a container of its own
    /// (a `dialog` in its sheet) is no route.
    func routes(in node: NodeView) -> [NodeView] {
        presenter.blocks.authored(NavigationTabs.routes(in: node), in: node.container).filter { $0.props["navigationKey"] != nil }
    }
    /// LLP 1080.001 D3: a modal route's ground is this host's view.
    func inspectionOwns(_ view: NSView) -> Bool { view is ModalGround }
    /// The page's ground changed colour: the grounds are painted again.
    func groundChanged() { for ground in grounds.values { ground.needsDisplay = true }; split.groundChanged() }
    func reset() {
        refused.removeAll(); panels.removeAll(); shown.removeAll(); selection = nil; barRoute = false; root = nil; routeless = false
        oneLocationAsked = nil; sheets = []; keptSheets.removeAll(); split.reset()
        for ground in grounds.values { ground.removeFromSuperview() }
        grounds.removeAll()
    }

    /// A modal route painted in the window covers the route beneath it,
    /// which stays shown under it as a page stays under a sheet: where the
    /// route paints no background of its own, the page's ground is its
    /// surface. What the author wrote on the route (a colour, a gradient)
    /// is the surface. One in its sheet stands on the sheet's.
    private func ground(_ routes: [NodeView], grounded: inout Set<UInt32>) {
        for route in routes where route.props["navigationPresentation"] == "modal" {
            let painted = (route.cgColor("background_color")?.alpha ?? 0) > 0 || route.style["background_image"] != nil
            // In its sheet the route stands on the sheet's own ground (D7.4).
            guard !painted, !sheets.contains(where: { $0.route === route }), !presenter.dialogs.owns(route) else { continue }
            grounded.insert(route.id)
            let ground = grounds[route.id] ?? ModalGround(presenter: presenter)
            grounds[route.id] = ground
            if ground.superview !== route {
                ground.frame = route.bounds
                route.addSubview(ground, positioned: .below, relativeTo: nil)
            }
        }
    }

    private func project(_ batch: Batch, reparented: Set<UInt32>, concerned: Bool?) {
        let gating = presenter.inputGates.touched(by: batch, reparented: reparented)
        var subtrees = gating.subtrees
        // Whether a header can have changed: what the window's bar reads again for.
        let concerned = concerned ?? presenter.toolbar.mayChangeAHeader(batch)
        if concerned { sheetsRead = true }
        var sheeted: [(base: NodeView, route: NodeView, header: SheetHeader?)] = []
        for op in batch.ops {
            // A row, or a symbol in one, came or changed: the split's painted list is read again (SplitMac.swift `veil`).
            if op.op == .create || op.op == .children || op.op == .props { split.rowsChanged = true }
            if op.op == .style || op.op == .frame, let id = op.nodeID { split.restyled(id) }
            if op.op == .destroy, let id = op.nodeID { refused.removeValue(forKey: id); shown.removeValue(forKey: id); keptSheets.removeValue(forKey: id) }
        }
        func gate(_ node: NodeView, hidden: Bool, inert: Bool) {
            if node.isHidden != hidden || node.routeInert != inert { subtrees.insert(node.id) }
            node.isHidden = hidden
            node.routeInert = inert
        }
        var managed = Set<UInt32>()
        var grounded = Set<UInt32>()
        let roots = presenter.carrying("navigationBack")
        selection = nil
        barRoute = false
        root = nil
        routeless = false
        for nav in roots {
            // @ref LLP 1075.003 §3.7 — with tabs, each panel is a stack.
            let tabs = NavigationTabs.of(nav, presenter)
            let locate = { (key: String) -> (at: Int?, stacks: [[NodeView]]) in
                let stacks = tabs.map { $0.panels.map(self.routes(in:)) } ?? [self.routes(in: nav)]
                return (stacks.firstIndex { $0.contains { $0.props["navigationKey"] == key } }, stacks)
            }
            var key = nav.props["navigationKey"] ?? ""
            var (found, stacks) = locate(key)
            if found == nil {
                if refused[nav.id] != key {
                    refused[nav.id] = key
                    presenter.session?.log("navigationKey \"\(key)\" matches no route among the root's children or those of the tabpanels its tablist names; the stack is unchanged")
                }
                // D1: left alone, less the routes that left the tree (a closed sheet's).
                let declared = stacks.map { $0.map { $0.props["navigationKey"] ?? "" } }
                if let survivor = NavigationRules.survivor(of: shown[nav.id] ?? [], declared: declared) {
                    key = survivor
                    (found, stacks) = locate(key)
                }
            } else {
                refused.removeValue(forKey: nav.id)
            }
            if nav === roots.first {
                if let at = found, stacks.indices.contains(at) {
                    let routes = stacks[at], selected = routes.firstIndex { $0.props["navigationKey"] == key } ?? 0
                    sheeted = sheetRoutes(routes, selected: selected, root: nav, concerned: concerned)
                    split.read(root: nav, routes: routes, selected: selected)
                } else { split.readNone() }
                sheets = sheeted
            }
            for routes in stacks { ground(routes, grounded: &grounded) }
            if nav === roots.first { root = (nav, tabs, tabs == nil ? nil : found); routeless = stacks.allSatisfy(\.isEmpty) }
            guard let at = found else {
                managed.formUnion((tabs?.panels ?? []).map(\.id))
                continue
            }
            for (index, panel) in (tabs?.panels ?? []).enumerated() {
                gate(panel, hidden: index != at, inert: index != at)
                managed.insert(panel.id)
            }
            for (stack, routes) in stacks.enumerated() {
                // The selected stack shows the route the root names; another
                // keeps its top laid out under its hidden panel.
                let selected = stack == at ? routes.firstIndex { $0.props["navigationKey"] == key } ?? 0 : routes.count - 1
                if stack == at {
                    let before = shown[nav.id] ?? [], after = routes.prefix(selected + 1).map { $0.props["navigationKey"] ?? "" }
                    shown[nav.id] = after
                    if nav === roots.first { transitions.changed(from: before, to: after, stack: routes, at: selected) }
                }
                if stack == at, nav === roots.first { selection = (nav.id, routes.map(\.id), selected) }
                if nav === roots.first, routes.contains(where: { $0.props["navigationBar"] != "none" }) { barRoute = true }
                // A modal route painted in the window shows the route beneath it, and so does a
                // full-screen route whose author wrote its background (`transparent` included),
                // drawn over that page as CSS composites it (Charlie, 2026-11-10). One that paints
                // nothing covers it.
                let overPage = routes.indices.contains(selected) && {
                    let route = routes[selected], presentation = route.props["navigationPresentation"]
                    return presentation == "modal" || presentation == "fullscreen" && (route.wrote("background_color") || route.wrote("background_image"))
                }()
                // The lowest route in a sheet: it and the sheets on it are shown, and the route they stand over.
                let lowest = stack == at && nav === roots.first ? sheeted.first.flatMap { low in routes.firstIndex { $0 === low.base } } : nil
                // A stack that splits (D4) shows its list and the route its pane shows, and the list stays live beside its detail.
                let splits = stack == at && nav === roots.first && split.active, beside = splits ? split.shown?.detail : nil
                let covered = routes.indices.contains(selected) && ["modal", "fullscreen"].contains(routes[selected].props["navigationPresentation"] ?? "")
                for (index, route) in routes.enumerated() {
                    let under = lowest.map { index == $0 - 1 || sheeted.contains { $0.route === route } } ?? false
                    // A route still in its sheet is shown until the sheet has gone (`DialogHost.syncRoutes`).
                    let leaving = presenter.dialogs.owns(route) && !sheeted.contains { $0.route === route }
                    let column = splits && (index == 0 || route === beside)
                    // In the detail's pane, out of its hidden panel's views: hidden and inert by name.
                    let away = stack != at && split.holds(route)
                    // A route a push holds beneath its entrance stays shown, as does a route playing its
                    // exit, and they stay inert until the part ends.
                    let beneath = transitions.held?.beneath == route.id || transitions.exiting == route.id
                    let held = beneath || transitions.held?.arriving == route.id
                    gate(route, hidden: leaving ? route.isHidden : !beneath && (away || (index != selected && !(overPage && index == selected - 1) && !under && !column)),
                         inert: held || away || (index != selected && !(splits && index == 0 && !covered)))
                }
            }
        }
        if roots.isEmpty { sheets = []; split.readNone() }
        // A sheet arrives and leaves after the batch: AppKit turns the main queue while it does.
        if !sheets.isEmpty || presenter.dialogs.entries.contains(where: \.route) { presenter.afterBatch { [weak presenter] in presenter?.dialogs.syncRoutes() } }
        for id in panels.subtracting(managed) {
            if let panel = presenter.views[id] { gate(panel, hidden: false, inert: false) }
        }
        panels = managed
        for id in Array(grounds.keys) where !grounded.contains(id) { grounds.removeValue(forKey: id)?.removeFromSuperview() }
        presenter.inputGates.settle(touched: gating.touched, subtrees: subtrees, placeholder: split.shownPlaceholder)
    }
}

/// @ref LLP 1115.002 D7.1 — the opaque surface of a modal route that paints
/// none: the page's ground, under everything the route holds (a child's
/// negative `z-index` included), at the route's size. It is no hit: a click
/// on it is a click on the route. A modal route is a sheet with a window of
/// its own (D7.4, RouteSheetMac.swift); this is the ground of one painted
/// in the window all the same (the driver's own chrome, a header its sheet
/// cannot take the title out of, no window) when the compiler gave it none
/// either (LLP 1116.009 D3 gives a route whose presentation it can read
/// the page's).
final class ModalGround: NSView {
    private weak var presenter: Presenter?
    init(presenter: Presenter) {
        self.presenter = presenter
        super.init(frame: .zero)
        autoresizingMask = [.width, .height]
        wantsLayer = true
        layer?.zPosition = -1
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) {
        (presenter?.opaqueGround ?? .windowBackgroundColor).setFill()
        // A capture's dirty rectangle is the captured view's, not this one's.
        bounds.intersection(dirtyRect).fill()
    }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
#endif
