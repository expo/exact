import AVFoundation
import ExactPackage
import Foundation
import simd

private final class Session: AVCaptureSession {
    var active = false
    var calls: [String] = []
    var acceptsOutput = false
    var added: [AVCaptureVideoDataOutput] = []
    private var attached: [AVCaptureOutput] = []
    var onAdd: ((AVCaptureVideoDataOutput) -> Void)?
    override var isRunning: Bool { active }
    override func startRunning() { calls.append("start"); active = true }
    override func stopRunning() { calls.append("stop"); active = false }
    override func beginConfiguration() { calls.append("begin") }
    override func commitConfiguration() { calls.append("commit") }
    override var outputs: [AVCaptureOutput] { attached }
    override func canAddOutput(_ output: AVCaptureOutput) -> Bool { acceptsOutput && attached.isEmpty }
    override func addOutput(_ output: AVCaptureOutput) {
        let output = output as! AVCaptureVideoDataOutput
        added.append(output)
        attached.append(output)
        onAdd?(output)
    }
    override func removeOutput(_ output: AVCaptureOutput) { attached.removeAll { $0 === output } }
}

private final class Device: AVCaptureDevice {
    static var identifier = "back"
    override var uniqueID: String { Self.identifier }
}

private class CalibrationSource: ExactVideoSource {
    let session: AVCaptureSession
    let device = class_createInstance(Device.self, 0) as! AVCaptureDevice
    private var objects: [String: AnyObject] = [:]
    init(_ session: Session = Session()) { self.session = session }
    func configure(stopped: Bool, _ change: @escaping (AVCaptureSession) -> Void) {
        run { VideoCapture.configure($0, stopped: stopped, change) }
    }
    func run(_ work: @escaping (AVCaptureSession) -> Void) {
        work(session)
    }
    func shared<T: AnyObject>(_ key: String, _ make: () -> T) -> T {
        if let made = objects[key] as? T { return made }
        let made = make(); objects[key] = made
        return made
    }
    func apply(_ constraints: [String: Any]) {}
    func focus(at point: CGPoint) {}
}

private final class RestartingSource: CalibrationSource {
    var stopped: Bool?
    var afterRestart: (() -> Void)?
    // A successful output connection needs camera hardware; this case tests the queue's completion.
    override func configure(stopped: Bool, _ change: @escaping (AVCaptureSession) -> Void) { self.stopped = stopped }
    override func run(_ work: @escaping (AVCaptureSession) -> Void) { afterRestart = { work(self.session) } }
}

@main
struct CalibrationConfigurationSuite {
    private static func drainMainQueue() -> Bool {
        var drained = false
        DispatchQueue.main.async { DispatchQueue.main.async { drained = true } }
        let limit = Date().addingTimeInterval(5)
        while !drained, Date() < limit { RunLoop.main.run(mode: .default, before: limit) }
        return drained
    }

