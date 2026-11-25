// @exact/media-capture on Apple (LLP 1133.009 D1-D3, D5-D9): the
// microphone as Media Capture's tracks, one capture for the process, each
// session's tracks in its track table (`context.tracks`) so a package that
// consumes a track (`@exact/recording`) reaches it by id.
//
// - The first lease configures capture for the process (D3). Normally the
//   AVAudioEngine input tap supplies each track. A mixed take uses the
//   camera session's microphone and its PCM output for other readers.
// - A track holds a lease while it has a consumer (D6): its first consumer
//   takes it, and after use only a start does (a recorder's start,
//   `setTrackEnabled(enabled=true)`); a reader joining an idle track hears
//   silence and opens nothing. A tool's or a wake's consumer never leases.
// - The audio session is the host's (D7): the capture claims it
//   (`.playAndRecord`), and hears its interruptions, their ends, resets and
//   route changes. A change of route or hardware format stops the engine,
//   which posts `AVAudioEngineConfigurationChange` on its own queue; the
//   handler only schedules the reopening on main (D7).
// - The permission is a fact (D9): `AVAudioApplication.recordPermission` on
//   iOS, `AVCaptureDevice.authorizationStatus(for: .audio)` on macOS, read
//   before anything starts, since iOS records silence when it is not granted.
// - Unsaid constraints are the platform's (D2): the input node unprocessed.
//   `echoCancellation`, `noiseSuppression` or `autoGainControl` written true
//   turn on voice processing, one switch on Apple.
// - Under the agent no engine is made and nothing is asked: a stream with
//   one track, "Agent microphone", whose mute follows the agent's preferred
//   `mediaStreamTrack` (D10 item 11).
// - Tier 3 (D5): `MediaCaptureModule.hatch` is given the engine and a mixer
//   after the input node once per capture, before it starts. The input
//   node's bus 0 is the fan-out's; a hatch taps the mixer, and
//   `holdsCapture(track:)` makes that tap a consumer that holds the capture.
import AVFoundation
import ExactPackage
import Foundation
#if os(iOS)
import UIKit
#else
import AudioToolbox
import CoreAudio
#endif

public final class MediaCaptureModule: ExactPackage, ExactTrackOwner {
    /// The app's access hatch (D5): the capture's engine and the mixer after
    /// its input node, once per capture, before `start()`. The app may tap
    /// the mixer's output bus and attach nodes after it; the input node, its
    /// bus 0 and every connection into the mixer are the module's.
    public static var hatch: ((AVAudioEngine, AVAudioMixerNode) -> Void)?

    /// A hatch's tap declared a consumer of `track` (D5): it holds the
    /// capture as a recorder does, until the app cancels the answer. Nil for
    /// a track no session holds. On the main thread.
    public static func holdsCapture(track id: String) -> ExactSubscription? {
        for m in ProcessCapture.shared.modules where m.tracks[id] != nil {
            return try? m.subscribe(id, HatchTap(), start: true, cause: ExactTrackCause("app"))
        }
        return nil
    }

    private final class HatchTap: ExactTrackConsumer {
        func muted(_ reason: String) {}
        func unmuted() {}
        func ended() {}
    }
    /// The camera's capture session, inside each configuration, on its queue
    /// (LLP 1133.008 D5): add outputs, never touch UI.
    public static var sessionHatch: ((AVCaptureSession) -> Void)?
    /// The camera after the module's constraints, under its lock, on the
    /// session's queue (LLP 1133.008 D5).
    public static var deviceHatch: ((AVCaptureDevice) -> Void)?

    private let context: ExactPackageContext
    private let audioSource: ExactAudioSource?
    private let substituteOrigin: AVAudioFramePosition?
    private let process: ProcessCapture?
    private var next = 0
    /// The cameras (VideoCapture.swift, LLP 1133.008).
    private lazy var cameras = VideoTracks(context: context) { [unowned self] in next += 1; return next }
    private var tracks: [String: Track] = [:]
    /// The agent's preferred `mediaStreamTrack`: an interruption begun while it says muted.
    private var overlay: String?
    private var opened = false

    final class Track {
        let id: String, stream: String
        let source: ExactAudioSource?
        var movieInput: AVCaptureDeviceInput?, movieSamples: AVCaptureAudioDataOutput?
        var movieConsumer: ExactMovieAudioConsumer? { consumers.values.lazy.compactMap { $0.consumer as? ExactMovieAudioConsumer }.first }
        var movieInBackground = false
        let engineMute: TrackMute
        var enabled = true { didSet { if enabled != oldValue { updateMute() } } }
        var audible: Bool { enabled && platformMute == nil && !pausedForBackground }
        private func updateMute() {
            if let mic = source as? AgentMicrophone { mic.setEnabled(audible) }
            else if movieConsumer == nil { engineMute.history.enabled(audible, at: .init(clock: CMClockGetHostTimeClock())) }
            movieConsumer?.microphone(movieInput, samples: movieSamples, enabled: audible)
            for reader in consumers.values { reader.audio?.enabled = audible }
        }
        var label: String, muted = true, muteReason: String? = "idle", readyState = "live"
        var settings: [String: Any] = [:], options: CaptureOptions
        var consumers: [ObjectIdentifier: (consumer: ExactTrackConsumer, cause: ExactTrackCause, audio: TrackAudioSource?)] = [:]
        var active = false, used = false, leased = false
        var pausedForBackground = false { didSet { if oldValue != pausedForBackground { updateMute() } } }
        var activationError: String?
        /// Why the platform mutes it now, if it does (an interruption, the background).
        var platformMute: String? { didSet { if (oldValue == nil) != (platformMute == nil) { updateMute() } } }
        init(id: String, stream: String, label: String, options: CaptureOptions, source: ExactAudioSource?, mute: TrackMute) {
            self.id = id; self.stream = stream; self.label = label; self.options = options
            self.source = source; engineMute = mute
        }
    }

    public convenience init(context: ExactPackageContext) {
        self.init(context: context, audioSource: context.agent ? nil : ProcessCapture.shared, process: context.agent ? nil : .shared)
    }

    init(context: ExactPackageContext, audioSource: ExactAudioSource?, substituteOrigin: AVAudioFramePosition? = nil, process: ProcessCapture? = nil) {
        self.context = context
        self.substituteOrigin = substituteOrigin
        self.audioSource = audioSource
        self.process = process
        process?.listen(self)
    }

