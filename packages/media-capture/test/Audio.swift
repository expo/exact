import AVFoundation
import ExactPackage
import Foundation

private var failures = 0
private func check(_ condition: Bool, _ name: String) {
    print("\(condition ? "PASS" : "FAIL") \(name)")
    if !condition { failures += 1 }
}

/// A microphone's callback thread, fed with a constant, nonzero signal.
private final class PCM: ExactAudioSource {
    var format = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 2)!
    let queue = DispatchQueue(label: "test.audio")
    var taps: [Int: (AVAudioPCMBuffer, AVAudioTime) -> Void] = [:]
    private var next = 0

    func tap(_ block: @escaping (AVAudioPCMBuffer, AVAudioTime) -> Void) -> ExactSubscription {
        next += 1
        let id = next
        queue.sync { taps[id] = block }
        return ExactSubscription { self.queue.sync { self.taps[id] = nil } }
    }

    func buffer(_ length: AVAudioFrameCount = 2400) -> AVAudioPCMBuffer {
        let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: length)!
        buffer.frameLength = length
        for audio in UnsafeMutableAudioBufferListPointer(buffer.mutableAudioBufferList) {
            if format.commonFormat == .pcmFormatFloat32 {
                let data = audio.mData!.assumingMemoryBound(to: Float.self)
                for i in 0..<Int(audio.mDataByteSize) / 4 { data[i] = 0.5 }
            } else {
                memset(audio.mData!, 0x40, Int(audio.mDataByteSize))
            }
        }
        return buffer
    }

    func send(_ frame: AVAudioFramePosition, length: AVAudioFrameCount = 2400) {
        let buffer = buffer(length)
        queue.sync {
            for tap in Array(taps.values) { tap(buffer, AVAudioTime(sampleTime: frame, atRate: format.sampleRate)) }
        }
    }
}

private final class Probe: ExactTrackConsumer {
    struct Received {
        let frames: AVAudioFrameCount, time: AVAudioFramePosition, format: AVAudioFormat, zero: Bool
    }
    var tap: ExactSubscription?
    var received: [Received] = []
    func live(_ source: ExactAudioSource?) {
        guard tap == nil else { return }
        tap = source?.tap { [weak self] buffer, time in
            let zero = UnsafeMutableAudioBufferListPointer(buffer.mutableAudioBufferList).allSatisfy {
                UnsafeRawBufferPointer(start: $0.mData, count: Int($0.mDataByteSize)).allSatisfy { $0 == 0 }
            }
            self?.received.append(Received(frames: buffer.frameLength, time: time.sampleTime, format: buffer.format, zero: zero))
        }
    }
    func muted(_ reason: String) {}
    func unmuted() {}
    func ended() {}
}

private final class FakeCaptureEngine: CaptureAudioEngine {
    let token = NSObject()
    var notificationObject: AnyObject? { token }
    var format: AVAudioFormat
    var hatchFormat: AVAudioFormat?
    var voiceProcessingEnabled: Bool = false
    var voiceProcessingHistory: [Bool] = []
    var startCount = 0
    var stopCount = 0
    var removeTapCount = 0
    var installTapFormats: [AVAudioFormat] = []
    var startError: Error?
    var onStart: ((FakeCaptureEngine) -> Void)?
    var tapBlock: ((AVAudioPCMBuffer, AVAudioTime) -> Void)?

    init(format: AVAudioFormat, hatchFormat: AVAudioFormat? = nil) {
        self.format = format
        self.hatchFormat = hatchFormat
    }

    func setVoiceProcessing(_ enabled: Bool) throws {
        voiceProcessingEnabled = enabled
        voiceProcessingHistory.append(enabled)
    }
    func inputFormat() -> AVAudioFormat { format }
    func installInputTap(format: AVAudioFormat, block: @escaping (AVAudioPCMBuffer, AVAudioTime) -> Void) {
        installTapFormats.append(format)
        tapBlock = block
    }
    func removeInputTap() {
        removeTapCount += 1
        tapBlock = nil
    }
    func start() throws {
        startCount += 1
        onStart?(self)
        if let error = startError { throw error }
    }
    func stop() {
        stopCount += 1
        tapBlock = nil
    }
    func emit(_ frames: AVAudioFrameCount = 512) {
        guard let tapBlock, let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames) else { return }
        buffer.frameLength = frames
        tapBlock(buffer, AVAudioTime(sampleTime: 0, atRate: format.sampleRate))
    }
}