    static func main() {
        var failures = 0
        for (running, stopped) in [(false, false), (true, false), (false, true), (true, true)] {
            let session = Session()
            session.active = running
            VideoCapture.configure(session, stopped: stopped) { session in
                if stopped && session.isRunning { failures += 1 }
                (session as! Session).calls.append("configure")
            }
            let paused = running && stopped
            let expected = (paused ? ["stop"] : []) + ["begin", "configure", "commit"] + (paused ? ["start"] : [])
            let ok = session.calls == expected && session.isRunning == running
            if !ok { failures += 1 }
            print("\(ok ? "PASS" : "FAIL") apple-calibration running=\(running), stopped=\(stopped): \(session.calls)")
        }
        var pixel: CVPixelBuffer?
        precondition(CVPixelBufferCreate(nil, 640, 480, kCVPixelFormatType_32BGRA, nil, &pixel) == kCVReturnSuccess)
        var description: CMVideoFormatDescription?
        precondition(CMVideoFormatDescriptionCreateForImageBuffer(allocator: nil, imageBuffer: pixel!,
                                                                 formatDescriptionOut: &description) == noErr)
        var timing = CMSampleTimingInfo(duration: .invalid, presentationTimeStamp: .zero, decodeTimeStamp: .invalid)
        var sample: CMSampleBuffer?
        precondition(CMSampleBufferCreateReadyWithImageBuffer(allocator: nil, imageBuffer: pixel!, formatDescription: description!,
                                                             sampleTiming: &timing, sampleBufferOut: &sample) == noErr)
        let missing = VideoCalibration.decode(sample!)
        var ok = missing["reason"] as? String == "intrinsics-not-reported" && (missing["intrinsicMatrix"] as? [Double])?.isEmpty == true
        if !ok { failures += 1 }
        print("\(ok ? "PASS" : "FAIL") apple-calibration a frame without intrinsics gives a reason")

        var matrix = matrix_float3x3(columns: (SIMD3(500, 0, 0), SIMD3(2, 510, 0), SIMD3(320, 240, 1)))
        let data = withUnsafeBytes(of: &matrix) { Data($0) }
        CMSetAttachment(sample!, key: kCMSampleBufferAttachmentKey_CameraIntrinsicMatrix,
                        value: data as CFData, attachmentMode: kCMAttachmentMode_ShouldNotPropagate)
        let answer = VideoCalibration.decode(sample!)
        ok = answer["reason"] as? String == "" && answer["referenceFrame"] as? String == "sample-buffer"
            && answer["intrinsicMatrix"] as? [Double] == [500, 2, 320, 0, 510, 240, 0, 0, 1]
            && answer["referenceDimensions"] as? [Int] == [640, 480]
            && (answer["distortion"] as? [Double])?.isEmpty == true
        if !ok { failures += 1 }
        print("\(ok ? "PASS" : "FAIL") apple-calibration a padded SIMD matrix becomes nine row-major values with reference dimensions")

        CMSetAttachment(sample!, key: kCMSampleBufferAttachmentKey_CameraIntrinsicMatrix,
                        value: Data(repeating: 0, count: 9 * MemoryLayout<Float>.size) as CFData,
                        attachmentMode: kCMAttachmentMode_ShouldNotPropagate)
        ok = VideoCalibration.decode(sample!)["reason"] as? String == "intrinsics-not-reported"
        if !ok { failures += 1 }
        print("\(ok ? "PASS" : "FAIL") apple-calibration a truncated matrix gives a reason")

        let source = CalibrationSource()
        let context = ExactPackageContext(agent: false, session: "calibration", changed: { _ in }, log: { _ in }, now: { 0 })
        let track = ExactTrackEntry(id: "video", kind: "video")
        track.video = { source }
        context.tracks.add(track)
        let module = CalibrationModule(context: context)
        for device in ["back", "front"] {
            Device.identifier = device
            var result: [String: Any]?
            module.later("getCameraCalibration", ["track": "video"], preferred: nil) { result = $0 }
            let limit = Date().addingTimeInterval(5)
            while result == nil, Date() < limit { RunLoop.main.run(mode: .default, before: limit) }
            ok = result?["reason"] as? String == "output-unavailable" && result?["deviceId"] as? String == device
            if !ok { failures += 1 }
            print("\(ok ? "PASS" : "FAIL") apple-calibration the shared reader answers for the current camera \(device)")
        }

        for refresh in [false, true] {
            CMSetAttachment(sample!, key: kCMSampleBufferAttachmentKey_CameraIntrinsicMatrix,
                            value: data as CFData, attachmentMode: kCMAttachmentMode_ShouldNotPropagate)
            let session = Session()
            session.acceptsOutput = true
            let source = CalibrationSource(session)
            let reader = VideoCalibration(source)
            var results: [[String: Any]] = []
            session.onAdd = { output in
                if Device.identifier == "front" {
                    var matrix = matrix_identity_float3x3
                    let data = withUnsafeBytes(of: &matrix) { Data($0) }
                    CMSetAttachment(sample!, key: kCMSampleBufferAttachmentKey_CameraIntrinsicMatrix,
                                    value: data as CFData, attachmentMode: kCMAttachmentMode_ShouldNotPropagate)
                }
                reader.captureOutput(output, didOutput: sample!, from: AVCaptureConnection(inputPorts: [], output: output))
            }
            Device.identifier = "back"
            reader.read { results.append($0) }
            let oldOutput = session.added[0]
            Device.identifier = "front"
            if refresh { reader.read { results.append($0) } }
            // This callback was already queued when the input changed.
            reader.captureOutput(oldOutput, didOutput: sample!, from: AVCaptureConnection(inputPorts: [], output: oldOutput))
            let drained = drainMainQueue()
            ok = drained && session.added.count == 2 && session.outputs.isEmpty && results.count == (refresh ? 2 : 1)
                && results.allSatisfy { $0["deviceId"] as? String == "front" && $0["reason"] as? String == ""
                    && $0["intrinsicMatrix"] as? [Double] == [1, 0, 0, 0, 1, 0, 0, 0, 1] }
            if !ok { failures += 1 }
            print("\(ok ? "PASS" : "FAIL") apple-calibration pending reads reject the old camera's frames, refresh=\(refresh): \(results)")
        }

        let restarting = RestartingSource()
        var timeouts: [DispatchWorkItem] = []
        let reader = VideoCalibration(restarting, scheduleDeadline: { timeouts.append($0) })
        var result: [String: Any]?
        reader.read { result = $0 }
        ok = drainMainQueue() && restarting.stopped == true && restarting.afterRestart != nil
            && timeouts.isEmpty && result == nil
        if !ok { failures += 1 }
        print("\(ok ? "PASS" : "FAIL") apple-calibration no frame deadline is armed while configuration and restart are pending")

        restarting.afterRestart?()
        restarting.afterRestart = nil
        ok = drainMainQueue() && timeouts.count == 1 && result == nil
        if !ok { failures += 1 }
        print("\(ok ? "PASS" : "FAIL") apple-calibration completing restart arms one frame deadline")

        timeouts.first?.perform()
        ok = result?["reason"] as? String == "no-frame" && result?["deviceId"] as? String == Device.identifier
        if !ok { failures += 1 }
        print("\(ok ? "PASS" : "FAIL") apple-calibration firing the controlled deadline answers no-frame")
        exit(failures == 0 ? 0 : 1)
    }
}