    // MARK: the names

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        let trackId = args["track"] as? String
        switch name {
        case "getUserMedia" where args["video"] as? Bool == true || args["videoConstraints"] is [String: Any]:
            withCamera(args, reply)
        case "getUserMedia": getUserMedia(args, reply)
        case "applyConstraints" where cameras.owns(trackId): cameras.applyConstraints(args, origin: context.origin, reply: reply)
        case "applyConstraints": reply(["error": tracks[trackId ?? ""] == nil ? "NotFoundError" : "NotSupportedError"])
        case "cameraStatus": reply(cameras.status)
        case "stopTrack" where cameras.owns(trackId): reply(cameras.stop(trackId!))
        case "setTrackEnabled" where cameras.owns(trackId): reply(cameras.setEnabled(trackId!, args["enabled"] as? Bool ?? true, origin: context.origin))
        case "mediaStreamTrack" where cameras.owns(trackId): reply(cameras.state(trackId!))
        case "stopTrack":
            guard let t = track(args) else { return reply(["error": "NotFoundError"]) }
            end(t, "stopTrack")
            reply(shape(t))
        case "setTrackEnabled":
            guard let t = track(args) else { return reply(["error": "NotFoundError"]) }
            t.enabled = args["enabled"] as? Bool ?? true
            // Turning a track back on is a start: it reopens an idle one that has consumers (D6).
            if t.enabled, !t.active, hasAppConsumer(t), context.origin == "app", context.foreground, t.readyState == "live" { activate(t) }
            announce()
            reply(shape(t))
        case "mediaStreamTrack":
            // A read of no track (`track=""`): an app reads before it holds one.
            if args["track"] as? String ?? "" == "" { return reply(["id": "", "kind": "audio", "label": "", "enabled": false, "muted": true, "readyState": "ended", "settings": [:]]) }
            reply(track(args).map(shape) ?? ["error": "NotFoundError"])
        case "enumerateDevices":
            let microphones = devices()["devices"] as? [[String: Any]] ?? []
            reply(["devices": microphones + cameras.devices])
        case "microphoneStatus": reply(status())
        default: reply(["error": "NotSupportedError"])
        }
    }

    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] {
        switch name {
        case "microphoneStatusSync": status()
        case "cameraStatusSync": cameras.status
        default: ["error": "NotSupportedError"]
        }
    }

    /// A stream with a camera's track, and the microphone's first where it asks for both (LLP 1133.008 D1).
    private func withCamera(_ args: [String: Any], _ reply: @escaping ([String: Any]) -> Void) {
        next += 1
        let stream = "stream-\(context.session)-\(next)"
        let video = { [self] (made: [[String: Any]]) in
            cameras.open(args, stream: stream) { [self] v in
                // What opened first is let go when the camera fails (LLP 1133.009 D3).
                if v["error"] != nil {
                    for m in made { if let id = m["id"] as? String, let t = tracks[id] { end(t, "the camera failed") } }
                    return reply(v)
                }
                let all = made + [v]
                context.tracks.add(ExactTrackEntry(id: stream, kind: "stream", tracks: all.compactMap { $0["id"] as? String }))
                reply(["id": stream, "active": true, "tracks": all])
            }
        }
        guard args["audio"] as? Bool == true || args["audioConstraints"] != nil else { return video([]) }
        getUserMedia(args) { a in
            guard a["error"] == nil else { return reply(a) }
            video(a["tracks"] as? [[String: Any]] ?? [])
        }
    }

    private func track(_ args: [String: Any]) -> Track? { (args["track"] as? String).flatMap { tracks[$0] } }
    private func announce() { context.changed("media-capture.mediaStreamTrack") }

    private func getUserMedia(_ args: [String: Any], _ reply: @escaping ([String: Any]) -> Void) {
        let constraints = args["audioConstraints"] as? [String: Any] ?? [:]
        // No kind asked for is Media Capture's TypeError, before anything opens.
        guard args["audio"] as? Bool == true || args["audioConstraints"] != nil else { return reply(["error": "TypeError"]) }
        let options = CaptureOptions(constraints: constraints, apple: args["apple"] as? [String: Any] ?? [:])
        let make = { [self] in
            next += 1
            let mute = TrackMute()
            let source: ExactAudioSource? = context.agent && process == nil ? AgentMicrophone(context, input: audioSource, origin: substituteOrigin, mute: mute) : audioSource
            let t = Track(id: "track-\(context.session)-\(next)", stream: "stream-\(context.session)-\(next)",
                          label: context.agent ? "Agent microphone" : Permission.inputName, options: options, source: source, mute: mute)
            t.settings = process.map { options.settings($0.format) } ?? Self.agentSettings
            if overlay != nil { t.platformMute = overlay }
            tracks[t.id] = t
            opened = true
            context.tracks.add(ExactTrackEntry(id: t.id, kind: "audio", owner: self))
            context.tracks.add(ExactTrackEntry(id: t.stream, kind: "stream", tracks: [t.id]))
            context.log("media-capture: \(t.stream) opened with \(t.id)")
            announce()
            context.changed("media-capture.enumerateDevices")
            reply(["id": t.stream, "active": true, "tracks": [shape(t)]])
        }
        if context.agent { return make() }
        guard Permission.inputAvailable else { return reply(["error": "NotFoundError"]) }
        Permission.ask { granted in
            DispatchQueue.main.async {
                guard granted else { return reply(["error": "NotAllowedError"]) }
                make()
            }
        }
    }

    // MARK: tracks and leases (D6)

    public func subscribe(_ id: String, _ consumer: ExactTrackConsumer, start: Bool, cause: ExactTrackCause) throws -> ExactSubscription {
        guard cause.current, let t = tracks[id] else { throw ExactTrackError("NotFoundError") }
        if t.readyState == "ended" {
            if consumer is ExactMovieAudioConsumer { throw ExactTrackError("NotReadableError") }
            consumer.ended(); return ExactSubscription {}
        }
        guard let movie = consumer as? ExactMovieAudioConsumer else {
            return subscribe(t, consumer, start: start, cause: cause)
        }
        // A live recording control can start audio alone while hidden (LLP 1126.004 D4).
        guard start, context.foreground, admitsCapture(cause, start: start) else { throw ExactTrackError("NotAllowedError") }
        guard !t.options.voiceProcessing, !context.audioSession.hasCall, !ProcessCapture.inCall else { throw ExactTrackError("NotSupportedError") }
        guard t.movieConsumer == nil else { throw ExactTrackError("NotReadableError") }
        let restore = try process?.reserveMovie(t.options, track: ObjectIdentifier(t), history: t.engineMute.history)
        t.engineMute.joinMovie()
        movie.useMicrophoneHistory(t.engineMute.history)
        let lease = subscribe(t, movie, start: true, cause: cause)
        do {
            if let process {
                guard t.leased else { throw ExactTrackError(t.activationError ?? "NotReadableError") }
                t.movieInput = try process.movieInput()
                t.movieSamples = process.movieSamples?.output
                t.settings = process.movieSettings
                process.movieReader = movie
                process.movieSamples?.record(movie)
                movie.cameraInterruption { [weak process] in process?.parkMovie($0, camera: true) }
                movie.prepareMicrophone {
                    [weak process] session in process?.beginMovie(in: session)
                }
            }
            movie.microphone(t.movieInput, samples: t.movieSamples, enabled: t.audible)
        } catch {
            lease.cancel()
            restore?.cancel()
            t.engineMute.leaveMovie()
            throw error
        }
        return ExactSubscription {
            t.movieInput = nil; t.movieSamples = nil; t.movieInBackground = false
            lease.cancel()
            if !t.active { self.release(t) }
            restore?.cancel()
            t.engineMute.leaveMovie()
        }
    }

    private func admitsCapture(_ cause: ExactTrackCause, start: Bool) -> Bool {
        (cause.origin == "app" && context.foreground) || (cause.origin == "press" && start)
    }

    private func subscribe(_ t: Track, _ consumer: ExactTrackConsumer, start: Bool, cause: ExactTrackCause) -> ExactSubscription {
        let audio = t.source.map { TrackAudioSource($0, cause: cause, track: ObjectIdentifier(t), engineMute: t.engineMute) }
        audio?.enabled = t.audible
        audio?.delivering = t.active && !t.muted && (process == nil || t.leased)
        t.consumers[ObjectIdentifier(consumer)]?.audio?.removeTaps()
        t.consumers[ObjectIdentifier(consumer)] = (consumer, cause, audio)
        // The app's own call starts the capture, and so does a live recording press the runner admitted (`press`).
        let person = admitsCapture(cause, start: start)
        if t.active, !(start && person && process != nil && !t.leased) {
            consumer.live(audio)
            if t.muted { consumer.muted(t.muteReason ?? "idle") }
        } else if person, t.enabled || consumer is ExactMovieAudioConsumer, !t.used || start {
            activate(t)
            // A retry can fail with the same mute reason; this new subscription must hear it too.
            if t.muted { consumer.muted(t.muteReason ?? "idle") }
        } else {
            consumer.muted(t.muteReason ?? "idle")
        }
        return ExactSubscription { [weak self, weak t, weak consumer] in
            guard let self, let t, let consumer, t.consumers[ObjectIdentifier(consumer)]?.cause === cause else { return }
            t.consumers.removeValue(forKey: ObjectIdentifier(consumer))?.audio?.removeTaps()
            if t.consumers.isEmpty, t.active { self.deactivate(t) }
        }
    }

    private func hasAppConsumer(_ track: Track) -> Bool {
        track.consumers.values.contains { $0.cause.origin == "app" && $0.cause.holdsCapture }
    }

    private func each(_ t: Track, _ f: (ExactTrackConsumer) -> Void) { for c in Array(t.consumers.values) { f(c.consumer) } }

    private func mute(_ t: Track, _ reason: String, consumersHear heard: String? = nil) {
        t.source?.advance(to: context.now())
        for reader in t.consumers.values { reader.audio?.delivering = false }
        guard !(t.muted && t.muteReason == reason) else { return }
        t.muted = true; t.muteReason = reason
        each(t) { $0.muted(heard ?? reason) }
        announce()
    }

    private func unmute(_ t: Track) {
        t.source?.advance(to: context.now())
        for reader in t.consumers.values { reader.audio?.delivering = t.active && (process == nil || t.leased) }
        guard t.muted else { return }
        t.muted = false; t.muteReason = nil
        each(t) { $0.unmuted() }
        announce()
    }

    /// The platform's mute over a running capture, or none.
    private func settle(_ t: Track) {
        if t.pausedForBackground { mute(t, "background") }
        else if let reason = t.platformMute { mute(t, reason) } else { unmute(t) }
    }

    /// Take the lease: the capture runs (D6). A capture that will not start
    /// leaves the track muted, `interrupted`, and says why.
    private func activate(_ t: Track) {
        t.active = true; t.used = true
        if let process, !t.leased {
            do {
                try process.lease(t.options, session: context.audioSession, log: context.log)
                t.leased = true; t.activationError = nil
                t.settings = process.movieSamples != nil ? process.movieSettings : t.options.settings(process.format)
                t.platformMute = nil
                context.changed("media-capture.microphoneStatus")
            } catch {
                t.activationError = (error as? ExactTrackError)?.code
                context.log("media-capture: \(t.id) did not open: \(error)")
                t.platformMute = "interrupted"
            }
        }
        for reader in Array(t.consumers.values) where reader.cause.current { reader.consumer.live(reader.audio) }
        settle(t)
    }

    /// Give the lease back: the track is idle.
    private func deactivate(_ t: Track) {
        t.active = false; t.pausedForBackground = false
        release(t)
        mute(t, "idle")
    }

    private func release(_ t: Track) {
        guard t.leased else { return }
        t.leased = false
        process?.unlease()
    }

    private func end(_ t: Track, _ why: String) {
        guard t.readyState == "live" else { return }
        t.source?.advance(to: context.now())
        t.readyState = "ended"; t.active = false
        for reader in t.consumers.values { reader.audio?.removeTaps() }
        if t.movieInput == nil { release(t) }
        context.log("media-capture: \(t.id) ended (\(why))")
        each(t) { $0.ended() }
        t.consumers.removeAll()
        announce()
    }

    // MARK: what the process's capture and the session tell (D7, D8)

    /// An event of the audio session or the engine, for every track this session runs.
    func heard(_ event: String) {
        for t in tracks.values where t.readyState == "live" {
            if ["interrupted", "route", "reopening", "reset"].contains(event) { t.engineMute.history.discontinuity() }
            switch event {
            case "reopening":
                if t.movieInput == nil && t.leased && (!context.foreground || !t.enabled || !hasAppConsumer(t)) { deactivate(t) }
                continue
            case "handoff-failed":
                release(t)
                t.platformMute = "interrupted"
            case "interrupted": t.platformMute = "interrupted"
            case "resumed", "reopened":
                if t.platformMute == "interrupted" {
                    t.platformMute = nil
                    if t.movieInput == nil && !t.pausedForBackground && (!context.foreground || !t.enabled || !hasAppConsumer(t)) { deactivate(t); continue }
                }
            case "reset":
                // A media-services reset: the capture is let go, and nothing records until a start (D7).
                t.platformMute = "interrupted"
                if t.active {
                    if t.muted && t.muteReason == "interrupted" {
                        for case let movie as ExactMovieAudioConsumer in t.consumers.values.map(\.consumer) { movie.muted("media-reset") }
                    } else { mute(t, "interrupted", consumersHear: "media-reset") }
                }
                t.active = false; t.pausedForBackground = false
                if t.movieInput == nil { release(t) }
                continue
            default: continue
            }
            if t.active { settle(t) }
        }
    }

    /// The app left the foreground or came back (D8): without
    /// `device.microphone-background` on iOS a capture's tracks mute,
    /// `background`, and its lease goes (LLP 1133.033 D3). Under the agent
    /// too, so a drive sees the pause; the lease is then the agent's.
    public func foreground(_ shown: Bool) {
        cameras.foreground(shown)
        if shown { context.changed("media-capture.microphoneStatus"); context.changed("media-capture.cameraStatus") }
        if shown {
            for t in tracks.values where t.readyState == "live" && !t.active && !t.used && t.enabled && hasAppConsumer(t) {
                activate(t)
            }
        }
        #if os(iOS)
        let keepsMicrophone = Permission.recordsInBackground
        #else
        let keepsMicrophone = true
        #endif
        if keepsMicrophone {
            for t in tracks.values where t.readyState == "live" && t.active && t.movieInBackground == shown {
                guard let movie = t.movieConsumer else { continue }
                t.movieInBackground = !shown
                if !shown { movie.muted("background") }
                else if t.platformMute == nil { movie.unmuted() }
                if t.movieInput != nil { process?.parkMovie(!shown) }
            }
            return
        }
        for t in tracks.values where t.readyState == "live" && t.active && (!shown || t.pausedForBackground) {
            t.pausedForBackground = !shown
            if shown {
                let interruption = t.movieConsumer != nil ? t.platformMute : nil
                t.platformMute = overlay
                if t.movieInput != nil, hasAppConsumer(t) { process?.parkMovie(false) }
                if (t.enabled || t.movieConsumer != nil), hasAppConsumer(t) {
                    activate(t)
                    if let interruption { t.platformMute = interruption; settle(t) }
                } else { deactivate(t) }
            } else {
                settle(t)
                if t.movieInput != nil { process?.parkMovie(true) }
                release(t)
            }
        }
    }

    /// Under the agent: a preferred `mediaStreamTrack` is the platform's change to every live track (D10 item 11).
    public func preferred(_ values: [String: Any]) {
        let value = values["mediaStreamTrack"] as? [String: Any]
        overlay = value?["muted"] as? Bool == true ? value?["muteReason"] as? String ?? "interrupted" : nil
        for t in tracks.values where t.readyState == "live" {
            t.platformMute = overlay
            if t.active { settle(t) }
        }
        cameras.preferred(value)
    }

    /// Handles a dropped reply carried: the stream's tracks end (D6).
    public func dispose(_ ids: [String]) {
        for id in ids {
            if let t = tracks[id] { end(t, "disposed") }
            for t in tracks.values where t.stream == id { end(t, "disposed") }
            cameras.dispose(id)
        }
    }

    public func destroy(_ done: @escaping () -> Void) {
        for t in tracks.values { end(t, "the session ended") }
        cameras.endAll("the session ended")
        process?.forget(self)
        done()
    }

    // MARK: answers

    private static let agentSettings: [String: Any] = ["deviceId": "agent-microphone", "groupId": "agent", "sampleRate": 48000, "sampleSize": 16,
                                                       "channelCount": 1, "echoCancellation": false, "autoGainControl": false, "noiseSuppression": false, "latency": 0.01]

    private func shape(_ t: Track) -> [String: Any] {
        var out: [String: Any] = ["id": t.id, "kind": "audio", "label": t.label, "enabled": t.enabled, "muted": t.muted,
                                  "readyState": t.readyState, "settings": t.settings]
        if t.muted, let reason = t.muteReason { out["muteReason"] = reason }
        if !context.agent, let port = Permission.inputPort { out["apple"] = port }
        return out
    }

    private func devices() -> [String: Any] {
        let granted = context.agent ? opened : Permission.state == "granted"
        guard granted else { return ["devices": [["deviceId": "", "kind": "audioinput", "label": "", "groupId": ""]]] }
        if context.agent { return ["devices": [["deviceId": "agent-microphone", "kind": "audioinput", "label": "Agent microphone", "groupId": "agent"]]] }
        return ["devices": Permission.inputs]
    }

    private func status() -> [String: Any] {
        ["available": Permission.inputAvailable, "state": Permission.state, "apple": ["recordPermission": Permission.word]]
    }
}

