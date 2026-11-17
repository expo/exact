import Foundation
import XCTest
#if canImport(UIKit)
import UIKit
#endif
@testable import ExactKit

final class RuntimeCompletionTests: XCTestCase {
    private typealias Receive = (RuntimeCompletion) -> Void
    private typealias Submit = (Runtime, @escaping Receive) -> Void

    private var paths: [(String, Submit)] {
        let timer = NativeTimerWake(index: 0, generation: 1, due: 10, interval: 10, tolerance: 0, once: false)
        return [
            ("replies", { $0.pumpAsync(now: 10, done: $1) }),
            ("fill", { $0.collectionFeedbackAsync(Data(), now: 10, done: $1) }),
            ("canvas", { $0.canvasDrawAsync(done: $1) }),
            ("notice", { $0.later([{ $0.pump(now: 10) }], done: $1) }),
            ("tick", { $0.tickAsync(now: 10, done: $1) }),
            ("timer admission", { $0.timerWaitsAsync(kinds: 1, cutoff: 10, done: $1) }),
            ("native timer", { $0.fireNativeTimerAsync(timer, now: 10, done: $1) }),
            ("advance", { $0.advanceAsync(now: 10, done: $1) }),
        ]
    }

    func testScrollFactsKeepObservedOrderAndRejectOnlySupersededOrRetiredWrites() {
        let runtime = Runtime(), other = Runtime()
        defer { runtime.destroy(); other.destroy() }
        let door = runtime.completions, lease = other.completionLease
        var seen: [Int] = []
        let current = door.lease()
        let first = door.scroll(current, view: 1) { seen.append(10) }
        let second = door.scroll(current, view: 1) { seen.append(20) }
        let anotherView = door.scroll(current, view: 2) { seen.append(25) }
        first(); second()
        XCTAssertEqual(seen, [10, 20], "observed scrolls keep every queued delivery")
        let queued = door.scroll(current, view: 1) { seen.append(30) }
        let quiet = door.scroll(current, view: 1) { seen.append(40) }
        quiet(); queued(); anotherView()
        XCTAssertEqual(seen, [10, 20, 40, 25], "order is per scroller")
        door.scroll(lease, view: 1) { XCTFail("a foreign lease cannot write") }()
        let retired = door.scroll(current, view: 1) { XCTFail("a retired generation cannot write") }
        door.replace()
        retired()
        door.scroll(door.lease(), view: 1) { seen.append(50) }()
        XCTAssertEqual(seen.last, 50)
    }

    func testEveryPostedPathRefusesARetiredGenerationBeforeEnteringRust() {
        refuses { $0.completions.replace() }
    }

    func testScrollOrderingRetainsOnlyOutstandingWrites() {
        let runtime = Runtime()
        defer { runtime.destroy() }
        let door = runtime.completions, lease = door.lease()
        var seen: [Int] = []
        for view in UInt32(1)...1000 {
            var older: (() -> Void)? = door.scroll(lease, view: view) { XCTFail("superseded") }
            let latest = door.scroll(lease, view: view) { seen.append(Int(view)) }
            latest()
            if view == 1 { XCTAssertEqual(door.pendingScrollViewsForTests, 1, "the older write still needs its watermark") }
            older?()
            older = nil
            latest()
        }
        XCTAssertEqual(door.pendingScrollViewsForTests, 0, "drained scrollers retain no history")
        XCTAssertEqual(seen, Array(1...1000), "a consumed write cannot run twice")
        var discarded: (() -> Void)? = door.scroll(lease, view: 1) { XCTFail("discarded") }
        XCTAssertNotNil(discarded)
        XCTAssertEqual(door.pendingScrollViewsForTests, 1)
        discarded = door.scroll(lease, view: 2) {}
        XCTAssertEqual(door.pendingScrollViewsForTests, 1, "mailbox replacement releases the displaced write")
        discarded = nil
        XCTAssertEqual(door.pendingScrollViewsForTests, 0)
        let retired = door.scroll(lease, view: 1) { XCTFail("retired") }
        door.replace()
        let current = door.scroll(door.lease(), view: 1) { seen.append(1001) }
        retired()
        XCTAssertEqual(door.pendingScrollViewsForTests, 1, "an old generation cannot release a new write")
        current()
        XCTAssertEqual(door.pendingScrollViewsForTests, 0)
        XCTAssertEqual(seen.last, 1001)
    }

