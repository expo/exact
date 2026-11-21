// @ref LLP 1047.001 D10 — navigation on the Mac as the core reaches it: the
// route stacks shown and hidden by the web's hide-and-inert rule, their sheets
// and transitions, the list beside its detail, Back and Forward, the root tabs
// as the window's sidebar, and the back swipe. The window's toolbar and title,
// the input gates and `NodeView.routeInert` are the core's: a plan with no
// route has them too.
//
// Without the family the presenter holds `NoNavigation`, `NoSidebar` and
// `NoBackSwipe`. A plan that asks for a route is refused before it runs
// (`Unlinked("navigation")`), so the null objects answer only what a plan
// with no navigation root observes from `NavigationHost`, `SidebarHost` and
// `BackSwipe`.
#if os(macOS)
import AppKit

/// What the core asks of a presenter's navigation. `sync` runs after a
/// batch's ops are applied, before the sidebar's.
package protocol NavigationHosting: AnyObject {
    func sync(_ batch: Batch, reparented: Set<UInt32>)
    /// What is shown, read again with no batch: a sheet left, a window came.
    func resync()
    func reset()
    /// The page's ground changed colour.
    func groundChanged()
    /// Whether a node that left keeps its view for its route's exit.
    func keepsForExit(_ id: UInt32) -> Bool
    /// A batch inside `startViewTransition()`'s cross-fade: its routes play no entrance or exit of their own.
    var crossFading: Bool { get set }

    /// The first navigation root, the stack that holds the route it shows
    /// and that route's place in it, as of the last batch.
    var selection: (root: UInt32, stack: [UInt32], at: Int)? { get }
    /// Whether a route under that root shows the platform's bar.
    var barRoute: Bool { get }
    var hasStack: Bool { get }
    var covered: Bool { get }
    var headerRoute: (root: NodeView, route: NodeView, pushed: Bool)? { get }
    var splitHeaderRoutes: (root: NodeView, owner: NodeView, beside: NodeView?)? { get }
    func splitPushed(_ route: NodeView) -> Bool
    var stackSplit: any StackSplitting { get }

    // Back and Forward.
    var back: NavigationBack.Way? { get }
    var backFacts: [UInt32] { get }
    var backPossible: Bool { get }
    var backInToolbar: Bool { get }
    var forwardPossible: Bool { get }
    var canGoBack: Bool { get }
    var canGoForward: Bool { get }
    func offers(back: Bool) -> Bool
    @discardableResult func goBack() -> Bool
    @discardableResult func goForward() -> Bool
    /// The platform's dismissal of a modal route; nil when it went, else why not.
    @discardableResult func dismiss(_ route: NodeView) -> String?

    // A modal route in its sheet, on the core's top-layer stack (`DialogHost`).
    /// The sheet's own rows round the route: its title, Back, cancel and confirm.
    func makeRouteChrome() -> (any SheetChrome)?
    func adoptRouteSheet(_ entry: DialogHost.Entry, _ sheet: DialogSheet)
    func releaseRouteSheet(_ entry: DialogHost.Entry)

    #if EXACT_AGENT
    /// `state.navigation.sheet` for a modal route painted in the window; nil with none.
    var paintedModalObservation: [String: Any]? { get }
    /// A route or tabpanel this host hid, which it inerts with it.
    func hides(_ node: NodeView) -> Bool
    func inspectionOwns(_ view: NSView) -> Bool
    func agentBack() -> (way: String, refused: String)?
    var backObservation: Any { get }
    func routes(in node: NodeView) -> [NodeView]
    #endif
}

