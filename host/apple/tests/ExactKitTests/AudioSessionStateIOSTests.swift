#if os(iOS)
import AVFoundation
import XCTest
@testable import ExactKit

final class AudioSessionStateIOSTests: XCTestCase {
    private final class FakeSession: AudioSessionBackend {
        var calls: [String] = []
        var failure: String?
        struct Failure: Error {}
        private func called(_ name: String) throws {
            calls.append(name)
            if failure == name { failure = nil; throw Failure() }
        }
        var storedCategory: AVAudioSession.Category = .ambient
        var storedMode: AVAudioSession.Mode = .default
        var storedOptions: AVAudioSession.CategoryOptions = [.mixWithOthers]
        var category: AVAudioSession.Category { calls.append("category"); return storedCategory }
        var categoryOptions: AVAudioSession.CategoryOptions { calls.append("options"); return storedOptions }
        var mode: AVAudioSession.Mode { calls.append("mode"); return storedMode }
        var secondaryAudioShouldBeSilencedHint: Bool { calls.append("silenced"); return false }
        func setCategory(_ category: AVAudioSession.Category, mode: AVAudioSession.Mode,
                         options: AVAudioSession.CategoryOptions) throws {
            try called("configure")
            storedCategory = category; storedMode = mode; storedOptions = options
        }
        func setActive(_ active: Bool, options: AVAudioSession.SetActiveOptions) throws {
            try called(active ? "activate" : "deactivate")
        }
    }

    private var savedConfiguration = AudioSession.configuration
    private var savedSession = AudioSession.session
    private var fake = FakeSession()

    override func setUp() {
        super.setUp()
        savedConfiguration = AudioSession.configuration
        savedSession = AudioSession.session
        fake = FakeSession()
        AudioSession.session = fake
        AudioSession.reset()
        fake.calls = []
    }

    override func tearDown() {
        AudioSession.reset()
        AudioSession.session = savedSession
        AudioSession.configuration = savedConfiguration
        super.tearDown()
    }

    func testJoiningAnActiveSessionMakesNoSessionCalls() throws {
        let first = NSObject(), next = NSObject()
        for value in ["ambient", "playback", "playback-mix"] {
            AudioSession.reset()
            AudioSession.configuration = .init(value)
            try AudioSession.hold(ObjectIdentifier(first))
            fake.calls = []
            try AudioSession.hold(ObjectIdentifier(next))
            try AudioSession.hold(ObjectIdentifier(next))
            AudioSession.release(ObjectIdentifier(first))
            XCTAssertEqual(fake.calls, [], value)
        }
    }

