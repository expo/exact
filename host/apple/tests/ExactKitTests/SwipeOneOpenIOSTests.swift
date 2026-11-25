#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactCollections
@testable import ExactGroupedLists

/// One swipe row open on a screen (`SwipeActionsHost.opening`), whatever
/// each row's kind: a grouped list's row swipes in UIKit's list cell, a
/// projected row in a one-row table of its own, a painted row by its scroll.
/// UIKit keeps one row open only among the rows of one view, and closes it
/// on a touch elsewhere in that view; the rows are swiped and tapped with
/// real touches from the XCTest touch runner:
///   bun host/apple/build.mjs --test --ios --only SwipeOneOpenIOSTests
final class SwipeOneOpenIOSTests: XCTestCase {
    enum Kind: String, CaseIterable { case grouped, native, painted, listPainted }

    private final class Screen: NSObject, UIGestureRecognizerDelegate {
        let p: Presenter
        let window: UIWindow
        /// Taps the window has seen end; the page's button's (60) presses, and
        /// the taps a tap recognizer on it recognized.
        var taps = 0, presses = 0, buttonTaps = 0
        init(p: Presenter, window: UIWindow) { self.p = p; self.window = window; super.init() }
        @objc func tapped() { taps += 1 }
        @objc func buttonTapped() { buttonTaps += 1 }
        /// The count sees a tap another recognizer takes.
        func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }
        /// Each kind's row: its owner, and the control its action stands for.
        let rows: [Kind: (owner: UInt32, action: UInt32)] = [.grouped: (21, 23), .native: (40, 43), .painted: (50, 53), .listPainted: (24, 27)]

