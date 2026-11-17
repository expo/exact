import XCTest
import CoreGraphics
@testable import ExactSurfaces

final class CanvasTouchSamplesTests: XCTestCase {
    func testCoalescedPositionsKeepStampsAndEdgesOnce() {
        var sequence = CanvasTouchSamples()
        var phases: [String] = [], stamps: [Double] = [], deltas: [CGFloat] = []
        let sample = { (x: CGFloat, t: Double) in
            CanvasTouchSamples.Sample(point: CGPoint(x: x, y: 0), timestamp: t)
        }
        let emit = { (phase: String, sample: CanvasTouchSamples.Sample, delta: CGPoint) in
            phases.append(phase); stamps.append(sample.timestamp); deltas.append(delta.x)
        }
        sequence.forward(phase: "down", current: sample(10, 1), coalesced: [sample(10, 1)], emit: emit)
        sequence.forward(phase: "move", current: sample(18, 4),
                         coalesced: [sample(18, 4), sample(12, 2), sample(15, 3)], emit: emit)
        sequence.forward(phase: "move", current: sample(18, 4),
                         coalesced: [sample(15, 3), sample(18, 4)], emit: emit)
        sequence.forward(phase: "up", current: sample(20, 6),
                         coalesced: [sample(19, 5), sample(20, 6)], emit: emit)
        XCTAssertEqual(phases, ["down", "move", "move", "move", "move", "move", "up"])
        XCTAssertEqual(stamps, [1, 2, 3, 4, 5, 6, 6])
        XCTAssertEqual(deltas, [0, 2, 3, 3, 1, 1, 0])
        XCTAssertEqual(deltas.reduce(0, +), 10)
    }

    func testEqualStampMovementAndCancellationKeepFinalDelta() {
        var sequence = CanvasTouchSamples()
        var phases: [String] = [], deltas: [CGFloat] = []
        let sample = { (x: CGFloat) in
            CanvasTouchSamples.Sample(point: CGPoint(x: x, y: 0), timestamp: 1)
        }
        let emit = { (phase: String, _: CanvasTouchSamples.Sample, delta: CGPoint) in
            phases.append(phase); deltas.append(delta.x)
        }
        sequence.forward(phase: "down", current: sample(0), coalesced: [], emit: emit)
        sequence.forward(phase: "move", current: sample(2), coalesced: [sample(1), sample(2)], emit: emit)
        sequence.forward(phase: "cancel", current: sample(3), coalesced: [], emit: emit)
        XCTAssertEqual(phases, ["down", "move", "move", "cancel"])
        XCTAssertEqual(deltas, [0, 1, 1, 1])
    }

    func testBulkCancelKeepsLastPositionAndStamp() {
        var sequence = CanvasTouchSamples()
        let sample = CanvasTouchSamples.Sample(point: CGPoint(x: 73, y: 91), timestamp: 12.5)
        sequence.forward(phase: "down", current: sample, coalesced: []) { _, _, _ in }
        var edges = 0
        let emit = { (phase: String, value: CanvasTouchSamples.Sample, delta: CGPoint) in
            edges += 1
            XCTAssertEqual(phase, "cancel")
            XCTAssertEqual(value.point, sample.point)
            XCTAssertEqual(value.timestamp, sample.timestamp)
            XCTAssertEqual(delta, .zero)
        }
        sequence.cancel(emit: emit)
        sequence.cancel(emit: emit)
        XCTAssertEqual(edges, 1)
    }

}
