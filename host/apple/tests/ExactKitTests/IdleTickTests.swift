import XCTest
#if os(iOS)
import UIKit
#endif
@testable import ExactKit

/// A timer's (or a scroll event's) batch that changes nothing only moves the clock: it does not
/// reach `apply`, as fills and list feedback already skip theirs, and the
/// next timer is armed for its deadline. A batch with ops, an error, a
/// control's viewless contents (`controls`, LLP 1069.011 §9), new motion,
/// moved page facts, a wake lock or word on work the app must finish still applies. The flag crosses the wire
/// as `"controls":true`. A published batch that shows nothing new is not outstanding work either.
final class IdleTickTests: XCTestCase {
    /// A published batch (a reply's, a frame's tick's) that shows nothing new
    /// does not hold settled or TTI (LLP 1132 D5); one that changes a view or
    /// asks for a canvas image holds them until it lands. A slice in flight
    /// holds them as `fill`, whatever its batch.
    func testAPublishedBatchIsOutstandingOnlyWhenItShowsSomethingNew() throws {
        let session = ExactApp.shared.makeSession(label: "published-not-loading")
        defer { session.destroy() }
        func reply(_ batch: Batch) {
            session.repliesInFlight += 1; session.replySubmissions.append(0)
            session.publish(.reply, batch, session.generation)
        }
        session.publishTickForTests(wireBatch([]))
        reply(wireBatch([]))
        // Work the app must finish is a hold, and an error is only logged: neither is loading
        var finished = wireBatch([])
        finished.mustFinish = false
        reply(finished)
        reply(Batch(ops: [], timers: false, motion: false, clock: nil, error: "boom"))
        XCTAssertEqual(session.hostOutstanding, [])
        reply(wireBatch([["op": "props", "id": 1, "set": ["testId": "x"]]]))
        XCTAssertEqual(session.hostOutstanding, ["published"])
        waitFor("the batches land") { session.hostOutstanding.isEmpty && !session.ownerInFlight }
        var image = wireBatch([])
        image.canvasImages = ["missing-canvas-image.png"]
        session.publishTickForTests(image)
        XCTAssertEqual(session.hostOutstanding, ["published"])
        waitFor("the tick lands") { session.hostOutstanding.isEmpty && !session.ownerInFlight }
        session.publishFillForTests(1, wireBatch([]), generation: session.generation)
        XCTAssertEqual(session.hostOutstanding, ["fill"])
        waitFor("the slice lands") { session.hostOutstanding.isEmpty }
    }

    func testAnInertOrientationRequestAppliesWithoutHoldingSettled() {
        let session = ExactApp.shared.makeSession(label: "inert-orientation")
        defer { session.destroy() }
        var batch = wireBatch([])
        batch.orientation = .init()
        batch.orientation?.types = 1
        XCTAssertTrue(session.showsNothing(batch), "an embedded session cannot turn its host's window")
        XCTAssertFalse(session.changesNothing(batch), "the request must still reach the presenter and readback")
        session.publishTickForTests(batch)
        XCTAssertEqual(session.hostOutstanding, [])
        waitFor("the orientation request lands") { !session.tickInFlight }
        XCTAssertEqual(session.presenter.orientationLock.request.types, 1)
    }

