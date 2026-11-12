#if os(iOS)
import AVFoundation
import ObjectiveC
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactSurfaces

private var categorySets = 0
private var activations: [Bool] = []
private var surfaceCreationAudio: [(category: AVAudioSession.Category, activations: [Bool])] = []

extension AVAudioSession {
    @objc fileprivate func countedCategory(_ category: AVAudioSession.Category, options: AVAudioSession.CategoryOptions) throws {
        categorySets += 1
        try countedCategory(category, options: options)
    }
    @objc fileprivate func countedCategory(_ category: AVAudioSession.Category, mode: AVAudioSession.Mode,
                                           options: AVAudioSession.CategoryOptions) throws {
        categorySets += 1
        try countedCategory(category, mode: mode, options: options)
    }
    @objc fileprivate func countedActive(_ active: Bool) throws {
        activations.append(active)
        try countedActive(active)
    }
    @objc fileprivate func countedActive(_ active: Bool, options: AVAudioSession.SetActiveOptions) throws {
        activations.append(active)
        try countedActive(active, options: options)
    }
}

/// LLP 1096 D8: the app's category and options follow its holders, and an
/// unclaimed session takes ambient before a GPU module makes a surface.
final class AudioSessionIOSTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() }
    private var saved = AudioSession.configuration
    override func setUp() { AudioSession.reset() }
    override func tearDown() { AudioSession.reset(); AudioSession.configuration = saved }

    private var session: AVAudioSession { AVAudioSession.sharedInstance() }

    /// Count the real setters; unchanged readback alone cannot detect a redundant set.
    private func counting(_ body: () throws -> Void) throws {
        let selectors: [(Selector, Selector)] = [
            (#selector(AVAudioSession.setCategory(_:options:)), #selector(AVAudioSession.countedCategory(_:options:))),
            (#selector(AVAudioSession.setCategory(_:mode:options:)), #selector(AVAudioSession.countedCategory(_:mode:options:))),
            (NSSelectorFromString("setActive:error:"), #selector(AVAudioSession.countedActive(_:))),
            (#selector(AVAudioSession.setActive(_:options:)), #selector(AVAudioSession.countedActive(_:options:))),
        ]
        let methods = try selectors.map { original, counted in
            (try XCTUnwrap(class_getInstanceMethod(AVAudioSession.self, original)),
             try XCTUnwrap(class_getInstanceMethod(AVAudioSession.self, counted)))
        }
        for (original, counted) in methods { method_exchangeImplementations(original, counted) }
        defer { for (original, counted) in methods { method_exchangeImplementations(original, counted) } }
        categorySets = 0; activations = []
        try body()
    }

    func testConfigurationsReadBackWithoutReapplyingOnAHold() throws {
        final class Holder {}
        let holder = Holder()
        for value in ["ambient", "playback", "playback-mix"] {
            AudioSession.configuration = .init(value)
            try session.setCategory(AudioSession.configuration.category, options: AudioSession.configuration.options)
            let readback = session.categoryOptions
            // Ambient's mixing is inherent, whether or not readback includes the mix bit.
            XCTAssertEqual(value == "ambient" ? readback.subtracting(.mixWithOthers) : readback,
                           AudioSession.configuration.options, value)
            try counting {
                try AudioSession.hold(ObjectIdentifier(holder))
                try AudioSession.hold(ObjectIdentifier(holder))
                XCTAssertEqual(categorySets, 0, value)
                XCTAssertEqual(session.categoryOptions, readback, value)
            }
            AudioSession.release(ObjectIdentifier(holder))
        }
    }

    func testPlaybackMixHoldsAndRestoresItsOptionAfterCaptureAndCall() throws {
        AudioSession.configuration = .init("playback-mix")
        final class Holder {}
        let holder = Holder()
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(session.category, .playback)
        XCTAssertEqual(session.categoryOptions, [.mixWithOthers])
        let capture = AudioSession.claimCapture(mode: .default, options: [.defaultToSpeaker], background: false) { _ in }
        XCTAssertNotEqual(capture, 0)
        XCTAssertEqual(session.category, .playAndRecord)
        AudioSession.releaseCapture(capture)
        XCTAssertEqual(session.category, .playback)
        XCTAssertEqual(session.categoryOptions, [.mixWithOthers])
        let call = AudioSession.claimCall(video: false) { _ in }
        XCTAssertEqual(session.category, .playAndRecord)
        AudioSession.callActivated(true)
        AudioSession.releaseCall(call)
        XCTAssertEqual(session.category, .playAndRecord, "restoration waits for the system's deactivation")
        try counting {
            AudioSession.callActivated(false)
            XCTAssertEqual(session.category, .playback)
            XCTAssertEqual(session.categoryOptions, [.mixWithOthers])
            XCTAssertEqual(activations, [], "restoring a call does not activate")
        }
        AudioSession.release(ObjectIdentifier(holder))
        XCTAssertEqual(session.category, .ambient)
        // The category alone matches: a non-mixing app must still clear the option.
        try session.setCategory(.playback, options: [.mixWithOthers])
        AudioSession.configuration = .init("playback")
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(session.category, .playback)
        XCTAssertEqual(session.categoryOptions, [])
        AudioSession.release(ObjectIdentifier(holder))
    }

    func testPreparationSetsAmbientOnceWithoutActivating() throws {
        XCTAssertFalse(ExactEnv.agentFreezes, "this test needs a live surface clock")
        try session.setActive(false)
        try session.setCategory(.soloAmbient)
        try counting {
            AudioSession.prepareForSurface()
            XCTAssertEqual(session.category, .ambient)
            XCTAssertEqual(categorySets, 1)
            AudioSession.prepareForSurface()
            XCTAssertEqual(categorySets, 1, "ambient is already in force")
            XCTAssertEqual(activations, [], "the inactive session is never activated")
        }
    }

    func testTheHostPreparesBeforeTheModuleCreatesItsSurface() throws {
        XCTAssertFalse(ExactEnv.agentFreezes, "this test needs a live surface clock")
        let appSession = ExactApp.shared.makeSession(label: "surface-audio-preparation")
        defer { appSession.destroy() }
        let module = GpuModule(create: { _, _, _, _, _ in
                surfaceCreationAudio.append((AVAudioSession.sharedInstance().category, activations))
                return 7
            }, bind: { _, _, _ in 0 }, render: { _, _, _, _, _ in 0 }, dirty: { _ in 0 }, destroy: { _ in },
            texture: { _, _, _, _, _ in 0 }, textureMetal: nil, sync: nil,
            childrenMode: { _ in 0 }, readback: { _, _, _, _, _, _, _ in 0 },
            child: { _, _, _, _, _, _, _, _, _, _, _, _ in 0 }, childrenCount: { _, _ in 0 }, placement: { _, _, _, _ in 0 },
            shader: nil, validateShader: nil, clearShaders: nil, errorLen: { 0 }, errorPtr: { nil },
            wantsInput: nil, input: nil, messages: nil, published: nil, agent: nil, outPtr: nil)
        let host = appSession.surfaceHost
        host.modules[""] = module; host.attempted = [""]
        let canvas = NodeView(id: 100, kind: "canvas", presenter: appSession.presenter)
        canvas.frame = CGRect(x: 0, y: 0, width: 100, height: 60)
        appSession.presenter.root.addSubview(canvas); appSession.presenter.views[100] = canvas
        XCTAssertNotNil(canvas.canvasMetal)
        try session.setActive(false)
        try session.setCategory(.soloAmbient)
        surfaceCreationAudio = []
        try counting {
            host.surface(view: canvas, name: "audio-fixture", values: [])
            XCTAssertEqual(host.entries[100]?.id, 7, "the host created the module's surface")
            XCTAssertEqual(surfaceCreationAudio.count, 1)
            let seen = try XCTUnwrap(surfaceCreationAudio.first)
            XCTAssertEqual(seen.category, .ambient, "ambient must be in force on entry to create")
            XCTAssertEqual(seen.activations, [], "preparation leaves the inactive session inactive")
        }
    }

    func testPreparationLeavesEveryHoldAndClaimAlone() throws {
        final class Holder {}
        let holder = Holder()
        func unchanged() throws {
            let category = session.category, options = session.categoryOptions, mode = session.mode
            try counting {
                AudioSession.prepareForSurface()
                XCTAssertEqual(categorySets, 0)
                XCTAssertEqual(activations, [])
                XCTAssertEqual(session.category, category)
                XCTAssertEqual(session.categoryOptions, options)
                XCTAssertEqual(session.mode, mode)
            }
        }
        AudioSession.configuration = .init("playback-mix")
        try AudioSession.hold(ObjectIdentifier(holder))
        try unchanged()
        AudioSession.release(ObjectIdentifier(holder))
        let capture = AudioSession.claimCapture(mode: .default, options: [.defaultToSpeaker], background: false) { _ in }
        XCTAssertNotEqual(capture, 0)
        try unchanged()
        AudioSession.releaseCapture(capture)
        // A player delays speech configuration, so this is a claim with no spoken type applied.
        AudioSession.playerCame(ObjectIdentifier(holder))
        try session.setCategory(.soloAmbient)
        let speech = AudioSession.claimSpeech(type: "transient") { _ in }
        XCTAssertNotEqual(speech, 0)
        XCTAssertNil(AudioSession.spoken)
        try unchanged()
        AudioSession.playerWent(ObjectIdentifier(holder))
        XCTAssertEqual(AudioSession.spoken, "transient")
        AudioSession.playerCame(ObjectIdentifier(holder))
        AudioSession.releaseSpeech(speech)
        try unchanged() // The spoken configuration outlives the claim while the player runs.
        AudioSession.playerWent(ObjectIdentifier(holder))
        let call = AudioSession.claimCall(video: false) { _ in }
        try unchanged()
        AudioSession.callActivated(true)
        AudioSession.releaseCall(call)
        try unchanged() // The system has not yet deactivated the call's session.
        AudioSession.callActivated(false)
    }

    func testAPlaybackAppHoldsPlaybackWhileSomethingHasSoundAndThenGoesAmbient() throws {
        AudioSession.configuration = .init("playback")
        final class Holder {}
        let a = Holder(), b = Holder()
        try AudioSession.hold(ObjectIdentifier(a))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playback)
        try AudioSession.hold(ObjectIdentifier(b))
        AudioSession.release(ObjectIdentifier(a))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playback, "one still has sound")
        AudioSession.release(ObjectIdentifier(b))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .ambient, "nothing has sound")
        // A release of something that never held is nothing.
        AudioSession.release(ObjectIdentifier(a))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .ambient)
        try AudioSession.hold(ObjectIdentifier(a))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playback, "sound again takes it back")
        AudioSession.release(ObjectIdentifier(a))
    }

    /// A muted player keeps running, so with one alive the category goes
    /// ambient but the session is not given up under it.
    func testWithAPlayerAliveReleaseGoesAmbientAndThePlayerLeavingGivesItUp() throws {
        AudioSession.configuration = .init("playback")
        final class Holder {}
        let player = Holder()
        AudioSession.playerCame(ObjectIdentifier(player))
        try AudioSession.hold(ObjectIdentifier(player))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playback)
        AudioSession.release(ObjectIdentifier(player))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .ambient)
        AudioSession.playerWent(ObjectIdentifier(player))
        AudioSession.playerWent(ObjectIdentifier(player)) // a second leave is nothing
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .ambient)
    }

    func testSoundAndPlayerReleasesLeaveACallsSessionToTheSystem() throws {
        final class Holder {}
        let sound = Holder(), player = Holder()
        for value in ["playback", "playback-mix"] {
            for video in [false, true] {
                for hasPlayer in [false, true] {
                    AudioSession.configuration = .init(value)
                    try AudioSession.hold(ObjectIdentifier(sound))
                    if hasPlayer { AudioSession.playerCame(ObjectIdentifier(player)) }
                    let call = AudioSession.claimCall(video: video) { _ in }
                    AudioSession.callActivated(true)
                    let mode: AVAudioSession.Mode = video ? .videoChat : .voiceChat
                    XCTAssertEqual(session.category, .playAndRecord)
                    XCTAssertEqual(session.mode, mode)
                    let options = session.categoryOptions
                    XCTAssertTrue(options.contains(.allowBluetoothHFP))

                    func unchanged(_ step: String) {
                        XCTAssertEqual(session.category, .playAndRecord, step)
                        XCTAssertEqual(session.mode, mode, step)
                        XCTAssertEqual(session.categoryOptions, options, step)
                        XCTAssertEqual(categorySets, 0, step)
                        XCTAssertEqual(activations, [], step)
                    }
                    try counting {
                        AudioSession.release(ObjectIdentifier(sound))
                        unchanged("sound released")
                        if hasPlayer {
                            AudioSession.playerWent(ObjectIdentifier(player))
                            unchanged("player removed")
                        }
                        let capture = AudioSession.claimCapture(mode: .default, options: [], background: false) { _ in }
                        XCTAssertNotEqual(capture, 0)
                        unchanged("capture joins the call")
                        AudioSession.releaseCapture(capture)
                        unchanged("capture released")
                        AudioSession.callActivated(false)
                        unchanged("the system deactivated a call whose claim is still held")
                        AudioSession.callActivated(true)
                        AudioSession.releaseCall(call)
                        unchanged("restoration waits for the system's deactivation")
                        AudioSession.callActivated(false)
                        XCTAssertEqual(session.category, .ambient, "the released sound holds nothing after the call")
                        XCTAssertEqual(activations, [], "restoring a call leaves activation to the system")
                    }
                    try counting {
                        try AudioSession.hold(ObjectIdentifier(sound))
                        AudioSession.release(ObjectIdentifier(sound))
                        XCTAssertTrue(activations.contains(false), "the removed player no longer keeps the session active")
                    }
                }
            }
        }
    }

    func testReleasesAfterTheLastCallClaimWaitForSystemDeactivation() throws {
        final class Holder {}
        let holder = Holder(), sound = ObjectIdentifier(holder)
        for value in ["playback", "playback-mix"] {
            for video in [false, true] {
                for kind in ["sound", "player", "capture", "speech"] {
                    AudioSession.reset()
                    AudioSession.configuration = .init(value)
                    var claim: UInt64 = 0
                    switch kind {
                    case "sound": try AudioSession.hold(sound)
                    case "player": AudioSession.playerCame(sound)
                    case "capture":
                        claim = AudioSession.claimCapture(mode: .default, options: [.defaultToSpeaker], background: false) { _ in }
                    default: claim = AudioSession.claimSpeech(type: "transient") { _ in }
                    }
                    let call = AudioSession.claimCall(video: video) { _ in }
                    AudioSession.callActivated(true)
                    AudioSession.releaseCall(call)
                    XCTAssertFalse(AudioSession.isCall(call))
                    XCTAssertTrue(AudioSession.callActive)
                    let mode = session.mode, options = session.categoryOptions
                    func unchanged(_ step: String) {
                        XCTAssertEqual(session.category, .playAndRecord, "\(value) \(kind): \(step)")
                        XCTAssertEqual(session.mode, mode, step)
                        XCTAssertEqual(session.categoryOptions, options, step)
                        XCTAssertEqual(categorySets, 0, step)
                        XCTAssertEqual(activations, [], step)
                    }
                    try counting {
                        switch kind {
                        case "sound": AudioSession.release(sound)
                        case "player": AudioSession.playerWent(sound)
                        case "capture": AudioSession.releaseCapture(claim)
                        default: AudioSession.releaseSpeech(claim)
                        }
                        unchanged("released before CallKit deactivated")
                    }
                    try counting {
                        AudioSession.callActivated(false)
                        XCTAssertEqual(session.category, .ambient, "the released \(kind) holds nothing after the call")
                        XCTAssertEqual(activations, [], "restoration leaves activation to the system")
                    }
                    if kind == "speech" {
                        XCTAssertNil(AudioSession.spoken)
                        try AudioSession.hold(sound)
                        XCTAssertEqual(session.category, .playback)
                        XCTAssertEqual(session.categoryOptions, AudioSession.configuration.options)
                        AudioSession.release(sound)
                        let speech = AudioSession.claimSpeech(type: "transient") { _ in }
                        XCTAssertNotEqual(speech, 0)
                        XCTAssertEqual(session.category, .playback)
                        XCTAssertEqual(session.mode, .voicePrompt)
                        XCTAssertTrue(session.categoryOptions.contains(.duckOthers))
                        XCTAssertTrue(session.categoryOptions.contains(.interruptSpokenAudioAndMixWithOthers))
                        AudioSession.releaseSpeech(speech)
                    }
                }
            }
        }
    }

    func testClaimsDuringACallAreRestoredAfterSystemDeactivation() throws {
        final class Holder {}
        let holder = Holder(), sound = ObjectIdentifier(holder)
        for value in ["playback", "playback-mix"] {
            for releaseBeforeClaim in [false, true] {
                for kind in ["sound", "capture", "speech"] {
                    AudioSession.reset()
                    AudioSession.configuration = .init(value)
                    let call = AudioSession.claimCall(video: false) { _ in }
                    AudioSession.callActivated(true)
                    if releaseBeforeClaim { AudioSession.releaseCall(call) }
                    let mode = session.mode, options = session.categoryOptions
                    var claim: UInt64 = 0
                    try counting {
                        switch kind {
                        case "sound": try AudioSession.hold(sound)
                        case "capture":
                            claim = AudioSession.claimCapture(mode: .default, options: [.defaultToSpeaker], background: false) { _ in }
                            XCTAssertNotEqual(claim, 0)
                        default:
                            claim = AudioSession.claimSpeech(type: "transient") { _ in }
                            XCTAssertNotEqual(claim, 0)
                        }
                        XCTAssertEqual(session.category, .playAndRecord, kind)
                        XCTAssertEqual(session.mode, mode, kind)
                        XCTAssertEqual(session.categoryOptions, options, kind)
                        XCTAssertEqual(categorySets, 0, kind)
                        XCTAssertEqual(activations, [], kind)
                        if !releaseBeforeClaim { AudioSession.releaseCall(call) }
                        XCTAssertEqual(session.category, .playAndRecord, kind)
                        XCTAssertEqual(session.mode, mode, kind)
                        XCTAssertEqual(session.categoryOptions, options, kind)
                        XCTAssertEqual(categorySets, 0, kind)
                        XCTAssertEqual(activations, [], kind)
                    }
                    try counting {
                        AudioSession.callActivated(false)
                        switch kind {
                        case "sound":
                            XCTAssertEqual(session.category, .playback)
                            XCTAssertEqual(session.categoryOptions, AudioSession.configuration.options)
                        case "capture":
                            XCTAssertEqual(session.category, .playAndRecord)
                            XCTAssertEqual(session.mode, .default)
                            XCTAssertEqual(session.categoryOptions, [.defaultToSpeaker])
                        default:
                            XCTAssertEqual(session.category, .playback)
                            XCTAssertEqual(session.mode, .voicePrompt)
                            XCTAssertTrue(session.categoryOptions.contains(.duckOthers))
                            XCTAssertTrue(session.categoryOptions.contains(.interruptSpokenAudioAndMixWithOthers))
                            XCTAssertEqual(AudioSession.spoken, "transient")
                        }
                        XCTAssertEqual(activations, [])
                    }
                    switch kind {
                    case "sound": AudioSession.release(sound)
                    case "capture": AudioSession.releaseCapture(claim)
                    default: AudioSession.releaseSpeech(claim)
                    }
                    XCTAssertEqual(session.category, .ambient)
                }
            }
        }
    }

    func testAMediaServicesResetRestoresTheCallsConfigurationWithoutActivating() throws {
        for video in [false, true] {
            for released in [false, true] {
                for kind in ["none", "capture", "speech"] {
                    AudioSession.reset()
                    switch kind {
                    case "capture":
                        _ = AudioSession.claimCapture(mode: .measurement, options: [.defaultToSpeaker], background: false) { _ in }
                    case "speech": _ = AudioSession.claimSpeech(type: "transient") { _ in }
                    default: break
                    }
                    let call = AudioSession.claimCall(video: video) { _ in }
                    AudioSession.callActivated(true)
                    if released { AudioSession.releaseCall(call) }
                    let mode = session.mode, options = session.categoryOptions
                    try session.setCategory(.ambient)
                    try counting {
                        NotificationCenter.default.post(name: AVAudioSession.mediaServicesWereResetNotification, object: nil)
                        XCTAssertEqual(session.category, .playAndRecord, kind)
                        XCTAssertEqual(session.mode, mode, kind)
                        XCTAssertEqual(session.categoryOptions, options, kind)
                        XCTAssertEqual(activations, [], "a reset never activates")
                    }
                    if !released { AudioSession.releaseCall(call) }
                    AudioSession.callActivated(false)
                }
            }
        }
    }

    /// A hold whose activation fails (during a call) still wants the
    /// session: the next activation takes playback, and its release yields.
    func testAFailedActivationStillHoldsAndIsActivatedAgain() throws {
        AudioSession.configuration = .init("playback")
        final class Holder {}
        let a = Holder(), b = Holder()
        struct Busy: Error {}
        AudioSession.nextHoldFailure = Busy()
        XCTAssertThrowsError(try AudioSession.hold(ObjectIdentifier(a)))
        try AudioSession.hold(ObjectIdentifier(b))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playback, "activated on the next try")
        AudioSession.release(ObjectIdentifier(b))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playback, "the failed one still wants it")
        AudioSession.release(ObjectIdentifier(a))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .ambient)
    }

    /// LLP 1133.009 D7: a capture's claim takes `.playAndRecord` from the
    /// first claim to the last, whatever the app's sound holds meanwhile,
    /// and the session goes back to what the sound wants after.
    func testACaptureClaimsPlayAndRecordUntilItsLastClaimEnds() throws {
        AudioSession.configuration = .init("playback")
        final class Holder {}
        let sound = Holder()
        let first = AudioSession.claimCapture(mode: .default, options: [.defaultToSpeaker], background: false) { _ in }
        XCTAssertNotEqual(first, 0)
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playAndRecord)
        try AudioSession.hold(ObjectIdentifier(sound))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playAndRecord, "the app's sound plays in the capture's category")
        let second = AudioSession.claimCapture(mode: .measurement, options: [], background: false) { _ in }
        AudioSession.releaseCapture(first)
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playAndRecord, "a claim is left")
        AudioSession.release(ObjectIdentifier(sound))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playAndRecord, "the sound's end leaves the capture's category")
        try AudioSession.hold(ObjectIdentifier(sound))
        AudioSession.releaseCapture(second)
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playback, "the last claim gone, the sound's category")
        AudioSession.release(ObjectIdentifier(sound))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .ambient)
    }

    /// A claim whose activation fails (during a call) is refused, 0, and holds nothing.
    func testARefusedClaimHoldsNothing() {
        struct Busy: Error {}
        AudioSession.nextHoldFailure = Busy()
        XCTAssertEqual(AudioSession.claimCapture(mode: .default, options: [], background: true) { _ in }, 0)
        XCTAssertFalse(AudioSession.playsInBackground, "no claim is held")
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .ambient)
    }

    func testAnAmbientAppStaysAmbient() throws {
        AudioSession.configuration = .init("ambient")
        final class Holder {}
        let a = Holder()
        try AudioSession.hold(ObjectIdentifier(a))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .ambient)
        AudioSession.release(ObjectIdentifier(a))
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .ambient)
    }

    /// LLP 1133.009 D7: the owner observes the session once for the process
    /// and tells each listening holder of sound before the claims.
    func testAListeningHolderHearsAResetAndTheAppBecomingActiveThroughTheOwner() {
        final class Holder {}
        let holder = Holder()
        var heard: [String] = []
        AudioSession.listen(ObjectIdentifier(holder)) { event in
            switch event {
            case .interruption(let began, _): heard.append(began ? "began" : "ended")
            case .appActive: heard.append("active")
            case .reset: heard.append("reset")
            }
        }
        var told: [UInt32] = []
        let claim = AudioSession.claimCapture(mode: .default, options: [.defaultToSpeaker], background: false) { told.append($0) }
        NotificationCenter.default.post(name: AVAudioSession.mediaServicesWereResetNotification, object: nil)
        NotificationCenter.default.post(name: UIApplication.didBecomeActiveNotification, object: nil)
        XCTAssertEqual(heard, ["reset", "active"])
        XCTAssertEqual(told, [3], "the capture hears the reset; the app's activity is the sound arm's")
        XCTAssertEqual(AVAudioSession.sharedInstance().category, .playAndRecord, "a reset restores the capture's configuration")
        AudioSession.releaseCapture(claim)
    }
}
#endif
