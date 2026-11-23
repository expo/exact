#if os(iOS)
import ObjectiveC.runtime
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// Navigation preserves requests made during UIKit callbacks and retries
/// writes UIKit declines without reporting a transition.
final class NavigationDeferredIOSTests: XCTestCase {
    /// A layout callback keeps its batch and leaves an existing hold alone.
    func testABatchDuringLayoutProjectsWhenNavigationSyncEnds() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "navigation needs window containment") }
        let (session, navigation, nav) = try fixture("nested-navigation-batch", in: scene)
        let root = try XCTUnwrap(navigation.container)
        let previous = try XCTUnwrap(nav.topViewController)
        let probe = NavigationLayoutProbe(frame: CGRect(x: 0, y: 0, width: 1, height: 1))
        nav.view.addSubview(probe)
        defer { probe.removeFromSuperview() }
        probe.run = {
            XCTAssertTrue(navigation.syncing)
            navigation.projectionHeld = true
            navigation.runHeldSync()
            XCTAssertTrue(navigation.pendingSync, "a reentrant completion leaves the hold in place")
            let syncs = navigation.syncCalls
            self.replaceRoute(in: session.presenter, root: root, id: 900_001, key: "nested")
            XCTAssertEqual(navigation.activeKey, "nested", "the nested batch selected its route")
            XCTAssertGreaterThan(navigation.syncCalls, syncs, "the nested batch requested a sync")
            XCTAssertTrue(nav.topViewController === previous)
        }
        probe.setNeedsLayout()
        nav.view.setNeedsLayout()

        navigation.resync()

        XCTAssertNil(probe.run, "UIKit laid the probe out during sync")
        XCTAssertEqual((nav.topViewController as? RouteController)?.key, "nested")
    }

    /// A chain of reentrant sync requests can yield twice. It journals once,
    /// and the agent waits for both continuations.
    func testReentrantNavigationYieldsAcrossTurnsAndJournalsOnce() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "navigation needs window containment") }
        let (session, navigation, nav) = try fixture("recursive-navigation-batches", in: scene)
        let root = try XCTUnwrap(navigation.container)
        var writes = 0
        try withNavigationStackWrites(shouldRefuse: { destination, routes in
            guard destination === nav, (routes.last as? RouteController)?.key == "pass-\(writes + 1)" else { return false }
            XCTAssertTrue(navigation.syncing)
            writes += 1
            if writes < 17 {
                self.replaceRoute(in: session.presenter, root: root, id: 900_001 + UInt32(writes), key: "pass-\(writes + 1)")
            }
            return false
        }) {
            replaceRoute(in: session.presenter, root: root, id: 900_001, key: "pass-1")
            navigation.resync()
            XCTAssertEqual(writes, 8, "the current turn has a bounded number of writes")
            XCTAssertEqual((nav.topViewController as? RouteController)?.key, "pass-8")
            XCTAssertFalse(navigation.inTransition, "UIKit finished the current projection")
            let agent = session.agentInstance
            XCTAssertTrue(agent.hostsInTransition, "the queued projection is still in flight")
            XCTAssertTrue(agent.awaitTransitions())
            XCTAssertEqual(writes, 17)
            XCTAssertEqual((nav.topViewController as? RouteController)?.key, "pass-17")
            let logs = session.agent(#"{"op":"logs","since":0}"#)
            let yield = "8 synchronous projection passes left work to run on the main queue's next turn"
            XCTAssertEqual(logs.components(separatedBy: yield).count - 1, 1)
        }
    }

    /// A numbered initial batch's final pass reads the nested child list
    /// that applies during installation, even when that list is unnumbered.
    func testInitialInstallProjectsTheNestedPanelChildrenLast() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "navigation needs window containment") }
        let (session, view) = mountedSession("initial-navigation-batch", in: scene)
        let presenter = session.presenter, navigation = presenter.navigationHost
        let plan = try fixturePlan("NESTED_PANEL_TABS")
        let added: UInt32 = 900_001
        var projectedPanel: UInt32?
        var expected: [UInt32] = []
        try withNavigationStackWrites(shouldRefuse: { nav, _ in
            guard projectedPanel == nil, navigation.stacks[ObjectIdentifier(nav)] != nil,
                  let panel = navigation.tabPanels.first,
                  let before = navigation.logicalChildren[panel] else { return false }
            XCTAssertTrue(navigation.syncing && presenter.applying)
            XCTAssertNil(navigation.primaryOwner, "still installing the initial owner")
            projectedPanel = panel
            expected = before + [added]
            presenter.apply(wireBatch([
                ["op": "create", "id": added, "kind": "column"],
                ["op": "children", "id": panel, "ids": expected]
            ]))
            return false
        }) {
            let initial = session.boot(plan: plan, size: view.bounds.size)
            XCTAssertNil(initial.error)
            XCTAssertGreaterThan(initial.sequence, 0)
        }
        let panel = try XCTUnwrap(presenter.views[try XCTUnwrap(projectedPanel)])
        XCTAssertEqual(navigation.siblings.recordedIDs(of: panel), expected)
    }

    /// A quiet stack retries the latest request with its press handoff and
    /// animation policy intact, even when UIKit sends no callback.
    func testTheLatestStackRetriesWithoutACompletionCallback() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "navigation needs window containment") }
        let (session, navigation, nav) = try fixture("refused-stack-write", in: scene)
        let root = try XCTUnwrap(navigation.container)
        let previous = try XCTUnwrap(nav.topViewController)
        let delegate = nav.delegate
        nav.delegate = nil
        defer { nav.delegate = delegate }
        let press = session.presenter.beginPress(control: nil)
        navigation.unanimated = true
        var refusing = true, refusals = 0
        try withNavigationStackWrites(shouldRefuse: { stack, _ in
            guard stack === nav, refusing else { return false }
            XCTAssertNil(stack.transitionCoordinator)
            refusals += 1
            return true
        }) {
            let requests: [(UInt32, String)] = [(900_001, "first"), (900_002, "latest")]
            for (id, key) in requests {
                let before = refusals
                replaceRoute(in: session.presenter, root: root, id: id, key: key)
                XCTAssertEqual(navigation.shownKeys, [key])
                XCTAssertGreaterThan(refusals, before, "UIKit declined the write for \(key)")
                XCTAssertTrue(navigation.stackWritesOwed)
                XCTAssertTrue(nav.topViewController === previous)
            }
            press.dispatched()
            XCTAssertTrue(press.pending, "the refused write still owes the press")
            XCTAssertTrue(navigation.unanimated, "the retry keeps the requested animation policy")
            let before = refusals
            waitFor("a second attempt while the setter still refuses", cap: 10) { refusals > before }
            XCTAssertTrue(navigation.stackWritesOwed)
            XCTAssertTrue(nav.topViewController === previous)
            XCTAssertTrue(press.pending)
            XCTAssertTrue(navigation.unanimated)
            refusing = false
            waitFor("the latest route lands", cap: 10) { (nav.topViewController as? RouteController)?.key == "latest" }
            XCTAssertFalse(navigation.stackWritesOwed)
            XCTAssertFalse(press.pending)
            XCTAssertFalse(navigation.unanimated)
        }
    }

    /// An exhausted refusal is visible at rest. Later batches retry the
    /// same write once per sync, and a new request gets a timer budget.
    func testAStackThatKeepsRefusingExhaustsItsRetryBudget() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "navigation needs window containment") }
        let (session, navigation, nav) = try fixture("exhausted-stack-write", in: scene)
        let root = try XCTUnwrap(navigation.container)
        let previous = try XCTUnwrap(nav.topViewController as? RouteController)
        let stackLabel = try XCTUnwrap(navigation.stacks[ObjectIdentifier(nav)]?.label)
        let delegate = nav.delegate
        nav.delegate = nil
        defer { nav.delegate = delegate }
        var refusing = true, attempts = 0
        var requested: UIViewController?
        try withNavigationStackWrites(shouldRefuse: { stack, routes in
            guard stack === nav else { return false }
            attempts += 1
            requested = routes.last
            return refusing
        }) {
            replaceRoute(in: session.presenter, root: root, id: 900_001, key: "refused")
            XCTAssertTrue(navigation.stackWriteRetry)
            XCTAssertNil(navigation.restDifference, "the first retry is scheduled")
            waitFor("the retry budget was spent", cap: 20) { !navigation.stackWriteRetry }
            let held = "holds [\(previous.key)] in UIKit"
            let exhaustion = "stack write still owed after 60 retry turns. stack \(stackLabel) \(held) and was asked for [refused]"
            XCTAssertTrue(navigation.stackWritesOwed)
            XCTAssertTrue(nav.topViewController === previous)
            XCTAssertEqual(navigation.restDifference, "the stack \(held) and [refused] in the router")
            XCTAssertFalse(navigation.pendingSync)
            XCTAssertFalse(navigation.inTransition)
            XCTAssertTrue(navigation.syncOwed, "the next batch owes a full projection")
            XCTAssertFalse(session.agentInstance.hostsInTransition, "clock settle has no navigation work to wait for")

            let owner = try XCTUnwrap(nav.parent)
            navigation.tabOwner = owner
            XCTAssertTrue(navigation.tabIntent(0, ""))
            waitForMainQueue()
            navigation.tabOwner = nil
            let logs = session.agent(#"{"op":"logs","since":0}"#)
            XCTAssertTrue(logs.contains("tabs: back() refused: nothing is beneath the top of the stack in front"))
            XCTAssertFalse(logs.contains("tabs: back() refused: a transition is running"))
            XCTAssertEqual(logs.components(separatedBy: exhaustion).count - 1, 1)

            let firstRequest = try XCTUnwrap(requested)
            for _ in 0..<2 {
                let before = attempts
                session.presenter.apply(wireBatch([]))
                XCTAssertGreaterThan(attempts, before, "the batch tries the write")
                XCTAssertTrue(requested === firstRequest)
                XCTAssertFalse(navigation.stackWriteRetry, "the same refusal has spent its budget")
                XCTAssertFalse(navigation.inTransition)
            }
            replaceRoute(in: session.presenter, root: root, id: 900_002, key: "refused")
            let nextRequest = try XCTUnwrap(requested as? RouteController)
            XCTAssertEqual(nextRequest.node.id, 900_002, "the route key now names a different controller")
            XCTAssertTrue(navigation.stackWriteRetry, "the new refusal gets a retry budget")
            let before = attempts
            waitFor("the new refusal spends its retry budget", cap: 20) { !navigation.stackWriteRetry }
            XCTAssertGreaterThan(attempts, before, "the scheduled retry tried the new write")
            let laterLogs = session.agent(#"{"op":"logs","since":0}"#)
            XCTAssertEqual(laterLogs.components(separatedBy: exhaustion).count - 1, 1, "both refusals have the same diagnostic")

            refusing = false
            session.presenter.apply(wireBatch([]))
            XCTAssertTrue(nav.topViewController === nextRequest)
            XCTAssertFalse(navigation.stackWritesOwed)
            XCTAssertNil(navigation.restDifference)
        }
    }

    private func mountedSession(_ label: String, in scene: UIWindowScene) -> (ExactSession, ExactView) {
        let session = ExactApp.shared.makeSession(label: label)
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = window.screen.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        view.frame = host.view.bounds
        host.view.addSubview(view)
        addTeardownBlock { session.destroy(); window.isHidden = true }
        return (session, view)
    }

    private func fixture(_ label: String, in scene: UIWindowScene) throws -> (ExactSession, NavigationHost, UINavigationController) {
        let (session, view) = mountedSession(label, in: scene)
        let plan = try fixturePlan("NAVIGATION_BARS")
        XCTAssertNil(session.boot(plan: plan, size: view.bounds.size).error)
        view.layoutIfNeeded()
        waitForActivation(session)
        let navigation = session.presenter.navigationHost
        waitFor("the initial stack is visible and idle") {
            guard let nav = navigation.primaryNavigation else { return false }
            return nav.topViewController?.viewIfLoaded?.window === view.window && nav.transitionCoordinator == nil && !navigation.inTransition
        }
        let nav = try XCTUnwrap(navigation.primaryNavigation)
        session.presenter.onCovers = nil
        return (session, navigation, nav)
    }

    private func replaceRoute(in presenter: Presenter, root: NodeView, id: UInt32, key: String) {
        presenter.apply(wireBatch([
            ["op": "create", "id": id, "kind": "column", "props": ["navigationKey": key]],
            ["op": "children", "id": root.id, "ids": [id]],
            ["op": "props", "id": root.id, "set": ["navigationKey": key]]
        ]))
    }
}

