// @ref LLP 1156.008.001 A1-A3 — Apple runtime fixes:
// A1: upward Float width rounding, 1/64 pt room snapping, dual-TextEngine preference invalidation.
// A2: nested sharedElement outer-to-inner z-ordering, resting destination geometry without moving-clip chase,
//     and orphan-in-FlightLayer landing cleanup.
// A3: public-API Obj-C @try/@catch bridge for AVAudioEngine and engine rebuild on caught exceptions.
import AVFoundation
import XCTest
@testable import ExactCore
@testable import ExactKit
@testable import ExactFlights
@testable import MediaCaptureImplementation
import MediaCaptureObjC
#if os(macOS)
import AppKit
#else
import UIKit
#endif

final class AppleRuntimeFixesTests: XCTestCase {
    // MARK: A1 — Text rounding, 1/64 pt room snapping, and dual-engine invalidation

    func testCeilFloatWidthNeverTruncatesBelowCGFloat() {
        // Values whose exact 1/64 pt or subpixel Double rounds down in Float32.
        let samples: [CGFloat] = [
            100.0 + 1.0 / 64.0,
            1024.0 + 1.0 / 64.0,
            16777216.0 - 0.25,
            317.3333333333333,
            8192.015625,
        ]
        for w in samples {
            let f = CSSLineBox.ceilFloatWidth(w)
            XCTAssertGreaterThanOrEqual(Double(f), Double(w), "width \(w) must not truncate below its Double value")
        }
        // Exact powers of two stay exact without bumping up.
        XCTAssertEqual(CSSLineBox.ceilFloatWidth(128.0), 128.0)
        XCTAssertEqual(CSSLineBox.ceilFloatWidth(0.0), 0.0)
    }

    func testSnapRoomHealsSubHalfUlpRoundingWithoutDistortingLargeWidths() {
        // Half an f32 ULP below an exact 1/64 pt grid value snaps up to the grid step.
        let exactGrid = 200.0 + 3.0 / 64.0
        let tinyShort = exactGrid - Double(Float(exactGrid).ulp) * 0.25
        XCTAssertEqual(LineBreakPlan.snapRoom(tinyShort), exactGrid)

        // Values genuinely below the grid step (more than half an f32 ULP away) stay untouched.
        let genuinelyNarrower = 1200.0 - 0.005
        XCTAssertEqual(LineBreakPlan.snapRoom(genuinelyNarrower), genuinelyNarrower)

        // Non-finite and non-positive rooms pass through unchanged.
        XCTAssertTrue(LineBreakPlan.snapRoom(.infinity).isInfinite)
        XCTAssertEqual(LineBreakPlan.snapRoom(0), 0)
    }

    func testInvalidatePreferencesClearsPainterAndMeasurerCaches() {
        let painter = TextEngine(assets: { _ in nil })
        let measurer = TextEngine(assets: { _ in nil })
        painter.connect(measurer: measurer, answerBytes: 1 << 16)

        _ = painter.font(size: 15, weight: 400, family: 0, italic: false)
        Owner.shared.sync {
            _ = measurer.font(size: 15, weight: 400, family: 0, italic: false)
        }
        XCTAssertFalse(painter.fonts.isEmpty)
        let measurerHadFonts = Owner.shared.sync { !measurer.fonts.isEmpty }
        XCTAssertTrue(measurerHadFonts)

        let beforePainterNS = painter.namespace
        let beforeMeasurerNS = Owner.shared.sync { measurer.namespace }

        painter.invalidatePreferences()
        Owner.shared.sync { measurer.invalidatePreferences() }

        XCTAssertTrue(painter.fonts.isEmpty)
        XCTAssertEqual(painter.namespace, beforePainterNS + 1)
        let after = Owner.shared.sync { (measurer.fonts.isEmpty, measurer.namespace) }
        XCTAssertTrue(after.0)
        XCTAssertEqual(after.1, beforeMeasurerNS + 1)
    }

    // MARK: A2 — Nested sharedElement z-order, resting destination geometry, and orphan landing

    #if os(macOS)
    func testNestedFlightsOrderOuterToInnerAndAvoidMovingClipDestinationChase() throws {
        _ = NSApplication.shared
        let p = Presenter()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 600),
                              styleMask: [], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        p.viewport.frame = NSRect(x: 0, y: 0, width: 400, height: 600)
        defer { window.close() }

