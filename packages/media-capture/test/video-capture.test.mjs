import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = resolve(import.meta.dir, '../../..');

test.skipIf(process.platform !== 'darwin')('Apple VideoCapture serializes self.input and self.device across rapid start, switchTo, and stop', () => {
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/media-capture-video-capture-'));
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
import ObjectiveC

private final class FakeDevice: AVCaptureDevice {
    static var names: [ObjectIdentifier: String] = [:]
    static var ids: [ObjectIdentifier: String] = [:]
    override var localizedName: String { Self.names[ObjectIdentifier(self)] ?? "camera" }
    override var uniqueID: String { Self.ids[ObjectIdentifier(self)] ?? "cam" }
    static func make(id: String, name: String) -> AVCaptureDevice {
        let d = class_createInstance(FakeDevice.self, 0) as! AVCaptureDevice
        names[ObjectIdentifier(d)] = name
        ids[ObjectIdentifier(d)] = id
        return d
    }
}

private final class FakeInput: AVCaptureInput {
    static var labels: [ObjectIdentifier: String] = [:]
    static func make(_ label: String) -> AVCaptureInput {
        let input = class_createInstance(FakeInput.self, 0) as! AVCaptureInput
        labels[ObjectIdentifier(input)] = label
        return input
    }
}

/// Models single-camera iOS AVCaptureSession: refuses a second input while one is attached.
private final class SingleCameraSession: AVCaptureSession {
    var attachedInputs: [AVCaptureInput] = []
    var attachedOutputs: [AVCaptureOutput] = []
    override var inputs: [AVCaptureInput] { attachedInputs }
    override var outputs: [AVCaptureOutput] { attachedOutputs }
    override func canSetSessionPreset(_ preset: AVCaptureSession.Preset) -> Bool { true }
    override func beginConfiguration() {}
    override func commitConfiguration() {}
    override func stopRunning() {}
    override func canAddInput(_ input: AVCaptureInput) -> Bool { attachedInputs.isEmpty }
    override func addInput(_ input: AVCaptureInput) {
        precondition(attachedInputs.isEmpty, "session already holds an input: \(attachedInputs)")
        attachedInputs.append(input)
    }
    override func removeInput(_ input: AVCaptureInput) { attachedInputs.removeAll { $0 === input } }
    override func removeOutput(_ output: AVCaptureOutput) { attachedOutputs.removeAll { $0 === output } }
}

private func drain(_ capture: VideoCapture) {
    let done = DispatchSemaphore(value: 0)
    capture.run { _ in done.signal() }
    precondition(done.wait(timeout: .now() + 5) == .success, "capture queue timed out")
}

@main struct Check {
    static func main() {
        let cam1 = FakeDevice.make(id: "cam-1", name: "Back")
        let cam2 = FakeDevice.make(id: "cam-2", name: "Front")
        let cam3 = FakeDevice.make(id: "cam-3", name: "UltraWide")

        // 1. Rapid start() -> switchTo() -> switchTo() while the serial camera queue is held.
        do {
            let session = SingleCameraSession()
            let gate = DispatchSemaphore(value: 0)
            let capture = VideoCapture(device: cam1, session: session) { FakeInput.make($0.uniqueID) }
            capture.run { _ in _ = gate.wait(timeout: .now() + 5) }
            capture.start(cam1)
            capture.switchTo(cam2)
            capture.switchTo(cam3)
            // Concurrent read of capture.device from another queue verifies thread-safety under TSan.
            let readDone = DispatchSemaphore(value: 0)
            DispatchQueue.global().async {
                _ = capture.device.uniqueID
                readDone.signal()
            }
            gate.signal()
            precondition(readDone.wait(timeout: .now() + 5) == .success)
            drain(capture)
            precondition(session.attachedInputs.count == 1, "expected 1 input, got \(session.attachedInputs.count)")
            let attachedId = FakeInput.labels[ObjectIdentifier(session.attachedInputs[0])]
            precondition(attachedId == "cam-3", "expected cam-3, got \(String(describing: attachedId))")
            precondition(capture.device.uniqueID == "cam-3", "device mismatch")
            capture.stop()
            drain(capture)
            precondition(session.attachedInputs.isEmpty, "stop must remove input")
        }

        // 2. Rapid start() -> stop() before start()'s queue block executes leaves zero inputs.
        do {
            let session = SingleCameraSession()
            let gate = DispatchSemaphore(value: 0)
            let capture = VideoCapture(device: cam1, session: session) { FakeInput.make($0.uniqueID) }
            capture.run { _ in _ = gate.wait(timeout: .now() + 5) }
            capture.start(cam1)
            capture.stop()
            gate.signal()
            drain(capture)
            precondition(session.attachedInputs.isEmpty, "fast teardown leaked input: \(session.attachedInputs)")
            precondition(session.attachedOutputs.isEmpty, "fast teardown leaked output")
        }

        print("PASS Apple VideoCapture input serialization")
    }
}
`);
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage', '-emit-module',
      '-emit-module-path', join(out, 'ExactPackage.swiftmodule'), 'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    run('xcrun', ['swiftc', '-sanitize=thread', '-parse-as-library', '-swift-version', '5', '-I', out, '-L', out, '-lExactPackage',
      'packages/media-capture/apple/MediaCapture.swift', 'packages/media-capture/apple/VideoCapture.swift', source, '-o', join(out, 'check')]);
    const stdout = run(join(out, 'check'), []);
    expect(stdout).toContain('PASS Apple VideoCapture input serialization');
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 120_000);