private func runMain(until condition: () -> Bool, timeout: TimeInterval = 2.0) -> Bool {
    let deadline = Date().addingTimeInterval(timeout)
    while !condition(), Date() < deadline {
        _ = RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01))
    }
    return condition()
}

private func testProcessCaptureReopen() throws {
    let f48 = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 1)!
    let f44 = AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 2)!
    let fZero = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 0, channels: 0, interleaved: false)!
    var activeClaims = 0
    let session = ExactAudioSession { _, _, _, _ in
        activeClaims += 1
        return { activeClaims -= 1 }
    }
    var inCallNow = false
    var created: [FakeCaptureEngine] = []
    var nextEngine: (() throws -> FakeCaptureEngine)?
    let capture = ProcessCapture(makeEngine: { _ in
        let engine = try nextEngine!()
        created.append(engine)
        return engine
    }, isInCall: { inCallNow })
    let options = CaptureOptions(constraints: ["deviceId": "built-in-mic"], apple: [:])

    // 1. Configuration change posted during initial start() is observed and triggers one in-place restart.
    let first = FakeCaptureEngine(format: f48)
    var postedDuringStart = false
    first.onStart = { engine in
        if !postedDuringStart {
            postedDuringStart = true
            NotificationCenter.default.post(name: .AVAudioEngineConfigurationChange, object: engine.token)
        }
    }
    nextEngine = { first }
    var receivedFormats: [AVAudioFormat] = []
    let tap = capture.tap { buffer, _ in receivedFormats.append(buffer.format) }
    try capture.lease(options, session: session, log: { _ in })
    check(runMain(until: { first.startCount == 2 }), "configuration change posted inside start() is observed and restarts in place")
    check(created.count == 1 && first.stopCount == 0 && capture.format == f48, "startup config change keeps the same engine")

    // 2. Configuration change burst coalesces into one in-place restart on the new input format.
    first.format = f44
    NotificationCenter.default.post(name: .AVAudioEngineConfigurationChange, object: first.token)
    NotificationCenter.default.post(name: .AVAudioEngineConfigurationChange, object: first.token)
    check(runMain(until: { first.startCount == 3 }), "configuration change burst coalesces into one in-place restart")
    RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05))
    check(created.count == 1 && first.startCount == 3 && first.stopCount == 0, "reopen keeps the same engine without rebuilding or looping")
    check(first.removeTapCount == 2 && first.installTapFormats == [f48, f48, f44] && capture.format == f44, "reopen reinstalls tap on the new input format")
    first.emit()
    check(receivedFormats.last == f44, "restarted engine delivers audio at the new format")

    // 3. In-place restart reevaluates call voice-processing policy when entering and leaving a call.
    inCallNow = true
    NotificationCenter.default.post(name: .AVAudioEngineConfigurationChange, object: first.token)
    check(runMain(until: { first.startCount == 4 }), "entering a call restarts in place")
    check(first.voiceProcessingEnabled && first.voiceProcessingHistory == [true], "entering a call enables voice processing on restart")
    inCallNow = false
    NotificationCenter.default.post(name: .AVAudioEngineConfigurationChange, object: first.token)
    check(runMain(until: { first.startCount == 5 }), "leaving a call restarts in place")
    check(!first.voiceProcessingEnabled && first.voiceProcessingHistory == [true, false], "leaving a call disables voice processing on restart")

    // 4. Explicit echoCancellation=false keeps voice processing off even while in a call.
    let noEchoCapture = ProcessCapture(makeEngine: { _ in FakeCaptureEngine(format: f48) }, isInCall: { true })
    let noEchoEngine = FakeCaptureEngine(format: f48)
    let noEchoCapture2 = ProcessCapture(makeEngine: { _ in noEchoEngine }, isInCall: { true })
    let noEchoOptions = CaptureOptions(constraints: ["echoCancellation": false], apple: [:])
    try noEchoCapture2.lease(noEchoOptions, session: session, log: { _ in })
    noEchoCapture2.reopen()
    check(!noEchoEngine.voiceProcessingEnabled && noEchoEngine.voiceProcessingHistory.isEmpty, "echoCancellation=false keeps voice processing disabled during a call")
    noEchoCapture2.unlease()
    _ = noEchoCapture

    // 5. Synchronized interleaving: a late callback from retired engine A cannot suppress engine B's notification.
    let second = FakeCaptureEngine(format: f48, hatchFormat: f48)
    nextEngine = { second }
    let bgEntered = DispatchSemaphore(value: 0), bgRelease = DispatchSemaphore(value: 0), bgDone = DispatchSemaphore(value: 0)
    let probeObserver = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: first.token, queue: nil) { _ in
        bgEntered.signal()
        _ = bgRelease.wait(timeout: .now() + 5)
    }
    DispatchQueue.global().async {
        NotificationCenter.default.post(name: .AVAudioEngineConfigurationChange, object: first.token)
        bgDone.signal()
    }
    check(bgEntered.wait(timeout: .now() + 5) == .success, "background notification on engine A entered")
    // Force rebuild to engine B while A's notification delivery is still in flight on the background thread.
    first.format = fZero
    capture.reopen()
    check(created.count == 2 && first.stopCount == 1 && second.startCount == 1, "0 Hz input format rebuilds to engine B")
    // Release A's in-flight notification, then immediately notify B; B must still restart!
    bgRelease.signal()
    check(bgDone.wait(timeout: .now() + 5) == .success, "background notification on engine A finished")
    NotificationCenter.default.removeObserver(probeObserver)
    NotificationCenter.default.post(name: .AVAudioEngineConfigurationChange, object: second.token)
    check(runMain(until: { second.startCount == 2 }), "late callback from retired engine A does not suppress engine B's reopen")

    // 6. A connected hatch mixer whose format no longer matches the input falls back to rebuilding.
    let third = FakeCaptureEngine(format: f44)
    nextEngine = { third }
    second.format = f44
    NotificationCenter.default.post(name: .AVAudioEngineConfigurationChange, object: second.token)
    check(runMain(until: { created.count == 3 }), "mismatched hatch mixer format triggers engine rebuild")
    check(second.stopCount == 1 && third.startCount == 1 && capture.format == f44, "hatch mismatch rebuilds cleanly")

    // 7. A throwing in-place start falls back to rebuilding a new engine.
    let fourth = FakeCaptureEngine(format: f48)
    nextEngine = { fourth }
    third.startError = CaptureError("start failed")
    NotificationCenter.default.post(name: .AVAudioEngineConfigurationChange, object: third.token)
    check(runMain(until: { created.count == 4 }), "throwing in-place restart falls back to rebuild")
    check(third.stopCount == 1 && fourth.startCount == 1 && capture.format == f48, "fallback rebuild recovers from throwing start")

    // 8. When both in-place restart and rebuild fail, the session claim is cancelled.
    fourth.startError = CaptureError("in-place failed")
    nextEngine = { throw CaptureError("rebuild failed") }
    NotificationCenter.default.post(name: .AVAudioEngineConfigurationChange, object: fourth.token)
    check(runMain(until: { activeClaims == 0 }), "failed rebuild cancels the audio session claim")

    tap.cancel()
    capture.unlease()
}

