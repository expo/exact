import Foundation
import XCTest
#if canImport(UIKit)
import UIKit
#endif
@testable import ExactKit
@testable import ExactCollections
@testable import ExactCore

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
        let runtime = Runtime()
        defer { runtime.destroy() }
        var mailbox = ScrollMailbox(), seen: [Double] = []
        let door = runtime.completions
        func values(_ top: Double, view: UInt32 = 1) -> ScrollMailbox.Values {
            ScrollMailbox.Values(lease: door.lease(), runtime: runtime.rt, view: view,
                position: .offset(page: false, left: 0, top: top))
        }
        func apply(_ note: ScrollMailbox.Note) {
            if mailbox.claim(note), case .offset(_, _, let top) = note.values.position { seen.append(top) }
        }
        for taken in [false, true] {
            mailbox.record(values(10))
            let draining = taken ? mailbox.take() : []
            apply(mailbox.record(values(20), observing: true))
            for note in taken ? draining : mailbox.take() { apply(note) }
            XCTAssertEqual(seen, [20], "an inline owner observation supersedes the older buffered offset")
            XCTAssertEqual(mailbox.observed, 0, "consuming both writes releases the watermark")
            seen.removeAll()
        }
        let first = mailbox.record(values(10), observing: true)
        let second = mailbox.record(values(20), observing: true)
        let another = mailbox.record(values(25, view: 2), observing: true)
        apply(first); apply(second)
        XCTAssertEqual(seen, [10, 20], "observed scrolls keep every queued delivery")
        let queued = mailbox.record(values(30), observing: true)
        mailbox.record(values(40))
        for note in mailbox.take() { apply(note) }
        apply(queued); apply(another)
        XCTAssertEqual(seen, [10, 20, 40, 25], "order is per scroller")
        XCTAssertEqual(mailbox.observed, 0)
        mailbox.record(values(50))
        let retired = mailbox.record(values(60), observing: true)
        door.replace()
        for note in mailbox.take() { XCTAssertFalse(mailbox.claim(note)) }
        apply(retired)
        XCTAssertEqual(seen, [10, 20, 40, 25])
        mailbox.record(values(70))
        mailbox.record(ScrollMailbox.Values(lease: door.lease(), runtime: runtime.rt, view: 1,
            position: .collection(revision: 7, sequence: 8, offset: 70)))
        XCTAssertEqual(mailbox.pending, 1, "one mailbox slot holds both facts for a scroller")
        let pending = mailbox.take()
        XCTAssertEqual(pending.count, 2)
        for note in pending { apply(note) }
        XCTAssertEqual(seen.last, 70)
        XCTAssertEqual(mailbox.pending, 0)
        XCTAssertEqual(mailbox.observed, 0)
    }

    func testEveryPostedPathRefusesARetiredGenerationBeforeEnteringRust() {
        refuses { $0.completions.replace() }
    }

    func testScrollOrderingRetainsOnlyOutstandingWrites() {
        let runtime = Runtime()
        defer { runtime.destroy() }
        var mailbox = ScrollMailbox()
        let door = runtime.completions
        func values(_ view: UInt32) -> ScrollMailbox.Values {
            ScrollMailbox.Values(lease: door.lease(), runtime: runtime.rt, view: view,
                position: .offset(page: false, left: 0, top: Double(view)))
        }
        for view in UInt32(1)...1000 {
            let older = mailbox.record(values(view), observing: true)
            mailbox.record(values(view))
            let latest = mailbox.take()
            XCTAssertEqual(latest.count, 1)
            XCTAssertTrue(mailbox.claim(latest[0]))
            XCTAssertEqual(mailbox.observed, 1, "the older observation still needs its watermark")
            XCTAssertFalse(mailbox.claim(older))
            XCTAssertEqual(mailbox.observed, 0, "drained scrollers retain no history")
        }
        mailbox.record(values(1))
        let inline = mailbox.record(values(1), observing: true)
        XCTAssertTrue(mailbox.claim(inline))
        mailbox.record(values(1))
        XCTAssertEqual(mailbox.observed, 1, "replacing a buffered offset releases its pending write")
        for note in mailbox.take() { XCTAssertTrue(mailbox.claim(note)) }
        XCTAssertEqual(mailbox.observed, 0)
        let retired = mailbox.record(values(1), observing: true)
        door.replace()
        let current = mailbox.record(values(1), observing: true)
        XCTAssertFalse(mailbox.claim(retired))
        XCTAssertEqual(mailbox.observed, 1, "an old generation cannot release a new write")
        XCTAssertTrue(mailbox.claim(current))
        XCTAssertFalse(mailbox.claim(current), "a consumed observation cannot run twice")
        XCTAssertEqual(mailbox.observed, 0)
        mailbox.record(values(1))
        door.close()
        for note in mailbox.take() { XCTAssertFalse(mailbox.claim(note)) }
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
        XCTAssertTrue(p.listHost.covers(node.id))
        waitForMainQueue("the ordered publications drain together")
        XCTAssertEqual(landed, Array(0..<8))
        XCTAssertEqual(session.publishedDrainCountForTests - drains, 1, "all ready publications share one scheduled drain")
        XCTAssertFalse(session.hasPublished())
        XCTAssertNotNil(p.views[1_001_999])
        XCTAssertTrue(p.listHost.covers(node.id))
        XCTContext.runActivity(named: "T large publication ops=4000 scroll_callback_ms=\(duration * 1000)") { _ in }
    }

    func testScrollHandlerResultsApplyBeforeReturningWithEarlierPublicationsInOrder() throws {
        let (session, window, node, scroll) = try grid()
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter
        let target = try XCTUnwrap(p.listHost.entries[node.id]?.snapshot.rows.first?.root)
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
        let entry = try XCTUnwrap(p.listHost.entries[node.id])
        let pin = try XCTUnwrap(entry.snapshot.rows.first?.root)
        XCTAssertNotNil(entry.lastFacts)
        XCTAssertEqual(p.listHost.owningCollection(pin), node.id)
        var reports = 0
        let feedback = p.listHost.onFeedback
        p.listHost.onFeedback = { reports += 1; feedback?($0) }
        session.holdFillForTests()
        p.listHost.pointer(pin)
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
