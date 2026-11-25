#if os(iOS)
import UIKit
import XCTest
import ExactCore
@testable import ExactKit
@testable import ExactNavigation
@testable import ExactGroupedLists

/// LLP 1127 on UIKit: a stack whose root route asks (on an iPad outside a
/// tab, also one whose list rows push) is a `.doubleColumn` split, the root
/// route in the list's navigation controller (N0) and the routes above it in
/// the detail's (N1). Collapsed (a compact width), UIKit nests N1 in N0, so
/// Back from the detail is N0's pop of N1, with N1 still holding its routes.
/// The size class is set on the window or the split, as an unfolding display
/// sets it, so each test runs the same on any simulator:
/// bun host/apple/build.mjs --test --ios --only SplitIOSTests
final class SplitIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?
    /// The view booted out of the window (`boot(mounted: false)`), which nothing else holds.
    private var unmounted: ExactView?

    override func tearDown() {
        session?.destroy()
        window?.isHidden = true
        window = nil
        session = nil
        unmounted = nil
        super.tearDown()
    }

    /// `fixture` booted in a window of `size` at the size class `width`
    /// (compact collapses the split, regular expands it).
    private func boot(_ fixture: String = "SPLIT_BACK", size: CGSize = CGSize(width: 402, height: 874),
                      width: UIUserInterfaceSizeClass = .compact, mounted: Bool = true) throws -> ExactSession {
        if ExactEnv.authoredChrome { throw XCTSkip("under the agent's chrome the stack is not split") }
        let plan = try fixturePlan(fixture)
        ExactGroupedLists.install() // split-back's list is a grouped list
        let session = ExactApp.shared.makeSession(label: "split-\(UUID().uuidString)")
        self.session = session
        let view = ExactView(session: session), host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(origin: .zero, size: size))
        window.frame = CGRect(origin: .zero, size: size)
        window.traitOverrides.horizontalSizeClass = width
        window.rootViewController = host
        window.makeKeyAndVisible()
        if mounted { host.view.addSubview(view) } else { unmounted = view }
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        return session
    }

    private func until(_ what: String, _ done: () -> Bool) { waitFor(what, cap: 20, done) }

    /// The split, once installed and at rest.
    private func split(_ session: ExactSession) throws -> SplitIOS {
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        until("a list whose rows push is a split") { navigation.split != nil && !agent.nativeInFlight() }
        return try XCTUnwrap(navigation.split)
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, testId)
    }

    private func has(_ session: ExactSession, _ testId: String) -> Bool {
        session.presenter.views.values.contains { $0.props["testId"] == testId }
    }

    private func tap(_ session: ExactSession, _ testId: String) throws {
        let target = try node(session, testId)
        if let lists = session.presenter.groupedLists as? GroupedListHost, lists.draws(target.id) {
            until("the row \(testId) is in the window") {
                if case let .view(cell, _)? = lists.shown(target) { return cell.window != nil }
                return false
            }
        }
        XCTAssertNil(Agent(session: session).tap(["id": Int(target.id)])["error"], "tap \(testId)")
    }

    /// Whether the runner reads `testId`'s node as inactive (the agent's tree).
    private func inactive(_ session: ExactSession, _ testId: String) throws -> Bool {
        let id = try node(session, testId).id
        let tree = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"tree"}"#).utf8)) as? [String: Any])
        let rows = try XCTUnwrap(tree["nodes"] as? [[String: Any]])
        return rows.first { ($0["id"] as? NSNumber)?.uint32Value == id }?["inactive"] as? Bool == true
    }

    /// A stack whose root route asks splits on any device, its primary custom, with no list (LLP 1127 D1, D19).
    func testAnExplicitStackCanUseACustomPrimary() throws {
        let session = try boot("SPLIT_CUSTOM", size: CGSize(width: 1100, height: 800), width: .regular)
        let nav = session.presenter.navigationHost
        let columns = try split(session)
        XCTAssertNil(columns.listNode)
        try tap(session, "custom-open")
        until("the custom primary opened its detail") { columns.detailRoutes.count == 1 }
        let detail = columns.detail.topViewController
        try tap(session, "customize")
        until("the primary becomes a directory") { columns.listNode != nil }
        try tap(session, "customize")
        until("the custom primary keeps its detail") { columns.listNode == nil && columns.detail.topViewController === detail }
        let platform = UISplitViewController(style: .doubleColumn)
        XCTAssertEqual(columns.controller.preferredPrimaryColumnWidth, platform.preferredPrimaryColumnWidth)
        XCTAssertEqual(columns.controller.preferredPrimaryColumnWidthFraction, platform.preferredPrimaryColumnWidthFraction)
        XCTAssertTrue(nav.split === columns)
        XCTAssertNotNil(try node(session, "custom-open"))
    }

    func testAnExplicitIPadTabKeepsItsColumnsAndDetailAcrossSelectionAndCollapse() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit presents a selected tab in its window scene") }
        let session = try boot("SPLIT_TABS", size: CGSize(width: 1100, height: 800), width: .regular)
        let nav = session.presenter.navigationHost, agent = Agent(session: session)
        let tabs = try XCTUnwrap(nav.tabController)
        guard UIDevice.current.userInterfaceIdiom == .pad else {
            XCTAssertNil(nav.split)
            XCTAssertTrue(tabs.selectedViewController is UINavigationController)
            return
        }
        let columns = try split(session)
        until("Library has two columns in the selected tab") { !columns.controller.isCollapsed && columns.controller.view.window === window && tabs.selectedViewController === nav.tabStack(0) }
        try tap(session, "library-North")
        let row = try node(session, "library-North").id
        until("North is selected") { columns.sidebarRow == row && columns.shown.count == 2 && !agent.nativeInFlight() }
        try tap(session, "open-notes")
        until("Notes has a detail Back") { columns.detailRoutes.count == 2 && !columns.detail.isNavigationBarHidden && columns.detail.topViewController?.navigationItem.hidesBackButton == false && !agent.nativeInFlight() }
        let notes = columns.detail.topViewController
        let library = try XCTUnwrap(nav.tabStack(0)), browse = try XCTUnwrap(nav.tabStack(1))
        try tap(session, "tab-browse")
        let second = try split(session)
        XCTAssertFalse(second === columns)
        XCTAssertEqual(Set([columns.list, columns.detail, second.list, second.detail].map(ObjectIdentifier.init)).count, 4)
        try tap(session, "browse-South")
        let south = try node(session, "browse-South").id
        until("Browse shows South") { second.sidebarRow == south && second.detailRoutes.count == 1 && !agent.nativeInFlight() }
        let subject = second.detail.topViewController
        func switchTabs() throws {
            try tap(session, "tab-library")
            until("Library kept North's notes") {
                nav.split === columns && tabs.selectedViewController === library && columns.detail.topViewController === notes
                    && columns.sidebarRow == row && !agent.nativeInFlight()
            }
            try tap(session, "tab-browse")
            until("Browse kept South") {
                nav.split === second && tabs.selectedViewController === browse && second.detail.topViewController === subject
                    && second.sidebarRow == south && !agent.nativeInFlight()
            }
        }
        try switchTabs()
        window?.traitOverrides.horizontalSizeClass = .compact
        until("compact Browse shows South") { second.controller.isCollapsed && second.nested && !agent.nativeInFlight() }
        try switchTabs()
        XCTAssertTrue(second.controller.isCollapsed && second.nested && second.front.topViewController === subject)
        try tap(session, "tab-library")
        until("compact Library shows Notes on top") {
            columns.controller.isCollapsed && columns.nested && columns.front.topViewController === notes && !agent.nativeInFlight()
        }
        try tap(session, "tab-browse")
        window?.traitOverrides.horizontalSizeClass = .regular
        until("Browse expands") { !second.controller.isCollapsed && !agent.nativeInFlight() }
        try switchTabs()
        try tap(session, "tab-profile")
        until("Profile is a stack") { nav.split == nil && nav.primaryNavigation != nil }
        try tap(session, "tab-library")
        until("Library expanded with Notes") { !columns.controller.isCollapsed && nav.split === columns && !agent.nativeInFlight() }
        XCTAssertNil(agent.back()["error"])
        until("Back returns to North") { columns.detailRoutes.count == 1 && columns.sidebarRow == row && !agent.nativeInFlight() }
    }

    func testUnopenedTabChoosesItsOwnStackAndAnInactiveSplitKeepsItsRow() throws {
        guard UIDevice.current.userInterfaceIdiom == .pad else { throw XCTSkip("the tab split is iPad only") }
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit presents a selected tab in its window scene") }
        let session = try boot("SPLIT_TABS", size: CGSize(width: 1032, height: 1376), width: .regular)
        let nav = session.presenter.navigationHost, agent = Agent(session: session)
        func tapControl(_ id: String) throws {
            let target = try node(session, id)
            until("the selected tab shows \(id)") { target.window != nil && !agent.nativeInFlight() }
            try tap(session, id)
        }
        let library = try split(session)
        let row = try node(session, "Second").id
        try tap(session, "Second")
        until("Browse shows and Library has the filed book") { nav.routerTab == 1 && library.shown.count == 2 && !agent.nativeInFlight() }
        try tap(session, "tab-library")
        let lists = try XCTUnwrap(session.presenter.groupedLists as? GroupedListHost)
        let list = try XCTUnwrap(lists.lists[try node(session, "list-library").id])
        until("the row that opened the shared book is selected") {
            list.collection.indexPathsForSelectedItems?.compactMap { list.source.itemIdentifier(for: $0) } == [row]
        }
        try tap(session, "tab-browse")
        try tapControl("toggle-library")
        until("an empty panel holds no retired route") { library.shown.isEmpty && library.detailRoutes.isEmpty && !nav.tabsHeld }
        try tapControl("toggle-library")
        until("the same columns hold the routes again") { library.shown.count == 2 }
        try tap(session, "tab-profile")
        try tapControl("open-account")
        until("Profile pushed Account") { nav.primaryNavigation?.viewControllers.count == 2 && !agent.nativeInFlight() }
        try tap(session, "tab-profile")
        until("reselecting Profile pops Account") { nav.primaryNavigation?.viewControllers.count == 1 && !agent.nativeInFlight() }
        try tap(session, "tab-library")
        try tapControl("open-notes")
        until("Library has two detail routes") { library.detailRoutes.count == 2 && !agent.nativeInFlight() }
        try tap(session, "tab-library")
        until("reselecting Library clears the detail") { library.shown.count == 1 && library.detailRoutes.isEmpty && !agent.nativeInFlight() }
    }

    /// LLP 1127 D2: the split is the stack's container from the first frame.
    /// A barless root whose list paints no cells would otherwise boot in a
    /// holder of its route alone (NavigationBootIOS.swift), and a stack whose
    /// rows push would boot pushed, each changed to the split a batch later.
    func testTheSplitIsInstalledBeforeTheFirstDraw() throws {
        guard #available(iOS 27.0, *), let scene = hostedWindowScene() else { throw missing("scene", "a deferred boot owner needs the hosted app's scene, on iOS 27") }
        if ExactEnv.authoredChrome { throw XCTSkip("under the agent's chrome the stack is not split") }
        waitFor("the test scene is active") { scene.activationState == .foregroundActive }
        let session = ExactApp.shared.makeSession(label: "split-boot-\(UUID().uuidString)")
        self.session = session
        session.hatchesOwnWindow = true
        let view = ExactView(session: session), host = PortraitController()
        let window = UIWindow(windowScene: scene)
        window.frame = window.screen.bounds
        window.traitOverrides.horizontalSizeClass = .compact
        window.rootViewController = host
        window.makeKeyAndVisible()
        self.window = window
        host.view.addSubview(view)
        view.frame = host.view.bounds
        XCTAssertNil(session.boot(plan: try fixturePlan("SPLIT_BOOT"), size: window.bounds.size).error)
        view.layoutIfNeeded()
        let navigation = session.presenter.navigationHost
        XCTAssertNil(session.firstDrawMs, "nothing drawn yet")
        XCTAssertNil(navigation.bootOwner, "no holder of the first route alone")
        let split = try XCTUnwrap(navigation.split, "the split holds the stack")
        XCTAssertTrue(navigation.primaryOwner === split.controller && split.controller.view.window === window)
        XCTAssertTrue(split.controller.isCollapsed, "collapsed at a compact width")
        XCTAssertEqual(split.shown.count, 1)
        XCTAssertEqual(split.shown.first?.key, navigation.selectedKey, "the list's route shows")
    }

    func testACollapsedSplitsBackSwipeIsTheStackWithARouteBeneathItsTop() throws {
        let session = try boot()
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        let split = try split(session)
        XCTAssertTrue(split.controller.isCollapsed, "a compact width collapses it")
        func begins(_ nav: UINavigationController) throws -> Bool {
            var pop = nav.interactivePopGestureRecognizer
            if #available(iOS 26.0, *) { pop = nav.interactiveContentPopGestureRecognizer ?? pop }
            return navigation.popMayBegin(try XCTUnwrap(pop), from: CGPoint(x: 100, y: 400), in: nav.view, velocity: CGPoint(x: 600, y: 20))
        }
        try tap(session, "row-a")
        until("the book is pushed") { split.shown.count == 2 && split.nested && !agent.nativeInFlight() }
        XCTAssertTrue(try begins(split.list), "at the detail's root the list's stack pops, back to the list")
        XCTAssertFalse(try begins(split.detail), "the detail's own has nothing beneath its root")

        try tap(session, "notes-a")
        until("the notes are pushed") { split.shown.count == 3 && !agent.nativeInFlight() }
        XCTAssertTrue(try begins(split.detail), "the detail's stack pops its top")
        XCTAssertFalse(try begins(split.list), "the list's would take the whole detail stack")
    }

    /// An iPad splits a stack whose list rows push, asked or not, and any other
    /// device only one whose root route asks (LLP 1127 D1, D19), whichever
    /// device runs the test.
    func testAnIPadSplitsAListWhoseRowsPushAndAnIPhoneOnlyWhereTheRouteAsks() {
        let found = StackSplit(list: 1, width: nil, rows: [:]), asked = StackSplit(asked: true, list: nil, width: nil, rows: [:])
        XCTAssertNil(NavigationHost.admitted(found, idiom: .phone), "an iPhone pushes a stack whose route does not ask")
        XCTAssertEqual(NavigationHost.admitted(found, idiom: .pad), found, "an iPad splits the list the rule finds")
        for idiom in [UIUserInterfaceIdiom.phone, .pad] {
            XCTAssertEqual(NavigationHost.admitted(asked, idiom: idiom), asked, "a route that asks splits on either")
            XCTAssertNil(NavigationHost.admitted(nil, idiom: idiom), "a stack the runner splits on no host pushes")
        }
    }

    /// A root route that says nothing splits by the rule on an iPad outside a
    /// tab, read again with each batch: a list whose rows come later makes a
    /// split, its route kept; any other device keeps the stack pushed (LLP 1127
    /// D19, D22).
    func testAnUnaskedStackSplitsOnAnIPadWhenItsRowsComeAndStaysPushedElsewhere() throws {
        let session = try boot("SPLIT_LATE")
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        until("the stack is up") { navigation.primaryNavigation != nil && !agent.nativeInFlight() }
        XCTAssertNil(navigation.split, "no row pushes yet")
        let root = try XCTUnwrap(navigation.primaryNavigation?.viewControllers.first as? RouteController)
        try tap(session, "fill")
        until("the rows came") { self.has(session, "row-a") && !agent.nativeInFlight() }
        if UIDevice.current.userInterfaceIdiom == .pad {
            until("the rows came: a split") { navigation.split != nil && !agent.nativeInFlight() }
            XCTAssertTrue(navigation.split?.list.viewControllers.first === root, "the same route, in the list's column")
        } else {
            XCTAssertNil(navigation.split, "a device other than an iPad splits only where the route asks")
        }
    }

    /// Whether a stack splits is what its root route says (LLP 1127 D2), read
    /// again with each batch, its routes kept: a root that asks is a split
    /// before its list has a row, stays the same split as the rows come, and
    /// is a stack again once it says `navigationSplit="none"`.
    func testAStackThatAsksSplitsBeforeItsRowsComeAndPushesWhenItsRootSaysNone() throws {
        let session = try boot("SPLIT_CHANGE")
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        until("the split is up, with no row yet") { navigation.split != nil && !agent.nativeInFlight() }
        let split = try XCTUnwrap(navigation.split)
        let root = try XCTUnwrap(split.list.viewControllers.first as? RouteController)
        try tap(session, "fill")
        until("the rows came") { self.has(session, "row-a") && !agent.nativeInFlight() }
        XCTAssertTrue(navigation.split === split, "the rows' coming keeps the split")
        try tap(session, "row-a")
        until("the book is in the detail") { navigation.split?.shown.count == 2 && !agent.nativeInFlight() }
        XCTAssertNil(agent.back()["error"])
        until("back at the list") { navigation.split?.shown.count == 1 && !agent.nativeInFlight() }
        try tap(session, "stop")
        until("the root says none: a stack again") { navigation.split == nil && navigation.primaryNavigation != nil && !agent.nativeInFlight() }
        XCTAssertTrue(navigation.primaryNavigation?.viewControllers.first === root, "the same route, in a stack")
        try tap(session, "row-b")
        until("a row pushes") { navigation.primaryNavigation?.viewControllers.count == 2 && !agent.nativeInFlight() }
    }

    /// Inside a callback the runtime waits on, no runtime call runs, and a
    /// sync there keeps the split by the rule the last batch carried
    /// (`Batch.split`, LLP 1072 T2), asking the runner nothing.
    func testASyncInsideARunnerCallbackKeepsTheSplitItHeard() throws {
        let session = try boot()
        let navigation = session.presenter.navigationHost
        let split = try split(session)
        var owed = true
        Owner.shared.sync {
            Owner.shared.callMain {
                navigation.resync()
                owed = navigation.splitAskOwed
            }
        }
        XCTAssertTrue(navigation.split === split, "the split stays")
        XCTAssertFalse(owed, "the rule the last batch carried answers")
    }

    /// A runner that cannot say at install whether the stack splits (a call
    /// inside a callback it waits on) leaves the stack pushed and is asked
    /// again on the main queue's next turn, which makes the split.
    func testAnInstallTheRunnerCannotAnswerAsksAgain() throws {
        let session = try boot(mounted: false)
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        let view = try XCTUnwrap(session.view), host = try XCTUnwrap(window?.rootViewController)
        XCTAssertNil(navigation.primaryOwner, "out of a window, the stack is not installed")
        var installed: (split: Bool, pushed: Bool, owed: Bool)?
        Owner.shared.sync {
            Owner.shared.callMain {
                // The view enters its window inside a callback the runner waits on, and its mount's sync installs the stack
                host.view.addSubview(view)
                navigation.mounted()
                installed = (navigation.split != nil, navigation.primaryNavigation != nil, navigation.splitAskOwed)
            }
        }
        XCTAssertEqual(installed?.split, false)
        XCTAssertEqual(installed?.pushed, true, "pushed while the runner cannot say")
        XCTAssertEqual(installed?.owed, true, "and asked again on the main queue's next turn")
        until("asked again: a split") { navigation.split != nil && !navigation.splitAskOwed && !agent.nativeInFlight() }
    }

    /// Nested in N0, the detail's top shows its header's items in N0's bar,
    /// where a popover or a menu anchors on the item (`NavigationHost.barItem`).
    func testACollapsedDetailsBarItemIsInTheListColumnsBar() throws {
        let session = try boot()
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        let split = try split(session)
        try tap(session, "row-photo")
        until("the photo is pushed") { split.shown.count == 2 && split.nested && !agent.nativeInFlight() }
        guard case .success(let (bar, frame))? = navigation.barItem(try node(session, "bar-photo-q")) else { return XCTFail("the photo's bar item") }
        XCTAssertTrue(bar === split.list.navigationBar, "N0's bar, which shows the photo's item")
        XCTAssertFalse(frame.isEmpty)
        let item = try node(session, "bar-photo-q")
        guard case .success(let placed)? = agent.barItem(item) else { return XCTFail("the agent finds the detail's button") }
        XCTAssertEqual(placed.bar.convert(placed.item, to: bar), frame)
    }

    /// LLP 1127 D8: beside the detail the list is live with it; while the
    /// list overlays the detail, only the list is; with the detail alone, or
    /// collapsed, only the detail.
    func testOnlyTheListIsLiveWhileItOverlaysTheDetail() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        let split = try split(session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        try tap(session, "row-a")
        until("the book is in the detail") { split.shown.count == 2 && !agent.nativeInFlight() }
        let cases: [(UIUserInterfaceSizeClass, UISplitViewController.DisplayMode, UISplitViewController.SplitBehavior, Bool, Bool)] = [
            (.regular, .oneBesideSecondary, .tile, false, false),
            (.regular, .oneOverSecondary, .overlay, false, true),
            (.regular, .secondaryOnly, .tile, true, false),
            (.compact, .automatic, .automatic, true, false),
        ]
        for (width, mode, behavior, list, detail) in cases {
            split.controller.traitOverrides.horizontalSizeClass = width
            split.controller.preferredSplitBehavior = behavior
            split.controller.preferredDisplayMode = mode
            let shows = width == .compact ? "collapsed" : "mode \(mode.rawValue)"
            until("the split is \(shows)") {
                split.controller.isCollapsed == (width == .compact) && (width == .compact || split.controller.displayMode == mode) && !agent.nativeInFlight()
            }
            until("\(shows): the list \(list ? "in" : "")active, the detail \(detail ? "in" : "")active") {
                (try? inactive(session, "row-a")) == list && (try? inactive(session, "notes-a")) == detail
            }
            XCTAssertEqual(navigation.isInactiveRoute(containing: try node(session, "row-a")), list, "the host's list, \(shows)")
            XCTAssertEqual(navigation.isInactiveRoute(containing: try node(session, "notes-a")), detail, "the host's detail, \(shows)")
            XCTAssertNil(navigation.restDifference, "UIKit and the router agree, \(shows)")
        }
    }

    /// LLP 1127 D8: with the detail alone and nothing selected, nothing of
    /// the split is live: the list, the stack's top, shows under the empty
    /// detail and is told so.
    func testTheListUnderTheEmptyDetailAloneIsNotLive() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        let split = try split(session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        split.controller.preferredSplitBehavior = .tile
        split.controller.preferredDisplayMode = .secondaryOnly
        until("the empty detail alone") { split.controller.displayMode == .secondaryOnly && !agent.nativeInFlight() }
        let root = try XCTUnwrap(split.list.stackRoutes.first as? RouteController)
        XCTAssertEqual(navigation.routesShown[root.node.id], .under)
        until("the runner reads the list as not live") { (try? inactive(session, "row-a")) == true }
        XCTAssertEqual(session.presenter.owners.toldShown[root.node.id], .under)
    }

    /// LLP 1127 D6: `show(_:)` reveals a new detail under a list shown over
    /// the detail, a `replace` that keeps its route included, shows the list
    /// over the detail alone when the stack goes back to its root, and is
    /// called for no other batch.
    func testTheSplitShowsAColumnForANewDetailAndTheRootOnly() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let agent = Agent(session: session)
        let split = try split(session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        try tap(session, "row-a")
        until("book a is the detail") { has(session, "book-a") && !agent.nativeInFlight() }
        split.controller.preferredSplitBehavior = .overlay
        split.controller.preferredDisplayMode = .oneOverSecondary
        until("the list over book a") { split.controller.displayMode == .oneOverSecondary && !agent.nativeInFlight() }
        try tap(session, "row-count")
        until("a row that opens nothing has run") { !agent.nativeInFlight() }
        XCTAssertEqual(split.controller.displayMode, .oneOverSecondary, "a batch that changes no detail shows no column")
        try tap(session, "row-b")
        until("book b replaces book a and shows") { has(session, "book-b") && split.controller.displayMode == .secondaryOnly && !agent.nativeInFlight() }
        XCTAssertNil(agent.back()["error"], "Back from the detail's base clears it (LLP 1127 D4)")
        until("back at the root, the list over the empty detail") {
            split.shown.count == 1 && split.controller.displayMode == .oneOverSecondary && !agent.nativeInFlight()
        }
    }

    /// A detail requested while UIKit moves the columns is revealed when
    /// the move settles, even though the split already recorded the request.
    func testAStackShownDuringASplitMoveRevealsItsDetailOnSettlement() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let split = try split(session), navigation = session.presenter.navigationHost
        showListOverDetail(split)
        let root = try XCTUnwrap(navigation.container)
        let list = try XCTUnwrap(split.shown.first)
        session.presenter.onCovers = nil

        _ = split.splitViewController(split.controller, topColumnForCollapsingToProposedTopColumn: .primary)
        applyDetail(in: session.presenter, root: root, list: list)
        XCTAssertTrue(split.moving)
        XCTAssertEqual(navigation.selectedStack.count, 2)
        XCTAssertFalse(split.show(navigation.selectedStack, animated: false))
        XCTAssertTrue(split.detailRoutes.isEmpty)
        XCTAssertEqual(split.controller.displayMode, .oneOverSecondary)

        // UIKit finishes with the split still expanded, as on a cancelled collapse
        split.splitViewControllerDidExpand(split.controller)
        until("the held detail is installed and revealed") {
            split.detailRoutes.last?.key == "requested-detail" && split.controller.displayMode == .secondaryOnly
        }
    }

    /// Neither column can finish the projection while its setter refuses.
    /// Both recover, and the newer numbered batch credits its row even when
    /// a lower numbered sync is the last one requested in the same write.
    func testRefusedSplitColumnWritesRetryAndKeepTheGreatestSequence() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let split = try split(session), navigation = session.presenter.navigationHost
        showListOverDetail(split)
        let root = try XCTUnwrap(navigation.container)
        let list = try XCTUnwrap(split.shown.first)
        let newerRow = try node(session, "row-a").id, olderRow = try node(session, "row-b").id
        let obstruction = UIViewController()
        let listDelegate = split.list.delegate, detailDelegate = split.detail.delegate
        split.list.delegate = nil
        split.detail.delegate = nil
        defer { split.list.delegate = listDelegate; split.detail.delegate = detailDelegate }
        split.list.setViewControllers([list, obstruction], animated: false)
        XCTAssertTrue(split.list.topViewController === obstruction)
        session.presenter.onCovers = nil
        navigation.lastTransition = "completed"
        var refuseList = true, refuseDetail = true
        var listRefusals = 0, detailRefusals = 0
        try withNavigationStackWrites(shouldRefuse: { nav, routes in
            if nav === split.list, refuseList {
                listRefusals += 1
                return true
            }
            guard nav === split.detail, refuseDetail, (routes.last as? RouteController)?.key == "requested-detail" else { return false }
            if detailRefusals == 0 {
                XCTAssertTrue(navigation.syncing)
                split.presses = [.init(row: newerRow, after: 69, through: 70), .init(row: olderRow, after: 19, through: 20)]
                for sequence: UInt64 in [70, 20, 0] {
                    var batch = wireBatch([])
                    batch.sequence = sequence
                    navigation.sync(batch)
                }
            }
            detailRefusals += 1
            return true
        }) {
            applyDetail(in: session.presenter, root: root, list: list)
            XCTAssertGreaterThan(listRefusals, 0)
            XCTAssertEqual(detailRefusals, 0, "a refused list write blocks the detail")
            XCTAssertTrue(navigation.stackWritesOwed)
            XCTAssertEqual(navigation.lastTransition, "completed")

            refuseList = false
            until("the list lands and UIKit refuses the detail") { detailRefusals > 0 }
            XCTAssertTrue(split.list.topViewController === list)
            XCTAssertTrue(split.detailRoutes.isEmpty)
            XCTAssertEqual(split.controller.displayMode, .oneOverSecondary)
            XCTAssertEqual(navigation.lastTransition, "completed")
            XCTAssertTrue(navigation.stackWritesOwed)
            XCTAssertEqual(split.presses.map(\.landed), [true, false])

            refuseDetail = false
            until("the detail lands and is revealed") {
                split.detailRoutes.last?.key == "requested-detail" && split.controller.displayMode == .secondaryOnly
            }
            XCTAssertEqual(navigation.lastTransition, "idle")
            XCTAssertFalse(navigation.stackWritesOwed)
        }
    }

    private func showListOverDetail(_ split: SplitIOS) {
        split.controller.preferredSplitBehavior = .overlay
        split.controller.preferredDisplayMode = .oneOverSecondary
        until("the expanded list overlays the empty detail") {
            !split.controller.isCollapsed && split.controller.displayMode == .oneOverSecondary && !split.inTransition
        }
        XCTAssertTrue(split.detailRoutes.isEmpty)
    }

    private func applyDetail(in presenter: Presenter, root: NodeView, list: RouteController) {
        let detail: UInt32 = 900_001
        presenter.apply(wireBatch([
            ["op": "create", "id": detail, "kind": "column", "props": ["navigationKey": "requested-detail"]],
            ["op": "children", "id": root.id, "ids": [list.node.id, detail]],
            ["op": "props", "id": root.id, "set": ["navigationKey": "requested-detail"]]
        ]))
    }

    /// LLP 1127 D7: N1's routes count as pushed, its first included: a base
    /// with no header keeps UIKit's bar, a column whose `navigation` hatch
    /// turned its bar off has none, and neither does one whose top says
    /// `navigationBar="none"`. Collapsed, the route on top decides N0's bar.
    func testTheDetailsColumnShowsItsBarAsAPushedRouteDoes() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        let split = try split(session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        try tap(session, "row-page")
        until("the page is the detail") { split.shown.count == 2 && !agent.nativeInFlight() }
        XCTAssertFalse(split.detail.isNavigationBarHidden, "the headerless base keeps UIKit's bar")
        let column = try XCTUnwrap(navigation.stacks[ObjectIdentifier(split.detail)])
        column.showsBar = false
        navigation.resync()
        until("the hatch's choice applied") { !agent.nativeInFlight() }
        XCTAssertTrue(split.detail.isNavigationBarHidden, "a column whose hatch turned its bar off has none")
        column.showsBar = true
        navigation.resync()
        until("the hatch's bar back") { !split.detail.isNavigationBarHidden && !agent.nativeInFlight() }
        try tap(session, "row-bare")
        until("the bare page pushed over the page") { split.shown.count == 3 && !agent.nativeInFlight() }
        XCTAssertTrue(split.detail.isNavigationBarHidden, "the bare page on top has no bar")
        window?.traitOverrides.horizontalSizeClass = .compact
        until("collapsed on the bare page") { split.controller.isCollapsed && split.nested && !agent.nativeInFlight() }
        XCTAssertTrue(split.list.isNavigationBarHidden, "collapsed, the bare page on top hides N0's bar")
        split.detail.popViewController(animated: false)
        until("UIKit's Back to the page") { split.shown.count == 2 && !agent.nativeInFlight() }
        XCTAssertFalse(split.list.isNavigationBarHidden, "collapsed, the headerless page shows UIKit's bar with its Back")
    }

    /// Collapsed, a route pushed in the detail's column shows its title in
    /// the list's bar, whatever the detail's own bar was: a pressable title's
    /// menu stands there (LLP 1021 §5.1), after a bare page hid the detail's.
    func testAPressableTitleInTheCollapsedListsBarHasItsMenu() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        let split = try split(session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        try tap(session, "row-bare")
        until("the bare page, its column's bar hidden") { split.detail.isNavigationBarHidden && !agent.nativeInFlight() }
        window?.traitOverrides.horizontalSizeClass = .compact
        until("collapsed") { split.controller.isCollapsed && split.nested && !agent.nativeInFlight() }
        split.list.popToRootViewController(animated: false)
        until("the list again") { split.shown.count == 1 && !agent.nativeInFlight() }
        try tap(session, "row-titled")
        let group = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "titled-group" })
        until("the titled page's title in the list's bar") {
            navigation.projectedTitle(for: group).map { NavigationHost.barShowing($0) === split.list.navigationBar } == true
        }
        let title = try XCTUnwrap(navigation.projectedTitle(for: group)), menus = session.presenter.menus.context
        let interaction = try XCTUnwrap(title.interactions.compactMap { $0 as? UIContextMenuInteraction }.first)
        let configuration = try XCTUnwrap(menus.contextMenuInteraction(interaction, configurationForMenuAtLocation: CGPoint(x: title.bounds.midX, y: title.bounds.midY)),
                                          "a long press on the title there opens the group's menu")
        menus.contextMenuInteraction(interaction, willEndFor: configuration, animator: nil)
    }

    /// LLP 1127 D8: a sheet over the expanded split covers both columns
    /// first, the list beside the detail with them, which still show under
    /// it; a full-screen cover hides them.
    func testAPresentationOverTheSplitCoversItsColumns() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "a sheet is presented only in a scene's window") }
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        let split = try split(session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        try tap(session, "row-a")
        until("the book is in the detail") { split.shown.count == 2 && !agent.nativeInFlight() }
        XCTAssertEqual(split.showing.beside.map(\.key), [split.shown.first?.key], "the list beside the detail")
        func live(_ what: String, list: Bool, detail: Bool) throws {
            until("\(what): the list \(list ? "" : "not ")live, the detail \(detail ? "" : "not ")live") {
                (try? inactive(session, "row-a")) == !list && (try? inactive(session, "notes-a")) == !detail
            }
            XCTAssertEqual(navigation.isInactiveRoute(containing: try node(session, "row-a")), !list, "the host's list, \(what)")
            XCTAssertEqual(navigation.isInactiveRoute(containing: try node(session, "notes-a")), !detail, "the host's detail, \(what)")
        }
        try tap(session, "share-a")
        until("the sheet is up") { session.presenter.modals.routes.count == 1 && !agent.nativeInFlight() }
        try live("under a sheet", list: false, detail: false)
        let list = try XCTUnwrap(split.list.viewControllers.first as? RouteController)
        XCTAssertTrue(navigation.splitShown.under.contains { $0 === list }, "the list shows under the sheet")
        XCTAssertEqual(session.presenter.owners.toldShown[list.node.id], .under, "and the runner is told so")
        XCTAssertNil(agent.back()["error"])
        until("the sheet is gone") { session.presenter.modals.routes.isEmpty && !agent.nativeInFlight() }
        try live("the sheet gone", list: true, detail: true)
        XCTAssertEqual(session.presenter.owners.toldShown[list.node.id], .beside)
        try tap(session, "cover-a")
        until("the cover is up") { session.presenter.modals.routes.count == 1 && !agent.nativeInFlight() }
        try live("under a full-screen cover", list: false, detail: false)
        XCTAssertTrue(navigation.splitShown.beside.isEmpty && navigation.splitShown.under.isEmpty, "nothing of the split shows under the cover")
        XCTAssertNil(agent.back()["error"])
        until("the cover is gone") { session.presenter.modals.routes.isEmpty && !agent.nativeInFlight() }
        try live("the cover gone", list: true, detail: true)
    }

    /// A row chosen beside a book replaces it, as the list-detail pattern's
    /// rows do: the detail's column holds the new book alone, the list stays
    /// beside it, and collapsed, Back from it shows the list.
    func testARowChosenBesideTheDetailReplacesIt() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        let split = try split(session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        try tap(session, "row-a")
        until("book a is the detail") { split.shown.count == 2 && has(session, "book-a") && !agent.nativeInFlight() }
        try tap(session, "row-b")
        until("book b replaces it") { split.shown.count == 2 && has(session, "book-b") && !has(session, "book-a") && !agent.nativeInFlight() }
        XCTAssertEqual(split.detailRoutes.count, 1, "the detail's column holds the new book alone")
        XCTAssertFalse(split.showing.beside.isEmpty, "the list stays beside it")
        XCTAssertNil(navigation.restDifference)
        window?.traitOverrides.horizontalSizeClass = .compact
        until("collapsed on book b") { split.controller.isCollapsed && split.nested && has(session, "book-b") && !agent.nativeInFlight() }
        XCTAssertNil(agent.back()["error"])
        until("back at the list") { split.shown.count == 1 && !has(session, "book-b") && !agent.nativeInFlight() }
    }

    /// LLP 1127 D7: beside its detail the list is UIKit's sidebar list, with
    /// no disclosure chevron, and the row whose detail shows stays selected,
    /// again as the split expands; collapsed, it is the authored list.
    func testBesideItsDetailTheListIsUIKitsSidebarList() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let agent = Agent(session: session)
        let split = try split(session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        let books = try node(session, "books").id, rowA = try node(session, "row-a").id
        let host = try XCTUnwrap(session.presenter.groupedLists as? GroupedListHost)
        until("the list is laid out") { host.lists[books] != nil }
        let list = try XCTUnwrap(host.lists[books])
        XCTAssertTrue(list.sidebar, "beside the detail, UIKit's sidebar list")
        let cell = { list.source.indexPath(for: rowA).flatMap { list.collection.cellForItem(at: $0) as? UICollectionViewListCell } }
        XCTAssertEqual(try XCTUnwrap(cell()).accessories.count, 0, "a sidebar's row draws no chevron")
        let selected = { (list.collection.indexPathsForSelectedItems ?? []).compactMap { list.source.itemIdentifier(for: $0) } }
        XCTAssertEqual(selected(), [], "no row selected before one is chosen")
        try tap(session, "row-a")
        until("book a is the detail, its row selected") { has(session, "book-a") && selected() == [rowA] && !agent.nativeInFlight() }
        window?.traitOverrides.horizontalSizeClass = .compact
        until("collapsed, the authored list") { split.controller.isCollapsed && !list.sidebar && !agent.nativeInFlight() }
        window?.traitOverrides.horizontalSizeClass = .regular
        until("expanded again, its row selected again") { !split.controller.isCollapsed && list.sidebar && selected() == [rowA] && !agent.nativeInFlight() }
        XCTAssertEqual(try XCTUnwrap(cell()).accessories.count, 0)
        XCTAssertNil(agent.back()["error"], "Back from the detail's base clears it (LLP 1127 D4)")
        until("nothing selected at the list") { split.shown.count == 1 && selected() == [] && !agent.nativeInFlight() }
    }

    /// LLP 1127 D3, D7: beside its detail, the sidebar's selected row is the
    /// base's, whatever a row's press did or a push within the detail added:
    /// UIKit's selection on a row that changed no detail goes back to it once
    /// the press's batch is applied, a book pushed within the detail keeps
    /// the base's row, a runner that cannot say leaves the selection, and the
    /// list's route appearing again keeps it.
    func testTheSidebarKeepsTheBasesRowSelected() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let agent = Agent(session: session)
        let split = try split(session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        let books = try node(session, "books").id, rowA = try node(session, "row-a").id
        let host = try XCTUnwrap(session.presenter.groupedLists as? GroupedListHost)
        until("the list is laid out") { host.lists[books] != nil }
        let list = try XCTUnwrap(host.lists[books])
        let selected = { (list.collection.indexPathsForSelectedItems ?? []).compactMap { list.source.itemIdentifier(for: $0) } }
        try tap(session, "row-a")
        until("book a is the detail, its row selected") { has(session, "book-a") && selected() == [rowA] && !agent.nativeInFlight() }
        try tap(session, "row-count")
        until("a row that opens nothing leaves book a's row selected") { selected() == [rowA] && !agent.nativeInFlight() }
        try tap(session, "push-b-a")
        until("book b is pushed within the detail") { split.shown.count == 3 && !agent.nativeInFlight() }
        XCTAssertEqual(selected(), [rowA], "the base's row, not book b's")
        Owner.shared.sync { Owner.shared.callMain { list.selectDetailRow() } }
        XCTAssertEqual(selected(), [rowA], "a runner that cannot say leaves the selection")
        host.appearing(try XCTUnwrap(split.list.stackRoutes.first as? RouteController).node, alongside: nil)
        XCTAssertEqual(selected(), [rowA], "the list's route appearing beside the detail keeps its row")
    }

    /// A detail no row brought, one the app opened at its location as a
    /// launch there does, selects the row the app marks current
    /// (`aria-current`); a base a press row brought selects that row (LLP
    /// 1127 D3), as the Mac's split and the web's do.
    func testTheSidebarSelectsTheCurrentRowOrThePressedRowForADetailNoLinkBrought() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let agent = Agent(session: session)
        let split = try split(session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        let books = try node(session, "books").id, rowPage = try node(session, "row-page").id
        let rowPress = try node(session, "row-press").id, rowReplace = try node(session, "row-replace").id
        let host = try XCTUnwrap(session.presenter.groupedLists as? GroupedListHost)
        until("the list is laid out") { host.lists[books] != nil }
        let list = try XCTUnwrap(host.lists[books])
        let selected = { (list.collection.indexPathsForSelectedItems ?? []).compactMap { list.source.itemIdentifier(for: $0) } }
        XCTAssertTrue(session.navigate("/page/n"), "the app opens a page no row links to")
        until("the page is the detail, the current row selected") {
            has(session, "page-page") && split.detailRoutes.count == 1 && selected() == [rowPage] && !agent.nativeInFlight()
        }
        XCTAssertNil(agent.back()["error"])
        until("back at the list, no row selected") { split.shown.count == 1 && selected().isEmpty && !agent.nativeInFlight() }
        try tap(session, "row-press")
        until("book c is the detail") { has(session, "book-c") && split.detailRoutes.count == 1 && !agent.nativeInFlight() }
        until("the row whose press brought it selected") { selected() == [rowPress] }
        try tap(session, "row-replace")
        until("book d takes book c's route") { has(session, "book-d") && !has(session, "book-c") && split.detailRoutes.count == 1 && !agent.nativeInFlight() }
        until("the row whose press moved it selected") { selected() == [rowReplace] }
    }

    /// LLP 1127 D3: a row's press whose batch waits behind another (a fill
    /// in flight) and lands after the press returned still credits the
    /// detail it brings, and until it lands the sidebar keeps the row the
    /// person pressed. A second press meanwhile that changes nothing (Count)
    /// takes nothing from it.
    func testAPressWhoseBatchLandsLaterIsCreditedWhenItLands() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular), agent = Agent(session: session)
        let split = try split(session), navigation = session.presenter.navigationHost
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        let books = try node(session, "books").id, rowPress = try node(session, "row-press").id, rowCount = try node(session, "row-count").id
        let host = try XCTUnwrap(session.presenter.groupedLists as? GroupedListHost)
        until("the list is a sidebar") { host.lists[books] != nil }
        let list = try XCTUnwrap(host.lists[books])
        let selected = { (list.collection.indexPathsForSelectedItems ?? []).compactMap { list.source.itemIdentifier(for: $0) } }
        // Each press's batch is made and held, as one behind a fill is, until after the presses return
        var held: [Batch] = []
        for row in [rowPress, rowCount] {
            navigation.listPress(row, in: list.owner) {
                let batch = session.runtime.press(row, now: session.now())
                held.append(batch)
                session.pending.append(PendingBatch(batch: batch))
            }
        }
        XCTAssertEqual(split.presses.count, 2, "the presses wait for their batches")
        XCTAssertNil(navigation.sidebarSelection(list.owner), "the sidebar keeps the pressed row meanwhile")
        session.pending.removeAll()
        held.forEach(session.apply)
        until("book c is the detail") { has(session, "book-c") && split.detailRoutes.count == 1 && !agent.nativeInFlight() }
        until("the presses ended") { split.presses.isEmpty }
        until("the row whose press brought it selected, not Count's") { selected() == [rowPress] }
    }

    /// LLP 1127 D3: a row pressed while a push within the detail is still
    /// animating, whose batch's sync UIKit's transition holds, is credited
    /// all the same: its press shows book c alone, a new base.
    func testARowPressedDuringATransitionIsCredited() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular), agent = Agent(session: session)
        let split = try split(session), navigation = session.presenter.navigationHost
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        let books = try node(session, "books").id, rowShow = try node(session, "row-show").id
        let host = try XCTUnwrap(session.presenter.groupedLists as? GroupedListHost)
        until("the list is a sidebar") { host.lists[books] != nil }
        let list = try XCTUnwrap(host.lists[books])
        try tap(session, "row-a")
        until("book a is the detail") { has(session, "book-a") && split.detailRoutes.count == 1 && !agent.nativeInFlight() }
        session.presenter.press(try node(session, "push-b-a").id)
        XCTAssertTrue(navigation.changing, "book b is being pushed within the detail")
        navigation.listPress(rowShow, in: list.owner) { session.presenter.press(rowShow) }
        XCTAssertTrue(navigation.pendingSync, "the press's sync waits for the push")
        until("book c alone is the detail") { has(session, "book-c") && split.detailRoutes.count == 1 && !agent.nativeInFlight() }
        until("the row whose press brought it selected") {
            (list.collection.indexPathsForSelectedItems ?? []).compactMap { list.source.itemIdentifier(for: $0) } == [rowShow]
        }
    }

    /// LLP 1127 D3: a row's press credits the detail it brings while the
    /// split is collapsed too, so beside the detail once the split expands,
    /// that row is the selected one.
    func testARowPressedWhileCollapsedIsSelectedOnceTheSplitExpands() throws {
        let session = try boot(), agent = Agent(session: session)
        let split = try split(session)
        try tap(session, "row-press")
        until("book c is pushed") { has(session, "book-c") && split.nested && split.detailRoutes.count == 1 && !agent.nativeInFlight() }
        window?.traitOverrides.horizontalSizeClass = .regular
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        let books = try node(session, "books").id, rowPress = try node(session, "row-press").id
        let host = try XCTUnwrap(session.presenter.groupedLists as? GroupedListHost)
        until("the list is a sidebar") { host.lists[books] != nil }
        let list = try XCTUnwrap(host.lists[books])
        until("the row whose press brought book c selected") {
            (list.collection.indexPathsForSelectedItems ?? []).compactMap { list.source.itemIdentifier(for: $0) } == [rowPress]
        }
    }

    /// LLP 1127 D11: the agent's `state window.split` names the split as
    /// UIKit shows it: collapsed on a book, then expanded with the list over it.
    func testTheAgentsStateNamesTheSplit() throws {
        let session = try boot()
        let agent = Agent(session: session)
        let split = try split(session)
        try tap(session, "row-a")
        until("the book is pushed") { split.shown.count == 2 && split.nested && !agent.nativeInFlight() }
        func state() throws -> [String: Any] { try XCTUnwrap((agent.stateSections()["window"] as? [String: Any])?["split"] as? [String: Any]) }
        let root = try XCTUnwrap(split.shown.first?.key), book = try XCTUnwrap(split.detailRoutes.first?.key)
        var said = try state()
        XCTAssertEqual(said["installed"] as? Bool, true)
        XCTAssertEqual(said["collapsed"] as? Bool, true)
        XCTAssertEqual(said["nested"] as? Bool, true, "UIKit holds N1 in N0")
        XCTAssertEqual(said["list"] as? [String], [root])
        XCTAssertEqual(said["detail"] as? [String], [book])
        XCTAssertEqual(said["empty"] as? Bool, false)
        XCTAssertEqual(said["row"] as? Int, Int(try node(session, "row-a").id), "the row whose detail shows")
        XCTAssertEqual(said["beside"] as? [String], [], "collapsed, the routes are one stack")
        window?.traitOverrides.horizontalSizeClass = .regular
        split.controller.preferredSplitBehavior = .overlay
        split.controller.preferredDisplayMode = .oneOverSecondary
        until("the list over the book") { !split.controller.isCollapsed && split.controller.displayMode == .oneOverSecondary && !agent.nativeInFlight() }
        said = try state()
        XCTAssertEqual(said["collapsed"] as? Bool, false)
        XCTAssertEqual(said["nested"] as? Bool, false)
        XCTAssertEqual(said["displayMode"] as? String, "oneOverSecondary")
        XCTAssertEqual(said["beside"] as? [String], [root], "the list is live")
        XCTAssertEqual(said["under"] as? [String], [book], "the book under it is not")
    }

    /// UIKit's stacks and the router agree at rest, whatever the split shows
    /// (`restDifference`, which journals a difference): N0 nests N1 while
    /// collapsed, and holds the root alone while expanded.
    func testUIKitAndTheRouterAgreeAtRest() throws {
        let session = try boot()
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        let split = try split(session)
        func agree(_ what: String) {
            until("\(what), at rest") { !agent.nativeInFlight() }
            XCTAssertNil(navigation.restDifference, what)
            XCTAssertFalse(navigation.syncOwed, "\(what): no sync owed")
        }
        agree("collapsed at the list")
        try tap(session, "row-a")
        until("the book is pushed") { split.shown.count == 2 && split.nested }
        agree("collapsed on the book")
        try tap(session, "notes-a")
        until("the notes are pushed") { split.shown.count == 3 }
        agree("collapsed on the notes")
        window?.traitOverrides.horizontalSizeClass = .regular
        until("expanded") { !split.controller.isCollapsed }
        agree("expanded on the notes")
        XCTAssertNil(agent.back()["error"])
        until("back at the book") { split.shown.count == 2 && !agent.nativeInFlight() }
        agree("expanded on the book")
        XCTAssertNil(agent.back()["error"], "Back from the detail's base clears it (LLP 1127 D4)")
        until("back at the list") { split.shown.count == 1 }
        agree("expanded at the list")
        window?.traitOverrides.horizontalSizeClass = .compact
        until("collapsed") { split.controller.isCollapsed }
        agree("collapsed at the list again")
    }

    /// LLP 1127 §1, item 2: UIKit takes N1 out of N0 as the split expands
    /// (an iPhone Duo unfolded on a book's notes), and nests it again as it
    /// collapses. Neither is a Back: the book and its notes stay shown, and
    /// Back from them, collapsed, returns to the book and then the list.
    /// The size class changes on the window, as an unfolding display changes
    /// it: the split still reads compact when UIKit moves N1 out.
    func testExpandingAndCollapsingKeepTheDetail() throws {
        let session = try boot()
        let agent = Agent(session: session)
        let split = try split(session)
        XCTAssertTrue(split.controller.isCollapsed, "a compact width collapses it")
        try tap(session, "row-a")
        until("the book is pushed") { split.shown.count == 2 && !agent.nativeInFlight() }
        try tap(session, "notes-a")
        until("its notes are pushed") { split.shown.count == 3 && !agent.nativeInFlight() }
        let window = try XCTUnwrap(self.window)
        window.frame = CGRect(x: 0, y: 0, width: 1100, height: 800)
        window.rootViewController?.view.subviews.first?.frame = window.bounds
        window.traitOverrides.horizontalSizeClass = .regular
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        until("the book and its notes show beside the list") { split.shown.count == 3 && has(session, "notes-a") }
        XCTAssertTrue(try inactive(session, "notes-a"), "expanded, the notes cover the book")
        window.traitOverrides.horizontalSizeClass = .compact
        until("collapsed") { split.controller.isCollapsed && !agent.nativeInFlight() }
        until("the notes are on top") { split.nested && split.shown.count == 3 && has(session, "notes-page") }
        XCTAssertNil(agent.back()["error"])
        until("back at the book") { split.shown.count == 2 && !agent.nativeInFlight() }
        XCTAssertFalse(try inactive(session, "notes-a"), "collapsed, the book is live")
        XCTAssertNil(agent.back()["error"])
        until("back at the list") { split.shown.count == 1 && !has(session, "notes-a") && !agent.nativeInFlight() }
    }

    /// An iPhone Duo unfolding: UIKit asks which display mode to expand to
    /// while the split still reads compact, then takes N1 out of N0, its
    /// `didShow` arriving before the split says it expanded. That is no Back:
    /// the router keeps the book and its notes. The order is UIKit's on the
    /// Duo simulator, replayed here.
    func testTheDetailLeavingTheListAsUIKitExpandsIsNoBack() throws {
        let session = try boot()
        let agent = Agent(session: session)
        let split = try split(session)
        try tap(session, "row-a")
        until("the book is pushed") { split.shown.count == 2 && !agent.nativeInFlight() }
        try tap(session, "notes-a")
        until("its notes are pushed") { split.shown.count == 3 && has(session, "notes-page") && !agent.nativeInFlight() }
        XCTAssertTrue(split.nested)
        XCTAssertEqual(split.controller.traitCollection.horizontalSizeClass, .compact)
        _ = split.splitViewController(split.controller, displayModeForExpandingToProposedDisplayMode: .oneBesideSecondary)
        split.list.setViewControllers(Array(split.list.viewControllers.prefix(1)), animated: false)
        waitForMainQueue("the calls the move made")
        XCTAssertTrue(has(session, "notes-page") && has(session, "notes-a"), "the router keeps the book and its notes")
        XCTAssertTrue(split.moving && agent.nativeInFlight(), "the move is in flight until UIKit says it expanded")
    }

    /// As UIKit collapses the split it puts N1 in N0 while the split still
    /// reads expanded. A sync meanwhile leaves N0's stack to UIKit: reset to
    /// its root, UIKit pushed the detail twice and asserted (an iPhone Duo
    /// folded on a book). The order is UIKit's on the Duo simulator,
    /// replayed here.
    func testASyncWhileUIKitCollapsesLeavesTheListsStackToIt() throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let agent = Agent(session: session)
        let split = try split(session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        try tap(session, "row-a")
        until("the book is in the detail") { split.shown.count == 2 && !agent.nativeInFlight() }
        let stack = split.shown
        _ = split.splitViewController(split.controller, topColumnForCollapsingToProposedTopColumn: .primary)
        let moving = UIViewController()
        split.list.setViewControllers(split.list.viewControllers + [moving], animated: false)
        split.show(stack, animated: false)
        XCTAssertTrue(split.list.viewControllers.last === moving, "N0's stack is UIKit's while it collapses")
    }

    /// The book, collapsed: UIKit's Back from it pops N1 off N0, which the
    /// person may also swipe (`InteractivePop`).
    private func collapsedBook(_ session: ExactSession) throws -> SplitIOS {
        let split = try split(session), agent = Agent(session: session)
        try tap(session, "row-a")
        until("the book is pushed") { split.shown.count == 2 && split.nested && !agent.nativeInFlight() }
        return split
    }

    /// #1079's written-anew case in a collapsed split: while UIKit's Back
    /// pops N1, and the book with it, the app writes the book anew under its
    /// key. UIKit starts the pop it was asked for at the next layout of the
    /// list's stack: here the commit of a transaction around the write, after
    /// the write's press returns.
    func testABackUIKitStartsAfterTheWriteGoesBackThroughTheBookWrittenAnew() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let session = try boot()
        let split = try collapsedBook(session)
        try checkABackThroughTheBookWrittenAnew(session, split.list, inTheWrite: false) { split.shown.count == 1 }
    }

    /// The order the case took in some runs: UIKit starts its pop once the
    /// write has landed and before the write's press returns (there the
    /// write's own transaction was the outermost, and its commit laid the
    /// stack out). The Back still reaches the router after the write, through
    /// the book written anew, and lands before the press returns.
    func testABackUIKitStartsBeforeTheWritesPressReturnsGoesBackThroughTheBookWrittenAnew() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let session = try boot()
        let split = try collapsedBook(session)
        try checkABackThroughTheBookWrittenAnew(session, split.list, inTheWrite: true) { split.shown.count == 1 }
    }

    /// The same steps on the stack that does not split end where the split
    /// ends, in either order.
    func testAPlainStacksBackThroughTheBookWrittenAnewEndsAtTheListInEitherOrder() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        for inTheWrite in [false, true] {
            let session = try boot(), agent = Agent(session: session), navigation = session.presenter.navigationHost
            _ = try split(session)
            try tap(session, "row-plain")
            until("the stack pushes") { navigation.split == nil && navigation.primaryNavigation != nil && !agent.nativeInFlight() }
            let stack = try XCTUnwrap(navigation.primaryNavigation)
            try tap(session, "row-a")
            until("the book is pushed") { stack.viewControllers.count == 2 && !agent.nativeInFlight() }
            try checkABackThroughTheBookWrittenAnew(session, stack, inTheWrite: inTheWrite) { stack.viewControllers.count == 1 }
            session.destroy()
            window?.isHidden = true
        }
    }

    /// UIKit's Back from book a (`stack`'s pop of its top), during which the
    /// app writes the book anew (Renew). `inTheWrite`: UIKit starts the pop
    /// before the write's press returns; else at the commit of a transaction
    /// around the write. Either way the write lands first, the Back reaches
    /// the router once, through the Back control of the book written anew,
    /// and the app ends at the list, the book gone.
    private func checkABackThroughTheBookWrittenAnew(_ session: ExactSession, _ stack: UINavigationController, inTheWrite: Bool,
                                                     atTheList: @escaping () -> Bool) throws {
        let agent = Agent(session: session), navigation = session.presenter.navigationHost
        let book = try node(session, "book-a"), oldBack = try node(session, "back-a").id, root = try XCTUnwrap(stack.viewControllers.first as? RouteController)
        var presses: [String] = [], depth = 0
        let press = session.presenter.onPress
        session.presenter.onPress = { [unowned self] id in
            let name = session.presenter.views[id]?.props["testId"] ?? "#\(id)"
            let anew = (try? self.node(session, "book-a")).map { $0 !== book } ?? false
            presses.append("\(depth) \(name)\(id == oldBack ? " of the old book" : "")\(name.hasPrefix("back") ? (anew ? ", the book written anew in the tree" : ", no book written anew") : "")")
            depth += 1
            press?(id)
            // The stack laid out once the write has landed, before its press returns: where the write's own
            // transaction, when it was the outermost, committed and laid the stack out in the runs that failed
            if inTheWrite, name == "renew-a" { stack.view.layoutIfNeeded() }
            depth -= 1
        }
        defer { session.presenter.onPress = press }
        // Commit what the last turn left open: the transaction below is the outermost
        CATransaction.flush()
        if !inTheWrite { CATransaction.begin() }
        stack.popViewController(animated: true)
        XCTAssertNotNil(stack.transitionCoordinator, "the app writes the book anew before UIKit's pop is over")
        session.presenter.press(try node(session, "renew-a").id)
        if inTheWrite {
            XCTAssertFalse(has(session, "book-a"), "the Back has landed when the write's press returns")
        } else {
            XCTAssertEqual(presses, ["0 renew-a"], "UIKit starts its pop after the write")
            CATransaction.commit()
        }
        until("the app went back from the book with UIKit") {
            stack.transitionCoordinator == nil && !agent.nativeInFlight() && atTheList() && !has(session, "book-a")
        }
        XCTAssertEqual(presses, ["0 renew-a", "\(inTheWrite ? 1 : 0) back-a, the book written anew in the tree"],
                       inTheWrite ? "UIKit starts its pop before the write's press returns" : "UIKit starts its pop after the write")
        XCTAssertEqual(navigation.selectedKey, root.key, "the app at the list")
    }

    /// The same Back cancelled, as a swipe let go: UIKit popped nothing, so
    /// no Back reaches the app, and the book written anew takes the old one's
    /// place.
    func testACancelledBackFromTheCollapsedDetailKeepsTheDetailWrittenAnew() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let session = try boot(), agent = Agent(session: session), navigation = session.presenter.navigationHost
        let split = try collapsedBook(session)
        let pop = InteractivePop()
        navigation.setAppDelegate(split.list, pop)
        defer { navigation.setAppDelegate(split.list, nil) }
        pop.interactive = true
        let library = try XCTUnwrap(split.list.viewControllers.first)
        split.list.popViewController(animated: true)
        until("UIKit's pop animates the list beneath") { library.viewIfLoaded?.window != nil }
        pop.interaction.update(0.4)
        session.presenter.press(try node(session, "renew-a").id)
        pop.interaction.cancel()
        pop.interactive = false
        let book = try node(session, "book-a")
        until("the book written anew is the detail") {
            split.list.transitionCoordinator == nil && !agent.nativeInFlight() && split.nested && (split.detail.topViewController as? RouteController)?.node === book
        }
        XCTAssertEqual(split.shown.count, 2, "the list and the book")
    }

    /// UIKit's own Back within N1 and a completed drag of N1 off N0 each
    /// dispatch the route's Back once (LLP 1127 D4, D5.7): its `id="back"`
    /// control is pressed once.
    func testUIKitsPopsOfACollapsedDetailDispatchOneBackEach() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let session = try boot(), agent = Agent(session: session), navigation = session.presenter.navigationHost
        let split = try collapsedBook(session)
        try tap(session, "notes-a")
        until("the notes are pushed") { split.shown.count == 3 && !agent.nativeInFlight() }
        var presses: [UInt32] = []
        let press = session.presenter.onPress
        session.presenter.onPress = { presses.append($0); press?($0) }
        defer { session.presenter.onPress = press }
        let notesBack = try node(session, "back-notes-a").id
        split.detail.popViewController(animated: true)
        until("back at the book") { split.shown.count == 2 && split.detail.transitionCoordinator == nil && !agent.nativeInFlight() }
        XCTAssertEqual(presses, [notesBack], "the notes' Back, once")
        presses = []
        let bookBack = try node(session, "back-a").id
        let pop = InteractivePop()
        navigation.setAppDelegate(split.list, pop)
        defer { navigation.setAppDelegate(split.list, nil) }
        pop.interactive = true
        let library = try XCTUnwrap(split.list.viewControllers.first)
        split.list.popViewController(animated: true)
        until("UIKit's drag shows the list beneath") { library.viewIfLoaded?.window != nil }
        pop.interaction.update(0.4)
        pop.interaction.finish()
        pop.interactive = false
        until("back at the list") { split.shown.count == 1 && split.list.transitionCoordinator == nil && !agent.nativeInFlight() }
        XCTAssertEqual(presses, [bookBack], "the book's Back, once")
    }

    /// The app replaces the notes with another visit while the finger is down
    /// on a swipe in N1's own stack: the app's stack counts the list's route
    /// beneath N1's, as deep as when the swipe began, so the swipe's source
    /// was replaced in place and the app goes back from the replacement,
    /// rather than its pending sync pushing it where the person swiped the
    /// notes away.
    func testASwipeInTheDetailTakesTheAppBackAfterTheAppReplacedItsSource() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let session = try boot(), agent = Agent(session: session), navigation = session.presenter.navigationHost
        let split = try collapsedBook(session)
        try tap(session, "notes-a")
        until("the notes are pushed") { split.shown.count == 3 && !agent.nativeInFlight() }
        let pop = InteractivePop()
        navigation.setAppDelegate(split.detail, pop)
        defer { navigation.setAppDelegate(split.detail, nil) }
        pop.interactive = true
        let book = try XCTUnwrap(split.detail.viewControllers.first)
        split.detail.popViewController(animated: true)
        until("UIKit's pop animates the book beneath") { book.viewIfLoaded?.window != nil }
        pop.interaction.update(0.4)
        session.presenter.press(try node(session, "swap-a").id)
        XCTAssertTrue(has(session, "swap-z"), "the notes replaced with another visit")
        pop.interaction.finish()
        pop.interactive = false
        until("the app went back from the replacement, to the book") {
            split.detail.transitionCoordinator == nil && !agent.nativeInFlight() && split.shown.count == 2 && !has(session, "notes-page")
        }
        XCTAssertTrue(split.shown.last === book, "the book, where the person swiped")
    }

    /// A collapsed split animates its detail as a pushed stack does: N0's
    /// push and pop of N1 play the route's own entrance and exit (LLP
    /// 1013.001); a route that names a `navigationSource` zooms from it, as
    /// N1's first route and pushed within N1; and a `replace` of N1's top is
    /// UIKit's push.
    func testACollapsedSplitAnimatesItsDetailAsAPushedStackDoes() throws {
        let session = try boot()
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        let split = try split(session)
        let root = try XCTUnwrap(split.list.stackRoutes.first)
        let list = StackSpy(), detail = StackSpy()
        navigation.setAppDelegate(split.list, list)
        navigation.setAppDelegate(split.detail, detail)
        defer { navigation.setAppDelegate(split.list, nil); navigation.setAppDelegate(split.detail, nil) }
        try tap(session, "row-page")
        until("the page is pushed") { split.shown.count == 2 && split.nested && !agent.nativeInFlight() }
        XCTAssertTrue(list.pushes.contains { $0 === split.detail }, "UIKit asked N0's delegate for its push of N1")
        XCTAssertTrue(navigation.routeTransition(split.list, operation: .push, from: root, to: split.detail) is RouteTransition, "the page's own entrance")
        XCTAssertTrue(navigation.routeTransition(split.list, operation: .pop, from: split.detail, to: root) is RouteTransition, "and its exit")
        XCTAssertNil(agent.back()["error"])
        until("back at the list") { split.shown.count == 1 && !agent.nativeInFlight() }
        if #available(iOS 18.0, *) {
            try tap(session, "row-photo")
            until("the photo is pushed") { split.shown.count == 2 && split.nested && !agent.nativeInFlight() }
            XCTAssertNotNil(split.detail.preferredTransition, "N1 zooms in with the photo, from its row")
            XCTAssertNil(agent.back()["error"])
            until("back at the list again") { split.shown.count == 1 && !agent.nativeInFlight() }
        }
        try tap(session, "row-a")
        until("the book is pushed") { split.shown.count == 2 && split.nested && !agent.nativeInFlight() }
        if #available(iOS 18.0, *) {
            XCTAssertNil(split.detail.preferredTransition, "a book that names no source is UIKit's push")
            try tap(session, "photo-a")
            until("the photo is pushed within N1") { split.shown.count == 3 && !agent.nativeInFlight() }
            XCTAssertNotNil(split.detailRoutes.last?.preferredTransition, "it zooms from the book's art")
            XCTAssertNil(agent.back()["error"])
            until("back at the book") { split.shown.count == 2 && !agent.nativeInFlight() }
        }
        detail.shown = []
        try tap(session, "next-a")
        until("a page replaces the book") { has(session, "page-page") && !has(session, "book-a") && split.shown.count == 2 && !agent.nativeInFlight() }
        let page = try node(session, "page-page")
        XCTAssertTrue(detail.shown.contains { ($0.0 as? RouteController)?.node === page && $0.1 }, "as UIKit's push, animated")
    }
}

