#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation
@testable import ExactGroupedLists

/// LLP 1116.003 over `scripts/fixtures/tab-edge.contract`: a tab's scroller,
/// scrolled to its end, brings its last row above the floating tab bar, as a
/// hand-built table view in a tab bar controller does (its adjusted content
/// inset holds the bar), and its content still runs under the glass to the
/// screen's bottom. A plain `scroll`, a plain `list` and a grouped `list`,
/// each at a tab's root and pushed inside a tab.
final class TabEdgeIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?

    override func tearDown() {
        session?.destroy()
        session = nil
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    private func spin(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }

    private func fixture(wholeScreen: Bool = false) throws -> ExactSession {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_TAB_EDGE_PLAN"], "build.mjs --test --ios compiles the tab-edge fixture")
        ExactGroupedLists.install()
        let session = ExactApp.shared.makeSession(label: "tab-edge")
        self.session = session
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = hostedWindowScene()
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = wholeScreen ? (window.windowScene?.screen.bounds ?? window.frame) : CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        try waitForRoute("scrolls")
        return session
    }

    private func node(_ testId: String) throws -> NodeView {
        try XCTUnwrap(session?.presenter.views.values.first { $0.props["testId"] == testId && $0.window != nil }
            ?? session?.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    private func waitForRoute(_ name: String) throws {
        let session = try XCTUnwrap(session), agent = Agent(session: session)
        let deadline = Date().addingTimeInterval(5)
        func ready() -> Bool {
            let top = session.presenter.navigationHost.primaryNavigation?.topViewController as? RouteController
            return top?.node.props["testId"] == "route-\(name)" && !agent.nativeInFlight()
        }
        while !ready() && Date() < deadline {
            _ = RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05))
        }
        XCTAssertTrue(ready(), "route \(name) is shown and settled")
    }

    private func tap(_ testId: String, route: String) throws {
        let reply = Agent(session: try XCTUnwrap(session)).tap(["id": Int(try node(testId).id)])
        XCTAssertNil(reply["error"], "\(reply)")
        try waitForRoute(route)
    }

    /// A grid header button, pressed with `Presenter.press`, where UIKit's bar item action ends: the bar has fewer places
    /// than the header has buttons, and UIKit keeps the rest in its overflow menu, where the agent does not tap.
    private func press(_ testId: String, route: String) throws {
        try XCTUnwrap(session).presenter.press(try node(testId).id)
        try waitForRoute(route)
    }

    private func box(_ v: UIView) -> CGRect { v.convert(v.bounds, to: nil) }

    /// The node a view is under in the authored tree, wherever UIKit holds it.
    private func nodeParent(_ v: UIView) -> NodeView? {
        let p = session?.presenter
        return sequence(first: p?.authoredParent(of: v), next: { $0.flatMap { p?.authoredParent(of: $0) } }).lazy.compactMap { $0 as? NodeView }.first
    }

    /// Scrolled to its end as a finger's scroll ends there: the offset UIKit
    /// rests at, its content size and insets as they are once laid out (a
    /// grouped list's cells size themselves as they come in). It seeks again
    /// whenever the measured end moves, and passes on a turn that found the
    /// scroller already there with no native work in flight.
    private func scrollToEnd(_ sv: UIScrollView, file: StaticString = #filePath, line: UInt = #line) {
        guard let session else { return XCTFail("no session", file: file, line: line) }
        let agent = Agent(session: session)
        func end() -> CGFloat {
            sv.layoutIfNeeded()
            let i = sv.adjustedContentInset
            return max(-i.top, sv.contentSize.height + i.bottom - sv.bounds.height)
        }
        waitFor("the scroller rests at its measured end", facts: { "offset \(sv.contentOffset.y), end \(end())" }) {
            session.presenter.settlePump()
            let end = end()
            guard abs(sv.contentOffset.y - end) < 0.5 else {
                sv.setContentOffset(CGPoint(x: sv.contentOffset.x, y: end), animated: false)
                return false
            }
            return !agent.nativeInFlight()
        }
    }

    /// The route `name`'s scroller and its last row's box in the window,
    /// scrolled to its end.
    private func end(of name: String) throws -> (scroll: UIScrollView, last: CGRect) {
        let p = try XCTUnwrap(session).presenter
        let content = try node("content-\(name)")
        if let list = p.groupedLists?.scroller(for: content.id) as? UICollectionView {
            scrollToEnd(list)
            let section = list.numberOfSections - 1, item = list.numberOfItems(inSection: section) - 1
            let row = try XCTUnwrap(list.layoutAttributesForItem(at: IndexPath(item: item, section: section)), "\(name)'s last row")
            return (list, list.convert(row.frame, to: nil))
        }
        let sv = try XCTUnwrap(content.scroll, "\(name) scrolls")
        scrollToEnd(sv)
        p.settlePump()
        sv.layoutIfNeeded()
        return (sv, box(try node("\(name)-30")))
    }

    private func assertClear(_ name: String, _ tabs: UITabBarController, file: StaticString = #filePath, line: UInt = #line) throws {
        let (sv, last) = try end(of: name)
        let bar = box(tabs.tabBar)
        XCTAssertFalse(tabs.tabBar.isHidden, file: file, line: line)
        XCTAssertLessThanOrEqual(last.maxY, bar.minY + 0.5, "\(name)'s last row ends above the tab bar's top \(bar.minY): \(last), inset \(sv.adjustedContentInset.bottom)", file: file, line: line)
        XCTAssertGreaterThan(last.maxY, bar.minY - 60, "\(name)'s last row rests near the bar, not short of it: \(last)", file: file, line: line)
        XCTAssertGreaterThanOrEqual(box(sv).maxY, bar.maxY - 0.5, "\(name)'s content runs under the glass to the screen's bottom: \(box(sv))", file: file, line: line)
    }

    func testATabsLastRowScrollsClearOfTheTabBar() throws {
        let session = try fixture()
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController, "the tabs are a UITabBarController")
        for (tab, root, pushed) in [("scrolls", "scrolls", "scrollsPushed"), ("plain", "plain", "plainPushed"), ("grouped", "grouped", "groupedPushed")] {
            try tap("tab-\(tab)", route: root)
            try assertClear(root, tabs)
            try tap("\(root)-30", route: pushed)
            let nav = try XCTUnwrap(tabs.selectedViewController as? UINavigationController)
            XCTAssertEqual(nav.viewControllers.count, 2, "\(tab) pushed")
            try assertClear(pushed, tabs)
        }
    }

    func testAReplacementBarClearsTheGridAndKeepsUIKitScrollAdjustment() throws {
        try replacementBar()
    }

    func testAReplacementBarClearsAPushedGridAndRestoresItsInset() throws {
        try replacementBar(pushed: true)
    }

    func testAReplacementBarWithHorizontalMarginsClearsTheGrid() throws {
        try replacementBar(inset: true)
    }

    /// The bar is the panels' sibling inside a column of their own, not the navigation root's child.
    func testAReplacementBarInAColumnWithThePanelsClearsTheGrid() throws {
        try replacementBar(wrapped: true)
    }

    /// A bar nested beside the panels, in the column that holds them, after them in tree order, both
    /// positioned: an in-flow row below the panels while the tab bar is hidden, it paints over the native
    /// container that stands in for them and takes its touches (LLP 1083.000 D4).
    func testABarNestedBesideThePanelsPaintsOverTheContainerAndTakesItsTouches() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the screen's pixels need a window the system draws") }
        try nestedBar(positioned: true)
    }

    /// The same bar in flow, which its column paints under the positioned holder of the panels before it: an
    /// in-flow row below the panels while the tab bar is hidden, it paints over the container all the same
    /// (LLP 1083.000 D4).
    func testAnInFlowBarNestedBesideThePanelsPaintsOverTheContainer() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the screen's pixels need a window the system draws") }
        try nestedBar(positioned: false)
    }

    private func nestedBar(positioned: Bool) throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("setTabBarHidden is iOS 18") }
        _ = try fixture(wholeScreen: true)
        try tap("tab-grid", route: "grid")
        try press("wrap-bar", route: "grid")
        if !positioned { try press("static-bar", route: "grid") }
        try press("select-grid", route: "grid")
        let bar = try node("selection-bar"), win = try XCTUnwrap(window)
        waitFor("the bar is shown") { bar.window != nil && bar.bounds.height > 0 }
        // The bar's own gray, at its right end, clear of its text
        let point = bar.convert(CGPoint(x: bar.bounds.maxX - 6, y: bar.bounds.midY), to: win)
        let hit = try XCTUnwrap(win.hitTest(point, with: nil))
        XCTAssertTrue(hit === bar || hit.isDescendant(of: bar), "the bar takes its touches, not \(hit)")
        XCTAssertEqual(try pixel(at: point), [238, 238, 238], "the bar paints over the container")
        // The route still takes the touches over its content: no empty panel stands over the container
        let content = try node("content-grid")
        let inside = content.convert(CGPoint(x: content.bounds.midX, y: content.bounds.midY), to: win)
        let routeHit = try XCTUnwrap(win.hitTest(inside, with: nil))
        XCTAssertTrue(routeHit.isDescendant(of: content) || routeHit === content, "the route's content takes its touches, not \(routeHit)")
    }

    /// A sidebar nested before the panels, in the column that holds them, which no route's box meets: it paints
    /// over the container and takes its touches, grows the container's leading safe area as a sidebar among the
    /// root's children does, Tab reaches it before the routes, as the tree orders it, and the mount lists it
    /// ahead of the container (LLP 1083.000 D4).
    func testASidebarNestedBeforeThePanelsPaintsOverTheContainerAndReadsFirst() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the screen's pixels need a window the system draws") }
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("show-sidebar", route: "grid")
        let side = try node("side"), win = try XCTUnwrap(window)
        waitFor("the sidebar is lifted over the container") { side.window != nil && side.bounds.width > 0 && navigation.siblings.holds(side) }
        let point = side.convert(CGPoint(x: side.bounds.midX, y: side.bounds.maxY - 24), to: win)
        let hit = try XCTUnwrap(win.hitTest(point, with: nil))
        XCTAssertTrue(hit === side || hit.isDescendant(of: side), "the sidebar takes its touches, not \(hit)")
        XCTAssertEqual(try pixel(at: point), [254, 220, 186], "the sidebar paints over the container")
        let owner = try XCTUnwrap(navigation.primaryOwner)
        waitFor("the container's safe area, UIKit's own inset included, reaches the sidebar") { abs(owner.view.safeAreaInsets.left - side.bounds.width) < 0.5 }
        // Tab: the sidebar's button first, then, among the rest, the route's
        let presenter = session.presenter, sideButton = try node("side-button"), first = try node("grid-first")
        XCTAssertTrue(presenter.moveFocus(backward: false))
        XCTAssertTrue(presenter.focusedNode === sideButton, "Tab reaches the sidebar first, not \(String(describing: presenter.focusedNode?.props["testId"]))")
        var stops = 0
        while presenter.focusedNode !== first, stops < 12, presenter.moveFocus(backward: false) { stops += 1 }
        XCTAssertTrue(presenter.focusedNode === first, "and the route's after it")
        // VoiceOver: the mount lists the sidebar ahead of the container
        let mount = navigation.ownerMount(try XCTUnwrap(navigation.container))
        let elements = try XCTUnwrap(mount.accessibilityElements, "the mount lists its children while a lifted node reads first")
        let sideAt = try XCTUnwrap(elements.firstIndex { ($0 as? UIView).map { $0 === side || $0.isDescendant(of: side) } ?? false })
        let ownerAt = try XCTUnwrap(elements.firstIndex { ($0 as? UIView) === owner.view })
        XCTAssertLessThan(sideAt, ownerAt, "the mount lists the sidebar ahead of the container")
    }

    /// A lifted node keeps its column's `accent-color` and `-exact-tint-adjustment`, which UIKit carries down
    /// the column's own views (LLP 1083.000 D4). Its stand takes them as authored, not as UIKit reads them back
    /// while an alert dims the views behind it: an accent the column takes while they are dimmed is the sidebar
    /// button's after it, and a column that keeps its tint keeps the button's from greying.
    func testALiftedSidebarKeepsItsColumnsAccentAndTintAdjustment() throws {
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("show-sidebar", route: "grid")
        let side = try node("side"), button = try node("side-button")
        waitFor("the sidebar is lifted over the container") { side.window != nil && navigation.siblings.holds(side) }
        func rgb(_ color: UIColor) -> [Int] {
            var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
            color.resolvedColor(with: button.traitCollection).getRed(&r, green: &g, blue: &b, alpha: &a)
            return [r, g, b].map { Int(($0 * 255).rounded()) }
        }
        // Dimmed as UIKit dims the views behind an alert: a window's own mode does not reach its views (measured)
        let behind = try XCTUnwrap(window?.rootViewController?.view)
        behind.tintAdjustmentMode = .dimmed
        defer { behind.tintAdjustmentMode = .automatic }
        try press("accent-column", route: "grid")
        navigation.siblings.place()
        XCTAssertNotEqual(rgb(button.tintColor), [255, 136, 0], "the button greys while the views are dimmed")
        behind.tintAdjustmentMode = .automatic
        XCTAssertEqual(rgb(button.tintColor), [255, 136, 0], "and draws in the accent its column took meanwhile once they are not")
        try press("keep-tint", route: "grid")
        behind.tintAdjustmentMode = .dimmed
        XCTAssertEqual(button.tintAdjustmentMode, .normal, "a column that keeps its tint keeps the button's from greying")
        XCTAssertEqual(rgb(button.tintColor), [255, 136, 0], "in its accent")
    }

    /// The sidebar at the trailing edge (`row-reverse`): each route is measured within its panel, which ends
    /// where the sidebar begins, so the sidebar is beside the routes, paints over the container, and grows its
    /// trailing safe area (LLP 1083.000 D4).
    func testATrailingSidebarNestedBesideThePanelsPaintsOverTheContainer() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the screen's pixels need a window the system draws") }
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("show-trailing-sidebar", route: "grid")
        let side = try node("side"), win = try XCTUnwrap(window)
        waitFor("the sidebar is lifted over the container") { side.window != nil && side.bounds.width > 0 && navigation.siblings.holds(side) }
        let point = side.convert(CGPoint(x: side.bounds.midX, y: side.bounds.maxY - 24), to: win)
        let hit = try XCTUnwrap(win.hitTest(point, with: nil))
        XCTAssertTrue(hit === side || hit.isDescendant(of: side), "the sidebar takes its touches, not \(hit)")
        XCTAssertEqual(try pixel(at: point), [254, 220, 186], "the sidebar paints over the container")
        let owner = try XCTUnwrap(navigation.primaryOwner)
        waitFor("the container's safe area, UIKit's own inset included, reaches the sidebar") { abs(owner.view.safeAreaInsets.right - side.bounds.width) < 0.5 }
    }

    /// A sidebar beside the panels set off by a margin, so it reaches neither the container's top nor its leading
    /// edge: lifted, it grows the container's leading safe area up to where the panels' holder, and so the
    /// routes, begin, and the bar's contents start there, as the route's header does on the web (LLP 1083.000 D4).
    /// An inset the container inherits is part of that reach, not added to it.
    func testASetOffSidebarGrowsTheSafeAreaToWhereTheRoutesBegin() throws {
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("set-off-sidebar", route: "grid")
        let side = try node("side"), owner = try XCTUnwrap(navigation.primaryOwner)
        waitFor("the sidebar is lifted over the container") { side.window != nil && side.bounds.width > 0 && navigation.siblings.holds(side) }
        let area = try XCTUnwrap(owner.view), box = side.convert(side.bounds, to: area)
        XCTAssertGreaterThan(box.minX, 0.5, "the sidebar is set off from the leading edge")
        XCTAssertGreaterThan(box.minY, 0.5, "and from the top")
        let start = try node("panels-holder").convert(try node("panels-holder").bounds, to: area).minX
        XCTAssertGreaterThan(start, box.maxX + 0.5, "the routes begin past the sidebar's margin")
        waitFor("the container's leading safe area reaches where the routes begin") { abs(owner.additionalSafeAreaInsets.left - start) < 0.5 }
        let nav = try XCTUnwrap(navigation.primaryNavigation)
        waitFor("the bar's layout clears the sidebar and its margin") { nav.navigationBar.safeAreaInsets.left >= start - 0.5 }
        let host = try XCTUnwrap(window?.rootViewController)
        host.additionalSafeAreaInsets.left = 20
        defer { host.additionalSafeAreaInsets.left = 0 }
        waitFor("the container adds what the inherited inset leaves") { abs(owner.additionalSafeAreaInsets.left - (start - 20)) < 0.5 }
        waitFor("so the bar's layout still starts where the routes begin") { abs(nav.navigationBar.safeAreaInsets.left - start) < 0.5 }
    }

    /// A container that defines its presentation context, as an app's may, covers the routes with a presentation in it:
    /// no route is in the window, while the session still is, and a report then leaves the sidebar's lift and the inset
    /// it asks of the container as they were, as it leaves the covers, so the bars under the cover keep their layout
    /// through its dismissal. (A presentation over the whole session takes the session out of the window, which retires
    /// the containers.)
    func testASidebarsInsetStandsUnderACoverOverItsContainer() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit presents only from a window a scene holds") }
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("show-sidebar", route: "grid")
        let side = try node("side"), owner = try XCTUnwrap(navigation.primaryOwner)
        waitFor("the sidebar insets the container") { navigation.siblings.holds(side) && owner.additionalSafeAreaInsets.left > 0.5 }
        let inset = owner.additionalSafeAreaInsets
        let cover = UIViewController()
        cover.modalPresentationStyle = .currentContext
        owner.definesPresentationContext = true
        owner.present(cover, animated: false)
        waitFor("the cover takes the routes out of the window") { owner.viewIfLoaded?.window == nil }
        XCTAssertNotNil(session.view?.window, "and leaves the session in it")
        navigation.reportCovers()
        XCTAssertEqual(owner.additionalSafeAreaInsets, inset, "the inset stands under the cover")
        XCTAssertTrue(navigation.siblings.holds(side), "as the lift does")
        cover.dismiss(animated: false)
        waitFor("the routes are back in the window") { owner.viewIfLoaded?.window != nil }
        XCTAssertEqual(owner.additionalSafeAreaInsets, inset)
    }

    /// Rows above and below the panels while the native bars show (LLP 1083.000 D4). The row above is beside the
    /// routes: it paints over the container, its content clears the status bar, and the container's top safe area
    /// reaches where the routes begin, so the navigation bar, and the route's content and cover, start below it. The
    /// row below is not beside them while the tab bar shows: it stays under the bar, which keeps its touches.
    func testARowAboveThePanelsTakesTheBarsBelowItAndARowBelowStaysUnderTheTabBar() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the screen's pixels need a window the system draws") }
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("show-rows", route: "grid")
        let top = try node("top-row"), mini = try node("mini-player"), win = try XCTUnwrap(window)
        waitFor("the row above the panels is lifted over the container") { navigation.siblings.holds(top) && top.bounds.height > 0 }
        let nav = try XCTUnwrap(navigation.primaryNavigation)
        waitFor("the navigation bar starts below the row") { abs(self.box(nav.navigationBar).minY - self.box(top).maxY) < 0.5 }
        let row = box(top), bar = box(nav.navigationBar)
        XCTAssertEqual(try pixel(at: CGPoint(x: row.midX, y: row.maxY - 4)), [18, 52, 86], "the row paints over the container")
        XCTAssertGreaterThanOrEqual(row.height, 44 + win.safeAreaInsets.top - row.minY - 0.5, "its content clears the status bar")
        XCTAssertGreaterThanOrEqual(box(try node("grid-1")).minY, bar.maxY - 0.5, "the route's content starts below the bar")
        let route = try node("route-grid")
        if case .edges(let edges)? = navigation.covers[route.id] {
            XCTAssertLessThanOrEqual(edges.top, bar.maxY - box(route).minY + 0.5, "the route's cover reaches no further than the bar")
        }
        navigation.reportCovers()
        XCTAssertFalse(navigation.siblings.holds(mini), "the row below the panels stays in its column")
        let aim = Agent(session: session).aim(["id": Int(try node("tab-grid").id), "aim": ["press": true]])
        XCTAssertNil(aim["error"], "a tab keeps its touches: \(aim)")
        let tabBar = try XCTUnwrap(navigation.tabController).tabBar
        // A vertical bar's box holds more than its items
        if !tabBar.traitCollection.placesVerticalBar {
            let point = tabBar.convert(CGPoint(x: tabBar.bounds.midX, y: tabBar.bounds.midY), to: win)
            XCTAssertTrue(win.hitTest(point, with: nil)?.isDescendant(of: tabBar) == true, "the tab bar keeps its touches")
        }
    }

    /// The mini player the tablist names (`navigationTabAccessory`) as UIKit's tab bar accessory (LLP 1075.003 §9.17).
    @available(iOS 26.0, *)
    private func accessory(_ name: String = "mini-player", by button: String = "use-accessory") throws -> (tabs: UITabBarController, content: UIView, mini: NodeView) {
        let navigation = try XCTUnwrap(session).presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press(button, route: "grid")
        let tabs = try XCTUnwrap(navigation.tabController), mini = try node(name)
        waitFor("UIKit holds the mini player as its tab bar's accessory, laid out in its box", facts: { "\(String(describing: mini.superview)), \(mini.frame)" }) {
            guard let content = tabs.bottomAccessory?.contentView, mini.superview === content, content.bounds.height > 0 else { return false }
            return mini.frame.size == content.bounds.size
        }
        return (tabs, try XCTUnwrap(tabs.bottomAccessory?.contentView), mini)
    }

    func testTheNamedRowIsTheTabBarsAccessoryInUIKitsBox() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        let (tabs, content, mini) = try accessory()
        XCTAssertNotEqual(mini.frame.height, 60, "UIKit's box, not the row's authored height: \(content.bounds)")
        let bottomBar = UIDevice.current.userInterfaceIdiom == .phone && !tabs.traitCollection.placesVerticalBar
        if bottomBar {
            XCTAssertLessThan(box(content).maxY, box(tabs.tabBar).minY + 0.5, "above the bar on the iPhone")
        }
        // The runner's frame and hit test find the row where UIKit shows it
        try probeAgrees(content, "mini-player", "above the bar")
        // An authored ancestor's double tap and long press reach a touch on the accessory
        let relay = try XCTUnwrap((content as? TabAccessoryContent)?.relay, "the content view carries a relay")
        let area = try node("player-area")
        area.handlers.formUnion(["contextmenu", "dblclick"])
        XCTAssertTrue(relay.listener(mini) { $0.handlers.contains("dblclick") } === area, "the area hears a double tap on the row")
        XCTAssertTrue(relay.listener(mini) { $0.handlers.contains("contextmenu") } === area, "and a long press")
        area.handlers.subtract(["contextmenu", "dblclick"])
        XCTAssertFalse(navigation.siblings.holds(mini), "the accessory is not lifted over the container")
        XCTAssertTrue(nodeParent(mini) === (try node("player-area")), "its authored parent is still its parent")
        // A window's snapshot leaves out UIKit's tab bar group, so the accessory's content is drawn on its own
        let drawn = UIGraphicsImageRenderer(bounds: content.bounds).image { content.layer.render(in: $0.cgContext) }
        let paint = try XCTUnwrap(SharePictureCase.rgba(try XCTUnwrap(drawn.cgImage), x: Int((content.bounds.maxX - 20) * drawn.scale), y: Int(content.bounds.midY * drawn.scale)))
        XCTAssertEqual([Int(paint.0), Int(paint.1), Int(paint.2)], [101, 67, 33], "the row paints in the accessory")
        let (_, last) = try end(of: "grid")
        XCTAssertLessThanOrEqual(last.maxY, box(content).minY + 0.5, "the grid's last row clears the accessory: \(last), accessory \(box(content))")
        let play = try node("mini-play")
        let reply = Agent(session: session).tap(["id": Int(play.id)])
        XCTAssertNil(reply["error"], "\(reply)")
        waitFor("the accessory's button takes its tap") { play.props["accessibilityLabel"] == "Pause" }
        // A point in the row clear of its button, relative to the row's box
        let empty: [Double] = [Double(mini.bounds.width - 8), Double(mini.bounds.height / 2)]
        XCTAssertNil(Agent(session: session).tap(["id": Int(mini.id), "at": empty])["error"])
        waitFor("a press on the accessory reaches its authored ancestor's listener") { area.props["accessibilityLabel"] == "Presses 1" }
        // The runner follows the accessory inline, where the bar was, as UIKit moves it
        if bottomBar {
            let above = box(content)
            try press("select-grid", route: "grid")
            waitFor("the accessory goes inline", facts: { "\(self.box(content))" }) { self.box(content) != above && mini.frame.size == content.bounds.size }
            try probeAgrees(content, "mini-player", "inline")
        }
        // The column's accent and tint adjustment reach the accessory, which UIKit's tab container would tint otherwise
        try press("accent-column", route: "grid")
        try press("keep-tint", route: "grid")
        func rgb(_ color: UIColor) -> [Int] {
            var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
            color.getRed(&r, green: &g, blue: &b, alpha: &a)
            return [r, g, b].map { Int(($0 * 255).rounded()) }
        }
        waitFor("the accessory takes the column's accent and tint adjustment", facts: { "\(rgb(content.tintColor)), \(content.tintAdjustmentMode.rawValue)" }) {
            content.tintAdjustmentMode == .normal && rgb(content.tintColor) == [255, 136, 0]
        }
        // An authored ancestor's `aria-hidden` hides the accessory from VoiceOver, though its view no longer holds it
        try press("hide-area", route: "grid")
        waitFor("the accessory follows the area's aria-hidden") { content.accessibilityElementsHidden }
        XCTAssertTrue(content.isUserInteractionEnabled, "hidden from VoiceOver, still touchable")
        // An inert authored ancestor makes the accessory inert, and an editor in it stops editing
        let field = UITextField(frame: CGRect(x: 0, y: 0, width: 40, height: 20))
        mini.addSubview(field)
        defer { field.removeFromSuperview() }
        XCTAssertTrue(field.becomeFirstResponder(), "the field edits")
        try press("inert-column", route: "grid")
        waitFor("the accessory follows the column's inertness") { !content.isUserInteractionEnabled && content.accessibilityElementsHidden }
        XCTAssertFalse(field.isFirstResponder, "the field stops editing")
    }

    /// Presses `button`, whose action writes `frame(id)` and what `elementFromPoint` finds near the box's trailing
    /// edge into its label, and checks both against where UIKit shows the accessory's content view.
    private func probeAgrees(_ content: UIView, _ id: String, by button: String = "probe-accessory", _ when: String) throws {
        let probe = try node(button), before = probe.props["accessibilityLabel"]
        try press(button, route: "grid")
        waitFor("the runner reads the accessory's frame \(when)", facts: { probe.props["accessibilityLabel"] ?? "" }) { probe.props["accessibilityLabel"] != before }
        let probed = (probe.props["accessibilityLabel"] ?? "").split(separator: " "), shown = box(content)
        let frame = probed.first.map { $0.split(separator: ",").compactMap { Double($0) } } ?? []
        XCTAssertEqual(frame.count, 4, "\(when): \(probed)")
        if frame.count == 4 {
            for (read, seen) in zip(frame, [shown.minX, shown.minY, shown.width, shown.height]) { XCTAssertEqual(read, Double(seen), accuracy: 0.5, "\(when): frame \(frame), shown \(shown)") }
        }
        XCTAssertEqual(probed.last.map(String.init), id, "\(when): elementFromPoint finds the row over the routes")
    }

    /// A row in flow after the positioned panels, which CSS paints under them: the runner finds it where UIKit draws
    /// it, above the routes, as it finds a presentation.
    func testTheRunnerFindsARowInFlowWhereUIKitDrawsIt() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        _ = try fixture(wholeScreen: true)
        let (_, content, _) = try accessory("bare-player", by: "use-bare-accessory")
        try probeAgrees(content, "bare-player", by: "probe-bare", "for a row in flow")
    }

    func testTheAccessoryGoesInlineWhereTheBarWasAndStaysReadable() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("an iPhone's bottom tab bar, where the accessory goes inline") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let navigation = try fixture(wholeScreen: true).presenter.navigationHost
        // A row beside the panels, which the walk that lifts rows below them while the bar hides reaches, once
        // another row of their column lifts and the walk reads the column's children
        try tap("tab-grid", route: "grid")
        try press("show-rows", route: "grid")
        let (tabs, content, mini) = try accessory("bare-player", by: "use-bare-accessory")
        guard !tabs.traitCollection.placesVerticalBar else { throw XCTSkip("a vertical tab bar, where the accessory stays as it is") }
        let above = content.bounds.size
        try press("select-grid", route: "grid")
        waitFor("the tab bar hides and the accessory goes inline", facts: { "\(content.bounds), \(mini.frame)" }) {
            tabs.isTabBarHidden && content.bounds.size != above && mini.frame.size == content.bounds.size
        }
        XCTAssertEqual(content.traitCollection.tabAccessoryEnvironment, .inline)
        navigation.reportCovers()
        XCTAssertTrue(navigation.siblings.holds(try node("player-area")), "the area below the panels lifts")
        XCTAssertTrue(mini.superview === content && !navigation.siblings.holds(mini), "UIKit still holds it; nothing lifts it")
        let hidden = sequence(first: content as UIView, next: \.superview).filter(\.accessibilityElementsHidden)
        XCTAssertTrue(hidden.isEmpty, "VoiceOver still reaches the accessory while the bar is hidden: \(hidden)")
    }

    func testANameThatGoesPutsTheRowBackAndClearsTheAccessoryItTook() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let session = try fixture(wholeScreen: true), tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let hatched = UIView()
        tabs.bottomAccessory = UITabAccessory(contentView: hatched)
        try tap("tab-grid", route: "grid")
        // The row stays once its name goes
        try press("show-rows", route: "grid")
        let (_, content, mini) = try accessory()
        XCTAssertFalse(tabs.bottomAccessory?.contentView === hatched, "naming one replaces the hatch's accessory")
        try press("drop-accessory", route: "grid")
        let area = try node("player-area")
        waitFor("the row is back in its parent at its authored height", facts: { "\(String(describing: mini.superview)), \(mini.frame)" }) {
            tabs.bottomAccessory == nil && mini.superview === area.container && abs(mini.frame.height - 60) < 0.5
        }
        let left = content.subviews.filter { !($0 is NodeView) }
        XCTAssertTrue(left.count == 1 && left[0].accessibilityElementsHidden && !left[0].isUserInteractionEnabled,
                      "the leaving accessory shows a snapshot of the row that takes no input: \(content.subviews)")
        // The runner's frame follows the row back into the page
        let probe = try node("probe-accessory"), before = probe.props["accessibilityLabel"]
        try press("probe-accessory", route: "grid")
        waitFor("the runner reads the row's frame", facts: { probe.props["accessibilityLabel"] ?? "" }) { probe.props["accessibilityLabel"] != before }
        let frame = (probe.props["accessibilityLabel"] ?? "").split(separator: " ").first.map { $0.split(separator: ",").compactMap { Double($0) } } ?? []
        let page = box(mini)
        XCTAssertEqual(frame.count, 4)
        if frame.count == 4 {
            for (read, seen) in zip(frame, [page.minX, page.minY, page.width, page.height]) { XCTAssertEqual(read, Double(seen), accuracy: 0.5, "frame \(frame), page \(page)") }
        }
    }

    func testAnElementThatGoesLeavesItsPictureForTheExit() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let session = try fixture(wholeScreen: true), tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let (_, content, mini) = try accessory()
        // The batch destroys the row, which the accessory drew before its ops ran
        try press("drop-accessory", route: "grid")
        waitFor("the accessory goes with its row") { tabs.bottomAccessory == nil && session.presenter.views[mini.id] == nil }
        let left = content.subviews.filter { !($0 is NodeView) }
        XCTAssertTrue(left.count == 1 && left[0] is UIImageView && left[0].accessibilityElementsHidden, "the row's picture stays for UIKit's exit: \(content.subviews)")
        XCTAssertTrue(!content.isUserInteractionEnabled && content.accessibilityElementsHidden, "the leaving accessory takes no touch and VoiceOver passes it")
    }

    /// A row with an authored exit leaves UIKit's exit a picture of it as the exit began, before the exit's first frame
    /// changes it.
    func testARowWithAnAuthoredExitLeavesItsPictureAsTheExitBegan() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let tabs = try XCTUnwrap(try fixture(wholeScreen: true).presenter.navigationHost.tabController)
        let (_, content, _) = try accessory("exit-player", by: "use-exit-accessory")
        try press("drop-exit-row", route: "grid")
        waitFor("the accessory goes as the row's exit begins") { tabs.bottomAccessory == nil }
        let picture = try XCTUnwrap(content.subviews.compactMap { $0 as? UIImageView }.first, "\(content.subviews)")
        let image = try XCTUnwrap(picture.image?.cgImage), scale = try XCTUnwrap(picture.image?.scale)
        let paint = try XCTUnwrap(SharePictureCase.rgba(image, x: Int((picture.bounds.maxX - 20) * scale), y: Int(picture.bounds.midY * scale)))
        XCTAssertEqual([Int(paint.0), Int(paint.1), Int(paint.2)], [101, 67, 33], "the row as it was, not its exit's first frame")
    }

    /// The picture is the content view's layers as shown, the row's own transform included.
    func testThePictureKeepsTheRowsTransform() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        _ = try fixture(wholeScreen: true)
        let (_, content, mini) = try accessory()
        let accessory = try XCTUnwrap(content as? TabAccessoryContent)
        mini.transform = CGAffineTransform(scaleX: 0.5, y: 0.5)
        accessory.freeze()
        mini.transform = .identity
        defer { accessory.thaw() }
        let picture = try XCTUnwrap(content.subviews.compactMap { $0 as? UIImageView }.first, "\(content.subviews)")
        let image = try XCTUnwrap(picture.image?.cgImage), scale = try XCTUnwrap(picture.image?.scale)
        let edge = try XCTUnwrap(SharePictureCase.rgba(image, x: Int((picture.bounds.maxX - 20) * scale), y: Int(picture.bounds.midY * scale)))
        let middle = try XCTUnwrap(SharePictureCase.rgba(image, x: Int(picture.bounds.midX * scale), y: Int(picture.bounds.midY * scale)))
        XCTAssertEqual(edge.3, 0, "outside the row at half its size")
        XCTAssertEqual([Int(middle.0), Int(middle.1), Int(middle.2)], [101, 67, 33], "inside it")
    }

    /// While UIKit's content view is out of the window, the claimed row shows nowhere: it stays an open box with no
    /// area, so the runner finds it neither in the page nor over it.
    func testADetachedAccessoryShowsNowhereToTheRunner() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let navigation = try fixture(wholeScreen: true).presenter.navigationHost
        let (_, content, mini) = try accessory()
        content.removeFromSuperview()
        let box = try XCTUnwrap(navigation.topLayerBoxes.first?.frame, "the row is still an open box")
        XCTAssertEqual(box.0, mini.id)
        XCTAssertTrue(box.1.isEmpty && box.2?.isEmpty == true, "with no area: \(box)")
    }

    /// A batch that projects nothing claims a named row that was held back (here by a handoff), with no change to the
    /// tabs or the name.
    func testABatchThatProjectsNothingClaimsANamedRowOnceItIsFree() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let presenter = try fixture(wholeScreen: true).presenter, navigation = presenter.navigationHost
        let tabs = try XCTUnwrap(navigation.tabController)
        try tap("tab-grid", route: "grid")
        navigation.tabAccessoryHandingOff = true
        defer { navigation.tabAccessoryHandingOff = false }
        try press("use-accessory", route: "grid")
        let mini = try node("mini-player")
        XCTAssertNil(tabs.bottomAccessory, "held back, the row is not claimed")
        navigation.tabAccessoryHandingOff = false
        presenter.apply(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        XCTAssertTrue(mini.superview === tabs.bottomAccessory?.contentView, "the pass after the batch claims it: \(String(describing: mini.superview))")
    }

    /// A menu an authored ancestor gives (`contextPopover`) answers a long press on the accessory, as one answers a
    /// press on a presentation's mount, from the pass that first shows it.
    func testAnAncestorsMenuReachesTheAccessory() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let presenter = try fixture(wholeScreen: true).presenter, navigation = presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("show-rows", route: "grid")
        try press("give-area-menu", route: "grid")
        try press("use-accessory", route: "grid")
        waitFor("a pass claims the row") { navigation.tabAccessory?.holds == true }
        let content = try XCTUnwrap(navigation.tabAccessory)
        let interaction = try XCTUnwrap(content.interactions.lazy.compactMap { $0 as? UIContextMenuInteraction }.first,
                                        "the claim takes the area's menu interaction at once")
        waitFor("UIKit lays the row out") { content.bounds.height > 0 }
        // A native view in the row, which a touch there hits rather than the row's node
        let mini = try node("mini-player"), native = UIView(frame: CGRect(x: mini.bounds.maxX - 40, y: 0, width: 40, height: mini.bounds.height))
        mini.addSubview(native)
        defer { native.removeFromSuperview() }
        let point = CGPoint(x: content.bounds.maxX - 20, y: content.bounds.midY)
        XCTAssertTrue(content.hitTest(point, with: nil) === native, "the touch lands on the native view")
        XCTAssertNotNil(interaction.delegate?.contextMenuInteraction(interaction, configurationForMenuAtLocation: point),
                        "the area's menu answers a long press on the row")
        // A run that hears a long press is nearer than every node: its paragraph's own recognizer takes a press on it
        let title = try node("mini-title"), onRun = title.convert(CGPoint(x: title.bounds.midX, y: title.bounds.midY), to: content)
        XCTAssertNotNil(title.inlineContextTarget(at: content.convert(onRun, to: title)), "the run lies under the touch")
        do {
            // A native view over the run, which the touch hits rather than the paragraph
            let cover = UIView(frame: title.bounds)
            title.addSubview(cover)
            defer { cover.removeFromSuperview() }
            XCTAssertTrue(content.hitTest(onRun, with: nil) === cover, "the touch lands on the view over the run")
            XCTAssertNil(interaction.delegate?.contextMenuInteraction(interaction, configurationForMenuAtLocation: onRun),
                         "the run's long press answers, not the area's menu")
        }
        // A nearer node that hears a long press takes it from the farther menu
        mini.handlers.insert("contextmenu")
        XCTAssertNil(interaction.delegate?.contextMenuInteraction(interaction, configurationForMenuAtLocation: point),
                     "the row's own long press answers instead")
        mini.handlers.remove("contextmenu")
        do {
            // A menu the row gives itself takes the press from a farther plain listener, which the relay leaves idle
            let area = try node("player-area"), relay = try XCTUnwrap(content.relay), areaMenu = area.props["contextPopover"]
            area.props["contextPopover"] = nil
            area.handlers.insert("contextmenu")
            defer { area.handlers.remove("contextmenu"); area.props["contextPopover"] = areaMenu }
            XCTAssertTrue(relay.longPressListener(native, at: point) === area, "the area hears a long press on the row")
            XCTAssertNil(relay.longPressListener(title, at: onRun), "but not one on the run, which hears it itself")
            mini.props["contextPopover"] = "area-menu"
            defer { mini.props["contextPopover"] = nil }
            XCTAssertNil(relay.longPressListener(native, at: point), "the row's own menu takes it")
        }
        let context = presenter.menus.context
        do {
            // A menu on the root, whose view holds the tab bar's, opens from the accessory through the mount's
            // interaction, as the root's own leaves it the press, and yields to a nearer menu
            let root = try XCTUnwrap(navigation.container), rootInteraction = UIContextMenuInteraction(delegate: context)
            let area = try node("player-area"), rootMenu = root.props["contextPopover"], areaMenu = area.props["contextPopover"]
            root.addInteraction(rootInteraction)
            root.props["contextPopover"] = "area-menu"
            area.props["contextPopover"] = nil
            defer { root.removeInteraction(rootInteraction); root.props["contextPopover"] = rootMenu; area.props["contextPopover"] = areaMenu }
            let rootPoint = content.convert(point, to: root)
            XCTAssertTrue(root.hitTest(rootPoint, with: nil) === native, "the root's hit test reaches the row")
            XCTAssertNil(context.contextMenuInteraction(rootInteraction, configurationForMenuAtLocation: rootPoint),
                         "the root's own interaction leaves the press")
            XCTAssertNotNil(interaction.delegate?.contextMenuInteraction(interaction, configurationForMenuAtLocation: point))
            XCTAssertTrue(context.open?.source === root, "the mount's opens the root's menu")
            area.props["contextPopover"] = areaMenu
            XCTAssertNotNil(interaction.delegate?.contextMenuInteraction(interaction, configurationForMenuAtLocation: point))
            XCTAssertTrue(context.open?.source === area, "a nearer menu takes the press")
            // The root's own long press, where it only listens, leaves the press to that nearer menu
            let rootHandlers = root.handlers
            root.props["contextPopover"] = rootMenu
            root.handlers.insert("contextmenu")
            defer { root.handlers = rootHandlers }
            XCTAssertFalse(root.hearsLongPress(on: native, at: rootPoint), "the root's long press leaves it to the area's menu")
            area.props["contextPopover"] = nil
            XCTAssertTrue(root.hearsLongPress(on: native, at: rootPoint), "and hears it where nothing nearer listens")
        }
        // The shown menu ends with the accessory and gives the focus back, as UIKit calls no `willEnd` for an
        // interaction it no longer holds, and gives no rows or preview when UIKit asks after that
        let configuration = try XCTUnwrap(interaction.delegate?.contextMenuInteraction(interaction, configurationForMenuAtLocation: point))
        let owner = try XCTUnwrap(context.open, "the area's menu opens")
        context.contextMenuInteraction(interaction, willDisplayMenuFor: configuration, animator: nil)
        XCTAssertTrue(presenter.menus.focus.suspended, "the shown menu sets the focus aside")
        try press("drop-accessory", route: "grid")
        waitFor("the pass lets the row go") { navigation.tabAccessory == nil }
        XCTAssertNil(context.open, "the menu ended with the accessory")
        XCTAssertFalse(presenter.menus.focus.suspended, "the focus is back")
        XCTAssertNil(context.menu(owner), "it gives no rows")
        XCTAssertNil(context.lift(owner), "or preview")
    }

    /// A field editing in a row that the tablist comes to name goes on editing: UIKit holds the content view in the
    /// window before the row moves into it.
    func testAFieldInARowThatComesToBeNamedKeepsEditing() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        _ = try fixture(wholeScreen: true)
        try tap("tab-grid", route: "grid")
        try press("show-rows", route: "grid")
        let mini = try node("mini-player")
        let field = UITextField(frame: CGRect(x: 0, y: 0, width: 40, height: 20)), ending = EditEnd {}
        field.delegate = ending
        mini.addSubview(field)
        defer { field.removeFromSuperview() }
        XCTAssertTrue(field.becomeFirstResponder(), "the field edits")
        _ = try accessory()
        XCTAssertTrue(field.isFirstResponder && ending.count == 0, "the field still edits: \(ending.count) edits ended")
    }

    /// A radio its group shows as a segment has no view of its own in the page: the tab bar shows no accessory, and the
    /// segmented control keeps both its segments.
    func testARadioShownAsASegmentIsNoAccessory() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let tabs = try XCTUnwrap(try fixture(wholeScreen: true).presenter.navigationHost.tabController)
        try tap("tab-grid", route: "grid")
        try press("use-speed-accessory", route: "grid")
        let group = try node("speed")
        func segmented(in view: UIView) -> UISegmentedControl? {
            (view as? UISegmentedControl) ?? view.subviews.lazy.compactMap(segmented(in:)).first
        }
        var control: UISegmentedControl?
        waitFor("the group shows as a segmented control") { control = segmented(in: group); return control != nil }
        XCTAssertNil(tabs.bottomAccessory, "no accessory")
        XCTAssertEqual(control?.numberOfSegments, 2, "both segments stay")
    }

    func testSwitchingTheNamedRowMovesTheAccessoryOnce() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let tabs = try XCTUnwrap(try fixture(wholeScreen: true).presenter.navigationHost.tabController)
        try tap("tab-grid", route: "grid")
        try press("show-rows", route: "grid")
        let (_, _, mini) = try accessory()
        // One batch names the other row: the release's UIKit callbacks run the covers while the pass replaces it
        try press("switch-accessory", route: "grid")
        let bare = try node("bare-player"), area = try node("player-area")
        waitFor("the other row shows and the mini player is back", facts: { "\(String(describing: mini.superview)), \(String(describing: bare.superview))" }) {
            bare.superview === tabs.bottomAccessory?.contentView && mini.superview === area.container
        }
        let held = (tabs.bottomAccessory?.contentView.subviews ?? []).filter { $0 is NodeView }
        XCTAssertEqual(held.count, 1, "one row in the accessory: \(held)")
    }

    func testARowHiddenAndRestyledInOneBatchLeavesItsPictureAsItWas() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let tabs = try XCTUnwrap(try fixture(wholeScreen: true).presenter.navigationHost.tabController)
        let (_, content, _) = try accessory()
        // The batch hides the row and paints it black: the picture is drawn before its ops ran
        try press("hide-player", route: "grid")
        waitFor("the accessory goes with the row's box") { tabs.bottomAccessory == nil }
        let picture = try XCTUnwrap(content.subviews.compactMap { $0 as? UIImageView }.first, "\(content.subviews)")
        let image = try XCTUnwrap(picture.image?.cgImage), scale = try XCTUnwrap(picture.image?.scale)
        let paint = try XCTUnwrap(SharePictureCase.rgba(image, x: Int((picture.bounds.maxX - 20) * scale), y: Int(picture.bounds.midY * scale)))
        XCTAssertEqual([Int(paint.0), Int(paint.1), Int(paint.2)], [101, 67, 33], "the row as it was, before the batch")
    }

    /// A batch that projects nothing (a scroller's rows) still reaches the accessory once it ends: a picture drawn for
    /// it goes, and the accessory follows its authored ancestors' `aria-hidden` and inertness.
    func testABatchThatProjectsNothingStillReachesTheAccessory() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let presenter = try fixture(wholeScreen: true).presenter
        let (_, content, mini) = try accessory()
        let area = try node("player-area"), nothing = Batch(ops: [], timers: false, motion: false, clock: nil, error: nil)
        let pictured = { content.subviews.contains { $0 is UIImageView } }
        try XCTUnwrap(content as? TabAccessoryContent).freeze()
        XCTAssertTrue(pictured(), "a picture covers the row")
        presenter.apply(nothing)
        XCTAssertFalse(pictured(), "the pass after the batch takes it away")
        // As a batch's props ops leave the area under a scroller
        area.props["accessibilityElementsHidden"] = "true"
        presenter.apply(nothing)
        XCTAssertTrue(content.accessibilityElementsHidden, "the accessory follows the area's aria-hidden")
        let field = UITextField(frame: CGRect(x: 0, y: 0, width: 40, height: 20))
        mini.addSubview(field)
        defer { field.removeFromSuperview() }
        XCTAssertTrue(field.becomeFirstResponder(), "the field edits")
        area.props["inert"] = "true"
        presenter.apply(nothing)
        XCTAssertFalse(content.isUserInteractionEnabled, "and its inertness")
        XCTAssertFalse(field.isFirstResponder, "which ends the field's editing")
    }

    /// A callback that retires the tabs while a pass keeps the accessory (an editor in it ending its edit as it goes
    /// inert) leaves the retired controller without Exact's accessory: the edit ends before the pass decides.
    func testTabsRetiredByACallbackDuringAPassKeepTheirPropertyClear() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let presenter = try fixture(wholeScreen: true).presenter, navigation = presenter.navigationHost
        let (tabs, _, mini) = try accessory()
        let field = UITextField(frame: CGRect(x: 0, y: 0, width: 40, height: 20)), ending = EditEnd { navigation.retireTabs() }
        field.delegate = ending
        mini.addSubview(field)
        defer { field.removeFromSuperview() }
        XCTAssertTrue(field.becomeFirstResponder(), "the field edits")
        try node("player-area").props["inert"] = "true"
        presenter.apply(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        XCTAssertEqual(ending.count, 1, "the pass ended the field's edit")
        XCTAssertNil(tabs.bottomAccessory, "the retired controller has no accessory: \(String(describing: tabs.bottomAccessory?.contentView))")
    }

    /// The runner is told the accessory's box only where UIKit's hit test reaches it: within every view that holds it,
    /// each taking touches.
    func testTheAccessorysBoxIsClippedWhereUIKitsHitTestReachesIt() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let navigation = try fixture(wholeScreen: true).presenter.navigationHost
        let (tabs, content, _) = try accessory()
        func shown() throws -> TopLayerFrame { try XCTUnwrap(navigation.topLayerBoxes.first?.frame, "the accessory's box") }
        let whole = try shown(), rest = try XCTUnwrap(whole.2)
        for (clip, box) in zip([rest.minX, rest.minY, rest.width, rest.height], [whole.1.minX, whole.1.minY, whole.1.width, whole.1.height]) {
            XCTAssertEqual(clip, box, accuracy: 0.5, "nothing clips it at rest: \(whole)")
        }
        // A root that ends halfway down the row, which stays on screen: a touch reaches the half outside where the
        // root's overflow is visible (`NodeView.hitTest`), and only the half within, inside its padding edge, where
        // it clips
        let root = try XCTUnwrap(navigation.container), mini = try node("mini-player")
        XCTAssertTrue(tabs.view.superview === root, "the root holds the tab container")
        let frame = root.frame, mask = tabs.view.autoresizingMask, overflow = root.style["overflow_y"], border = root.style["border_width_bottom"]
        tabs.view.autoresizingMask = []
        root.frame.size.height = content.convert(content.bounds, to: root).midY
        let outside = content.convert(CGPoint(x: content.bounds.midX, y: content.bounds.height * 0.75), to: root)
        let half = try shown(), reachesOutside = root.hitTest(outside, with: nil)?.isDescendant(of: mini) == true
        root.style["overflow_y"] = .string("clip")
        let clipped = try shown()
        root.style["border_width_bottom"] = .number(10)
        let bordered = try shown()
        root.style["overflow_y"] = overflow
        root.style["border_width_bottom"] = border
        root.frame = frame
        tabs.view.autoresizingMask = mask
        XCTAssertTrue(reachesOutside, "a touch reaches the half outside the root")
        XCTAssertEqual(half.2?.height ?? 0, whole.1.height, accuracy: 1, "so the runner is told all of it: \(half)")
        XCTAssertEqual(clipped.2?.height ?? 0, whole.1.height / 2, accuracy: 1, "and the half within a root that clips: \(clipped)")
        XCTAssertEqual(bordered.2?.height ?? 0, whole.1.height / 2 - 10, accuracy: 1, "inside its padding edge: \(bordered)")
        // A view that holds it and refuses touches
        tabs.view.isUserInteractionEnabled = false
        let refused = try shown()
        tabs.view.isUserInteractionEnabled = true
        XCTAssertEqual(refused.2?.isEmpty, true, "UIKit's hit test does not reach it: \(refused)")
        // A faded navigation root, whose CSS opacity leaves it taking touches (`NodeView.hitTest`)
        let point = CGPoint(x: content.bounds.midX, y: content.bounds.midY), opacity = root.alpha
        root.alpha = 0
        let faded = try shown(), reaches = root.hitTest(content.convert(point, to: root), with: nil)?.isDescendant(of: mini) == true
        root.alpha = opacity
        XCTAssertTrue(reaches, "a touch still reaches the row")
        XCTAssertEqual(faded.2?.height ?? 0, whole.1.height, accuracy: 0.5, "and the runner is told so: \(faded)")
        // The content view is no target of its own: a touch the row lets through passes on
        XCTAssertTrue(content.hitTest(point, with: nil)?.isDescendant(of: mini) == true, "the row takes a touch on it")
        mini.isHidden = true
        defer { mini.isHidden = false }
        XCTAssertNil(content.hitTest(point, with: nil), "the content view takes none")
    }

    /// On the iPad, the accessory follows UIKit's tab sidebar as it shows: the same row stays claimed, laid out in
    /// UIKit's narrower box, and the route clears it.
    func testTheAccessoryFollowsTheIPadSidebar() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard UIDevice.current.userInterfaceIdiom == .pad else { throw XCTSkip("the iPad's tab sidebar") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        _ = try fixture(wholeScreen: true)
        let (tabs, content, mini) = try accessory()
        XCTAssertEqual(tabs.mode, .tabSidebar, "an unsaid tablist adapts to the sidebar")
        let without = content.bounds.size
        tabs.sidebar.isHidden = false
        waitFor("the sidebar shows and the accessory narrows beside it", facts: { "\(content.bounds), \(mini.frame)" }) {
            content.bounds.width < without.width && mini.frame.size == content.bounds.size
        }
        XCTAssertTrue(mini.superview === content && tabs.bottomAccessory?.contentView === content, "the same row stays claimed")
        let (_, last) = try end(of: "grid")
        XCTAssertLessThanOrEqual(last.maxY, box(content).minY + 0.5, "the grid's last row clears the accessory: \(last), \(box(content))")
    }

    /// A field in a row whose name goes keeps editing as the row returns to the page, since the row never left the
    /// document.
    func testAFieldInARowWhoseNameGoesKeepsEditing() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        _ = try fixture(wholeScreen: true)
        try tap("tab-grid", route: "grid")
        // The row stays once its name goes
        try press("show-rows", route: "grid")
        let (tabs, _, mini) = try accessory()
        let field = UITextField(frame: CGRect(x: 0, y: 0, width: 40, height: 20))
        mini.addSubview(field)
        defer { field.removeFromSuperview() }
        XCTAssertTrue(field.becomeFirstResponder(), "the field edits")
        try press("drop-accessory", route: "grid")
        let area = try node("player-area")
        waitFor("the row is back in its parent") { tabs.bottomAccessory == nil && mini.superview === area.container }
        XCTAssertTrue(field.isFirstResponder, "the field still edits")
    }

    /// An edit that ends as the row goes inert runs its callbacks before the pass reads what to show: a callback that
    /// takes the row leaves no accessory without it.
    func testAnEditEndingAsTheRowGoesInertComesBeforeThePassDecides() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let presenter = try fixture(wholeScreen: true).presenter
        let (tabs, _, mini) = try accessory()
        let holder = UIView(), field = UITextField(frame: CGRect(x: 0, y: 0, width: 40, height: 20))
        let ending = EditEnd { holder.addSubview(mini) }
        field.delegate = ending
        mini.addSubview(field)
        defer { field.removeFromSuperview() }
        XCTAssertTrue(field.becomeFirstResponder(), "the field edits")
        try node("player-area").props["inert"] = "true"
        presenter.apply(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        XCTAssertEqual(ending.count, 1, "the pass ended the field's edit")
        let shown = tabs.bottomAccessory?.contentView
        XCTAssertTrue(shown == nil || mini.superview === shown, "no accessory shows without the row: \(String(describing: shown?.subviews))")
    }

    func testARowWhoseNameGoesReturnsToItsPlaceAmongItsSiblings() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        let tabs = try XCTUnwrap(try fixture(wholeScreen: true).presenter.navigationHost.tabController)
        try tap("tab-grid", route: "grid")
        try press("show-rows", route: "grid")
        let (_, content, mini) = try accessory()
        // A sibling written just after it comes while UIKit holds it, in an area whose children are all views, which
        // keeps no order of its own for a child that is not in its view
        try press("show-mid-row", route: "grid")
        _ = try node("area-after")
        XCTAssertTrue(mini.superview === content, "UIKit still holds the row")
        try press("drop-accessory", route: "grid")
        let area = try node("player-area")
        waitFor("the row is back in its area", facts: { "\(String(describing: mini.superview))" }) { tabs.bottomAccessory == nil && mini.superview === area.container }
        XCTAssertNotNil((content as? TabAccessoryContent)?.picture, "UIKit's exit shows a picture of the row")
        let order = area.container.subviews.compactMap { ($0 as? NodeView)?.props["testId"] }
        XCTAssertEqual(Array(order.prefix(2)), ["mini-player", "area-after"], "before the button that came after it while it was held")
    }

    /// A route offset in its block clears the part of each edge's inset its own box reaches: one below a row above the
    /// panels, past a sidebar, is covered at the top for the bar alone and at its leading edge for nothing (LLP
    /// 1083.000 D4).
    func testARouteOffsetPastARowAndASidebarIsCoveredOnlyForWhatItsBoxDoesNotClear() {
        let safe = NavigationHost.routeSafe(UIEdgeInsets(top: 160, left: 80, bottom: 34, right: 0),
                                            box: CGRect(x: 80, y: 106, width: 322, height: 768), in: CGRect(x: 0, y: 0, width: 402, height: 874))
        XCTAssertEqual(safe, UIEdgeInsets(top: 54, left: 0, bottom: 34, right: 0))
    }

    /// A drawer nested after the panels and positioned over the routes, which its column paints after the
    /// panels' holder: it paints over the container that stands for them, takes its touches, and the agent taps
    /// it; an overlay over the routes, it insets nothing (LLP 1083.000 D4).
    func testAButtonNestedOverTheRoutesPaintsOverTheContainer() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the screen's pixels need a window the system draws") }
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("show-fab", route: "grid")
        let fab = try node("fab"), win = try XCTUnwrap(window)
        waitFor("the button is lifted over the container") { fab.window != nil && navigation.siblings.holds(fab) }
        let point = fab.convert(CGPoint(x: fab.bounds.midX, y: fab.bounds.maxY - 6), to: win)
        let hit = try XCTUnwrap(win.hitTest(point, with: nil))
        XCTAssertTrue(hit === fab || hit.isDescendant(of: fab), "the button takes its touches, not \(hit)")
        XCTAssertEqual(try pixel(at: point), [255, 0, 0], "the button paints over the container")
        XCTAssertEqual(try XCTUnwrap(navigation.primaryOwner).additionalSafeAreaInsets.right, 0, accuracy: 0.5, "an overlay insets nothing")
        let reply = Agent(session: session).tap(["id": Int(fab.id)])
        XCTAssertNil(reply["error"], "\(reply)")
        // Events follow the authored tree from it: its column's inertness is its own
        XCTAssertFalse(fab.authoredInert)
        try press("inert-column", route: "grid")
        waitFor("the column is inert") { (try? self.node("panels-column"))?.props["inert"] == "true" }
        XCTAssertTrue(fab.authoredInert, "the lifted button is inert with its column")
    }

    /// A box with no area positioned over the routes, after the panels, whose button shows outside it: it is
    /// decided by the box its button paints over, so it lifts with the button, which paints over the container
    /// and takes its touches, and the layer reads the button through it, as a node's view reads such a box
    /// (LLP 1083.000 D4).
    func testABoxWithNoAreaOverTheRoutesLiftsWithTheButtonItHolds() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the screen's pixels need a window the system draws") }
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("show-wrapped-fab", route: "grid")
        let wrapper = try node("fab-wrapper"), button = try node("wrapped-fab"), win = try XCTUnwrap(window)
        waitFor("the box is lifted over the container") { wrapper.window != nil && navigation.siblings.holds(wrapper) }
        XCTAssertTrue(wrapper.bounds.isEmpty, "the box has no area")
        let point = button.convert(CGPoint(x: button.bounds.midX, y: button.bounds.maxY - 6), to: win)
        let hit = try XCTUnwrap(win.hitTest(point, with: nil))
        XCTAssertTrue(hit === button || hit.isDescendant(of: button), "the button takes its touches, not \(hit)")
        XCTAssertEqual(try pixel(at: point), [0, 0, 255], "the button paints over the container")
        let layer = try XCTUnwrap(navigation.siblings.layer), mount = try XCTUnwrap(layer.superview)
        XCTAssertNil(mount.accessibilityElements, "the mount leaves its children to UIKit, the layer among them")
        XCTAssertEqual((layer.accessibilityElements ?? []).filter { ($0 as AnyObject) === button }.count, 1, "the layer reads the button once")
    }

    /// A button over the routes that straddles the bottom edge of its column, which clips only its x axis
    /// (`overflow-x: clip`): lifted, its stand keeps the column's clip box's extent on x and spans the layer on
    /// y, also once the box has measured its reach without the button, so the button shows and takes its
    /// touches past the box, cut only where the column clips it (LLP 1083.000 D4).
    func testALiftedButtonUnderAOneAxisClipShowsPastItsColumnOnTheOtherAxis() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("setTabBarHidden is iOS 18") }
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        // Select mode puts its bar below the column, which then ends above the screen's bottom
        try press("select-grid", route: "grid")
        let bar = try node("selection-bar")
        waitFor("the bar is shown") { bar.window != nil && bar.bounds.height > 0 }
        try press("show-edge-button", route: "grid")
        let button = try node("edge-button"), column = try node("panels-column")
        waitFor("the button is lifted over the container") { button.window != nil && navigation.siblings.holds(button) }
        let clipBox = try XCTUnwrap(column.clipBox, "the column clips one axis in a clip box"), stand = try XCTUnwrap(button.superview)
        // The box measures its reach again, as a move of what it holds makes it, now without the lifted button
        column.syncShadowClip()
        navigation.siblings.place()
        XCTAssertGreaterThan(button.frame.maxY, clipBox.bounds.maxY + 0.5, "the button straddles the box's bottom, past its reach")
        XCTAssertTrue(stand.clipsToBounds, "the stand clips as the box does")
        XCTAssertEqual(stand.bounds.minX, clipBox.bounds.minX, accuracy: 0.5, "on x, the box's extent")
        XCTAssertEqual(stand.bounds.width, clipBox.bounds.width, accuracy: 0.5)
        let layer = try XCTUnwrap(navigation.siblings.layer), span = layer.convert(layer.bounds, to: column.container)
        XCTAssertEqual(stand.bounds.minY, span.minY, accuracy: 0.5, "on y, the layer's extent")
        XCTAssertEqual(stand.bounds.maxY, span.maxY, accuracy: 0.5)
        let touch = CGPoint(x: button.frame.midX, y: button.frame.maxY - 4)
        XCTAssertGreaterThan(touch.y, clipBox.bounds.maxY, "the touch is past the box")
        let hit = try XCTUnwrap(stand.hitTest(touch, with: nil), "a touch on the button past the box")
        XCTAssertTrue(hit === button || hit.isDescendant(of: button), "the button takes it, not \(hit)")
    }

    /// A button beside a tab's routes, in its panel, which the panel paints after them: it paints over the
    /// container while its tab is shown, and goes back under it when another tab is, as the web hides the
    /// panel (LLP 1083.000 D4).
    func testAButtonInATabPanelPaintsOverTheContainerOnlyWhileItsTabIsShown() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the screen's pixels need a window the system draws") }
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("show-fab", route: "grid")
        let button = try node("panel-fab"), win = try XCTUnwrap(window)
        waitFor("the button is lifted over the container") { button.window != nil && navigation.siblings.holds(button) }
        let point = button.convert(CGPoint(x: button.bounds.midX, y: button.bounds.maxY - 6), to: win)
        let hit = try XCTUnwrap(win.hitTest(point, with: nil))
        XCTAssertTrue(hit === button || hit.isDescendant(of: button), "the button takes its touches, not \(hit)")
        XCTAssertEqual(try pixel(at: point), [0, 255, 0], "the button paints over the container")
        try tap("tab-scrolls", route: "scrolls")
        waitFor("it goes back into its panel") { !navigation.siblings.holds(button) }
        let after = try XCTUnwrap(win.hitTest(point, with: nil))
        XCTAssertFalse(after === button || after.isDescendant(of: button), "the hidden tab's button takes no touch")
    }

    /// A lifted bar that leaves with an exit animation plays it to its end where it was lifted, over the container,
    /// and its column stacks as it starts; once it has gone, the next batch takes the layer that held it away (LLP
    /// 1083.000 D4).
    func testALiftedBarPlaysItsExitOverTheContainer() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("setTabBarHidden is iOS 18") }
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("wrap-bar", route: "grid")
        try press("fade-bar", route: "grid")
        try press("select-grid", route: "grid")
        let bar = try node("selection-bar"), column = try node("panels-column")
        waitFor("the bar is lifted") { navigation.siblings.holds(bar) }
        try press("select-grid", route: "grid")
        waitFor("the bar leaves") { session.presenter.isLeaving(bar) }
        XCTAssertGreaterThan(column.paintGhosts, 0, "its column stacks as it starts")
        var moved = false
        waitFor("the exit ends", late: 3) {
            if bar.superview != nil, navigation.siblings.parent(of: bar) !== column { moved = true }
            return bar.superview == nil
        }
        XCTAssertFalse(moved, "it plays its exit where it was lifted, to its end")
        try press("inset-bar", route: "grid")
        XCTAssertNil(navigation.siblings.layer, "the layer goes with the last node it held")
    }

    /// A lifted bar that leaves keeps its index among the nodes lifted with it, as its column would keep its ghost's
    /// (LLP 1083.000 §4 item 3), so the drawer after it in the tree paints over it while it plays.
    func testALeavingBarKeepsItsPlaceUnderTheNodesLiftedAfterIt() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("setTabBarHidden is iOS 18") }
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("wrap-bar", route: "grid")
        try press("fade-bar", route: "grid")
        try press("show-fab", route: "grid")
        try press("select-grid", route: "grid")
        let bar = try node("selection-bar"), drawer = try node("fab")
        waitFor("the bar and the drawer are lifted") { navigation.siblings.holds(bar) && navigation.siblings.holds(drawer) }
        let stand = try XCTUnwrap(bar.superview)
        XCTAssertTrue(drawer.superview === stand, "into the stand for what the column paints after the panels")
        XCTAssertTrue(bar.frame.intersects(drawer.frame), "where they overlap")
        try press("select-grid", route: "grid")
        waitFor("the bar leaves") { session.presenter.isLeaving(bar) }
        navigation.reportCovers()
        let barAt = try XCTUnwrap(stand.subviews.firstIndex(of: bar)), drawerAt = try XCTUnwrap(stand.subviews.firstIndex(of: drawer))
        XCTAssertLessThan(barAt, drawerAt, "the drawer still paints over the leaving bar")
    }

    /// A node put back into its column takes its place among the column's flat boxes at once (LLP 1068 §6.1): the box
    /// after the sidebar in the tree paints over it again, with no later batch to lay the boxes out.
    func testASidebarPutBackKeepsTheFlatBoxAfterItOverIt() throws {
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("show-sidebar", route: "grid")
        let side = try node("side"), column = try node("panels-column"), owner = try XCTUnwrap(navigation.primaryOwner)
        waitFor("the sidebar is lifted over the container") { navigation.siblings.holds(side) }
        let flats = session.presenter.flats, order = try XCTUnwrap(flats.children(of: column.id))
        let at = try XCTUnwrap(order.firstIndex(of: side.id))
        let strip = try XCTUnwrap(order.indices.contains(at + 1) ? flats.leaves[order[at + 1]] : nil, "the flat box after the sidebar")
        // A report that lifts nothing, as More's list makes, puts the sidebar back
        navigation.siblings.apply([], in: navigation.ownerMount(try XCTUnwrap(navigation.container)), after: try XCTUnwrap(owner.view))
        XCTAssertFalse(navigation.siblings.holds(side), "the sidebar is back in its column")
        let layers = try XCTUnwrap(column.container.layer.sublayers)
        let sideAt = try XCTUnwrap(layers.firstIndex(of: side.layer)), stripAt = try XCTUnwrap(layers.firstIndex(of: strip.layer))
        XCTAssertGreaterThan(stripAt, sideAt, "the flat box paints over the sidebar, as the tree orders them")
    }

    /// The window's pixel at `point`, drawn as the screen shows it.
    private func pixel(at point: CGPoint) throws -> [Int] {
        let win = try XCTUnwrap(window)
        let image = UIGraphicsImageRenderer(bounds: win.bounds).image { _ in _ = win.drawHierarchy(in: win.bounds, afterScreenUpdates: true) }
        let rgba = try XCTUnwrap(SharePictureCase.rgba(try XCTUnwrap(image.cgImage), x: Int(point.x * image.scale), y: Int(point.y * image.scale)))
        return [Int(rgba.0), Int(rgba.1), Int(rgba.2)]
    }

    private func replacementBar(pushed: Bool = false, inset: Bool = false, wrapped: Bool = false) throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("setTabBarHidden is iOS 18") }
        let session = try fixture(wholeScreen: true), p = session.presenter
        try tap("tab-grid", route: "grid")
        if pushed { try press("open-grid", route: "gridPushed") }
        let name = pushed ? "gridPushed" : "grid"
        if inset { try press("inset-bar", route: name) }
        if wrapped { try press("wrap-bar", route: name) }
        let tabs = try XCTUnwrap(p.navigationHost.tabController)
        let route = try XCTUnwrap(p.navigationHost.activeRoute)
        let scroll = try XCTUnwrap(try node("content-\(name)").scrollView)
        scrollToEnd(scroll)
        let originalInset = scroll.adjustedContentInset.bottom
        let originalOffset = scroll.contentOffset.y
        let originalFrame = box(scroll)
        XCTAssertGreaterThan(originalInset, 0)

        let host = try XCTUnwrap(window?.rootViewController)
        let reference = UIViewController(), native = UIScrollView()
        host.addChild(reference)
        reference.view.frame = scroll.convert(scroll.bounds, to: host.view)
        host.view.insertSubview(reference.view, at: 0)
        reference.didMove(toParent: host)
        defer {
            reference.willMove(toParent: nil)
            reference.view.removeFromSuperview()
            reference.removeFromParent()
        }
        native.frame = reference.view.bounds
        native.contentInsetAdjustmentBehavior = .always
        reference.view.addSubview(native)
        reference.view.layoutIfNeeded()
        let systemBottom = reference.view.safeAreaInsets.bottom
        reference.additionalSafeAreaInsets.bottom = originalInset - systemBottom
        reference.view.layoutIfNeeded()
        native.contentSize = scroll.contentSize
        native.contentOffset = scroll.contentOffset
        XCTAssertEqual(native.adjustedContentInset.bottom, originalInset, accuracy: 0.5)

        try press("select-grid", route: name)
        let bar = try node("selection-bar")
        waitFor("the native bar hides and select mode settles") { tabs.isTabBarHidden && !Agent(session: session).nativeInFlight() }
        XCTAssertEqual(box(bar).minX, inset ? 16 : 0, accuracy: 0.5, "inset-bar gives the row its side margins")
        XCTAssertEqual(nodeParent(bar) === (try node("app")), !wrapped, "wrap-bar puts the row in the panels' column")
        let obstruction = max(0, box(scroll).maxY - box(bar).minY)
        reference.additionalSafeAreaInsets.bottom = max(0, obstruction - systemBottom)
        reference.view.layoutIfNeeded()
        XCTAssertEqual(scroll.adjustedContentInset.bottom, obstruction, accuracy: 0.5,
                       "the authored row replaces the tab bar's obstruction, including the indicator only once")
        XCTAssertEqual(box(scroll), originalFrame, "the grid keeps its whole-view allocation")
        XCTAssertEqual(scroll.contentOffset.y - originalOffset, native.contentOffset.y - originalOffset, accuracy: 0.5,
                       "a grid at its end follows the same inset change as an ordinary UIKit scroller")
        XCTAssertLessThanOrEqual(box(try node("\(name)-30")).maxY, box(bar).minY + 0.5,
                                 "the final row is visible above the replacement bar")
        // In the panels' column too, the row is over the container that stands for the panels (LLP 1083.000 D4)
        let middle = bar.convert(CGPoint(x: bar.bounds.midX, y: bar.bounds.midY), to: nil)
        XCTAssertTrue(try XCTUnwrap(window).hitTest(middle, with: nil)?.isDescendant(of: bar) == true,
                      "the authored row is over the native container")
        let selectedOffset = scroll.contentOffset.y, nativeSelectedOffset = native.contentOffset.y

        try press("select-grid", route: name)
        waitFor("the row leaves the tree and its layer") { !session.presenter.navigationHost.siblings.holds(bar) }
        reference.additionalSafeAreaInsets.bottom = originalInset - systemBottom
        reference.view.layoutIfNeeded()
        waitFor("the native bar and its inset return") {
            !tabs.isTabBarHidden && !Agent(session: session).nativeInFlight()
                && abs(scroll.adjustedContentInset.bottom - originalInset) < 0.5
        }
        XCTAssertEqual(scroll.adjustedContentInset.bottom, originalInset, accuracy: 0.5)
        XCTAssertEqual(tabs.additionalSafeAreaInsets.bottom, 0, accuracy: 0.5)
        XCTAssertEqual(route.additionalSafeAreaInsets.bottom, 0, accuracy: 0.5)
        XCTAssertEqual(scroll.contentOffset.y - selectedOffset, native.contentOffset.y - nativeSelectedOffset, accuracy: 0.5,
                       "restoring the bar leaves scroll adjustment to UIKit too")
        scrollToEnd(scroll)
        XCTAssertLessThanOrEqual(box(try node("\(name)-30")).maxY, box(tabs.tabBar).minY + 0.5)
    }

    func testAnAuthoredTablistClearsTheGridAndRestoresTheNativeInset() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("setTabBarHidden is iOS 18") }
        let session = try fixture(wholeScreen: true), p = session.presenter
        try tap("tab-grid", route: "grid")
        let tabs = try XCTUnwrap(p.navigationHost.tabController)
        let scroll = try XCTUnwrap(try node("content-grid").scrollView)
        scrollToEnd(scroll)
        let originalInset = scroll.adjustedContentInset.bottom
        try press("author-tabs", route: "grid")
        let bar = try node("tabs")
        XCTAssertTrue(tabs.isTabBarHidden, "the press hides the native bar")
        // The inset takes the obstruction only after the press returns
        waitFor("the grid takes the tablist's obstruction", facts: {
            "inset \(scroll.adjustedContentInset.bottom), obstruction \(box(scroll).maxY - box(bar).minY)"
        }) { abs(scroll.adjustedContentInset.bottom - (box(scroll).maxY - box(bar).minY)) < 0.5 }
        let obstruction = box(scroll).maxY - box(bar).minY
        XCTAssertGreaterThan(obstruction, try XCTUnwrap(window).safeAreaInsets.bottom)
        scrollToEnd(scroll)
        XCTAssertLessThanOrEqual(box(try node("grid-30")).maxY, box(bar).minY + 0.5)
        XCTAssertGreaterThan(bar.layer.zPosition, tabs.view.layer.zPosition)
        try press("author-tabs", route: "grid")
        waitFor("the native tab bar restores its inset") {
            !tabs.isTabBarHidden && abs(scroll.adjustedContentInset.bottom - originalInset) < 0.5
        }
        XCTAssertEqual(tabs.additionalSafeAreaInsets.bottom, 0, accuracy: 0.5)
    }

    func testAReplacementBarSharesClearanceWithOuterInsetsAndTheKeyboard() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("setTabBarHidden is iOS 18") }
        guard hostedWindowScene() != nil else {
            throw missing("scene", "native search editing needs the hosted app responder lifecycle")
        }
        let session = try fixture(wholeScreen: true), p = session.presenter
        try tap("tab-grid", route: "grid")
        try press("select-grid", route: "grid")
        let tabs = try XCTUnwrap(p.navigationHost.tabController), route = try XCTUnwrap(p.navigationHost.activeRoute)
        let scroll = try XCTUnwrap(try node("content-grid").scrollView)
        let bar = try node("selection-bar"), host = try XCTUnwrap(window?.rootViewController)
        let indicator = host.view.safeAreaInsets.bottom
        let obstruction = box(scroll).maxY - box(bar).minY
        waitFor("the authored row sets the bottom inset") { abs(scroll.adjustedContentInset.bottom - obstruction) < 0.5 }
        let added = tabs.additionalSafeAreaInsets.bottom
        for _ in 0..<3 { p.navigationHost.reportCovers() }
        XCTAssertEqual(tabs.additionalSafeAreaInsets.bottom, added, accuracy: 0.5, "reporting does not accumulate the row's inset")

        host.additionalSafeAreaInsets.bottom = 20
        host.view.layoutIfNeeded()
        p.navigationHost.reportCovers()
        waitFor("an outer container's obstruction is counted once") {
            abs(scroll.adjustedContentInset.bottom - max(indicator + 20, obstruction)) < 0.5
        }
        XCTAssertEqual(tabs.additionalSafeAreaInsets.bottom, max(0, obstruction - indicator - 20), accuracy: 0.5)
        host.additionalSafeAreaInsets.bottom = 0
        host.view.layoutIfNeeded()
        p.navigationHost.reportCovers()
        waitFor("the row takes back only its own clearance") { abs(scroll.adjustedContentInset.bottom - obstruction) < 0.5 }

        tabs.additionalSafeAreaInsets.bottom += 20
        tabs.view.layoutIfNeeded()
        p.navigationHost.reportCovers()
        tabs.view.layoutIfNeeded()
        XCTAssertEqual(tabs.additionalSafeAreaInsets.bottom, 20, accuracy: 0.5, "an app's own inset survives the row's reconciliation")
        tabs.additionalSafeAreaInsets.bottom -= 20
        tabs.view.layoutIfNeeded()
        p.navigationHost.reportCovers()
        tabs.view.layoutIfNeeded()
        XCTAssertEqual(tabs.additionalSafeAreaInsets.bottom, added, accuracy: 0.5)

        let search = try XCTUnwrap(route.navigationItem.searchController)
        search.isActive = true
        let field = search.searchBar.searchTextField
        XCTAssertTrue(field.becomeFirstResponder())
        waitFor("the search field is presented") { search.isActive && !search.isBeingPresented && field.window != nil }
        let top = try XCTUnwrap(window).bounds.maxY - 300
        setKeyboard(p, top: top, responder: field)
        XCTAssertTrue(p.editsInBar(of: route.view))
        XCTAssertEqual(p.keyboardTop ?? 0, top, accuracy: 0.5)
        defer {
            setKeyboard(p, top: nil)
            field.resignFirstResponder()
            search.isActive = false
        }
        waitFor("the keyboard extends the inherited row's safe area") {
            abs(route.view.safeAreaInsets.bottom - 300) < 0.5 && abs(scroll.adjustedContentInset.bottom - 300) < 0.5
        }
        XCTAssertEqual(tabs.additionalSafeAreaInsets.bottom, added, accuracy: 0.5)
        setKeyboard(p, top: nil)
        field.resignFirstResponder()
        waitFor("dismissing the keyboard restores the row's inset") { abs(scroll.adjustedContentInset.bottom - obstruction) < 0.5 }
    }

    func testRouteWaitIncludesDelayedPressDelivery() throws {
        let session = try fixture(), presenter = session.presenter
        let press = presenter.onPress
        let delivered = expectation(description: "delayed press delivered")
        presenter.onPress = { id in
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.75) {
                press?(id)
                delivered.fulfill()
            }
        }
        defer { presenter.onPress = press }
        try tap("tab-grouped", route: "grouped")
        XCTAssertEqual((presenter.navigationHost.primaryNavigation?.topViewController as? RouteController)?.node.props["testId"], "route-grouped")
        XCTAssertFalse(Agent(session: session).nativeInFlight())
        wait(for: [delivered], timeout: 5)
    }

    func testOneAgentWheelReachesTheMeasuredEndOfAGroupedList() throws {
        let session = try fixture(), agent = Agent(session: session)
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        for name in ["grouped", "plainPushed"] {
            if name == "grouped" { try tap("tab-grouped", route: "grouped") }
            else { try tap("tab-plain", route: "plain"); try tap("plain-1", route: "plainPushed") }
            let content = try node("content-\(name)")
            let sv = try XCTUnwrap(session.presenter.groupedLists?.scroller(for: content.id) as? UICollectionView)
            let reply = agent.tap(["id": Int(content.id), "wheel": [0.0, 20000.0]])
            XCTAssertNil(reply["error"], "\(reply)")
            session.presenter.settlePump()
            sv.layoutIfNeeded()
            let inset = sv.adjustedContentInset
            let end = max(-inset.top, sv.contentSize.height + inset.bottom - sv.bounds.height)
            XCTAssertEqual(sv.contentOffset.y, end, accuracy: 0.5, "one wheel reaches the measured end")
            let last = IndexPath(item: sv.numberOfItems(inSection: sv.numberOfSections - 1) - 1, section: sv.numberOfSections - 1)
            let frame = try XCTUnwrap(sv.layoutAttributesForItem(at: last)).frame
            XCTAssertNotNil(sv.cellForItem(at: last), "the final row is realized")
            XCTAssertLessThanOrEqual(sv.convert(frame, to: nil).maxY, box(tabs.tabBar).minY + 0.5, "the final row clears the tab bar")
        }
    }

    /// The `scrolls` route holds a menu between its header and its scroller
    /// and a `dialog` after it. Closed, neither is laid out, and the scroller
    /// keeps both edges. A menu given back to layout to show in Exact's views
    /// (a context preview, the agent's painted popover) is laid out out of
    /// the flow, as HTML's top layer is, so the scroller keeps its place and
    /// its edges while it shows, and after it closes.
    func testAClosedMenuAndDialogBesideTheScrollerLeaveItItsEdges() throws {
        let session = try fixture()
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        try assertClear("scrolls", tabs)
        let content = try node("content-scrolls")
        let route = try XCTUnwrap(sequence(first: content.next, next: { $0?.next }).compactMap { $0 as? RouteController }.first)
        XCTAssertTrue(route.edgeScroll === content, "the scroller reaches the bottom edge")
        XCTAssertTrue(route.collapseScroll === content, "and goes under the bar")
        let menu = try node("sort-menu")
        XCTAssertFalse(RouteGeometry.inFlow(menu), "a closed menu is not laid out")
        XCTAssertFalse(RouteGeometry.inFlow(try node("confirm")), "nor is a closed dialog")
        let before = box(content)
        session.presenter.menus.layOut(menu)
        spin(0.2)
        XCTAssertGreaterThan(menu.bounds.height, 0, "the menu is laid out")
        // Out of the flow, as HTML's top layer is: opening it moves nothing.
        XCTAssertFalse(RouteGeometry.inFlow(menu), "an open popover is out of its parent's flow")
        XCTAssertEqual(box(content), before, "so the scroller beside it stays where it was")
        session.presenter.menus.coverClosed()
        spin(0.3)
        XCTAssertFalse(RouteGeometry.inFlow(menu))
        XCTAssertEqual(box(content), before)
        XCTAssertTrue(route.edgeScroll === content)
    }
}

/// A text field's delegate that counts the edits that end and runs `ended` for each.
private final class EditEnd: NSObject, UITextFieldDelegate {
    private let ended: () -> Void
    private(set) var count = 0
    init(_ ended: @escaping () -> Void) { self.ended = ended }
    func textFieldDidEndEditing(_ textField: UITextField) {
        count += 1
        ended()
    }
}

#endif