/// What a lease configures (D2, D4): voice processing, the session's mode and options.
struct CaptureOptions {
    let voiceProcessing: Bool, mode: String?, categoryOptions: [String], background: Bool
    /// The capture asked `echoCancellation=false`: no voice processing even during a call.
    let echoRefused: Bool
    let preferredIOBufferDuration: Double?, deviceId: String?
    /// iOS's words for the input (D4): no system alert interrupts, a data source (front, back, bottom) and its pattern.
    let prefersNoInterruptions: Bool, inputDataSource: String?, polarPattern: String?
    init(constraints: [String: Any], apple: [String: Any]) {
        voiceProcessing = ["echoCancellation", "noiseSuppression", "autoGainControl"].contains { constraints[$0] as? Bool == true }
        echoRefused = constraints["echoCancellation"] as? Bool == false
        mode = apple["mode"] as? String
        categoryOptions = apple["categoryOptions"] as? [String] ?? []
        background = Permission.recordsInBackground
        preferredIOBufferDuration = (apple["preferredIOBufferDuration"] as? Double) ?? (constraints["latency"] as? Double)
        deviceId = constraints["deviceId"] as? String
        prefersNoInterruptions = apple["prefersNoInterruptionsFromSystemAlerts"] as? Bool == true
        inputDataSource = apple["inputDataSource"] as? String
        polarPattern = apple["polarPattern"] as? String
    }
    /// What the track applied.
    func settings(_ format: AVAudioFormat) -> [String: Any] {
        ["deviceId": deviceId ?? "default", "groupId": "default", "sampleRate": format.sampleRate, "sampleSize": 32,
         "channelCount": Int(format.channelCount), "echoCancellation": voiceProcessing, "autoGainControl": voiceProcessing,
         "noiseSuppression": voiceProcessing]
    }
}

