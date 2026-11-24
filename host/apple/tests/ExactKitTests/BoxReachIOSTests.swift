#if os(iOS)
@testable import ExactKit
import ExactCore
import UIKit
import XCTest

/// A box that does not scroll reaches past its laid-out border box (LLP 1123 D13): its view is drawn
/// past it, painting and taking touches there, its children where layout put them, and its layout,
/// geometry and accessibility frame keep the laid-out box.
@MainActor
final class BoxReachIOSTests: XCTestCase {
    private var window: UIWindow?

    override func tearDown() {
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    func testABoxReachesPastItsBoxAndItsChildrenStayWhereLayoutPutThem() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "reach is drawn in a window") }
        let p = presenter(in: scene)
        let window = try XCTUnwrap(self.window)
        let (box, child) = (try view(p, 2), try view(p, 3))
        let resting = child.convert(child.bounds, to: window)
        p.setReach(2, UIEdgeInsets(top: 47, left: 0, bottom: 0, right: 0))
        XCTAssertEqual(box.frame, CGRect(x: 0, y: 53, width: 300, height: 147), "drawn 47 past its box's top")
        XCTAssertEqual(box.laidOutBox, CGRect(x: 0, y: 0, width: 300, height: 100))
        XCTAssertEqual(child.convert(child.bounds, to: window), resting, "its child where layout put it")
        // The strip paints its background and takes its touches
        XCTAssertEqual(pixel(window, at: CGPoint(x: 150, y: 70)), [51, 102, 153], "its background in the strip")
        XCTAssertTrue(window.hitTest(CGPoint(x: 150, y: 70), with: nil) === box, "a touch in the strip is the box's")
        // Its accessibility frame is its laid-out box
        p.syncAccessibility()
        XCTAssertEqual(box.accessibilityFrame, UIAccessibility.convertToScreenCoordinates(CGRect(x: 0, y: 100, width: 300, height: 100), in: window))
        // A new frame is drawn past by the same reach
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 0.0, "y": 100.0, "w": 300.0, "h": 120.0]]))
        XCTAssertEqual(box.frame, CGRect(x: 0, y: 53, width: 300, height: 167))
        XCTAssertEqual(child.convert(child.bounds, to: window), resting)
        // A flat box takes a view to reach
        XCTAssertTrue(p.flats.isFlat(4), "a plain box with no children is a flat leaf")
        p.setReach(4, UIEdgeInsets(top: 0, left: 0, bottom: 34, right: 0))
        XCTAssertEqual(try view(p, 4).frame, CGRect(x: 0, y: 300, width: 300, height: 134), "promoted and drawn past its foot")
        // Reach back to zero draws the box at its laid-out frame
        p.setReach(2, .zero)
        XCTAssertEqual(box.frame, CGRect(x: 0, y: 100, width: 300, height: 120))
        XCTAssertEqual(box.bounds.origin, .zero)
        // A fractional reach is drawn on the pixel grid, its laid-out box, its content box and its child
        // where the grid drew them
        p.setReach(2, UIEdgeInsets(top: 0.2, left: 0, bottom: 0, right: 0))
        XCTAssertEqual(box.laidOutBox, CGRect(x: 0, y: 0, width: 300, height: 120))
        XCTAssertEqual(box.layoutBounds.origin, .zero, "the content box pointers read from")
        let moved = child.convert(child.bounds, to: window)
        XCTAssertEqual(moved.minY, resting.minY, accuracy: 0.001, "its child where layout put it")
        XCTAssertEqual(moved.height, resting.height, accuracy: 0.001)
        p.setReach(2, .zero)
        // A box that starts to scroll keeps no reach of its own: its scroll view reaches instead
        p.setReach(2, UIEdgeInsets(top: 47, left: 0, bottom: 0, right: 0))
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["overflow_y": "auto", "background_color": [51, 102, 153, 255]]]]))
        XCTAssertNotNil(box.scrollView)
        XCTAssertEqual(box.frame, CGRect(x: 0, y: 100, width: 300, height: 120), "at its laid-out frame")
        XCTAssertEqual(box.bounds.origin, .zero)
    }

    /// A presenter of its own in a scene window: a root (1), 300 by 400, holding a box (2) 100 points
    /// tall and 100 down, its background `#336699` and its accessibility label "Panel", with a 50-by-30
    /// child (3) 10 points in, a view of its own; and a plain box (4), a flat leaf, the last 100 points.
    private func presenter(in scene: UIWindowScene) -> Presenter {
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 300, height: 400)
        window.makeKeyAndVisible()
        self.window = window
        let p = Presenter()
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "props": ["accessibilityLabel": "Panel", "accessibilityRole": "group"], "style": ["background_color": [51, 102, 153, 255]]],
            ["op": "create", "id": 3, "kind": "view", "props": ["testId": "child"], "style": ["background_color": [255, 255, 255, 255]]],
            ["op": "create", "id": 4, "kind": "view", "style": ["background_color": [0, 0, 0, 255]]],
            ["op": "children", "id": 1, "ids": [2, 4]], ["op": "children", "id": 2, "ids": [3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 400.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 100.0, "w": 300.0, "h": 100.0],
            ["op": "frame", "id": 3, "x": 10.0, "y": 10.0, "w": 50.0, "h": 30.0],
            ["op": "frame", "id": 4, "x": 0.0, "y": 300.0, "w": 300.0, "h": 100.0],
        ]))
        window.layoutIfNeeded()
        return p
    }

    private func view(_ p: Presenter, _ id: UInt32) throws -> NodeView { try XCTUnwrap(p.views[id], "node \(id)") }

    /// The window's pixel at `point`, red, green and blue.
    private func pixel(_ window: UIWindow, at point: CGPoint) -> [Int] {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let image = UIGraphicsImageRenderer(size: CGSize(width: 1, height: 1), format: format).image { context in
            context.cgContext.translateBy(x: -point.x, y: -point.y)
            window.layer.render(in: context.cgContext)
        }
        guard let data = image.cgImage?.dataProvider?.data, let bytes = CFDataGetBytePtr(data) else { return [] }
        return [Int(bytes[0]), Int(bytes[1]), Int(bytes[2])]
    }
}
#endif
