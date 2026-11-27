#if os(iOS)
import UIKit
import UIKit.UIGestureRecognizerSubclass
import XCTest
@testable import ExactKit

/// LLP 1057 §10.6 on UIKit: `layoutPanning` releases once, after the final
/// delta, at UIKit's own velocity; a cancelled pan at rest. Recognizer phases
/// are set by the test (UIKit synthesizes no touches for a unit test).
final class PanReleaseIOSTests: XCTestCase {
    private var window: UIWindow!

    private final class Pan: UIPanGestureRecognizer {
        var phase = UIGestureRecognizer.State.possible
        var moved = CGPoint.zero, speed = CGPoint.zero
        override var state: UIGestureRecognizer.State { get { phase } set { phase = newValue } }
        override func translation(in view: UIView?) -> CGPoint { moved }
        override func velocity(in view: UIView?) -> CGPoint { speed }
    }

    private func host(_ handlers: [String]) -> (Presenter, NodeView) {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "handlers": handlers],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 200.0]
        ]))
        window.makeKeyAndVisible()
        return (p, p.views[1]!)
    }

    private func drive(_ node: NodeView, _ steps: [(UIGestureRecognizer.State, CGFloat)], speed: CGPoint = .zero) {
        let pan = Pan()
        pan.speed = speed
        for (state, x) in steps { pan.phase = state; pan.moved = CGPoint(x: x, y: 0); node.layoutPanning(pan) }
    }

    func testAnEndedPanReleasesOnceAfterItsFinalDeltaAtUIKitsVelocity() throws {
        let (p, node) = host(["pan", "panrelease"])
        XCTAssertNotNil(node.layoutPanRecognizer)
        var log: [String] = []
        p.onPan = { _, dx, _ in log.append("pan \(dx)") }
        p.onPanRelease = { id, vx, vy in log.append("release \(id) \(vx) \(vy)") }
        drive(node, [(.began, 10), (.changed, 30), (.ended, 45)], speed: CGPoint(x: 800, y: -20))
        XCTAssertEqual(log, ["pan 10.0", "pan 20.0", "pan 15.0", "release 1 800.0 -20.0"])
    }

    func testACancelledPanReleasesAtRest() {
        let (p, node) = host(["pan", "panrelease"])
        var releases: [[Double]] = []
        p.onPanRelease = { _, vx, vy in releases.append([vx, vy]) }
        drive(node, [(.began, 10), (.changed, 20), (.cancelled, 20)], speed: CGPoint(x: 500, y: 0))
        XCTAssertEqual(releases, [[0, 0]])
        drive(node, [(.began, 10), (.failed, 10)])
        drive(node, [(.failed, 0)])
        XCTAssertEqual(releases, [[0, 0], [0, 0]])
    }

    func testANodeWithoutPanreleaseHearsNone() {
        let (p, node) = host(["pan"])
        var pans = 0, releases = 0
        p.onPan = { _, _, _ in pans += 1 }
        p.onPanRelease = { _, _, _ in releases += 1 }
        drive(node, [(.began, 10), (.ended, 30)], speed: CGPoint(x: 300, y: 0))
        XCTAssertEqual(pans, 2)
        XCTAssertEqual(releases, 0)
    }
    private func admits(_ node: NodeView, action: String?, velocity: CGPoint, translation: CGPoint = .zero) -> Bool {
        node.style["touch_action"] = action.map(BatchValue.string)
        let pan = Pan()
        pan.speed = velocity; pan.moved = translation
        node.layoutPanRecognizer = pan
        return node.gestureRecognizerShouldBegin(pan)
    }

    func testPanYLeavesVerticalContactsToNativeScrolling() {
        let (presenter, node) = host(["pan", "panrelease"])
        withExtendedLifetime(presenter) {
            for y: CGFloat in [-200, 200] {
                XCTAssertFalse(admits(node, action: "pan-y", velocity: CGPoint(x: 12, y: y)))
                XCTAssertTrue(admits(node, action: "pan-y", velocity: CGPoint(x: y, y: 12)))
            }
        }
    }

    func testPanXLeavesHorizontalContactsToNativeScrolling() {
        let (presenter, node) = host(["pan", "panrelease"])
        withExtendedLifetime(presenter) {
            for x: CGFloat in [-200, 200] {
                XCTAssertFalse(admits(node, action: "pan-x", velocity: CGPoint(x: x, y: 12)))
                XCTAssertTrue(admits(node, action: "pan-x", velocity: CGPoint(x: 12, y: x)))
            }
        }
    }

    func testDirectionalAndCombinedTouchActionsKeepTheirNativeDirections() {
        let (presenter, node) = host(["pan", "panrelease"])
        withExtendedLifetime(presenter) {
            for (action, native) in [("pan-left", CGPoint(x: 200, y: 0)), ("pan-right", CGPoint(x: -200, y: 0)),
                                     ("pan-up", CGPoint(x: 0, y: 200)), ("pan-down", CGPoint(x: 0, y: -200))] {
                XCTAssertFalse(admits(node, action: action, velocity: native), action)
                XCTAssertTrue(admits(node, action: action, velocity: CGPoint(x: -native.x, y: -native.y)), action)
            }
            XCTAssertFalse(admits(node, action: "pan-x pan-y", velocity: CGPoint(x: 200, y: 0)))
            XCTAssertFalse(admits(node, action: "pan-y pinch-zoom", velocity: CGPoint(x: 0, y: 200)))
            XCTAssertTrue(admits(node, action: "pan-y pinch-zoom", velocity: CGPoint(x: 200, y: 0)))
        }
    }

    func testDefaultAndNoNativePanRetainExistingLayoutPanAdmission() {
        let (presenter, node) = host(["pan", "panrelease"])
        withExtendedLifetime(presenter) {
            for action: String? in [nil, "auto", "manipulation", "none", "pinch-zoom"] {
                XCTAssertTrue(admits(node, action: action, velocity: CGPoint(x: 200, y: 0)))
                XCTAssertTrue(admits(node, action: action, velocity: CGPoint(x: 0, y: 200)))
            }
        }
    }

    func testZeroVelocityUsesTranslationAndUnknownDirectionDoesNotFail() {
        let (presenter, node) = host(["pan", "panrelease"])
        withExtendedLifetime(presenter) {
            XCTAssertFalse(admits(node, action: "pan-y", velocity: .zero, translation: CGPoint(x: 0, y: 20)))
            XCTAssertTrue(admits(node, action: "pan-y", velocity: .zero, translation: CGPoint(x: 20, y: 0)))
            XCTAssertTrue(admits(node, action: "pan-y", velocity: .zero))
        }
    }

    private final class DirectTouch: UITouch {
        var point: CGPoint
        var currentPhase: UITouch.Phase
        weak var hit: UIView?
        init(_ point: CGPoint, phase: UITouch.Phase = .began, view: UIView? = nil) {
            self.point = point; self.currentPhase = phase; self.hit = view
        }
        override var type: UITouch.TouchType { .direct }
        override var phase: UITouch.Phase { currentPhase }
        override var view: UIView? { hit }
        override func location(in view: UIView?) -> CGPoint { point }
    }

    func testReportingPinchObservesAlongsideTheScrollPan() throws {
        let (_, node) = host(["pinchrelease"])
        let pinch = try XCTUnwrap(node.extras?.reportingPinch)
        let indirect = try XCTUnwrap(node.extras?.reportingIndirectPinch)
        let scroll = UIScrollView().panGestureRecognizer
        XCTAssertTrue(pinch.idle)
        XCTAssertTrue(indirect.idle)
        XCTAssertFalse(pinch.cancelsTouchesInView)
        XCTAssertFalse(pinch.delaysTouchesBegan)
        XCTAssertFalse(pinch.delaysTouchesEnded)
        XCTAssertFalse(pinch.canPrevent(scroll))
        XCTAssertFalse(pinch.canBePrevented(by: scroll))
        XCTAssertTrue(pinch.gestureRecognizer(pinch, shouldRecognizeSimultaneouslyWith: scroll))
        XCTAssertTrue(indirect.allowedTouchTypes.isEmpty)
        XCTAssertFalse(indirect.gestureRecognizer(indirect, shouldReceive: DirectTouch(CGPoint(x: 100, y: 100), view: node)))
        node.handlers = ["pinch"]
        XCTAssertTrue(node.extras?.reportingPinch === pinch)
        XCTAssertTrue(node.extras?.reportingIndirectPinch === indirect)
        node.handlers = []
        XCTAssertNil(pinch.view)
        XCTAssertNil(indirect.view)
    }

    func testReportingPinchRePairsWithSurvivingOrRestingThirdFingerAndHandlesMidPinchCancel() throws {
        let (presenter, node) = host(["pinch", "pinchrelease"])
        var events: [(UInt32, Double, Double, Double)] = []
        presenter.onClipboard = { _, kind, payload in
            let parts = payload.split(separator: ",").compactMap { Double($0) }
            if parts.count == 3 { events.append((kind, parts[0], parts[1], parts[2])) }
        }
        let pinch = try XCTUnwrap(node.extras?.reportingPinch)
        let event = UIEvent()

        // 1. Staggered lift then a new finger beside the survivor forms a second pinch.
        let f1 = DirectTouch(CGPoint(x: 100, y: 100), view: node)
        let f2 = DirectTouch(CGPoint(x: 200, y: 100), view: node)
        XCTAssertTrue(pinch.gestureRecognizer(pinch, shouldReceive: f1))
        XCTAssertTrue(pinch.gestureRecognizer(pinch, shouldReceive: f2))
        pinch.touchesBegan([f1, f2], with: event)
        XCTAssertFalse(pinch.idle)
        f1.point = CGPoint(x: 50, y: 100); f1.currentPhase = .moved
        f2.point = CGPoint(x: 250, y: 100); f2.currentPhase = .moved
        pinch.touchesMoved([f1, f2], with: event)
        XCTAssertEqual(events.last?.0, 67)
        XCTAssertEqual(events.last?.1 ?? 0, 2.0, accuracy: 0.01)
        f1.currentPhase = .ended
        pinch.touchesEnded([f1], with: event)
        XCTAssertEqual(events.filter { $0.0 == 68 }.count, 1)
        XCTAssertEqual(events.last?.1 ?? 0, 2.0, accuracy: 0.01)
        // New finger lands beside still-down f2 at (250, 100).
        let f3 = DirectTouch(CGPoint(x: 350, y: 100), view: node)
        XCTAssertTrue(pinch.gestureRecognizer(pinch, shouldReceive: f3))
        pinch.touchesBegan([f3], with: event)
        f3.point = CGPoint(x: 400, y: 100); f3.currentPhase = .moved
        pinch.touchesMoved([f3], with: event)
        XCTAssertEqual(events.last?.0, 67)
        XCTAssertEqual(events.last?.1 ?? 0, 1.5, accuracy: 0.01)
        f2.currentPhase = .ended; f3.currentPhase = .ended
        pinch.touchesEnded([f2, f3], with: event)
        XCTAssertEqual(events.filter { $0.0 == 68 }.count, 2)
        XCTAssertEqual(events.last?.1 ?? 0, 1.5, accuracy: 0.01)
        pinch.reset()
        XCTAssertTrue(pinch.idle)

        // 2. A resting third finger is retained and pairs with a fourth finger after the first pair lifts.
        events.removeAll()
        let a1 = DirectTouch(CGPoint(x: 100, y: 100), view: node)
        let a2 = DirectTouch(CGPoint(x: 200, y: 100), view: node)
        let a3 = DirectTouch(CGPoint(x: 150, y: 200), view: node)
        XCTAssertTrue(pinch.gestureRecognizer(pinch, shouldReceive: a3))
        pinch.touchesBegan([a1, a2, a3], with: event)
        a1.point = CGPoint(x: 50, y: 100); a2.point = CGPoint(x: 250, y: 100)
        pinch.touchesMoved([a1, a2], with: event)
        XCTAssertEqual(events.last?.0, 67)
        a1.currentPhase = .ended; a2.currentPhase = .ended
        pinch.touchesEnded([a1, a2], with: event)
        XCTAssertEqual(events.filter { $0.0 == 68 }.count, 1)
        let a4 = DirectTouch(CGPoint(x: 250, y: 200), view: node)
        pinch.touchesBegan([a4], with: event)
        a4.point = CGPoint(x: 350, y: 200)
        pinch.touchesMoved([a4], with: event)
        XCTAssertEqual(events.last?.0, 67)
        XCTAssertEqual(events.last?.1 ?? 0, 2.0, accuracy: 0.01)
        a3.currentPhase = .cancelled; a4.currentPhase = .cancelled
        pinch.touchesCancelled([a3, a4], with: event)
        XCTAssertEqual(events.filter { $0.0 == 68 }.count, 2)
        XCTAssertEqual(events.last?.1 ?? 0, 1.0, accuracy: 0.01)
        pinch.reset()

        // 3. Becoming ineligible mid-pinch emits one scale-1 pinchrelease and cancels.
        events.removeAll()
        let b1 = DirectTouch(CGPoint(x: 100, y: 100), view: node)
        let b2 = DirectTouch(CGPoint(x: 200, y: 100), view: node)
        pinch.touchesBegan([b1, b2], with: event)
        b1.point = CGPoint(x: 50, y: 100); b2.point = CGPoint(x: 250, y: 100)
        pinch.touchesMoved([b1, b2], with: event)
        XCTAssertEqual(events.last?.0, 67)
        node.props["disabled"] = "true"
        b2.point = CGPoint(x: 280, y: 100)
        pinch.touchesMoved([b2], with: event)
        let cancels = events.filter { $0.0 == 68 }
        XCTAssertEqual(cancels.count, 1)
        XCTAssertEqual(cancels[0].1, 1.0, accuracy: 0.01)
        node.props.removeValue(forKey: "disabled")
        b1.currentPhase = .ended; b2.currentPhase = .ended
        pinch.touchesEnded([b1, b2], with: event)
        XCTAssertEqual(events.filter { $0.0 == 68 }.count, 1)
        withExtendedLifetime(presenter) {}
    }

    func testReportingPinchNestedChildAddedMidTouchdownDoesNotBlockParent() throws {
        let presenter = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 320, height: 480))
        presenter.viewport.frame = window.bounds
        window.addSubview(presenter.viewport)
        presenter.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "handlers": ["pinch", "pinchrelease"]],
            ["op": "create", "id": 2, "kind": "view", "handlers": ["press"]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 320.0, "h": 480.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 320.0, "h": 480.0],
        ]))
        window.makeKeyAndVisible()
        let parent = try XCTUnwrap(presenter.views[1])
        let child = try XCTUnwrap(presenter.views[2])
        let parentPinch = try XCTUnwrap(parent.extras?.reportingPinch)
        let event = UIEvent()
        let f1 = DirectTouch(CGPoint(x: 100, y: 100), view: child)
        parentPinch.touchesBegan([f1], with: event)
        // Add a pinch listener on the child after the first touchdown.
        child.handlers = ["press", "pinch", "pinchrelease"]
        let childPinch = try XCTUnwrap(child.extras?.reportingPinch)
        let f2 = DirectTouch(CGPoint(x: 200, y: 100), view: child)
        childPinch.touchesBegan([f2], with: event)
        parentPinch.touchesBegan([f2], with: event)
        // Child only saw f2, so parent must still be allowed to begin.
        XCTAssertTrue(parentPinch.gestureRecognizerShouldBegin(parentPinch))
        // Once child actually tracks the same pair, parent yields to child.
        childPinch.touchesBegan([f1], with: event)
        XCTAssertFalse(parentPinch.gestureRecognizerShouldBegin(parentPinch))
        withExtendedLifetime(presenter) {}
    }

    func testReportingPinchDeliveryRechecksEligibilityAndIndirectControlBoundary() throws {
        let presenter = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 320, height: 480))
        presenter.viewport.frame = window.bounds
        window.addSubview(presenter.viewport)
        presenter.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "handlers": ["pinch", "pinchrelease"]],
            ["op": "create", "id": 2, "kind": "control"],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 320.0, "h": 480.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 320.0, "h": 480.0],
        ]))
        window.makeKeyAndVisible()
        let parent = try XCTUnwrap(presenter.views[1])
        let control = try XCTUnwrap(presenter.views[2])
        var delivered: [(UInt32, String)] = []
        presenter.onClipboard = { _, kind, payload in delivered.append((kind, payload)) }
        presenter.pinch(parent, ended: false, scale: 1.5, x: 10, y: 20)
        XCTAssertEqual(delivered.count, 1)
        // Disabling the node blocks further pinch/pinchrelease delivery.
        parent.props["disabled"] = "true"
        presenter.pinch(parent, ended: false, scale: 1.7, x: 10, y: 20)
        presenter.pinch(parent, ended: true, scale: 1.7, x: 10, y: 20)
        XCTAssertEqual(delivered.count, 1)
        // Indirect pinch over a nested control is refused; over the parent itself it begins
        // even when a direct finger is resting on ReportingPinch.
        parent.props.removeValue(forKey: "disabled")
        let pinch = try XCTUnwrap(parent.extras?.reportingPinch)
        let indirect = try XCTUnwrap(parent.extras?.reportingIndirectPinch)
        pinch.touchesBegan([DirectTouch(CGPoint(x: 100, y: 100), view: parent)], with: UIEvent())
        XCTAssertFalse(indirect.gestureRecognizerShouldBegin(indirect))
        control.removeFromSuperview()
        XCTAssertTrue(indirect.gestureRecognizerShouldBegin(indirect))
        withExtendedLifetime(presenter) {}
    }

    func testPinchLiftSuppressesStaggeredLiftFlingButAllowsMovingSurvivorOrThirdFinger() {
        let scroll = UIScrollView(frame: CGRect(x: 0, y: 0, width: 320, height: 480))
        scroll.contentSize = CGSize(width: 320, height: 2000)
        scroll.contentOffset = CGPoint(x: 0, y: 400)

        let lift = PinchLift()
        PinchLift.attach(lift, to: scroll)
        // Active pinch is not cleared by a concurrent scrollViewWillBeginDragging.
        PinchLift.clearIfEnded(for: scroll)
        // Simultaneous lift on any UIScrollView (including grouped lists/viewports): anti-fling holds.
        lift.endPair(scrolls: [scroll])
        let endedPan = Pan()
        endedPan.phase = .ended
        lift.panChanged(endedPan)
        XCTAssertTrue(PinchLift.consume(for: scroll))
        // Consumed once.
        XCTAssertFalse(PinchLift.consume(for: scroll))

        // Staggered lift with stationary survivor: anti-fling holds until noteMoved() fires.
        let staggered = PinchLift()
        staggered.endPair(scrolls: [scroll])
        let activePan = Pan()
        activePan.phase = .changed
        staggered.panChanged(activePan)
        XCTAssertTrue(staggered.endsPinch(for: scroll))
        staggered.noteMoved()
        XCTAssertFalse(staggered.endsPinch(for: scroll))

        // Third finger staying still keeps anti-fling; moving scroll past jitter restores flinging.
        let third = PinchLift()
        PinchLift.attach(third, to: scroll)
        third.endPair(scrolls: [scroll])
        scroll.panGestureRecognizer.state = .possible
        third.panChanged(activePan)
        XCTAssertTrue(third.endsPinch(for: scroll))
        scroll.contentOffset = CGPoint(x: 0, y: 520)
        XCTAssertFalse(third.endsPinch(for: scroll))
        // An ended lift left over without a willEndDragging is cleared on the next scroll start.
        PinchLift.clearIfEnded(for: scroll)
        XCTAssertFalse(PinchLift.consume(for: scroll))
    }

    private func hostedPinchSurface(_ scene: UIWindowScene) throws -> (UIWindow, Presenter, ScrollView, () -> [(UInt32, Double, Double, Double)], () -> Void) {
        let p = Presenter()
        let window = UIWindow(windowScene: scene), host = UIViewController()
        window.rootViewController = host
        window.makeKeyAndVisible()
        let size = host.view.bounds.size
        p.viewport.frame = host.view.bounds
        host.view.addSubview(p.viewport)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "style": ["overflow_y": "scroll"], "handlers": ["pinch", "pinchrelease"]],
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": size.width, "h": size.height],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": size.width, "h": size.height],
            ["op": "content", "id": 2, "w": size.width, "h": 4000.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": size.width, "h": 4000.0],
        ]))
        let scroll = try XCTUnwrap(p.views[2]?.scroll)
        scroll.setContentOffset(CGPoint(x: 0, y: 600), animated: false)
        var events: [(UInt32, Double, Double, Double)] = []
        p.onClipboard = { _, kind, payload in
            let parts = payload.split(separator: ",").compactMap { Double($0) }
            if parts.count == 3 { events.append((kind, parts[0], parts[1], parts[2])) }
        }
        waitFor("the pinch window receives touches") { Agent.frontRefusal(window) == nil }
        return (window, p, scroll, { events }, { events.removeAll() })
    }

    private func fixed(_ point: CGPoint, in window: UIWindow) throws -> [Double] {
        let scene = try XCTUnwrap(window.windowScene)
        let p = window.screen.fixedCoordinateSpace.convert(point, from: scene.coordinateSpace)
        return [Double(p.x), Double(p.y)]
    }

    func testHostedPinchReportsScaleFocalPointAndSuppressesFling() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "real touches reach UIKit") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "injected pinches reach UIPinchGestureRecognizer and UIScrollView")
        }
        let touch = try TouchRunner(self)
        let (window, p, scroll, events, clear) = try hostedPinchSurface(scene)
        defer {
            touch.connection.cancel()
            window.isHidden = true
            window.rootViewController = nil
            window.windowScene = nil
            withExtendedLifetime(p) {}
        }
        let cx = window.bounds.midX, cy = window.bounds.midY

        // 1. Spread: span 100 -> 200 (~2x) reports pinch + one pinchrelease at pair midpoint, no post-pinch fling.
        XCTAssertEqual(try touch.ask([
            "op": "pinch",
            "screen": [try fixed(CGPoint(x: cx - 50, y: cy), in: window), try fixed(CGPoint(x: cx + 50, y: cy), in: window)],
            "screenTo": [try fixed(CGPoint(x: cx - 100, y: cy), in: window), try fixed(CGPoint(x: cx + 100, y: cy), in: window)],
            "over": 0.25,
        ])["done"] as? Bool, true)
        waitFor("spread pinchrelease arrives", cap: 5) { events().contains { $0.0 == 68 } }
        let spreadMoves = events().filter { $0.0 == 67 }
        let spreadReleases = events().filter { $0.0 == 68 }
        XCTAssertGreaterThan(spreadMoves.count, 0)
        XCTAssertEqual(spreadReleases.count, 1)
        XCTAssertEqual(spreadReleases[0].1, 2.0, accuracy: 0.15)
        XCTAssertEqual(spreadReleases[0].2, Double(cx), accuracy: 15)
        XCTAssertEqual(spreadReleases[0].3, Double(cy), accuracy: 15)
        let offsetAfterSpread = scroll.contentOffset.y
        RunLoop.main.run(until: Date().addingTimeInterval(0.25))
        XCTAssertFalse(scroll.isDecelerating, "pinch end must not start a scroll fling")
        XCTAssertEqual(scroll.contentOffset.y, offsetAfterSpread, accuracy: 2.0)

        // 2. Squeeze: span 200 -> 100 (~0.5x) reports one pinchrelease at ~0.5x.
        clear()
        XCTAssertEqual(try touch.ask([
            "op": "pinch",
            "screen": [try fixed(CGPoint(x: cx - 100, y: cy), in: window), try fixed(CGPoint(x: cx + 100, y: cy), in: window)],
            "screenTo": [try fixed(CGPoint(x: cx - 50, y: cy), in: window), try fixed(CGPoint(x: cx + 50, y: cy), in: window)],
            "over": 0.25,
        ])["done"] as? Bool, true)
        waitFor("squeeze pinchrelease arrives", cap: 5) { events().contains { $0.0 == 68 } }
        let squeezeReleases = events().filter { $0.0 == 68 }
        XCTAssertEqual(squeezeReleases.count, 1)
        XCTAssertEqual(squeezeReleases[0].1, 0.5, accuracy: 0.1)

        // 3. One-finger vertical drag still scrolls cleanly with zero pinch reports.
        clear()
        scroll.setContentOffset(CGPoint(x: 0, y: 600), animated: false)
        let beforeScroll = scroll.contentOffset.y
        var drag = try TouchRunner.aim(CGPoint(x: cx, y: cy + 120), in: window)
        let end = try TouchRunner.aim(CGPoint(x: cx, y: cy - 120), in: window)
        drag.merge(["op": "drag", "to": end["point"]!, "screenTo": end["screen"]!, "velocity": 800, "press": 0.02, "hold": 0.0]) { _, new in new }
        XCTAssertEqual(try touch.ask(drag)["done"] as? Bool, true)
        RunLoop.main.run(until: Date().addingTimeInterval(0.2))
        XCTAssertGreaterThan(scroll.contentOffset.y, beforeScroll + 80)
        XCTAssertTrue(events().isEmpty)
    }

}
#endif