/// The microphone's permission and inputs, as the platform says them (D9).
enum Permission {
    static var state: String {
        #if os(iOS)
        switch AVAudioApplication.shared.recordPermission {
        case .granted: "granted"
        case .denied: "denied"
        default: "prompt"
        }
        #else
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: "granted"
        case .denied, .restricted: "denied"
        default: "prompt"
        }
        #endif
    }
    /// The platform's own word.
    static var word: String {
        #if os(iOS)
        switch AVAudioApplication.shared.recordPermission {
        case .granted: "granted"
        case .denied: "denied"
        default: "undetermined"
        }
        #else
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: "authorized"
        case .denied: "denied"
        case .restricted: "restricted"
        default: "notDetermined"
        }
        #endif
    }
    static func ask(_ done: @escaping (Bool) -> Void) {
        #if os(iOS)
        AVAudioApplication.requestRecordPermission(completionHandler: done)
        #else
        AVCaptureDevice.requestAccess(for: .audio, completionHandler: done)
        #endif
    }
    static var inputAvailable: Bool {
        #if os(iOS)
        AVAudioSession.sharedInstance().isInputAvailable
        #else
        AVCaptureDevice.default(for: .audio) != nil
        #endif
    }
    static var inputName: String {
        #if os(iOS)
        AVAudioSession.sharedInstance().currentRoute.inputs.first?.portName ?? "iPhone Microphone"
        #else
        AVCaptureDevice.default(for: .audio)?.localizedName ?? "Microphone"
        #endif
    }
    static var inputPort: [String: Any]? {
        #if os(iOS)
        AVAudioSession.sharedInstance().currentRoute.inputs.first.map { ["portType": $0.portType.rawValue, "portName": $0.portName] }
        #else
        AVCaptureDevice.default(for: .audio).map { ["portName": $0.localizedName] }
        #endif
    }
    static var inputs: [[String: Any]] {
        #if os(iOS)
        (AVAudioSession.sharedInstance().availableInputs ?? []).map { ["deviceId": $0.uid, "kind": "audioinput", "label": $0.portName, "groupId": $0.uid] }
        #else
        AVCaptureDevice.DiscoverySession(deviceTypes: [.microphone], mediaType: .audio, position: .unspecified).devices
            .map { ["deviceId": $0.uniqueID, "kind": "audioinput", "label": $0.localizedName, "groupId": $0.modelID] }
        #endif
    }
    /// `device.microphone-background`'s key (LLP 1133.033 D3): the capture
    /// records on in the background. Not the `audio` mode, which other lines
    /// and an app's own playback derive too.
    static var recordsInBackground: Bool {
        Bundle.main.object(forInfoDictionaryKey: "ExactCapturesInBackground") as? Bool == true
    }
}

