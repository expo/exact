#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactCollections

/// Hand-written swipe rows in a route's page (`host/apple/tests/fixtures/swipe-push.contract`),
/// a projected row and a painted one, swiped with real touches from the XCTest touch runner:
///   bun host/apple/build.mjs --test --ios --only SwipeRoutesIOSTests
/// UIKit's table, measured on the iOS 27.1 simulator, keeps a revealed row open through a
/// push and after the pop, and scrolls on a vertical drag that begins on a row.
final class SwipeRoutesIOSTests: XCTestCase {
    enum Kind: String, CaseIterable { case native, painted }

    private final class Screen {
        let session: ExactSession, window: UIWindow, touch: TouchRunner
        init(session: ExactSession, window: UIWindow, touch: TouchRunner) { self.session = session; self.window = window; self.touch = touch }
        var p: Presenter { session.presenter }
        func node(_ testId: String) throws -> NodeView { try XCTUnwrap(p.views.values.first { $0.props["testId"] == testId }, "no \(testId)") }
        func row(_ kind: Kind, on route: String = "start") throws -> NodeView { try node("\(route)-\(kind.rawValue)-row") }
        func isOpen(_ kind: Kind, on route: String = "start") -> Bool {
            guard let owner = try? row(kind, on: route) else { return false }
            if kind == .painted { return (owner.scrollView?.contentOffset.x ?? 0) > 1 }
            return p.swipeHost.cell(of: owner)?.configurationState.isSwiped == true
        }
        /// Whether the row's own motion is over: its scroll's, or its cell's.
        func atRest(_ kind: Kind) -> Bool {
            guard let owner = try? row(kind) else { return false }
            if kind == .painted { return !(owner.scrollView.map { $0.isDragging || $0.isDecelerating } ?? false) }
            func moving(_ layer: CALayer) -> Bool { !(layer.animationKeys() ?? []).isEmpty || (layer.sublayers ?? []).contains(where: moving) }
            var table = p.swipeHost.cell(of: owner)?.superview
            while let view = table, !(view is UITableView) { table = view.superview }
            return !(table.map { moving($0.layer) } ?? false)
        }
        func frame(_ kind: Kind, on route: String = "start") throws -> CGRect {
            let owner = try row(kind, on: route)
            return owner.convert(owner.bounds, to: window)
        }
        func drag(from: CGPoint, to: CGPoint, velocity: Double) throws {
            var drag = try TouchRunner.aim(from, in: window)
            let end = try TouchRunner.aim(to, in: window)
            drag.merge(["op": "drag", "to": end["point"]!, "screenTo": end["screen"]!, "press": 0.02, "hold": 0.0, "velocity": velocity]) { _, new in new }
            XCTAssertEqual(try touch.ask(drag)["done"] as? Bool, true)
        }
        func swipe(_ kind: Kind, on route: String = "start") throws {
            let r = try frame(kind, on: route)
            try drag(from: CGPoint(x: r.maxX - 30, y: r.midY), to: CGPoint(x: r.maxX - 200, y: r.midY), velocity: 500)
        }
        func facts(_ kind: Kind) -> String {
            "\(kind) at \((try? frame(kind)) ?? .null), open row: \(p.swipeHost.openRow.map(String.init) ?? "none"), page y: \((try? node("start-page"))?.scrollView?.contentOffset.y ?? -1)"
        }
    }

    private func screen(_ scene: UIWindowScene, _ label: String) throws -> Screen {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_SWIPE_PUSH_PLAN"], "build.mjs --test --ios compiles the fixtures' plans")
        let plan = try Data(contentsOf: URL(fileURLWithPath: path))
        let touch = try TouchRunner(self)
        let session = ExactApp.shared.makeSession(label: label)
        let view = ExactView(session: session), root = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        window.rootViewController = root
        window.makeKeyAndVisible()
        root.view.addSubview(view)
        view.frame = root.view.bounds
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.layoutIfNeeded()
        let s = Screen(session: session, window: window, touch: touch)
        waitFor("the rows are laid out", cap: 15) { (try? s.frame(.painted)).map { $0.height > 0 } ?? false }
        waitFor("the screen is in front", cap: 15) { Agent.frontRefusal(window) == nil }
        CATransaction.flush()
        waitForMainQueue()
        // After another test's window, the first real swipe on a projected
        // row can open nothing (QUEUE.md, "The first swipe on a second test
        // window"): one is swiped open first, and closed.
        for _ in 0..<3 where !s.isOpen(.native) {
            try s.swipe(.native)
            waitFor("native opens", cap: 3, onFailure: { _ in }) { s.isOpen(.native) }
        }
        try s.drag(from: CGPoint(x: 60, y: try s.frame(.native).midY), to: CGPoint(x: 240, y: try s.frame(.native).midY), velocity: 500)
        waitFor("native closes", cap: 5, facts: { s.facts(.native) }) { !s.isOpen(.native) }
        return s
    }