@main
private struct AudioTests {
    static func main() throws {
        let directory = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
        let tracks = ExactTracks(), pcm = PCM()
        var now = 0.0
        let captureContext = ExactPackageContext(agent: true, session: "audio", changed: { _ in }, log: { _ in }, now: { now }, tracks: tracks)
        let capture = MediaCaptureModule(context: captureContext, audioSource: pcm)
        let recordingContext = ExactPackageContext(agent: false, session: "audio", changed: { _ in }, log: { _ in }, now: { now }, tracks: tracks,
                                                  file: { directory.appendingPathComponent(String($0.split(separator: "/").last!)) })
        let recording = RecordingModule(context: recordingContext)
        defer { capture.destroy {} }

        func call(_ module: ExactPackage, _ name: String, _ args: [String: Any]) -> [String: Any] {
            var reply: [String: Any]?
            module.later(name, args, preferred: nil) { reply = $0 }
            return reply!
        }
        func open() -> (String, String) {
            let stream = call(capture, "getUserMedia", ["audio": true])
            return (stream["id"] as! String, (stream["tracks"] as! [[String: Any]])[0]["id"] as! String)
        }
        let (stream, track) = open(), (otherStream, otherTrack) = open()
        let first = call(recording, "startMediaRecorder", ["stream": stream, "path": "app:/data/first.wav", "mimeType": "audio/wav"])
        let second = call(recording, "startMediaRecorder", ["stream": otherStream, "path": "app:/data/second.wav", "mimeType": "audio/wav"])
        check(first["state"] as? String == "recording" && second["state"] as? String == "recording", "both native recorders start")
        let paused = call(recording, "pauseMediaRecorder", ["recorder": first["id"]!])
        let resumed = call(recording, "resumeMediaRecorder", ["recorder": first["id"]!])
        check(paused["state"] as? String == "paused" && resumed["state"] as? String == "recording", "a sole consumer resumes across its lease change")
        _ = call(recording, "audioLevel", ["track": track])
        _ = call(recording, "audioLevel", ["track": otherTrack])

        for (i, enabled) in [true, false, true].enumerated() {
            _ = call(capture, "setTrackEnabled", ["track": track, "enabled": enabled])
            pcm.send(AVAudioFramePosition(i * 2400))
            now += 50
            let level = call(recording, "audioLevel", ["track": track])
            let other = call(recording, "audioLevel", ["track": otherTrack])
            check(level["audioLevel"] as? Double == (enabled ? 0.5 : 0), "track level at step \(i) follows enabled")
            check(other["audioLevel"] as? Double == 0.5, "the other track stays audible at step \(i)")
            check(abs((level["totalSamplesDuration"] as! Double) - now / 1000) < 1e-9, "sample duration includes step \(i)")
            let energy = Double(i == 0 ? 1 : i) * 0.0125
            check(abs((level["totalAudioEnergy"] as! Double) - energy) < 1e-9, "silence adds no energy at step \(i)")
        }

        for (id, name) in [(first["id"] as! String, "first"), (second["id"] as! String, "second")] {
            var result: [String: Any]?
            recording.later("stopMediaRecorder", ["recorder": id], preferred: nil) { result = $0 }
            let deadline = Date().addingTimeInterval(5)
            while result == nil, Date() < deadline { _ = RunLoop.main.run(mode: .default, before: deadline) }
            check(result?["duration"] as? Double == 150, "\(name)'s duration includes silence")
            let file = try AVAudioFile(forReading: directory.appendingPathComponent("\(name).wav"))
            let data = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: AVAudioFrameCount(file.length))!
            try file.read(into: data)
            check(data.frameLength == 7200, "\(name) keeps all recorded frames")
            for channel in 0..<Int(file.processingFormat.channelCount) {
                for step in 0..<3 {
                    let samples = data.floatChannelData![channel]
                    let meanSquare = (step * 2400..<(step + 1) * 2400).reduce(0.0) { $0 + Double(samples[$1] * samples[$1]) } / 2400
                    let expected = name == "first" && step == 1 ? 0.0 : 0.25
                    check(abs(meanSquare - expected) < 1e-6, "\(name) channel \(channel), recorded level at step \(step)")
                }
            }
        }
        recording.release("audioLevel")
        check(pcm.queue.sync { pcm.taps.isEmpty }, "stopped recorders and meters detach their taps")