/// One session-clock signal per substitute track. Every tap receives the same PCM,
/// including the final partial packet before an enabled change, pause or stop.
private final class AgentMicrophone: ExactAudioSource {
    private let defaultFormat = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 1)!
    private let context: ExactPackageContext
    private let input: ExactAudioSource?, origin: AVAudioFramePosition?, mute: TrackMute
    private var enabled = true
    private var upstream: ExactSubscription?
    private let lock = NSLock()
    var format: AVAudioFormat { input?.format ?? defaultFormat }
    private var nextFrame = 0, nextTap = 0
    private var maskedThrough = -Double.infinity
    private var edges: [(milliseconds: Double, enabled: Bool)] = []
    private var taps: [Int: (AVAudioPCMBuffer, AVAudioTime) -> Void] = [:]
    private var timer: (() -> Void)?
    init(_ context: ExactPackageContext, input: ExactAudioSource?, origin: AVAudioFramePosition?, mute: TrackMute) {
        self.context = context; self.input = input; self.origin = origin; self.mute = mute
    }

    private var stamp: CMTime { CMTime(seconds: context.now() / 1000, preferredTimescale: 1_000_000_000) }

    func setEnabled(_ value: Bool) {
        let now = context.now()
        if !value && mute.hasMovie {
            // Run the virtual capture callbacks already due. Disable before masking the
            // outstanding packet, or before an injected source can deliver any held PCM.
            generate(to: now) { self.mute.history.enabled(false, at: .init(self.stamp)) }
        } else {
            advance(to: now)
            mute.history.enabled(value, at: .init(stamp))
        }
        lock.withLock {
            enabled = value
            if !value || !mute.hasMovie { mark(now, value) }
        }
    }

    private func mark(_ milliseconds: Double, _ audible: Bool) {
        if audible != (edges.last?.enabled ?? true) { edges.append((milliseconds, audible)) }
    }

    func substituteLevel(at milliseconds: Double, standalone: Bool) -> Double? {
        guard input == nil else { return nil }
        if standalone && (mute.hasMovie || milliseconds < maskedThrough) { return nil }
        let frame = Int((milliseconds * 48).rounded(.down))
        let enabled = lock.withLock { edges.last(where: { $0.milliseconds <= milliseconds })?.enabled ?? true }
        return enabled ? exactMicrophoneLevel(frame / 2400) : 0
    }

    func tap(_ block: @escaping (AVAudioPCMBuffer, AVAudioTime) -> Void) -> ExactSubscription {
        advance(to: context.now())
        nextTap += 1
        let id = nextTap
        let first = lock.withLock { taps[id] = block; return taps.count == 1 }
        if first {
            if let input { upstream = input.tap { [weak self] pcm, time in self?.receive(pcm, time) } }
            else { schedule() }
        }
        return ExactSubscription { [weak self] in
            guard let self else { return }
            let last = self.lock.withLock { self.taps[id] = nil; return self.taps.isEmpty }
            if last {
                self.timer?(); self.timer = nil
                self.upstream?.cancel(); self.upstream = nil
            }
        }
    }

    private func receive(_ pcm: AVAudioPCMBuffer, _ time: AVAudioTime) {
        let (readers, enabled) = lock.withLock { (Array(taps.values), self.enabled) }
        let count = Int(pcm.frameLength), rate = Int32(pcm.format.sampleRate)
        let (offset, overflow) = time.sampleTime.subtractingReportingOverflow(origin ?? 0)
        let dated = time.isSampleTimeValid && time.sampleRate == pcm.format.sampleRate && !overflow
        let frame = dated ? offset : 0
        let sessionTime = AVAudioTime(sampleTime: frame, atRate: pcm.format.sampleRate)
        let cuts: [Range<Int>]
        if mute.hasMovie {
            if origin != nil && dated {
                cuts = mute.history.silence(from: CMTime(value: frame, timescale: rate),
                                            frames: count, rate: rate, through: .init(stamp))
            } else { cuts = [0..<count] }
        } else { cuts = enabled ? [] : [0..<count] }
        if let shared = ProcessCapture.masked(pcm, cuts) { for deliver in readers { deliver(shared, sessionTime) } }
    }

    private func schedule() {
        timer = context.after(100, coalescing: true) { [weak self] in
            guard let self, !self.taps.isEmpty else { return }
            self.advance(to: self.context.now())
            self.schedule()
        }
    }

    func advance(to milliseconds: Double) { generate(to: milliseconds) }

    private func generate(to milliseconds: Double, beforeLast edge: (() -> Void)? = nil) {
        var pending = edge
        defer { pending?() }
        if let input {
            pending?(); pending = nil
            input.advance(to: milliseconds)
            return
        }
        let end = Int((milliseconds * 48).rounded(.up))
        let provenEnd = Int((context.now() * 48).rounded(.down))
        if taps.isEmpty { nextFrame = end; return }
        while nextFrame < end {
            let first = nextFrame, count = min(480, end - first)
            if first + count == end { pending?(); pending = nil }
            guard let pcm = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(count)) else { return }
            pcm.frameLength = pcm.frameCapacity
            let data = pcm.floatChannelData![0]
            let through = CMTime(value: Int64(min(first + count, provenEnd)), timescale: 48000)
            let cuts = mute.hasMovie ? mute.history.silence(from: CMTime(value: Int64(first), timescale: 48000),
                frames: count, rate: 48000, through: .init(through)) : (enabled ? [] : [0..<count])
            if mute.hasMovie {
                maskedThrough = Double(first + count) / 48
                lock.withLock {
                    mark(Double(first) / 48, cuts.first?.lowerBound != 0)
                    for cut in cuts {
                        mark(Double(first + cut.lowerBound) / 48, false)
                        if cut.upperBound < count { mark(Double(first + cut.upperBound) / 48, true) }
                    }
                }
            }
            for i in 0..<count {
                let frame = first + i
                let tone = exactMicrophoneSample(at: Double(frame) / 48, level: exactMicrophoneLevel(frame / 2400))
                data[i] = cuts.contains(where: { $0.contains(i) }) ? 0 : Float(tone) / 32768
            }
            nextFrame += count
            let time = AVAudioTime(sampleTime: Int64(first), atRate: format.sampleRate)
            for deliver in taps.values { deliver(pcm, time) }
        }
    }
}

/// Keep the track's history across capture owners. A standalone engine tap
/// keeps its original enabled delivery until a movie needs timestamped handoff.
final class TrackMute {
    let history = ExactAudioMute()
    private let lock = NSLock()
    private var joined = false, active = false
    var hasMovie: Bool { lock.withLock { joined } }
    var inMovie: Bool { lock.withLock { active } }
    func joinMovie() { lock.withLock { joined = true; active = true } }
    func leaveMovie() {
        lock.withLock {
            active = false
            if history.proven { joined = false }
        }
    }
}

/// Media Capture's disabled audio is silence, with the source's frames and timestamps (LLP 1133.033 D8).
final class TrackAudioSource: ExactAudioSource {
    private let source: ExactAudioSource, cause: ExactTrackCause
    private let lock = NSRecursiveLock()
    private var closed = false
    private var isEnabled = true, isDelivering = false
    var delivering: Bool {
        get { lock.withLock { isDelivering } }
        set { lock.withLock { isDelivering = newValue } }
    }
    private var next = 0
    private var taps: [Int: ExactSubscription] = [:]
    var format: AVAudioFormat { source.format }
    func advance(to milliseconds: Double) { source.advance(to: milliseconds) }
    func substituteLevel(at milliseconds: Double, standalone: Bool) -> Double? { source.substituteLevel(at: milliseconds, standalone: standalone) }

    var enabled: Bool {
        get { lock.lock(); defer { lock.unlock() }; return isEnabled }
        set { lock.lock(); isEnabled = newValue; lock.unlock() }
    }

    private let track: ObjectIdentifier, engineMute: TrackMute

    init(_ source: ExactAudioSource, cause: ExactTrackCause, track: ObjectIdentifier, engineMute: TrackMute) {
        self.source = source; self.cause = cause; self.track = track; self.engineMute = engineMute
    }

    func tap(_ block: @escaping (AVAudioPCMBuffer, AVAudioTime) -> Void) -> ExactSubscription {
        lock.lock()
        guard !closed && cause.current else { lock.unlock(); return ExactSubscription {} }
        next += 1; let id = next; taps[id] = ExactSubscription {}; lock.unlock()
        // Only this tap's audio callback accesses the reusable buffer. The shared input is never changed.
        var silence = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 4096)!
        let receive: ProcessCapture.TrackTap = { [weak self] buffer, time, masked in
            guard let self else { return }
            let enabled = self.lock.withLock { self.isDelivering && self.taps[id] != nil ? (masked || self.isEnabled) : nil }
            guard let enabled else { return }
            if enabled { return self.cause.whileCurrent { block(buffer, time) } }
            if silence.format != buffer.format || silence.frameCapacity < buffer.frameLength {
                guard let replacement = AVAudioPCMBuffer(pcmFormat: buffer.format, frameCapacity: buffer.frameLength) else { return }
                silence = replacement
            }
            silence.frameLength = buffer.frameLength
            for audio in UnsafeMutableAudioBufferListPointer(silence.mutableAudioBufferList) {
                if let data = audio.mData { memset(data, 0, Int(audio.mDataByteSize)) }
            }
            self.cause.whileCurrent { block(silence, time) }
        }
        let upstream: ExactSubscription
        if let process = source as? ProcessCapture { upstream = process.tap(track, mute: engineMute, receive) }
        else { let masked = source is AgentMicrophone; upstream = source.tap { receive($0, $1, masked) } }
        lock.lock()
        guard taps[id] != nil else { lock.unlock(); upstream.cancel(); return ExactSubscription {} }
        taps[id] = upstream
        lock.unlock()
        return ExactSubscription { [weak self] in self?.removeTap(id) }
    }

    private func removeTap(_ id: Int) {
        lock.lock(); let upstream = taps.removeValue(forKey: id); lock.unlock()
        upstream?.cancel()
    }

    func removeTaps() {
        lock.lock(); closed = true; let upstream = Array(taps.values); taps.removeAll(); lock.unlock()
        for tap in upstream { tap.cancel() }
    }

    deinit { removeTaps() }
}

