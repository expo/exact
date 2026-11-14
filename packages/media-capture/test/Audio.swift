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
        last.tap?.cancel()
        lastLease.cancel()
        check(pcm.queue.sync { pcm.taps.isEmpty }, "the last consumer detaches its tap and lease")
        let replacement = Probe()
        let replacementLease = try tracks.subscribe(lastTrack, replacement, start: true)
        pcm.send(23104)
        check(replacement.received.last?.zero == false, "an explicit start can reuse the track's source")
        capture.destroy {}
        check(pcm.queue.sync { pcm.taps.isEmpty }, "session destruction removes all audio subscriptions")
        replacementLease.cancel()
        print("audio tracks: \(failures) failures")
        exit(failures == 0 ? 0 : 1)
    }
}