    private func close(_ s: Screen) {
        s.touch.connection.cancel()
        s.session.destroy()
        s.window.isHidden = true
    }

    /// A revealed row of each kind stays open while a push covers it and
    /// after the pop, as UIKit's table keeps its row.
    func testARevealedRowStaysOpenThroughAPushAndThePop() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a real swipe reaches UIKit's table") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "build.mjs --test --ios starts the XCTest touch runner")
        }
        let s = try screen(scene, "swipe-push")
        defer { close(s) }
        let nav = try XCTUnwrap(s.p.navigation.primaryNavigation)
        func settle(_ count: Int, _ what: String) {
            waitFor(what, cap: 10) { nav.viewControllers.count == count && nav.transitionCoordinator == nil }
            waitForMainQueue()
        }
        for kind in Kind.allCases {
            try s.swipe(kind)
            waitFor("\(kind) opens", cap: 5, facts: { s.facts(kind) }) { s.isOpen(kind) }
            XCTAssertNotNil(s.p.swipeHost.openRow, "\(kind) is the open row: \(s.facts(kind))")
            // The header's button, outside the row's view, pushes.
            s.p.press(try s.node("open-detail").id)
            settle(2, "detail pushed")
            XCTAssertTrue(s.isOpen(kind), "\(kind) stays open under the pushed screen: \(s.facts(kind))")
            XCTAssertNil(try s.row(kind).window, "the covered screen is out of the window, as UIKit's, and out of VoiceOver's reach")
            // A row opening on the pushed screen leaves it so: another table.
            try s.swipe(kind, on: "detail")
            waitFor("\(kind) opens on the pushed screen", cap: 5, facts: { s.facts(kind) }) { s.isOpen(kind, on: "detail") }
            XCTAssertTrue(s.isOpen(kind), "\(kind) stays open under a row opening above it: \(s.facts(kind))")
            nav.popViewController(animated: true)
            settle(1, "back on the rows")
            XCTAssertTrue(s.isOpen(kind), "\(kind) is still open after the pop: \(s.facts(kind))")
            XCTAssertEqual(s.p.swipeHost.openRow, try s.row(kind).id, "and is the open row again")
            let r = try s.frame(kind)
            try s.drag(from: CGPoint(x: 60, y: r.midY), to: CGPoint(x: 240, y: r.midY), velocity: 500)
            waitFor("\(kind) closes by its own drag", cap: 5, facts: { s.facts(kind) }) { !s.isOpen(kind) }
        }
    }

    /// A vertical drag begun on a row scrolls the page, the row closed or
    /// open (and an open one closes), as a horizontal scroll inside a
    /// vertical one does in UIKit and on the web.
    func testAVerticalDragOnARowScrollsThePage() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a real swipe reaches UIKit's table") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "build.mjs --test --ios starts the XCTest touch runner")
        }
        let s = try screen(scene, "swipe-drag")
        defer { close(s) }
        let page = try XCTUnwrap(try s.node("start-page").scrollView)
        let top = -page.adjustedContentInset.top
        for kind in Kind.allCases {
            for open in [false, true] {
                if open {
                    try s.swipe(kind)
                    waitFor("\(kind) opens", cap: 5, facts: { s.facts(kind) }) { s.isOpen(kind) }
                }
                // A finger landing while the row's own swipe still moves is
                // the row's, in UIKit too.
                waitFor("\(kind) comes to rest", cap: 5) { s.atRest(kind) }
                let r = try s.frame(kind)
                try s.drag(from: CGPoint(x: 60, y: r.midY), to: CGPoint(x: 60, y: r.midY - 120), velocity: 600)
                waitFor("a vertical drag on the \(open ? "open" : "closed") \(kind) row scrolls the page", cap: 5, facts: { s.facts(kind) }) {
                    page.contentOffset.y > top + 20 && !s.isOpen(kind)
                }
                waitFor("the page comes to rest", cap: 5) { !page.isDragging && !page.isDecelerating }
                page.setContentOffset(CGPoint(x: 0, y: top), animated: false)
                waitForMainQueue()
            }
        }
    }
}
#endif
