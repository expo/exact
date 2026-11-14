// @ref LLP 1096 D8, LLP 1133.009 D7. The app's one AVAudioSession owner: the
// only `setCategory` caller, so the sound arm, a video with sound, a canvas's
// audio and a package's capture all run in one session the owner composes.
// iOS and tvOS only: AVAudioSession is unavailable on the Mac, where the arm
// starts its engine with no category and a capture's claim composes nothing.
//
// The app holds `.ambient`, `.playback`, or `.playback` with `.mixWithOthers`
// (`playback-mix`). With no holder, claim or active call it rests: `.ambient`, or for
// `playback-mix` the app's own configuration, which already mixes, so its
// category never changes under output that is running. Before a GPU module
// makes a surface, an unclaimed session takes its resting configuration
// without activation, so an output opened before its `exact:audio` request
// mixes (and, under `playback-mix`, is already in the category the request
// asks for: a change while that output plays interrupts other apps' audio).
//
// A capture (a microphone, LLP 1133.009) claims the session: while any claim
// is held the category is `.playAndRecord`, with the mode and options of the
// claim that started the running capture, so the app's own sound still plays
// (`.record` would silence it); the category changes when the first claim
// comes and when the last goes, and nowhere else while one is held. Each claim
// hears the owner's interruptions, their ends, media-services resets and
// route changes, observed once for the process on the main queue; so does
// each holder of sound that listens (the sound arm).
//
// Speech claims too (LLP 1133.012 D4): a queue that speaks holds one, of a
// spoken type, and the process's speech configuration is the strongest held
// (`playback` over `transient-solo` over `transient`), all `.playback`. The
// session changes for speech only while no other I/O runs (no capture, no
// live player, no holder left once an idle sound arm has been asked to stop),
// since a change stops a running engine or player; until then speech plays in
// the session as it stands, and the next boundary at which nothing runs
// applies it. When the last speech claim goes and nothing else runs, the
// session is given up with `.notifyOthersOnDeactivation`, so what a prompt
// ducked or interrupted comes back.
//
// A call claims too (LLP 1133.033 D10), made by `@exact/calls` when CallKit
// starts or answers a call: `.playAndRecord` in `.voiceChat` (`.videoChat`
// for a video call), set before the action is fulfilled, and never
// activated here: the system activates a call's session
// (`provider(_:didActivate:)`), and an app that activates first blocks the
// phone session. Until its last claim ends and the system deactivates it,
// no other path changes the category or activation: captures and the app's
// own sound use the call's configuration. The claim lasts while any call
// does; when the last ends, the app's category comes back at the deactivation
// that follows, or at once, without activating, when already inactive.
#if os(iOS) || os(tvOS)
import AVFoundation
import Foundation
import UIKit

