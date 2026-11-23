import XCTest
#if os(macOS)
import AppKit
#endif
@testable import ExactKit
@testable import ExactNativeViews

/// LLP 1097 D10: a quit is held while the module's storage lands, and let
/// go when it has or when the bound passes.
final class QuitHoldTests: XCTestCase {
    final class Probe {
        var pending = true, ended = 0
        func hold(bound: TimeInterval? = nil) -> QuitHold {
            QuitHold(every: 0.01, bound: bound, pending: { self.pending }, end: { self.ended += 1 })
        }
    }

    func testNothingLandingTakesNoHold() {
        let probe = Probe(), hold = probe.hold()
        probe.pending = false
        XCTAssertFalse(hold.hold())
        XCTAssertFalse(hold.holding)
        XCTAssertEqual(probe.ended, 0)
    }

    func testTheHoldEndsWhenTheStorageLands() {
        let probe = Probe(), hold = probe.hold()
        XCTAssertTrue(hold.hold())
        XCTAssertFalse(hold.hold(), "one hold at a time")
        hold.check()
        XCTAssertEqual(probe.ended, 0, "still landing")
        probe.pending = false
        let ended = expectation(description: "ended by the timer")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { if probe.ended == 1 { ended.fulfill() } }
        wait(for: [ended], timeout: 2)
        XCTAssertFalse(hold.holding)
    }

    func testTheBoundEndsAHoldThatNeverLands() {
        let probe = Probe(), hold = probe.hold(bound: 0.05)
        XCTAssertTrue(hold.hold())
        let ended = expectation(description: "ended at the bound")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { if probe.ended == 1 { ended.fulfill() } }
        wait(for: [ended], timeout: 2)
        XCTAssertFalse(hold.holding)
    }

    func testTheHoldEndsOnce() {
        let probe = Probe(), hold = probe.hold()
        XCTAssertTrue(hold.hold())
        hold.release()
        hold.release()
        probe.pending = false
        hold.check()
        XCTAssertEqual(probe.ended, 1)
        XCTAssertFalse(hold.holding)
    }

    #if os(macOS)
    private func frozenSession(_ label: String, _ source: String) throws -> ExactSession {
        _ = NSApplication.shared
        let plan = try TableFixture.plan(source)
        let session = ExactApp.shared.makeSession(label: label)
        session.clock = 0
        XCTAssertNil(session.boot(plan: plan, size: CGSize(width: 400, height: 500)).error)
        return session
    }

    private func answer(_ session: ExactSession, path: String, method: String, n: Int) {
        let fixtures = session.agent("""
        {"op":"prefer","faults":{"answer":"https://desk.test/\(path)","method":"\(method)","response":{"status":200,"headers":{},"body":"{\\"n\\":\(n)}"}}}
        """)
        XCTAssertFalse(fixtures.contains("\"error\""), fixtures)
    }