/// The process's microphone and audio-session claim (D3, D7).
final class ProcessCapture: ExactAudioSource {
    static let shared = ProcessCapture()
    /// Closing an owner waits for its last callback before the other input can attach.
    private final class EngineDelivery {
        private let lock = NSLock()
        private var open = true
        func send(_ work: () -> Void) { lock.withLock { if open { work() } } }
        func close() { lock.withLock { open = false } }
    }
    private var engineDelivery: EngineDelivery?
    private var engine: AVAudioEngine?
    typealias EngineTap = (AVAudioPCMBuffer, AVAudioTime) -> Void
    private let openEngine: ((CaptureOptions, @escaping EngineTap) throws -> ExactSubscription)?
    private var substituteEngine: ExactSubscription?
    private let substituteInput: (() -> AVCaptureDeviceInput)?
    init(movieInput: (() -> AVCaptureDeviceInput)? = nil, openEngine: ((CaptureOptions, @escaping EngineTap) throws -> ExactSubscription)? = nil) {
        substituteInput = movieInput; self.openEngine = openEngine
    }
    private var leases = 0
    private var claim: ExactSubscription?
    private var options: CaptureOptions?
    private var log: (String) -> Void = { _ in }
    private var observer: NSObjectProtocol?
    private var listeners: [ObjectIdentifier: () -> MediaCaptureModule?] = [:]
    /// The taps, read on the audio thread under the lock.
    private let lock = NSLock()
    typealias TrackTap = (AVAudioPCMBuffer, AVAudioTime, Bool) -> Void
    struct Reader {
        let track: ObjectIdentifier?
        let mute: TrackMute?
        let receive: TrackTap
    }
    private var taps: [Int: Reader] = [:]
    private var nextTap = 0
    private(set) var format = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 1)!

    func listen(_ m: MediaCaptureModule) { listeners[ObjectIdentifier(m)] = { [weak m] in m } }
    func forget(_ m: MediaCaptureModule) { listeners[ObjectIdentifier(m)] = nil }
    var modules: [MediaCaptureModule] { listeners.values.compactMap { $0() } }
    private func tell(_ event: String) { for get in Array(listeners.values) { get()?.heard(event) } }

    func tap(_ block: @escaping (AVAudioPCMBuffer, AVAudioTime) -> Void) -> ExactSubscription {
        tap(nil) { buffer, time, _ in block(buffer, time) }
    }

    func tap(_ track: ObjectIdentifier?, mute: TrackMute? = nil, _ block: @escaping TrackTap) -> ExactSubscription {
        lock.lock()
        nextTap += 1; let id = nextTap
        taps[id] = Reader(track: track, mute: mute, receive: block)
        movieSamples?.setTaps(Array(taps.values))
        lock.unlock()
        return ExactSubscription { [weak self] in
            guard let self else { return }
            self.lock.withLock {
                self.taps[id] = nil
                self.movieSamples?.setTaps(Array(self.taps.values))
            }
        }
    }

    /// Mask once per track. Camera callbacks carry their own PTS and clock.
    static func deliver(_ buffer: AVAudioPCMBuffer, _ time: AVAudioTime, to readers: [Reader],
                        captureTime: CMTime? = nil, session: AVCaptureSession? = nil) {
        var adjusted: [ObjectIdentifier: AVAudioPCMBuffer] = [:]
        for reader in readers {
            guard let track = reader.track, let mute = reader.mute, mute.hasMovie else { reader.receive(buffer, time, false); continue }
            if let shared = adjusted[track] { reader.receive(shared, time, true); continue }
            let frames = Int(buffer.frameLength), rate = Int32(buffer.format.sampleRate)
            let cuts: [Range<Int>]
            if let captureTime {
                cuts = mute.history.silence(from: captureTime, frames: frames, rate: rate,
                    through: .init(clock: session?.synchronizationClock))
            } else if mute.inMovie || !time.isHostTimeValid {
                cuts = mute.history.undated(frames)
            } else {
                // The standalone engine timestamps its own packets in host time.
                cuts = mute.history.silence(from: CMClockMakeHostTimeFromSystemUnits(time.hostTime),
                    frames: frames, rate: rate, through: .init(clock: CMClockGetHostTimeClock()))
            }
            guard let shared = masked(buffer, cuts) else { continue }
            adjusted[track] = shared
            reader.receive(shared, time, true)
        }
    }

    fileprivate static func masked(_ buffer: AVAudioPCMBuffer, _ cuts: [Range<Int>]) -> AVAudioPCMBuffer? {
        if cuts.isEmpty { return buffer }
        guard let pcm = AVAudioPCMBuffer(pcmFormat: buffer.format, frameCapacity: buffer.frameLength) else { return nil }
        pcm.frameLength = buffer.frameLength
        let stride = Int(buffer.format.streamDescription.pointee.mBytesPerFrame)
        for (dst, src) in zip(UnsafeMutableAudioBufferListPointer(pcm.mutableAudioBufferList),
                              UnsafeMutableAudioBufferListPointer(buffer.mutableAudioBufferList)) {
            guard let dest = dst.mData, let source = src.mData else { return nil }
            memcpy(dest, source, Int(src.mDataByteSize))
            for cut in cuts { memset(dest + cut.lowerBound * stride, 0, cut.count * stride) }
        }
        return pcm
    }

    /// A lease: the first claims the session and starts the engine with its options.
    func lease(_ options: CaptureOptions, session: ExactAudioSession, log: @escaping (String) -> Void) throws {
        if claim == nil {
            if leases > 0 { tell("reopening") }
            let options = leases > 0 ? self.options ?? options : options
            self.log = log
            guard let claim = session.claim(mode: options.mode, options: options.categoryOptions, background: options.background,
                                            events: { [weak self] in self?.tell($0) }) else { throw CaptureError("NotReadableError: the audio session would not activate") }
            self.claim = claim
            self.options = options
            do { try start(options) } catch { claim.cancel(); self.claim = nil; throw error }
            if leases > 0 { tell("reopened") }
        }
        leases += 1
    }

    func unlease() {
        leases = max(0, leases - 1)
        guard leases == 0 else { return }
        stop()
        if movieSamples == nil || parked { claim?.cancel(); claim = nil }
    }

    private var parked = false
    private var routeGeneration = 0

    /// When the camera backgrounds, remaining microphone leases return to the engine.
    private var background = false, cameraInterrupted = false
    func parkMovie(_ value: Bool, camera: Bool = false) {
        guard let movieReader else { return }
        let wasParked = background || cameraInterrupted
        if camera { cameraInterrupted = value } else { background = value }
        let park = background || cameraInterrupted
        guard wasParked != park else { return }
        routeGeneration += 1
        let generation = routeGeneration
        parked = false
        if !park { movieSamples?.takeEngine(relinquishEngine()) }
        movieReader.captureMicrophone(!park) { [self] in
            guard routeGeneration == generation, movieSamples != nil, park else { return }
            movieSamples?.park()
            parked = true
            restoreEngine()
        }
    }

    private func restoreEngine() {
        guard leases > 0 else { claim?.cancel(); claim = nil; return }
        guard substituteEngine == nil, engine == nil, claim != nil, let options else { return }
        do { try start(options) }
        catch {
            claim?.cancel(); claim = nil
            log("media-capture: the microphone did not reopen: \(error)")
            tell("handoff-failed")
        }
    }

    fileprivate weak var movieReader: ExactMovieAudioConsumer?
    private(set) var movieSamples: MovieSamples?
    private var movieDevice: AVCaptureDevice?
    private var lastEngineTime: AVAudioTime?

    func beginMovie(in session: AVCaptureSession) {
        movieSamples?.releaseEngine()
        movieSamples?.begin(session, anchor: lock.withLock { lastEngineTime })
    }

    /// Reserve the microphone for the movie. Its track holds the session claim through detachment.
    func reserveMovie(_ requested: CaptureOptions, track: ObjectIdentifier, history: ExactAudioMute) throws -> ExactSubscription {
        guard movieSamples == nil else { throw ExactTrackError("NotReadableError") }
        let effective = leases > 0 ? options ?? requested : requested
        guard !requested.voiceProcessing, !Self.inCall, !voiceProcessed(effective), engine?.inputNode.isVoiceProcessingEnabled != true else { throw ExactTrackError("NotSupportedError") }
        #if os(macOS)
        movieDevice = try engine.map { try Self.runningDevice($0.inputNode) }
        #endif
        let samples = MovieSamples(track, history: history)
        // Main reserves the engine. The graph releases it immediately before attaching the microphone.
        lock.withLock {
            movieSamples = samples
            samples.setTaps(Array(taps.values))
        }
        samples.takeEngine(relinquishEngine())
        return ExactSubscription { [self] in
            lock.withLock { movieSamples = nil }
            samples.releaseEngine()
            samples.finish()
            movieDevice = nil; movieReader = nil
            background = false; cameraInterrupted = false
            routeGeneration += 1
            parked = false
            restoreEngine()
        }
    }

    var movieSettings: [String: Any] {
        var settings = options?.settings(format) ?? [:]
        #if os(macOS)
        settings["deviceId"] = movieDevice?.uniqueID
        #else
        settings["deviceId"] = AVAudioSession.sharedInstance().currentRoute.inputs.first?.uid
        #endif
        return settings
    }

    func movieInput() throws -> AVCaptureDeviceInput {
        if let substituteInput { return substituteInput() }
        var device = movieDevice ?? AVCaptureDevice.default(for: .audio)
        #if os(macOS)
        if movieDevice == nil, let id = options?.deviceId, let selected = AVCaptureDevice(uniqueID: id), selected.hasMediaType(.audio) {
            device = selected
        }
        #endif
        guard let device else { throw ExactTrackError("NotFoundError") }
        let input = try AVCaptureDeviceInput(device: device)
        movieDevice = device
        #if os(macOS)
        let desc = input.ports.first { $0.mediaType == .audio }?.formatDescription ?? device.activeFormat.formatDescription
        let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(desc)
        let rate = asbd?.pointee.mSampleRate ?? 0, channels = asbd?.pointee.mChannelsPerFrame ?? 0
        #else
        let session = AVAudioSession.sharedInstance()
        let rate = session.sampleRate, channels = AVAudioChannelCount(session.inputNumberOfChannels)
        #endif
        if rate > 0, channels > 0, let actual = AVAudioFormat(standardFormatWithSampleRate: rate, channels: channels) { format = actual }
        return input
    }

    fileprivate static var inCall: Bool {
        #if os(iOS)
        let mode = AVAudioSession.sharedInstance().mode
        return mode == .voiceChat || mode == .videoChat
        #else
        return false
        #endif
    }

    private func voiceProcessed(_ options: CaptureOptions) -> Bool {
        options.voiceProcessing || (Self.inCall && !options.echoRefused)
    }

    private func start(_ options: CaptureOptions) throws {
        let processed = voiceProcessed(options)
        if movieSamples != nil, processed || Self.inCall { throw ExactTrackError("NotSupportedError") }
        if movieSamples != nil && !parked {
            #if os(iOS)
            configureInput(options, AVAudioSession.sharedInstance())
            #endif
            return
        }
        for reader in lock.withLock({ Array(taps.values) }) { reader.mute?.history.beginProducer() }
        let delivery = EngineDelivery()
        engineDelivery = delivery
        let deliver: EngineTap = { [weak self] buffer, time in
            guard let self else { return }
            delivery.send {
                let readers = self.lock.withLock { self.lastEngineTime = time; return Array(self.taps.values) }
                Self.deliver(buffer, time, to: readers)
            }
        }
        if let openEngine { substituteEngine = try openEngine(options, deliver); return }
        let engine = AVAudioEngine()
        let input = engine.inputNode
        if processed { try input.setVoiceProcessingEnabled(true) }
        #if os(iOS)
        configureInput(options, AVAudioSession.sharedInstance())
        #endif
        #if os(macOS)
        // A `deviceId` is ideal, not exact (Media Capture): one the Mac does not have leaves the default input.
        if let id = options.deviceId, let unit = input.audioUnit, var device = Self.device(uid: id) {
            let status = AudioUnitSetProperty(unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, 0, &device, UInt32(MemoryLayout<AudioDeviceID>.size))
            if status != noErr { log("media-capture: the input \(id) was not chosen (\(status))") }
        }
        #endif
        let format = input.outputFormat(forBus: 0)
        // No input: a tap on a 0 Hz format raises an exception that ends the app, so refuse first.
        guard format.sampleRate > 0, format.channelCount > 0 else { throw CaptureError("NotFoundError: no microphone is available") }
        self.format = format
        input.installTap(onBus: 0, bufferSize: 4096, format: format, block: deliver)
        if let hatch = MediaCaptureModule.hatch {
            let mixer = AVAudioMixerNode()
            engine.attach(mixer)
            engine.connect(input, to: mixer, format: format)
            // The engine renders what reaches its output: the mixer feeds the main mixer at no volume,
            // which keeps its own output whole for a tap and never plays the input (§13's monitoring).
            engine.connect(mixer, to: engine.mainMixerNode, format: format)
            mixer.volume = 0
            hatch(engine, mixer)
        }
        engine.prepare()
        try engine.start()
        self.engine = engine
        // A route or hardware change stops the engine; the handler only schedules the reopening (D7).
        observer = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil) { [weak self, weak engine] _ in
            DispatchQueue.main.async { [weak engine] in if let engine, self?.engine === engine { self?.reopen() } }
        }
    }

    #if os(iOS)
    private func configureInput(_ options: CaptureOptions, _ s: AVAudioSession) {
        if let d = options.preferredIOBufferDuration { try? s.setPreferredIOBufferDuration(d) }
        if let id = options.deviceId, let port = s.availableInputs?.first(where: { $0.uid == id }) { try? s.setPreferredInput(port) }
        if options.prefersNoInterruptions { try? s.setPrefersNoInterruptionsFromSystemAlerts(true) }
        if options.inputDataSource != nil || options.polarPattern != nil { Self.chooseDataSource(options, s, log) }
    }
    #endif

    #if os(iOS)
    /// The input's data source and polar pattern (D4), by the source's
    /// orientation or name: a built-in microphone's are front, back and bottom
    /// ([Selecting a microphone](https://developer.apple.com/documentation/avfaudio/avaudiosessiondatasourcedescription)).
    private static func chooseDataSource(_ options: CaptureOptions, _ s: AVAudioSession, _ log: (String) -> Void) {
        guard let port = s.preferredInput ?? s.currentRoute.inputs.first.flatMap({ i in s.availableInputs?.first { $0.uid == i.uid } }) else { return }
        let sources = port.dataSources ?? []
        let want = options.inputDataSource?.lowercased()
        guard let source = want.map({ w in sources.first { $0.orientation?.rawValue.lowercased() == w || $0.dataSourceName.lowercased() == w } })
                ?? (port.selectedDataSource ?? sources.first) else { return log("media-capture: \(port.portName) has no data source \(options.inputDataSource ?? "")") }
        let patterns: [String: AVAudioSession.PolarPattern] = ["stereo": .stereo, "cardioid": .cardioid, "omnidirectional": .omnidirectional]
        if let p = options.polarPattern.flatMap({ patterns[$0] }) {
            if source.supportedPolarPatterns?.contains(p) == true { try? source.setPreferredPolarPattern(p) }
            else { log("media-capture: \(source.dataSourceName) has no \(options.polarPattern ?? "") pattern") }
        }
        try? port.setPreferredDataSource(source)
        try? s.setPreferredInput(port)
    }
    #endif

    #if os(macOS)
    private static func runningDevice(_ node: AVAudioInputNode) throws -> AVCaptureDevice {
        var id = AudioDeviceID(kAudioObjectUnknown), size = UInt32(MemoryLayout<AudioDeviceID>.size)
        guard let unit = node.audioUnit, AudioUnitGetProperty(unit, kAudioOutputUnitProperty_CurrentDevice,
            kAudioUnitScope_Global, 0, &id, &size) == noErr else { throw ExactTrackError("NotReadableError") }
        var address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyDeviceUID, mScope: kAudioObjectPropertyScopeGlobal,
                                                 mElement: kAudioObjectPropertyElementMain)
        var uid: Unmanaged<CFString>?
        size = UInt32(MemoryLayout.size(ofValue: uid))
        guard AudioObjectGetPropertyData(id, &address, 0, nil, &size, &uid) == noErr, let uid,
              let device = AVCaptureDevice(uniqueID: uid.takeRetainedValue() as String) else { throw ExactTrackError("NotReadableError") }
        return device
    }

    /// The Core Audio device whose UID `AVCaptureDevice.uniqueID` is.
    private static func device(uid: String) -> AudioDeviceID? {
        var address = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyTranslateUIDToDevice,
                                                 mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var cf = uid as CFString, device = AudioDeviceID(kAudioObjectUnknown), size = UInt32(MemoryLayout<AudioDeviceID>.size)
        let status = withUnsafeMutablePointer(to: &cf) {
            AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, UInt32(MemoryLayout<CFString>.size), $0, &size, &device)
        }
        return status == noErr && device != kAudioObjectUnknown ? device : nil
    }
    #endif

    /// Keep the running tap until the graph is about to attach the input. The
    /// session queue takes the closure once. Cancellation also releases it.
    private func relinquishEngine() -> () -> Void {
        let held = engine, substitute = substituteEngine, notification = observer, delivery = engineDelivery
        engineDelivery = nil
        engine = nil; substituteEngine = nil; observer = nil
        return {
            delivery?.close()
            substitute?.cancel()
            if let notification { NotificationCenter.default.removeObserver(notification) }
            held?.inputNode.removeTap(onBus: 0)
            held?.stop()
        }
    }
    private func stop() {
        movieSamples?.releaseEngine()
        relinquishEngine()()
    }

    /// The engine stopped under a configuration change: another one, the gap journaled.
    func reopen() {
        tell("reopening")
        guard leases > 0, let options else { return }
        let began = Date()
        stop()
        do {
            try start(options)
            log("media-capture: capture reopened, \(Int(Date().timeIntervalSince(began) * 1000)) ms, \(Int(format.sampleRate)) Hz, \(format.channelCount) ch")
            tell("reopened")
        } catch {
            claim?.cancel()
            claim = nil
            log("media-capture: the capture did not reopen: \(error)")
            tell("interrupted")
        }
    }
}