/// A stack shown as a list beside its detail (LLP 1115.002 D4), as the core reaches it.
package protocol StackSplitting: AnyObject {
    /// Whether a window shows the split.
    var active: Bool { get }
    /// Whether the window's owner should compose a controller for it.
    var wanted: Bool { get }
    var onWanted: (() -> Void)? { get set }
    /// What the selected stack shows as a list beside its detail: the root,
    /// the list's route, the detail the pane shows and the pane's routes.
    var shownSplit: (root: NodeView, list: NodeView, detail: NodeView?, routes: [NodeView])? { get }
    /// Whether the list's header owns the window's title.
    var listOwns: Bool { get }
    /// The window of the controller the owner composed for the split.
    var window: NSWindow? { get }
    var detailPane: NSView? { get }
    /// The split view and the divider between the list and its detail.
    var listDivider: (view: NSSplitView, index: Int)? { get }
    var hasInspector: Bool { get }
    var emphasizedRow: NodeView? { get }
    /// The press of the row the list is pressing: a route it brings is that row's detail.
    var pressing: UInt32? { get set }
    /// The list whose row pushes the detail.
    var table: UInt32? { get }
    func pressesPaintedRow(_ node: NodeView) -> Bool
    func authored(_ mounted: [NodeView], in home: NSView) -> [NodeView]
    func deselected(_ list: UInt32)
    func holds(_ route: NodeView) -> Bool
    func holdsFocus(_ view: NSView?) -> Bool
    func pane(holding view: NSView?) -> NSView?
    func recolor(_ view: NodeView)
    func retire(_ route: NodeView)
    func samplePlacement()
    #if EXACT_AGENT
    var agentRoot: NSView { get }
    var agentCaptureArea: CGRect { get }
    var summary: [String: Any] { get }
    #endif
}

/// The root tabs as the window's sidebar (LLP 1115.002 D3).
package protocol SidebarHosting: AnyObject {
    func sync(_ batch: Batch?)
    func reset()
    var rooted: Bool { get set }
    var wanted: Bool { get }
    var onWanted: (() -> Void)? { get set }
    var splitView: NSSplitView? { get }
    /// Whether a sidebar is installed for this session (View ▸ Show Sidebar).
    var installed: Bool { get }
    func installed(in window: NSWindow) -> Bool
    func inToolbar(of window: NSWindow) -> Bool
    /// Whether the sidebar shows `node` as a row; nil when it does not stand for it.
    func shows(_ node: NodeView) -> Bool?
    /// Whether the sidebar shows this tablist's tabs: no segmented control is made of it.
    func adopts(_ tablist: NodeView) -> Bool
    #if EXACT_AGENT
    func hides(_ node: NodeView) -> Bool
    func suppresses(_ node: NodeView) -> Bool
    func activate(_ node: NodeView) -> Bool?
    func observation(_ node: NodeView) -> [String: Any]?
    var summary: [String: Any] { get }
    #endif
}

extension SidebarHosting {
    /// Read again with no batch: the window or its owner changed.
    package func sync() { sync(nil) }
}

/// The trackpad's swipe back and forward (LLP 1115.000 D5.1, D5.5).
package protocol BackSwiping: AnyObject {
    func scroll(_ event: NSEvent) -> Bool
    func swiped(deltaX: CGFloat) -> Bool
}

/// What a window's owner asks of the split view it composes.
package protocol WindowSplitting: NSViewController {
    /// Whether it shows the root tab bar as a sidebar.
    var hasSidebar: Bool { get }
    func install(in window: NSWindow)
    /// Give the view back and leave the window.
    func dismantle()
    /// The window's `viewport-fit` changed: the page's pane is pinned again.
    func viewportFitChanged()
}

/// How a window's split view is composed.
package struct WindowSplitOptions {
    package var autosaveName: String?
    package var minimumPageWidth: CGFloat
    package var sidebar: Bool
    package var minimumDetailWidth: CGFloat?
    package init(autosaveName: String?, minimumPageWidth: CGFloat, sidebar: Bool, minimumDetailWidth: CGFloat?) {
        (self.autosaveName, self.minimumPageWidth, self.sidebar, self.minimumDetailWidth) = (autosaveName, minimumPageWidth, sidebar, minimumDetailWidth)
    }
}

