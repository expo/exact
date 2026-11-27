import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = resolve(import.meta.dir, '../../..');

test.skipIf(process.platform !== 'darwin')('Apple audio Recorder propagates open, write, recovery, and agent failures and recovers Opus into MP4', () => {
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/recording-audio-failure-'));
  const run = (cmd, args) => {
    const r = spawnSync(cmd, args, { cwd: root, encoding: 'utf8', timeout: 120_000 });
    expect(r.status, `${cmd}: ${r.stdout}\n${r.stderr}`).toBe(0);
    return r.stdout;
  };
  try {
    const source = join(out, 'Check.swift');
    writeFileSync(source, String.raw`import AVFoundation
import ExactPackage
import Foundation

private final class StubAudioSource: ExactAudioSource {
    let format: AVAudioFormat
    var handler: ((AVAudioPCMBuffer, AVAudioTime) -> Void)?
    init(_ format: AVAudioFormat) { self.format = format }
    func tap(_ block: @escaping (AVAudioPCMBuffer, AVAudioTime) -> Void) -> ExactSubscription {
        handler = block
        return ExactSubscription { self.handler = nil }
    }
    func push(frames: AVAudioFrameCount = 1024) {
        let buf = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames)!
        buf.frameLength = frames
        handler?(buf, AVAudioTime(sampleTime: 0, atRate: format.sampleRate))
    }
}

private final class StubOwner: ExactTrackOwner {
    let audio: StubAudioSource?
    init(_ audio: StubAudioSource?) { self.audio = audio }
    func subscribe(_ id: String, _ consumer: ExactTrackConsumer, start: Bool, cause: ExactTrackCause) throws -> ExactSubscription {
        consumer.live(audio)
        return ExactSubscription {}
    }
    func leased(_ id: String) -> Bool { true }
}

private func wait(_ ready: () -> Bool) {
    let limit = Date().addingTimeInterval(5)
    while !ready() && Date() < limit { _ = RunLoop.main.run(mode: .default, before: limit) }
    precondition(ready(), "timed out waiting for callback")
}

@main struct Check {
    static func main() throws {
        let dir = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
        let format = AVAudioFormat(standardFormatWithSampleRate: 48_000, channels: 1)!

        // 1. Open failure (Writer.init throws when parent path is a regular file).
        do {
            let fileAsDir = dir.appendingPathComponent("not-a-dir")
            try Data("x".utf8).write(to: fileAsDir)
            let blocked = fileAsDir.appendingPathComponent("open-fail.wav")
            let tracks = ExactTracks()
            let owner = StubOwner(StubAudioSource(format))
            tracks.add(ExactTrackEntry(id: "t1", kind: "audio", owner: owner))
            tracks.add(ExactTrackEntry(id: "s1", kind: "stream", tracks: ["t1"], owner: owner))
            let ctx = ExactPackageContext(agent: false, session: "open", changed: { _ in }, log: { _ in }, now: { 0 }, tracks: tracks, file: { _ in blocked })
            let mod = RecordingModule(context: ctx)
            var started: [String: Any]?
            mod.later("startMediaRecorder", ["stream": "s1", "path": "app:/data/open-fail.wav", "mimeType": "audio/wav"], preferred: nil) { started = $0 }
            let id = started!["id"] as! String
            var stopped: [String: Any]?
            mod.later("stopMediaRecorder", ["recorder": id], preferred: nil) { stopped = $0 }
            wait { stopped != nil }
            precondition(stopped?["error"] as? String == "UnknownError", "expected UnknownError on open failure, got \(String(describing: stopped))")
            var state: [String: Any]?
            mod.later("mediaRecorderState", ["recorder": id], preferred: nil) { state = $0 }
            precondition(state?["reason"] as? String == "error", "expected reason=error, got \(String(describing: state))")
            precondition(state?["error"] is String, "expected error message, got \(String(describing: state))")
            precondition(state?["result"] == nil, "result must stay nil on failure, got \(String(describing: state))")
            // A subsequent stopMediaRecorder returns the cached failure, not success.
            var stoppedAgain: [String: Any]?
            mod.later("stopMediaRecorder", ["recorder": id], preferred: nil) { stoppedAgain = $0 }
            wait { stoppedAgain != nil }
            precondition(stoppedAgain?["error"] as? String == "UnknownError", "cached stop must return error")
        }

        // 2. Agent write failure (parent path is a regular file).
        do {
            let blocked = dir.appendingPathComponent("not-a-dir").appendingPathComponent("agent-fail.wav")
            let tracks = ExactTracks()
            let owner = StubOwner(nil)
            tracks.add(ExactTrackEntry(id: "t2", kind: "audio", owner: owner))
            tracks.add(ExactTrackEntry(id: "s2", kind: "stream", tracks: ["t2"], owner: owner))
            let ctx = ExactPackageContext(agent: true, session: "agent", changed: { _ in }, log: { _ in }, now: { 100 }, tracks: tracks, file: { _ in blocked })
            let mod = RecordingModule(context: ctx)
            var started: [String: Any]?
            mod.later("startMediaRecorder", ["stream": "s2", "path": "app:/data/agent-fail.wav"], preferred: nil) { started = $0 }
            let id = started!["id"] as! String
            var stopped: [String: Any]?
            mod.later("stopMediaRecorder", ["recorder": id], preferred: nil) { stopped = $0 }
            wait { stopped != nil }
            precondition(stopped?["error"] as? String == "UnknownError", "expected UnknownError on agent write failure, got \(String(describing: stopped))")
            var state: [String: Any]?
            mod.later("mediaRecorderState", ["recorder": id], preferred: nil) { state = $0 }
            precondition(state?["reason"] as? String == "error" && state?["result"] == nil, "expected error state on agent write failure: \(String(describing: state))")
        }

        // 3. Opus recovery writes .recovered.mp4 and succeeds instead of failing with kAudioFileUnsupportedDataFormatError.
        do {
            let url = dir.appendingPathComponent("opus-take.m4a")
            let writer = try Writer(url: url, type: "audio/mp4", format: format, bitRate: 64_000, bitRateMode: nil, apple: ["formatID": "opus"])
            let buf = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 2400)!
            buf.frameLength = 2400
            if let ch = buf.floatChannelData?[0] {
                for i in 0..<Int(buf.frameLength) { ch[i] = Float(sin(Double(i) * 0.05)) * 0.25 }
            }
            writer.append(buf)
            try writer.recover()
            let attrs = try FileManager.default.attributesOfItem(atPath: url.path)
            let size = (attrs[.size] as? NSNumber)?.intValue ?? 0
            precondition(size > 0, "recovered Opus file must be non-empty")
        }

        // 4. Backpressure (>2 s behind) propagates as stopMediaRecorder error (reason="error", result=nil).
        do {
            let url = dir.appendingPathComponent("backpressure.m4a")
            let src = StubAudioSource(format)
            let tracks = ExactTracks()
            let owner = StubOwner(src)
            tracks.add(ExactTrackEntry(id: "t4", kind: "audio", owner: owner))
            tracks.add(ExactTrackEntry(id: "s4", kind: "stream", tracks: ["t4"], owner: owner))
            let ctx = ExactPackageContext(agent: false, session: "bp", changed: { _ in }, log: { _ in }, now: { 0 }, tracks: tracks, file: { _ in url })
            let mod = RecordingModule(context: ctx)
            var started: [String: Any]?
            mod.later("startMediaRecorder", ["stream": "s4", "path": "app:/data/backpressure.m4a", "mimeType": "audio/mp4"], preferred: nil) { started = $0 }
            let id = started!["id"] as! String
            let huge = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 120_000)!
            huge.frameLength = 120_000
            src.handler?(huge, AVAudioTime(sampleTime: 0, atRate: format.sampleRate))
            // Wait for the async backpressure dispatch on main to stop the recorder before calling stopMediaRecorder.
            wait {
                var s: [String: Any]?
                mod.later("mediaRecorderState", ["recorder": id], preferred: nil) { s = $0 }
                return s?["state"] as? String == "inactive"
            }
            var stopped: [String: Any]?
            mod.later("stopMediaRecorder", ["recorder": id], preferred: nil) { stopped = $0 }
            wait { stopped != nil }
            precondition(stopped?["error"] as? String == "UnknownError", "backpressure must answer UnknownError, got \(String(describing: stopped))")
            precondition((stopped?["message"] as? String)?.contains("fell behind") == true, "expected backpressure message, got \(String(describing: stopped))")
            var state: [String: Any]?
            mod.later("mediaRecorderState", ["recorder": id], preferred: nil) { state = $0 }
            precondition(state?["reason"] as? String == "error" && state?["result"] == nil, "backpressure must leave result nil, got \(String(describing: state))")
        }

        // 5. Writer write/recovery failure (where Recorder.error was nil until Writer.finish) sets reason="error" and fails stopMediaRecorder.
        do {
            let url = dir.appendingPathComponent("write-fail.m4a")
            let src = StubAudioSource(format)
            let tracks = ExactTracks()
            let owner = StubOwner(src)
            tracks.add(ExactTrackEntry(id: "t5", kind: "audio", owner: owner))
            tracks.add(ExactTrackEntry(id: "s5", kind: "stream", tracks: ["t5"], owner: owner))
            let ctx = ExactPackageContext(agent: false, session: "wf", changed: { _ in }, log: { _ in }, now: { 0 }, tracks: tracks, file: { _ in url })
            let mod = RecordingModule(context: ctx)
            var started: [String: Any]?
            mod.later("startMediaRecorder", ["stream": "s5", "path": "app:/data/write-fail.m4a", "mimeType": "audio/mp4"], preferred: nil) { started = $0 }
            let id = started!["id"] as! String
            // An unconvertible buffer format trips Writer.conformed() == nil -> both failure and backupFailure.
            let oddFormat = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 10_000_000, channels: 1, interleaved: false)!
            let oddBuf = AVAudioPCMBuffer(pcmFormat: oddFormat, frameCapacity: 64)!
            oddBuf.frameLength = 64
            src.handler?(oddBuf, AVAudioTime(sampleTime: 0, atRate: 48_000))
            var stopped: [String: Any]?
            mod.later("stopMediaRecorder", ["recorder": id], preferred: nil) { stopped = $0 }
            wait { stopped != nil }
            precondition(stopped?["error"] as? String == "UnknownError", "writer failure must answer UnknownError, got \(String(describing: stopped))")
            var state: [String: Any]?
            mod.later("mediaRecorderState", ["recorder": id], preferred: nil) { state = $0 }
            precondition(state?["reason"] as? String == "error" && state?["result"] == nil, "writer failure must set reason=error and result=nil, got \(String(describing: state))")
            precondition(state?["error"] is String, "writer failure must populate error message, got \(String(describing: state))")
        }

        print("PASS Apple audio recording failures and Opus recovery")
    }
}
`);
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage', '-emit-module',
      '-emit-module-path', join(out, 'ExactPackage.swiftmodule'), 'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    run('xcrun', ['swiftc', '-parse-as-library', '-swift-version', '5', '-I', out, '-L', out, '-lExactPackage',
      'packages/recording/apple/Recording.swift', 'packages/recording/apple/VideoRecorder.swift', source, '-o', join(out, 'check')]);
    const stdout = run(join(out, 'check'), [out]);
    expect(stdout).toContain('PASS Apple audio recording failures and Opus recovery');
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 120_000);