    private func press(_ session: ExactSession, _ testID: String) throws {
        let button = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testID })
        session.apply(session.runtime.press(button.id, now: 0))
    }

    private func derive(_ session: ExactSession, _ name: String) -> Int? {
        let state = try? JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any]
        return (state?["derives"] as? [String: Any])?[name] as? Int
    }

    private func saveSession(_ label: String, count: Int) throws -> ExactSession {
        let sends = (1...count).map { "    send saved = save(\($0))" }.joined(separator: "\n")
        let session = try frozenSession(label, """
        shape Ack
          n: number
        source save(n: number) as shape Ack
          url "https://desk.test/save/{n}"
          method "POST"
          body json
        component App
          state marks = 0
          mutation saved as shape Ack queue then mark
          derive answer = match saved { case some(a) => a.n, case none => 0 }
          derive marked = marks
          action mark
            marks = marks + 1
          action saveAll
        \(sends)
          view
            button press=saveAll testId="save"
              text `${answer}`

        """)
        for n in 1...count { answer(session, path: "save/\(n)", method: "POST", n: n) }
        return session
    }

    func testClosingOnAFrozenClockDrainsSendsArmedByLaterReplies() throws {
        let session = try saveSession("close-queue", count: 3)
        defer { session.destroy() }
        try press(session, "save")
        func answered(_ n: Int) -> Bool { derive(session, "answer") == n }
        waitFor("the first save's reply") { answered(1) }
        guard answered(1) else { return }
        func startedNext(waiting: Int) -> Bool {
            session.agent(#"{"op":"logs"}"#).contains("saved next (\(waiting) waiting)")
        }
        XCTAssertFalse(startedNext(waiting: 1))

        var ended = false
        let hold = QuitHold(every: 60, pending: { session.finishOnClose(queuedOnly: true) }) { ended = true }
        defer { hold.release() }
        XCTAssertTrue(hold.hold())
        guard startedNext(waiting: 1) else { return XCTFail("the second save was due on close") }
        waitFor("the second save's reply") { answered(2) }
        guard answered(2) else { return }
        XCTAssertFalse(startedNext(waiting: 0))
        hold.check()
        guard startedNext(waiting: 0) else { return XCTFail("the second reply made the third save due") }
        waitFor("the third save's reply") { answered(3) }
        hold.check()
        XCTAssertTrue(ended)
        XCTAssertEqual(derive(session, "marked"), 2, "only the two turns with waiting sends need their then")
        XCTAssertEqual(session.clock, 0)
    }

    func testAgentQuitDrainsTwoSavesOnAFrozenClock() throws {
        let session = try saveSession("agent-quit-queue", count: 2)
        let routes = Agent.routes
        let exiting = Agent.exiting
        var ended = false
        defer {
            Agent.exiting?.release()
            Agent.exiting = exiting
            Agent.routes = routes
            session.destroy()
        }
        Agent.routes = [("quit", session)]
        try press(session, "save")
        XCTAssertEqual(derive(session, "answer"), 0, "the first save is still in flight")
        Agent.handle(#"{"op":"quit"}"#, quit: { ended = true })
        XCTAssertTrue(Agent.exiting?.holding == true)
        waitFor("the first save's reply", giveUp: { ended }) { (derive(session, "answer") ?? 0) >= 1 }
        Agent.exiting?.check()
        XCTAssertTrue(session.agent(#"{"op":"logs"}"#).contains("saved next (0 waiting)"))
        waitFor("the quit hold to end") { ended }
        XCTAssertEqual(derive(session, "answer"), 2)
        XCTAssertEqual(session.clock, 0)
    }

    func testAgentDisconnectDrainsTheClosedWindowAfterItsRouteIsRemoved() throws {
        let session = try saveSession("closed-queue", count: 2)
        let idle = try frozenSession("idle-window", "component App\n  view\n    text \"Idle\"\n")
        func window(_ session: ExactSession) -> NSWindow {
            let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 500),
                                  styleMask: [.titled, .closable], backing: .buffered, defer: false)
            window.isReleasedWhenClosed = false
            window.contentView = session.presenter.viewport
            return window
        }
        let closing = window(session), remaining = window(idle)
        let routes = Agent.routes, exiting = Agent.exiting
        let hold = QuitHold(every: 60, pending: { session.finishOnClose(queuedOnly: true) }, end: {})
        let observer = NotificationCenter.default.addObserver(forName: NSWindow.willCloseNotification, object: closing, queue: nil) { _ in
            Agent.route("closing", nil)
        }
        defer {
            NotificationCenter.default.removeObserver(observer)
            Agent.exiting?.release()
            Agent.exiting = exiting
            Agent.routes = routes
            hold.release()
            remaining.close()
            idle.destroy()
            session.destroy()
        }
        Agent.routes = [("closing", session), ("idle", idle)]
        try press(session, "save")
        XCTAssertEqual(derive(session, "answer"), 0, "the first save is in flight")
        XCTAssertTrue(hold.hold())
        closing.close()
        XCTAssertEqual(Agent.routes.map(\.0), ["idle"])

        let connection = Pipe()
        var disconnected = false
        Thread {
            Agent.serve(fd: connection.fileHandleForReading.fileDescriptor, quit: { disconnected = true })
        }.start()
        try connection.fileHandleForWriting.close()
        waitFor("the driver disconnect to drain the closed window", cap: 5) { disconnected }
        XCTAssertEqual(derive(session, "answer"), 2)
        XCTAssertEqual(session.clock, 0)
    }

    func testQueueLandingWithNothingDueReturnsAnEmptyBatch() throws {
        let session = try frozenSession("empty-queue", "component App\n  view\n    text \"Idle\"\n")
        defer { session.destroy() }
        let batch = session.runtime.landThenFirst(queuesOnly: true)
        XCTAssertFalse(batch.busy)
        XCTAssertNil(batch.error)
        XCTAssertNil(batch.clock)
        XCTAssertTrue(batch.ops.isEmpty)
    }

    func testClosingWithAQueuedGetLeavesAnUnrelatedThenDue() throws {
        let pages = try FramePages()
        let session = try frozenSession("close-queue-only", """
        shape Ack
          n: number
        source read() as shape Ack
          url "http://127.0.0.1:\(pages.port)/slow"
        source save(n: number) as shape Ack
          url "https://desk.test/save/{n}"
          method "POST"
          body json
        component App
          mutation queuedRead as shape Ack queue
          mutation unrelated as shape Ack then followUp
          mutation sent as shape Ack
          derive ready = match unrelated { case some(a) => a.n, case none => 0 }
          derive readAnswer = match queuedRead { case some(a) => a.n, case none => 0 }
          action start
            send unrelated = save(1)
            send queuedRead = read()
          action followUp
            send sent = save(2)
          view
            button press=start testId="start"
              text `${ready}`

        """)
        defer { session.destroy() }
        answer(session, path: "save/1", method: "POST", n: 1)
        answer(session, path: "save/2", method: "POST", n: 2)
        try press(session, "start")
        waitFor("the unrelated reply and the GET held at the server", cap: 5) {
            derive(session, "ready") == 1 && pages.referers["/slow"] != nil
        }
        XCTAssertEqual(derive(session, "readAnswer"), 0, "the GET reply is still in flight")
        func sentFollowUp() -> Bool {
            session.agent(#"{"op":"logs"}"#).contains("(sent): POST https://desk.test/save/2")
        }
        XCTAssertFalse(sentFollowUp())
        XCTAssertTrue(session.finishOnClose(queuedOnly: true), "the queued GET still holds the session")
        XCTAssertFalse(sentFollowUp(), "close leaves the unrelated then due")
        session.applyUnlessEmpty(session.runtime.landThen())
        XCTAssertTrue(sentFollowUp(), "a regular landing runs the unrelated then")
        XCTAssertEqual(session.clock, 0)
    }
    #endif
}

/// LLP 1069.010 Q4: a native module's quit work joins the quit's one hold.
/// The module is a table in memory whose `will_quit` keeps `done` for the
/// test to call.
final class ModuleQuitHoldTests: XCTestCase {
    typealias Done = @convention(c) (UnsafeMutableRawPointer?) -> Void
    nonisolated(unsafe) static var asked = 0
    nonisolated(unsafe) static var done: (() -> Void)?

    static func table(size: UInt32) -> [UInt8] {
        var t = [UInt8](repeating: 0, count: 216)
        func put<T>(_ value: T, _ offset: Int) { withUnsafeBytes(of: value) { for (i, b) in $0.enumerated() { t[offset + i] = b } } }
        let none: @convention(c) (UnsafeMutableRawPointer?) -> Void = { _ in }
        let willQuit: @convention(c) (UnsafeMutableRawPointer?, UnsafeMutableRawPointer?, Done?) -> Void = { _, ctx, fn in
            ModuleQuitHoldTests.asked += 1
            ModuleQuitHoldTests.done = { fn?(ctx) }
        }
        put(NativeViews.tableMajor, 0); put(size, 4); put(UnsafeRawPointer(strdup("{}")), 8)
        for offset in [16, 24, 32, 48, 72, 80, 88, 96] { put(unsafeBitCast(none, to: UnsafeRawPointer.self), offset) }
        put(unsafeBitCast(willQuit, to: UnsafeRawPointer.self), 208)
        return t
    }

    func natives(size: UInt32 = 216) -> NativeViews {
        Self.asked = 0
        Self.done = nil
        Self.table(size: size).withUnsafeBytes { NativeViews.install(table: $0.baseAddress!) }
        let natives = NativeViews()
        natives.install(module: UnsafeMutableRawPointer(bitPattern: 1)!)
        return natives
    }

    override func tearDown() { NativeViews.uninstallTable() }

    /// Asked once however often the quit is; held, with the main thread
    /// running, until `done` arrives from another thread.
    func testTheHoldWaitsForTheModuleOnceAndKeepsTheMainThreadRunning() {
        let natives = natives()
        natives.willQuit()
        natives.willQuit()
        XCTAssertEqual(Self.asked, 1, "a repeated quit asks the module once")
        XCTAssertTrue(natives.quitWorkPending)
        var ended = 0
        let hold = QuitHold(every: 0.01, bound: 5, pending: { natives.quitWorkPending }, end: { ended += 1 })
        XCTAssertTrue(hold.hold())
        let finished = expectation(description: "the hold ends after done")
        var turns = 0
        let ticker = Timer(timeInterval: 0.01, repeats: true) { _ in
            turns += 1
            if turns == 5 { DispatchQueue.global().async { Self.done?() } }
            if ended == 1 { finished.fulfill() }
        }
        RunLoop.main.add(ticker, forMode: .common)
        wait(for: [finished], timeout: 2)
        ticker.invalidate()
        XCTAssertGreaterThanOrEqual(turns, 5, "the main thread ran while the quit was held")
        XCTAssertFalse(natives.quitWorkPending)
        natives.willQuit()
        XCTAssertEqual(Self.asked, 1)
        XCTAssertEqual(ended, 1)
    }

    /// Work that never finishes is cut at the bound; the session ending
    /// stops the wait too.
    func testUnfinishedWorkIsCutAtTheBoundAndEndsWithTheSession() {
        let natives = natives()
        natives.willQuit()
        var ended = 0
        let hold = QuitHold(every: 0.01, bound: 0.05, pending: { natives.quitWorkPending }, end: { ended += 1 })
        XCTAssertTrue(hold.hold())
        let cut = expectation(description: "cut at the bound")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { if ended == 1 { cut.fulfill() } }
        wait(for: [cut], timeout: 2)
        XCTAssertTrue(natives.quitWorkPending, "the work is still running; the hold let go anyway")
        natives.destroyModule()
        XCTAssertFalse(natives.quitWorkPending)
        Self.done?() // A late done after the module has gone is harmless.
    }

    /// A module made after the quit was decided (its first pixel came during
    /// the hold) is asked when it is made, and once.
    func testAModuleMadeDuringTheHoldIsAskedWhenMade() {
        Self.asked = 0
        Self.done = nil
        Self.table(size: 216).withUnsafeBytes { NativeViews.install(table: $0.baseAddress!) }
        let natives = NativeViews()
        natives.willQuit()
        XCTAssertEqual(Self.asked, 0, "no module yet")
        XCTAssertFalse(natives.quitWorkPending)
        natives.install(module: UnsafeMutableRawPointer(bitPattern: 1)!)
        XCTAssertEqual(Self.asked, 1)
        XCTAssertTrue(natives.quitWorkPending)
        natives.willQuit()
        XCTAssertEqual(Self.asked, 1)
        Self.done?()
        XCTAssertFalse(natives.quitWorkPending)
    }

    /// A module built before the entry existed is not asked and holds nothing.
    func testAModuleWithoutTheEntryHoldsNothing() {
        let natives = natives(size: 208)
        natives.willQuit()
        XCTAssertEqual(Self.asked, 0)
        XCTAssertFalse(natives.quitWorkPending)
        XCTAssertFalse(QuitHold(pending: { natives.quitWorkPending }, end: {}).hold())
    }
}