package enum AudioSession {
    /// `app.json`'s `audio_session`, baked into the Info.plist as
    /// `ExactAudioSession`. Both playback values ignore the ring/silent
    /// switch; `playback-mix` also leaves other apps' audio at its level.
    struct Configuration {
        let category: AVAudioSession.Category
        let options: AVAudioSession.CategoryOptions
        init(_ value: String?) {
            category = value == "playback" || value == "playback-mix" ? .playback : .ambient
            options = value == "playback-mix" ? [.mixWithOthers] : []
        }
    }
    /// The app's category and options, settable together for tests.
    nonisolated(unsafe) static var configuration = Configuration(Bundle.main.object(forInfoDictionaryKey: "ExactAudioSession") as? String)
    /// The session with nothing holding it. Returning to ambient exists to stop
    /// interrupting other apps' audio, which a mixing configuration never does.
    static var resting: Configuration {
        configuration.options.contains(.mixWithOthers) ? configuration : Configuration(nil)
    }
    /// What wants the app's category now: the sound arm and a canvas's audio
    /// for as long as they run, and each video while it has sound. A want
    /// whose activation failed (during a call) stays, and is activated again
    /// when the interruption ends with `shouldResume`.
    nonisolated(unsafe) private static var holders: Set<ObjectIdentifier> = []
    /// Every live media player, muted or not: deactivating the session would
    /// stop a running one, so the session is only given up when none is left.
    nonisolated(unsafe) private static var players: Set<ObjectIdentifier> = []
    nonisolated(unsafe) private static var observing = false
    /// A canvas's hold, which it never gives back.
    private final class Forever {}
    private static let forever = Forever()

    /// A capture's claim: its mode, options, whether it may run in the
    /// background, and what it is told (1 interrupted, 2 the interruption
    /// ended, 3 media services were reset, 4 the route changed).
    struct Capture {
        let mode: AVAudioSession.Mode, options: AVAudioSession.CategoryOptions, background: Bool
        let events: (UInt32) -> Void
    }
    nonisolated(unsafe) private static var captures: [UInt64: Capture] = [:]
    nonisolated(unsafe) private static var nextCapture: UInt64 = 0
    /// The first capture's mode and options, kept until the last capture ends
    /// (LLP 1133.009 D3, D7), including while a call defers applying them.
    nonisolated(unsafe) private static var configuring: Capture?

    /// A queue's speech claim (LLP 1133.012 D4): its spoken type and what it is told.
    struct Speech {
        let type: String, events: (UInt32) -> Void
    }
    nonisolated(unsafe) private static var speeches: [UInt64: Speech] = [:]
    /// The spoken type whose configuration the session has now, if any.
    nonisolated(unsafe) private(set) static var spoken: String?
    /// Holders that stop when idle: the sound arm's engine with no voice
    /// submitted and not ended. Each answers whether it let its hold go.
    nonisolated(unsafe) private static var idlers: [ObjectIdentifier: () -> Bool] = [:]
    nonisolated(unsafe) private static var composing = false
    /// The calls' claims, and whether the system has activated their session.
    nonisolated(unsafe) private static var calls: Set<UInt64> = []
    nonisolated(unsafe) private(set) static var callActive = false
    /// Kept through system deactivation so a media-services reset can restore it.
    nonisolated(unsafe) private static var callMode: AVAudioSession.Mode?
    /// The call owns the session until its last claim ends and the system deactivates it.
    static var inCall: Bool { !calls.isEmpty || callActive }
    static let spokenTypes = ["transient", "transient-solo", "playback"]

    /// The process keeps running in the background with `audio` in
    /// `UIBackgroundModes` and either playback value (LLP 1096 D8), so
    /// a sequence plays on with the screen locked (LLP 1125 D4), or a capture
    /// claim that may run in the background (LLP 1133.009 D7).
    static var playsInBackground: Bool {
        guard (Bundle.main.object(forInfoDictionaryKey: "UIBackgroundModes") as? [String] ?? []).contains("audio") else { return false }
        return configuration.category == .playback || captures.values.contains { $0.background }
            || (spoken == "playback" && speeches.values.contains { $0.type == "playback" })
    }
    /// For tests: the next activation fails with this, as it can during a call.
    nonisolated(unsafe) static var nextHoldFailure: Error?

    /// Prepare for a GPU module opening output before its `exact:audio`
    /// request: the resting configuration. No activation, and no change to a
    /// held or claimed session. The driver's frozen clock opens no output;
    /// platform timing does.
    package static func prepareForSurface() {
        guard !ExactEnv.agentFreezes, holders.isEmpty, captures.isEmpty, configuring == nil,
              speeches.isEmpty, spoken == nil, !inCall else { return }
        try? apply(resting)
    }

    /// Set the app's category and options and activate the session, for good (a
    /// canvas's audio).
    package static func activate() throws {
        try hold(ObjectIdentifier(forever))
    }

    /// `holder` plays sound: the session takes the app's category and options and is
    /// active while anything wants it. A failure to activate throws, and the
    /// want stays for the interruption's end to activate.
    package static func hold(_ holder: ObjectIdentifier) throws {
        holders.insert(holder)
        observe()
        try reconcile()
    }

    /// `holder` no longer plays sound. With nothing left wanting it, a
    /// playback app goes back to `.ambient`, which mixes with other apps'
    /// audio, as Bluesky's player does on re-mute; `playback-mix` already
    /// mixes, so it stays. A muted player keeps
    /// running, so the session itself is given up (telling other apps they
    /// may resume) only once no player is left. A capture's category stays;
    /// a call keeps its category and activation.
    package static func release(_ holder: ObjectIdentifier) {
        guard holders.remove(holder) != nil, holders.isEmpty, captures.isEmpty, !inCall else { return }
        // A boundary at which nothing may run: speech takes its configuration, or gives it up.
        if !speeches.isEmpty || spoken != nil { return composeSpeech() }
        guard configuration.category != .ambient else { return }
        try? apply(resting)
        giveUpIfIdle()
    }

    /// `holder` stops itself when the owner asks and it has nothing to play
    /// (the sound arm's engine): `stop` answers whether it let its hold go.
    package static func stopsWhenIdle(_ holder: ObjectIdentifier, _ stop: @escaping () -> Bool) { idlers[holder] = stop }
    package static func forgetIdle(_ holder: ObjectIdentifier) { idlers[holder] = nil }

    /// A media player exists, muted or not.
    package static func playerCame(_ player: ObjectIdentifier) { players.insert(player) }
    /// A media player is gone; with no player and no holder left, a
    /// playback app's session is given up unless a claim keeps it.
    package static func playerWent(_ player: ObjectIdentifier) {
        guard players.remove(player) != nil else { return }
        if !speeches.isEmpty || spoken != nil { return composeSpeech() }
        giveUpIfIdle()
    }

    /// A queue's speech claim (LLP 1133.012 D4) of `type`, one of
    /// `spokenTypes`. 0 when the session would not activate (a call).
    package static func claimSpeech(type: String, events: @escaping (UInt32) -> Void) -> UInt64 {
        observe()
        nextCapture += 1
        let id = nextCapture
        speeches[id] = Speech(type: type, events: events)
        do { try composeSpeechThrowing() } catch {
            speeches[id] = nil
            composeSpeech()
            return 0
        }
        return id
    }

    /// A speech claim ends: with none left and nothing else running, the
    /// session is given up and other apps' audio returns.
    package static func releaseSpeech(_ id: UInt64) {
        guard speeches.removeValue(forKey: id) != nil else { return }
        composeSpeech()
    }

    /// Whether `id` is a speech claim (the claim and release hatches take both kinds).
    package static func isSpeech(_ id: UInt64) -> Bool { speeches[id] != nil }

    /// The strongest spoken type held, if any.
    private static var strongestSpeech: String? {
        speeches.values.compactMap { spokenTypes.firstIndex(of: $0.type) }.max().map { spokenTypes[$0] }
    }

    /// What the session takes for a spoken type (D4's table).
    static func speechConfiguration(_ type: String) -> (AVAudioSession.Mode, AVAudioSession.CategoryOptions) {
        switch type {
        case "transient": (.voicePrompt, [.duckOthers, .interruptSpokenAudioAndMixWithOthers])
        case "playback": (.spokenAudio, [])
        default: (.default, [])
        }
    }

    /// Whether I/O other than speech runs: a capture, a live player, or a
    /// holder that would not stop. Idle holders are asked to stop first.
    private static var othersRun: Bool {
        if !captures.isEmpty || !players.isEmpty { return true }
        for (holder, stop) in idlers where holders.contains(holder) { _ = stop() }
        return !holders.isEmpty
    }

    private static func composeSpeech() {
        do { try composeSpeechThrowing() } catch {}
    }

    /// The speech configuration, at a boundary: applied, kept, or given up,
    /// only while nothing else runs.
    private static func composeSpeechThrowing() throws {
        guard !composing, !inCall else { return }
        composing = true
        defer { composing = false }
        // A capture's configuration holds for its whole length; speech plays under it.
        guard captures.isEmpty else { return }
        let want = strongestSpeech, session = AVAudioSession.sharedInstance()
        if want == spoken {
            if want != nil { try activateForSpeech(session) }
            return
        }
        if othersRun {
            // Speech plays in the session as it stands; the next boundary applies it.
            if want != nil { try activateForSpeech(session) }
            return
        }
        if let want {
            if let failure = nextHoldFailure { nextHoldFailure = nil; throw failure }
            let (mode, options) = speechConfiguration(want)
            try session.setCategory(.playback, mode: mode, options: options)
            try session.setActive(true)
            spoken = want
        } else {
            spoken = nil
            try? session.setCategory(resting.category, options: resting.options)
            try? session.setActive(false, options: .notifyOthersOnDeactivation)
        }
    }

    private static func activateForSpeech(_ session: AVAudioSession) throws {
        if let failure = nextHoldFailure { nextHoldFailure = nil; throw failure }
        try session.setActive(true)
    }

    /// A capture claims the session (LLP 1133.009 D7): `.playAndRecord`
    /// from the first claim, activated. 0 when the session would not
    /// activate (a call), and then nothing is held.
    package static func claimCapture(mode: AVAudioSession.Mode, options: AVAudioSession.CategoryOptions, background: Bool,
                                     failure: (Error) -> Void = { _ in },
                                     events: @escaping (UInt32) -> Void) -> UInt64 {
        observe()
        let claim = Capture(mode: mode, options: options, background: background, events: events)
        if captures.isEmpty {
            if !inCall {
                do {
                    if let failure = nextHoldFailure { nextHoldFailure = nil; throw failure }
                    let session = AVAudioSession.sharedInstance()
                    // The session is the capture's now: speech after it applies its own configuration again.
                    spoken = nil
                    try session.setCategory(.playAndRecord, mode: mode, options: options)
                    try session.setActive(true)
                } catch {
                    failure(error)
                    restoreAfterCaptures()
                    return 0
                }
            }
            configuring = claim
        }
        nextCapture += 1
        captures[nextCapture] = claim
        return nextCapture
    }

    /// A capture's claim ends; with none left the session goes back to what
    /// the app's sound wants.
    package static func releaseCapture(_ id: UInt64) {
        guard captures.removeValue(forKey: id) != nil, captures.isEmpty else { return }
        configuring = nil
        restoreAfterCaptures()
    }

    /// The configuration the app's own sound wants, or the session given up.
    private static func restoreAfterCaptures() {
        guard !inCall else { return }
        let session = AVAudioSession.sharedInstance()
        if !speeches.isEmpty, holders.isEmpty { return composeSpeech() }
        if holders.isEmpty {
            try? apply(resting)
            if players.isEmpty { try? session.setActive(false, options: .notifyOthersOnDeactivation) }
        } else {
            try? apply(configuration)
            try? session.setActive(true)
        }
    }

    /// A call's claim (file doc): the call's category and mode, set now and
    /// not activated. Every claim made meanwhile hears the session's events.
    package static func claimCall(video: Bool, events: @escaping (UInt32) -> Void) -> UInt64 {
        observe()
        if calls.isEmpty {
            callMode = video ? .videoChat : .voiceChat
            spoken = nil
            configureCall()
        }
        nextCapture += 1
        calls.insert(nextCapture)
        callEvents[nextCapture] = events
        return nextCapture
    }
    nonisolated(unsafe) private static var callEvents: [UInt64: (UInt32) -> Void] = [:]

    private static func configureCall() {
        guard let callMode else { return }
        try? AVAudioSession.sharedInstance().setCategory(.playAndRecord, mode: callMode, options: [.allowBluetoothHFP])
    }

    /// Whether `id` is a call's claim.
    package static func isCall(_ id: UInt64) -> Bool { calls.contains(id) }

    /// A call's claim ends. With none left, the app's own configuration
    /// comes back now only if the session is already inactive; else at
    /// `callActivated(false)`.
    package static func releaseCall(_ id: UInt64) {
        guard calls.remove(id) != nil else { return }
        callEvents[id] = nil
        if calls.isEmpty && !callActive { restoreAfterCall() }
    }

    /// The system activated (`provider(_:didActivate:)`) or deactivated the
    /// calls' session: every claim hears it, as an interruption's end (2) or
    /// beginning (1) would tell it.
    package static func callActivated(_ active: Bool) {
        callActive = active
        tell(active ? 2 : 1)
        for e in Array(callEvents.values) { e(active ? 2 : 1) }
        if !active && calls.isEmpty { restoreAfterCall() }
    }

    /// The app's category and options back, without activating: activation waits for
    /// the app's next sound or an interruption's end (D10).
    private static func restoreAfterCall() {
        callMode = nil
        spoken = nil
        let session = AVAudioSession.sharedInstance()
        if !captures.isEmpty, let claim = configuring {
            try? session.setCategory(.playAndRecord, mode: claim.mode, options: claim.options)
        } else if let type = strongestSpeech, holders.isEmpty, players.isEmpty {
            let (mode, options) = speechConfiguration(type)
            do {
                try session.setCategory(.playback, mode: mode, options: options)
                spoken = type
            } catch {}
        } else {
            try? apply(holders.isEmpty ? resting : configuration)
        }
    }

    /// For tests: nothing held, no player, no capture, the category back to ambient.
    static func reset() {
        holders.removeAll()
        players.removeAll()
        captures.removeAll()
        calls.removeAll()
        callEvents.removeAll()
        callActive = false
        callMode = nil
        configuring = nil
        speeches.removeAll()
        spoken = nil
        idlers.removeAll()
        listeners.removeAll()
        nextHoldFailure = nil
        try? AVAudioSession.sharedInstance().setCategory(.ambient)
    }

    private static func reconcile() throws {
        guard !holders.isEmpty || !captures.isEmpty, !inCall else { return }
        if let failure = nextHoldFailure { nextHoldFailure = nil; throw failure }
        let session = AVAudioSession.sharedInstance()
        // A sound started during speech plays in the configuration in force (LLP 1133.012 D4).
        if captures.isEmpty, spoken == nil { try apply(configuration) }
        try session.setActive(true)
    }

    private static func apply(_ configuration: Configuration) throws {
        let session = AVAudioSession.sharedInstance()
        // Ambient mixes inherently; its implicit mix bit is not an app option.
        let options = configuration.category == .ambient ? session.categoryOptions.subtracting(.mixWithOthers) : session.categoryOptions
        guard session.category != configuration.category || options != configuration.options else { return }
        // Reasserting a category or its options can stop a running engine.
        try session.setCategory(configuration.category, options: configuration.options)
    }

    private static func giveUpIfIdle() {
        guard holders.isEmpty, players.isEmpty, captures.isEmpty, speeches.isEmpty,
              !inCall, configuration.category != .ambient else { return }
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    private static func tell(_ event: UInt32) {
        for claim in Array(captures.values) { claim.events(event) }
        for claim in Array(speeches.values) { claim.events(event) }
    }

    /// What a holder of sound hears of the session (D7): the owner observes
    /// once for the process and tells each, before the claims.
    package enum Heard {
        /// An interruption began, or ended with or without the system's recommendation to resume.
        case interruption(began: Bool, resume: Bool)
        /// The app became active, and whether another app's primary audio plays.
        case appActive(othersPlaying: Bool)
        /// The media services were reset.
        case reset
    }
    nonisolated(unsafe) private static var listeners: [ObjectIdentifier: (Heard) -> Void] = [:]

    /// `holder` hears the session's events, through `hear`, from now on.
    package static func listen(_ holder: ObjectIdentifier, _ hear: @escaping (Heard) -> Void) {
        observe()
        listeners[holder] = hear
    }

    private static func heard(_ event: Heard) { for hear in Array(listeners.values) { hear(event) } }

    /// An interruption began (the session was deactivated by the system).
    private static func interrupted() {
        heard(.interruption(began: true, resume: false))
        tell(1)
    }

    /// An interruption ended. The session is taken back only with
    /// `shouldResume`: without it the person started other audio, and the
    /// next want activates the session instead; a capture's tracks hear the
    /// end either way, and its recorders stay paused for the app (D8).
    private static func resumable(_ resume: Bool) {
        if resume { try? reconcile() }
        heard(.interruption(began: false, resume: resume))
        tell(2)
    }

    /// The process's observers, on the main queue. From iOS 27 an
    /// interruption is the session's life cycle: `didBecomeInactive` from
    /// the system and the `resumptionRecommendation` that follows, which
    /// replace the interruption notification
    /// ([AVAudioSession](https://developer.apple.com/documentation/avfaudio/avaudiosession)).
    /// A media-services reset restores the owning call, capture or speech
    /// configuration without activating it (D7).
    private static func observe() {
        guard !observing else { return }
        observing = true
        let center = NotificationCenter.default
        if #available(iOS 27, tvOS 27, *) {
            center.addObserver(forName: AVAudioSession.didBecomeInactiveNotification, object: nil, queue: .main) { note in
                // The app's own deactivation is not an interruption.
                let context = note.userInfo?[AVAudioSession.deactivationContextKey] as? AVAudioSession.DeactivationContext
                if context?.source == .system { interrupted() }
            }
            center.addObserver(forName: AVAudioSession.resumptionRecommendationNotification, object: nil, queue: .main) { note in
                let context = note.userInfo?[AVAudioSession.resumptionContextKey] as? AVAudioSession.ResumptionContext
                resumable(context?.recommendation == .shouldResume)
            }
        } else {
            center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { note in
                guard let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                      let type = AVAudioSession.InterruptionType(rawValue: raw) else { return }
                if type == .began { return interrupted() }
                resumable(AVAudioSession.InterruptionOptions(rawValue: note.userInfo?[AVAudioSessionInterruptionOptionKey] as? UInt ?? 0).contains(.shouldResume))
            }
        }
        center.addObserver(forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main) { _ in
            heard(.appActive(othersPlaying: AVAudioSession.sharedInstance().secondaryAudioShouldBeSilencedHint))
        }
        center.addObserver(forName: AVAudioSession.mediaServicesWereResetNotification, object: nil, queue: .main) { _ in
            if inCall { configureCall() }
            else {
                if let claim = configuring { try? AVAudioSession.sharedInstance().setCategory(.playAndRecord, mode: claim.mode, options: claim.options) }
                else if let type = spoken {
                    let (mode, options) = speechConfiguration(type)
                    try? AVAudioSession.sharedInstance().setCategory(.playback, mode: mode, options: options)
                }
            }
            heard(.reset)
            tell(3)
        }
        center.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: .main) { _ in tell(4) }
    }
}
#endif
