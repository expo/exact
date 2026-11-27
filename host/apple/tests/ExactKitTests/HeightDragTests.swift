import XCTest
#if os(iOS)
import UIKit
#endif
@testable import ExactKit
@testable import ExactDrag

final class HeightDragTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactDrag.install() } // LLP 1047.001 D4
    func testBindingPreservesGenerationalKeysAndDoesNotResolveNames() {
        let binding = HeightDragBinding([
            "id": 7, "target": 3,
            "handleKey": "18446744073709551615",
            "targetKey": "9007199254740993"
        ])
        XCTAssertEqual(binding?.id, 7)
        XCTAssertEqual(binding?.target, 3)
        XCTAssertEqual(binding?.handleKey, UInt64.max)
        XCTAssertEqual(binding?.targetKey, 9007199254740993)
        XCTAssertNil(HeightDragBinding([
            "id": 7, "target": 3, "handleKey": 42, "targetKey": "43"
        ]))
    }

    func testBindingRetirementIsExplicitAndPartialTargetPairIsRefused() {
        let retired = HeightDragBinding([
            "id": 7, "target": NSNull(), "handleKey": "42", "targetKey": NSNull()
        ])
        XCTAssertNotNil(retired)
        XCTAssertNil(retired?.target)
        XCTAssertNil(retired?.targetKey)
        XCTAssertNil(HeightDragBinding([
            "id": 7, "target": 3, "handleKey": "42", "targetKey": NSNull()
        ]))
        XCTAssertNil(HeightDragBinding([
            "id": 7, "target": NSNull(), "handleKey": "42", "targetKey": "43"
        ]))
    }

    func testCaughtHeightUsesOriginAndUpwardDragGrowsWithoutAuthoredSnapping() {
        let mapping = HeightDragPosition(base: 213.125)!
        XCTAssertEqual(mapping.value(downward: 0), 213.125)
        XCTAssertEqual(mapping.value(downward: -20), 233.125)
        XCTAssertEqual(mapping.value(downward: 20), 193.125)
        XCTAssertEqual(mapping.value(downward: 300), 0)
        XCTAssertEqual(mapping.value(downward: 210), 3.125)
    }

    func testPositionAdmissionRejectsMalformedAndOverflow() {
        XCTAssertNil(HeightDragPosition(base: -1))
        XCTAssertNil(HeightDragPosition(base: .nan))
        XCTAssertNil(HeightDragPosition(base: .infinity))
        XCTAssertNil(HeightDragPosition(base: Double.greatestFiniteMagnitude))
        let mapping = HeightDragPosition(base: 0)!
        XCTAssertNil(mapping.value(downward: .nan))
        XCTAssertNil(mapping.value(downward: -.infinity))
        XCTAssertNil(mapping.value(downward: -Double.greatestFiniteMagnitude))
        XCTAssertEqual(mapping.value(downward: -Double(Float.greatestFiniteMagnitude)), Double(Float.greatestFiniteMagnitude))
    }

    #if os(iOS)
    private final class Travel: NSObject {
        var upward: CGFloat = 0
        @objc func read(_ pan: UIPanGestureRecognizer) {
            upward = -pan.translation(in: pan.view?.window).y
        }
    }

    func testOneContactExpandsAndScrollsAndTheNextFlingOnlyScrolls() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a sheet drag reaches UIKit") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "the sheet shares a system touch with its scroll view")
        }
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_SHEET_SCROLL_PLAN"])
        let touch = try TouchRunner(self)
        let session = ExactApp.shared.makeSession(label: "sheet-scroll")
        let window = UIWindow(windowScene: scene), root = UIViewController(), view = ExactView(session: session)
        window.rootViewController = root
        window.makeKeyAndVisible()
        root.view.addSubview(view)
        view.frame = root.view.bounds
        defer { touch.connection.cancel(); session.destroy(); window.isHidden = true }
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: window.bounds.size).error)
        view.layoutIfNeeded()
        let port = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "port" }?.scrollView)
        let sheet = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "sheet" })
        func drag(up distance: CGFloat, velocity: Double, hold: Double) throws {
            let start = port.convert(CGPoint(x: port.bounds.midX, y: port.bounds.maxY - 40), to: window)
            var request = try TouchRunner.aim(start, in: window)
            let end = try TouchRunner.aim(CGPoint(x: start.x, y: start.y - distance), in: window)
            request.merge(["op": "drag", "to": end["point"]!, "screenTo": end["screen"]!,
                           "velocity": velocity, "press": 0.1, "hold": hold]) { _, new in new }
            XCTAssertEqual(try touch.ask(request)["done"] as? Bool, true)
        }
        func slots() throws -> [String: Double] {
            let reply = try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any]
            return try XCTUnwrap(reply?["slots"] as? [String: Double])
        }
        waitFor("the window receives touches") { Agent.frontRefusal(window) == nil }
        let initialHeight = sheet.bounds.height
        let travel = Travel()
        port.panGestureRecognizer.addTarget(travel, action: #selector(Travel.read(_:)))
        try drag(up: 250, velocity: 400, hold: 0.3)
        XCTAssertEqual(sheet.bounds.height, 480, accuracy: 1)
        // This box is unscaled, so the list gets the recognized window travel left after the sheet expands.
        let firstOffset = travel.upward - (sheet.bounds.height - initialHeight)
        XCTAssertGreaterThan(firstOffset, 0)
        XCTAssertEqual(port.contentOffset.y, firstOffset, accuracy: 1)
        let release = try slots()
        XCTAssertEqual(release["releases"], 1)
        XCTAssertEqual(release["released"], 480)
        XCTAssertEqual(try XCTUnwrap(release["speed"]), 0, accuracy: 0.01)
        try drag(up: 90, velocity: 800, hold: 0)
        waitFor("UIKit coasts beyond the second drag") { port.contentOffset.y > firstOffset + 90 }
        XCTAssertEqual(sheet.bounds.height, 480, accuracy: 1)
        XCTAssertEqual(try slots(), release)
    }
    #endif
}