        let probe = Probe(), otherProbe = Probe()
        let lease = try tracks.subscribe(track, probe, start: true)
        let otherLease = try tracks.subscribe(otherTrack, otherProbe, start: true)
        _ = call(capture, "setTrackEnabled", ["track": track, "enabled": false])
        let late = Probe(), lateLease = try tracks.subscribe(track, late, start: false)
        pcm.send(7200)
        check(late.received.last?.zero == true, "a new consumer of a disabled track hears silence")
        late.tap?.cancel(); lateLease.cancel()

        pcm.format = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 44100, channels: 2, interleaved: true)!
        pcm.send(9600, length: 8192)
        let changed = probe.received.last!
        check(changed.zero && changed.frames == 8192 && changed.time == 9600 && changed.format == pcm.format,
              "silence preserves timestamps, larger frame counts and an interleaved format change")
        check(otherProbe.received.last?.zero == false, "silencing never changes the shared input")
        pcm.format = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 24000, channels: 2, interleaved: false)!
        pcm.send(17792, length: 512)
        check(probe.received.last?.zero == true && probe.received.last?.format == pcm.format, "every integer PCM channel is silent too")

        // Hold the input before it calls the track: a main-thread flag change must reach that callback.
        let ready = DispatchSemaphore(value: 0), proceed = DispatchSemaphore(value: 0), done = DispatchSemaphore(value: 0)
        let buffer = pcm.buffer()
        pcm.queue.async {
            ready.signal()
            if proceed.wait(timeout: .now() + 5) == .success {
                for tap in Array(pcm.taps.values) { tap(buffer, AVAudioTime(sampleTime: 18304, atRate: pcm.format.sampleRate)) }
            }
            done.signal()
        }
        check(ready.wait(timeout: .now() + 5) == .success, "the audio callback is ready")
        _ = call(capture, "setTrackEnabled", ["track": track, "enabled": true])
        proceed.signal()
        check(done.wait(timeout: .now() + 5) == .success, "the audio callback finishes")
        check(probe.received.last?.zero == false, "enabling is visible on the audio thread")

        let racing = DispatchSemaphore(value: 0)
        pcm.queue.async {
            racing.signal()
            for i in 0..<1000 {
                for tap in Array(pcm.taps.values) { tap(buffer, AVAudioTime(sampleTime: Int64(i * 2400), atRate: pcm.format.sampleRate)) }
            }
        }
        check(racing.wait(timeout: .now() + 5) == .success, "the concurrent audio drive starts")
        for i in 0..<1000 { _ = call(capture, "setTrackEnabled", ["track": track, "enabled": i % 2 == 0]) }
        pcm.queue.sync {}
        _ = call(capture, "setTrackEnabled", ["track": track, "enabled": false])
        pcm.send(20704)
        check(probe.received.last?.zero == true, "disabling takes effect after concurrent flag changes")

        let stale = pcm.queue.sync { Array(pcm.taps.values) }
        probe.tap?.cancel(); probe.tap?.cancel()
        let count = probe.received.count
        pcm.queue.sync { for tap in stale { tap(buffer, AVAudioTime(sampleTime: 20704, atRate: pcm.format.sampleRate)) } }
        check(probe.received.count == count, "a cancelled tap ignores an already captured callback")
        lease.cancel(); lease.cancel()
        check(pcm.queue.sync { pcm.taps.count == 1 }, "the other track's subscription survives teardown")
        _ = call(capture, "stopTrack", ["track": otherTrack])
        check(pcm.queue.sync { pcm.taps.isEmpty }, "ending a track removes even a retained consumer's tap")
        otherLease.cancel()

        let (_, lastTrack) = open(), last = Probe()
        let lastLease = try tracks.subscribe(lastTrack, last, start: true)
        lastLease.cancel()
        pcm.send(23104)
        check(last.received.isEmpty, "a retained tap receives no shared input after its track gives up the lease")
        check(pcm.queue.sync { pcm.taps.isEmpty }, "the last consumer detaches its tap and lease")
        let replacement = Probe()
        let replacementLease = try tracks.subscribe(lastTrack, replacement, start: true)
        pcm.send(23104)
        check(last.received.isEmpty, "the earlier subscription receives nothing when the track restarts")
        last.tap?.cancel()
        check(replacement.received.last?.zero == false, "an explicit start can reuse the track's source")
        capture.destroy {}
        check(pcm.queue.sync { pcm.taps.isEmpty }, "session destruction removes all audio subscriptions")
        replacementLease.cancel()
        try testProcessCaptureReopen()
        print("audio tracks: \(failures) failures")
        exit(failures == 0 ? 0 : 1)
    }
}
