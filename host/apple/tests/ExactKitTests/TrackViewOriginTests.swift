import AVFoundation
import CameraImplementation
import ExactPackage
import RecordingImplementation
@testable import MediaCaptureImplementation
import XCTest

final class TrackViewOriginTests: XCTestCase {
    private final class Owner: ExactTrackOwner {
        var subscriptions: [String] = []
        var causes: [ExactTrackCause] = []
        var listeners: [ObjectIdentifier: String] = [:]
        func subscribe(_ id: String, _ consumer: ExactTrackConsumer, start: Bool, cause: ExactTrackCause) throws -> ExactSubscription {
            XCTAssertFalse(start)
            let key = ObjectIdentifier(consumer)
            subscriptions.append("\(id) \(cause.origin)")
            causes.append(cause)
            listeners[key] = "\(id) \(cause.origin)"
            return ExactSubscription { self.listeners[key] = nil }
        }
    }

    private final class Consumer: ExactTrackConsumer {
        func muted(_ reason: String) {}
        func unmuted() {}
        func ended() {}
    }

    func testAReplacementDrawsTheHeldReaderWhileItsPropIsProvisional() {
        let owner = Owner(), tracks = ExactTracks()
        tracks.add(ExactTrackEntry(id: "a", kind: "audio", owner: owner))
        var drawn: ExactTrackConsumer?
        func mount(_ value: String) -> ExactSubscription {
            tracks.present("wave", id: "node", prop: "track", value: value, kind: "audio",
                           make: { _ in Consumer() }, show: { drawn = $0 })
        }
        let first = mount("a")
        tracks.commitViewBindings(["wave": ["node": ["track": ExactViewBinding(value: "a", origin: "app")]]])?()
        let held = drawn
        XCTAssertNotNil(held)
        first.cancel()
        let next = mount("old")
        XCTAssertTrue(drawn === held)
        XCTAssertEqual(owner.subscriptions, ["a app"])
        tracks.commitViewBindings([:])?()
        XCTAssertNil(drawn)
        next.cancel()
    }

    func testEngineRestartBetweenCommittedWaveformHandoffAndNativeDelivery() throws {
        let ctx = ExactPackageContext(agent: true, session: "s", changed: { _ in }, log: { _ in }, now: { 0 },
                                      audioSession: ExactAudioSession { _, _, _, _ in {} })
        var starts = 0
        let engine = ProcessCapture { _ in starts += 1 }
        let capture = MediaCaptureModule(context: ctx, audioSource: engine, process: engine)
        let recording = RecordingModule(context: ctx)
        var stream: [String: Any] = [:]
        capture.later("getUserMedia", ["audio": true], preferred: nil) { stream = $0 }
        let track = try XCTUnwrap((stream["tracks"] as? [[String: Any]])?.first?["id"] as? String)
        let events = ExactPackageEvents(send: { _, _ in }, size: { _ in })
        let views = try ["first", "second"].map {
            try recording.view("audio-waveform", id: $0, props: ["track": track], events: events)
        }
        defer {
            ctx.tracks.endViewBindings()
            for view in views { view.destroy() }
            recording.destroy {}
            capture.destroy {}
        }
        let fact = ["track": ExactViewBinding(value: track, origin: "app")]
        ctx.tracks.commitViewBindings(["audio-waveform": ["first": fact]])?()
        let handoff = ctx.tracks.commitViewBindings(["audio-waveform": ["second": fact]])
        engine.reopen()
        handoff?()
        XCTAssertEqual(starts, 2)
        capture.later("mediaStreamTrack", ["track": track], preferred: nil) { XCTAssertEqual($0["muted"] as? Bool, false) }

        let hide = ctx.tracks.commitViewBindings([:])
        let reveal = ctx.tracks.commitViewBindings(["audio-waveform": ["first": fact]])
        engine.reopen()
        hide?()
        reveal?()
        XCTAssertEqual(starts, 2)
        capture.later("mediaStreamTrack", ["track": track], preferred: nil) { XCTAssertEqual($0["muted"] as? Bool, true) }
    }

