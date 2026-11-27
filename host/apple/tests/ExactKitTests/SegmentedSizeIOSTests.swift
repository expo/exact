#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A segmented control's `-exact-control-size` (LLP 1069.011.001 D8): UIKit's control is drawn
/// as tall as SwiftUI's segmented picker of that size, centred in its tablist's box.
final class SegmentedSizeIOSTests: XCTestCase {
    private var window: UIWindow!

    override func tearDown() {
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    func testALargeSegmentedControlIsSwiftUIsLargePickersHeight() throws {
        let p = Presenter()
        window = hostedWindowScene().map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 400)
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "props": ["accessibilityRole": "tablist"]],
            ["op": "create", "id": 3, "kind": "button", "handlers": ["press"], "props": ["accessibilityRole": "tab", "accessibilitySelected": "true"]],
            ["op": "create", "id": 4, "kind": "button", "handlers": ["press"], "props": ["accessibilityRole": "tab"]],
            ["op": "create", "id": 5, "kind": "text", "props": ["text": "Reading"]],
            ["op": "create", "id": 6, "kind": "text", "props": ["text": "Finished"]],
            ["op": "children", "id": 3, "ids": [5]], ["op": "children", "id": 4, "ids": [6]],
            ["op": "children", "id": 2, "ids": [3, 4]], ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 400],
            ["op": "frame", "id": 2, "x": 20, "y": 100, "w": 360, "h": 64],
        ]))
        window.layoutIfNeeded()
        let control = try XCTUnwrap(p.segments.control(of: 2))
        let own = control.intrinsicContentSize.height
        XCTAssertEqual(control.frame.height, own, accuracy: 0.5, "unsized, UIKit's own height")

        p.apply(wireBatch([["op": "style", "id": 2, "style": ["control_size": "large"]]]))
        window.layoutIfNeeded()
        // SwiftUI's segmented picker on iOS 27.1: 31 points at regular, 48 at large
        XCTAssertEqual(control.frame.height, own + 17, accuracy: 0.5, "large, SwiftUI's large picker's height")
        XCTAssertEqual(control.frame.midY, 32, accuracy: 0.5, "centred in the tablist's box")

        p.apply(wireBatch([["op": "style", "id": 2, "style": ["control_size": "medium"]]]))
        window.layoutIfNeeded()
        XCTAssertEqual(control.frame.height, own, accuracy: 0.5, "medium, UIKit's own height again")
    }
}
#endif
