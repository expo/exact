import Foundation
import XCTest
import ExactPresence
@testable import ExactKit
@testable import ExactPickers
@testable import ExactWebViews
@testable import ExactToolDoor

// It compiles its plans with the Contract compiler through `Process`, which iOS lacks; the
// generation rules it tests are shared code, run on macOS.
#if os(macOS)
/// A platform answer reaches only the session generation that asked
/// (`SessionLease`): a reload keeps the session but starts a runner that
/// numbers its calls and nodes from the start again.
final class SessionLeaseTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactPresence.install() }
    private var sessions: [ExactSession] = []
    override func tearDown() { sessions.forEach { $0.destroy() }; sessions = []; super.tearDown() }

    private func compile(_ source: String) throws -> Data {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"], "build.mjs --test builds the Contract compiler and names it"))
        compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
        compiler.standardError = FileHandle.nullDevice
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: dir.appendingPathComponent("app.plan"))
    }

    private static let plain = """
    component Probe
      view
        column testId="page"
          text "page"

    """

    /// A session running `source`, with no view; `before` runs before it boots.
    private func session(_ source: String = plain, before: (ExactSession) -> Void = { _ in }) throws -> ExactSession {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let session = ExactApp.shared.makeSession(label: "stamp-\(UUID().uuidString.prefix(6))")
        sessions.append(session)
        before(session)
        session.presenter.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 400) // what a reload lays out in
        XCTAssertNil(session.boot(plan: try compile(source), size: CGSize(width: 300, height: 400)).error)
        return session
    }

    private func logs(_ session: ExactSession) -> String { session.agent(#"{"op":"logs","since":0}"#) }

    func testMissingCapabilitiesAreReportedOncePerSession() throws {
        let markdown = MarkdownLink.installed, reorders = ReorderLink.make
        MarkdownLink.installed = nil; ReorderLink.make = nil
        defer { MarkdownLink.installed = markdown; ReorderLink.make = reorders }
        let session = try session()
        let batch = wireBatch([
            ["op": "create", "id": 900, "kind": "text", "props": ["markup": "markdown", "text": "**bold**"]],
            ["op": "create", "id": 901, "kind": "view", "props": ["reorderFor": "rows"]],
        ])
        session.apply(batch)
        session.apply(wireBatch([["op": "props", "id": 900, "set": ["markup": "markdown"]]]))
        let journal = logs(session)
        for product in ["ExactMarkdown", "ExactCollections"] {
            XCTAssertEqual(journal.components(separatedBy: "link \(product) and call \(product).install()").count - 1, 1, journal)
        }
    }

    /// Drain what the main queue holds now.
    private func drain() { RunLoop.main.run(until: Date().addingTimeInterval(0.05)) }

    func testALeaseOutlivesNeitherItsGenerationNorItsSession() throws {
        let session = try session()
        let stamp = SessionLease(session)
        XCTAssertTrue(stamp.session === session)
        XCTAssertTrue(session.apply(try compile(Self.plain)), "reloaded")
        XCTAssertNil(stamp.session, "a reload ends the generation")
        let next = SessionLease(session)
        XCTAssertTrue(next.session === session)
        session.destroy()
        XCTAssertNil(next.session, "so does the session's end")
    }

    // MARK: presence

    /// The check's ending, here synchronous: this test host's module serves no
    /// authenticator, so the check fails at once and journals why.
    private let presence: [String: Any] = ["call": 0, "name": "token", "policy": "user-presence"]
    private let presenceFailed = "presence token: the platform's check failed"

    func testAPresenceAnswerReachesTheGenerationThatAsked() throws {
        let session = try session()
        session.presenceOp(presence, issued: SessionLease(session))
        drain()
        XCTAssertTrue(logs(session).contains(presenceFailed), logs(session))
    }

    func testAPresenceAnswerAfterAReloadReachesNoCall() throws {
        let session = try session()
        let asked = SessionLease(session)
        XCTAssertTrue(session.apply(try compile(Self.plain)))
        session.presenceOp(presence, issued: asked)
        drain()
        XCTAssertFalse(logs(session).contains(presenceFailed), "the old check answers the new runner's call 0")
    }

    func testAPresenceOpQueuedWhenItsGenerationEndsIsNotStarted() throws {
        let session = try session()
        session.apply(wireBatch([["op": "presence"].merging(presence) { a, _ in a }]))
        drain()
        XCTAssertTrue(logs(session).contains(presenceFailed), "the batch's check started: \(logs(session))")
        session.apply(wireBatch([["op": "presence", "call": 1, "name": "key", "policy": "user-presence"]]))
        // A reload pumps the main queue while it prepares, so the generation ends here with nothing run in between.
        session.commitForTests()
        drain()
        XCTAssertFalse(logs(session).contains("presence key:"), "the old batch's check was started")
    }

    // MARK: file picker

    private static let picking = """
    component App
      state picked = "none"
      state cancels = 0
      action onPick(files: Picked[])
        picked = match first(files) { case some(f) => f.name, case none => "empty" }
      action onCancel
        cancels = cancels + 1
      view
        column
          input type="file" id="attach" change=onPick cancel=onCancel

    """

    private func slots(_ session: ExactSession) throws -> [String: Any] {
        let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any])
        return try XCTUnwrap(state["slots"] as? [String: Any])
    }

    /// A picker shown before a reload answers after it, when the same page
    /// has put its input at the old one's node id.
    func testAPickerShownBeforeAReloadAnswersNoInputAfterIt() throws {
        let session = try session(Self.picking)
        let old = try XCTUnwrap(session.pickerHost.request("attach"))
        XCTAssertNil(session.pickerHost.request("attach"), "open: a second showPicker does nothing")
        XCTAssertTrue(session.apply(try compile(Self.picking)))
        let new = try XCTUnwrap(session.pickerHost.request("attach"), "the replacement's input opens its own picker at once")
        XCTAssertEqual(new.view, old.view, "the replacement numbers its nodes afresh")
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID().uuidString).txt")
        try Data("old".utf8).write(to: file)
        defer { try? FileManager.default.removeItem(at: file) }
        session.pickerHost.finish(old, files: [(file, "old.txt")])
        session.pickerHost.finish(old, files: [])
        XCTAssertEqual(try slots(session)["picked"] as? String, "none", "the old pick changed the new input")
        XCTAssertEqual(try slots(session)["cancels"] as? Int, 0, "the old cancel reached the new input")
        XCTAssertNil(session.pickerHost.request("attach"), "the new input's picker is still open")
        session.pickerHost.finish(new, files: [])
        XCTAssertEqual(try slots(session)["cancels"] as? Int, 1, "the current generation's cancel")
    }

    func testAPickerCannotAnswerAnotherSessionsInput() throws {
        let a = try session(Self.picking), b = try session(Self.picking)
        let request = try XCTUnwrap(a.pickerHost.request("attach"))
        b.pickerHost.finish(request, files: [])
        XCTAssertEqual(try slots(b)["cancels"] as? Int, 0)
        a.pickerHost.finish(request, files: [])
        XCTAssertEqual(try slots(a)["cancels"] as? Int, 1)
    }

    // MARK: tool calls

    /// Wait on the main run loop until `done`, two seconds at most.
    private func until(_ done: () -> Bool) {
        let end = Date().addingTimeInterval(2)
        while !done() && Date() < end { RunLoop.main.run(until: Date().addingTimeInterval(0.01)) }
    }

    /// A tool call still running when the app reloads is polled after it.
    /// The replacement's runner numbers its calls afresh, so its call 1 is
    /// another invocation (here a stand-in's answer), whose outcome must
    /// not answer this one. A fresh boot over the session and a reload
    /// that carries its state both end the runner.
    func testAToolCallPolledAfterTheRunnerIsReplacedEndsInterrupted() throws {
        for fresh in [false, true] {
            let session = try session()
            let runner = ExactTools.runner(session)
            var replaced = false
            let agent = { (text: String) -> String? in
                guard runner(text) != nil else { return nil }
                return replaced ? #"{"outcome":"ok","certainty":"applied","reason":"","value":"another call's"}"# : #"{"call":1,"state":"running"}"#
            }
            var answer: [String: Any]?
            ExactTools.follow(1, agent: agent, cancellation: ExactTools.Cancellation()) { answer = $0 }
            XCTAssertNil(answer, "still running")
            if fresh { XCTAssertNil(session.boot(plan: try compile(Self.plain), size: CGSize(width: 300, height: 400)).error) }
            else { XCTAssertTrue(session.apply(try compile(Self.plain))) }
            replaced = true
            until { answer != nil }
            XCTAssertEqual(answer?["outcome"] as? String, "interrupted", "fresh \(fresh): \(answer ?? [:])")
            XCTAssertEqual(answer?["certainty"] as? String, "uncertain")
            XCTAssertNil(answer?["value"])
        }
    }

    // MARK: openURL

    private static let opening = """
    component Probe
      state heard = "none"
      action hear(v: string)
        heard = v
      view
        column
          input id="result" change=hear

    """

    func testAnOpenAnswersItsGenerationOpenedOrFailed() throws {
        let session = try session(Self.opening)
        session.openURLAnswer(["https://example.com/", "result"])(true)
        XCTAssertEqual(try slots(session)["heard"] as? String, "opened")
        session.openURLAnswer(["https://example.com/", "result"])(false)
        XCTAssertEqual(try slots(session)["heard"] as? String, "failed")
    }

    /// UIKit's completion arrives after a reload whose page has its own
    /// `result`, which never asked; or after the session's end.
    func testAnOpenAnsweredAfterAReloadOrTheSessionsEndChangesNothing() throws {
        let session = try session(Self.opening)
        let answer = session.openURLAnswer(["app-settings:", "result"])
        XCTAssertTrue(session.apply(try compile(Self.opening)))
        answer(true)
        XCTAssertEqual(try slots(session)["heard"] as? String, "none", "the replacement's handler ran")
        let current = session.openURLAnswer(["app-settings:", "result"])
        session.destroy()
        current(false)
    }

    // MARK: iframe events

    private static let framing = """
    component App
      state landed = 0
      action land(e: FrameNavigation)
        landed = landed + 1
      view
        column
          iframe id="guest" src="https://example.invalid/" navigatesuccess=land width=300 height=180 testId="guest"

    """

    /// A frame whose arm never loads: its entry still takes the arm's events.
    private func framed() throws -> (ExactSession, NodeView, Data) {
        let session = try session(Self.framing) { $0.webHost.moduleLocation = { "/nonexistent/libexact_web.dylib" } }
        let frame = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "guest" })
        let record: [String: Any] = ["event": "navigatesuccess", "record": ["url": "https://old.example/", "navigationId": 7]]
        return (session, frame, try JSONSerialization.data(withJSONObject: record))
    }

    func testAFramesQueuedEventReachesItsHandlerInItsGeneration() throws {
        let (session, frame, event) = try framed()
        session.webHost.receivedEvent(id: frame.id, kind: 4, data: event)
        drain()
        XCTAssertEqual(try slots(session)["landed"] as? Int, 1)
    }

    /// The arm's event is queued to a later turn; before it, the generation
    /// ends, or the node's frame is replaced by another at its id, as a
    /// reload replaces the tree.
    func testAFramesQueuedEventIsDroppedWhenItsGenerationOrEntryEnds() throws {
        for replaced in [false, true] {
            let (session, frame, event) = try framed()
            session.webHost.receivedEvent(id: frame.id, kind: 4, data: event)
            if replaced {
                session.webviews.destroy(id: frame.id)
                _ = session.webviews.create(owner: frame)
            } else {
                session.commitForTests()
            }
            drain()
            XCTAssertEqual(try slots(session)["landed"] as? Int, 0, replaced ? "a replaced frame's event" : "an ended generation's event")
        }
    }

    func testAToolCallPolledInItsGenerationReadsItsRunner() throws {
        let session = try session()
        let reply = try XCTUnwrap(ExactTools.runner(session)(#"{"op":"called","id":1}"#))
        XCTAssertTrue(reply.contains("error"), "this runner has no call 1: \(reply)")
    }

    // MARK: document & save pickers

    private static let documents = """
    component App
      state doc = "none"
      state docCancels = 0
      state saved = "none"
      state saveCancels = 0
      action onDoc(v: string)
        doc = v
      action onDocCancel
        docCancels = docCancels + 1
      action onSave(v: string)
        saved = v
      action onSaveCancel
        saveCancels = saveCancels + 1
      view
        column
          input id="doc" change=onDoc cancel=onDocCancel
          input id="save" change=onSave cancel=onSaveCancel

    """

    func testDocumentAndSavePickersDropStaleCompletionsAfterReloadAndDestroy() throws {
        let session = try session(Self.documents)
        let docView = try XCTUnwrap(session.presenter.views.values.first { $0.props["id"] == "doc" }?.id)
        let saveView = try XCTUnwrap(session.presenter.views.values.first { $0.props["id"] == "save" }?.id)
        let oldLease = SessionLease(session)

        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lease-picker-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let src = dir.appendingPathComponent("src.txt"), dst = dir.appendingPathComponent("dst.txt")
        try Data("from-old-gen".utf8).write(to: src)
        let fh = try FileHandle(forReadingFrom: src)
        defer { try? fh.close() }

        // Reload replaces the runner while keeping the same view IDs.
        XCTAssertTrue(session.apply(try compile(Self.documents)))
        var cleaned = 0
        session.pickerHost.finishDocument(docView, name: "showOpenFilePicker", urls: [src], issued: oldLease)
        session.pickerHost.finishDocument(docView, name: "showOpenFilePicker", urls: nil, issued: oldLease)
        session.pickerHost.finishSave(view: saveView, file: fh, to: dst, issued: oldLease) { cleaned += 1 }
        session.pickerHost.finishSave(view: saveView, file: fh, to: nil, issued: oldLease) { cleaned += 1 }

        XCTAssertEqual(cleaned, 2, "cleanup closures still run for stale save panels")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dst.path), "stale save must not copy file to disk")
        let s = try slots(session)
        XCTAssertEqual(s["doc"] as? String, "none")
        XCTAssertEqual(s["docCancels"] as? Int, 0)
        XCTAssertEqual(s["saved"] as? String, "none")
        XCTAssertEqual(s["saveCancels"] as? Int, 0)

        // Current generation still works; after destroy, completions are dropped with cleanup intact.
        let currentLease = SessionLease(session)
        session.pickerHost.finishSave(view: saveView, file: fh, to: dst, issued: currentLease) { cleaned += 1 }
        XCTAssertEqual(cleaned, 3)
        XCTAssertEqual(try Data(contentsOf: dst), Data("from-old-gen".utf8))
        XCTAssertEqual(try slots(session)["saved"] as? String, "dst.txt")

        try FileManager.default.removeItem(at: dst)
        session.destroy()
        session.pickerHost.finishDocument(docView, name: "showOpenFilePicker", urls: [src], issued: currentLease)
        session.pickerHost.finishSave(view: saveView, file: fh, to: dst, issued: currentLease) { cleaned += 1 }
        XCTAssertEqual(cleaned, 4)
        XCTAssertFalse(FileManager.default.fileExists(atPath: dst.path))
    }
}
#endif