        /// What a swipe that did not open left behind.
        func facts(_ kind: Kind) -> String {
            let owner = p.views[rows[kind]!.owner]
            return "open: \(Kind.allCases.filter(isOpen)), \(kind) at \(owner.map { $0.convert($0.bounds, to: window) } ?? .null)"
                + ", projected: \(owner.map { p.swipeHost.cell(of: $0) != nil } ?? false), key: \(window.isKeyWindow)"
                + ", open row: \(p.swipeHost.openRow.map(String.init) ?? "none"), painted moving: \(p.views[50]?.scrollView.map { $0.isDragging || $0.isDecelerating } ?? false)"
        }
        func isOpen(_ kind: Kind) -> Bool {
            let row = rows[kind]!
            if kind == .painted || kind == .listPainted { return (p.views[row.owner]?.scrollView?.contentOffset.x ?? 0) > 1 }
            // Swiped as UIKit's cell has it: false once a close begins.
            let swiped = kind == .native ? p.views[row.owner].flatMap(p.swipeHost.cell)?.configurationState.isSwiped == true
                : (p.groupedLists as? GroupedListHost)?.lists[1]?.cell(row.owner)?.configurationState.isSwiped == true
            return swiped && p.swipeHost.actionView(row.action) != nil
        }
        /// Where a finger lands on `kind`'s row, near its trailing edge.
        func start(_ kind: Kind) throws -> CGPoint {
            let owner = try XCTUnwrap(p.views[rows[kind]!.owner])
            let frame = owner.convert(owner.bounds, to: window)
            return CGPoint(x: frame.maxX - 30, y: frame.midY)
        }
    }

    /// A grouped list holding a swipe row (21: content 22, Delete 23), a
    /// plain row (30) and a painted row (24: content 26, Delete 27); below it a scrolling page (100) holding a projected
    /// native row (40: content 42, Delete 43), then a painted one (50:
    /// content 52, Delete 53), a button (60), then blank page; below the
    /// page, the screen.
    private func screen(_ scene: UIWindowScene) throws -> Screen {
        ExactGroupedLists.install()
        let p = Presenter()
        let lists = try XCTUnwrap(p.groupedLists as? GroupedListHost)
        lists.model = { $0 == 1 ? GroupedListModel(sections: [
            .init(view: 2, header: nil, footer: nil, rows: [.init(view: 21, custom: true), .init(view: 30, title: "Plain"), .init(view: 24, custom: true)]),
        ]) : nil }
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        let root = UIViewController()
        window.rootViewController = root
        window.makeKeyAndVisible()
        // Keep every touch inside the safe area, as a drag from the screen's
        // bottom edge sends the app Home
        p.viewport.frame = window.bounds.inset(by: window.safeAreaInsets)
        root.view.addSubview(p.viewport)
        let w = Double(p.viewport.bounds.width)
        func create(_ id: Int, _ kind: String, _ props: [String: String] = [:], _ style: [String: Any] = [:]) -> [String: Any] {
            ["op": "create", "id": id, "kind": kind, "props": props, "handlers": kind == "button" ? ["press"] : [String](), "style": style]
        }
        func frame(_ id: Int, _ x: Double, _ y: Double, _ width: Double, _ height: Double) -> [String: Any] {
            ["op": "frame", "id": id, "x": x, "y": y, "w": width, "h": height]
        }
        let swipe = ["swipeContent": "body", "swipeTrailing": "delete"]
        let scrolls: [String: Any] = ["overflow_x": "scroll", "overflow_y": "hidden"]
        var ops: [[String: Any]] = [
            create(100, "view", [:], ["overflow_y": "scroll"]),
            frame(100, 0, 220, w, 360), ["op": "content", "id": 100, "w": w, "h": 1200.0],
            create(1, "list", ["listStyle": "inset-grouped"], ["overflow_y": "scroll"]),
            create(2, "view", ["semanticTag": "section"]), create(4, "view"),
            create(21, "view", swipe, scrolls.merging(["padding_right": 16.0]) { a, _ in a }),
            create(22, "view", ["id": "body"]),
            create(23, "button", ["id": "delete", "accessibilityLabel": "Delete Grouped", "destructive": "true"]),
            create(30, "view"),
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [4]],
            create(24, "view", swipe, scrolls), create(25, "view"), create(26, "view", ["id": "body"]),
            create(27, "button", ["id": "delete", "accessibilityLabel": "Delete Listed"], ["appearance": "none", "written": ["appearance"]]),
            ["op": "children", "id": 4, "ids": [21, 30, 24]], ["op": "children", "id": 21, "ids": [22, 23]],
            ["op": "children", "id": 24, "ids": [25]], ["op": "children", "id": 25, "ids": [26, 27]],
            frame(1, 0, 0, w, 220),
            frame(21, 16, 0, w - 16, 53), frame(22, 0, 9, w - 32, 34), frame(23, w - 32, 9, 60, 34),
            frame(30, 16, 52, w - 32, 52),
            frame(24, 16, 104, w - 32, 53), frame(25, 0, 0, w + 58, 53), frame(26, 0, 0, w - 32, 53), frame(27, w - 32, 0, 90, 53),
            ["op": "content", "id": 24, "w": w + 58, "h": 53.0],
        ]
        for (owner, top, label, painted) in [(40, 0.0, "Delete Native", false), (50, 90.0, "Delete Painted", true)] {
            ops += [
                create(owner, "view", swipe, scrolls), create(owner + 1, "view"),
                create(owner + 2, "view", ["id": "body"], ["background_color": [255, 255, 255, 255]]),
                create(owner + 3, "button", ["id": "delete", "accessibilityLabel": label],
                       painted ? ["appearance": "none", "written": ["appearance"]] : [:]),
                ["op": "children", "id": owner, "ids": [owner + 1]], ["op": "children", "id": owner + 1, "ids": [owner + 2, owner + 3]],
                frame(owner, 0, top, w, 80), frame(owner + 1, 0, 0, w + 90, 80), frame(owner + 2, 0, 0, w, 80), frame(owner + 3, w, 0, 90, 80),
                ["op": "content", "id": owner, "w": w + 90, "h": 80.0],
            ]
        }
        ops += [create(60, "button", ["accessibilityLabel": "Elsewhere"]), frame(60, 0, 180, w, 80),
                ["op": "children", "id": 100, "ids": [40, 50, 60]], ["op": "roots", "ids": [1, 100]]]
        p.apply(wireBatch(ops))
        lists.lists[1]?.collection.layoutIfNeeded()
        // A touch reaches only a window in front, and one Core Animation has shown.
        waitFor("the screen is in front", cap: 15) { Agent.frontRefusal(window) == nil }
        CATransaction.flush()
        waitForMainQueue()
        for kind in [Kind.grouped, .native] {
            XCTAssertTrue(p.swipeHost.ownsAction(kind == .grouped ? 23 : 43), "\(kind) row swipes natively: \(p.swipeHost.refusal(of: p.views[kind == .grouped ? 21 : 40]!) ?? "")")
        }
        let s = Screen(p: p, window: window)
        let taps = UITapGestureRecognizer(target: s, action: #selector(Screen.tapped))
        taps.cancelsTouchesInView = false; taps.delaysTouchesEnded = false; taps.delegate = s
        window.addGestureRecognizer(taps)
        p.onPress = { [weak s] id in if id == 60 { s?.presses += 1 } }
        let buttonTaps = UITapGestureRecognizer(target: s, action: #selector(Screen.buttonTapped))
        buttonTaps.cancelsTouchesInView = false
        p.views[60]?.addGestureRecognizer(buttonTaps)
        return s
    }

    private func swipe(_ touch: TouchRunner, _ s: Screen, _ kind: Kind) throws {
        let from = try s.start(kind)
        try drag(touch, s, from: from, to: CGPoint(x: from.x - 170, y: from.y), velocity: 500)
    }

    private func drag(_ touch: TouchRunner, _ s: Screen, from: CGPoint, to: CGPoint, velocity: Double = 600) throws {
        var drag = try TouchRunner.aim(from, in: s.window)
        let end = try TouchRunner.aim(to, in: s.window)
        drag.merge(["op": "drag", "to": end["point"]!, "screenTo": end["screen"]!, "press": 0.02, "hold": 0.0, "velocity": velocity]) { _, new in new }
        XCTAssertEqual(try touch.ask(drag)["done"] as? Bool, true)
    }

    private func tap(_ touch: TouchRunner, _ s: Screen, _ point: CGPoint) throws {
        var tap = try TouchRunner.aim(point, in: s.window)
        tap["op"] = "tap"
        XCTAssertEqual(try touch.ask(tap)["done"] as? Bool, true)
    }

    /// Each row opened after a row of each other kind (the issue's two
    /// orders among them): the row opened last is the one open. Then what a
    /// tap and a scroll do to each kind's open row, as UIKit's table does.
    /// One screen serves every check, since the first swipe on a new window
    /// can open nothing.
    func testOneSwipeRowIsOpenWhateverItsKind() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a real swipe reaches UIKit's list cell and table") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "build.mjs --test --ios starts the XCTest touch runner")
        }
        let touch = try TouchRunner(self)
        defer { touch.connection.cancel() }
        let s = try screen(scene)
        defer { s.p.reset(); s.window.isHidden = true }
        // After another test's window, the first real swipe on a grouped or
        // projected row can open nothing (QUEUE.md, "The first swipe on a
        // second test window"): one is swiped open first, and closed.
        let plain = try XCTUnwrap((s.p.groupedLists as? GroupedListHost)?.lists[1]?.cell(30))
        let other = plain.convert(CGPoint(x: plain.bounds.midX, y: plain.bounds.midY), to: s.window)
        for _ in 0..<3 where !s.isOpen(.grouped) {
            try swipe(touch, s, .grouped)
            waitFor("grouped opens", cap: 3, onFailure: { _ in }) { s.isOpen(.grouped) }
        }
        try tap(touch, s, other)
        waitFor("grouped closes", cap: 5, facts: { s.facts(.grouped) }) { !s.isOpen(.grouped) }
        let order: [Kind] = [.grouped, .native, .grouped, .painted, .grouped, .native, .painted, .native, .listPainted, .grouped]
        for (i, kind) in order.enumerated() {
            try swipe(touch, s, kind)
            let before = i > 0 ? "\(order[i - 1]) closes" : "nothing else is open"
            waitFor("\(kind) opens and \(before)", cap: 5, facts: { s.facts(kind) }) {
                Kind.allCases.filter(s.isOpen) == [kind]
            }
        }
        // With the cell swipe open, a touch on the list's painted row closes
        // it, as one on any other row of the list does (UIKit's).
        try swipe(touch, s, .listPainted)
        waitFor("grouped closes", cap: 5, facts: { s.facts(.grouped) }) { !s.isOpen(.grouped) }
        // A touch elsewhere in the open row's view closes it and presses
        // nothing, and one outside that view leaves it open, as UIKit does:
        // the grouped row's view is its list, the projected and painted
        // rows' the page.
        let page = try XCTUnwrap(s.p.views[100]).convert(try XCTUnwrap(s.p.views[100]).bounds, to: s.window)
        let blank = CGPoint(x: page.midX, y: page.maxY - 50), below = CGPoint(x: page.midX, y: page.maxY + 30)
        let button = try XCTUnwrap(s.p.views[60]).convert(try XCTUnwrap(s.p.views[60]).bounds, to: s.window)
        let elsewhere = CGPoint(x: button.midX, y: button.midY)
        for kind in [Kind.grouped, .native, .painted] {
            if !s.isOpen(kind) { try swipe(touch, s, kind) }
            waitFor("\(kind) opens", cap: 5, facts: { s.facts(kind) }) { s.isOpen(kind) }
            let taps = s.taps
            try tap(touch, s, kind == .grouped ? blank : below)
            waitFor("the tap outside its view ends", cap: 5) { s.taps > taps }
            waitForMainQueue()
            XCTAssertTrue(s.isOpen(kind), "\(kind) stays open on a tap outside its view")
            try tap(touch, s, kind == .grouped ? other : elsewhere)
            waitFor("\(kind) closes on a tap elsewhere in its view", cap: 5, facts: { s.facts(kind) }) { !s.isOpen(kind) }
            waitFor("the tap elsewhere ends", cap: 5) { s.taps > taps + 1 }
            waitForMainQueue()
            XCTAssertEqual(s.presses, 0, "the tap that closes \(kind) presses nothing")
            XCTAssertEqual(s.buttonTaps, 0, "nor is it a tap recognizer's")
        }
        try tap(touch, s, elsewhere)
        waitFor("a tap with no row open presses", cap: 5) { s.presses == 1 && s.buttonTaps == 1 }
        // A row closed by its own tap or drag leaves the next tap elsewhere
        // to press.
        for kind in [Kind.native, .painted] {
            try swipe(touch, s, kind)
            waitFor("\(kind) opens", cap: 5, facts: { s.facts(kind) }) { s.isOpen(kind) }
            let at = CGPoint(x: page.minX + 40, y: try s.start(kind).y)
            if kind == .native { try tap(touch, s, at) } else { try drag(touch, s, from: at, to: CGPoint(x: at.x + 170, y: at.y)) }
            waitFor("\(kind) closes by itself", cap: 5, facts: { s.facts(kind) }) { !s.isOpen(kind) }
            let presses = s.presses
            try tap(touch, s, elsewhere)
            waitFor("the next tap elsewhere presses", cap: 5) { s.presses == presses + 1 }
        }
        // And a scroll of the page closes the rows it holds, one begun on the
        // open projected row too.
        let scroll = try XCTUnwrap(s.p.views[100]?.scrollView)
        let starts: [(Kind, CGPoint?)] = [(.native, blank), (.painted, blank), (.native, nil)]
        for (kind, from) in starts {
            try swipe(touch, s, kind)
            waitFor("\(kind) opens", cap: 5, facts: { s.facts(kind) }) { s.isOpen(kind) }
            let at = try from ?? CGPoint(x: page.minX + 40, y: s.start(kind).y)
            try drag(touch, s, from: at, to: CGPoint(x: at.x, y: at.y - 120))
            waitFor("\(kind) closes as the page scrolls", cap: 5, facts: { s.facts(kind) }) { !s.isOpen(kind) && scroll.contentOffset.y > 0 }
            waitFor("the page comes to rest", cap: 5) { !scroll.isDragging && !scroll.isDecelerating }
            scroll.setContentOffset(.zero, animated: false)
        }
    }
}
#endif