/// A route's sheet's own rows round the route it holds (navigation's `RouteChrome`).
package protocol SheetChrome: NSView {
    func hold(_ content: BlockView)
    func size(around size: CGSize) -> CGSize
    /// The height the rows take from the sheet's content.
    var rows: CGFloat { get }
    /// Whether `view` is one of the rows' own views.
    func owns(_ view: NSView) -> Bool
    /// The rows' buttons Tab reaches after the route's own stops.
    var stops: [NSView] { get }
    var backButton: NSButton { get }
    var cancelButton: NSButton { get }
    var confirmButton: NSButton { get }
}

/// What a route's sheet shows of its header (navigation's `SheetHeader`).
package protocol SheetHeading {
    var heading: NodeView? { get }
    var title: String { get }
    var cancel: NodeView? { get }
    var confirm: NodeView? { get }
    /// The header shows nothing else: its whole box is the platform's.
    var whole: Bool { get }
    /// The header's nodes the sheet's own rows stand for.
    var stoodFor: [NodeView] { get }
}

/// The slots are set before any session is made, and read on the main thread.
package enum NavigationLink {
    package static var make: ((Presenter) -> any NavigationHosting)?
    package static var makeSidebar: ((Presenter) -> any SidebarHosting)?
    package static var makeBackSwipe: ((Presenter) -> any BackSwiping)?
    /// The split view a window's owner composes when the app's root tab bar is
    /// its sidebar or a stack shows a list beside its detail
    /// (`ExactView.wantsWindowSidebar`, `wantsWindowSplit`, which are false
    /// without navigation).
    package static var makeWindowSplit: ((ExactView, WindowSplitOptions) -> any WindowSplitting)?
}

/// Navigation where the composition links none. The input gates a batch
/// changes are applied as `NavigationHost.sync` applies them, with no route
/// or panel of its own; Back and Forward answer as a plan with no
/// navigation root has them.
package final class NoNavigation: NavigationHosting {
    private unowned let presenter: Presenter
    package let stackSplit: any StackSplitting
    private var menusQueued = false
    private var oneLocationAsked: Bool?
    package var crossFading = false
    package init(_ presenter: Presenter) { self.presenter = presenter; stackSplit = NoSplit(presenter) }

    package func sync(_ batch: Batch, reparented: Set<UInt32>) {
        let gating = presenter.inputGates.touched(by: batch, reparented: reparented)
        presenter.inputGates.settle(touched: gating.touched, subtrees: gating.subtrees, placeholder: nil)
        // After the batch a painted popover goes where its invoker now is, as the split's
        // presentation places it when nothing splits (`SplitHost.readNone`).
        guard !menusQueued else { return }
        menusQueued = true
        presenter.afterBatch { [weak self] in
            self?.menusQueued = false
            self?.presenter.menus.layout()
        }
    }
    package func resync() { sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil), reparented: []) }
    package func reset() { oneLocationAsked = nil }
    package func groundChanged() {}
    package func keepsForExit(_ id: UInt32) -> Bool { false }

    package var selection: (root: UInt32, stack: [UInt32], at: Int)? { nil }
    package var barRoute: Bool { false }
    package var hasStack: Bool { false }
    package var covered: Bool { false }
    package var headerRoute: (root: NodeView, route: NodeView, pushed: Bool)? { nil }
    package var splitHeaderRoutes: (root: NodeView, owner: NodeView, beside: NodeView?)? { nil }
    package func splitPushed(_ route: NodeView) -> Bool { false }

    package var back: NavigationBack.Way? { nil }
    /// `NavigationHost.backFacts` with no pushed route: whether the root offers Back, which with no root it does.
    package var backFacts: [UInt32] { [1] }
    /// Whether the app's route table declares more than one location, asked of
    /// the runtime once a session, as `NavigationHost.backPossible` asks it.
    package var backPossible: Bool {
        if oneLocationAsked == nil, let asked = presenter.session?.runtime.routeOneLocation() { oneLocationAsked = asked }
        return oneLocationAsked != true
    }
    package var backInToolbar: Bool { false }
    package var forwardPossible: Bool { false }
    package var canGoBack: Bool { false }
    package var canGoForward: Bool { false }
    package func offers(back: Bool) -> Bool { true }
    package func goBack() -> Bool { false }
    package func goForward() -> Bool { false }
    package func dismiss(_ route: NodeView) -> String? { "its stack has no navigation root" }
    package func makeRouteChrome() -> (any SheetChrome)? { nil }
    package func adoptRouteSheet(_ entry: DialogHost.Entry, _ sheet: DialogSheet) {}
    package func releaseRouteSheet(_ entry: DialogHost.Entry) {}

    #if EXACT_AGENT
    package var paintedModalObservation: [String: Any]? { nil }
    package func hides(_ node: NodeView) -> Bool { false }
    package func inspectionOwns(_ view: NSView) -> Bool { false }
    package func agentBack() -> (way: String, refused: String)? { nil }
    package var backObservation: Any { NSNull() }
    package func routes(in node: NodeView) -> [NodeView] { [] }
    #endif
}

