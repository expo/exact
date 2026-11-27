#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// A pop UIKit runs reaches the app (`NavigationHost.followPops`, and a
/// swipe's end in `completeTransition`): the app follows a pop UIKit
/// committed, whatever became of the source's key or of a route the app
/// selected again meanwhile, and a cancelled swipe dispatches no Back. The
/// tree is the test's own, written to the presenter as the runner writes
/// it; a Back is the press of the route's `navigationBack` control, which
/// the test answers as the app would, in the same turn.
final class NavigationPopIOSTests: XCTestCase {
    private var window: UIWindow?
    private var sessions: [ExactSession] = []

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    /// UIKit animates a pop only in a window a scene holds.
    private var hasScene: Bool { UIApplication.shared.connectedScenes.contains { $0 is UIWindowScene } }

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }

    private struct Stack {
        let session: ExactSession
        let nav: UINavigationController
        let apply: ([[String: Any]]) -> Void
        var key: String? { (nav.topViewController as? RouteController)?.key }
    }

    /// Settings (2), Updates (3, its Back 4) and Log (5, its Back 6) under
    /// the root (1), Log selected, and UIKit's stack written from them.
    private func stack(_ label: String) throws -> Stack {
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        XCTAssertNotNil(session.boot(plan: Data(), size: CGSize(width: 402, height: 874)).error, "no plan in no bytes")
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        view.frame = host.view.bounds
        self.window = window
        session.presenter.viewport.frame = host.view.bounds
        let apply = { (ops: [[String: Any]]) in session.presenter.apply(wireBatch(ops)) }
        apply([
            ["op": "create", "id": 1, "kind": "view", "props": ["navigationBack": "back", "navigationKey": "log"]], frame(1),
            ["op": "create", "id": 2, "kind": "view", "props": ["navigationKey": "settings"]], frame(2),
            ["op": "create", "id": 3, "kind": "view", "props": ["navigationKey": "updates"]], frame(3),
            ["op": "create", "id": 5, "kind": "view", "props": ["navigationKey": "log"]], frame(5),
        ] + back(4, in: 3) + back(6, in: 5) + [
            ["op": "children", "id": 1, "ids": [2, 3, 5]],
            ["op": "roots", "ids": [1]],
        ])
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        until("UIKit holds the three routes") { nav.viewControllers.count == 3 && nav.transitionCoordinator == nil }
        return Stack(session: session, nav: nav, apply: apply)
    }

    private func frame(_ id: Int) -> [String: Any] { ["op": "frame", "id": id, "x": 0.0, "y": 0.0, "w": 402.0, "h": 874.0] }

    private func back(_ id: Int, in route: Int) -> [[String: Any]] {
        [["op": "create", "id": id, "kind": "button", "handlers": ["press"], "props": ["id": "back"]],
         ["op": "frame", "id": id, "x": 0.0, "y": 0.0, "w": 80.0, "h": 44.0],
         ["op": "children", "id": route, "ids": [id]]]
    }

    /// An interactive pop of the top route, its animation started. Once the
    /// test finishes or cancels it, the stack Exact writes next animates on
    /// its own (`interactive` false), not under this interaction.
    private func swipe(_ s: Stack) -> InteractivePop {
        let pop = InteractivePop()
        s.session.presenter.navigationHost.setAppDelegate(s.nav, pop)
        pop.interactive = true
        let destination = s.nav.viewControllers[s.nav.viewControllers.count - 2]
        s.nav.popViewController(animated: true)
        until("UIKit's pop animates the route beneath") { destination.viewIfLoaded?.window != nil }
        pop.interaction.update(0.4)
        return pop
    }

    /// Log's key moves while the finger is down (a route that redirects as
    /// it loads): the source rule no longer names it, and the app still goes
    /// back once UIKit's pop is over.
    func testAFinishedPopTakesTheAppBackAfterTheSourcesKeyMoved() throws {
        guard hasScene else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let s = try stack("pop-key-moved")
        var pressed: [UInt32] = []
        s.session.presenter.onPress = { id in
            pressed.append(id)
            s.apply([["op": "children", "id": 1, "ids": [2, 3]], ["op": "destroy", "id": 6], ["op": "destroy", "id": 5],
                     ["op": "props", "id": 1, "set": ["navigationKey": "updates"]]])
        }
        let pop = swipe(s)
        s.apply([["op": "props", "id": 5, "set": ["navigationKey": "log-2"]], ["op": "props", "id": 1, "set": ["navigationKey": "log-2"]]])
        pop.interaction.finish()
        pop.interactive = false
        until("the pop lands on Updates") { s.nav.transitionCoordinator == nil && s.nav.viewControllers.count == 2 && s.key == "updates" }
        XCTAssertEqual(pressed, [6], "Log's Back, once")
        XCTAssertEqual(s.session.presenter.navigationHost.selectedKey, "updates")
        s.session.presenter.navigationHost.setAppDelegate(s.nav, nil)
    }

    /// The app writes Log anew, under the same key, during a pop UIKit's
    /// back button started: the source's node is gone, and the app still goes
    /// back once UIKit's pop is over, through the new Log's Back.
    func testAFinishedBackButtonPopTakesTheAppBackAfterTheSourceWasWrittenAnew() throws {
        guard hasScene else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let s = try stack("pop-source-anew")
        var pressed: [UInt32] = []
        s.session.presenter.onPress = { id in
            pressed.append(id)
            s.apply([["op": "children", "id": 1, "ids": [2, 3]], ["op": "destroy", "id": 12], ["op": "destroy", "id": 11],
                     ["op": "props", "id": 1, "set": ["navigationKey": "updates"]]])
        }
        s.nav.popViewController(animated: true)
        XCTAssertNotNil(s.nav.transitionCoordinator, "the app writes Log anew before UIKit's pop is over")
        s.apply([["op": "create", "id": 11, "kind": "view", "props": ["navigationKey": "log"]], frame(11)] + back(12, in: 11) + [
            ["op": "children", "id": 1, "ids": [2, 3, 11]], ["op": "destroy", "id": 6], ["op": "destroy", "id": 5],
        ])
        until("the pop lands on Updates") { s.nav.transitionCoordinator == nil && s.nav.viewControllers.count == 2 && s.key == "updates" }
        XCTAssertEqual(pressed, [12], "the new Log's Back, once")
        XCTAssertEqual(s.session.presenter.navigationHost.selectedKey, "updates")
    }

    /// The app puts Draft under Log while the finger is down: the swipe took
    /// Log alone, so the app goes back through Log's own Back, to Draft, not
    /// by a history jump past Draft to the Updates UIKit shows.
    func testASwipeGoesBackThroughTheSourcesBackAfterTheAppInsertedARouteUnderIt() throws {
        guard hasScene else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let s = try stack("pop-inserted-under")
        var pressed: [UInt32] = []
        s.session.presenter.onPress = { id in
            pressed.append(id)
            s.apply([["op": "children", "id": 1, "ids": [2, 3, 9]], ["op": "destroy", "id": 6], ["op": "destroy", "id": 5],
                     ["op": "props", "id": 1, "set": ["navigationKey": "draft"]]])
        }
        let pop = swipe(s)
        s.apply([["op": "create", "id": 9, "kind": "view", "props": ["navigationKey": "draft"]], frame(9)] + back(10, in: 9) + [
            ["op": "children", "id": 1, "ids": [2, 3, 9, 5]],
        ])
        pop.interaction.finish()
        pop.interactive = false
        until("Draft shows over Updates") { s.nav.transitionCoordinator == nil && s.nav.viewControllers.count == 3 && s.key == "draft" }
        XCTAssertEqual(pressed, [6], "Log's Back, once")
        s.session.presenter.navigationHost.setAppDelegate(s.nav, nil)
    }

    /// The app replaces Log in place with Day while the finger is down, and
    /// the swipe is cancelled: UIKit popped nothing, so no Back reaches the
    /// app, and Day takes Log's place.
    func testACancelledPopDispatchesNoBackAfterTheSourceWasReplaced() throws {
        guard hasScene else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let s = try stack("pop-cancelled-replaced")
        var pressed: [UInt32] = []
        s.session.presenter.onPress = { pressed.append($0) }
        let pop = swipe(s)
        s.apply([["op": "create", "id": 7, "kind": "view", "props": ["navigationKey": "day"]], frame(7)] + back(8, in: 7) + [
            ["op": "children", "id": 1, "ids": [2, 3, 7]], ["op": "destroy", "id": 6], ["op": "destroy", "id": 5],
            ["op": "props", "id": 1, "set": ["navigationKey": "day"]],
        ])
        pop.interaction.cancel()
        pop.interactive = false
        until("Day takes Log's place") { s.nav.transitionCoordinator == nil && s.nav.viewControllers.count == 3 && s.key == "day" }
        XCTAssertEqual(pressed, [], "a cancelled pop performed no Back")
        s.session.presenter.navigationHost.setAppDelegate(s.nav, nil)
    }

    /// UIKit pops to Settings while the app selects Updates again, a route
    /// it retained: the pop took Updates, so the app goes back from it
    /// rather than UIKit pushing it again.
    func testAPopToTheRootTakesARouteTheAppSelectedAgainMeanwhile() throws {
        guard hasScene else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let s = try stack("pop-retained-selected")
        var pressed: [UInt32] = []
        s.session.presenter.onPress = { id in
            pressed.append(id)
            s.apply([["op": "props", "id": 1, "set": ["navigationKey": "settings"]]])
        }
        s.nav.popToRootViewController(animated: true)
        XCTAssertNotNil(s.nav.transitionCoordinator, "the app selects Updates before UIKit's pop is over")
        s.apply([["op": "props", "id": 1, "set": ["navigationKey": "updates"]]])
        until("the pop lands on Settings") { s.nav.transitionCoordinator == nil && s.nav.viewControllers.count == 1 && s.key == "settings" }
        XCTAssertEqual(pressed, [4], "Updates' Back, once")
        XCTAssertEqual(s.session.presenter.navigationHost.selectedKey, "settings")
    }

    /// The app goes back from Log, a route it retains, and then selects it
    /// again: Exact took Log out for the app, so Log selected again is a
    /// request Exact writes, and no pop of UIKit's to follow.
    func testARouteTheAppWentBackFromIsWrittenWhenSelectedAgain() throws {
        guard hasScene else { throw missing("scene", "UIKit animates the pop and the push Exact writes only in a window a scene holds") }
        let s = try stack("app-back-retained")
        var pressed: [UInt32] = []
        s.session.presenter.onPress = { pressed.append($0) }
        s.apply([["op": "props", "id": 1, "set": ["navigationKey": "updates"]]])
        until("Exact takes Log out") { s.nav.transitionCoordinator == nil && s.nav.viewControllers.count == 2 && s.key == "updates" }
        s.apply([["op": "props", "id": 1, "set": ["navigationKey": "log"]]])
        until("Exact pushes Log again") { s.nav.transitionCoordinator == nil && s.nav.viewControllers.count == 3 && s.key == "log" }
        waitForMainQueue()
        XCTAssertEqual(pressed, [], "no Back")
    }

    /// LLP 1057.001 §1 rules 1–2 (expo/exact#1843): past the 20 pt leading edge,
    /// a pointer or `pan` hit chain whose intersected `touch-action` (through the
    /// nearest scroll container inclusive) excludes the pop direction keeps iOS's
    /// content pop from beginning; the leading edge, plain boxes, `panrelease`
    /// alone, inert subtrees, and pop-allowing actions keep popping.
    func testContentPopRespectsIntersectedTouchActionOnPointerAndPanHitChains() throws {
        let s = try stack("pop-touch-action")
        let pop: UIGestureRecognizer
        if #available(iOS 26.0, *), let contentPop = s.nav.interactiveContentPopGestureRecognizer {
            pop = contentPop
        } else {
            pop = try XCTUnwrap(s.nav.interactivePopGestureRecognizer)
        }
        let host = s.session.presenter.navigationHost
        let inside = CGPoint(x: 160, y: 300), edge = CGPoint(x: 1, y: 300)
        let rightward = CGPoint(x: 400, y: 0), leftward = CGPoint(x: -400, y: 0)

        func setPad(parentAction: String?, parentHandlers: [String], childAction: String? = nil, childHandlers: [String] = [],
                    parentDisabled: Bool = false, parentInert: Bool = false, parentOverflowY: String? = nil, rootAction: String? = nil) {
            var rootStyle: [String: Any] = [:]
            if let rootAction { rootStyle["touch_action"] = rootAction }
            var parentStyle: [String: Any] = ["background_color": [136, 170, 255, 255]]
            if let parentAction { parentStyle["touch_action"] = parentAction }
            if let parentOverflowY { parentStyle["overflow_y"] = parentOverflowY }
            var parentProps: [String: Any] = [:]
            if parentDisabled { parentProps["disabled"] = "true" }
            if parentInert { parentProps["inert"] = "true" }
            var childStyle: [String: Any] = [:]
            if let childAction { childStyle["touch_action"] = childAction }
            s.apply([
                ["op": "style", "id": 1, "style": rootStyle],
                ["op": "destroy", "id": 21], ["op": "destroy", "id": 20],
                ["op": "create", "id": 20, "kind": "view", "handlers": parentHandlers, "props": parentProps, "style": parentStyle],
                ["op": "frame", "id": 20, "x": 0.0, "y": 100.0, "w": 402.0, "h": 500.0],
                ["op": "create", "id": 21, "kind": "view", "handlers": childHandlers, "style": childStyle],
                ["op": "frame", "id": 21, "x": 0.0, "y": 0.0, "w": 402.0, "h": 500.0],
                ["op": "children", "id": 20, "ids": [21]],
                ["op": "children", "id": 5, "ids": [6, 20]],
            ])
        }

        // Actions that exclude a rightward finger pan refuse content pop past 20 pt, while edge stays pop's.
        for action in ["pan-y", "none", "pan-right", "pan-up", "pan-down", "pinch-zoom"] {
            setPad(parentAction: action, parentHandlers: ["pointermove"])
            XCTAssertFalse(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "\(action) past edge")
            XCTAssertFalse(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: .zero), "\(action) zero vel past edge")
            XCTAssertTrue(host.popMayBegin(pop, from: edge, in: s.nav.view, velocity: rightward), "\(action) at leading edge")
        }

        // Actions that permit a rightward finger pan (or absent) still allow content pop.
        for action: String? in [nil, "auto", "manipulation", "pan-x", "pan-left", "pan-x pan-y"] {
            setPad(parentAction: action, parentHandlers: ["pointermove"])
            XCTAssertTrue(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "\(action ?? "nil") past edge")
        }

        // Without a pointer or pan listener, touch-action alone does not block content pop.
        setPad(parentAction: "pan-y", parentHandlers: [])
        XCTAssertTrue(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward))

        // `panrelease` alone installs no recognizer and does not block content pop.
        setPad(parentAction: "pan-y", parentHandlers: ["panrelease"])
        XCTAssertTrue(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "panrelease alone")

        // Disabled listener does not block, but an enabled pointer child under a disabled `pan-y` parent still receives pointermove.
        setPad(parentAction: "pan-y", parentHandlers: ["pointermove"], parentDisabled: true)
        XCTAssertTrue(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "disabled pointer listener")
        setPad(parentAction: "pan-y", parentHandlers: [], childHandlers: ["pointermove"], parentDisabled: true)
        XCTAssertFalse(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "disabled pan-y ancestor + enabled pointermove child")

        // Inert ancestor suppresses pointer delivery and lets content pop proceed.
        setPad(parentAction: "pan-y", parentHandlers: [], childHandlers: ["pointermove"], parentInert: true)
        XCTAssertTrue(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "inert ancestor")

        // Intersected touch-action across ancestor and descendant in either direction blocks content pop
        // (including a bare non-auto touch-action child with no handlers).
        setPad(parentAction: "pan-y", parentHandlers: [], childAction: nil, childHandlers: ["pointermove"])
        XCTAssertFalse(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "ancestor pan-y + child pointermove")
        setPad(parentAction: nil, parentHandlers: ["pointerdown"], childAction: "pan-y", childHandlers: [])
        XCTAssertFalse(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "ancestor pointerdown + child pan-y")

        // Layout pan with touch-action="pan-y" keeps content pop off past 20 pt.
        setPad(parentAction: "pan-y", parentHandlers: ["pan"])
        XCTAssertFalse(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "pan + pan-y")

        // Layout pan whose own touch-action gives rightward pan to the platform rejects
        // horizontal drags even when a descendant sets `touch-action: none`, so pop stays allowed.
        setPad(parentAction: "pan-x", parentHandlers: ["pan"], childAction: "none")
        XCTAssertTrue(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "pan + pan-x over none child yields to pop")

        // Layout pan refuses touches inside a nested text input, so pop is not suppressed over it.
        s.apply([
            ["op": "destroy", "id": 21], ["op": "destroy", "id": 20],
            ["op": "create", "id": 20, "kind": "view", "handlers": ["pan"], "style": ["touch_action": "none"]],
            ["op": "frame", "id": 20, "x": 0.0, "y": 100.0, "w": 402.0, "h": 500.0],
            ["op": "create", "id": 21, "kind": "input", "props": ["value": "abc"]],
            ["op": "frame", "id": 21, "x": 0.0, "y": 0.0, "w": 402.0, "h": 500.0],
            ["op": "children", "id": 20, "ids": [21]],
            ["op": "children", "id": 5, "ids": [6, 20]],
        ])
        XCTAssertTrue(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "pan over nested field yields to pop")

        // A childless `overflow_y: "hidden"` scroll container stays a NodeView (even without testId)
        // and stops ancestor `pan-y` from crossing into its `touch-action: auto` hit.
        for withTestId in [false, true] {
            var childProps: [String: Any] = [:]
            if withTestId { childProps["testId"] = "clip-box" }
            s.apply([
                ["op": "style", "id": 1, "style": ["touch_action": "pan-y"]],
                ["op": "destroy", "id": 21], ["op": "destroy", "id": 20],
                ["op": "create", "id": 20, "kind": "view", "handlers": ["pointermove"], "style": ["touch_action": "pan-y"]],
                ["op": "frame", "id": 20, "x": 0.0, "y": 100.0, "w": 402.0, "h": 500.0],
                ["op": "create", "id": 21, "kind": "view", "props": childProps, "style": ["overflow_y": "hidden", "touch_action": "auto"]],
                ["op": "frame", "id": 21, "x": 0.0, "y": 0.0, "w": 402.0, "h": 500.0],
                ["op": "children", "id": 20, "ids": [21]],
                ["op": "children", "id": 5, "ids": [6, 20]],
            ])
            XCTAssertFalse(s.session.presenter.flats.isFlat(21), "overflow:hidden box stays a NodeView (testId=\(withTestId))")
            XCTAssertTrue(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "overflow:hidden bounds touch-action (testId=\(withTestId))")
        }

        // `swiperight` with an explicit touch-action giving rightward pan to the platform lets content pop begin.
        setPad(parentAction: "pan-left", parentHandlers: ["swiperight"])
        XCTAssertTrue(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "swiperight + pan-left yields to pop")
        setPad(parentAction: "pan-y", parentHandlers: ["swiperight"])
        XCTAssertFalse(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "swiperight + pan-y keeps swipe")

        // Route logical parent's touch-action applies when no scroll container sits between them,
        // and stops at the nearest scroll container inclusive (Pointer Events §9.2).
        setPad(parentAction: nil, parentHandlers: [], childHandlers: ["pointermove"], rootAction: "pan-y")
        XCTAssertFalse(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "route logical parent pan-y applies")
        setPad(parentAction: "auto", parentHandlers: [], childHandlers: ["pointermove"], parentOverflowY: "scroll", rootAction: "pan-y")
        XCTAssertTrue(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "pan intersection stops at scroll container")
        setPad(parentAction: "pan-y", parentHandlers: [], childHandlers: ["pointermove"], parentOverflowY: "scroll", rootAction: "auto")
        XCTAssertFalse(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "scroll container's own pan-y applies")

        // Portal: declared parent's pointer listener pairs with destination's touch-action,
        // while a destination-only pointer listener does not suppress pop for a portal child.
        setPad(parentAction: "pan-y", parentHandlers: [])
        s.apply([
            ["op": "destroy", "id": 23], ["op": "destroy", "id": 22],
            ["op": "create", "id": 22, "kind": "view", "handlers": ["pointermove"]],
            ["op": "frame", "id": 22, "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0],
            ["op": "create", "id": 23, "kind": "view", "props": ["declaredParent": "22"]],
            ["op": "frame", "id": 23, "x": 0.0, "y": 0.0, "w": 402.0, "h": 500.0],
            ["op": "children", "id": 20, "ids": [23]],
            ["op": "children", "id": 5, "ids": [6, 20, 22]],
        ])
        let dest = try XCTUnwrap(s.session.presenter.views[20])
        let declParent = try XCTUnwrap(s.session.presenter.views[22])
        XCTAssertFalse(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "portal declared pointermove + destination pan-y")
        declParent.handlers = []
        dest.handlers = ["pointermove"]
        XCTAssertTrue(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: rightward), "portal ignores destination-only pointermove")
        s.apply([["op": "destroy", "id": 23], ["op": "destroy", "id": 22]])

        // RTL: leading edge is at x = width - 1 and pop moves leftward (`pan-right` allows, `pan-left` blocks).
        s.nav.view.semanticContentAttribute = .forceRightToLeft
        defer { s.nav.view.semanticContentAttribute = .unspecified }
        let rtlEdge = CGPoint(x: s.nav.view.bounds.width - 1, y: 300)
        setPad(parentAction: "pan-y", parentHandlers: ["pointermove"])
        XCTAssertTrue(host.popMayBegin(pop, from: rtlEdge, in: s.nav.view, velocity: .zero), "RTL leading edge zero vel")
        XCTAssertTrue(host.popMayBegin(pop, from: rtlEdge, in: s.nav.view, velocity: leftward), "RTL leading edge leftward")
        XCTAssertFalse(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: leftward), "RTL pan-y past edge")
        setPad(parentAction: "pan-left", parentHandlers: ["pointermove"])
        XCTAssertFalse(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: leftward), "RTL pan-left excludes leftward pop")
        setPad(parentAction: "pan-right", parentHandlers: ["pointermove"])
        XCTAssertTrue(host.popMayBegin(pop, from: inside, in: s.nav.view, velocity: leftward), "RTL pan-right permits leftward pop")
    }

    /// Real-touch iOS 26+ integration test for expo/exact#1843: dragging rightward
    /// past 20 pt over a `touch-action="pan-y"` box with `pointermove` delivers
    /// pointer moves and keeps the pushed route, while a plain box and a leading-
    /// edge drag still pop.
    func testContentPopRealTouchDeliversPointerMoveOverPanYAndKeepsRoute() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("interactiveContentPopGestureRecognizer is iOS 26+") }
        guard hasScene, let scene = hostedWindowScene() else { throw missing("scene", "real touches need a window scene") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "build.mjs --test --ios starts the XCTest touch runner")
        }
        let touches = try TouchRunner(self)
        defer { touches.connection.cancel() }
        let s = try stack("pop-touch-action-live")
        _ = try XCTUnwrap(s.nav.interactiveContentPopGestureRecognizer, "iOS 26+ content pop recognizer")
        var popped: [UInt32] = []
        s.session.presenter.onPress = { id in
            popped.append(id)
            s.apply([["op": "children", "id": 1, "ids": [2, 3]], ["op": "destroy", "id": 20], ["op": "destroy", "id": 6], ["op": "destroy", "id": 5],
                     ["op": "props", "id": 1, "set": ["navigationKey": "updates"]]])
        }
        var moves = 0
        s.session.presenter.onPointer = { _, kind, _ in if kind == .move { moves += 1 } }
        s.apply([
            ["op": "create", "id": 20, "kind": "view", "handlers": ["pointerdown", "pointermove", "pointerup"],
             "style": ["touch_action": "pan-y", "background_color": [136, 170, 255, 255]]],
            ["op": "frame", "id": 20, "x": 0.0, "y": 140.0, "w": 402.0, "h": 420.0],
            ["op": "children", "id": 5, "ids": [6, 20]],
        ])
        let window = try XCTUnwrap(self.window)
        window.layoutIfNeeded()
        func drag(from start: CGPoint, to end: CGPoint) throws {
            let fixed = window.screen.fixedCoordinateSpace
            let a = fixed.convert(start, from: scene.coordinateSpace), b = fixed.convert(end, from: scene.coordinateSpace)
            let reply = try touches.ask(["op": "drag", "point": [start.x, start.y], "to": [end.x, end.y],
                "screen": [a.x, a.y], "screenTo": [b.x, b.y], "display": [window.screen.bounds.width, window.screen.bounds.height],
                "press": 0.02, "hold": 0.0, "velocity": 600.0])
            XCTAssertEqual(reply["done"] as? Bool, true)
        }
        // 1. Rightward drag past 20 pt over touch-action="pan-y" + pointermove keeps route and delivers moves.
        try drag(from: CGPoint(x: 100, y: 320), to: CGPoint(x: 340, y: 320))
        until("pointer moves arrived") { moves > 0 && s.nav.transitionCoordinator == nil }
        XCTAssertEqual(s.key, "log", "pan-y + pointermove past 20 pt must not pop")
        XCTAssertEqual(s.nav.viewControllers.count, 3)
        XCTAssertEqual(popped, [])

        // 2. Leading-edge drag from x = 4 over the same pan-y + pointermove box still pops (LLP 1057.001 rule 1).
        try drag(from: CGPoint(x: 4, y: 320), to: CGPoint(x: 320, y: 320))
        until("leading-edge swipe pops") { s.nav.transitionCoordinator == nil && s.nav.viewControllers.count == 2 && s.key == "updates" }
        XCTAssertEqual(popped, [6])
    }
}
#endif
