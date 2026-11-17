#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// UIKit's stacks are the state the router follows (LLP 1075.003 §3.7): a
/// transition started by UIKit or the app, a second step made in the same
/// turn or once its animations start, and then, once nothing is under way,
/// UIKit and the router agree (`NavigationHost.restDifference`), and still
/// do after each tab is visited again. A Back reaches the router once, and
/// no pop is left for the router's stack to be written back over. Over
/// `more-tabs.contract`, in a scene's window, where UIKit animates and ends
/// transitions as in an app.
final class NavigationSequenceIOSTests: XCTestCase {
    private var window: UIWindow?
    private var sessions: [ExactSession] = []

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        UIView.setAnimationsEnabled(true)
        super.tearDown()
    }

    /// The state a sequence starts from.
    enum Start: String, CaseIterable { case home = "Home", homeDetail = "Home over Detail", fiveDeeper = "Five's Deeper under More" }

    /// What starts a transition.
    enum First: String, CaseIterable { case uikitBack = "UIKit's Back", appBack = "the app's Back", moreAgain = "More tapped again", appPush = "the app's push" }

    /// The second step.
    enum Second: String, CaseIterable {
        case barHome = "a tap on Home", barBarless = "a tap on Barless", more = "a tap on More", rowSix = "Six's row picked"
        case push = "the app's push on Home", width = "a regular width and back"
    }

    func testSequencesFromHome() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates and ends transitions as in an app only in a scene's window") }
        try run(.home, [.appPush])
    }

    func testSequencesFromHomeOverDetail() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates and ends transitions as in an app only in a scene's window") }
        try run(.homeDetail, [.uikitBack, .appBack])
    }

    func testSequencesFromFivesDeeperUnderMore() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates and ends transitions as in an app only in a scene's window") }
        try run(.fiveDeeper, [.uikitBack, .appBack, .moreAgain])
    }

    // An app's selection made while a transition is under way, on a route that does not appear in it, is held over the
    // route's layout and has run its edge passes by the transition's end. The covers report is swallowed, so no batch
    // it brings runs them (`held`).

    /// On the route a push covers
    func testASelectionAPushHoldsRunsAtItsEnd() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", Self.transitionsInAScene) }
        try held(.push)
    }

    /// On Five's root, in flight as More pops to its list, when Home is tapped meanwhile
    func testASelectionMoresPopHoldsRunsAtItsEnd() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", Self.transitionsInAScene) }
        try held(.morePop)
    }

    /// On the route behind a sheet as the sheet is presented
    func testASelectionAPresentationHoldsRunsAtItsEnd() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", Self.transitionsInAScene) }
        try held(.present)
    }

    /// On the route behind a sheet as an interactive dismissal of it is cancelled
    func testASelectionACancelledDismissalHoldsRunsAtItsEnd() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", Self.transitionsInAScene) }
        try held(.cancelled)
    }

    private static let transitionsInAScene = "UIKit animates and ends transitions as in an app only in a scene's window"

    enum Held: String { case push = "the app's push", morePop = "More's pop to its list", present = "the sheet's presentation", cancelled = "the sheet's dismissal, cancelled" }

    private func held(_ step: Held) throws {
        let session = try fixture()
        let p = session.presenter, navigation = p.navigation
        let tabs = try XCTUnwrap(navigation.tabController)
        var more: UINavigationController { tabs.moreNavigationController }
        if step == .morePop {
            guard tabs.tabBar.items?.count == 5 else { throw XCTSkip("UIKit's bar shows every tab at this width: there is no More") }
            tapTab(tabs, of: more)
            settle(navigation, more, step.rawValue)
            tabs.selectedViewController = tabs.viewControllers?[4]
            settle(navigation, more, step.rawValue)
            try tapNode(session, "open-deeper")
        } else if step == .cancelled {
            try tapNode(session, "open-sheet")
        }
        settle(navigation, more, step.rawValue)
        let deliver = p.onCovers, dismissal = SheetDismissal()
        p.onCovers = { _ in }
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false); dismissal.interactive = false; p.onCovers = deliver }
        switch step {
        case .push:
            try tapNode(session, "open-detail")
            waitForAnimationsToStart(try XCTUnwrap(navigation.navigation))
        case .morePop:
            _ = tabs.delegate?.tabBarController?(tabs, shouldSelect: more)
            tabs.delegate?.tabBarController?(tabs, didSelect: try XCTUnwrap(tabs.selectedViewController))
            more.popToRootViewController(animated: true)
            waitForAnimationsToStart(more)
        case .present:
            try tapNode(session, "open-sheet")
            waitFor("the presentation starts") { p.modals.inTransition }
        case .cancelled:
            // The presented controller holds the sheet's navigation as its child
            let sheet = try XCTUnwrap(controller(navigation, "route-sheet").navigationController?.parent)
            sheet.transitioningDelegate = dismissal
            sheet.dismiss(animated: true)
            waitFor("the dismissal starts") { sheet.transitionCoordinator?.isInteractive == true && dismissal.started }
            dismissal.interaction.update(0.4)
        }
        let c = try controller(navigation, step == .morePop ? "route-five" : "route-home"), native = UIScrollView(frame: c.view.bounds)
        c.view.addSubview(native)
        defer { native.removeFromSuperview() }
        let list = try XCTUnwrap(c.collapseScroll, "\(step.rawValue): the route's title follows its list")
        c.setContentScrollView(native, for: .top)
        c.view.layoutIfNeeded()
        XCTAssertTrue(c.scrollEdgesOwed, "\(step.rawValue): the transition holds the passes over the route's layout")
        XCTAssertTrue(c.collapseScroll === list)
        if step == .morePop {
            tapTab(tabs, 0)
            XCTAssertTrue(c.scrollEdgesOwed, "Home's sync leaves them owed by Five's root, in no stack while More pops")
        }
        if step == .cancelled { dismissal.interaction.cancel() }
        let syncs = navigation.syncCalls
        settle(navigation, more, step.rawValue)
        waitFor("\(step.rawValue): the sheet's transition ends") { !p.modals.inTransition }
        XCTAssertFalse(c.scrollEdgesOwed, "\(step.rawValue): they have run by its end")
        XCTAssertNil(c.collapseScroll, "\(step.rawValue): and moved the title off the list to the app's view")
        // No sync was asked for by the cancelled dismissal's end, where `runHeldSync` or the route's layout runs them (the
        // sheet's navigation reaches it as it shows again); one followed the push, whose arriving route's local insets reach
        // the runtime during it and hold their batch's sync to its end, More's pop and the presentation
        if step == .cancelled { XCTAssertEqual(navigation.syncCalls, syncs, "\(step.rawValue): no sync was asked for") }
        else { XCTAssertGreaterThan(navigation.syncCalls, syncs, "\(step.rawValue): a sync followed") }
    }

    /// The end of a transition on a stack navigation does not follow, reported with nothing held, runs the edge passes
    /// an app's selection owes (`runHeldSync`): Three's stack's `didShow` while Home's shows. The covers report is
    /// swallowed: the runtime's answer to it lands within the call when no list fill is in flight, and its batch is no
    /// sync the end ran.
    func testAnotherStacksTransitionEndRunsOwedPassesWithNothingHeld() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit builds and lays out the tabs' stacks as in an app only in a scene's window") }
        let session = try fixture()
        let navigation = session.presenter.navigation
        let tabs = try XCTUnwrap(navigation.tabController)
        var more: UINavigationController { tabs.moreNavigationController }
        tapTab(tabs, 2)
        settle(navigation, more)
        tapTab(tabs, 0)
        settle(navigation, more)
        let three = try XCTUnwrap(tabs.viewControllers?[2] as? UINavigationController)
        XCTAssertFalse(navigation.navigation === three, "navigation follows Home's stack")
        let c = try controller(navigation, "route-home"), native = UIScrollView(frame: c.view.bounds)
        let list = try XCTUnwrap(c.collapseScroll, "Home's title follows its list")
        c.view.addSubview(native)
        defer { native.removeFromSuperview() }
        c.setContentScrollView(native, for: .top)
        XCTAssertTrue(c.scrollEdgesOwed && c.collapseScroll === list)
        XCTAssertFalse(navigation.pendingSync || navigation.tabsHeld, "nothing is held")
        let deliver = session.presenter.onCovers
        session.presenter.onCovers = { _ in }
        defer { session.presenter.onCovers = deliver }
        let syncs = navigation.syncCalls
        three.delegate?.navigationController?(three, didShow: try XCTUnwrap(three.topViewController), animated: false)
        XCTAssertFalse(c.scrollEdgesOwed, "the end runs the passes the selection owes")
        XCTAssertNil(c.collapseScroll, "and the title leaves the list for the app's view")
        XCTAssertEqual(navigation.syncCalls, syncs, "with no sync")
    }

    private func controller(_ navigation: NavigationHost, _ testId: String) throws -> RouteController {
        try XCTUnwrap(navigation.controllers.values.first { $0.node.props["testId"] == testId }, "\(testId) has a controller")
    }

    private func run(_ start: Start, _ firsts: [First]) throws {
        for first in firsts {
            for second in Second.allCases {
                for started in [false, true] {
                    let label = "\(start.rawValue), \(first.rawValue), then \(second.rawValue) \(started ? "once its animations start" : "in the same turn")"
                    try XCTContext.runActivity(named: label) { _ in try sequence(start, first, second, started: started, label) }
                }
            }
        }
    }

    private func sequence(_ start: Start, _ first: First, _ second: Second, started: Bool, _ label: String) throws {
        let session = try fixture()
        let navigation = session.presenter.navigation
        let tabs = try XCTUnwrap(navigation.tabController)
        guard tabs.tabBar.items?.count == 5 else { throw XCTSkip("UIKit's bar shows every tab at this width: there is no More") }
        // UIKit makes a new More after a width change moves every tab to the bar and back
        var more: UINavigationController { tabs.moreNavigationController }
        switch start {
        case .home: break
        case .homeDetail:
            try tapNode(session, "open-detail")
        case .fiveDeeper:
            tapTab(tabs, of: more)
            settle(navigation, more, label)
            tabs.selectedViewController = tabs.viewControllers?[4]
            settle(navigation, more, label)
            try tapNode(session, "open-deeper")
        }
        settle(navigation, more, label)
        agree(navigation, "\(label): before")
        // The Back control of the route the first step pops, if it pops one
        let back = session.presenter.views.values.first { ["back-detail", "back-deeper"].contains($0.props["testId"] ?? "") }?.id
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        let stack = navigation.navigation
        switch first {
        case .uikitBack: stack?.popViewController(animated: true)
        case .appBack: try tapNode(session, start == .homeDetail ? "back-detail" : "back-deeper")
        case .appPush: try tapNode(session, "open-detail")
        case .moreAgain:
            _ = tabs.delegate?.tabBarController?(tabs, shouldSelect: more)
            tabs.delegate?.tabBarController?(tabs, didSelect: try XCTUnwrap(tabs.selectedViewController))
            more.popToRootViewController(animated: true)
        }
        if started, let stack, stack.transitionCoordinator != nil { waitForAnimationsToStart(stack) }
        switch second {
        case .barHome: tapTab(tabs, 0)
        case .barBarless: tapTab(tabs, 1)
        case .more: tapTab(tabs, of: more)
        case .rowSix: tabs.selectedViewController = tabs.viewControllers?[5]
        case .push:
            tapTab(tabs, 0)
            if let open = session.presenter.views.values.first(where: { $0.props["testId"] == "open-detail" }) {
                _ = Agent(session: session).tap(["id": Int(open.id)])
            }
        case .width:
            setSizeClasses(tabs, horizontal: .regular)
            waitForMainQueue()
            setSizeClasses(tabs, horizontal: .compact)
        }
        settle(navigation, more, label)
        agree(navigation, "\(label): at rest")
        // Each tab visited again shows what the router holds for it
        for index in [0, 1] {
            tapTab(tabs, index)
            settle(navigation, more, label)
            agree(navigation, "\(label): tab \(index) visited")
        }
        tapTab(tabs, of: more)
        settle(navigation, more, label)
        agree(navigation, "\(label): More visited")
        if let back {
            XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "\(label): the Back reached the router once")
        }
        XCTAssertEqual(journal(session, "the router's stacks are written back"), 0, "\(label): no pop was left unfollowed")
        XCTAssertFalse(navigation.tabsHeld, "\(label): no write waits")
    }

    /// How many times the session's journal holds `line`.
    private func journal(_ session: ExactSession, _ line: String) -> Int {
        session.agent(#"{"op":"logs","since":0}"#).components(separatedBy: line).count - 1
    }

    /// Waits until nothing is under way: no transition on More or the stack
    /// navigation follows, no sync owed, and a moment more for UIKit's
    /// deferred work.
    private func settle(_ navigation: NavigationHost, _ more: UINavigationController, _ label: String = "") {
        waitFor("\(label): nothing is under way", cap: 8, facts: {
            "navigation's transition \(navigation.navigation?.transitionCoordinator != nil), routes in flight \(navigation.routesInFlight(navigation.selectedStack)), held sync \(navigation.pendingSync), router tab \(navigation.routerTab), selected \(navigation.selectedKey ?? "none"), UIKit holds \(navigation.navigation?.viewControllers.map { ($0 as? RouteController)?.key ?? "list" } ?? []), More's transition \(more.transitionCoordinator != nil)"
        }) {
            // A stack off screen can keep a transition UIKit dropped (LLP 1075.003 §9.16)
            !navigation.inTransition && more.transitionCoordinator == nil
                && navigation.navigation.map { $0.viewIfLoaded?.window == nil || $0.transitionCoordinator == nil } ?? true
        }
        let until = Date().addingTimeInterval(0.6)
        waitFor("UIKit's deferred work") { Date() >= until }
    }

    private func agree(_ navigation: NavigationHost, _ label: String, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertNil(navigation.restDifference, label, file: file, line: line)
    }

    private func fixture() throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env["EXACT_MORE_TABS_PLAN"], "build.mjs --test --ios compiles the more-tabs fixture")))
        for old in sessions { old.destroy() }
        let session = ExactApp.shared.makeSession(label: "sequence")
        sessions = [session]
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        let window = UIWindow(windowScene: scene)
        window.frame = frame
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window?.isHidden = true
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: frame.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        UIView.setAnimationsEnabled(false)
        waitFor("the tab container and the selected stack are in the window") {
            let navigation = session.presenter.navigation
            return navigation.tabController?.selectedViewController?.viewIfLoaded?.window != nil && !Agent(session: session).nativeInFlight()
        }
        return session
    }
}
#endif