    func testEveryPostedPathRefusesADestroyedSessionBeforeEnteringRust() {
        refuses { $0.destroy() }
    }

    private func refuses(_ retire: (Runtime) -> Void) {
        let runtime = Runtime()
        defer { runtime.destroy() }
        var reads = 0
        runtime.observeBatch = { _, _ in reads += 1 }
        var replies: [(String, RuntimeCompletion)] = []
        // Retirement runs before these jobs without a clock, sleep or race.
        Owner.shared.sync {
            for (name, submit) in paths { submit(runtime) { replies.append((name, $0)) } }
            retire(runtime)
        }
        Owner.shared.sync {}
        XCTAssertEqual(replies.count, paths.count, "every refused job releases its caller")
        XCTAssertEqual(reads, 0, "a stale job must not call the C ABI, even if its batch would be dropped")
        for (name, reply) in replies {
            XCTAssertTrue(reply.batch.held, name)
            XCTAssertFalse(runtime.accepts(reply.lease), name)
        }
    }

    func testEveryPostedPathKeepsItsLeaseThroughPublication() {
        let runtime = Runtime(), other = Runtime()
        defer { runtime.destroy(); other.destroy() }
        var reads = 0
        runtime.observeBatch = { _, _ in reads += 1 }
        var replies: [(String, RuntimeCompletion)] = []
        for (name, submit) in paths { submit(runtime) { replies.append((name, $0)) } }
        Owner.shared.sync {}
        XCTAssertEqual(replies.count, paths.count)
        XCTAssertEqual(reads, paths.count, "live jobs reach the real runtime")
        for (name, reply) in replies {
            XCTAssertFalse(reply.batch.held, name)
            XCTAssertTrue(runtime.accepts(reply.lease), name)
            XCTAssertFalse(other.accepts(reply.lease), "another session at the same generation: \(name)")
        }
        Owner.shared.sync { runtime.completions.replace() }
        for (name, reply) in replies { XCTAssertFalse(runtime.accepts(reply.lease), "retired before apply: \(name)") }
    }

    func testADeferredSequenceChecksTheLeaseBeforeEachCall() {
        let runtime = Runtime()
        defer { runtime.destroy() }
        var calls = 0, replies: [RuntimeCompletion] = []
        runtime.later([
            { r in calls += 1; r.completions.replace(); return Runtime.busy },
            { _ in calls += 1; return Runtime.busy },
        ]) { replies.append($0) }
        Owner.shared.sync {}
        XCTAssertEqual(calls, 1)
        XCTAssertEqual(replies.count, 2)
        XCTAssertTrue(replies[1].batch.held)
    }

    func testAdmissionCannotMoveAJobToANewerGeneration() {
        let runtime = Runtime()
        defer { runtime.destroy() }
        var reads = 0, replies: [RuntimeCompletion] = []
        runtime.observeBatch = { _, _ in reads += 1 }
        runtime.beforeEvent = { [weak runtime] in runtime?.completions.replace() }
        runtime.pumpAsync(now: 10) { replies.append($0) }
        Owner.shared.sync {}
        XCTAssertEqual(reads, 0)
        XCTAssertEqual(replies.count, 1)
        XCTAssertFalse(runtime.accepts(replies[0].lease))
    }

    func testReplacementRetiresOnlyAfterTheNativeCommitSucceeds() throws {
        let runtime = Runtime()
        defer { runtime.destroy() }
        let before = runtime.completionLease
        XCTAssertNotNil(runtime.bootPlan(Data(), width: 400, height: 600).error)
        XCTAssertTrue(runtime.accepts(before), "a refused boot keeps the running generation")
        let plan = try fixturePlan("ASYNC_ADVANCE")
        XCTAssertNil(runtime.bootPlan(plan, width: 400, height: 600).error)
        XCTAssertFalse(runtime.accepts(before))
        let booted = runtime.completionLease
        XCTAssertNil(runtime.preparePlan(plan, width: 400, height: 600).error)
        XCTAssertTrue(runtime.accepts(booted), "preparing a candidate does not retire its predecessor")
        XCTAssertNil(runtime.commitPlan().error)
        XCTAssertFalse(runtime.accepts(booted))
        let committed = runtime.completionLease
        XCTAssertNotNil(runtime.commitPlan().error, "there is no second prepared candidate")
        XCTAssertTrue(runtime.accepts(committed))
    }