/// While a movie owns the input, the same session supplies PCM to existing recorders and meters.
final class MovieSamples: NSObject, AVCaptureAudioDataOutputSampleBufferDelegate {
    let output = AVCaptureAudioDataOutput()
    private let queue = DispatchQueue(label: "exact.media-capture.movie-audio")
    private var taps: [ProcessCapture.Reader] = []
    private let track: ObjectIdentifier
    private var clock: CMClock?

    private let transfer = NSLock()
    private var engineRelease: (() -> Void)?
    func takeEngine(_ release: @escaping () -> Void) {
        transfer.withLock { engineRelease?(); engineRelease = release }
    }
    func releaseEngine() { transfer.withLock { engineRelease?(); engineRelease = nil } }

    private weak var recorder: ExactMovieAudioConsumer?
    func record(_ reader: ExactMovieAudioConsumer) { queue.sync { recorder = reader } }
    private var accepting = false
    func park() { queue.sync { accepting = false; history.discontinuity() } }

    func setTaps(_ readers: [ProcessCapture.Reader]) { queue.async { self.taps = readers } }

    private var converter: AVAudioConverter?
    private weak var session: AVCaptureSession?
    private var anchor: AVAudioTime?

    func begin(_ session: AVCaptureSession, anchor: AVAudioTime?) {
        queue.async {
            self.history.discontinuity()
            self.session = session; self.clock = session.synchronizationClock
            self.anchor = anchor; self.accepting = true
        }
    }