package final class NoSplit: StackSplitting {
    private unowned let presenter: Presenter
    package init(_ presenter: Presenter) { self.presenter = presenter }
    package var active: Bool { false }
    package var wanted: Bool { false }
    package var onWanted: (() -> Void)?
    package var shownSplit: (root: NodeView, list: NodeView, detail: NodeView?, routes: [NodeView])? { nil }
    package var listOwns: Bool { false }
    package var window: NSWindow? { nil }
    package var detailPane: NSView? { nil }
    package var listDivider: (view: NSSplitView, index: Int)? { nil }
    package var hasInspector: Bool { false }
    package var emphasizedRow: NodeView? { nil }
    package var pressing: UInt32?
    package var table: UInt32? { nil }
    package func pressesPaintedRow(_ node: NodeView) -> Bool { false }
    package func authored(_ mounted: [NodeView], in home: NSView) -> [NodeView] { mounted }
    package func deselected(_ list: UInt32) {}
    package func holds(_ route: NodeView) -> Bool { false }
    package func holdsFocus(_ view: NSView?) -> Bool { false }
    package func pane(holding view: NSView?) -> NSView? { nil }
    package func recolor(_ view: NodeView) {}
    package func retire(_ route: NodeView) {}
    package func samplePlacement() {}
    #if EXACT_AGENT
    package var agentRoot: NSView { presenter.viewport }
    package var agentCaptureArea: CGRect { CGRect(origin: .zero, size: presenter.viewport.bounds.size) }
    package var summary: [String: Any] { ["installed": false, "wanted": false] }
    #endif
}

package final class NoSidebar: SidebarHosting {
    private unowned let presenter: Presenter
    package init(_ presenter: Presenter) { self.presenter = presenter }
    /// No tablist stands for a sidebar: its cover is reported empty, as `SidebarHost` reports it with no root.
    package func sync(_ batch: Batch?) { presenter.covers.report("sidebar", [:]) }
    package func reset() {}
    package var rooted = false
    package var wanted: Bool { false }
    package var onWanted: (() -> Void)?
    package var splitView: NSSplitView? { nil }
    package var installed: Bool { false }
    package func installed(in window: NSWindow) -> Bool { false }
    package func inToolbar(of window: NSWindow) -> Bool { false }
    package func shows(_ node: NodeView) -> Bool? { nil }
    package func adopts(_ tablist: NodeView) -> Bool { false }
    #if EXACT_AGENT
    package func hides(_ node: NodeView) -> Bool { false }
    package func suppresses(_ node: NodeView) -> Bool { false }
    package func activate(_ node: NodeView) -> Bool? { nil }
    package func observation(_ node: NodeView) -> [String: Any]? { nil }
    package var summary: [String: Any] { ["installed": false, "wanted": false] }
    #endif
}

package final class NoBackSwipe: BackSwiping {
    package init() {}
    package func scroll(_ event: NSEvent) -> Bool { false }
    package func swiped(deltaX: CGFloat) -> Bool { false }
}
#endif