    #if os(iOS) || os(tvOS)
    private func grid() throws -> (ExactSession, UIWindow, NodeView, UIScrollView) {
        let session = ExactApp.shared.makeSession(label: "publication-grid")
        let p = session.presenter
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 700))
        window.addSubview(p.viewport); p.viewport.frame = window.bounds; window.makeKeyAndVisible()
        XCTAssertNil(session.boot(plan: try fixturePlan("SHOP_SCROLL"), size: window.bounds.size).error)
        waitForActivation(session)
        p.scrollPump.settle(); session.drainFill()
        waitForMainQueue("initial grid publications")
        let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "product-grid" })
        return (session, window, node, try XCTUnwrap(node.scrollView))
    }

    private func publish(_ batch: Batch, to session: ExactSession) {
        session.noticesInFlight += 1
        session.publish(.notice, RuntimeCompletion(lease: session.runtime.completionLease, batch: batch))
    }

    func testALargePublicationLandsOutsideTheScrollCallback() throws {
        let (session, window, node, scroll) = try grid()
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter
        node.scrollEnd = ScrollEnd(); node.scrollEnd?.live = true
        scroll.contentOffset.y = 1
        waitForMainQueue("warm scroll callback")
        var ops: [[String: Any]] = []
        for id in 1_000_000..<1_002_000 {
            ops.append(["op": "create", "id": id, "kind": "view"])
            ops.append(["op": "frame", "id": id, "x": 0, "y": 0, "w": 100, "h": 100])
        }
        var landed: [Int] = [], inCallback = false
        session.onLandForTests = { _, batch in
            XCTAssertFalse(inCallback, "an atomic publication never spends the scroll callback's frame budget")
            landed += batch.journal.compactMap(Int.init)
        }
        let drains = session.publishedDrainCountForTests
        for n in 0..<8 {
            var batch = n == 0 ? wireBatch(ops) : wireBatch([])
            batch.journal = [String(n)]
            publish(batch, to: session)
        }
        inCallback = true
        let began = CACurrentMediaTime()
        scroll.contentOffset.y = 2
        let duration = CACurrentMediaTime() - began
        inCallback = false
        XCTAssertLessThan(duration, 0.002)
        XCTAssertTrue(landed.isEmpty)
        XCTAssertTrue(session.hasPublished())
        XCTAssertNil(p.views[1_000_000])
        XCTAssertTrue(p.collections.covers(node.id))
        waitForMainQueue("the ordered publications drain together")
        XCTAssertEqual(landed, Array(0..<8))
        XCTAssertEqual(session.publishedDrainCountForTests - drains, 1, "all ready publications share one scheduled drain")
        XCTAssertFalse(session.hasPublished())
        XCTAssertNotNil(p.views[1_001_999])
        XCTAssertTrue(p.collections.covers(node.id))
        XCTContext.runActivity(named: "T large publication ops=4000 scroll_callback_ms=\(duration * 1000)") { _ in }
    }

    func testScrollHandlerResultsApplyBeforeReturningWithEarlierPublicationsInOrder() throws {
        let (session, window, node, scroll) = try grid()
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter
        let target = try XCTUnwrap(p.collections.entries[node.id]?.snapshot.rows.first?.root)
        let view = try XCTUnwrap(p.views[target])
        var landed: [Int] = []
        session.onLandForTests = { _, batch in landed += batch.journal.compactMap(Int.init) }
        p.onScroll = { _, event in
            session.applyUnlessEmpty(wireBatch([["op": "frame", "id": target,
                "x": event[1], "y": 0, "w": 400, "h": 330]]), scrolled: true)
        }
        node.handlers.insert("scroll")
        node.scrollEnd = ScrollEnd(); node.scrollEnd?.live = true
        for frame in 1...3 {
            for n in 0..<8 {
                var batch = wireBatch([]); batch.journal = [String(frame * 10 + n)]
                publish(batch, to: session)
            }
            scroll.contentOffset.y = CGFloat(frame)
            node.sendScrollEvent()
            XCTAssertEqual(view.frame.minX, CGFloat(frame), "the handler's current offset applies in this frame")
            XCTAssertFalse(session.hasPublished())
            XCTAssertEqual(landed, (1...frame).flatMap { f in (0..<8).map { f * 10 + $0 } })
        }
    }

    #if os(iOS)
    func testNativeMeasurementsFollowAllReadyPublications() {
        for delivery in ["queue", "read", "event"] {
            measuredPublications(delivery: delivery)
        }
    }

    private func measuredPublications(delivery: String) {
        let session = ExactApp.shared.makeSession(label: "measured-publications")
        defer { session.destroy() }
        let p = session.presenter
        var landed: [Int] = [], measuredAfter: [Int] = []
        let intrinsic = p.onIntrinsic
        p.onIntrinsic = { sizes in
            measuredAfter.append(landed.count)
            intrinsic?(sizes)
        }
        session.onLandForTests = { _, batch in
            landed += batch.journal.compactMap(Int.init)
            if landed.count == 1 {
                XCTAssertTrue(p.controls.owesSizes, "the first batch mounted a control with an intrinsic measurement")
                XCTAssertTrue(p.controls.controls[2] is UISwitch)
            }
        }
        for n in 0..<8 {
            var batch = n == 0 ? wireBatch([
                ["op": "create", "id": 1, "kind": "view"],
                ["op": "create", "id": 2, "kind": "control",
                 "props": ["type": "checkbox", "accessibilityRole": "switch", "checked": "false"]],
                ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
                ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 100, "h": 50],
            ]) : wireBatch([])
            batch.journal = [String(n)]
            publish(batch, to: session)
        }
        switch delivery {
        case "read": session.drainFill()
        case "event": session.apply(wireBatch([]))
        default:
            waitForMainQueue("the ready publications and their native measurements")
        }
        XCTAssertEqual(landed, Array(0..<8))
        XCTAssertEqual(measuredAfter, [8], "every ready publication lands before native sizes report")
        XCTAssertFalse(p.controls.owesSizes)
        XCTAssertFalse(session.hasPublished())
    }
    #endif

    func testPinsReportAfterTheFillPublicationLands() throws {
        let (session, window, node, _) = try grid()
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter
        let entry = try XCTUnwrap(p.collections.entries[node.id])
        let pin = try XCTUnwrap(entry.snapshot.rows.first?.root)
        XCTAssertNotNil(entry.lastFacts)
        XCTAssertEqual(p.collections.owningCollection(pin), node.id)
        var reports = 0
        let feedback = p.collections.onFeedback
        p.collections.onFeedback = { reports += 1; feedback?($0) }
        session.holdFillForTests()
        p.collections.pointer(pin)
        XCTAssertEqual(reports, 0, "a pin report waits for the in-flight fill")
        session.publish(.fill(node.id), RuntimeCompletion(lease: session.runtime.completionLease, batch: wireBatch([])))
        waitFor("the landed fill releases its pin report") { entry.lastFacts?.interaction == pin }
        XCTAssertGreaterThan(reports, 0)
        XCTAssertNotNil(p.views[pin])
    }

    func testDestructionFinishesBeforeTheNextScrollCallback() {
        let session = ExactApp.shared.makeSession(label: "synchronous-scroll-destruction")
        defer { session.destroy() }
        session.presenter.apply(wireBatch([["op": "create", "id": 1, "kind": "view"]]))
        var order: [String] = []
        session.onLandForTests = { _, _ in order.append("earlier") }
        for _ in 0..<3 { publish(wireBatch([]), to: session) }
        session.apply(wireBatch([["op": "destroy", "id": 1]]))
        XCTAssertEqual(order, ["earlier", "earlier", "earlier"])
        XCTAssertNil(session.presenter.views[1], "destruction cannot outlive its synchronous report")
        XCTAssertFalse(session.hasPublished())
    }

    func testDocumentPublicationsDeliverBetweenCallbacksAndDuringTracking() {
        CFRunLoopAddCommonMode(CFRunLoopGetMain(), CFRunLoopMode(rawValue: RunLoop.Mode.tracking.rawValue as CFString))
        let session = ExactApp.shared.makeSession(label: "document-publication")
        defer { session.destroy() }
        let pump = session.presenter.scrollPump, scroll = session.presenter.viewportScroll
        var landed = 0
        session.onLandForTests = { _, _ in landed += 1 }
        publish(wireBatch([]), to: session)
        pump.scrollViewDidScroll(scroll)
        XCTAssertEqual(landed, 0)
        waitFor("delivery between active callbacks") { landed == 1 }
        XCTAssertEqual(landed, 1)
        pump.scrollViewDidScroll(scroll)
        publish(wireBatch([]), to: session)
        var deliveryMode: RunLoop.Mode?
        session.onLandForTests = { _, _ in
            landed += 1
            deliveryMode = RunLoop.current.currentMode
        }
        let deadline = Date().addingTimeInterval(5)
        while landed < 2, Date() < deadline {
            RunLoop.main.run(mode: .tracking, before: Date().addingTimeInterval(0.005))
        }
        XCTAssertEqual(landed, 2, "a stationary finger delivers without another scroll callback")
        XCTAssertEqual(deliveryMode, .tracking)
        XCTAssertFalse(session.hasPublished())
    }

    func testBackgroundPublicationsDoNotWaitForDisplayFrames() {
        let session = ExactApp.shared.makeSession(label: "background-publications")
        defer {
            session.destroy()
            NotificationCenter.default.post(name: UIApplication.willEnterForegroundNotification, object: nil)
            AppBackground.invalidate()
        }
        NotificationCenter.default.post(name: UIApplication.willEnterForegroundNotification, object: nil)
        var landed = 0
        session.onLandForTests = { _, _ in landed += 1 }
        for _ in 0..<3 { publish(wireBatch([]), to: session) }
        XCTAssertEqual(landed, 0)
        NotificationCenter.default.post(name: UIApplication.didEnterBackgroundNotification, object: nil)
        waitForMainQueue("the background publications drain")
        XCTAssertEqual(landed, 3)
        publish(wireBatch([]), to: session)
        waitFor("a background publication's main-queue delivery") { landed == 4 }
    }

    func testDestructionDiscardsQueuedPublications() {
        let session = ExactApp.shared.makeSession(label: "cancel-publication-frames")
        var landed = 0
        session.onLandForTests = { _, _ in landed += 1 }
        for _ in 0..<3 { publish(wireBatch([]), to: session) }
        session.destroy()
        waitForMainQueue("stale publications release their batches")
        XCTAssertFalse(session.hasPublished())
        XCTAssertEqual(session.noticesInFlight, 0)
        XCTAssertEqual(landed, 0)
    }
    #endif

    func testACompletionCannotApplyToAnotherSession() {
        let a = ExactApp.shared.makeSession(label: "door-a"), b = ExactApp.shared.makeSession(label: "door-b")
        defer { a.destroy(); b.destroy() }
        let before = b.appliedBatches
        b.noticesInFlight += 1
        b.publish(.notice, RuntimeCompletion(lease: a.runtime.completionLease,
            batch: Batch(ops: [], timers: true, motion: false, clock: nil, error: nil, timerDueMs: 123)))
        b.drainFill()
        XCTAssertEqual(b.appliedBatches, before)
        XCTAssertEqual(b.noticesInFlight, 0)
        XCTAssertNil(b.timerDue)
    }

    func testAStalePreparedCandidateKeepsTheLiveRunnersLeases() throws {
        let session = ExactApp.shared.makeSession(label: "door-stale-candidate")
        defer { session.destroy() }
        let runtime = session.runtime, plan = try fixturePlan("ASYNC_ADVANCE")
        XCTAssertNil(runtime.bootPlan(plan, width: 400, height: 600).error)
        let live = runtime.completionLease, platform = SessionLease(session)
        XCTAssertNil(runtime.preparePlan(plan, width: 400, height: 600).error)
        XCTAssertNil(runtime.advance(now: 1000).error)
        let state = runtime.agent(#"{"op":"state"}"#)

        let refused = runtime.commitPlan()
        XCTAssertNil(refused.error)
        XCTAssertTrue(refused.pending, "the live clock changed after preparation")
        XCTAssertEqual(runtime.agent(#"{"op":"state"}"#), state, "the live runner stays current")
        XCTAssertTrue(runtime.accepts(live), "outstanding completions still belong to the live runner")
        XCTAssertTrue(platform.session === session, "platform callbacks keep their requesting session")
    }
}
