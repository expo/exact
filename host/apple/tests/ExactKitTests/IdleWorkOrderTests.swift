import XCTest
@testable import ExactKit

/// Work a session runs when idle (`whenIdle`) runs in the order asked (LLP 1072 §3): work that waited for a slice in
/// flight runs after the slice's batch and before work asked while that batch lands, or after a stale slice lands.
final class IdleWorkOrderTests: XCTestCase {
    func testAReportTriggeredByABatchSeesItsAnswerBeforeTheNextReportRuns() {
        let session = ExactApp.shared.makeSession(label: "idle-report-answer")
        defer { session.destroy() }
        var events: [String] = []
        session.onApplyForTests = { [unowned session] batch in
            if batch.journal == ["answer"] { events.append("applied"); return }
            guard batch.journal == ["input"] else { return }
            session.whenIdle {
                events.append("report")
                var answer = wireBatch([])
                answer.journal = ["answer"]
                session.apply(answer)
                events.append("returned")
            }
            session.whenIdle { events.append("next report") }
        }
        var input = wireBatch([])
        input.journal = ["input"]
        session.apply(input)
        XCTAssertEqual(events, ["report", "applied", "returned", "next report"])
    }

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

    /// A covers report queued behind a fill posts asynchronously instead of blocking main on `Owner.sync` when an
    /// earlier waiting reply pump starts work on the owner, preserves its `CoversChain`, holds following `whenIdle`
    /// work until it lands, and settles in one `drainFill()` call (#1375).
    func testWaitingCoversPostsAsyncBehindInFlightReplyAndDrainsToFixedPoint() {
        let session = ExactApp.shared.makeSession(label: "idle-order-fill-then-reply")
        defer { session.destroy() }
        let chain = NSObject()
        let release = DispatchSemaphore(value: 0)
        var events: [String] = []
        session.onLandForTests = { kind, _ in
            if case .covers(let cause) = kind {
                events.append(cause === chain ? "covers-landed-in-chain" : "covers-landed-outside-chain")
            }
        }
        session.holdFillForTests()
        session.whenIdle {
            events.append("pump")
            Owner.shared.post { _ = release.wait(timeout: .now() + 5) }
            session.pumpReplies()
        }
        session.inCoversChain(chain) {
            session.presenter.onCovers?([(1, .whole)])
        }
        session.whenIdle(bound: true) {
            events.append("after-covers")
        }
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
        let began = CACurrentMediaTime()
        session.landFill()
        XCTAssertLessThan(CACurrentMediaTime() - began, 0.5, "landing the fill must not block main on the owner")
        XCTAssertEqual(events, ["pump"], "covers posted asynchronously; later idle work waits until it lands")
        XCTAssertEqual(session.repliesInFlight, 1)
        XCTAssertEqual(session.coversInFlight, 1)
        release.signal()
        session.drainFill()
        XCTAssertEqual(events, ["pump", "covers-landed-in-chain", "after-covers"])
        XCTAssertFalse(session.ownerInFlight)
    }

    /// Direct `onCovers` and `localInsets` calls while a reply is building on the owner post asynchronously without
    /// blocking main, holding later `whenIdle` work (such as carried scroll writes) until they land (#1375).
    func testDirectCoversAndLocalInsetsPostAsyncBehindInFlightReply() {
        let session = ExactApp.shared.makeSession(label: "idle-order-direct-reply")
        defer { session.destroy() }
        let release = DispatchSemaphore(value: 0)
        Owner.shared.post { _ = release.wait(timeout: .now() + 5) }
        session.pumpReplies()
        var ran: [String] = []
        session.onLandForTests = { kind, _ in
            if case .covers = kind { ran.append("covers-landed") }
        }
        let enteredBefore = session.runtime.entered
        let began = CACurrentMediaTime()
        session.presenter.onCovers?([(1, .whole)])
        session.localInsets([(nil, [10, 0, 0, 0])])
        session.whenIdle(bound: true) { ran.append("scrolls") }
        XCTAssertLessThan(CACurrentMediaTime() - began, 0.5)
        XCTAssertTrue(ran.isEmpty, "covers waits on the owner while insets and later idle work queue behind it")
        XCTAssertEqual(session.coversInFlight, 1, "localInsets waits in afterFill until the first covers batch lands")
        XCTAssertEqual(session.runtime.entered, enteredBefore, "neither covers nor insets blocked main on Owner.sync")
        release.signal()
        session.drainFill()
        XCTAssertEqual(ran, ["covers-landed", "scrolls"])
        XCTAssertEqual(session.runtime.entered, enteredBefore + 1, "once the owner is idle, queued localInsets applies synchronously before scrolls")
        XCTAssertFalse(session.ownerInFlight)

        // Direct `localInsets` while a reply is in flight also posts asynchronously without blocking main.
        let release2 = DispatchSemaphore(value: 0)
        Owner.shared.post { _ = release2.wait(timeout: .now() + 5) }
        session.pumpReplies()
        let enteredBefore2 = session.runtime.entered
        let began2 = CACurrentMediaTime()
        session.localInsets([(nil, [20, 0, 0, 0])])
        session.whenIdle(bound: true) { ran.append("after-insets") }
        XCTAssertLessThan(CACurrentMediaTime() - began2, 0.5)
        XCTAssertEqual(session.coversInFlight, 1)
        XCTAssertEqual(session.runtime.entered, enteredBefore2)
        release2.signal()
        session.drainFill()
        XCTAssertEqual(ran, ["covers-landed", "scrolls", "covers-landed", "after-insets"])
        XCTAssertEqual(session.runtime.entered, enteredBefore2)
        XCTAssertFalse(session.ownerInFlight)
    }
}