        // Source tree: root (1) -> leaver card (2) -> leaver badge (3), plus destination holder (4).
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "props": ["testId": "root"]],
            ["op": "create", "id": 2, "kind": "view", "props": ["testId": "card-from"]],
            ["op": "create", "id": 3, "kind": "view", "props": ["testId": "badge-from"]],
            ["op": "create", "id": 4, "kind": "view", "props": ["testId": "holder"]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 1, "ids": [2, 4]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 600.0],
            ["op": "frame", "id": 2, "x": 10.0, "y": 20.0, "w": 100.0, "h": 100.0],
            ["op": "frame", "id": 3, "x": 10.0, "y": 10.0, "w": 30.0, "h": 20.0],
            ["op": "frame", "id": 4, "x": 0.0, "y": 0.0, "w": 400.0, "h": 600.0],
        ]))

        // Destination tree: holder (4) -> arriver card (5) -> arriver badge (6), both flying simultaneously.
        // Card moves from (10,20,100,100) to (100,200,200,200); badge is at local (20,30,60,40) inside card
        // so badge's true resting rect in window is (120,230,60,40).
        p.apply(wireBatch([
            ["op": "flight", "id": 5, "from": 2],
            ["op": "flight", "id": 6, "from": 3],
            ["op": "destroy", "id": 3],
            ["op": "destroy", "id": 2],
            ["op": "create", "id": 5, "kind": "view", "props": ["testId": "card-to"]],
            ["op": "create", "id": 6, "kind": "view", "props": ["testId": "badge-to"]],
            ["op": "children", "id": 5, "ids": [6]],
            ["op": "children", "id": 4, "ids": [5]],
            ["op": "frame", "id": 5, "x": 100.0, "y": 200.0, "w": 200.0, "h": 200.0],
            ["op": "frame", "id": 6, "x": 20.0, "y": 30.0, "w": 60.0, "h": 40.0],
            ["op": "present", "id": 5, "property": "flight", "x": 0.0, "y": 0.0],
            ["op": "present", "id": 6, "property": "flight", "x": 1.0, "y": 0.0],
        ]))

        let card = try XCTUnwrap(p.views[5])
        let badge = try XCTUnwrap(p.views[6])
        let cardClip = try XCTUnwrap(card.superview as? FlightClip)
        let badgeClip = try XCTUnwrap(badge.superview as? FlightClip)
        let layer = try XCTUnwrap(cardClip.superview as? FlightLayer)
        XCTAssertTrue(badgeClip.superview === layer)

        // Outer-to-inner z-order: nested child flight (badgeClip) must paint ABOVE ancestor flight (cardClip).
        let cardIdx = try XCTUnwrap(layer.subviews.firstIndex(of: cardClip))
        let badgeIdx = try XCTUnwrap(layer.subviews.firstIndex(of: badgeClip))
        XCTAssertGreaterThan(badgeIdx, cardIdx, "nested child flight must sit above ancestor flight in FlightLayer")

        // Resting destination geometry: even though card is at progress 0.0 (at (10,20,100,100) scaled 0.5x),
        // badge at progress 1.0 must land at its resting destination (120,230,60,40), NOT chasing card's moving clip!
        XCTAssertEqual(badgeClip.frame.minX, 120, accuracy: 0.5)
        XCTAssertEqual(badgeClip.frame.minY, 230, accuracy: 0.5)
        XCTAssertEqual(badgeClip.frame.width, 60, accuracy: 0.5)
        XCTAssertEqual(badgeClip.frame.height, 40, accuracy: 0.5)
        XCTAssertTrue(p.flightHost.hasFlyingAncestor(badge))

        // Orphan-in-FlightLayer landing: if the slot loses its superview before landing, the view must NOT stay
        // orphaned inside FlightLayer.
        let cardFlight = try XCTUnwrap(p.flights[5])
        cardFlight.slot?.removeFromSuperview()
        p.landFlight(cardFlight)
        XCTAssertFalse(card.superview is FlightLayer, "landed view must never be orphaned inside pass-through FlightLayer")
        XCTAssertFalse(card.superview is FlightClip)
    }
    #endif

    // MARK: A3 — Obj-C @try/@catch bridge catches NSException and triggers engine rebuild

    func testObjCBridgeCatchesNSExceptionAndReturnsExactDomainError() {
        do {
            try ExactMediaCaptureBridge.catchObjCException {
                NSException(name: .genericException, reason: "simulated AVAudioEngine failure", userInfo: nil).raise()
            }
            XCTFail("expected catchObjCException to throw")
        } catch let error as NSError {
            XCTAssertEqual(error.domain, ExactMediaCaptureExceptionDomain)
            XCTAssertTrue(error.localizedDescription.contains("simulated AVAudioEngine failure"))
        }
    }

    func testObjCBridgeStopAndInputFormatHandleEngineSafely() {
        let engine = AVAudioEngine()
        // Calling stop on a fresh never-started engine must never crash or throw.
        ExactMediaCaptureBridge.stop(engine)
        // Checking voice processing or output format either succeeds cleanly or throws a Swift Error (never aborts).
        do {
            _ = try ExactMediaCaptureBridge.voiceProcessingEnabled(for: engine)
        } catch let error as NSError {
            XCTAssertEqual(error.domain, ExactMediaCaptureExceptionDomain)
        }
    }
}