    #if os(iOS)
    func testAnIPadTypeRequestLeavesSettledAloneButALockChangeDoesNot() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "orientation resolution needs a window scene") }
        guard scene.traitCollection.userInterfaceIdiom == .pad else { throw XCTSkip("n/a: iPad orientation policy") }
        guard #available(iOS 26, *) else { throw XCTSkip("n/a: current orientation lock needs iPadOS 26") }
        let session = ExactApp.shared.makeSession(label: "ipad-orientation")
        session.hatchesOwnWindow = true
        let window = UIWindow(windowScene: scene), host = UIViewController()
        window.rootViewController = host
        window.makeKeyAndVisible()
        let view = ExactView(session: session)
        host.view.addSubview(view)
        view.frame = host.view.bounds
        defer { session.destroy(); window.isHidden = true }
        var batch = wireBatch([])
        batch.orientation = .init()
        batch.orientation?.types = 1
        XCTAssertTrue(session.showsNothing(batch), "an iPad ignores an orientation type")
        session.applyUnlessEmpty(batch)
        XCTAssertEqual(session.presenter.orientationLock.why, "an iPad applies no type")
        XCTAssertFalse(session.presenter.orientationLock.locked)
        batch.orientation?.types = 15
        batch.orientation?.current = true
        XCTAssertFalse(session.showsNothing(batch), "a current lock changes the controller's answer")
        session.applyUnlessEmpty(batch)
        XCTAssertTrue(session.presenter.orientationLock.locked)
        XCTAssertTrue(session.showsNothing(batch), "repeating the held lock changes nothing")
        batch.orientation = .init()
        XCTAssertFalse(session.showsNothing(batch), "releasing the lock changes the controller's answer")
        session.applyUnlessEmpty(batch)
        XCTAssertFalse(session.presenter.orientationLock.locked)
    }
    #endif

    func testATickThatChangesNothingOnlyMovesTheClock() throws {
        let session = ExactApp.shared.makeSession(label: "idle-tick")
        defer { session.destroy() }
        let before = session.appliedBatches
        let due = session.now() + 250
        var idle = Batch(ops: [], timers: true, motion: false, clock: nil, error: nil, timerDueMs: due, pending: false)
        XCTAssertTrue(session.changesNothing(idle))
        session.applyUnlessEmpty(idle)
        XCTAssertEqual(session.appliedBatches, before, "an idle tick does not reach apply")
        XCTAssertEqual(session.timerDue, due, "its deadline is kept")
        if !ExactEnv.agentMode {
            XCTAssertEqual(session.clockDue, due, "and the next timer armed for it")
            XCTAssertEqual(session.clockTimer?.isValid, true)
        }
        // A later deadline re-arms; no deadline invalidates what was armed.
        let armed = session.clockTimer
        idle.timerDueMs = due + 250
        session.applyUnlessEmpty(idle)
        if !ExactEnv.agentMode {
            XCTAssertEqual(session.clockDue, due + 250)
            XCTAssertEqual(armed?.isValid, false, "the earlier timer is replaced")
        }
        let later = session.clockTimer
        if !ExactEnv.agentMode { XCTAssertEqual(later?.isValid, true, "the successor is armed") }
        idle.timerDueMs = nil
        session.applyUnlessEmpty(idle)
        XCTAssertEqual(session.appliedBatches, before)
        XCTAssertNil(session.clockTimer)
        XCTAssertNil(session.clockDue)
        XCTAssertNotEqual(later?.isValid, true, "and invalidated")
        // A scroll event's batch is no timer's (`timers` false): the same.
        let scrolled = Batch(ops: [], timers: false, motion: false, clock: nil, error: nil, timerDueMs: due)
        session.applyUnlessEmpty(scrolled)
        XCTAssertEqual(session.appliedBatches, before, "an empty scroll event does not reach apply")
        XCTAssertEqual(session.timerDue, due)
        // Any other event's empty batch still applies: the pass reconciles a
        // native control that changed itself first, and seeks native
        // animations to the agent's clock.
        session.apply(scrolled)
        XCTAssertEqual(session.appliedBatches, before + 1, "an ordinary empty batch reaches apply")
        // Each of these is news: the tick applies.
        var controls = Batch(ops: [], timers: true, motion: false, clock: nil, error: nil, timerDueMs: due)
        controls.controls = true
        let failed = Batch(ops: [], timers: true, motion: false, clock: nil, error: "boom", timerDueMs: due)
        let moving = Batch(ops: [], timers: true, motion: true, clock: nil, error: nil, timerDueMs: due)
        let ops = wireBatch([["op": "props", "id": 1, "set": ["testId": "x"]]])
        // A timer's own `setMode` or `setRootFontSize` (page facts), wake lock or word on work to finish, with no node changed.
        var facts = Batch(ops: [], timers: true, motion: false, clock: nil, error: nil, timerDueMs: due)
        facts.facts = true
        var wakelock = Batch(ops: [], timers: true, motion: false, clock: nil, error: nil, timerDueMs: due)
        wakelock.wakelock = false
        var finish = Batch(ops: [], timers: true, motion: false, clock: nil, error: nil, timerDueMs: due)
        finish.mustFinish = false
        for batch in [controls, failed, moving, ops, facts, wakelock, finish] {
            XCTAssertFalse(session.changesNothing(batch))
            let count = session.appliedBatches
            session.applyUnlessEmpty(batch)
            XCTAssertEqual(session.appliedBatches, count + 1)
        }
    }

    /// A slice the owner published before an empty scroll batch lands
    /// first, and the scroll batch then still skips the pass: a fling's
    /// fills are often in flight.
    func testAnEmptyScrollBatchBehindAPublishedSliceLandsItAndSkipsThePass() throws {
        let session = ExactApp.shared.makeSession(label: "idle-behind-slice")
        defer { session.destroy() }
        session.clock = 0
        let due = session.now() + 250
        session.publishTickForTests(wireBatch([["op": "props", "id": 1, "set": ["testId": "x"]]]))
        XCTAssertTrue(session.tickInFlight)
        let before = session.appliedBatches
        session.applyUnlessEmpty(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil, timerDueMs: due), scrolled: true)
        XCTAssertFalse(session.tickInFlight, "the slice landed")
        XCTAssertEqual(session.appliedBatches, before + 1, "the slice's pass, not the empty scroll batch's")
        XCTAssertEqual(session.timerDue, due, "the scroll batch's deadline, the owner's latest")
        // A report the landing owes waits for the batch, then is asked: not stranded by the skip.
        session.publishTickForTests(wireBatch([["op": "props", "id": 1, "set": ["testId": "y"]]]))
        session.oweColorsForTests()
        let held = session.appliedBatches
        session.applyUnlessEmpty(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil, timerDueMs: due), scrolled: true)
        XCTAssertEqual(session.appliedBatches, held + 2, "the slice, then the colours' report; the scroll batch skipped")
        XCTAssertNotEqual(session.timerDue, due, "the report applied after the skip, its clock the latest (an unbooted runtime's has none)")
    }

    func testWorkWaitingForAStaleSliceRunsAfterTheCommitAndDropsOldViewReports() {
        let session = ExactApp.shared.makeSession(label: "stale-slice")
        defer { session.destroy() }
        var calls: [String] = []
        session.holdFillForTests()
        session.whenIdle { calls.append("pump") }
        session.whenIdle(bound: true, { calls.append("old views") }, cancelled: { calls.append("cancelled") })
        let staleGeneration = session.generation
        session.commitForTests()
        session.whenIdle(bound: true) { calls.append("new views") }
        session.publishFillForTests(1, wireBatch([["op": "create", "id": 999, "kind": "view"]]), generation: staleGeneration)
        session.drainFill()
        XCTAssertTrue(calls.isEmpty, "the commit's tree must apply before its waiters")
        XCTAssertNil(session.presenter.views[999], "a stale slice applies nothing")
        let finished = expectation(description: "stale waiters released")
        DispatchQueue.main.async { finished.fulfill() }
        wait(for: [finished], timeout: 5)
        XCTAssertEqual(calls, ["pump", "cancelled", "new views"])
        session.holdFillForTests()
        session.whenIdle { calls.append("same generation") }
        session.publishFillForTests(1, Batch(ops: [], timers: false, motion: false, clock: nil, error: nil), generation: session.generation)
        session.drainFill()
        XCTAssertEqual(calls.last, "same generation")
    }

    func testTheControlsFlagDecodes() throws {
        let data = Data(#"{"ops":[],"timers":false,"motion":false,"controls":true}"#.utf8)
        XCTAssertTrue(Batch.decode(data).controls)
        XCTAssertFalse(Batch.decode(Data(#"{"ops":[],"timers":false,"motion":false}"#.utf8)).controls)
    }
}
