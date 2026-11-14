#if os(iOS) || os(macOS)
import XCTest
#if os(iOS)
import AVFoundation
import ObjectiveC
import UIKit
#else
import AppKit
#endif
@testable import ExactKit
@testable import ExactSurfaces
import ExactPackage
@testable import MediaCaptureImplementation

final class PageFreezeTests: XCTestCase {
    private func text(_ session: ExactSession, _ id: String) -> String? {
        session.presenter.views.values.first { $0.props["testId"] == id }?.props["text"]
    }

    func testTheFreezeBitStopsAPlayingSequenceInThePageCommit() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let session = ExactApp.shared.makeSession(label: "page-freeze")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("PAGE_FREEZE"), size: CGSize(width: 390, height: 844)).error)
        session.tellPage()
        let play = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "play" })
        session.presenter.onPress?(play.id)
        XCTAssertEqual(text(session, "transport"), "playing")
        let bits = PageFacts.bits(nil, readsOnLine: false) & ~UInt32(128)
        session.apply(session.runtime.setPage(bits | 128))
        XCTAssertEqual(text(session, "frozen"), "true", "bit 7 reaches the runner through the Apple bridge")
        XCTAssertEqual(text(session, "transport"), "stopped")
        XCTAssertEqual(text(session, "reason"), "background")
        session.apply(session.runtime.setPage(bits))
        XCTAssertEqual(text(session, "frozen"), "false")
        XCTAssertEqual(text(session, "transport"), "stopped", "foreground alone does not restart")
    }

    func testPreferAcceptsAndReadsTheFrozenFact() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let saved = PageFacts.agent
        defer { PageFacts.agent = saved }
        let session = ExactApp.shared.makeSession(label: "prefer-freeze")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("PAGE_FREEZE"), size: CGSize(width: 390, height: 844)).error)
        for value in [true, false] {
            let reply = session.agentInstance.prefer(["page": ["frozen": value]])
            XCTAssertNil(reply["error"], "\(reply)")
            XCTAssertEqual(PageFacts.agent.frozen, value)
            XCTAssertEqual((reply["page"] as? [String: Any])?["frozen"] as? Bool, PageFacts.frozen)
        }
        if !PageFacts.substituted { XCTAssertFalse(PageFacts.frozen) }
    }
}

#if os(iOS)
private var audioMode = false
private weak var freezePlayer: AVPlayer?
private var freezeStatus = AVPlayer.TimeControlStatus.waitingToPlayAtSpecifiedRate
extension AVPlayer {
    @objc fileprivate func freezeTimeControlStatus() -> AVPlayer.TimeControlStatus {
        self === freezePlayer ? freezeStatus : freezeTimeControlStatus()
    }
}
extension Bundle {
    @objc fileprivate func freezeInfo(_ key: String) -> Any? {
        if key == "UIBackgroundModes" { return audioMode ? ["audio"] : [] }
        if key == "ExactCapturesInBackground" { return audioMode }
        return freezeInfo(key)
    }
}

/// A 100 ms sample and an engine that keeps running through rests.
private enum ClickOutput {
    nonisolated(unsafe) static var clock = 0.0
    nonisolated(unsafe) static var starts: [Double] = []
    nonisolated(unsafe) static var voices: [UInt64: Double] = [:]
    nonisolated(unsafe) static var closes = 0
    static let library = SoundLibrary(open: { _, _, _, _, _, _, _ in UnsafeMutableRawPointer(bitPattern: 1) },
        op: { _, kind, id, _, at, _, _, _, _ in
            if kind == 1 { starts.append(at); voices[id] = at }
            if kind == 2 { voices[id] = nil }
            if kind == 3 { voices = [:] }
            return true
        }, counts: { _, out in
            out?[0] = UInt64(voices.values.filter { clock < $0 + 0.1 }.count)
            out?[3] = 1
        }, engine: { _, _, _, _ in true }, close: { _ in closes += 1; voices = [:] })
}

