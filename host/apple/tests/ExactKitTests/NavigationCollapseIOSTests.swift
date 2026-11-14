#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
import ExactGroupedLists

/// LLP 1075.003 Stage 3: a large title collapses with its route's content
/// scroll view (the fixture's Second tab, which names no scroller: LLP
/// 1115.000 D11.1). CSS `scrollTop` counts from the
/// scrollport's top, which under the bar is the bar's bottom whatever its
/// height — UIKit keeps the offset plus that inset fixed as the title
/// collapses — so an authored offset lands where the browser's does (0, 80,
/// the middle, the end of what shows). At rest, CSS 0, the title is
/// expanded; an authored 0 after a collapse lands at 0 under the bar as it
/// is, and the title expands when the user pulls.
/// Gestures (a finger's collapse, pull to top, a held sheet) need real
/// touches, which a unit test has none of.
final class NavigationCollapseIOSTests: XCTestCase {
    private var window: UIWindow?
    private var sessions: [ExactSession] = []

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    private func spin(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }

    /// The fixture booted in a window with its Second tab selected.
    private func second(_ label: String) throws -> (ExactSession, UINavigationController) {
        let env = ProcessInfo.processInfo.environment
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env["EXACT_FIXTURE_PLAN"])))
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: CGSize(width: 402, height: 874)).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        spin(0.3)
        let navigation = session.presenter.navigation
        let tabs = try XCTUnwrap(navigation.tabController)
        tapTab(tabs, 1)
        XCTAssertEqual(tabs.selectedIndex, 1, "Second selected")
        return (session, try XCTUnwrap(tabs.selectedViewController as? UINavigationController))
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    /// `scripts/fixtures/navigation-bars.contract` booted in a window, with
    /// the route `name` pushed over its first one.
    private func pushed(_ name: String) throws -> (ExactSession, UINavigationController, RouteController) {
        let session = try booted("EXACT_NAVIGATION_BARS_PLAN", label: "scroller-\(name)")
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        let reply = Agent(session: session).tap(["id": Int(try node(session, "start-open-\(name)").id)])
        XCTAssertNil(reply["error"], "the opener for \(name) is tapped")
        until("\(name) pushed") { nav.viewControllers.count == 2 && nav.transitionCoordinator == nil }
        spin(0.3)
        return (session, nav, try XCTUnwrap(nav.topViewController as? RouteController))
    }

    /// A fixture's plan (`scripts/fixtures`, by the name build.mjs gives its
    /// path) booted in a window.
    /// `beforeItsWindow`: the plan boots before its view is in a window, as
    /// an app's does at launch.
    private func booted(_ plan: String, label: String, beforeItsWindow: Bool = false) throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env[plan], "build.mjs --test --ios compiles the fixtures' plans")))
        ExactGroupedLists.install() // navigation-bars' `rows` route is a grouped list
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        self.window = window
        if beforeItsWindow {
            XCTAssertNil(session.boot(plan: plan, size: CGSize(width: 402, height: 874)).error)
            view.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
            view.layoutIfNeeded()
        }
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        if !beforeItsWindow { XCTAssertNil(session.boot(plan: plan, size: CGSize(width: 402, height: 874)).error) }
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        waitForActivation(session)
        waitFor("the navigation layout is ready") { !Agent(session: session).nativeInFlight() }
        return session
    }

    func testTopScrollerFollowsOnlyLaidOutNeutralWrappers() throws {
        let p = Presenter()
        let wrapper = NodeView(id: 1, kind: "view", presenter: p)
        let scroller = NodeView(id: 2, kind: "view", presenter: p)
        let sibling = NodeView(id: 3, kind: "view", presenter: p)
        wrapper.frame = CGRect(x: 0, y: 0, width: 400, height: 600)
        scroller.frame = CGRect(x: 0, y: 0, width: 400, height: 500)
        scroller.style["overflow_y"] = .string("scroll")
        scroller.syncScroll()
        wrapper.container.addSubview(scroller)
        XCTAssertTrue(NavigationHost.topScroller(wrapper, target: scroller) === scroller)
        XCTAssertTrue(NavigationHost.topScroller(scroller, target: scroller) === scroller)
        XCTAssertNil(NavigationHost.topScroller(wrapper, target: nil))

        sibling.frame = CGRect(x: 0, y: 500, width: 400, height: 100)
        wrapper.container.addSubview(sibling)
        XCTAssertTrue(NavigationHost.topScroller(wrapper, target: scroller) === scroller, "a sibling below leaves the scroller at the top")
        sibling.frame.origin.y = 0
        XCTAssertNil(NavigationHost.topScroller(wrapper, target: scroller), "content beside the scroller owns the top too")
        wrapper.container.insertSubview(sibling, at: 0)
        XCTAssertNil(NavigationHost.topScroller(wrapper, target: scroller), "the named scroller must be first in flow")
        sibling.isHidden = true
        XCTAssertTrue(NavigationHost.topScroller(wrapper, target: scroller) === scroller)
        sibling.removeFromSuperview()

        wrapper.style["padding_top"] = 16
        wrapper.applyContentBox(padding: [16, 0, 0, 0])
        XCTAssertNil(NavigationHost.topScroller(wrapper, target: scroller))
        wrapper.style["padding_top"] = nil
        wrapper.applyContentBox(padding: [0, 0, 0, 0])
        wrapper.style["overflow_y"] = .string("hidden")
        XCTAssertNil(NavigationHost.topScroller(wrapper, target: scroller), "overflow clipping is not a neutral wrapper")
        wrapper.style["overflow_y"] = nil
        wrapper.transform = CGAffineTransform(translationX: 10, y: 0)
        XCTAssertNil(NavigationHost.topScroller(wrapper, target: scroller))
        wrapper.transform = .identity
        wrapper.layer.transform = CATransform3DMakeRotation(0.2, 1, 0, 0)
        XCTAssertNil(NavigationHost.topScroller(wrapper, target: scroller))
        wrapper.layer.transform = CATransform3DIdentity
        wrapper.frame.size = .zero
        XCTAssertNil(NavigationHost.topScroller(wrapper, target: scroller), "unlaid wrappers cannot establish top alignment")
        wrapper.frame.size = CGSize(width: 400, height: 600)
        wrapper.style["overflow_y"] = .string("scroll")
        wrapper.syncScroll()
        XCTAssertNil(NavigationHost.topScroller(wrapper, target: scroller), "an intervening scroller owns its own scrolling")
    }

    func testWrappedScrollerLosesAndRegainsTheHeaderWhenContentMovesAboveIt() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the native bar follows its window's content scroller") }
        let (session, nav, route) = try pushed("wrapped")
        let p = session.presenter
        let scroller = try node(session, "wrapped-scroll")
        let scroll = try XCTUnwrap(scroller.scrollView)
        XCTAssertTrue(route.collapseScroll === scroller)
        XCTAssertTrue(route.contentScrollView(for: .top) === scroll)
        let observation = try XCTUnwrap(p.navigation.observation())
        XCTAssertEqual(observation["collapseScroller"] as? Int, Int(scroller.id))
        XCTAssertNotNil(observation["barHeight"] as? Double)
        let barBottom = nav.navigationBar.convert(nav.navigationBar.bounds, to: nil).maxY
        let top = try node(session, "wrapped-top")
        XCTAssertEqual(top.convert(top.bounds, to: nil).minY, barBottom, accuracy: 1)

        p.press(try node(session, "toggle-prefix").id)
        until("the prefix takes the top away from the scroller") { route.collapseScroll == nil }
        XCTAssertTrue(route.contentScrollView(for: .top) == nil)
        p.press(try node(session, "toggle-prefix").id)
        until("the neutral wrapper restores the header's scroller") { route.collapseScroll === scroller }
        XCTAssertTrue(route.contentScrollView(for: .top) === scroll)
    }

    private final class InsetWrites: UIScrollView {
        var writes = 0
        override var contentInsetAdjustmentBehavior: UIScrollView.ContentInsetAdjustmentBehavior {
            didSet { writes += 1 }
        }
    }

    func testUnchangedProjectionLeavesTheNativeInsetPolicyAlone() throws {
        let session = try booted("EXACT_LARGE_TITLE_PLAN", label: "inset-policy")
        let navigation = session.presenter.navigation
        let nav = try XCTUnwrap(navigation.primaryNavigation)
        let route = try XCTUnwrap(nav.topViewController as? RouteController)
        let home = try node(session, "home-scroll")
        let scroll = InsetWrites(frame: try XCTUnwrap(home.scrollView).frame)
        home.addSubview(scroll)
        home.more.nativeScrollView = scroll
        navigation.prepareRoutes([route], in: nav)
        XCTAssertTrue(route.contentScrollView(for: .top) === scroll)
        for transparent in [false, true, false] {
            route.node.props["navigationBar"] = transparent ? "transparent" : nil
            navigation.prepareRoutes([route], in: nav)
            XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, transparent ? .never : .automatic)
            scroll.writes = 0
            navigation.prepareRoutes([route], in: nav)
            navigation.prepareRoutes([route], in: nav)
            XCTAssertEqual(scroll.writes, 0, "unchanged projection leaves UIKit's scroll configuration alone")
        }
    }

    /// `navigationBar="transparent"` over `scripts/fixtures/transparent-bar.contract`:
    /// the bar shows, clear at the top of the route's scroller, whose content
    /// starts at the top edge under it (UIKit insets it nothing), as under a
    /// fixed transparent header on the web; the route is covered nothing
    /// at the top, so the hero picture bleeds to the edge.
    func testATransparentBarInsetsNothingOfItsScroller() throws {
        let session = try booted("EXACT_TRANSPARENT_BAR_PLAN", label: "transparent-bar", beforeItsWindow: true)
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        let route = try XCTUnwrap(nav.topViewController as? RouteController)
        XCTAssertFalse(nav.isNavigationBarHidden, "the bar shows")
        XCTAssertEqual(route.navigationItem.scrollEdgeAppearance, NavigationHost.clearScrollEdge, "clear at the scroller's top")
        let home = try node(session, "home-scroll"), sv = try XCTUnwrap(home.scroll)
        XCTAssertTrue(route.collapseScroll === home, "the bar follows the route's scroller")
        XCTAssertEqual(sv.contentInsetAdjustmentBehavior, .never)
        XCTAssertEqual(sv.adjustedContentInset.top, 0, "UIKit insets it nothing")
        let hero = try node(session, "hero")
        XCTAssertEqual(hero.convert(hero.bounds, to: nil).minY, 0, accuracy: 0.5, "the picture starts at the top edge, under the bar")
        XCTAssertEqual(home.scrollOrigin, 0, "CSS scrollTop 0 is the top edge")
    }

    /// LLP 1115.000 D11.1 over `scripts/fixtures/large-title.contract`: one
    /// stack with no tabs, the shape `exact new` makes, whose first screen
    /// names no scroller. Its large title opens expanded, its content below
    /// it, and collapses as the content scrolls. UIKit shows the title
    /// collapsed when the stack is laid out in the window before it has its
    /// root (`installPrimary`).
    func testASingleStacksFirstScreenOpensWithItsLargeTitleExpanded() throws {
        let session = try booted("EXACT_LARGE_TITLE_PLAN", label: "large-title", beforeItsWindow: true)
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        let route = try XCTUnwrap(nav.topViewController as? RouteController)
        let home = try node(session, "home-scroll"), sv = try XCTUnwrap(home.scroll)
        XCTAssertTrue(route.collapseScroll === home, "the scroller it does not name is the bar's")
        let top = try node(session, "home-top"), bar = nav.navigationBar
        let expanded = bar.frame.height
        XCTAssertGreaterThan(expanded, 90, "the large title is expanded as the screen opens")
        XCTAssertEqual(sv.contentOffset.y + sv.adjustedContentInset.top, 0, accuracy: 0.5, "its content at its top")
        XCTAssertEqual(top.convert(top.bounds, to: nil).minY, bar.convert(bar.bounds, to: nil).maxY, accuracy: 0.5, "just below the expanded title")
        XCTAssertGreaterThan(home.scrollOrigin, 0, "the expanded inset is CSS 0")
        home.pendingScrollTop = 300
        home.applyPendingScroll()
        spin(0.3)
        XCTAssertLessThan(bar.frame.height, expanded, "and the title collapses as the content scrolls")
    }

    /// The agent's `tap <scroller> drag 0 -900` to the end under an expanded
    /// large title rests at the end, as a finger's drag does once its bounce
    /// settles: UIKit adds the collapsing title's height to the offset, which
    /// left the content that far past its end (euxy's Settings, 52 points).
    func testAnAgentDragToTheEndUnderALargeTitleRestsAtTheEnd() throws {
        let session = try booted("EXACT_LARGE_TITLE_PLAN", label: "large-title-drag", beforeItsWindow: true)
        let home = try node(session, "home-scroll"), sv = try XCTUnwrap(home.scroll)
        let bar = try XCTUnwrap(session.presenter.navigation.primaryNavigation).navigationBar
        let expanded = bar.frame.height
        let reply = Agent(session: session).tap(["id": Int(home.id), "scrollDrag": [0.0, -100_000.0], "ms": 100.0])
        XCTAssertNil(reply["error"], "\(reply)")
        spin(0.3)
        XCTAssertLessThan(bar.frame.height, expanded, "the title collapsed")
        let end = sv.contentSize.height + sv.adjustedContentInset.bottom - sv.bounds.height
        XCTAssertEqual(sv.contentOffset.y, end, accuracy: 0.5, "at the end, not past it")
    }

    /// The agent's wheel under a large title leaves the scroller and the bar
    /// where a person's scroll by as much leaves a hand-built list's (measured
    /// on iOS 27): the content moved by the wheel's distance exactly while the
    /// title collapses, a wheel back to the top opening the title again, and
    /// none resting with the content partway under the title, which eases to
    /// the nearer of its open and closed rests.
    func testTheWheelUnderALargeTitleRestsAsAPersonsScrollDoes() throws {
        let session = try booted("EXACT_LARGE_TITLE_PLAN", label: "large-title-wheel", beforeItsWindow: true)
        let home = try node(session, "home-scroll"), sv = try XCTUnwrap(home.scroll)
        let top = try node(session, "home-top"), bar = try XCTUnwrap(session.presenter.navigation.primaryNavigation).navigationBar
        let expanded = bar.frame.height, open = sv.contentOffset.y
        let onScreen = { top.convert(top.bounds, to: nil).minY }, opened = onScreen()
        Agent.scroll(from: sv, dx: 0, dy: 300)
        XCTAssertLessThan(bar.frame.height, expanded, "the title collapsed")
        XCTAssertEqual(onScreen(), opened - 300, accuracy: 0.5, "the content moved by the wheel's 300, not the title's height besides")
        let closed = -sv.adjustedContentInset.top, band = closed - open
        XCTAssertGreaterThan(band, 40, "the title's height")
        Agent.scroll(from: sv, dx: 0, dy: -1_000)
        XCTAssertEqual(bar.frame.height, expanded, accuracy: 0.5, "the title opened at the top")
        XCTAssertEqual(sv.contentOffset.y, open, accuracy: 0.5, "at the top, not past it")
        XCTAssertEqual(onScreen(), opened, accuracy: 0.5, "the content just below the title")
        // A third of the title down eases back open; two thirds, closed
        Agent.scroll(from: sv, dx: 0, dy: (band / 3).rounded())
        XCTAssertEqual(sv.contentOffset.y, open, accuracy: 0.5, "a third of the way in eases back open")
        XCTAssertEqual(bar.frame.height, expanded, accuracy: 0.5)
        Agent.scroll(from: sv, dx: 0, dy: (band * 2 / 3).rounded())
        XCTAssertEqual(sv.contentOffset.y, closed, accuracy: 0.5, "two thirds of the way in eases closed")
        XCTAssertLessThan(bar.frame.height, expanded)
        // From the closed title's top, the same up
        Agent.scroll(from: sv, dx: 0, dy: -(band / 3).rounded())
        XCTAssertEqual(sv.contentOffset.y, closed, accuracy: 0.5, "a third of a pull eases back closed")
        Agent.scroll(from: sv, dx: 0, dy: -(band * 2 / 3).rounded())
        XCTAssertEqual(sv.contentOffset.y, open, accuracy: 0.5, "two thirds of one opens it")
        XCTAssertEqual(bar.frame.height, expanded, accuracy: 0.5)
    }

    func testAnAuthoredVerticalSnapWinsOverTheLargeTitleRest() throws {
        let session = try booted("EXACT_LARGE_TITLE_PLAN", label: "large-title-snap", beforeItsWindow: true)
        let home = try node(session, "home-scroll"), sv = try XCTUnwrap(home.scroll)
        let top = try node(session, "home-top")
        let open = sv.contentOffset.y
        home.style["scroll_snap_type"] = .string("y mandatory")
        top.style["scroll_snap_align"] = .string("start")
        top.frame.origin.y = 30
        let want = CGPoint(x: 0, y: open + 30)
        var finger = want
        home.scrollViewWillEndDragging(sv, withVelocity: .zero, targetContentOffset: &finger)
        XCTAssertEqual(finger.y, want.y, accuracy: 0.5)
        Agent.scroll(from: sv, dx: 0, dy: 30)
        XCTAssertEqual(sv.contentOffset.y, finger.y, accuracy: 0.5, "the agent keeps the authored rest inside the title band")
        top.style["scroll_snap_align"] = nil
        Agent.scroll(from: sv, dx: 0, dy: -1_000)
        Agent.scroll(from: sv, dx: 0, dy: 30)
        XCTAssertGreaterThan(sv.contentOffset.y, finger.y + 1, "without an area the title still settles closed")
    }

    /// The agent's drag past the top under a collapsed large title opens it,
    /// as a finger's pull does, and rests at the top once the bounce settles.
    func testAnAgentDragPastTheTopOpensACollapsedLargeTitle() throws {
        let session = try booted("EXACT_LARGE_TITLE_PLAN", label: "large-title-pull", beforeItsWindow: true)
        let home = try node(session, "home-scroll"), sv = try XCTUnwrap(home.scroll)
        let bar = try XCTUnwrap(session.presenter.navigation.primaryNavigation).navigationBar
        let expanded = bar.frame.height, start = sv.contentOffset.y
        let agent = Agent(session: session)
        XCTAssertNil(agent.tap(["id": Int(home.id), "scrollDrag": [0.0, -300.0], "ms": 100.0])["error"])
        XCTAssertLessThan(bar.frame.height, expanded, "the title collapsed")
        let pull = agent.tap(["id": Int(home.id), "scrollDrag": [0.0, 1_000.0], "ms": 100.0])
        XCTAssertNil(pull["error"], "\(pull)")
        XCTAssertEqual(bar.frame.height, expanded, accuracy: 0.5, "the pull opened the title")
        XCTAssertEqual(sv.contentOffset.y, start, accuracy: 0.5, "at the top")
        XCTAssertEqual(try XCTUnwrap(pull["scrolledBy"] as? [Double])[1], -300, accuracy: 0.5, "the content moved back by the 300 it went")
    }

    /// An appearance, tint or text-size change that comes with a new size (an
    /// iPhone Duo's snapshot of the app unfolded) fits the native container
    /// before the change's batches: every batch lays the
    /// navigation out, and UIKit must not lay it out in the old size's frames
    /// (ExactViewIOS's `fitAheadOfLayout`).
    @MainActor
    func testATraitOrTintChangeWithANewSizeAppliesTheSizeFirst() throws {
        let session = try booted("EXACT_LARGE_TITLE_PLAN", label: "appearance-resize")
        let view = try XCTUnwrap(session.view)
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        XCTAssertEqual(nav.view.bounds.width, 402, accuracy: 0.5)
        // Fix the window's tint so the appearance change reaches the view
        // through its trait callback alone: the system tint changes with the
        // appearance, and its callback comes first.
        let tint = UIColor(red: 0.8, green: 0.1, blue: 0.1, alpha: 1)
        window?.tintColor = tint
        window?.layoutIfNeeded()
        func resize(to width: CGFloat, with change: String, _ make: () -> Void) {
            XCTAssertFalse(session.ownerInFlight, "no owner work lands ahead of the \(change) change's first batch")
            let before = session.appliedBatches
            var resizedIn: Int?
            let observation = nav.view.layer.observe(\.bounds, options: [.new]) { _, bounds in
                if resizedIn == nil, let new = bounds.newValue?.width, abs(new - width) < 0.5 { resizedIn = session.appliedBatches - before }
            }
            defer { observation.invalidate() }
            view.frame = CGRect(x: 0, y: 0, width: width, height: 700)
            make()
            XCTAssertEqual(resizedIn, 0, "navigation takes the new size before the \(change) change's batches")
            XCTAssertGreaterThan(session.appliedBatches - before, 1, "the \(change) change applied a batch after the size")
        }
        resize(to: 600, with: "appearance") {
            window?.overrideUserInterfaceStyle = view.traitCollection.userInterfaceStyle == .dark ? .light : .dark
            window?.updateTraitsIfNeeded()
        }
        XCTAssertEqual(window?.tintColor, tint, "the tint the appearance change left alone")
        resize(to: 500, with: "tint") { window?.tintColor = UIColor(red: 0.1, green: 0.5, blue: 0.2, alpha: 1) }
        resize(to: 450, with: "text size") {
            window?.traitOverrides.preferredContentSizeCategory = .accessibilityLarge
            window?.updateTraitsIfNeeded()
        }
    }

    @MainActor
    func testEveryBatchAfterAResizeUsesTheNewNavigationSize() throws {
        let session = try booted("EXACT_LARGE_TITLE_PLAN", label: "batch-resize")
        let view = try XCTUnwrap(session.view)
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        let feed = try node(session, "home-scroll"), scroll = try XCTUnwrap(feed.scroll)
        feed.handlers.insert("refresh")
        feed.props["refreshing"] = "true"
        feed.updateRefresh()
        let control = try XCTUnwrap(scroll.refreshControl)
        control.beginRefreshing()
        scroll.setContentOffset(CGPoint(x: 0, y: -scroll.adjustedContentInset.top - 60), animated: false)
        XCTAssertTrue(control.isRefreshing)
        let refreshInset = scroll.adjustedContentInset.top - scroll.safeAreaInsets.top
        defer { control.endRefreshing() }
        func resize(_ width: CGFloat, _ source: String, bounds: Bool = false, _ batch: () -> Void) {
            let before = session.appliedBatches
            var resizedIn: Int?
            let observation = nav.view.layer.observe(\.bounds, options: [.new]) { _, change in
                if resizedIn == nil, change.newValue?.width == width { resizedIn = session.appliedBatches - before }
            }
            defer { observation.invalidate(); session.onApplyForTests = nil }
            session.onApplyForTests = { _ in
                XCTAssertEqual(nav.view.bounds.width, width, accuracy: 0.5, source)
            }
            if bounds { view.bounds.size = CGSize(width: width, height: 700) }
            else { view.frame.size = CGSize(width: width, height: 700) }
            batch()
            XCTAssertEqual(resizedIn, 0, "\(source): the size precedes every batch")
            XCTAssertEqual(nav.view.bounds.width, width, accuracy: 0.5, source)
            XCTAssertEqual(scroll.adjustedContentInset.top - scroll.safeAreaInsets.top, refreshInset, accuracy: 0.5, source)
            XCTAssertTrue(control.isRefreshing, source)
        }
        session.publishTickForTests(wireBatch([
            ["op": "props", "id": Int(feed.id), "set": ["title": "published before resize"]],
        ]))
        resize(600, "a published batch before activation") { session.tellPage() }
        resize(500, "scene") {
            NotificationCenter.default.post(name: UIScene.didActivateNotification, object: window?.windowScene)
        }
        var queued = false
        DispatchQueue.main.async {
            resize(450, "main queue") { session.apply(session.runtime.tick(now: session.now())) }
            queued = true
        }
        waitFor("the queued batch") { queued }
        var fired = false
        let timer = Timer(timeInterval: 0, repeats: false) { _ in
            resize(550, "timer") { session.apply(session.runtime.advance(now: session.now())) }
            fired = true
        }
        RunLoop.main.add(timer, forMode: .default)
        defer { timer.invalidate() }
        waitFor("the timer batch") { fired }
        resize(480, "bounds", bounds: true) { session.tellPage() }
        resize(650, "display scale") {
            window?.traitOverrides.displayScale = 2
            window?.updateTraitsIfNeeded()
        }
    }

    @MainActor
    func testAResizeAtAnAppearanceFlipDoesNotGiveInheritedTextItsOwnAppearance() throws {
        let session = try booted("EXACT_LARGE_TITLE_PLAN", label: "resize-color")
        let view = try XCTUnwrap(session.view)
        let text = try node(session, "home-top")
        window?.tintColor = .red
        window?.overrideUserInterfaceStyle = .light
        window?.updateTraitsIfNeeded()
        for dark in [true, false] {
            view.frame.size = CGSize(width: dark ? 600 : 450, height: 700)
            window?.overrideUserInterfaceStyle = dark ? .dark : .light
            window?.updateTraitsIfNeeded()
            XCTAssertEqual(session.schemeDark, dark)
            XCTAssertTrue(session.viewDark.isEmpty, "inherited colours must resolve by the session: \(session.viewDark)")
            // Keep the child's inherited traits at the old appearance until
            // after the session reports, as during UIKit's trait walk. A
            // traitOverrides.userInterfaceStyle would give it a native override.
            session.scheme(dark: !dark)
            let parent = try XCTUnwrap(text.superview)
            let index = try XCTUnwrap(parent.subviews.firstIndex(of: text))
            let staging = UIWindow(frame: view.bounds)
            staging.overrideUserInterfaceStyle = dark ? .light : .dark
            staging.addSubview(text)
            text.updateTraitsIfNeeded()
            XCTAssertEqual(text.overrideUserInterfaceStyle, .unspecified, "the child's old traits are inherited")
            view.bounds.size.width = dark ? 450 : 600
            XCTAssertEqual(text.drawsDark, !dark)
            XCTAssertTrue(session.unreported.contains(text.id), "the child's trait callback deferred its report")
            XCTAssertTrue(session.viewDark.isEmpty, "the session's pending trait report is not a subtree override")
            session.scheme(dark: dark)
            XCTAssertNil(session.viewDark[text.id], "the session reported before the child's traits changed")
            XCTAssertTrue(session.unreported.contains(text.id), "the child's own callback must finish its report")
            parent.insertSubview(text, at: index)
            text.updateTraitsIfNeeded()
            XCTAssertFalse(session.unreported.contains(text.id), "the child's trait callback completed its report")
            view.setNeedsLayout()
            view.layoutIfNeeded()
            XCTAssertTrue(session.viewDark.isEmpty)
        }
    }

    @MainActor
    func testResizingAndRotationKeepTheSettledSafeAreaWhileRefreshing() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "rotation of a refreshing navigation container")
        }
        let session = try booted("EXACT_COVER_SCROLL_PLAN", label: "resize-safe-area")
        let view = try XCTUnwrap(session.view), window = try XCTUnwrap(window)
        let host = try XCTUnwrap(window.rootViewController)
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        window.frame = scene.coordinateSpace.bounds
        view.frame = host.view.bounds
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        window.layoutIfNeeded()
        let feed = try node(session, "home-scroll"), scroll = try XCTUnwrap(feed.scroll)
        feed.handlers.insert("refresh")
        feed.props["refreshing"] = "true"
        feed.updateRefresh()
        let control = try XCTUnwrap(scroll.refreshControl)
        control.beginRefreshing()
        scroll.setContentOffset(CGPoint(x: 0, y: -scroll.adjustedContentInset.top - 60), animated: false)
        let wasLandscape = scene.interfaceOrientation.isLandscape
        defer {
            scene.requestGeometryUpdate(.iOS(interfaceOrientations: wasLandscape ? .landscapeLeft : .portrait))
        }
        for landscape in [true, false] {
            scene.requestGeometryUpdate(.iOS(interfaceOrientations: landscape ? .landscapeLeft : .portrait))
            waitFor("the scene rotates") { scene.interfaceOrientation.isLandscape == landscape }
            window.frame = scene.coordinateSpace.bounds
            for fraction: CGFloat in [0.75, 1] {
                host.additionalSafeAreaInsets = UIEdgeInsets(top: 7, left: 11, bottom: 13, right: 17)
                view.frame = CGRect(x: 0, y: 0, width: host.view.bounds.width * fraction, height: host.view.bounds.height)
                session.tellPage()
                window.layoutIfNeeded()
                view.setNeedsLayout()
                view.layoutIfNeeded()
                waitFor("the native layout settles") { !Agent(session: session).nativeInFlight() }
                XCTAssertEqual(session.presenter.insets, view.safeAreaInsets, "the layout rereads UIKit's settled safe area")
                XCTAssertEqual(session.presenter.viewport.bounds.size, view.bounds.size)
                XCTAssertEqual(nav.view.bounds.size, view.bounds.size)
                XCTAssertTrue(control.isRefreshing)
                window.overrideUserInterfaceStyle = landscape ? .dark : .light
                window.updateTraitsIfNeeded()
                XCTAssertTrue(session.viewDark.isEmpty)
            }
        }
        control.endRefreshing()
        waitFor("the refresh inset ends") { abs(scroll.contentOffset.y + scroll.adjustedContentInset.top) < 0.5 }
        XCTAssertEqual(scroll.contentOffset.y + scroll.adjustedContentInset.top, 0, accuracy: 0.5)
    }

    /// LLP 1054 A4: the agent's drag pulls to refresh as a finger does. A
    /// real touch's 200 points past the top refreshed, 180 did not (iOS 27).
    func testTheAgentsDragPullsToRefreshAsAFingerDoes() throws {
        let session = try booted("EXACT_PULL_REFRESH_PLAN", label: "pull-refresh")
        let agent = Agent(session: session)
        let feed = try node(session, "feed"), control = try XCTUnwrap(feed.scroll?.refreshControl)
        let short = agent.tap(["id": Int(feed.id), "scrollDrag": [0.0, 180.0], "ms": 100.0])
        XCTAssertNil(short["error"], "\(short)")
        XCTAssertEqual(short["refreshed"] as? Bool, false, "short of the pull")
        XCTAssertFalse(control.isRefreshing)
        let pull = agent.tap(["id": Int(feed.id), "scrollDrag": [0.0, 200.0], "ms": 100.0])
        XCTAssertNil(pull["error"], "\(pull)")
        XCTAssertEqual(pull["refreshed"] as? Bool, true)
        XCTAssertTrue(control.isRefreshing, "the spinner shows until the app's `refreshing` says otherwise")
        // Target-action goes through UIApplication, which a bare test run has
        // none of: the control's own actions, sent as UIKit would.
        for target in control.allTargets {
            for action in control.actions(forTarget: target, forControlEvent: .valueChanged) ?? [] {
                _ = (target.base as AnyObject).perform(NSSelectorFromString(action), with: control)
            }
        }
        until("the refresh ran the app's action") { (try? self.node(session, "pulls"))?.props["text"] == "1" }
    }

    func testAnAgentScrollDragMovesThroughoutItsDurationUnderLiveCapture() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else {
            throw missing("scene", "live capture of a timed scroll drag")
        }
        let session = try booted("EXACT_PULL_REFRESH_PLAN", label: "timed-scroll")
        session.clock = nil
        let agent = Agent(session: session)
        let feed = try node(session, "feed"), scroll = try XCTUnwrap(feed.scroll)
        let health = session.frameHealth
        XCTAssertEqual(health.setAgentCapture("start")["capture"] as? Bool, true)
        health.beginScroll(scroll, session: session)
        let native = try XCTUnwrap(health.nativeHealth)
        defer { health.stopScroll(); _ = health.setAgentCapture("stop") }
        var offsets: [(time: Double, y: CGFloat)] = []
        var firstFrame: UInt64?
        let began = CACurrentMediaTime(), origin = scroll.contentOffset.y
        let reads = Timer(timeInterval: 0.05, repeats: true) { _ in
            offsets.append((CACurrentMediaTime() - began, scroll.contentOffset.y - origin))
            let capture = native.snapshot()
            if firstFrame == nil, capture.callbacks > 0 { firstFrame = capture.last_arrival_ns }
        }
        RunLoop.main.add(reads, forMode: .common)
        defer { reads.invalidate() }
        let reply = agent.tap(["id": Int(feed.id), "scrollDrag": [0.0, -300.0], "ms": 500.0])
        let elapsed = CACurrentMediaTime() - began
        XCTAssertNil(reply["error"], "\(reply)")
        XCTAssertGreaterThanOrEqual(elapsed, 0.5)
        XCTAssertEqual(scroll.contentOffset.y - origin, 300, accuracy: 0.5)
        XCTAssertEqual((reply["scrolledBy"] as? [Double])?[1] ?? -1, 300, accuracy: 0.5)
        for band in [0.05..<0.2, 0.2..<0.35, 0.35..<0.5] {
            XCTAssertTrue(offsets.contains { band.contains($0.time) && $0.y > 0 && $0.y < 300 }, "no intermediate offset in \(band): \(offsets)")
        }
        XCTAssertTrue(zip(offsets, offsets.dropFirst()).allSatisfy { $0.y <= $1.y }, "the drag follows the finger")
        let capture = native.snapshot()
        XCTAssertGreaterThan(capture.callbacks, 5, "live capture observes the drag's frames")
        // The passive observer has no cadence; its arrivals still span the gesture.
        XCTAssertGreaterThan(capture.last_arrival_ns - (try XCTUnwrap(firstFrame)), 350_000_000)
    }

    func testATimedScrollDragStopsWhenItsScrollerIsDisabled() throws {
        let session = try booted("EXACT_PULL_REFRESH_PLAN", label: "cancelled-scroll")
        let agent = Agent(session: session)
        let feed = try node(session, "feed"), scroll = try XCTUnwrap(feed.scroll)
        let origin = scroll.contentOffset.y
        var stopped: CGFloat?
        let observation = scroll.observe(\.contentOffset, options: [.new]) { _, _ in
            if stopped == nil, scroll.contentOffset.y - origin >= 60 {
                stopped = scroll.contentOffset.y
                scroll.isScrollEnabled = false
            }
        }
        defer { observation.invalidate() }
        let reply = agent.tap(["id": Int(feed.id), "scrollDrag": [0.0, -300.0], "ms": 500.0])
        XCTAssertNil(reply["error"], "\(reply)")
        XCTAssertEqual(reply["cancelled"] as? Bool, true)
        XCTAssertEqual(scroll.contentOffset.y, try XCTUnwrap(stopped), accuracy: 0.5)
        XCTAssertLessThan(scroll.contentOffset.y - origin, 300)
        observation.invalidate()
        scroll.isScrollEnabled = true
        scroll.setContentOffset(CGPoint(x: 0, y: 100), animated: false)
        let pullObservation = scroll.observe(\.contentOffset, options: [.new]) { _, _ in
            if scroll.contentOffset.y <= 0 { scroll.isScrollEnabled = false }
        }
        defer { pullObservation.invalidate() }
        let pull = agent.tap(["id": Int(feed.id), "scrollDrag": [0.0, 400.0], "ms": 500.0])
        XCTAssertNil(pull["error"], "\(pull)")
        XCTAssertEqual(pull["cancelled"] as? Bool, true)
        XCTAssertEqual(pull["refreshed"] as? Bool, false, "a cancelled pull cannot refresh")
        XCTAssertEqual(reply["refreshed"] as? Bool, false)
        XCTAssertFalse(try XCTUnwrap(scroll.refreshControl).isRefreshing)
        scroll.isScrollEnabled = true
        for duration in [-1.0, 2001.0, .infinity, .nan] {
            XCTAssertNotNil(agent.tap(["id": Int(feed.id), "scrollDrag": [0.0, -300.0], "ms": duration])["error"])
        }
    }

    func testATimedScrollDragHoldsScrollEndAndTimersUntilLiftOrCancellation() throws {
        for mode in ["bound", "stationary", "cancelled"] {
            let session = try booted("EXACT_PULL_REFRESH_PLAN", label: "scroll-end-\(mode)")
            session.clock = nil
            let p = session.presenter, agent = Agent(session: session)
            let feed = try node(session, "feed"), scroll = try XCTUnwrap(feed.scroll)
            let end = scroll.contentSize.height + scroll.adjustedContentInset.bottom - scroll.bounds.height
            scroll.setContentOffset(CGPoint(x: 0, y: mode == "stationary" ? end : end - 100), animated: false)
            feed.endScroll()
            feed.handlers.formUnion(["scroll", "scrollend"])
            var events: [String] = [], ended: [Double] = [], admitted: [Double] = []
            let began = CACurrentMediaTime()
            p.onScroll = { _, _ in events.append("scroll") }
            p.onScrollend = { _ in
                events.append("end")
                ended.append(CACurrentMediaTime() - began)
                scroll.isScrollEnabled = false
            }
            var cancelledAt: Double?
            let observation = scroll.observe(\.contentOffset, options: [.new]) { _, _ in
                if mode == "cancelled", cancelledAt == nil, scroll.contentOffset.y >= end - 50 {
                    cancelledAt = CACurrentMediaTime() - began
                    scroll.isScrollEnabled = false
                }
            }
            let reads = Timer(timeInterval: 0.025, repeats: true) { _ in
                if (try? self.node(session, "rests"))?.props["text"] == "1" {
                    admitted.append(CACurrentMediaTime() - began)
                }
            }
            RunLoop.main.add(reads, forMode: .common)
            defer { observation.invalidate(); reads.invalidate() }
            p.press(try node(session, "wait-for-rest").id)
            let reply = agent.tap(["id": Int(feed.id), "scrollDrag": [0.0, -1000.0], "ms": 500.0])
            let elapsed = CACurrentMediaTime() - began
            XCTAssertNil(reply["error"], "\(mode): \(reply)")
            XCTAssertEqual(reply["cancelled"] as? Bool ?? false, mode == "cancelled", "\(mode): \(reply)")
            if mode == "cancelled" {
                XCTAssertGreaterThanOrEqual(try XCTUnwrap(ended.first), try XCTUnwrap(cancelledAt))
                XCTAssertLessThan(scroll.contentOffset.y, end)
            } else {
                XCTAssertGreaterThanOrEqual(elapsed, 0.5)
                XCTAssertEqual(scroll.contentOffset.y, end, accuracy: 0.5)
                XCTAssertTrue(ended.allSatisfy { $0 >= 0.5 }, "\(mode): ended during the drag: \(ended)")
                XCTAssertTrue(admitted.allSatisfy { $0 >= 0.5 }, "\(mode): scrollend timer ran during the drag: \(admitted)")
            }
            XCTAssertEqual(ended.count, mode == "stationary" ? 0 : 1)
            if !ended.isEmpty { XCTAssertEqual(events.suffix(2), ["scroll", "end"]) }
            until("\(mode): the scrollend timer runs after the gesture") { (try? self.node(session, "rests"))?.props["text"] == "1" }
            XCTAssertFalse(feed.scrollEnd?.live == true, "\(mode): the gesture released its hold")
            observation.invalidate()
            reads.invalidate()
            session.destroy()
            sessions.removeLast()
            window?.isHidden = true
        }
    }

    /// A diagonal agent drag moves a scroller along the axes it scrolls
    /// alone, though its content overflows the other (the page is 800 wide,
    /// the strip's row 100 tall), and reports the drag's own distance while
    /// the collapsing title adds its height to the offset.
    func testADiagonalAgentDragMovesAScrollerAlongItsOwnAxisAlone() throws {
        let session = try booted("EXACT_LARGE_TITLE_PLAN", label: "large-title-diagonal", beforeItsWindow: true)
        let agent = Agent(session: session)
        let page = try node(session, "home-scroll"), sv = try XCTUnwrap(page.scroll)
        let strip = try node(session, "home-strip"), row = try XCTUnwrap(strip.scroll)
        XCTAssertEqual(page.scroll?.scrollsX, false)
        XCTAssertEqual(strip.scroll?.scrollsY, false)
        let across = agent.tap(["id": Int(strip.id), "scrollDrag": [-240.0, -30.0], "ms": 100.0])
        XCTAssertNil(across["error"], "\(across)")
        XCTAssertEqual(row.contentOffset.x, 240, accuracy: 0.5, "the strip scrolls sideways")
        XCTAssertEqual(row.contentOffset.y, 0, accuracy: 0.5, "and not down")
        let x = sv.contentOffset.x, y = sv.contentOffset.y
        let down = agent.tap(["id": Int(page.id), "scrollDrag": [-40.0, -300.0], "ms": 100.0])
        XCTAssertNil(down["error"], "\(down)")
        XCTAssertEqual(sv.contentOffset.x, x, accuracy: 0.5, "the page scrolls down and not sideways")
        XCTAssertGreaterThan(sv.contentOffset.y, y + 299)
        let by = try XCTUnwrap(down["scrolledBy"] as? [Double])
        XCTAssertEqual(by[0], 0, accuracy: 0.5)
        XCTAssertEqual(by[1], 300, accuracy: 0.5, "the drag's 300, not the title's height besides")
    }

    /// LLP 1115.000 D11.1: a route that names no scroller has one all the
    /// same, its first that scrolls vertically, as a hand-built screen's
    /// scroll view is its bar's with nothing written. Right after the header
    /// the bar replaces, it goes under the bar, which insets it, and the
    /// status bar's tap scrolls that same one. (A large title's collapse
    /// with nothing written is the fixture's Second tab, above.)
    func testARoutesScrollerIsTheBarsWithNothingWritten() throws {
        let (session, nav, route) = try pushed("tall")
        let tall = try node(session, "tall-scroll")
        let sv = try XCTUnwrap(tall.scroll)
        XCTAssertNil(route.node.props["navigationScroll"].flatMap { $0.isEmpty ? nil : $0 }, "the route names nothing")
        XCTAssertTrue(route.collapseScroll === tall, "the scroller after the header is the bar's")
        XCTAssertTrue(route.contentScrollView(for: .top) === sv)
        XCTAssertEqual(sv.contentInsetAdjustmentBehavior, .automatic, "UIKit insets it under the bar")
        XCTAssertTrue(session.agent(#"{"op":"logs","since":0}"#).contains("collapses its title with its scroller"))
        XCTAssertEqual(tall.convert(tall.bounds, to: nil).minY, 0, accuracy: 0.5, "the scroller itself reaches the screen's top, under the bar")
        let top = try node(session, "tall-top"), bar = nav.navigationBar
        XCTAssertEqual(top.convert(top.bounds, to: nil).minY, bar.convert(bar.bounds, to: nil).maxY, accuracy: 0.5, "its first line rests just below the bar")
        XCTAssertEqual(sv.contentOffset.y + sv.adjustedContentInset.top, 0, accuracy: 0.5, "at its top (CSS 0)")
        tall.pendingScrollTop = 300
        tall.applyPendingScroll()
        spin(0.3)
        XCTAssertLessThan(top.convert(top.bounds, to: nil).maxY, bar.convert(bar.bounds, to: nil).maxY, "scrolled, its content passes under the bar")
        XCTAssertTrue(session.presenter.scrollsToTopTarget() === sv, "the status bar's tap scrolls the same one")
        XCTAssertTrue(sv.scrollsToTop)
    }

    /// Under a `viewport-fit="cover"` root a route's scroller spans the
    /// window, side insets included. UIKit insets it at its top and bottom
    /// only, so its content, as wide as it, has no sideways range (LLP 1123
    /// D3); `.always` added the side inset too, and it scrolled sideways by it.
    func testUnderACoverRootTheBarsScrollerHasNoSidewaysRange() throws {
        let session = try booted("EXACT_COVER_SCROLL_PLAN", label: "cover-scroll")
        let sv = try XCTUnwrap(try node(session, "home-scroll").scroll)
        let host = try XCTUnwrap(window?.rootViewController), before = sv.safeAreaInsets.right
        host.additionalSafeAreaInsets = UIEdgeInsets(top: 0, left: 0, bottom: 0, right: 84)
        host.view.layoutIfNeeded()
        until("the scroller lies under the side inset") { sv.safeAreaInsets.right >= before + 83.5 }
        XCTAssertEqual(sv.adjustedContentInset.left, 0, accuracy: 0.5)
        XCTAssertEqual(sv.adjustedContentInset.right, 0, accuracy: 0.5, "UIKit adds no side inset")
        XCTAssertLessThanOrEqual(sv.contentSize.width + sv.adjustedContentInset.left + sv.adjustedContentInset.right, sv.bounds.width + 0.5, "no sideways range")
        XCTAssertGreaterThan(sv.adjustedContentInset.top, 0, "inset under the bar")
    }

    func testAPoppingRouteKeepsItsSideClearanceUntilItLeavesTheWindow() throws {
        for left in [false, true] {
            let (session, nav, route) = try pushed("tall")
            let navigation = session.presenter.navigation
            let host = try XCTUnwrap(window?.rootViewController)
            host.additionalSafeAreaInsets = UIEdgeInsets(top: 0, left: left ? 84 : 0, bottom: 0, right: left ? 0 : 84)
            host.view.layoutIfNeeded()
            func side() -> CGFloat? {
                guard case .edges(let edges)? = navigation.covers[route.node.id] else { return nil }
                return left ? edges.left : edges.right
            }
            until("the route clears the side safe area") { (side() ?? 0) >= 83.5 }
            let clearance = try XCTUnwrap(side())
            let content = try node(session, "tall-scroll")
            let contentWidth = content.bounds.width
            XCTAssertTrue(nav.popViewController(animated: true) === route)
            let transition = try XCTUnwrap(nav.transitionCoordinator)
            XCTAssertTrue(transition.viewController(forKey: .from) === route)
            XCTAssertFalse(nav.viewControllers.contains(route), "UIKit removes the source before its visible transition ends")
            XCTAssertNotNil(route.view.window)
            navigation.reportCovers()
            XCTAssertEqual(side() ?? 0, clearance, accuracy: 0.5, "the departing screen retains its side clearance")
            XCTAssertEqual(content.bounds.width, contentWidth, accuracy: 0.5, "its content does not reflow as Back begins")
            until("the pop ends") { nav.transitionCoordinator == nil && route.view.window == nil }
            navigation.reportCovers()
            XCTAssertNil(navigation.covers[route.node.id], "the departed screen releases its clearance")
            session.destroy()
            sessions.removeLast()
            window?.isHidden = true
        }
    }

    /// D11.1, several scrollers: the bar's is the one the status bar's tap
    /// already picked, the first that scrolls vertically, by one rule for
    /// both; `navigationScroll` is how a route names a different one.
    func testWithSeveralScrollersTheBarsIsTheStatusBarsAndARouteMayNameAnother() throws {
        let (session, _, pair) = try pushed("pair")
        let first = try node(session, "pair-first"), second = try node(session, "pair-second")
        XCTAssertTrue(pair.collapseScroll === first, "the first vertical scroller is the bar's")
        XCTAssertTrue(session.presenter.scrollsToTopTarget() === first.scroll, "and the status bar's")
        XCTAssertTrue(session.presenter.navigation.contentScroller(of: pair) === first)
        XCTAssertEqual(second.scroll?.scrollsToTop, false)
        sessions.removeLast().destroy()
        window?.isHidden = true
        let (named, _, route) = try pushed("named")
        let other = try node(named, "named-second")
        XCTAssertTrue(named.presenter.navigation.contentScroller(of: route) === other, "the one the route names")
        XCTAssertTrue(named.presenter.scrollsToTopTarget() === other.scroll, "the status bar's too")
        XCTAssertEqual(try node(named, "named-first").scroll?.scrollsToTop, false)
        XCTAssertNil(route.collapseScroll, "it is not at the route's top, so the title keeps still, as before")
    }

    /// D11.1, no header: a pushed route whose bar holds the system Back
    /// alone runs its scroller under that bar too.
    func testAHeaderlessRoutesScrollerRunsUnderTheBackOnlyBar() throws {
        let (session, nav, route) = try pushed("bare")
        let bare = try node(session, "bare-scroll")
        let sv = try XCTUnwrap(bare.scroll)
        XCTAssertFalse(nav.isNavigationBarHidden, "the bar with Back alone (D5.2)")
        XCTAssertTrue(route.collapseScroll === bare)
        XCTAssertEqual(bare.convert(bare.bounds, to: nil).minY, 0, accuracy: 0.5, "under the bar")
        let top = try node(session, "bare-top"), bar = nav.navigationBar
        XCTAssertEqual(top.convert(top.bounds, to: nil).minY, bar.convert(bar.bounds, to: nil).maxY, accuracy: 0.5, "its first line just below the bar")
        XCTAssertEqual(sv.contentOffset.y + sv.adjustedContentInset.top, 0, accuracy: 0.5)
    }

    func testAnAuthoredScrollTopLandsAsTheBrowsersWhileTheTitleCollapses() throws {
        let (session, nav) = try second("collapse")
        let node = try self.node(session, "list-second")
        let sv = try XCTUnwrap(node.scroll)
        spin(0.3)
        // LLP 1115.000 D11.1: Second names no scroller, and its large title
        // collapses with the one right after its header all the same.
        let route = try XCTUnwrap(nav.topViewController as? RouteController)
        XCTAssertEqual(route.node.props["navigationScroll"] ?? "", "", "the route names nothing")
        XCTAssertTrue(route.collapseScroll === node)
        XCTAssertTrue(session.agent(#"{"op":"logs","since":0}"#).contains("collapses its title with its scroller"))
        XCTAssertGreaterThan(node.scrollOrigin, 0, "the expanded inset is CSS 0")
        let css = { sv.contentOffset.y + sv.adjustedContentInset.top }
        let expanded = nav.navigationBar.frame.height
        XCTAssertEqual(css(), 0, accuracy: 0.5, "at rest, CSS 0")
        func assign(_ top: Double) {
            node.pendingScrollTop = top
            node.applyPendingScroll()
            spin(0.3)
        }
        assign(80)
        XCTAssertEqual(css(), 80, accuracy: 0.5, "80 lands at 80")
        XCTAssertLessThan(nav.navigationBar.frame.height, expanded, "and the title collapsed")
        assign(300)
        XCTAssertEqual(css(), 300, accuracy: 0.5, "the middle lands")
        assign(0)
        XCTAssertEqual(css(), 0, accuracy: 0.5, "0 lands at 0 (the title expands when the user pulls)")
        assign(100_000)
        let end = sv.contentSize.height + sv.adjustedContentInset.bottom - sv.bounds.height + sv.adjustedContentInset.top
        XCTAssertEqual(css(), end, accuracy: 0.5, "the end clamps to the scroller's own end")
        // A user's offset reads back in CSS terms, as `layout` reports it.
        sv.setContentOffset(CGPoint(x: 0, y: 30 - sv.adjustedContentInset.top), animated: false)
        spin(0.2)
        let layout = Agent(session: session).layout([:])
        let row = (layout["nodes"] as? [[String: Any]])?.first { ($0["id"] as? Int) == Int(node.id) }
        XCTAssertEqual(row?["sy"] as? Double ?? -1, 30, accuracy: 0.5)
    }

    /// A scroller replaced in place by a taller one, then a shorter one, as
    /// a quiz's results replace its question (bake-off round 3: the results
    /// opened scrolled by the bar, their score and Play Again under it).
    /// UIKit insets a new scroller from the offset it had, 0, which leaves
    /// its content's top under the bar; each opens at its top instead, just
    /// below the bar. The bar stays as the page before left it, its title
    /// collapsed, as UIKit keeps the bar of a hand-built screen whose scroll
    /// view is replaced in place (measured on iOS 27); a pull opens it.
    func testAScrollerReplacedInPlaceOpensAtItsTopBelowTheBarAsItWasLeft() throws {
        let (session, nav) = try second("collapse-replaced")
        let list = try node(session, "list-second")
        spin(0.3)
        let expanded = nav.navigationBar.frame.height
        list.pendingScrollTop = 100_000
        list.applyPendingScroll()
        spin(0.3)
        let collapsed = nav.navigationBar.frame.height
        XCTAssertLessThan(collapsed, expanded, "the title collapsed over the first page's end")
        for (press, page) in [("second-taller", "taller"), ("second-shorter", "shorter")] {
            _ = Agent(session: session).tap(["id": Int(try node(session, press).id)])
            until("the \(page) scroller in place") { (try? self.node(session, "list-second-\(page)")) != nil }
            spin(0.3)
            let sv = try XCTUnwrap(try node(session, "list-second-\(page)").scroll)
            let top = try node(session, "second-\(page)-top"), bar = nav.navigationBar
            XCTAssertEqual(sv.contentOffset.y + sv.adjustedContentInset.top, 0, accuracy: 0.5, "the \(page) page at its top (CSS 0)")
            XCTAssertEqual(top.convert(top.bounds, to: nil).minY, bar.convert(bar.bounds, to: nil).maxY + 12, accuracy: 0.5,
                           "its first line just below the bar, past its 12 of padding")
            XCTAssertEqual(bar.frame.height, collapsed, accuracy: 0.5, "the title still collapsed, as UIKit leaves it")
        }
    }
}
#endif
