import XCTest
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif
@testable import ExactKit

/// LLP 1003.003 D7 on Apple: a clip transition the host hands to Core
/// Animation plays on the box's own clip layer, keeps its phase when the mask
/// is replaced, and is hit-tested as the presentation tree has it.
final class ClipHandOverTests: XCTestCase {
    #if canImport(UIKit)
    private var window: UIWindow?
    #else
    private var window: NSWindow?
    #endif

    override func tearDown() {
        #if canImport(UIKit)
        window?.isHidden = true
        #else
        window?.close()
        #endif
        window = nil
        super.tearDown()
    }

    /// A 100-point blue box clipped to its top `height` points, shown in a window.
    private func clipped(height: Double) -> Presenter {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 200, height: 200)
        #if canImport(UIKit)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 200, height: 200))
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        #else
        _ = NSApplication.shared
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 200, height: 200), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        #endif
        self.window = window
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [0, 0, 255, 255], "clip_path": Self.top(height)]],
            ["op": "roots", "ids": [1]], ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 100, "h": 100],
        ]))
        // Its first layout installs the mask it keeps: a later mask takes a
        // run from the session's clock, which this presenter has none of
        #if canImport(UIKit)
        p.viewport.layoutIfNeeded()
        #else
        p.viewport.layoutSubtreeIfNeeded()
        #endif
        CATransaction.flush()
        return p
    }

    private static func top(_ height: Double) -> [String: Any] {
        ["commands": [["M", [0, 0]], ["L", [100, 0]], ["L", [100, height]], ["L", [0, height]], ["Z", []]]]
    }

    /// The `path` spec the Rust host hands over (`clip_handover::spec`): one
    /// second, linear, from the top 20 points to the top 100.
    private static let grow: [String: Any] = [
        "id": "clip#0#path", "k": "path", "s": 0.0, "dl": 0.0, "d": 1.0, "n": 1.0, "t": [0.0, 1.0],
        "v": [[0, 0, 0, 1, 100, 0, 1, 100, 20, 1, 0, 20, 3], [0, 0, 0, 1, 100, 0, 1, 100, 100, 1, 0, 100, 3]],
        "c": [], "fill": 2.0, "m": 0.0,
    ]

    private func presentedHeight(_ node: NodeView) throws -> CGFloat {
        CATransaction.flush()
        return try XCTUnwrap(node.presentedClipPath).boundingBoxOfPath.height
    }

    func testAPlayedClipRunsOnTheClipLayerAndHitsAsItShows() throws {
        let p = clipped(height: 100)
        let node = try XCTUnwrap(p.views[1])
        p.apply(wireBatch([["op": "animations", "id": 1, "specs": [Self.grow]]]))
        let clip = try XCTUnwrap(BoxMaskState.existing(node)?.clipLayer)
        XCTAssertNotNil(clip.animation(forKey: "clip#0#path"), "on the clip layer")
        #if canImport(UIKit)
        XCTAssertNil(node.layer.animation(forKey: "clip#0#path"), "not on the box's")
        #endif
        // Half way through: the top 60 points show and hit
        p.boxAnimations.seek(clock: 500)
        XCTAssertEqual(try presentedHeight(node), 60, accuracy: 0.5)
        #if canImport(UIKit)
        XCTAssertTrue(node.hitTest(CGPoint(x: 50, y: 50), with: nil) === node)
        XCTAssertNil(node.hitTest(CGPoint(x: 50, y: 80), with: nil), "inside the model's clip, outside what shows")
        #endif
    }

    func testARunSurvivesTheBoxsOtherStyleChanges() throws {
        let p = clipped(height: 100)
        let node = try XCTUnwrap(p.views[1])
        p.apply(wireBatch([["op": "animations", "id": 1, "specs": [Self.grow]]]))
        // A new background re-applies the box's mask; whatever layer is the
        // clip then takes the run in phase from its start
        p.apply(wireBatch([["op": "style", "id": 1, "style": ["background_color": [255, 0, 0, 255], "clip_path": Self.top(100)]]]))
        let clip = try XCTUnwrap(BoxMaskState.existing(node)?.clipLayer)
        XCTAssertNotNil(clip.animation(forKey: "clip#0#path"))
        p.boxAnimations.seek(clock: 500)
        XCTAssertEqual(try presentedHeight(node), 60, accuracy: 0.5)
    }

    /// A `steps(4)` run after a half-second delay, as the Rust host hands it
    /// over: four paths (the top 20, 40, 60 and 80 points), each held to the
    /// next of five key times, the model's 100 once it ends.
    func testAStepsRunHoldsEachStepAndEndsAtTheModel() throws {
        let p = clipped(height: 100)
        let node = try XCTUnwrap(p.views[1])
        let top = { (h: Double) -> [Double] in [0, 0, 0, 1, 100, 0, 1, 100, h, 1, 0, h, 3] }
        let steps: [String: Any] = [
            "id": "clip#0#path", "k": "path", "s": 0.0, "dl": 0.5, "d": 1.0, "n": 1.0,
            "t": [0.0, 0.25, 0.5, 0.75, 1.0], "v": [top(20), top(40), top(60), top(80)], "c": [], "fill": 2.0, "m": 1.0,
        ]
        p.apply(wireBatch([["op": "animations", "id": 1, "specs": [steps]]]))
        p.boxAnimations.seek(clock: 250)
        XCTAssertEqual(try presentedHeight(node), 20, accuracy: 0.5, "the delay holds the first step")
        p.boxAnimations.seek(clock: 875)
        XCTAssertEqual(try presentedHeight(node), 40, accuracy: 0.5, "the second step holds across its quarter")
        p.boxAnimations.seek(clock: 1600)
        XCTAssertEqual(try presentedHeight(node), 100, accuracy: 0.5, "ended: the model's clip")
    }
}