/// Runs one test callback from UIKit layout.
final class NavigationLayoutProbe: UIView {
    var run: (() -> Void)?

    override func layoutSubviews() {
        super.layoutSubviews()
        let work = run
        run = nil
        work?()
    }
}

/// Intercepts stack setters for a test and restores UIKit's implementation
/// when the test finishes. A true result from `shouldRefuse` skips the setter.
func withNavigationStackWrites(shouldRefuse: @escaping (UINavigationController, [UIViewController]) -> Bool,
                               _ test: () throws -> Void) throws {
    let writer = try XCTUnwrap(class_getInstanceMethod(UINavigationController.self, #selector(UINavigationController.setViewControllers(_:animated:))))
    let shim = try XCTUnwrap(class_getInstanceMethod(UINavigationController.self, #selector(UINavigationController.interceptStackWrite(_:animated:))))
    precondition(StackWriteInterception.shouldRefuse == nil)
    StackWriteInterception.shouldRefuse = shouldRefuse
    method_exchangeImplementations(writer, shim)
    defer {
        method_exchangeImplementations(writer, shim)
        StackWriteInterception.shouldRefuse = nil
    }
    try test()
}

private enum StackWriteInterception {
    static var shouldRefuse: ((UINavigationController, [UIViewController]) -> Bool)?
}

extension UINavigationController {
    @objc dynamic fileprivate func interceptStackWrite(_ routes: [UIViewController], animated: Bool) {
        if StackWriteInterception.shouldRefuse?(self, routes) == true { return }
        interceptStackWrite(routes, animated: animated)
    }
}
#endif