extension SplitIOSTests {
    /// Expanded, a route pushed within N1 from a source zooms from it, as a
    /// pushed stack's push does (ZoomPushIOS).
    func testAnExpandedDetailsPushZoomsFromItsSource() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("the zoom transition is iOS 18's") }
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular), agent = Agent(session: session)
        let split = try split(session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        try tap(session, "row-a")
        until("book a is the detail") { split.detailRoutes.count == 1 && has(session, "book-a") && !agent.nativeInFlight() }
        try tap(session, "photo-a")
        until("the photo is pushed within N1") { split.detailRoutes.count == 2 && !agent.nativeInFlight() }
        XCTAssertNotNil(split.detailRoutes.last?.preferredTransition, "it zooms from the book's art")
    }

    /// N1's own transition, which N0's push and pop of N1 play, is its
    /// base's (LLP 1127 D6, "How the columns move"): folded, the photo's
    /// zoom; open, a book chosen beside the list names no source and takes
    /// it away, so folded again N0 pops N1 with no zoom whose source has
    /// gone; a photo chosen beside the list gives N1 the photo's zoom from
    /// its row again.
    func testTheDetailColumnsTransitionIsItsBases() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("the zoom transition is iOS 18's") }
        let session = try boot(size: CGSize(width: 1100, height: 800)), agent = Agent(session: session)
        let split = try split(session)
        try tap(session, "row-photo")
        until("the photo is pushed") { split.shown.count == 2 && split.nested && !agent.nativeInFlight() }
        XCTAssertNotNil(split.detail.preferredTransition, "N0 pushes N1 with the photo's zoom")
        split.list.popViewController(animated: false)
        until("UIKit's Back at the list") { split.shown.count == 1 && !agent.nativeInFlight() }
        window?.traitOverrides.horizontalSizeClass = .regular
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        try tap(session, "row-a")
        until("book a is the detail") { split.detailRoutes.count == 1 && has(session, "book-a") && !agent.nativeInFlight() }
        XCTAssertNil(split.detail.preferredTransition, "book a names no source")
        window?.traitOverrides.horizontalSizeClass = .compact
        until("collapsed on book a") { split.controller.isCollapsed && split.nested && !agent.nativeInFlight() }
        XCTAssertNil(split.detail.preferredTransition, "N0 pops N1 without the photo's zoom")
        window?.traitOverrides.horizontalSizeClass = .regular
        until("expanded again") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        XCTAssertNil(agent.back()["error"], "Back from the detail's base clears it (LLP 1127 D4)")
        until("back at the list") { split.shown.count == 1 && !agent.nativeInFlight() }
        try tap(session, "row-photo")
        until("the photo is the detail") { has(session, "photo-page") && split.detailRoutes.count == 1 && !agent.nativeInFlight() }
        XCTAssertNotNil(split.detail.preferredTransition, "the photo chosen beside the list: N1 zooms from its row")
    }

    /// The zoom's drag-down dismissal of N1, which N0 pops whole, begins
    /// only while N1 holds just its base, as N0's back swipe does (LLP 1127
    /// D5.3); over a route pushed within N1, that route's own pop is N1's.
    func testTheDetailColumnsZoomDismissalBeginsOnlyOverItsBase() throws {
        let session = try boot(), agent = Agent(session: session), navigation = session.presenter.navigationHost
        let split = try split(session)
        try tap(session, "row-photo")
        until("the photo is pushed") { split.shown.count == 2 && split.nested && !agent.nativeInFlight() }
        XCTAssertTrue(navigation.zoomDismissalBegins(split.detail), "N1 over its base alone")
        XCTAssertFalse(navigation.zoomDismissalBegins(try XCTUnwrap(split.detailRoutes.first)), "N1 cannot pop its base")
        try tap(session, "photo-notes-q")
        until("the notes are pushed within N1") { split.shown.count == 3 && !agent.nativeInFlight() }
        XCTAssertFalse(navigation.zoomDismissalBegins(split.detail), "N0's pop would take the notes and the photo both")
        XCTAssertTrue(navigation.zoomDismissalBegins(try XCTUnwrap(split.detailRoutes.last)), "the notes' own pop")
        if #available(iOS 18.0, *) {
            XCTAssertNil(split.detail.preferredTransition, "over the notes, N0's pop of N1 is the notes' own, not the photo's zoom")
            XCTAssertNil(agent.back()["error"])
            until("back on the photo") { split.shown.count == 2 && split.nested && !agent.nativeInFlight() }
            XCTAssertNotNil(split.detail.preferredTransition, "the photo alone: N0 pops N1 with its zoom")
        }
    }
}

/// A root controller that turns with the device in portrait only, as an app
/// whose boot may be deferred (`NavigationHost.deferInitialOwner`).
private final class PortraitController: UIViewController {
    override var supportedInterfaceOrientations: UIInterfaceOrientationMask { .portrait }
}

/// What UIKit tells a stack's delegate: each controller it shows, and
/// whether animated, and each push it asks an animator for.
private final class StackSpy: NSObject, UINavigationControllerDelegate {
    var shown: [(UIViewController, Bool)] = []
    var pushes: [UIViewController] = []
    func navigationController(_ nav: UINavigationController, willShow controller: UIViewController, animated: Bool) { shown.append((controller, animated)) }
    func navigationController(_ nav: UINavigationController, animationControllerFor operation: UINavigationController.Operation,
                              from: UIViewController, to: UIViewController) -> UIViewControllerAnimatedTransitioning? {
        if operation == .push { pushes.append(to) }
        return nil
    }
}
#endif
