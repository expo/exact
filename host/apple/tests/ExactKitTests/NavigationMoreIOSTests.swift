#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1075.003 §3.7 over more root tabs than an iPhone's tab bar shows
/// (`more-tabs.contract`), so UIKit lists the last ones under its More item.
/// UIKit's selection is the state and the router follows it; the bar is
/// driven as a tap drives it (`tapTab`).
final class NavigationMoreIOSTests: XCTestCase {
    private var window: UIWindow?
    private var sessions: [ExactSession] = []
    private var animationsWereEnabled = true

    override func setUp() {
        super.setUp()
        // Hostless UIKit does not finish an animated tab selection.
        animationsWereEnabled = UIView.areAnimationsEnabled
        UIView.setAnimationsEnabled(false)
    }

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        UIView.setAnimationsEnabled(animationsWereEnabled)
        super.tearDown()
    }

    /// A tab's press that moves the router to a third tab, neither the one
    /// tapped nor the one shown, is the app's answer to the tap: UIKit shows
    /// the router's tab, and the tapped one is not selected.
    func testAPressThatMovesTheRouterToAnotherTabShowsThatTab() throws {
        let session = try fixture("more-redirect")
        let navigation = session.presenter.navigation
        let tabs = try XCTUnwrap(navigation.tabController)
        try tapNode(session, "redirect")
        let redirect = try node(session, "redirect")
        waitFor("Redirect is on") { redirect.accessibleText.contains("Redirecting") }
        tapTab(tabs, 3)
        XCTAssertEqual(navigation.routerTab, 2, "Four's press selected Three")
        let three = try XCTUnwrap(tabs.tabControllers?[2])
        XCTAssertTrue(tabs.selectedViewController === three, "UIKit shows the router's tab")
        XCTAssertTrue(navigation.primaryNavigation === three)
        XCTAssertEqual(three.stackRoutes.count, 1, "Three is built")
    }

    /// A tab not built yet whose root shows no bar is its navigation
    /// controller, empty, its bar hidden; the tap's press fills it before
    /// the call returns.
    func testATapOnATabNotBuiltThatShowsNoBarFillsItsController() throws {
        let session = try fixture("more-barless")
        let navigation = session.presenter.navigation
        let tabs = try XCTUnwrap(navigation.tabController)
        let barless = try XCTUnwrap(tabs.tabControllers?[1] as? UINavigationController, "Barless's panel draws its stack")
        XCTAssertTrue(barless.viewControllers.isEmpty)
        tapTab(tabs, 1)
        XCTAssertTrue(tabs.selectedViewController === barless)
        XCTAssertEqual(navigation.routerTab, 1)
        XCTAssertTrue(navigation.primaryNavigation === barless)
        XCTAssertEqual((barless.viewControllers.first as? RouteController)?.node.props["testId"], "route-barless")
        XCTAssertTrue(barless.isNavigationBarHidden)
        XCTAssertNotNil(barless.viewControllers.first?.view.window)
    }

    /// A tab not built yet gets its routes within the tap, though another
    /// tab's push still runs: its navigation controller is the same one, so
    /// nothing in the tab controller is replaced under that transition.
    func testATabNotBuiltIsBuiltWithinATapDuringAnotherTabsPush() throws {
        let session = try fixture("more-mid-push")
        let navigation = session.presenter.navigation
        let tabs = try XCTUnwrap(navigation.tabController)
        let home = try XCTUnwrap(tabs.tabControllers?[0] as? UINavigationController)
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        try tapNode(session, "open-detail")
        waitFor("Home's push runs") { home.transitionCoordinator != nil }
        let unbuilt = try XCTUnwrap(tabs.tabControllers?[2] as? UINavigationController)
        tapTab(tabs, 2)
        let three = try XCTUnwrap(tabs.selectedViewController as? UINavigationController, "Three's navigation controller, built within the tap")
        XCTAssertTrue(three === unbuilt)
        XCTAssertEqual(three.viewControllers.count, 1)
        XCTAssertTrue(navigation.primaryNavigation === three)
    }

    /// More tapped: before its list shows, the runner builds each tab whose
    /// row it lists and whose navigation controller is empty, selecting
    /// none, since a row for an empty navigation controller does nothing
    /// (iOS 27, measured). A plain tab is pushed empty, so it waits.
    func testMoreTappedBuildsTheTabsItListsWithoutSelectingThem() throws {
        let (session, tabs, more) = try moreFixture("more-builds")
        let navigation = session.presenter.navigation
        let five = try XCTUnwrap(tabs.tabControllers?[4] as? UINavigationController)
        let six = try XCTUnwrap(tabs.tabControllers?[5] as? PlainTab, "Six's panel shows one fixed route")
        XCTAssertTrue(five.viewControllers.isEmpty && six.route == nil, "neither is built")
        tapTab(tabs, of: tabs.moreNavigationController)
        XCTAssertEqual((five.viewControllers.first as? RouteController)?.node.props["testId"], "route-five", "Five is built")
        XCTAssertNil(six.route, "Six waits for its row")
        XCTAssertEqual(navigation.routerTab, 0, "the router stays on Home")
        XCTAssertTrue(tabs.selectedViewController === more)
        XCTAssertTrue(more.stackRoutes.isEmpty, "More shows its list")
    }

    /// A row of More's list: UIKit sends no tab delegate call, More's
    /// `willShow` presses the tab, and UIKit moves its routes into More's
    /// stack above its list (iOS 27, measured). The root shows UIKit's Back
    /// to the list.
    func testARowOfMoresListSelectsItsTabAndItsRoutesShowInMore() throws {
        let (session, tabs, more) = try moreFixture("more-row")
        let navigation = session.presenter.navigation
        tapTab(tabs, of: tabs.moreNavigationController)
        pickRow(tabs, 4)
        waitFor("the router follows Five") { navigation.routerTab == 4 }
        waitFor("More holds Five's root above its list") { more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        let root = try XCTUnwrap(more.stackRoutes.first as? RouteController)
        XCTAssertEqual(root.node.props["testId"], "route-five")
        XCTAssertEqual(more.stackFront.count, 1, "UIKit's list stays before them")
        XCTAssertEqual((tabs.tabControllers?[4] as? UINavigationController)?.viewControllers.count, 0, "UIKit moved its routes into More")
        XCTAssertTrue(navigation.primaryNavigation === more)
        XCTAssertTrue(root.pushed, "pushed over the list")
        XCTAssertFalse(more.isNavigationBarHidden)
        XCTAssertTrue(more.navigationBar.topItem === root.navigationItem, "More's bar shows the route's item")
        XCTAssertFalse(root.navigationItem.hidesBackButton, "with UIKit's Back to the list")
        XCTAssertTrue(navigation.primaryRoutes.contains { $0 === root }, "a route of the root's stacks (owner blocks, the status bar's tap)")
    }

    /// A push in a tab under More goes onto More's stack, and UIKit's Back
    /// there is the router's Back. More's own Back on the root goes to its
    /// list, which is no route: the router keeps its tab.
    func testATabUnderMorePushesThereAndItsBackGoesToTheList() throws {
        let (session, tabs, more) = try moreFixture("more-push")
        let navigation = session.presenter.navigation
        tapTab(tabs, of: tabs.moreNavigationController)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { session.presenter.navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        try tapNode(session, "open-deeper")
        waitFor("Deeper pushed onto More") { more.stackRoutes.count == 2 && more.transitionCoordinator == nil && !navigation.changing }
        XCTAssertEqual((more.topViewController as? RouteController)?.node.props["testId"], "route-deeper")
        XCTAssertEqual(more.stackFront.count, 1)
        more.popViewController(animated: false)
        waitFor("the router popped Deeper") { (navigation.observation()["stack"] as? [String])?.count == 1 && !navigation.changing }
        XCTAssertEqual(navigation.observation()["route"] as? String, (more.stackRoutes.first as? RouteController)?.key)
        let five = navigation.observation()["route"] as? String
        let swipe = try XCTUnwrap(more.interactivePopGestureRecognizer)
        XCTAssertTrue(navigation.popMayBegin(swipe, from: nil, in: nil, velocity: .zero), "the root's swipe to the list is UIKit's")
        more.popViewController(animated: false)
        waitFor("More shows its list") { more.stackRoutes.isEmpty && !navigation.changing }
        waitForMainQueue()
        XCTAssertEqual(navigation.routerTab, 4, "the list is no tab: the router keeps Five")
        XCTAssertEqual(navigation.observation()["route"] as? String, five, "and its root")
        // More's pop to its list leaves Five's root in Five's own stack (LLP 1075.003 §9.16); a sync there prepares
        // it as that stack's root, whose bar is hidden, before Five is picked again
        navigation.sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        let watcher = BarWatcher()
        navigation.setAppDelegate(more, watcher)
        // Five picked again: UIKit moves its routes back into More, which navigation follows within More's willShow,
        // a turn later
        pickRow(tabs, 4)
        waitFor("More's willShow for Five's root") { !watcher.hiddenAtWillShow.isEmpty && more.transitionCoordinator == nil }
        XCTAssertEqual(watcher.hiddenAtWillShow, [false], "More's bar shows in More's willShow, before its push draws")
        XCTAssertTrue(more.topViewController is RouteController, "More shows Five's root again")
        XCTAssertFalse(more.isNavigationBarHidden, "More's bar, with its Back to the list, for a root with no header")
        waitForMainQueue()
        XCTAssertTrue(navigation.primaryNavigation === more, "navigation follows More within its willShow")
        XCTAssertEqual(navigation.observation()["stack"] as? [String], five.map { [$0] })
    }

    /// A width that puts every tab on the bar moves a tab's routes out of
    /// More into its own stack (measured): UIKit's Back there is the
    /// router's Back, as in any stack.
    func testATabLeavingMoreKeepsItsBack() throws {
        let (session, tabs, more) = try moreFixture("more-leave")
        let navigation = session.presenter.navigation
        tapTab(tabs, of: tabs.moreNavigationController)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { session.presenter.navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        let root = try XCTUnwrap(navigation.observation()["route"] as? String)
        try tapNode(session, "open-deeper")
        waitFor("Deeper pushed onto More") { more.stackRoutes.count == 2 && more.transitionCoordinator == nil && !navigation.changing }
        tabs.traitOverrides.horizontalSizeClass = .regular
        tabs.view.setNeedsLayout()
        tabs.view.layoutIfNeeded()
        let five = try XCTUnwrap(tabs.tabControllers?[4] as? UINavigationController)
        waitFor("Five's routes are in its own stack, which navigation follows") { five.viewControllers.count == 2 && navigation.primaryNavigation === five }
        five.popViewController(animated: false)
        waitFor("the router is back on Five's root") { navigation.observation()["route"] as? String == root && !navigation.changing }
        waitForMainQueue()
        XCTAssertEqual(five.viewControllers.count, 1, "no batch pushes Deeper back")
    }

    /// UIKit makes a new More when it needs one again (iOS 27.1, measured): a
    /// regular width moves Five's routes out of More, and a compact one moves
    /// them into a More, which Exact adopts as the routes move. Its bar shows
    /// Deeper's header once, with Back, and the router stays on Deeper.
    func testATabUIKitMovesIntoAMoreAgainShowsItsHeaderOnce() throws {
        let (session, tabs, first) = try moreFixture("more-again")
        let navigation = session.presenter.navigation
        tapTab(tabs, of: first)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { navigation.routerTab == 4 && first.stackRoutes.count == 1 && first.transitionCoordinator == nil }
        try tapNode(session, "open-deeper")
        waitFor("Deeper pushed onto More") { first.stackRoutes.count == 2 && first.transitionCoordinator == nil && !navigation.changing }
        let deeper = try XCTUnwrap(first.topViewController as? RouteController)
        let five = try XCTUnwrap(tabs.tabControllers?[4] as? UINavigationController)
        setSizeClasses(tabs, horizontal: .regular)
        waitFor("Five's routes are in its own stack") { deeper.navigationController === five && navigation.primaryNavigation === five && !navigation.changing }
        setSizeClasses(tabs, horizontal: .compact)
        let again = try waitForAdoptedMore(navigation, holding: deeper, apart: five)
        if again !== first { assertLetGo(navigation, first) }
    }

    /// A height-only size class change at a compact width, as the folded
    /// Duo's cover display turned sideways makes: where the bar shows every
    /// tab at a regular height, a compact one moves the last ones into More,
    /// a new More each time (LLP 1075.003 §9.16). Exact adopts it as the
    /// routes move, also when a fold makes several moves in one turn, and its
    /// bar shows Deeper's header once, with Back.
    func testAHeightChangeAtCompactWidthMovesATabIntoMoreWithOneHeader() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit lays the iPhone Duo's side bar out only in a scene's window") }
        let session = try fixture("more-height", wholeScreen: true)
        let navigation = session.presenter.navigation
        let tabs = try XCTUnwrap(navigation.tabController)
        setSizeClasses(tabs, horizontal: .compact, vertical: .regular)
        let items = tabs.tabBar.items?.count ?? 0
        guard items == 6 else { throw XCTSkip("UIKit's bar shows \(items) items at a compact width and a regular height in a \(tabs.view.bounds.size) window here, not every tab (the iPhone Duo's shows them all)") }
        tapTab(tabs, 4)
        let five = try XCTUnwrap(tabs.tabControllers?[4] as? UINavigationController)
        waitFor("Five selected and built") { navigation.routerTab == 4 && five.viewControllers.count == 1 }
        try tapNode(session, "open-deeper")
        waitFor("Deeper pushed onto Five") { five.viewControllers.count == 2 && five.transitionCoordinator == nil && !navigation.changing }
        let deeper = try XCTUnwrap(five.topViewController as? RouteController)
        setSizeClasses(tabs, horizontal: .compact, vertical: .compact)
        let more = try waitForAdoptedMore(navigation, holding: deeper, apart: five)
        setSizeClasses(tabs, horizontal: .compact, vertical: .regular)
        waitFor("Five's routes are back in its own stack") { deeper.navigationController === five && navigation.primaryNavigation === five && !navigation.changing }
        setSizeClasses(tabs, horizontal: .compact, vertical: .compact)
        let again = try waitForAdoptedMore(navigation, holding: deeper, apart: five)
        if again !== more { assertLetGo(navigation, more) }
        // Several moves in one turn, as a fold makes them: out of More, into another, out and into a third
        setSizeClasses(tabs, horizontal: .compact, vertical: .regular)
        setSizeClasses(tabs, horizontal: .compact, vertical: .compact)
        setSizeClasses(tabs, horizontal: .compact, vertical: .regular)
        setSizeClasses(tabs, horizontal: .compact, vertical: .compact)
        let last = try waitForAdoptedMore(navigation, holding: deeper, apart: five)
        if last !== again { assertLetGo(navigation, again) }
    }

    /// A More Exact adopted before the one it follows now has no record and
    /// no delegate of Exact's.
    private func assertLetGo(_ navigation: NavigationHost, _ more: UINavigationController, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertNil(navigation.stacks[ObjectIdentifier(more)], "the More UIKit let go has no record", file: file, line: line)
        XCTAssertFalse(more.delegate is NavigationDelegateProxy, "nor Exact's delegate", file: file, line: line)
    }

    /// The More UIKit moved `route`'s stack into, once Exact follows it: Exact
    /// is its delegate, navigation follows it, its bar shows the route's
    /// header as the title (lifted out of the content) with Back, and the
    /// router stays on the route.
    @discardableResult
    private func waitForAdoptedMore(_ navigation: NavigationHost, holding route: RouteController, apart tab: UINavigationController) throws -> UINavigationController {
        let key = route.key
        waitFor("UIKit moves the routes into a More, which Exact adopts and follows", facts: { "parent \(String(describing: route.navigationController)), navigation \(String(describing: navigation.primaryNavigation)), adopted \(String(describing: navigation.adoptedMore)) holding \(navigation.adoptedMore?.viewControllers ?? []), its delegate Exact's \(navigation.adoptedMore.map { self.delegatesToExact(navigation, $0) } ?? false), the tab's own \(tab.viewControllers)" }) {
            guard let more = route.navigationController, more !== tab, more.transitionCoordinator == nil else { return false }
            return navigation.more === more && delegatesToExact(navigation, more) && navigation.primaryNavigation === more && !navigation.changing
        }
        let more = try XCTUnwrap(route.navigationController)
        XCTAssertTrue(navigation.more === more, "Exact's More is the one UIKit shows")
        XCTAssertFalse(more.isNavigationBarHidden, "More's bar")
        XCTAssertTrue(more.navigationBar.topItem === route.navigationItem, "with the route's item")
        XCTAssertTrue(route.lifted?.isHidden == true, "the route's header is the bar's title, not content")
        XCTAssertFalse(route.navigationItem.hidesBackButton, "and UIKit's Back")
        XCTAssertEqual(navigation.observation()["route"] as? String, key, "the router stays on the route")
        return more
    }

    /// A row whose tab's press leaves the router where it was is the app
    /// declining: UIKit shows the router's tab again, as for a tap on the
    /// bar, and More its list. The pick presses the tab once.
    func testARowWhosePressIsDeclinedShowsTheRoutersTabAgain() throws {
        let (session, tabs, more) = try moreFixture("more-declined")
        let navigation = session.presenter.navigation
        try tapNode(session, "decline")
        let six = try node(session, "tab-six").id
        let home = try XCTUnwrap(tabs.tabControllers?[0])
        tapTab(tabs, of: tabs.moreNavigationController)
        pickRow(tabs, 5)
        waitFor("UIKit shows Home again") { tabs.selectedViewController === home }
        waitForMainQueue()
        XCTAssertEqual(navigation.routerTab, 0)
        XCTAssertEqual(more.viewControllers.count, 1, "More shows its list again, not the declined tab")
        XCTAssertEqual(journal(session, "press view \(six) (pick)"), 1, "Six's press ran once")
    }

    /// The same for a tab whose routes UIKit moves into More, a navigation
    /// controller: More's pop to its list takes its root out of More, and
    /// UIKit puts the root back in the tab's own controller, so a later pick
    /// of its row shows it.
    func testARowOfANavigationTabWhosePressIsDeclinedKeepsItsRoot() throws {
        let (session, tabs, more) = try moreFixture("more-declined-stack")
        let navigation = session.presenter.navigation
        let five = try XCTUnwrap(tabs.viewControllers?[4] as? UINavigationController)
        let press = try node(session, "tab-five")
        press.handlers.remove("press")
        let home = try XCTUnwrap(tabs.viewControllers?[0])
        tapTab(tabs, of: more)
        waitFor("Five is built for More's list") { five.viewControllers.count == 1 }
        pickRow(tabs, 4)
        waitFor("UIKit shows Home again, and Five's own controller holds its root", facts: { "selected \(String(describing: tabs.selectedViewController)), Five holds \(five.viewControllers.count), More holds \(more.viewControllers.count), in transition \(navigation.inTransition)" }) {
            tabs.selectedViewController === home && five.viewControllers.count == 1 && !navigation.inTransition
        }
        XCTAssertEqual(navigation.routerTab, 0)
        XCTAssertNil(navigation.restDifference, "UIKit and the router agree")
        press.handlers.insert("press")
        tapTab(tabs, of: more)
        pickRow(tabs, 4)
        waitFor("Five's row shows its root") { navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil && !navigation.inTransition }
    }

    /// More's list is no route: a swipe there pops nothing, though the
    /// router's tab has a route pushed and its Back goes.
    func testASwipeOnMoresListPopsNothing() throws {
        let (session, tabs, more) = try moreFixture("more-list-swipe")
        let navigation = session.presenter.navigation
        let home = try XCTUnwrap(tabs.tabControllers?[0] as? UINavigationController)
        try tapNode(session, "open-detail")
        waitFor("Home pushed Detail") { home.viewControllers.count == 2 && !navigation.changing }
        XCTAssertTrue(navigation.canInvokeBack)
        tapTab(tabs, of: more)
        waitFor("More shows its list") { tabs.selectedViewController === more && more.viewControllers.count == 1 }
        let swipe = try XCTUnwrap(more.interactivePopGestureRecognizer)
        XCTAssertFalse(navigation.popMayBegin(swipe, from: nil, in: nil, velocity: .zero), "More's list has nothing to pop")
    }

    /// A plain tab's row: More pushes the tab's own controller, empty, and
    /// its press builds the route into it. More's bar shows the tab's title
    /// with UIKit's Back to the list, the route's cover clears that bar, and
    /// More's swipe back is UIKit's.
    func testAPlainTabUnderMoreShowsUnderMoresBar() throws {
        // A hostless run gives the route no safe area under More, where its plain tab has one (iOS 27.0, measured)
        guard hostedWindowScene() != nil else { throw missing("scene", "a controller's child under More gets its safe area only in a scene's window") }
        let (session, tabs, more) = try moreFixture("more-plain")
        let navigation = session.presenter.navigation
        let six = try XCTUnwrap(tabs.tabControllers?[5] as? PlainTab)
        tapTab(tabs, of: tabs.moreNavigationController)
        pickRow(tabs, 5)
        waitFor("the router follows Six") { navigation.routerTab == 5 }
        waitFor("More shows Six") { more.topViewController === six && more.transitionCoordinator == nil }
        XCTAssertEqual(six.route?.node.props["testId"], "route-six")
        XCTAssertNotNil(six.route?.view.window)
        XCTAssertTrue(navigation.primaryPlain === six)
        XCTAssertTrue(navigation.activeRoute === six.route)
        XCTAssertFalse(more.isNavigationBarHidden, "More's bar")
        XCTAssertEqual(more.navigationBar.topItem?.title, "Six")
        XCTAssertNotNil(more.navigationBar.backItem, "with UIKit's Back to the list")
        // A route's cover is its safe area past the document's own insets (`routeEdges`)
        let route = try XCTUnwrap(six.route)
        let barBottom = more.navigationBar.convert(more.navigationBar.bounds, to: nil).maxY
        waitFor("Six's route clears More's bar", facts: { "cover \(String(describing: navigation.covers[route.node.id])), safe area \(route.view.safeAreaInsets)" }) {
            guard case .edges(let edges)? = navigation.covers[route.node.id] else { return false }
            return edges.top + session.presenter.insets.top >= barBottom - 0.5
        }
        let swipe = try XCTUnwrap(more.interactivePopGestureRecognizer)
        XCTAssertTrue(navigation.popMayBegin(swipe, from: nil, in: nil, velocity: .zero), "More's swipe to its list is UIKit's")
    }

    /// A tab whose panel shows one fixed route with a header is a navigation
    /// controller from launch, built or not, and its bar shows that header,
    /// as the platform shows a screen's header (LLP 1075.003 §3.7).
    func testAFixedRouteWithAHeaderShowsItInItsTabsNavigationBar() throws {
        let session = try fixture("more-four")
        let navigation = session.presenter.navigation
        let tabs = try XCTUnwrap(navigation.tabController)
        let four = try XCTUnwrap(tabs.tabControllers?[3] as? UINavigationController, "Four's route can show a header")
        XCTAssertTrue(four.viewControllers.isEmpty, "Four is not built yet")
        tapTab(tabs, 3)
        waitFor("Four shows its route") { navigation.routerTab == 3 && four.viewControllers.count == 1 && four.viewIfLoaded?.window != nil }
        let root = try XCTUnwrap(four.viewControllers.first as? RouteController)
        XCTAssertTrue(tabs.tabControllers?[3] === four, "the tab keeps its controller")
        XCTAssertFalse(four.isNavigationBarHidden, "the header is the bar")
        XCTAssertEqual(root.navigationItem.title, "four")
    }

    /// A plain tab on the bar has no navigation bar: its route's cover is its
    /// own safe area past the document's insets, as a route under a hidden
    /// bar has (`routeEdges`).
    func testAPlainTabOnTheBarIsCoveredForItsSafeArea() throws {
        let session = try fixture("more-plain-bar")
        let navigation = session.presenter.navigation
        let tabs = try XCTUnwrap(navigation.tabController)
        tabs.traitOverrides.horizontalSizeClass = .regular
        tabs.view.setNeedsLayout()
        tabs.view.layoutIfNeeded()
        let six = try XCTUnwrap(tabs.tabControllers?[5] as? PlainTab)
        waitFor("the bar shows Six") { navigation.barItem(ofTab: 5) != nil }
        tapTab(tabs, 5)
        waitFor("the router follows Six") { navigation.routerTab == 5 && six.route?.view.window != nil }
        let route = try XCTUnwrap(six.route)
        let document = session.presenter.insets
        waitFor("Six's route is covered for its safe area", facts: { "cover \(String(describing: navigation.covers[route.node.id])), safe area \(route.view.safeAreaInsets), document \(document)" }) {
            guard case .edges(let edges)? = navigation.covers[route.node.id] else { return false }
            return abs(edges.top - max(0, route.view.safeAreaInsets.top - document.top)) < 0.5 && (edges.top > 0 || edges.bottom > 0)
        }
    }

    /// More tapped while it shows a tab above its root is UIKit's pop to its
    /// list (measured): refused where Back does not go (LLP 1035.001 D1).
    func testMoreTappedAgainAboveARootWhereBackIsRefusedPopsNothing() throws {
        let (session, tabs, more) = try moreFixture("more-reselect")
        tapTab(tabs, of: tabs.moreNavigationController)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { session.presenter.navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        try tapNode(session, "open-deeper")
        waitFor("Deeper pushed onto More") { more.stackRoutes.count == 2 && more.transitionCoordinator == nil }
        try node(session, "back-deeper").handlers.remove("press")
        XCTAssertEqual(tabs.delegate?.tabBarController?(tabs, shouldSelect: more), false, "More's pop is refused")
    }

    /// More tapped again while it shows a tab one route above its root,
    /// where Back goes: UIKit pops More to its list (measured). The route
    /// that goes with it is the platform's Back, which presses the authored
    /// Back control; the list is no route, so the router keeps the tab.
    func testMoreTappedAgainAboveARootTakesTheRouterBackToIt() throws {
        let (session, tabs, more) = try moreFixture("more-retap")
        let navigation = session.presenter.navigation
        tapTab(tabs, of: tabs.moreNavigationController)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { session.presenter.navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        let five = try XCTUnwrap(navigation.observation()["route"] as? String)
        try tapNode(session, "open-deeper")
        waitFor("Deeper pushed onto More") { more.stackRoutes.count == 2 && more.transitionCoordinator == nil && !navigation.changing }
        let back = try node(session, "back-deeper").id
        // More's tap as UIKit performs it (measured): `shouldSelect(more)`, `didSelect` of the tab it shows, then the pop to the list
        XCTAssertEqual(tabs.delegate?.tabBarController?(tabs, shouldSelect: more), true)
        tabs.delegate?.tabBarController?(tabs, didSelect: try XCTUnwrap(tabs.selectedViewController))
        more.popToRootViewController(animated: false)
        waitFor("the router is back on Five's root") { navigation.observation()["route"] as? String == five && !navigation.changing }
        XCTAssertEqual(navigation.routerTab, 4)
        XCTAssertTrue(more.stackRoutes.isEmpty, "More shows its list")
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "the authored Back pressed")
    }

    /// A compact width moves a tab's stack, pushed on the bar, into More
    /// (measured), and More tapped again pops More to its list, animated as a
    /// tap animates it: UIKit puts the tab's root back in the tab's own
    /// controller, and no `didShow` of that controller says so. The router
    /// goes back to the root, and UIKit and the router agree.
    func testMoreTappedAgainAfterAWidthMovedATabsStackIntoItTakesTheRouterBackToItsRoot() throws {
        let (session, tabs, _) = try moreFixture("more-retap-moved")
        let navigation = session.presenter.navigation
        setSizeClasses(tabs, horizontal: .regular)
        let five = try XCTUnwrap(tabs.tabControllers?[4] as? UINavigationController)
        tapTab(tabs, 4)
        waitFor("the router follows Five on the bar") { navigation.routerTab == 4 && five.viewControllers.count == 1 && navigation.primaryNavigation === five && !navigation.changing }
        let root = try XCTUnwrap(navigation.observation()["route"] as? String)
        try tapNode(session, "open-deeper")
        waitFor("Deeper pushed onto Five") { five.viewControllers.count == 2 && five.transitionCoordinator == nil && !navigation.changing }
        let deeper = try XCTUnwrap(five.topViewController as? RouteController)
        setSizeClasses(tabs, horizontal: .compact)
        let more = try waitForAdoptedMore(navigation, holding: deeper, apart: five)
        // Tap after the next turn's sync, which follows UIKit's move
        waitForMainQueue()
        // Unanimated, Exact's sync writes the root into Five's empty controller before UIKit puts it back (measured)
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        XCTAssertEqual(tabs.delegate?.tabBarController?(tabs, shouldSelect: more), true)
        tabs.delegate?.tabBarController?(tabs, didSelect: try XCTUnwrap(tabs.selectedViewController))
        more.popToRootViewController(animated: true)
        waitFor("the router is back on Five's root, and no pop is pending", facts: {
            "route \(String(describing: navigation.observation()["route"])), changing \(navigation.changing), Five holds \(five.viewControllers.count)"
        }) { navigation.observation()["route"] as? String == root && !navigation.changing && more.transitionCoordinator == nil }
        waitForMainQueue()
        XCTAssertEqual(navigation.routerTab, 4)
        XCTAssertTrue(more.stackRoutes.isEmpty, "More shows its list")
        XCTAssertEqual(five.viewControllers.count, 1, "Five's own controller holds its root")
        XCTAssertNil(navigation.restDifference, "UIKit and the router agree")
    }

    /// UIKit's animated Back under More reaches the router as the pop
    /// begins, and a batch synced once its animations start pushes nothing
    /// back.
    func testASyncDuringAnAnimatedBackUnderMoreKeepsTheBack() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit keeps a popped controller as More's child as the animations start only in a scene's window") }
        let (session, tabs, more) = try moreFixture("more-back-sync")
        let navigation = session.presenter.navigation
        tapTab(tabs, of: tabs.moreNavigationController)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        let root = try XCTUnwrap(navigation.observation()["route"] as? String)
        try tapNode(session, "open-deeper")
        waitFor("Deeper pushed onto More") { more.stackRoutes.count == 2 && more.transitionCoordinator == nil && !navigation.changing }
        let back = try node(session, "back-deeper").id
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        more.popViewController(animated: true)
        waitForAnimationsToStart(more)
        navigation.sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        waitFor("the router is back on Five's root", facts: { "route \(String(describing: navigation.observation()["route"])), More holds \(more.viewControllers.count), changing \(navigation.changing)" }) {
            navigation.observation()["route"] as? String == root && more.transitionCoordinator == nil && !navigation.changing
        }
        waitForMainQueue()
        XCTAssertEqual(more.stackRoutes.count, 1, "no sync pushes Deeper back")
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "UIKit's Back is the router's")
    }

    /// More tapped while Home's animated pop runs: More's list shows, Home's
    /// Back reached the router as the pop began, and Detail does not come
    /// back when Home shows again.
    func testMoreTappedDuringAnotherTabsPopKeepsThatPopsBack() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "a pop left off screen by a tap on More ends as UIKit's does only in a scene's window") }
        let (session, tabs, more) = try moreFixture("more-during-pop")
        let navigation = session.presenter.navigation
        let home = try XCTUnwrap(tabs.tabControllers?[0] as? UINavigationController)
        let root = try XCTUnwrap(navigation.observation()["route"] as? String)
        try tapNode(session, "open-detail")
        waitFor("Detail pushed in Home") { home.viewControllers.count == 2 && home.transitionCoordinator == nil && !navigation.changing }
        let back = try node(session, "back-detail").id
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        home.popViewController(animated: true)
        waitForAnimationsToStart(home)
        tapTab(tabs, of: more)
        waitFor("More selected, showing its list") { tabs.selectedViewController === more && more.stackRoutes.isEmpty && more.transitionCoordinator == nil }
        waitForMainQueue()
        tapTab(tabs, 0)
        waitFor("the router is back on Home's root", facts: { "route \(String(describing: navigation.observation()["route"])), Home holds \(home.viewControllers.count), changing \(navigation.changing)" }) {
            navigation.observation()["route"] as? String == root && home.transitionCoordinator == nil && !navigation.changing
        }
        waitForMainQueue()
        XCTAssertEqual(home.viewControllers.count, 1, "no sync pushes Detail back")
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "Home's pop is the router's Back")
    }

    /// A tab on the bar tapped once More's animated Back has started: UIKit
    /// ends the pop off screen, and More sends no `didShow` then (LLP 1075.003
    /// §9.16). The Back reaches the router as the pop begins, syncs on the
    /// tab tapped reach UIKit, so a push there shows, and no sync puts the
    /// popped route back.
    func testABarTabTappedDuringMoresBackLeavesItsSyncsFreeAndKeepsTheBack() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates More's pop and the tab change as in an app only in a scene's window") }
        try tapABarTabDuringMoresBack("more-pop-leave", started: true)
    }

    /// As above, with Home tapped in the turn the Back is asked for, before
    /// its animations start: More's pop starts within the tap, once Home's
    /// press has moved the router, and More's `willShow` for Five's root then
    /// presses nothing (§9.16).
    func testABarTabTappedInTheTurnOfMoresBackLeavesItsSyncsFreeAndKeepsTheBack() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates More's pop and the tab change as in an app only in a scene's window") }
        try tapABarTabDuringMoresBack("more-pop-leave-turn", started: false)
    }

    private func tapABarTabDuringMoresBack(_ label: String, started: Bool) throws {
        let (session, tabs, more, root, back) = try deeperUnderMore(label)
        let navigation = session.presenter.navigation
        let home = try XCTUnwrap(tabs.tabControllers?[0] as? UINavigationController)
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        more.popViewController(animated: true)
        if started { waitForAnimationsToStart(more) }
        tapTab(tabs, 0)
        waitFor("the router follows Home") { navigation.routerTab == 0 && tabs.selectedViewController === home && home.transitionCoordinator == nil }
        try tapNode(session, "open-detail")
        waitFor("Detail pushed in Home", facts: { "Home holds \(home.viewControllers.count), changing \(navigation.changing)" }) {
            home.viewControllers.count == 2 && home.transitionCoordinator == nil && !navigation.changing
        }
        XCTAssertEqual(more.stackRoutes.count, 1, "no sync put Deeper back in More")
        XCTAssertEqual(journal(session, "press left the router"), 0, "no tab's press was declined")
        let detailBack = try node(session, "back-detail").id
        tapTab(tabs, of: more)
        waitFor("More shows Five's root, and the router follows", facts: { "router tab \(navigation.routerTab), route \(String(describing: navigation.observation()["route"])), More holds \(more.viewControllers.count), changing \(navigation.changing)" }) {
            navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil && !navigation.changing
                && navigation.observation()["route"] as? String == root
        }
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "UIKit's Back under More is the router's")
        XCTAssertEqual(journal(session, "press view \(detailBack) (back)"), 0, "and not Home's")
        XCTAssertEqual(home.viewControllers.count, 2, "Home still holds Detail")
    }

    /// The router returning, by its own selection, to a tab whose Back under
    /// More ended off screen, as the agent's tap on the tab's authored tab
    /// does: no sync waits, and More shows the route the router is on.
    func testTheRouterGoingBackToATabWhoseBackEndedOffScreenWaitsForNothing() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit ends More's pop off screen as in an app only in a scene's window") }
        let (session, _, more, _) = try leaveMoresBackForHome("more-kept-router")
        let navigation = session.presenter.navigation
        try tapNode(session, "tab-five")
        waitFor("the router follows Five under More, with nothing pending", facts: { "router tab \(navigation.routerTab), route \(String(describing: navigation.observation()["route"])), More holds \(more.viewControllers.count), in transition \(navigation.inTransition)" }) {
            navigation.routerTab == 4 && more.transitionCoordinator == nil && !navigation.inTransition
        }
        waitForMainQueue()
        XCTAssertEqual((more.topViewController as? RouteController)?.key, navigation.observation()["route"] as? String, "More shows the route the router is on")
    }

    /// A regular width moving a tab's routes out of More after its Back ended
    /// off screen: the router follows Five on the bar, with nothing pending.
    func testAWidthMovingATabOutOfMoreAfterItsBackEndedOffScreenWaitsForNothing() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit ends More's pop off screen as in an app only in a scene's window") }
        let (session, tabs, _, _) = try leaveMoresBackForHome("more-kept-width")
        let navigation = session.presenter.navigation
        setSizeClasses(tabs, horizontal: .regular)
        let five = try XCTUnwrap(tabs.tabControllers?[4] as? UINavigationController)
        waitFor("Five is on the bar") { navigation.barItem(ofTab: 4) != nil }
        waitForMainQueue()
        tapTab(tabs, 4)
        waitFor("the router follows Five on the bar, with nothing pending", facts: { "router tab \(navigation.routerTab), route \(String(describing: navigation.observation()["route"])), Five holds \(five.viewControllers.count), in transition \(navigation.inTransition)" }) {
            navigation.routerTab == 4 && five.transitionCoordinator == nil && !navigation.inTransition
        }
        XCTAssertEqual((five.topViewController as? RouteController)?.key, navigation.observation()["route"] as? String, "Five shows the route the router is on")
    }

    /// More tapped again before its Back, left by a tap on Home, has ended:
    /// the router follows Five, and the Back reaches it.
    func testMoreTappedAgainBeforeItsBackLeftByAnotherTabEndsKeepsTheBack() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates More's pop and the tab changes as in an app only in a scene's window") }
        let (session, tabs, more, root, back) = try deeperUnderMore("more-pop-back-again")
        let navigation = session.presenter.navigation
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        more.popViewController(animated: true)
        waitForAnimationsToStart(more)
        tapTab(tabs, 0)
        XCTAssertNotNil(more.transitionCoordinator, "More's pop still runs")
        tapTab(tabs, of: more)
        waitFor("More shows Five's root, and the router follows", facts: { "router tab \(navigation.routerTab), route \(String(describing: navigation.observation()["route"])), More holds \(more.viewControllers.count), in transition \(navigation.inTransition)" }) {
            navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil && !navigation.inTransition
                && navigation.observation()["route"] as? String == root
        }
        XCTAssertEqual(journal(session, "press left the router"), 0, "no tab's press was declined")
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "UIKit's Back under More is the router's")
    }

    /// More tapped again above Five's root, then Home tapped before More's pop
    /// to its list ends: UIKit's pop drops Deeper and puts Five's root back in
    /// Five's own controller as it ends, off screen (LLP 1075.003 §9.16). The
    /// Back reaches the router as the pop begins, and Five's row then shows
    /// Five's root.
    func testMoreTappedAgainThenAnotherTabKeepsTheBack() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates More's pop and the tab changes as in an app only in a scene's window") }
        let (session, tabs, more, _, back) = try deeperUnderMore("more-list-left")
        let navigation = session.presenter.navigation
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        // More's tap as UIKit performs it (measured): `shouldSelect(more)`, `didSelect` of the tab it shows, then the pop to the list
        XCTAssertEqual(tabs.delegate?.tabBarController?(tabs, shouldSelect: more), true)
        tabs.delegate?.tabBarController?(tabs, didSelect: try XCTUnwrap(tabs.selectedViewController))
        more.popToRootViewController(animated: true)
        waitForAnimationsToStart(more)
        tapTab(tabs, 0)
        waitFor("the router follows Home, and More's pop has ended") { navigation.routerTab == 0 && more.transitionCoordinator == nil && !navigation.inTransition }
        tapTab(tabs, of: more)
        waitFor("More shows its list") { tabs.selectedViewController === more && more.stackRoutes.isEmpty && more.transitionCoordinator == nil }
        pickRow(tabs, 4)
        waitFor("Five's row shows the route the router is on, with nothing pending", facts: { "router tab \(navigation.routerTab), route \(String(describing: navigation.observation()["route"])), More holds \(more.viewControllers.count), in transition \(navigation.inTransition)" }) {
            navigation.routerTab == 4 && more.transitionCoordinator == nil && !navigation.inTransition
                && (more.topViewController as? RouteController)?.key == navigation.observation()["route"] as? String
        }
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "UIKit's pop to More's list is the router's Back")
        XCTAssertNil(navigation.restDifference, "UIKit and the router agree")
    }

    /// A tab not built yet, tapped after another tab's Back under More ended
    /// off screen: its controller is filled within the tap, with its bar as
    /// its first route says, as at any other time.
    func testATabNotBuiltTappedAfterMoresBackEndedOffScreenIsFilledWithinTheTap() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit ends More's pop off screen as in an app only in a scene's window") }
        let (session, tabs, _, _) = try leaveMoresBackForHome("more-kept-unbuilt")
        let navigation = session.presenter.navigation
        let barless = try XCTUnwrap(tabs.tabControllers?[1] as? UINavigationController, "Barless's panel draws its stack")
        XCTAssertTrue(barless.viewControllers.isEmpty)
        tapTab(tabs, 1)
        XCTAssertTrue(tabs.selectedViewController === barless)
        XCTAssertEqual(navigation.routerTab, 1)
        XCTAssertEqual((barless.viewControllers.first as? RouteController)?.node.props["testId"], "route-barless")
        XCTAssertTrue(barless.isNavigationBarHidden, "Barless's first route shows no bar")
    }

    /// A control in a sheet over Home moving the router to Five, whose Back
    /// under More ended off screen: the sync that closes the sheet runs with
    /// no further batch, and More shows the route the router is on.
    func testASheetsControlMovingTheRouterToATabWhoseBackEndedOffScreenClosesTheSheet() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit presents a sheet, and ends More's pop off screen, as in an app only in a scene's window") }
        let (session, _, more, back) = try leaveMoresBackForHome("more-kept-sheet")
        let navigation = session.presenter.navigation
        // A sheet waits for the first drawn frame, which this window never draws
        if session.firstDrawMs == nil { session.firstDrawMs = ExactEnv.wall() }
        try tapNode(session, "open-sheet")
        waitFor("the sheet is up") { session.presenter.modals.routes.count == 1 && !session.presenter.modals.inTransition && !navigation.inTransition }
        try tapNode(session, "sheet-five")
        waitFor("the sheet is gone, with nothing pending", facts: { "router tab \(navigation.routerTab), sheets \(session.presenter.modals.routes.count), in transition \(navigation.inTransition), changing \(navigation.changing), More holds \(more.viewControllers.count)" }) {
            navigation.routerTab == 4 && session.presenter.modals.routes.isEmpty && !session.presenter.modals.inTransition && !navigation.inTransition
        }
        waitForMainQueue()
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "the Back reached the router once")
        XCTAssertEqual((more.topViewController as? RouteController)?.key, navigation.observation()["route"] as? String, "More shows the route the router is on")
    }

    /// A tab on the bar tapped once Home's own animated Back has started:
    /// UIKit ends the pop off screen (LLP 1075.003 §9.16). The Back reaches
    /// the router as the pop begins, and no sync writes Detail back into
    /// Home's controller, so Home shows its root when it is tapped again.
    func testABarTabTappedDuringATabsOwnBackKeepsTheBack() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates Home's pop and the tab change as in an app only in a scene's window") }
        try tapABarTabDuringHomesBack("home-pop-leave", started: true)
    }

    /// As above, with Barless tapped in the turn Home's Back is asked for:
    /// UIKit drops the pop's transition, and its coordinator stays until Home
    /// shows again (§9.16).
    func testABarTabTappedInTheTurnOfATabsOwnBackKeepsTheBack() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates Home's pop and the tab change as in an app only in a scene's window") }
        try tapABarTabDuringHomesBack("home-pop-leave-turn", started: false)
    }

    private func tapABarTabDuringHomesBack(_ label: String, started: Bool) throws {
        let (session, tabs, _) = try moreFixture(label)
        let navigation = session.presenter.navigation
        let home = try XCTUnwrap(tabs.viewControllers?[0] as? UINavigationController)
        let root = try XCTUnwrap(navigation.observation()["route"] as? String)
        try tapNode(session, "open-detail")
        waitFor("Detail pushed in Home") { home.viewControllers.count == 2 && home.transitionCoordinator == nil && !navigation.changing }
        let back = try node(session, "back-detail").id
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        home.popViewController(animated: true)
        if started { waitForAnimationsToStart(home) }
        tapTab(tabs, 1)
        waitFor("the router follows Barless", facts: { "router tab \(navigation.routerTab), in transition \(navigation.inTransition)" }) {
            navigation.routerTab == 1 && !navigation.inTransition
        }
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "Home's Back reached the router")
        tapTab(tabs, 0)
        waitFor("Home shows its root, and the router follows", facts: { "router tab \(navigation.routerTab), route \(String(describing: navigation.observation()["route"])), Home holds \(home.viewControllers.count), in transition \(navigation.inTransition)" }) {
            navigation.routerTab == 0 && home.transitionCoordinator == nil && !navigation.inTransition && navigation.observation()["route"] as? String == root
        }
        waitForMainQueue()
        XCTAssertEqual(home.viewControllers.count, 1, "no sync wrote Detail back into Home")
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "and the Back reached it once")
        XCTAssertNil(navigation.restDifference, "UIKit and the router agree")
        XCTAssertTrue(navigation.retiringRoots.isEmpty, "Detail's views went once no window showed it")
    }

    /// UIKit's Back reaches the router as it begins, so the app's batch
    /// removes Detail while UIKit still animates it out: Detail keeps its
    /// pixels for the rest of the pop.
    func testARouteRemovedDuringUIKitsBackKeepsItsPixels() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop as in an app only in a scene's window") }
        let (session, tabs, _) = try moreFixture("home-back-pixels")
        let navigation = session.presenter.navigation
        let home = try XCTUnwrap(tabs.viewControllers?[0] as? UINavigationController)
        let root = try XCTUnwrap(navigation.observation()["route"] as? String)
        try tapNode(session, "open-detail")
        waitFor("Detail pushed in Home") { home.viewControllers.count == 2 && home.transitionCoordinator == nil && !navigation.changing }
        let detail = try XCTUnwrap(home.topViewController as? RouteController)
        let before = Set(detail.view.subviews.map(ObjectIdentifier.init))
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        home.popViewController(animated: true)
        waitFor("the Back reaches the router") { navigation.observation()["route"] as? String == root }
        XCTAssertNotNil(home.transitionCoordinator, "while UIKit's pop still runs")
        XCTAssertNil(session.presenter.views[detail.node.id], "the app's batch removed Detail")
        XCTAssertTrue(detail.view.subviews.contains { !before.contains(ObjectIdentifier($0)) }, "Detail's pixels stay for the pop: in a window \(detail.view.window != nil), subviews \(detail.view.subviews.map { String(describing: type(of: $0)) }), before \(before.count), node's superview \(String(describing: detail.node.superview.map { type(of: $0) }))")
    }

    /// A tab on the bar selected in code, as an app's own code may select
    /// one: UIKit sends no tab delegate call (LLP 1075.003 §9.16), and the
    /// router follows as the tab's route appears.
    func testATabSelectedInCodeIsFollowedAsItsRouteAppears() throws {
        let (session, tabs, _) = try moreFixture("more-code-select")
        let navigation = session.presenter.navigation
        tapTab(tabs, 2)
        waitFor("the router follows Three") { navigation.routerTab == 2 && !navigation.inTransition }
        tapTab(tabs, 0)
        waitFor("the router follows Home") { navigation.routerTab == 0 && !navigation.inTransition }
        tabs.selectedViewController = tabs.viewControllers?[2]
        waitFor("the router follows Three, selected in code", facts: { "router tab \(navigation.routerTab)" }) { navigation.routerTab == 2 && !navigation.inTransition }
        XCTAssertNil(navigation.restDifference, "UIKit and the router agree")
        // The router moved by the app's own select, then Three selected in code again
        session.presenter.press(try node(session, "tab-home").id)
        waitFor("the router and UIKit on Home") { navigation.routerTab == 0 && tabs.selectedViewController === tabs.viewControllers?[0] && !navigation.inTransition }
        tabs.selectedViewController = tabs.viewControllers?[2]
        waitFor("the router follows Three again", facts: { "router tab \(navigation.routerTab)" }) { navigation.routerTab == 2 && !navigation.inTransition }
    }

    /// The development check journals, before a batch, where UIKit and the
    /// router disagree at rest (LLP 1075.003 §3.5), once.
    func testTheDevelopmentCheckJournalsWhereUIKitAndTheRouterDisagree() throws {
        let (session, _, _) = try moreFixture("more-rest-check")
        let navigation = session.presenter.navigation
        XCTAssertNil(navigation.restDifference)
        navigation.routerTab = 2
        navigation.checkOwned()
        navigation.checkOwned()
        XCTAssertEqual(journal(session, "navigation: at rest, UIKit shows tab 0 and the router is on tab 2"), 1)
        navigation.routerTab = 0
    }

    /// The development check reads each sheet's stack as well as the tabs':
    /// a controller in a sheet's navigation controller that the router does
    /// not hold is a difference.
    func testTheDevelopmentCheckReadsASheetsStack() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit presents a sheet only from a window a scene holds") }
        let (session, _, _) = try moreFixture("more-sheet-rest")
        let navigation = session.presenter.navigation
        // A sheet waits for the first drawn frame, which this window never draws
        if session.firstDrawMs == nil { session.firstDrawMs = ExactEnv.wall() }
        try tapNode(session, "open-sheet")
        waitFor("the sheet is up") { session.presenter.modals.routes.count == 1 && !session.presenter.modals.inTransition && !navigation.inTransition }
        waitForMainQueue()
        XCTAssertNil(navigation.restDifference, "UIKit and the router agree")
        let sheet = try XCTUnwrap(navigation.presentedNavigations.first)
        let key = try XCTUnwrap((sheet.topViewController as? RouteController)?.key)
        sheet.pushViewController(UIViewController(), animated: false)
        XCTAssertEqual(navigation.restDifference, "sheet 1 holds [\(key), ?] in UIKit and [\(key)] in the router")
        sheet.popViewController(animated: false)
    }

    /// A row of More's list picked, and a batch's sync before More shows it:
    /// the sync lays the tab container out, which starts More's push, and
    /// More's `willShow` presses the row's tab during that sync. The press's
    /// batch builds Six, and Six's route is projected once the sync ends.
    func testAPressMoresWillShowMakesDuringASyncIsProjected() throws {
        let (session, tabs, more) = try moreFixture("more-press-in-sync")
        let navigation = session.presenter.navigation
        tapTab(tabs, of: more)
        waitFor("More shows its list") { tabs.selectedViewController === more && more.transitionCoordinator == nil }
        waitForMainQueue()
        pickRow(tabs, 5)
        navigation.sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        waitFor("the router follows Six, whose route More shows", facts: { "router tab \(navigation.routerTab), Six shows \(String(describing: (tabs.viewControllers?[5] as? PlainTab)?.route?.node.props["testId"])), in transition \(navigation.inTransition)" }) {
            navigation.routerTab == 5 && (tabs.viewControllers?[5] as? PlainTab)?.route?.node.props["testId"] == "route-six" && !navigation.inTransition
        }
        XCTAssertNil(navigation.restDifference, "UIKit and the router agree")
    }

    /// More showing Five's Deeper, its animated Back started, and Home tapped;
    /// the Back reached the router as it began, and once it has ended off
    /// screen More holds Five's root.
    private func leaveMoresBackForHome(_ label: String) throws -> (ExactSession, UITabBarController, UINavigationController, UInt32) {
        let (session, tabs, more, _, back) = try deeperUnderMore(label)
        let navigation = session.presenter.navigation
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        more.popViewController(animated: true)
        waitForAnimationsToStart(more)
        tapTab(tabs, 0)
        waitFor("the router follows Home, and More's Back has ended") { navigation.routerTab == 0 && more.transitionCoordinator == nil && !navigation.inTransition }
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "the Back reached the router")
        XCTAssertEqual(more.stackRoutes.count, 1, "More holds Five's root")
        return (session, tabs, more, back)
    }

    /// More showing Five's Deeper over Five's root, picked from More's list:
    /// the router's route on Five's root, and Deeper's Back control.
    private func deeperUnderMore(_ label: String) throws -> (ExactSession, UITabBarController, UINavigationController, String, UInt32) {
        let (session, tabs, more) = try moreFixture(label)
        let navigation = session.presenter.navigation
        tapTab(tabs, of: tabs.moreNavigationController)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        let root = try XCTUnwrap(navigation.observation()["route"] as? String)
        try tapNode(session, "open-deeper")
        waitFor("Deeper pushed onto More") { more.stackRoutes.count == 2 && more.transitionCoordinator == nil && !navigation.changing }
        return (session, tabs, more, root, try node(session, "back-deeper").id)
    }

    /// The router's own selection of Five while another tab shows, with
    /// Full, which hides the tab bar, over Five's root in More: Exact selects
    /// More, as a tap on More does, and More keeps Full. Five's own
    /// controller selected in code would pop More to its list and drop Full
    /// (LLP 1075.003 §9.16).
    func testTheRoutersSelectionOfATabWhoseRoutesMoreHoldsKeepsThem() throws {
        let (session, tabs, more) = try moreFixture("more-select-held")
        let navigation = session.presenter.navigation
        tapTab(tabs, of: tabs.moreNavigationController)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        try tapNode(session, "open-full")
        waitFor("Full pushed onto More") { more.stackRoutes.count == 2 && more.transitionCoordinator == nil && !navigation.changing }
        let full = try XCTUnwrap(more.topViewController as? RouteController)
        XCTAssertTrue(full.hidesBottomBarWhenPushed, "Full hides the tab bar")
        // The app's own selection of Home, then of Five again
        session.presenter.press(try node(session, "tab-home").id)
        waitFor("the router and UIKit on Home") { navigation.routerTab == 0 && tabs.selectedViewController === tabs.tabControllers?[0] && !navigation.inTransition }
        session.presenter.press(try node(session, "tab-five").id)
        waitFor("the router on Five") { navigation.routerTab == 4 && !navigation.inTransition }
        waitForMainQueue()
        XCTAssertTrue(more.topViewController === full, "More keeps Full: More holds \(more.stackRoutes.map { ($0 as? RouteController)?.key ?? "?" })")
        XCTAssertEqual(navigation.selectedKey, full.key, "and the router stays on it")
        XCTAssertNil(navigation.restDifference, "UIKit and the router agree")
    }

    /// An app that sets More's delegate itself takes More's calls from Exact:
    /// a pop of More reaches the router in the turn after the next sync, and
    /// holds no sync after that.
    func testAnAppsOwnDelegateOnMoreHoldsNoSync() throws {
        let (session, _, more, _, back) = try deeperUnderMore("more-app-delegate")
        let navigation = session.presenter.navigation
        XCTAssertNotNil(more.view.window, "More is on screen")
        let app = BarWatcher()
        more.delegate = app
        more.popViewController(animated: false)
        navigation.sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        waitFor("no sync waits for a call Exact cannot hear", facts: { "changing \(navigation.changing)" }) { !navigation.changing }
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "the pop reached the router")
    }

    /// An interactive pop toward More's list, cancelled while a sync waits
    /// for it: navigation follows More again and the sync runs, with no batch
    /// after it.
    func testACancelledSwipeTowardMoresListKeepsFollowingMore() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "an interactive pop of More runs as an app's only in a scene's window") }
        let (session, tabs, more) = try moreFixture("more-swipe-cancel")
        let navigation = session.presenter.navigation
        tapTab(tabs, of: tabs.moreNavigationController)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil && !navigation.changing }
        let root = try XCTUnwrap(navigation.observation()["route"] as? String)
        let pop = InteractivePop()
        navigation.setAppDelegate(more, pop)
        pop.interactive = true
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        more.popViewController(animated: true)
        waitForAnimationsToStart(more)
        pop.interaction.update(0.4)
        navigation.sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        pop.interaction.cancel()
        waitFor("the cancelled swipe leaves More on Five's root") { more.transitionCoordinator == nil && more.stackRoutes.count == 1 }
        waitFor("navigation follows More again, with nothing owed", facts: { "follows More \(navigation.primaryNavigation === more), changing \(navigation.changing), in transition \(navigation.inTransition), stack \(String(describing: navigation.observation()["stack"]))" }) {
            navigation.primaryNavigation === more && !navigation.inTransition
        }
        XCTAssertEqual(navigation.observation()["route"] as? String, root)
    }

    /// A tap on the selected tab above its root is UIKit's pop, which the
    /// router follows as a Back, and then the tab's own press, as every tap
    /// on a tab presses it.
    func testAReselectAboveTheRootPressesTheTabAfterUIKitsPop() throws {
        let (session, tabs, _) = try moreFixture("more-reselect-home")
        let navigation = session.presenter.navigation
        let home = try XCTUnwrap(tabs.tabControllers?[0] as? UINavigationController)
        let root = try XCTUnwrap(navigation.observation()["route"] as? String)
        try tapNode(session, "open-detail")
        waitFor("Detail pushed in Home") { home.viewControllers.count == 2 && home.transitionCoordinator == nil && !navigation.changing }
        let tab = try node(session, "tab-home").id, back = try node(session, "back-detail").id
        tapTab(tabs, 0)
        waitFor("Home popped to its root") { home.viewControllers.count == 1 && home.transitionCoordinator == nil && !navigation.changing }
        waitForMainQueue()
        XCTAssertEqual(navigation.observation()["route"] as? String, root)
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "UIKit's pop is the router's Back")
        XCTAssertEqual(journal(session, "press view \(tab) (pick)"), 1, "and the tab is pressed")
    }

    /// The agent's `back` over a tab's root or a plain tab that More shows is
    /// More's Back to its list, which UIKit's bar shows there: the router
    /// keeps the tab.
    func testTheAgentsBackUnderMoreGoesToMoresList() throws {
        let (session, tabs, more) = try moreFixture("more-agent-back")
        let navigation = session.presenter.navigation
        let agent = Agent(session: session)
        tapTab(tabs, of: tabs.moreNavigationController)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        let five = try XCTUnwrap(navigation.observation()["route"] as? String)
        XCTAssertNotNil(navigation.observation()["nativeBack"] as? String, "UIKit's Back to the list shows")
        var reply = agent.back()
        XCTAssertNil(reply["error"] ?? reply["refused"], "\(reply)")
        waitFor("More shows its list") { more.stackRoutes.isEmpty && more.transitionCoordinator == nil && !navigation.changing }
        waitForMainQueue()
        XCTAssertEqual(navigation.routerTab, 4, "the list is no tab: the router keeps Five")
        XCTAssertEqual(navigation.observation()["route"] as? String, five, "and its root")
        XCTAssertTrue(navigation.observation()["nativeBack"] is NSNull, "the list has no Back")

        let six = try XCTUnwrap(tabs.tabControllers?[5] as? PlainTab)
        pickRow(tabs, 5)
        waitFor("More shows Six") { navigation.routerTab == 5 && more.topViewController === six && more.transitionCoordinator == nil }
        reply = agent.back()
        XCTAssertNil(reply["error"] ?? reply["refused"], "\(reply)")
        waitFor("More shows its list again") { more.topViewController !== six && more.transitionCoordinator == nil }
        XCTAssertEqual(navigation.routerTab, 5)
    }

    /// A navigation controller the app's own container holds, as one its
    /// `tabContainer` puts a plain tab in, is the app's, not a More to adopt:
    /// its delegate stays the app's, and Exact keeps its More.
    func testAnAppsNavigationControllerTakingAPlainTabIsNotMore() throws {
        let (session, _, more) = try moreFixture("more-offer")
        let navigation = session.presenter.navigation
        XCTAssertTrue(navigation.more === more)
        let apps = UINavigationController(), container = UITabBarController()
        container.viewControllers = [apps]
        navigation.moved(into: apps)
        XCTAssertNil(apps.delegate, "not given Exact's delegate")
        XCTAssertTrue(navigation.more === more, "Exact keeps UIKit's More")
        XCTAssertTrue(more.delegate is NavigationDelegateProxy, "and keeps its delegate")
    }

    /// The agent's tap on an authored tab the bar has no item for is the
    /// router's selection, which UIKit shows under More, as selecting the
    /// tab in code does (measured); the reply names More's list.
    func testTheAgentsTapOnATabUnderMoreShowsItThere() throws {
        let (session, tabs, more) = try moreFixture("more-agent")
        let navigation = session.presenter.navigation
        let reply = Agent(session: session).tap(["id": Int(try node(session, "tab-five").id)])
        XCTAssertNil(reply["error"], "\(reply)")
        XCTAssertEqual(reply["native"] as? String, "more-list-row")
        XCTAssertEqual(navigation.routerTab, 4)
        XCTAssertTrue(tabs.selectedViewController === tabs.tabControllers?[4])
        waitFor("the router follows Five, whose root More holds") { session.presenter.navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        XCTAssertEqual((more.stackRoutes.first as? RouteController)?.node.props["testId"], "route-five")
        XCTAssertTrue(navigation.primaryNavigation === more)
    }

    /// The agent's tap on the tab the router is on while More shows its list
    /// (More tapped again above that tab's root): UIKit selects the tab, as a
    /// person's pick of its row does, and More shows its root again.
    func testTheAgentsTapOnTheRoutersTabUnderMoreShowsItFromMoresList() throws {
        let (session, tabs, more) = try moreFixture("more-agent-list")
        let navigation = session.presenter.navigation
        tapTab(tabs, of: more)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        XCTAssertEqual(tabs.delegate?.tabBarController?(tabs, shouldSelect: more), true)
        tabs.delegate?.tabBarController?(tabs, didSelect: try XCTUnwrap(tabs.selectedViewController))
        more.popToRootViewController(animated: false)
        waitFor("More shows its list, and the router stays on Five") { tabs.selectedViewController === more && more.stackRoutes.isEmpty && !navigation.inTransition }
        XCTAssertEqual(navigation.routerTab, 4)
        let reply = Agent(session: session).tap(["id": Int(try node(session, "tab-five").id)])
        XCTAssertNil(reply["error"], "\(reply)")
        waitFor("More shows Five's root again", facts: { "selected \(String(describing: tabs.selectedViewController)), More holds \(more.viewControllers.count), router tab \(navigation.routerTab)" }) {
            tabs.selectedViewController === tabs.viewControllers?[4] && more.stackRoutes.count == 1 && more.transitionCoordinator == nil && !navigation.inTransition
        }
        XCTAssertEqual(navigation.routerTab, 4)
        XCTAssertEqual((more.stackRoutes.first as? RouteController)?.node.props["testId"], "route-five")
        XCTAssertNil(navigation.restDifference, "UIKit and the router agree")
    }

    /// An app that sets a tab's delegate itself, past `navigation.delegate`,
    /// leaves the tab Exact's stack: only a navigation controller Exact's tab
    /// container holds with no record of Exact's is More, so the tab's next
    /// push adopts nothing and More stays UIKit's.
    func testATabWhoseDelegateTheAppReplacedIsNotTakenForMore() throws {
        let (session, tabs, more) = try moreFixture("more-replaced-delegate")
        let navigation = session.presenter.navigation
        let home = try XCTUnwrap(tabs.tabControllers?[0] as? UINavigationController)
        let stranger = BarWatcher()
        home.delegate = stranger
        try tapNode(session, "open-detail")
        waitFor("Detail pushed onto Home") { home.viewControllers.count == 2 && home.transitionCoordinator == nil && !navigation.changing }
        XCTAssertTrue(navigation.more === more, "More is still UIKit's")
        XCTAssertTrue(delegatesToExact(navigation, more), "with Exact's delegate")
        XCTAssertNotNil(navigation.stacks[ObjectIdentifier(home)], "and Home keeps its record")
    }

    /// The agent's tap on the selected tab above its root runs a person's
    /// tap: UIKit pops the tab to its root, where Back goes, the router
    /// follows with one Back, and the tab is pressed after that Back, at
    /// `didSelect`.
    func testTheAgentsTapOnTheSelectedTabAboveItsRootPopsItAndTheRouterFollows() throws {
        let (session, tabs, _) = try moreFixture("more-agent-reselect")
        let navigation = session.presenter.navigation
        let home = try XCTUnwrap(tabs.tabControllers?[0] as? UINavigationController)
        try tapNode(session, "open-detail")
        waitFor("Detail pushed onto Home") { home.viewControllers.count == 2 && home.transitionCoordinator == nil && !navigation.changing }
        let tab = try node(session, "tab-home")
        let reply = try XCTUnwrap(navigation.activateChrome(tab))
        XCTAssertNil(reply["error"], "\(reply)")
        XCTAssertEqual(reply["native"] as? String, "tab-bar-item")
        waitFor("UIKit pops Home to its root, and the router follows") {
            home.viewControllers.count == 1 && (navigation.observation()["stack"] as? [String])?.count == 1 && home.transitionCoordinator == nil && !navigation.changing
        }
        waitForMainQueue()
        XCTAssertEqual(navigation.observation()["route"] as? String, (home.viewControllers.first as? RouteController)?.key)
        XCTAssertEqual(journal(session, "press view \(tab.id) (pick)"), 1, "the tab pressed once")
    }

    /// Whether `nav` holds the delegate of Exact's record for it.
    private func delegatesToExact(_ navigation: NavigationHost, _ nav: UINavigationController) -> Bool {
        navigation.stacks[ObjectIdentifier(nav)].map { nav.delegate === $0.proxy } ?? false
    }

    /// The more-tabs fixture, skipped where UIKit's bar shows every tab.
    private func moreFixture(_ label: String) throws -> (ExactSession, UITabBarController, UINavigationController) {
        let session = try fixture(label)
        let tabs = try XCTUnwrap(session.presenter.navigation.tabController)
        guard tabs.tabBar.items?.count == 5 else { throw XCTSkip("UIKit's bar shows every tab at this width: there is no More") }
        return (session, tabs, tabs.moreNavigationController)
    }

    /// A row of More's list picked: UIKit selects the tab's controller as
    /// `selectedViewController` set in code does, with no tab delegate call;
    /// More's `willShow` and `didShow` follow in a later turn (measured).
    private func pickRow(_ tabs: UITabBarController, _ index: Int) {
        tabs.selectedViewController = tabs.tabControllers![index]
    }

    /// `wholeScreen`: the window is the device's screen, as an app's is,
    /// which decides how many items UIKit's bar holds (the iPhone Duo's side
    /// bar holds every tab at a regular height); else iPhone 17's size.
    private func fixture(_ label: String, wholeScreen: Bool = false) throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env["EXACT_MORE_TABS_PLAN"], "build.mjs --test --ios compiles the more-tabs fixture")))
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        // In the test host's scene when it has one: UIKit presents only from a window a scene holds.
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let iPhone17 = CGRect(x: 0, y: 0, width: 402, height: 874)
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: iPhone17)
        let frame = wholeScreen ? (window.windowScene?.coordinateSpace.bounds ?? iPhone17) : iPhone17
        window.frame = frame
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: frame.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        waitFor("the tab container and the selected stack are in the window") {
            let navigation = session.presenter.navigation
            return navigation.tabController?.viewIfLoaded?.window != nil
                && navigation.tabController?.selectedViewController?.viewIfLoaded?.window != nil
                && !Agent(session: session).nativeInFlight()
        }
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    /// How many times the session's journal holds `line`.
    private func journal(_ session: ExactSession, _ line: String) -> Int {
        session.agent(#"{"op":"logs","since":0}"#).components(separatedBy: line).count - 1
    }
}

/// An app's navigation delegate that notes whether the bar is hidden as a
/// route is about to show.
private final class BarWatcher: NSObject, UINavigationControllerDelegate {
    var hiddenAtWillShow: [Bool] = []
    func navigationController(_ nav: UINavigationController, willShow controller: UIViewController, animated: Bool) {
        if controller is RouteController { hiddenAtWillShow.append(nav.isNavigationBarHidden) }
    }
}
#endif
