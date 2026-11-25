#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A scroller's keyboard dismiss padding (LLP 1123 D7): the window's one padding, which the newest interactive drag
/// holds until it ends, and a real drag that starts the keyboard following that far above it.
final class KeyboardDismissIOSTests: XCTestCase {
    func testTheWindowsPaddingIsTheNewestDragsUntilNoneHoldsIt() {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        window.isHidden = false
        defer { window.isHidden = true }
        let sampler = KeyboardSampleIOS.install(in: window)
        let a = UIScrollView(), b = UIScrollView(), leaving = UIScrollView()
        for scroll in [a, b, leaving] { window.addSubview(scroll) }
        sampler.hold(150, for: a)
        XCTAssertEqual(sampler.dismissPadding, 150)
        sampler.hold(60, for: b)
        XCTAssertEqual(sampler.dismissPadding, 60, "the newest drag's")
        sampler.hold(140, for: a)
        XCTAssertEqual(sampler.dismissPadding, 60, "a drag that holds it again keeps its place")
        sampler.release(b)
        XCTAssertEqual(sampler.dismissPadding, 140)
        sampler.hold(90, for: leaving)
        leaving.removeFromSuperview()
        sampler.hold(140, for: a)
        XCTAssertEqual(sampler.dismissPadding, 140, "a drag out of the window holds nothing")
        sampler.release(a)
        XCTAssertEqual(sampler.dismissPadding, 0, "none holds it")
    }

    private final class Tops: NSObject {
        var lowest: CGFloat = 0
        let read: () -> CGFloat?
        private var link: CADisplayLink?
        init(_ read: @escaping () -> CGFloat?) { self.read = read }
        func start(at top: CGFloat) {
            lowest = top
            link = CADisplayLink(target: self, selector: #selector(tick))
            link?.add(to: .main, forMode: .common)
        }
        func stop() { link?.invalidate(); link = nil }
        @objc private func tick() { if let top = read() { lowest = max(lowest, top) } }
    }

    func testAScrollersPaddingStartsTheKeyboardFollowingThatFarAboveIt() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a real drag reaches UIKit") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "the keyboard follows a real drag of the scroller")
        }
        let touch = try TouchRunner(self)
        let session = ExactApp.shared.makeSession(label: "keyboard-dismiss-padding")
        let window = UIWindow(windowScene: scene), host = UIViewController()
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer { touch.connection.cancel(); window.endEditing(true); window.isHidden = true; session.destroy() }
        let p = session.presenter, size = host.view.bounds.size
        p.viewport.frame = host.view.bounds
        host.view.addSubview(p.viewport)
        func scroller(_ padding: Double) -> [String: Any] { ["overflow_y": "scroll", "keyboard_dismiss_padding": padding] }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "style": scroller(150)],
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
        XCTAssertEqual(scroll.keyboardDismissMode, .interactive)
        let field = UITextField(frame: CGRect(x: 20, y: 120, width: 240, height: 44))
        try XCTUnwrap(p.views[3]).addSubview(field)
        let sampler = KeyboardSampleIOS.install(in: window)
        let tops = Tops { sampler.sample?.frame.minY }
        waitFor("the window receives touches") { Agent.frontRefusal(window) == nil }
        // How far the keyboard's top goes down while a finger drags the scroller from 260 points above it to 30
        // above it and holds there, never reaching it
        func followed() throws -> CGFloat {
            scroll.setContentOffset(.zero, animated: false)
            if !field.isFirstResponder { field.becomeFirstResponder() }
            waitFor("the keyboard is up", cap: 15) { sampler.sample.map { $0.frame.maxY >= window.bounds.maxY - 0.5 } ?? false }
            waitForRunLoop(seconds: 1)
            let top = try XCTUnwrap(sampler.sample).frame.minY, x = window.bounds.midX
            var request = try TouchRunner.aim(CGPoint(x: x, y: top - 260), in: window)
            let end = try TouchRunner.aim(CGPoint(x: x, y: top - 30), in: window)
            request.merge(["op": "drag", "to": end["point"]!, "screenTo": end["screen"]!,
                           "velocity": 150, "press": 0.05, "hold": 0.6]) { _, new in new }
            tops.start(at: top)
            XCTAssertEqual(try touch.ask(request)["done"] as? Bool, true)
            tops.stop()
            waitFor("the drag's end lets the padding go", cap: 5) { sampler.dismissPadding == 0 }
            return tops.lowest - top
        }
        XCTAssertGreaterThan(try followed(), 100, "150 points of padding: the keyboard follows a finger 30 points above it")
        p.apply(wireBatch([["op": "style", "id": 2, "style": scroller(0)]]))
        XCTAssertEqual(try followed(), 0, accuracy: 0.5, "with none it waits for the finger")
    }

    private func waitForRunLoop(seconds: TimeInterval) {
        let until = Date().addingTimeInterval(seconds)
        while Date() < until { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01)) }
    }
}
#endif