    func testAConfigurationChangeAppliesAndActivatesOnce() throws {
        let holder = NSObject()
        AudioSession.configuration = .init("playback")
        try AudioSession.hold(ObjectIdentifier(holder))
        fake.calls = []
        AudioSession.configuration = .init("playback-mix")
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, ["configure", "activate"])
        XCTAssertEqual(fake.storedOptions, [.mixWithOthers])
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, [])
    }

    func testReleaseAndInterruptionRequireActivationAgain() throws {
        let holder = NSObject()
        AudioSession.configuration = .init("playback-mix")
        try AudioSession.hold(ObjectIdentifier(holder))
        fake.calls = []
        AudioSession.release(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, ["deactivate"])
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, ["activate"])
        AudioSession.interrupted()
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, ["activate"])
    }

    func testAMutedPlayerDelaysDeactivation() throws {
        let holder = NSObject()
        AudioSession.configuration = .init("playback-mix")
        AudioSession.playerCame(ObjectIdentifier(holder))
        try AudioSession.hold(ObjectIdentifier(holder))
        fake.calls = []
        AudioSession.release(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, [])
        AudioSession.playerWent(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, ["deactivate"])
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, ["activate"])
    }

    func testHoldersJoinCaptureAndSpeechWithoutSessionCalls() throws {
        let holder = NSObject()
        AudioSession.configuration = .init("playback-mix")
        let capture = AudioSession.claimCapture(mode: .measurement, options: [.defaultToSpeaker], background: false) { _ in }
        XCTAssertNotEqual(capture, 0)
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, [])
        AudioSession.releaseCapture(capture)
        XCTAssertEqual(fake.storedCategory, .playback)
        XCTAssertEqual(fake.storedMode, .default)
        XCTAssertEqual(fake.storedOptions, [.mixWithOthers])
        AudioSession.release(ObjectIdentifier(holder))
        let speech = AudioSession.claimSpeech(type: "transient") { _ in }
        XCTAssertNotEqual(speech, 0)
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, [])
        XCTAssertEqual(fake.storedMode, .voicePrompt)
        AudioSession.release(ObjectIdentifier(holder))
        AudioSession.releaseSpeech(speech)
        XCTAssertEqual(fake.calls, ["configure", "deactivate"])
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, ["activate"])
        XCTAssertEqual(fake.storedMode, .default)
    }

    func testAnIdleCaptureReleaseRequiresActivationForTheNextHolder() throws {
        let holder = NSObject()
        AudioSession.configuration = .init("playback-mix")
        let capture = AudioSession.claimCapture(mode: .default, options: [], background: false) { _ in }
        XCTAssertNotEqual(capture, 0)
        AudioSession.releaseCapture(capture)
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, ["activate"])
    }

    func testCallsKeepActivationWithTheSystemAndRestoreBeforeTheNextHold() throws {
        let holder = NSObject()
        AudioSession.configuration = .init("playback-mix")
        try AudioSession.hold(ObjectIdentifier(holder))
        let call = AudioSession.claimCall(video: true) { _ in }
        AudioSession.callActivated(true)
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        AudioSession.releaseCall(call)
        XCTAssertEqual(fake.calls, [])
        AudioSession.callActivated(false)
        XCTAssertEqual(fake.calls, ["configure"])
        XCTAssertEqual(fake.storedMode, .default)
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, ["activate"])
    }

    func testMediaResetInvalidatesConfigurationAndRouteChangesKeepIt() throws {
        let holder = NSObject()
        AudioSession.configuration = .init("playback")
        try AudioSession.hold(ObjectIdentifier(holder))
        fake.calls = []
        NotificationCenter.default.post(name: AVAudioSession.routeChangeNotification, object: nil)
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, [])
        fake.storedCategory = .soloAmbient
        NotificationCenter.default.post(name: AVAudioSession.mediaServicesWereResetNotification, object: nil)
        XCTAssertFalse(fake.calls.contains("activate"))
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.storedCategory, .playback)
        XCTAssertEqual(fake.calls.filter { $0 == "configure" || $0 == "activate" }, ["configure", "activate"])
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, [])
    }

    func testResetRestoresCaptureAndSpeechWithoutActivation() throws {
        let holder = NSObject()
        for capture in [true, false] {
            AudioSession.reset()
            let claim = capture
                ? AudioSession.claimCapture(mode: .measurement, options: [.defaultToSpeaker], background: false) { _ in }
                : AudioSession.claimSpeech(type: "transient") { _ in }
            XCTAssertNotEqual(claim, 0)
            fake.storedCategory = .soloAmbient
            fake.storedMode = .default
            fake.storedOptions = []
            fake.calls = []
            NotificationCenter.default.post(name: AVAudioSession.mediaServicesWereResetNotification, object: nil)
            XCTAssertEqual(fake.storedCategory, capture ? .playAndRecord : .playback)
            XCTAssertEqual(fake.storedMode, capture ? .measurement : .voicePrompt)
            XCTAssertEqual(fake.calls.filter { $0 == "configure" || $0 == "activate" }, ["configure"])
            fake.calls = []
            try AudioSession.hold(ObjectIdentifier(holder))
            XCTAssertEqual(fake.calls, ["activate"])
            fake.calls = []
            try AudioSession.hold(ObjectIdentifier(holder))
            XCTAssertEqual(fake.calls, [])
        }
    }

    func testFailedSessionOperationsAreRetried() throws {
        let holder = NSObject()
        AudioSession.configuration = .init("playback-mix")
        fake.failure = "configure"
        XCTAssertThrowsError(try AudioSession.hold(ObjectIdentifier(holder)))
        XCTAssertFalse(fake.calls.contains("activate"))
        fake.failure = "activate"
        XCTAssertThrowsError(try AudioSession.hold(ObjectIdentifier(holder)))
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, ["activate"])
        // A deactivation can make the session inactive even when it throws isBusy.
        fake.failure = "deactivate"
        AudioSession.release(ObjectIdentifier(holder))
        fake.calls = []
        try AudioSession.hold(ObjectIdentifier(holder))
        XCTAssertEqual(fake.calls, ["activate"])
    }

}
#endif
