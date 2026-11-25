import QuartzCore
import XCTest
@testable import ExactKit
@testable import ExactAnimations

/// A held CSS animation (an authored pause, or a play waiting for the first
/// presented frame, LLP 1003.001 D4): on screen it is held at `speed` 0; in
/// a tree a `CARenderer` draws (a live filter picture) it runs, placed from
/// the render's own instant, so drawn then it shows exactly where it is held.
class CssAnimationsHeldTests: XCTestCase {
    func testHeldScaleKeepsBothAxisTracks() throws {
        let layer = CALayer()
        for (axis, end) in [("x", 1.0), ("y", 0.1)] {
            let spec = try XCTUnwrap(CssAnimation(["id": axis, "k": "transform.scale.\(axis)", "s": 0.0, "d": 1.0,
                "n": 1.0, "t": [0.0, 1.0], "v": [1.0, end], "fill": 3, "h": 0.5]))
            let track = try XCTUnwrap(CssAnimations.make(spec, layer: layer, clock: nil) as? CAKeyframeAnimation)
            XCTAssertEqual(track.keyPath, "transform.scale.\(axis)")
            XCTAssertEqual(track.values as? [Double], [1, end])
            XCTAssertEqual(track.speed, 0)
            XCTAssertEqual(track.timeOffset, 0.5, accuracy: 1e-9)
        }
    }

    func testAHeldAnimationIsPlacedFromTheRenderInstantOffscreen() throws {
        let layer = CALayer()
        let spec = try XCTUnwrap(CssAnimation(["id": "fade#0#opacity", "k": "opacity", "s": 0.0, "dl": 0.0, "d": 1.0, "n": 1.0,
                                                       "t": [0.0, 1.0], "v": [0.0, 1.0], "c": [Any](), "fill": 3, "h": 0.25]))
        let drawn: CFTimeInterval = 1000
        let offscreen = try XCTUnwrap(CssAnimations.make(spec, layer: layer, clock: nil, offscreen: drawn))
        XCTAssertEqual(offscreen.speed, 1)
        XCTAssertEqual(offscreen.beginTime, layer.convertTime(drawn, from: nil) - 0.25, accuracy: 1e-9)
        let onscreen = try XCTUnwrap(CssAnimations.make(spec, layer: layer, clock: nil))
        XCTAssertEqual(onscreen.speed, 0)
        XCTAssertEqual(onscreen.timeOffset, 0.25, accuracy: 1e-9)
    }
}
