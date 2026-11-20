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

    /// Rows above and below the panels while the native bars show: neither is beside the routes, so neither
    /// lifts over the container, and the tab bar keeps its touches (LLP 1083.000 D4).
    func testRowsAboveAndBelowThePanelsLeaveTheNativeBarsInPlace() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the screen's pixels need a window the system draws") }
        let session = try fixture(wholeScreen: true), navigation = session.presenter.navigationHost
        try tap("tab-grid", route: "grid")
        try press("show-rows", route: "grid")
        let top = try node("top-row"), mini = try node("mini-player"), win = try XCTUnwrap(window)
        waitFor("the rows are laid out") { top.window != nil && mini.window != nil && top.bounds.height > 0 && mini.bounds.height > 0 }
        navigation.reportCovers()
        XCTAssertFalse(navigation.siblings.holds(top), "the row above the panels stays in its column")
        XCTAssertFalse(navigation.siblings.holds(mini), "the row below them too")
        let bar = try XCTUnwrap(navigation.tabController).tabBar
        let point = bar.convert(CGPoint(x: bar.bounds.midX, y: bar.bounds.midY), to: win)
        XCTAssertTrue(win.hitTest(point, with: nil)?.isDescendant(of: bar) == true, "the tab bar keeps its touches")
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

    /// A lifted bar that leaves with an exit animation plays it where it is, over the container, while its column
    /// stacks; once it has gone, the next batch takes the layer that held it away (LLP 1083.000 D4).
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
        XCTAssertTrue(navigation.siblings.parent(of: bar) === column, "it plays its exit where it was lifted")
        XCTAssertGreaterThan(column.paintGhosts, 0, "its column stacks while it plays")
        waitFor("the exit ends", late: 3) { bar.superview == nil }
        try press("inset-bar", route: "grid")
        XCTAssertNil(navigation.siblings.layer, "the layer goes with the last node it held")
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
#endif