    private let history: ExactAudioMute

    init(_ track: ObjectIdentifier, history: ExactAudioMute) {
        self.history = history; self.track = track
        super.init()
        output.setSampleBufferDelegate(self, queue: queue)
    }

    func finish() {
        output.setSampleBufferDelegate(nil, queue: nil)
        queue.sync { accepting = false; taps.removeAll() }
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sample: CMSampleBuffer, from connection: AVCaptureConnection) {
        guard connection.isActive, output.connection(with: .audio) === connection else { return }
        receive(sample)
    }

    func receive(_ sample: CMSampleBuffer) {
        guard accepting else { return }
        let masked = recorder?.movieAudio(sample)
        guard !taps.isEmpty, let desc = sample.formatDescription else { return }
        if let current = session?.synchronizationClock { clock = current }
        guard let clock else { return }
        let format = AVAudioFormat(cmAudioFormatDescription: desc)
        let host = CMSyncConvertTime(sample.presentationTimeStamp, from: clock, to: CMClockGetHostTimeClock())
        guard host.isNumeric else { return }
        let ticks = CMClockConvertHostTimeToSystemUnits(host)
        let position = CMTimeConvertScale(sample.presentationTimeStamp, timescale: Int32(format.sampleRate), method: .default).value
        let time = anchor.flatMap { $0.sampleRate == format.sampleRate ? AVAudioTime(hostTime: ticks).extrapolateTime(fromAnchor: $0) : nil }
            ?? AVAudioTime(hostTime: ticks, sampleTime: position, atRate: format.sampleRate)
        let same = taps.filter { $0.track == track }, others = taps.filter { $0.track != track }
        let original = (!others.isEmpty || (!same.isEmpty && masked === sample)) ? pcm(sample, format) : nil
        if let original { ProcessCapture.deliver(original, time, to: others, captureTime: sample.presentationTimeStamp, session: session) }
        if !same.isEmpty, let masked, let shared = (masked === sample ? original : pcm(masked, format)) {
            for tap in same { tap.receive(shared, time, true) }
        }
    }

    private func pcm(_ sample: CMSampleBuffer, _ format: AVAudioFormat) -> AVAudioPCMBuffer? {
        let length = AVAudioFrameCount(sample.numSamples)
        guard length > 0, let pcm = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: length) else { return nil }
        pcm.frameLength = length
        guard CMSampleBufferCopyPCMDataIntoAudioBufferList(sample, at: 0, frameCount: Int32(length), into: pcm.mutableAudioBufferList) == noErr else { return nil }
        if format.commonFormat == .pcmFormatFloat32, !format.isInterleaved { return pcm }
        if converter?.inputFormat != format, let floats = AVAudioFormat(standardFormatWithSampleRate: format.sampleRate, channels: format.channelCount) {
            converter = AVAudioConverter(from: format, to: floats)
        }
        guard let converter, let converted = AVAudioPCMBuffer(pcmFormat: converter.outputFormat, frameCapacity: length),
              (try? converter.convert(to: converted, from: pcm)) != nil else { return nil }
        return converted
    }
}

struct CaptureError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}
