import QuartzCore
import XCTest
@testable import ExactKit
@testable import ExactSticky
@testable import ExactAnimations
@testable import ExactSvg
@testable import ExactSurfaces
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// Presenter.reset() across a session restart drops every ViewId-keyed
/// subsystem (`stickies`, `boxAnimations`, `svg`, `canvas2d`, `tableGrid`) so
/// the restarted runtime's fresh ViewIds inherit no prior-tree state.
final class PresenterResetTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        ExactSticky.install()
        ExactAnimations.install()
        ExactSvg.install()
        ExactSurfaces.install()
    }

    private func viewLayer(_ view: NodeView?) -> CALayer? { view?.layer }

    func testResetDropsStickyAnimationSvgCanvasAndTableState() throws {
        let p = Presenter()
        #if os(macOS)
        p.viewport.frame = NSRect(x: 0, y: 0, width: 300, height: 200)
        #else
        p.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 200)
        #endif
        let shape: [String: Any] = [
            "id": 1, "n": 7, "h": 1, "o": 1,
            "p": [0, 10, 5, 1, 110, 5, 1, 110, 55, 1, 10, 55, 3],
            "f": [239, 68, 68, 255], "w": 12, "ml": 4,
        ]
        let animSpec: [String: Any] = [
            "id": "slide#0#transform.translation.x", "k": "transform.translation.x",
            "s": 0.0, "dl": 0.0, "d": 1.0, "n": 1.0,
            "t": [0.0, 1.0], "v": [0.0, 40.0], "c": [Any](), "fill": 3,
        ]
        let canvasOp = { (lifetime: Int) -> [String: Any] in
            ["op": "canvas2d", "id": 5, "lifetime": lifetime, "generation": 0, "seq": 0, "fresh": true,
             "w": 40, "h": 40, "scale": 1.0, "stretch": false, "lists": [String]()]
        }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "scroll", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "create", "id": 4, "kind": "svg"],
            ["op": "create", "id": 5, "kind": "canvas2d"],
            ["op": "create", "id": 6, "kind": "view", "props": ["semanticTag": "table"]],
            ["op": "create", "id": 7, "kind": "view", "props": ["semanticTag": "th"]],
            ["op": "children", "id": 6, "ids": [7]],
            ["op": "children", "id": 1, "ids": [2, 3, 4, 5, 6]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 200.0, "h": 100.0],
            ["op": "content", "id": 1, "w": 200.0, "h": 500.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 20.0, "w": 50.0, "h": 50.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 50.0, "h": 50.0],
            ["op": "frame", "id": 4, "x": 0.0, "y": 0.0, "w": 120.0, "h": 60.0],
            ["op": "frame", "id": 5, "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0],
            ["op": "frame", "id": 6, "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0],
            ["op": "frame", "id": 7, "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0],
            ["op": "sticky", "id": 2, "scroller": 1, "natural": [0, 20, 50, 70],
             "limit": [0, 0, 200, 500], "port": [0, 0, 200, 100], "insets": [0, NSNull(), NSNull(), NSNull()]],
            ["op": "animations", "id": 3, "specs": [animSpec]],
            ["op": "svg", "id": 4, "scene": ["box": [0, 0, 120, 60], "t": [1, 0, 0, 1, 0, 0], "els": [shape]]],
            canvasOp(1),
            ["op": "table", "id": 6, "size": [1, 1]],
            ["op": "table", "id": 7, "table": 6, "place": [0, 1, 0, 1]],
        ]))
        p.canvas2d.waitForReplays()

        let stickyHost = try XCTUnwrap(p.stickies as? StickyHost)
        let oldAnimatedLayer = viewLayer(p.views[3])
        let oldCanvasLayer = try XCTUnwrap(viewLayer(p.views[5])?.sublayers?.first { $0.contents != nil })
        let oldTable = try XCTUnwrap(p.views[6])
        XCTAssertFalse(stickyHost.isEmpty)
        XCTAssertTrue(p.boxAnimations.tracks(3))
        XCTAssertTrue(p.boxAnimations.playsTransform(3, flying: false, clock: nil))
        XCTAssertEqual(p.svg?.target(4, at: CGPoint(x: 60, y: 30)), 7)
        XCTAssertNotNil(p.tableGrid.sizes[6])
        XCTAssertNotNil(p.tableGrid.places[7])
        #if os(macOS)
        _ = oldTable.accessibilityColumns()
        _ = oldTable.accessibilityHeader()
        XCTAssertNotNil(p.tableGrid.columns[6])
        XCTAssertNotNil(p.tableGrid.headers[6])
        #else
        _ = oldTable
        #endif

        p.reset()

        XCTAssertTrue(stickyHost.isEmpty)
        XCTAssertFalse(p.boxAnimations.tracks(3))
        XCTAssertFalse(p.boxAnimations.playsTransform(3, flying: false, clock: nil))
        XCTAssertTrue(oldAnimatedLayer?.animationKeys()?.isEmpty ?? true)
        XCTAssertNil(p.svg?.target(4, at: CGPoint(x: 60, y: 30)))
        XCTAssertNil(oldCanvasLayer.superlayer)
        XCTAssertTrue(p.tableGrid.sizes.isEmpty)
        XCTAssertTrue(p.tableGrid.places.isEmpty)
        #if os(macOS)
        XCTAssertTrue(p.tableGrid.columns.isEmpty)
        XCTAssertTrue(p.tableGrid.headers.isEmpty)
        #endif

        // A fresh tree reusing the same ViewIds inherits none of the prior tree's state.
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "scroll", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "create", "id": 4, "kind": "view"],
            ["op": "create", "id": 5, "kind": "canvas2d"],
            ["op": "children", "id": 1, "ids": [2, 3, 4, 5]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 200.0, "h": 100.0],
            ["op": "content", "id": 1, "w": 200.0, "h": 500.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 20.0, "w": 50.0, "h": 50.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 50.0, "h": 50.0],
            ["op": "frame", "id": 4, "x": 0.0, "y": 0.0, "w": 120.0, "h": 60.0],
            ["op": "frame", "id": 5, "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0],
            canvasOp(1),
        ]))
        p.canvas2d.waitForReplays()
        let freshView2 = try XCTUnwrap(p.views[2])
        p.stickies.scrolled(1)
        p.stickies.regridded(2)
        XCTAssertEqual(freshView2.stickyOffset, .zero)
        XCTAssertFalse(p.boxAnimations.playsTransform(3, flying: false, clock: nil))
        XCTAssertNil(p.svg?.target(4, at: CGPoint(x: 60, y: 30)))
        let freshCanvasLayer = try XCTUnwrap(viewLayer(p.views[5])?.sublayers?.first { $0.contents != nil })
        XCTAssertFalse(freshCanvasLayer === oldCanvasLayer)
    }
}