/// UIKit lifecycle and the declared audio mode, with deterministic audio
/// owners, cover the execution-eligibility edges.
final class PageFreezeIOSTests: XCTestCase {
    private var saved = AudioSession.configuration
    private var exchanged: [(Method, Method)] = []
    override func setUpWithError() throws {
        for (type, original, substitute) in [
            (Bundle.self, #selector(Bundle.object(forInfoDictionaryKey:)), #selector(Bundle.freezeInfo(_:))),
        ] {
            let a = try XCTUnwrap(class_getInstanceMethod(type, original)), b = try XCTUnwrap(class_getInstanceMethod(type, substitute))
            method_exchangeImplementations(a, b)
            exchanged.append((a, b))
        }
        freezePlayer = nil
        audioMode = false
        AppBackground.invalidate()
        AudioSession.reset()
        move(false)
    }
    override func tearDown() {
        AudioSession.reset()
        AudioSession.configuration = saved
        for (a, b) in exchanged.reversed() { method_exchangeImplementations(a, b) }
        exchanged = []
        AppBackground.invalidate()
    }
    private func move(_ hidden: Bool) {
        NotificationCenter.default.post(name: hidden ? UIApplication.didEnterBackgroundNotification : UIApplication.willEnterForegroundNotification, object: nil)
    }

    func testAWaitingVideoFreezesUntilItActuallyPlays() throws {
        audioMode = true
        AudioSession.configuration = .init("playback")
        let server = try HeldMediaServer()
        defer { server.stop() }
        let presenter = Presenter()
        defer { presenter.reset() }
        presenter.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "video", "props": [
                "src": server.url, "paused": "false", "muted": "false", "volume": "1", "playsinline": "true",
            ]],
            ["op": "roots", "ids": [1]],
        ]))
        let owner = try XCTUnwrap(presenter.views[1])
        let video = try XCTUnwrap(owner.video)
        func playerLayer(_ view: UIView) -> AVPlayerLayer? {
            (view.layer as? AVPlayerLayer) ?? view.subviews.lazy.compactMap { playerLayer($0) }.first
        }
        let player = try XCTUnwrap(playerLayer(owner)?.player)
        waitFor("the player waiting for its source", facts: { "\(video.state())" }) {
            player.timeControlStatus == .waitingToPlayAtSpecifiedRate && player.rate != 0
        }
        let snapshot = video.state()
        XCTAssertEqual(snapshot["playing"] as? Bool, false)
        XCTAssertEqual(snapshot["paused"] as? Bool, false)
        XCTAssertEqual(snapshot["muted"] as? Bool, false)
        XCTAssertGreaterThan(snapshot["volume"] as? Double ?? 0, 0)
        let session = ExactApp.shared.makeSession(label: "waiting-video-freeze")
        session.clock = 0
        defer { move(false); session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("PAGE_FREEZE"), size: CGSize(width: 390, height: 844)).error)
        session.tellPage()
        let play = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "play" })
        session.presenter.onPress?(play.id)
        session.apply(session.runtime.advance(now: 0))
        func word(_ id: String) -> String? { session.presenter.views.values.first { $0.props["testId"] == id }?.props["text"] }
        XCTAssertEqual(word("transport"), "playing")
        move(true)
        XCTAssertTrue(PageFacts.frozen)
        try assertTransportStopped(session)
        session.apply(session.runtime.advance(now: 0))
        XCTAssertEqual(word("frozen"), "true")
        XCTAssertEqual(word("transport"), "stopped")
        XCTAssertEqual(word("reason"), "background")
        // Supply the status edge without requiring the simulator's audio output device.
        let original = try XCTUnwrap(class_getInstanceMethod(AVPlayer.self, #selector(getter: AVPlayer.timeControlStatus)))
        let substitute = try XCTUnwrap(class_getInstanceMethod(AVPlayer.self, #selector(AVPlayer.freezeTimeControlStatus)))
        freezePlayer = player
        method_exchangeImplementations(original, substitute)
        defer { method_exchangeImplementations(original, substitute); freezePlayer = nil }
        func status(_ value: AVPlayer.TimeControlStatus) {
            player.willChangeValue(forKey: "timeControlStatus")
            freezeStatus = value
            player.didChangeValue(forKey: "timeControlStatus")
        }
        status(.playing)
        XCTAssertEqual(video.state()["playing"] as? Bool, true)
        XCTAssertFalse(PageFacts.frozen)
        session.apply(session.runtime.advance(now: 0))
        XCTAssertEqual(word("frozen"), "false")
        XCTAssertEqual(word("transport"), "stopped", "resuming eligibility does not cycle the sequence's gate")
        status(.waitingToPlayAtSpecifiedRate)
        XCTAssertTrue(PageFacts.frozen, "a stall while already hidden removes eligibility")
        XCTAssertFalse(server.timedOut)
    }

    func testSoundOutputKeepsASequenceEligibleAcrossRestsThenReleasesTheFinalVoice() throws {
        audioMode = true
        AudioSession.configuration = .init("playback")
        ClickOutput.starts = []; ClickOutput.voices = [:]; ClickOutput.closes = 0; ClickOutput.clock = ExactEnv.t0
        let session = ExactApp.shared.makeSession(label: "resting-metronome")
        let output = SoundOutput(log: { _ in }, resolve: { $0 }, library: { ClickOutput.library })
        output.now = { ClickOutput.clock }
        session.sound = output
        session.clock = 0
        defer { move(false); session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("PAGE_FREEZE_SOUND"), size: CGSize(width: 390, height: 844)).error)
        session.tellPage()
        func word(_ id: String) -> String? { session.presenter.views.values.first { $0.props["testId"] == id }?.props["text"] }
        let play = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "play" })
        session.presenter.onPress?(play.id)
        session.apply(session.runtime.advance(now: 0))
        XCTAssertFalse(ClickOutput.starts.isEmpty)
        XCTAssertEqual(word("transport"), "playing")
        // Background between the first 100 ms click and the 500 ms refill.
        ClickOutput.clock = ExactEnv.t0 + 0.2
        move(true)
        AudioSession.playbackChanged()
        XCTAssertFalse(PageFacts.frozen)
        XCTAssertEqual(word("transport"), "playing")
        XCTAssertEqual(ClickOutput.closes, 0)
        session.clock = 600
        session.apply(session.runtime.advance(now: 600))
        XCTAssertGreaterThan(ClickOutput.starts.count, 1, "the next click is scheduled after the rest")
        ClickOutput.clock = ExactEnv.t0 + 1.2
        AudioSession.playbackChanged()
        XCTAssertFalse(PageFacts.frozen)
        XCTAssertFalse(output.stopIfIdle(), "a playing transport keeps the output between hits")
        // Stop without ending the sample already sounding: it keeps eligibility only until its end.
        session.clock = 2000
        session.apply(session.runtime.advance(now: 2000))
        ClickOutput.clock = ExactEnv.t0 + 2.01
        let stop = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "stop-with-sound" })
        session.presenter.onPress?(stop.id)
        session.apply(session.runtime.advance(now: 2000))
        XCTAssertEqual(word("transport"), "stopped")
        XCTAssertFalse(PageFacts.frozen)
        ClickOutput.clock = ExactEnv.t0 + 2.2
        AudioSession.playbackChanged()
        XCTAssertTrue(PageFacts.frozen)
        XCTAssertEqual(ClickOutput.closes, 1)
    }

    func testSpeechLeavesItsEffectivePlaybackCategoryWithAnAmbientSequence() throws {
        audioMode = true
        AudioSession.configuration = .init(nil)
        let speech = AudioSession.claimSpeech(type: "playback") { _ in }
        XCTAssertNotEqual(speech, 0)
        ClickOutput.starts = []; ClickOutput.voices = [:]; ClickOutput.closes = 0; ClickOutput.clock = ExactEnv.t0
        let session = ExactApp.shared.makeSession(label: "speech-over-ambient-sequence")
        let output = SoundOutput(log: { _ in }, resolve: { $0 }, library: { ClickOutput.library })
        output.now = { ClickOutput.clock }
        session.sound = output
        session.clock = 0
        defer { move(false); AudioSession.releaseSpeech(speech); session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("PAGE_FREEZE_SOUND"), size: CGSize(width: 390, height: 844)).error)
        session.tellPage()
        let play = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "play" })
        session.presenter.onPress?(play.id)
        session.apply(session.runtime.advance(now: 0))
        XCTAssertFalse(ClickOutput.starts.isEmpty)
        move(true)
        AudioSession.releaseSpeech(speech)
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playback)
        XCTAssertFalse(PageFacts.frozen, "the running sound owns the effective playback category")
        XCTAssertEqual(session.presenter.views.values.first { $0.props["testId"] == "transport" }?.props["text"], "playing")
        ClickOutput.clock = ExactEnv.t0 + 0.2
        AudioSession.playbackChanged()
        let stop = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "stop" })
        session.presenter.onPress?(stop.id)
        XCTAssertTrue(PageFacts.frozen, "the last running owner ended while hidden")
    }

    func testReplacingAPlayingPlanWithoutSequencesReleasesTheIdleOutput() throws {
        audioMode = true
        AudioSession.configuration = .init("playback")
        ClickOutput.starts = []; ClickOutput.voices = [:]; ClickOutput.closes = 0; ClickOutput.clock = ExactEnv.t0
        let session = ExactApp.shared.makeSession(label: "replace-playing-plan")
        let output = SoundOutput(log: { _ in }, resolve: { $0 }, library: { ClickOutput.library })
        output.now = { ClickOutput.clock }
        session.sound = output
        session.clock = 0
        defer { move(false); session.destroy() }
        let size = CGSize(width: 390, height: 844)
        XCTAssertNil(session.boot(plan: try fixturePlan("PAGE_FREEZE_SOUND"), size: size).error)
        session.tellPage()
        let play = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "play" })
        session.presenter.onPress?(play.id)
        session.apply(session.runtime.advance(now: 0))
        XCTAssertFalse(ClickOutput.starts.isEmpty)
        ClickOutput.clock = ExactEnv.t0 + 0.2
        move(true)
        XCTAssertFalse(PageFacts.frozen)
        XCTAssertFalse(output.stopIfIdle())
        XCTAssertNil(session.boot(plan: try fixturePlan("PAGE_FREEZE_NO_SEQUENCE"), size: size).error)
        XCTAssertTrue(ClickOutput.voices.isEmpty, "replacement clears the old voices while keeping the same sound table")
        XCTAssertTrue(output.stopIfIdle(), "the removed sequence cannot keep the idle engine")
        AudioSession.playbackChanged()
        XCTAssertEqual(ClickOutput.closes, 1)
        XCTAssertTrue(PageFacts.frozen)
        XCTAssertEqual(session.presenter.views.values.first { $0.props["testId"] == "frozen" }?.props["text"], "true")
    }

    func testCanvasAudioReleasesBeforeTheBackgroundPageCommitAndOnDestruction() throws {
        ExactSurfaces.install()
        audioMode = true
        AudioSession.configuration = .init("playback")
        let session = ExactApp.shared.makeSession(label: "canvas-freeze")
        let window = UIWindow(frame: UIScreen.main.bounds)
        window.addSubview(session.presenter.viewport); window.isHidden = false
        defer { move(false); window.isHidden = true; session.destroy() }
        // Register the page observer first: notification registration order cannot grant stale eligibility.
        XCTAssertNil(session.boot(plan: try fixturePlan("PAGE_FREEZE"), size: CGSize(width: 390, height: 844)).error)
        let view = NodeView(id: 100, kind: "canvas", presenter: session.presenter)
        let entry = CanvasesHost.Entry(view: view, name: "fixture", values: [])
        entry.id = 1
        session.surfaceHost.entries[100] = entry
        let lifecycle = session.surfaceHost.lifecycle
        lifecycle.requestAudio()
        XCTAssertTrue(AudioSession.backgroundExecutionEligible())
        move(true)
        XCTAssertTrue(lifecycle.hidden)
        XCTAssertTrue(PageFacts.frozen)
        XCTAssertEqual(session.presenter.views.values.first { $0.props["testId"] == "frozen" }?.props["text"], "true")
        move(false)
        lifecycle.refresh()
        XCTAssertTrue(AudioSession.backgroundExecutionEligible())
        session.surfaceHost.destroy(view: 100)
        XCTAssertFalse(AudioSession.backgroundExecutionEligible(), "the final canvas gives its hold back")
        move(true)
        XCTAssertTrue(PageFacts.frozen)
        move(false)
        lifecycle.refresh()
        session.surfaceHost.entries[100] = entry
        lifecycle.requestAudio()
        XCTAssertTrue(AudioSession.backgroundExecutionEligible())
        session.destroy()
        XCTAssertFalse(AudioSession.backgroundExecutionEligible(), "destroying the session releases its canvas audio")
    }

    func testTheFreezeCommitHoldsABackgroundTaskUntilItReturns() throws {
        let session = ExactApp.shared.makeSession(label: "freeze-close-hold")
        defer { move(false); session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("PAGE_FREEZE"), size: CGSize(width: 390, height: 844)).error)
        session.tellPage()
        var events: [String] = []
        let record = { [weak session] (event: String) in
            let frozen = session?.presenter.views.values.first { $0.props["testId"] == "frozen" }?.props["text"] ?? "missing"
            events.append("\(event):\(frozen)")
        }
        session.freezeCloseHold = FinishHold(begin: { _ in record("begin"); return true }, end: { record("end") })
        move(true)
        XCTAssertEqual(events, ["begin:false", "end:true"])
    }

    func testBackgroundWithoutPlaybackFreezesAndStopsTheSequenceBeforeReturning() throws {
        let session = ExactApp.shared.makeSession(label: "background-freeze")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("PAGE_FREEZE"), size: CGSize(width: 390, height: 844)).error)
        session.tellPage()
        let play = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "play" })
        session.presenter.onPress?(play.id)
        func word(_ id: String) -> String? { session.presenter.views.values.first { $0.props["testId"] == id }?.props["text"] }
        XCTAssertEqual(word("transport"), "playing")
        AudioSession.configuration = .init("playback")
        audioMode = true
        XCTAssertFalse(PageFacts.frozen)
        session.dataPreparing = true
        move(true)
        session.dataPreparing = false
        XCTAssertEqual(session.noticesInFlight, 0, "freeze is applied even while activation prepares")
        XCTAssertTrue(PageFacts.frozen, "a configured category with no playback grants nothing")
        XCTAssertEqual(word("frozen"), "true")
        XCTAssertEqual(word("transport"), "stopped")
        XCTAssertEqual(word("reason"), "background")
        move(false)
        XCTAssertFalse(PageFacts.frozen)
        XCTAssertEqual(word("frozen"), "false")
        XCTAssertEqual(word("transport"), "stopped")
    }

    private func assertTransportStopped(_ session: ExactSession, file: StaticString = #filePath, line: UInt = #line) throws {
        let bytes = try XCTUnwrap(session.agent("{\"op\":\"state\"}").data(using: .utf8), file: file, line: line)
        let state = try XCTUnwrap(JSONSerialization.jsonObject(with: bytes) as? [String: Any], file: file, line: line)
        let transport = try XCTUnwrap(state["transport"] as? [String: Any], file: file, line: line)
        XCTAssertEqual(transport["state"] as? String, "stopped", file: file, line: line)
    }

    func testPlaybackEndingWhileHiddenIsPolledAndFreezesThePlanBeforePublishing() throws {
        audioMode = true
        AudioSession.configuration = .init("playback")
        let session = ExactApp.shared.makeSession(label: "finite-playback")
        session.clock = 0
        defer { move(false); session.destroy() }
        let owner = NSObject(), holder = ObjectIdentifier(owner)
        var running = true, released = false
        try AudioSession.hold(holder, running: { running })
        AudioSession.stopsWhenIdle(holder) {
            guard !running else { return false }
            released = true
            AudioSession.release(holder)
            return true
        }
        defer { AudioSession.forgetIdle(holder); AudioSession.release(holder) }
        XCTAssertNil(session.boot(plan: try fixturePlan("PAGE_FREEZE"), size: CGSize(width: 390, height: 844)).error)
        session.tellPage()
        let play = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "play" })
        session.presenter.onPress?(play.id)
        session.apply(session.runtime.advance(now: 0))
        move(true)
        XCTAssertFalse(PageFacts.frozen)
        XCTAssertEqual(session.presenter.views.values.first { $0.props["testId"] == "transport" }?.props["text"], "playing")
        waitForMainQueue()
        var published = false
        let tokens = PageFacts.observe {
            if PageFacts.frozen { published = true }
        }
        defer { PageFacts.forget(tokens) }
        // No output batch or owner notification follows the one-shot's end.
        running = false
        waitFor("the background eligibility poll", cap: 3) { published }
        XCTAssertTrue(released)
        XCTAssertTrue(PageFacts.frozen)
        try assertTransportStopped(session)
        session.apply(session.runtime.advance(now: 0))
        XCTAssertEqual(session.presenter.views.values.first { $0.props["testId"] == "frozen" }?.props["text"], "true")
        XCTAssertEqual(session.presenter.views.values.first { $0.props["testId"] == "transport" }?.props["text"], "stopped")
        XCTAssertEqual(session.presenter.views.values.first { $0.props["testId"] == "reason" }?.props["text"], "background")
    }

    func testStoppingBetweenHitsPresentsTheStopBeforeTheFreeze() throws {
        audioMode = true
        AudioSession.configuration = .init("playback")
        ClickOutput.starts = []; ClickOutput.voices = [:]; ClickOutput.closes = 0; ClickOutput.clock = ExactEnv.t0
        let session = ExactApp.shared.makeSession(label: "stop-between-hits")
        let output = SoundOutput(log: { _ in }, resolve: { $0 }, library: { ClickOutput.library })
        output.now = { ClickOutput.clock }
        session.sound = output
        session.clock = 0
        defer { move(false); session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("PAGE_FREEZE_SOUND"), size: CGSize(width: 390, height: 844)).error)
        session.tellPage()
        func word(_ id: String) -> String? { session.presenter.views.values.first { $0.props["testId"] == id }?.props["text"] }
        let play = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "play" })
        session.presenter.onPress?(play.id)
        session.apply(session.runtime.advance(now: 0))
        ClickOutput.clock = ExactEnv.t0 + 0.2
        move(true)
        XCTAssertFalse(PageFacts.frozen)
        XCTAssertEqual(word("status"), "false:true")
        let stop = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "stop" })
        session.presenter.onPress?(stop.id)
        XCTAssertTrue(PageFacts.frozen)
        try assertTransportStopped(session)
        session.apply(session.runtime.advance(now: 0))
        XCTAssertEqual(word("transport"), "stopped")
        XCTAssertEqual(word("status"), "true:false")
        XCTAssertEqual(word("freeze-branch"), "Frozen")
        XCTAssertNil(word("unfrozen-stop"))
        XCTAssertEqual(ClickOutput.closes, 1)
    }

    func testCaptureReleaseReactivatesInterruptedPlaybackWithoutAnotherHold() throws {
        audioMode = true
        AudioSession.configuration = .init("playback")
        let owner = NSObject(), id = ObjectIdentifier(owner)
        var playing = true
        try AudioSession.hold(id, running: { playing })
        defer { AudioSession.release(id); move(false) }
        let capture = AudioSession.claimCapture(mode: .default, options: [.defaultToSpeaker], background: true) { _ in }
        XCTAssertNotEqual(capture, 0)
        playing = false
        AudioSession.interrupted()
        AudioSession.releaseCapture(capture)
        playing = true
        AudioSession.playbackChanged()
        move(true)
        XCTAssertFalse(PageFacts.frozen, "the existing player resumes under the restored session")
        playing = false
        AudioSession.playbackChanged()
        XCTAssertTrue(PageFacts.frozen)
    }

    func testAFailedCaptureRestartReleasesEligibilityAndANewLeaseRecovers() throws {
        audioMode = true
        AudioSession.configuration = .init("playback")
        struct Unavailable: Error {}
        var fails = false
        let capture = ProcessCapture(openEngine: { _ in if fails { throw Unavailable() } })
        let session = ExactAudioSession { _, _, background, events in
            let id = AudioSession.claimCapture(mode: .default, options: [.defaultToSpeaker], background: background) { _ in }
            return id == 0 ? nil : { AudioSession.releaseCapture(id) }
        }
        let options = CaptureOptions(constraints: [:], apple: [:])
        try capture.lease(options, session: session, log: { _ in })
        defer { capture.unlease(); move(false) }
        move(true)
        XCTAssertFalse(PageFacts.frozen)
        var publications: [Bool] = []
        let tokens = PageFacts.observe { publications.append(PageFacts.frozen) }
        defer { PageFacts.forget(tokens) }
        fails = true
        capture.reopen()
        XCTAssertTrue(PageFacts.frozen, "a failed engine holds no background claim")
        XCTAssertEqual(publications.last, true)
        fails = false
        move(false)
        try capture.lease(options, session: session, log: { _ in })
        move(true)
        XCTAssertFalse(PageFacts.frozen, "a new start reacquires the session")
        capture.unlease()
        XCTAssertFalse(PageFacts.frozen, "the recovered engine still has its original lease")
    }

    func testAFailedOwnerDoesNotInheritAnotherOwnersActivation() throws {
        audioMode = true
        AudioSession.configuration = .init("playback")
        let good = NSObject(), failed = NSObject()
        try AudioSession.hold(ObjectIdentifier(good))
        struct Busy: Error {}
        AudioSession.nextHoldFailure = Busy()
        XCTAssertThrowsError(try AudioSession.hold(ObjectIdentifier(failed)))
        AudioSession.release(ObjectIdentifier(good))
        move(true)
        XCTAssertTrue(PageFacts.frozen)
        AudioSession.release(ObjectIdentifier(failed))
    }

    func testIdleFailedInterruptedAndReleasedOwnersGrantNothing() throws {
        audioMode = true
        AudioSession.configuration = .init("playback")
        let owner = NSObject(), id = ObjectIdentifier(owner)
        struct Busy: Error {}
        AudioSession.nextHoldFailure = Busy()
        XCTAssertThrowsError(try AudioSession.hold(id))
        move(true)
        XCTAssertTrue(PageFacts.frozen, "a failed activation grants nothing")
        try AudioSession.hold(id, running: { false })
        XCTAssertTrue(PageFacts.frozen, "an idle output grants nothing")
        try AudioSession.hold(id)
        XCTAssertFalse(PageFacts.frozen)
        AudioSession.interrupted()
        XCTAssertTrue(PageFacts.frozen, "system interruption removes eligibility even with a holder")
        AudioSession.release(id)
        let capture = AudioSession.claimCapture(mode: .default, options: [.defaultToSpeaker], background: true) { _ in }
        XCTAssertNotEqual(capture, 0)
        XCTAssertFalse(PageFacts.frozen)
        AudioSession.releaseCapture(capture)
        XCTAssertTrue(PageFacts.frozen)
        let speech = AudioSession.claimSpeech(type: "playback") { _ in }
        XCTAssertNotEqual(speech, 0)
        XCTAssertFalse(PageFacts.frozen)
        AudioSession.releaseSpeech(speech)
        XCTAssertTrue(PageFacts.frozen)
        let call = AudioSession.claimCall(video: false) { _ in }
        XCTAssertTrue(PageFacts.frozen, "a configured call is not an active call")
        AudioSession.callActivated(true)
        XCTAssertFalse(PageFacts.frozen)
        AudioSession.releaseCall(call)
        AudioSession.callActivated(false)
        XCTAssertTrue(PageFacts.frozen)
        audioMode = false
        try AudioSession.hold(id)
        XCTAssertTrue(PageFacts.frozen, "playback without the background mode cannot continue")
        AudioSession.release(id)
    }
}
#endif
#endif