    func testInterruptedCameraJoiningAFreshLeaseDoesNotKeepTheOldMute() {
        let ctx = ExactPackageContext(agent: true, session: "s", changed: { _ in }, log: { _ in }, now: { 0 })
        let cameras = VideoTracks(context: ctx, nextId: { 1 })
        let track = VideoTrack(id: "track", stream: "stream", label: "Agent camera")
        let reader = Consumer(), key = ObjectIdentifier(reader)
        track.consumers[key] = (reader, ExactTrackCause("app"))
        cameras.lease(track, origin: "app")
        XCTAssertFalse(track.muted)
        track.consumers[key] = (reader, ExactTrackCause("tool"))
        cameras.interrupted(track, "in-use")
        XCTAssertFalse(track.leased)
        track.consumers[key] = (reader, ExactTrackCause("app"))
        cameras.lease(track, origin: "app")
        XCTAssertTrue(track.leased)
        XCTAssertFalse(track.muted)
        cameras.release(track)
    }

    #if os(iOS)
    func testForegroundRecorderReclaimsTheMicrophoneAfterAnInterruptionWhileHidden() throws {
        final class Audio: ExactAudioSource {
            let format = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 1)!
            var receive: ((AVAudioPCMBuffer, AVAudioTime) -> Void)?
            func tap(_ block: @escaping (AVAudioPCMBuffer, AVAudioTime) -> Void) -> ExactSubscription {
                receive = block
                return ExactSubscription { self.receive = nil }
            }
        }
        final class Recorder: ExactTrackConsumer {
            var buffers = 0
            var tap: ExactSubscription?
            func live(_ source: ExactAudioSource?) {
                tap?.cancel()
                tap = source?.tap { _, _ in self.buffers += 1 }
            }
            func muted(_ reason: String) {}
            func unmuted() {}
            func ended() { tap?.cancel() }
        }
        let ctx = ExactPackageContext(agent: true, session: "rec", changed: { _ in }, log: { _ in }, now: { 0 },
                                      audioSession: ExactAudioSession { _, _, _, _ in {} })
        var starts = 0
        let audio = Audio(), process = ProcessCapture { _ in starts += 1 }
        let capture = MediaCaptureModule(context: ctx, audioSource: audio, process: process)
        var stream: [String: Any] = [:]
        capture.later("getUserMedia", ["audio": true], preferred: nil) { stream = $0 }
        let id = try XCTUnwrap((stream["tracks"] as? [[String: Any]])?.first?["id"] as? String)
        let recorder = Recorder()
        let sub = try ctx.tracks.subscribe(id, recorder, start: true)
        defer { sub.cancel(); capture.destroy {} }
        ctx.foreground = false
        capture.foreground(false)
        capture.heard("interrupted")
        ctx.foreground = true
        capture.foreground(true)
        capture.heard("resumed")
        let buffer = try XCTUnwrap(AVAudioPCMBuffer(pcmFormat: audio.format, frameCapacity: 1))
        buffer.frameLength = 1
        audio.receive?(buffer, AVAudioTime(sampleTime: 0, atRate: 48000))
        XCTAssertEqual(starts, 2)
        XCTAssertEqual(recorder.buffers, 1)
    }
    #endif

    func testCameraHandoffKeepsTheLeaseAndForegroundKeepsTheInterruption() throws {
        let ctx = ExactPackageContext(agent: true, session: "cam", changed: { _ in }, log: { _ in }, now: { 0 })
        let cameras = VideoTracks(context: ctx, nextId: { 1 })
        let owner = Owner(), track = VideoTrack(id: "cam", stream: "stream", label: "camera")
        ctx.tracks.add(ExactTrackEntry(id: track.id, kind: "video", owner: owner))
        let fact = ["srcObject": ExactViewBinding(value: track.id, origin: "app")]
        let held = ctx.tracks.present("camera", id: "first", prop: "srcObject", value: track.id, kind: "video",
                                      make: { _ in Consumer() }, show: { _ in })
        defer { held.cancel(); ctx.tracks.endViewBindings(); cameras.release(track) }
        ctx.tracks.commitViewBindings(["camera": ["first": fact]])?()
        let reader = Consumer()
        track.consumers[ObjectIdentifier(reader)] = (reader, try XCTUnwrap(owner.causes.first))
        cameras.lease(track, origin: "app")
        let deliver = ctx.tracks.commitViewBindings(["camera": ["second": fact]])
        cameras.interrupted(track, "in-use")
        XCTAssertTrue(track.leased)
        ctx.foreground = false
        cameras.foreground(track, false)
        ctx.foreground = true
        cameras.foreground(track, true)
        XCTAssertTrue(track.leased)
        XCTAssertEqual(track.muteReason, "in-use")
        deliver?()
    }

    func testSameCameraPropRetriesWhenItsTrackArrives() throws {
        let ctx = ExactPackageContext(agent: true, session: "cam", changed: { _ in }, log: { _ in }, now: { 0 })
        let camera = CameraModule(context: ctx), owner = Owner()
        let events = ExactPackageEvents(send: { _, _ in }, size: { _ in })
        let props = ["srcObject": "track"]
        let view = try camera.view("exact-camera", id: "v", props: props, events: events)
        defer { ctx.tracks.endViewBindings(); view.destroy(); camera.destroy {} }
        ctx.tracks.commitViewBindings(["exact-camera": ["v": ["srcObject": ExactViewBinding(value: "track", origin: "app")]]])?()
        XCTAssertTrue(owner.subscriptions.isEmpty)
        ctx.tracks.add(ExactTrackEntry(id: "track", kind: "video", owner: owner))
        try view.setProps(props)
        try view.setProps(props)
        XCTAssertEqual(owner.subscriptions, ["track app"])
    }

    func testAnotherSessionRestartingTheEngineRechecksSurvivingReaders() throws {
        enum Failure: Error { case interrupted }
        for revoke in ["view", "disable", "background"] {
            var fail = false, started = 0
            let engine = ProcessCapture { _ in
                if fail { throw Failure.interrupted }
                started += 1
            }
            func context(_ session: String) -> ExactPackageContext {
                ExactPackageContext(agent: true, session: session, changed: { _ in }, log: { _ in }, now: { 0 },
                                    audioSession: ExactAudioSession { _, _, _, _ in {} })
            }
            func track(_ module: MediaCaptureModule) throws -> String {
                var made: [String: Any] = [:]
                module.later("getUserMedia", ["audio": true], preferred: nil) { made = $0 }
                return try XCTUnwrap((made["tracks"] as? [[String: Any]])?.first?["id"] as? String)
            }
            let a = context("a"), b = context("b")
            let first = MediaCaptureModule(context: a, audioSource: engine, process: engine)
            let next = MediaCaptureModule(context: b, audioSource: engine, process: engine)
            let firstTrack = try track(first), nextTrack = try track(next)
            let recording = RecordingModule(context: a)
            let view = try recording.view("audio-waveform", id: "v", props: ["track": firstTrack],
                                          events: ExactPackageEvents(send: { _, _ in }, size: { _ in }))
            a.tracks.commitViewBindings(["audio-waveform": ["v": ["track": ExactViewBinding(value: firstTrack, origin: "app")]]])?()
            let passive = try a.tracks.subscribe(firstTrack, Consumer(), start: false, origin: "tool")
            fail = true
            engine.reopen()
            XCTAssertEqual(started, 1)
            var release: (() -> Void)?
            switch revoke {
            case "view": release = a.tracks.commitViewBindings([:])
            case "disable": first.later("setTrackEnabled", ["track": firstTrack, "enabled": false], preferred: nil) { _ in }
            default: a.foreground = false
            }
            fail = false
            let active = try b.tracks.subscribe(nextTrack, Consumer(), start: false, origin: "app")
            XCTAssertEqual(started, 2)
            first.later("mediaStreamTrack", ["track": firstTrack], preferred: nil) {
                XCTAssertEqual($0["muteReason"] as? String, "idle", revoke)
            }
            next.later("mediaStreamTrack", ["track": nextTrack], preferred: nil) { XCTAssertEqual($0["muted"] as? Bool, false) }
            release?()
            a.tracks.endViewBindings()
            view.destroy()
            active.cancel(); passive.cancel()
            recording.destroy {}; first.destroy {}; next.destroy {}
        }
    }

    func testAnAudioReaderCannotSupplyTheOriginOfAStart() throws {
        let ctx = ExactPackageContext(agent: true, session: "s", changed: { _ in }, log: { _ in }, now: { 0 })
        let capture = MediaCaptureModule(context: ctx)
        func call(_ name: String, _ args: [String: Any]) -> [String: Any] {
            var answer: [String: Any] = [:]
            capture.later(name, args, preferred: nil) { answer = $0 }
            return answer
        }
        let stream = call("getUserMedia", ["audio": true])
        let track = try XCTUnwrap((stream["tracks"] as? [[String: Any]])?.first?["id"] as? String)
        let reader = Consumer(), app = Consumer()
        let restricted = try ctx.tracks.subscribe(track, reader, start: false, origin: "action")
        let enabled = call("setTrackEnabled", ["track": track, "enabled": true])
        XCTAssertEqual(enabled["muteReason"] as? String, "idle", String(describing: enabled))
        ctx.foreground = false
        let eligible = try ctx.tracks.subscribe(track, app, start: false, origin: "app")
        XCTAssertEqual(call("mediaStreamTrack", ["track": track])["muted"] as? Bool, true)
        ctx.foreground = true
        capture.foreground(true)
        XCTAssertEqual(call("mediaStreamTrack", ["track": track])["muted"] as? Bool, false)
        eligible.cancel()
        #if os(iOS)
        ctx.foreground = false
        capture.foreground(false)
        ctx.foreground = true
        capture.foreground(true)
        XCTAssertEqual(call("mediaStreamTrack", ["track": track])["muteReason"] as? String, "idle")
        #endif
        restricted.cancel()
        capture.destroy {}
    }

    func testARebuiltWaveformKeepsTheRunningMicrophoneUntilItsNodeHides() throws {
        let ctx = ExactPackageContext(agent: true, session: "s", changed: { _ in }, log: { _ in }, now: { 0 })
        let capture = MediaCaptureModule(context: ctx), recording = RecordingModule(context: ctx)
        var stream: [String: Any] = [:]
        capture.later("getUserMedia", ["audio": true], preferred: nil) { stream = $0 }
        let track = try XCTUnwrap((stream["tracks"] as? [[String: Any]])?.first?["id"] as? String)
        func muted() -> Bool? {
            var value: Bool?
            capture.later("mediaStreamTrack", ["track": track], preferred: nil) { value = $0["muted"] as? Bool }
            return value
        }
        let events = ExactPackageEvents(send: { _, _ in }, size: { _ in })
        let binding = ["track": ExactViewBinding(value: track, origin: "app")]
        let first = try recording.view("audio-waveform", id: "v", props: ["track": track], events: events)
        ctx.tracks.commitViewBindings(["audio-waveform": ["v": binding]])?()
        XCTAssertEqual(muted(), false)
        first.destroy()
        let replacement = try recording.view("audio-waveform", id: "v", props: ["track": "previous-layout"], events: events)
        XCTAssertEqual(muted(), false)
        try replacement.setProps(["track": track])
        ctx.tracks.commitViewBindings(["audio-waveform": ["v": binding]])?()
        XCTAssertEqual(muted(), false)
        let second = try recording.view("audio-waveform", id: "second", props: ["track": track], events: events)
        let next = ["audio-waveform": ["second": binding]]
        let swap = ctx.tracks.commitViewBindings(["audio-waveform": ["third": binding]])
        let swapAgain = ctx.tracks.commitViewBindings(next)
        swap?()
        swapAgain?()
        XCTAssertEqual(muted(), false, "consecutive handoffs keep a continuously held microphone")
        let ahead = ["audio-waveform": ["new": binding]]
        ctx.tracks.commitViewBindings(ahead)?()
        XCTAssertEqual(muted(), false, "the runner transfers a live track before constructing its presentation")
        let new = try recording.view("audio-waveform", id: "new", props: ["track": track], events: events)
        replacement.destroy()
        let hide = ctx.tracks.commitViewBindings([:])
        let reveal = ctx.tracks.commitViewBindings(next)
        reveal?()
        hide?()
        XCTAssertEqual(muted(), true, "a true gap releases the used microphone even when effects drain together")
        capture.later("setTrackEnabled", ["track": track, "enabled": true], preferred: nil) { _ in }
        XCTAssertEqual(muted(), false)
        let pending = ctx.tracks.commitViewBindings([:])
        let passive = try ctx.tracks.subscribe(track, Consumer(), start: false, origin: "tool")
        XCTAssertEqual(muted(), true, "a new reader sees the committed hide before its effects are scheduled")
        pending?()
        passive.cancel()
        new.destroy()
        second.destroy()
        ctx.tracks.endViewBindings()
        capture.destroy {}
        recording.destroy {}
    }

    func testCameraStartRechecksAdmissionOnItsDeviceQueue() {
        let starts = CameraStarts(), queue = DispatchQueue(label: "test.camera.start")
        let app = ExactTrackCause("app")
        var count = 0
        queue.suspend()
        starts.update([app], on: queue) { count += 1 }
        starts.update([], on: queue) { count += 1 }
        queue.resume()
        queue.sync { XCTAssertEqual(count, 0) }
        starts.update([app], on: queue) { count += 1 }
        starts.update([app], on: queue) { count += 1 }
        queue.sync { XCTAssertEqual(count, 1) }
    }

    func testOnlyAnAppConsumerMayReopenTheCamera() throws {
        var last: [String: Any] = [:]
        let ctx = ExactPackageContext(agent: true, session: "s", changed: { _ in }, log: { _ in }, now: { 0 },
                                      state: { last = $0 })
        let cameras = VideoTracks(context: ctx, nextId: { 1 })
        var opened: [String: Any] = [:]
        cameras.open([:], stream: "stream") { opened = $0 }
        let id = try XCTUnwrap(opened["id"] as? String)
        func leased() -> Bool? { (last["videoTracks"] as? [[String: Any]])?.first?["lease"] as? Bool }
        let reader = Consumer(), app = Consumer()
        let passive = try cameras.subscribe(id, reader, start: false, cause: ExactTrackCause("tool"))
        _ = cameras.setEnabled(id, true, origin: "app")
        XCTAssertEqual(leased(), false)
        let active = try cameras.subscribe(id, app, start: false, cause: ExactTrackCause("app"))
        XCTAssertEqual(leased(), true)
        active.cancel()
        XCTAssertEqual(leased(), true, "the remaining reader holds a capture already running")
        var move: [String: Any] = [:]
        cameras.applyConstraints(["track": id, "facingMode": "user"], origin: "app") { move = $0 }
        XCTAssertEqual(move["error"] as? String, "NotAllowedError")
        ctx.foreground = false
        cameras.foreground(false)
        ctx.foreground = true
        cameras.foreground(true)
        #if os(iOS)
        XCTAssertEqual(leased(), false, "foreground recovery requires a current app consumer")
        #endif
        passive.cancel()
        XCTAssertEqual(leased(), false)
        ctx.foreground = false
        let pending = try cameras.subscribe(id, app, start: false, cause: ExactTrackCause("app"))
        XCTAssertEqual(leased(), false)
        ctx.foreground = true
        cameras.foreground(true)
        XCTAssertEqual(cameras.state(id)["muted"] as? Bool, false)
        pending.cancel()

        let waiting = VideoTrack(id: "waiting", stream: "s", label: "test")
        waiting.consumers[ObjectIdentifier(reader)] = (reader, ExactTrackCause("tool"))
        waiting.waiting = "app"
        cameras.retry(waiting)
        XCTAssertFalse(waiting.leased)
        waiting.consumers[ObjectIdentifier(app)] = (app, ExactTrackCause("app"))
        ctx.foreground = false
        waiting.waiting = "app"
        cameras.retry(waiting)
        XCTAssertFalse(waiting.leased)
        ctx.foreground = true
        waiting.waiting = "app"
        cameras.retry(waiting)
        XCTAssertTrue(waiting.leased)
        cameras.interrupted(waiting, "interrupted")
        waiting.consumers[ObjectIdentifier(app)] = nil
        cameras.interrupted(waiting, nil)
        XCTAssertFalse(waiting.leased)
        XCTAssertTrue(waiting.muted, "an interruption ending cannot unmute a revoked holder")
    }

    func testForegroundReadsTheBindingThatSurvivedTheBackground() throws {
        for camera in [true, false] {
            let ctx = ExactPackageContext(agent: true, session: "s", changed: { _ in }, log: { _ in }, now: { 0 })
            let capture = MediaCaptureModule(context: ctx)
            let module: ExactPackage = camera ? CameraModule(context: ctx) : RecordingModule(context: ctx)
            let tag = camera ? "exact-camera" : "audio-waveform", prop = camera ? "srcObject" : "track"
            var stream: [String: Any] = [:]
            capture.later("getUserMedia", [camera ? "video" : "audio": true], preferred: nil) { stream = $0 }
            let id = try XCTUnwrap((stream["tracks"] as? [[String: Any]])?.first?["id"] as? String)
            func bind(_ origin: String) {
                ctx.tracks.commitViewBindings([tag: ["v": [prop: ExactViewBinding(value: id, origin: origin)]]])?()
            }
            func muted() -> Bool? {
                var value: Bool?
                capture.later("mediaStreamTrack", ["track": id], preferred: nil) { value = $0["muted"] as? Bool }
                return value
            }
            ctx.foreground = false
            capture.foreground(false)
            bind("app")
            let view = try module.view(tag, id: "v", props: [prop: id], events: ExactPackageEvents(send: { _, _ in }, size: { _ in }))
            let retired = ctx.tracks.commitViewBindings([tag: ["v": [prop: ExactViewBinding(value: id, origin: "tool")]]])
            ctx.foreground = true
            capture.foreground(true)
            XCTAssertEqual(muted(), true, "the device checks the revocation even before the package applies it")
            retired?()
            bind("app")
            XCTAssertEqual(muted(), false, tag)
            let pending = ctx.tracks.commitViewBindings([:])
            ctx.tracks.endViewBindings()
            pending?()
            bind("app")
            try view.setProps([prop: id])
            XCTAssertEqual(muted(), true, tag)
            let late = try module.view(tag, id: "v", props: [prop: id], events: ExactPackageEvents(send: { _, _ in }, size: { _ in }))
            XCTAssertEqual(muted(), true, "a late mount cannot consume after session teardown")
            late.destroy()
            view.destroy()
            module.destroy {}
            capture.destroy {}
        }
    }

    func testViewfinderAndWaveformUseBothCommits() throws {
        for camera in [true, false] {
            let owner = Owner(), tracks = ExactTracks()
            for id in ["a", "b"] {
                tracks.add(ExactTrackEntry(id: id, kind: camera ? "video" : "audio", owner: owner))
            }
            let context = ExactPackageContext(agent: true, session: "s", changed: { _ in }, log: { _ in }, now: { 0 }, tracks: tracks)
            let module: ExactPackage = camera ? CameraModule(context: context) : RecordingModule(context: context)
            let tag = camera ? "exact-camera" : "audio-waveform", prop = camera ? "srcObject" : "track"
            let events = ExactPackageEvents(send: { _, _ in }, size: { _ in })
            let view = try module.view(tag, id: "v", props: [prop: "a"], events: events)
            defer { module.destroy {} }
            func expect(_ subscription: String?, file: StaticString = #filePath, line: UInt = #line) {
                XCTAssertEqual(Array(owner.listeners.values), subscription.map { [$0] } ?? [], tag, file: file, line: line)
            }
            func report(_ value: String, _ origin: String) {
                tracks.commitViewBindings([tag: ["v": [prop: ExactViewBinding(value: value, origin: origin)]]])?()
            }
            expect(nil)
            report("a", "tool")
            expect("a tool")
            tracks.commitViewBindings([:])?()
            expect(nil)
            report("b", "tool")
            expect(nil)
            try view.setProps([prop: "b"])
            expect("b tool")
            view.destroy()
            expect("b tool")
            let rebuilt = try module.view(tag, id: "v", props: [prop: "a"], events: events)
            expect("b tool")
            try rebuilt.setProps([prop: "b"])
            view.destroy()
            expect("b tool")
            XCTAssertEqual(owner.subscriptions, ["a tool", "b tool"], tag)

            report("a", "app")
            expect(nil)
            try rebuilt.setProps([prop: "a"])
            expect("a app")
            report("a", "action")
            expect("a action")
            rebuilt.destroy()
            tracks.endViewBindings()
            report("a", "app")
            try rebuilt.setProps([prop: "a"])
            expect(nil)
            XCTAssertTrue(owner.causes.allSatisfy { !$0.current })
            XCTAssertEqual(owner.subscriptions, ["a tool", "b tool", "a app", "a action"], tag)
        }
    }
}
