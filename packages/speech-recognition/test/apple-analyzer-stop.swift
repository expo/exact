import AVFoundation
import ExactPackage
import Foundation

private final class FakeTrackOwner: ExactTrackOwner {
    var activeLeases = 0
    func subscribe(_ id: String, _ consumer: ExactTrackConsumer, start: Bool, cause: ExactTrackCause) throws -> ExactSubscription {
        activeLeases += 1
        return ExactSubscription { [weak self] in
            self?.activeLeases -= 1
        }
    }
}

@main
private struct AnalyzerStopTest {
    static func pump(until condition: () -> Bool, timeout: TimeInterval = 5.0) -> Bool {
        let deadline = Date(timeIntervalSinceNow: timeout)
        while !condition(), Date() < deadline {
            RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01))
        }
        return condition()
    }

    static func main() {
        let tracks = ExactTracks()
        let owner = FakeTrackOwner()
        tracks.add(ExactTrackEntry(id: "audio-1", kind: "audio", owner: owner))

        let context = ExactPackageContext(
            agent: false, session: "analyzer-stop-test",
            changed: { _ in }, log: { _ in }, now: { 0 },
            tracks: tracks
        )
        let module = SpeechRecognitionModule(context: context)

        func startTrack() -> String {
            var started: [String: Any]?
            module.later("startSpeechRecognition", ["track": "audio-1"], preferred: nil) { started = $0 }
            guard let id = started?["id"] as? String, !id.isEmpty else {
                fail("startSpeechRecognition did not return id: \(String(describing: started))")
            }
            return id
        }

        // 1. Mid-flight stop (and a concurrent second stop) during AnalyzerEngine setup settles and releases the track lease.
        let r1 = startTrack()
        guard owner.activeLeases == 1 else { fail("expected active lease 1, got \(owner.activeLeases)") }
        var stopA: [String: Any]?
        var stopB: [String: Any]?
        module.later("stopSpeechRecognition", ["recognition": r1], preferred: nil) { stopA = $0 }
        module.later("stopSpeechRecognition", ["recognition": r1], preferred: nil) { stopB = $0 }
        guard pump(until: { stopA != nil && stopB != nil && owner.activeLeases == 0 }) else {
            fail("mid-flight stop hung or leaked lease: stopA=\(String(describing: stopA)) stopB=\(String(describing: stopB)) leases=\(owner.activeLeases)")
        }
        var state1: [String: Any]?
        module.later("speechRecognitionState", ["recognition": r1], preferred: nil) { state1 = $0 }
        guard state1?["state"] as? String == "ended" else {
            fail("expected ended state after stop, got \(String(describing: state1))")
        }

        // 2. Mid-flight abort during AnalyzerEngine setup settles with "aborted" and releases the track lease.
        let r2 = startTrack()
        guard owner.activeLeases == 1 else { fail("expected active lease 1 for r2, got \(owner.activeLeases)") }
        var aborted: [String: Any]?
        module.later("abortSpeechRecognition", ["recognition": r2], preferred: nil) { aborted = $0 }
        guard pump(until: { aborted != nil && owner.activeLeases == 0 }) else {
            fail("mid-flight abort hung or leaked lease: aborted=\(String(describing: aborted)) leases=\(owner.activeLeases)")
        }
        guard aborted?["error"] as? String == "aborted" else {
            fail("expected aborted error, got \(String(describing: aborted))")
        }

        // 3. destroy() during AnalyzerEngine setup settles and releases the track lease.
        _ = startTrack()
        guard owner.activeLeases == 1 else { fail("expected active lease 1 for r3, got \(owner.activeLeases)") }
        var destroyed = false
        module.destroy { destroyed = true }
        guard pump(until: { destroyed && owner.activeLeases == 0 }) else {
            fail("destroy hung or leaked lease: destroyed=\(destroyed) leases=\(owner.activeLeases)")
        }

        // Let any cancelled setup tasks drain cleanly without crashing.
        let settle = Date(timeIntervalSinceNow: 0.1)
        while Date() < settle { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01)) }
        guard owner.activeLeases == 0 else { fail("late task re-acquired lease: \(owner.activeLeases)") }

        // 4. AnalyzerEngine.conform follows format changes, drains stream boundaries, skips empty buffers,
        // and consumes small inputs even when the converter holds buffered output.
        if #available(macOS 26.0, iOS 26.0, *) {
            let target = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16000, channels: 1, interleaved: false)!
            let f48 = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 1)!
            let f44 = AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 2)!
            let f24 = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 24000, channels: 2, interleaved: true)!

            func makeBuffer(_ format: AVAudioFormat, frames: AVAudioFrameCount, value: Float) -> AVAudioPCMBuffer {
                let b = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: max(frames, 1))!
                b.frameLength = frames
                for audio in UnsafeMutableAudioBufferListPointer(b.mutableAudioBufferList) {
                    if let data = audio.mData {
                        let floats = data.assumingMemoryBound(to: Float.self)
                        for i in 0..<(Int(audio.mDataByteSize) / MemoryLayout<Float>.size) { floats[i] = value }
                    }
                }
                return b
            }
            func samples(_ buffers: [AVAudioPCMBuffer]) -> [Float] {
                buffers.flatMap { b -> [Float] in
                    guard let ptr = b.floatChannelData?[0] else { return [] }
                    return Array(UnsafeBufferPointer(start: ptr, count: Int(b.frameLength)))
                }
            }

            // Empty input does not corrupt a fresh converter.
            var converter = AVAudioConverter(from: f48, to: target)
            var emptyOut: [AVAudioPCMBuffer] = []
            AnalyzerEngine.conform(makeBuffer(f48, frames: 0, value: 0.5), to: target, using: &converter) { emptyOut.append($0) }
            guard emptyOut.isEmpty else { fail("empty buffer produced output") }

            // 48 kHz (+0.5) -> target passthrough (-0.5) -> 48 kHz (-0.25): entering passthrough drains the
            // old converter and clears it, so returning to 48 kHz never replays positive samples from before.
            var firstOut: [AVAudioPCMBuffer] = []
            AnalyzerEngine.conform(makeBuffer(f48, frames: 4800, value: 0.5), to: target, using: &converter) { firstOut.append($0) }
            let firstFrames = firstOut.reduce(0) { $0 + Int($1.frameLength) }
            guard firstFrames > 1000, firstOut.allSatisfy({ $0.format == target }) else {
                fail("initial 48 kHz conversion produced unexpected frame count \(firstFrames)")
            }

            // Tiny input after large input still gets consumed (converter does not drop it on buffered output).
            var tinyOut: [AVAudioPCMBuffer] = []
            AnalyzerEngine.conform(makeBuffer(f48, frames: 3, value: 0.5), to: target, using: &converter) { tinyOut.append($0) }
            guard !tinyOut.isEmpty else { fail("tiny buffer after buffered input produced nothing") }

            let passthrough = makeBuffer(target, frames: 160, value: -0.5)
            var passOut: [AVAudioPCMBuffer] = []
            AnalyzerEngine.conform(passthrough, to: target, using: &converter) { passOut.append($0) }
            guard converter == nil, passOut.last === passthrough else {
                fail("passthrough did not clear converter or emit passthrough buffer last")
            }

            var resumedOut: [AVAudioPCMBuffer] = []
            AnalyzerEngine.conform(makeBuffer(f48, frames: 4800, value: -0.25), to: target, using: &converter) { resumedOut.append($0) }
            let resumedSamples = samples(resumedOut)
            guard !resumedSamples.isEmpty, resumedSamples.allSatisfy({ $0 <= 0.01 }) else {
                fail("resumed 48 kHz converter replayed stale positive samples: max=\(resumedSamples.max() ?? 0)")
            }

            // Mid-stream format changes (44.1 kHz stereo, 24 kHz interleaved) drain the previous converter and update.
            for (label, format, frames, val) in [
                ("reopened 44.1kHz stereo", f44, AVAudioFrameCount(4410), Float(0.25)),
                ("reopened 24kHz interleaved", f24, AVAudioFrameCount(2400), Float(-0.25)),
            ] {
                var out: [AVAudioPCMBuffer] = []
                AnalyzerEngine.conform(makeBuffer(format, frames: frames, value: val), to: target, using: &converter) { out.append($0) }
                let totalFrames = out.reduce(0) { $0 + Int($1.frameLength) }
                guard totalFrames > 500, out.allSatisfy({ $0.format == target }), converter?.inputFormat == format else {
                    fail("failed format transition to \(label): frames=\(totalFrames)")
                }
            }
        }
        print("analyzer stop+abort ok")
    }

    private static func fail(_ message: String) -> Never {
        FileHandle.standardError.write(Data("\(message)\n".utf8))
        exit(1)
    }
}
