import XCTest
@testable import ExactKit

/// Work a session runs when idle (`whenIdle`) runs in the order asked (LLP 1072 §3): work that waited for a slice in
/// flight runs after the slice's batch and before work asked while that batch lands, or after a stale slice lands.
final class IdleWorkOrderTests: XCTestCase {
    func testWorkAskedWhileASliceLandsRunsAfterTheWorkThatWaited() throws {
        let session = ExactApp.shared.makeSession(label: "idle-order-landing")
        defer { session.destroy() }
        var order: [String] = []
        session.holdFillForTests()
        session.whenIdle { order.append("waited") }
        session.onLandForTests = { _, _ in session.whenIdle { order.append("asked while it lands") } }
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
        session.landFill()
        XCTAssertEqual(order, ["waited", "asked while it lands"])
    }

    func testWorkAskedAfterAStaleSliceLandsRunsAfterTheWorkThatWaited() throws {
        let session = ExactApp.shared.makeSession(label: "idle-order-stale")
        defer { session.destroy() }
        var order: [String] = []
        session.holdFillForTests()
        session.whenIdle { order.append("waited") }
        let stale = session.runtime.completionLease
        session.commitForTests()
        session.publishFillForTests(1, wireBatch([]), lease: stale)
        session.landFill()
        session.whenIdle { order.append("asked after it landed") }
        waitForMainQueue("the stale slice's waiting work")
        XCTAssertEqual(order, ["waited", "asked after it landed"])
    }

    /// Work asked in a chain of covers corrections runs in it, queued behind a slice or not, and a slice's batch lands in
    /// none, whatever lands it
    func testQueuedWorkKeepsTheCoversChainItWasAskedInAndASliceLandsInNone() throws {
        let session = ExactApp.shared.makeSession(label: "idle-order-chain")
        defer { session.destroy() }
        let chain = NSObject()
        var ran: [String] = []
        session.holdFillForTests()
        session.inCoversChain(chain) { session.whenIdle { ran.append(session.coversCause === chain ? "in its chain" : "outside it") } }
        session.onLandForTests = { _, _ in ran.append(session.coversCause == nil ? "slice in none" : "slice in a chain") }
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
        session.inCoversChain(chain) { session.landFill() }
        XCTAssertEqual(ran, ["slice in none", "in its chain"])
    }

    /// Work asked while the waiting work runs joins the queue and runs in the same drain, after the work that asked it
    func testWorkAskedWhileTheWaitingWorkRunsRunsInTheSameDrainAfterIt() throws {
        let session = ExactApp.shared.makeSession(label: "idle-order-same-drain")
        defer { session.destroy() }
        var runs = 0, depth = 0, deepest = 0
        func again() {
            depth += 1
            defer { depth -= 1 }
            deepest = max(deepest, depth)
            runs += 1
            if runs < 100 { session.whenIdle { again() } }
        }
        session.holdFillForTests()
        session.whenIdle { again() }
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
        session.landFill()
        XCTAssertEqual(runs, 100, "the drain runs what was asked while it ran")
        XCTAssertEqual(deepest, 1, "each after the work that asked it, none inside it")
    }

    /// A drain that never empties, feedback that never settles, leaves the rest for the main queue's next turn after a
    /// thousand, so UIKit keeps running
    func testADrainThatNeverEmptiesLeavesTheRestForTheNextTurn() throws {
        let session = ExactApp.shared.makeSession(label: "idle-order-never-empties")
        defer { session.destroy() }
        var runs = 0
        func again() { runs += 1; session.whenIdle { again() } }
        session.holdFillForTests()
        session.whenIdle { again() }
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
        session.landFill()
        XCTAssertEqual(runs, 1_000)
        waitForMainQueue("the next turn's drain")
        XCTAssertEqual(runs, 2_000)
    }

    /// A slice landed by work a drain runs leaves the waiting work to that drain, which runs it after the work that landed
    /// the slice; the drain after a stale slice lands is one that can be asked again so
    func testASliceLandedByWaitingWorkLeavesTheRestToTheRunningDrain() throws {
        let session = ExactApp.shared.makeSession(label: "idle-order-nested-landing")
        defer { session.destroy() }
        var order: [String] = []
        session.holdFillForTests()
        session.whenIdle {
            order.append("A starts")
            session.whenIdle { order.append("C") }
            session.publishFillForTests(2, wireBatch([]), lease: session.runtime.completionLease)
            session.landFill()
            order.append("A ends")
        }
        session.whenIdle { order.append("B") }
        let stale = session.runtime.completionLease
        session.commitForTests()
        session.publishFillForTests(1, wireBatch([]), lease: stale)
        session.landFill()
        waitForMainQueue("the stale slice's waiting work")
        XCTAssertEqual(order, ["A starts", "A ends", "B", "C"])
    }
}
