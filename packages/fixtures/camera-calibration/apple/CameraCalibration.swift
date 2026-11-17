import AVFoundation
import ExactPackage
import Foundation
import simd

public final class CalibrationModule: ExactPackage {
    private let context: ExactPackageContext
    public init(context: ExactPackageContext) { self.context = context }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        guard name == "getCameraCalibration" else { return reply(["error": "NotSupportedError"]) }
        guard let track = args["track"] as? String, let source = context.tracks.lookup(track)?.video?() else {
            return reply(VideoCalibration.empty("track-not-running"))
        }
        let calibration = source.shared("fixture.camera-calibration") { VideoCalibration(source) }
        calibration.read(reply)
    }
}

// One output per capture, shared by concurrent reads and removed after the first frame.
final class VideoCalibration: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate {
    private weak var source: ExactVideoSource?
    private var deviceId = ""
    private let frames = DispatchQueue(label: "fixture.camera-calibration.frames")
    private var output: AVCaptureVideoDataOutput?
    private var waiting: [([String: Any]) -> Void] = []
    private var deadline: DispatchWorkItem?
    private let scheduleDeadline: (DispatchWorkItem) -> Void

    init(_ source: ExactVideoSource, scheduleDeadline: @escaping (DispatchWorkItem) -> Void = {
        DispatchQueue.main.asyncAfter(deadline: .now() + 2, execute: $0)
    }) {
        self.source = source
        self.scheduleDeadline = scheduleDeadline
    }

    static func empty(_ reason: String) -> [String: Any] {
        ["reason": reason, "deviceId": "", "referenceFrame": "", "intrinsicMatrix": [Double](),
         "distortion": [Double](), "focalLengths": [Double](), "sensorSize": [Double](),
         "pixelArraySize": [Double](), "preCorrectionActiveArray": [Double](), "referenceDimensions": [Double]()]
    }

    func read(_ reply: @escaping ([String: Any]) -> Void) {
        guard let source else { return reply(Self.empty("track-not-running")) }
        waiting.append(reply)
        if output != nil, deviceId == source.device.uniqueID { return }
        start(source)
    }

    private func start(_ source: ExactVideoSource) {
        detach()
        deviceId = source.device.uniqueID
        let output = AVCaptureVideoDataOutput()
        self.output = output
        output.alwaysDiscardsLateVideoFrames = true
        output.setSampleBufferDelegate(self, queue: frames)
        source.configure(stopped: true) { session in
            var reason: String?
            if session.canAddOutput(output) {
                session.addOutput(output)
                #if os(iOS)
                if let connection = output.connection(with: .video), connection.isCameraIntrinsicMatrixDeliverySupported {
                    connection.isCameraIntrinsicMatrixDeliveryEnabled = true
                } else { reason = "intrinsics-not-supported" }
                #else
                reason = "intrinsics-not-supported"
                #endif
            } else { reason = "output-unavailable" }
            if let reason { DispatchQueue.main.async { self.finish(output, Self.empty(reason)) } }
        }
        source.run { _ in
            DispatchQueue.main.async {
                guard self.output === output else { return }
                let timeout = DispatchWorkItem { self.finish(output, Self.empty("no-frame")) }
                self.deadline = timeout
                self.scheduleDeadline(timeout)
            }
        }
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sample: CMSampleBuffer, from connection: AVCaptureConnection) {
        let answer = Self.decode(sample)
        DispatchQueue.main.async { self.finish(output, answer) }
    }

    private func detach() {
        guard let output else { return }
        output.setSampleBufferDelegate(nil, queue: nil)
        self.output = nil
        deadline?.cancel(); deadline = nil
        source?.configure { session in
            if session.outputs.contains(where: { $0 === output }) { session.removeOutput(output) }
        }
    }

    private func finish(_ output: AVCaptureOutput, _ answer: [String: Any]) {
        guard output === self.output else { return }
        if let source, source.device.uniqueID != deviceId {
            start(source)
            return
        }
        detach()
        var answer = answer
        answer["deviceId"] = deviceId
        let replies = waiting
        waiting.removeAll()
        for reply in replies { reply(answer) }
    }

    static func decode(_ sample: CMSampleBuffer) -> [String: Any] {
        guard let data = CMGetAttachment(sample, key: kCMSampleBufferAttachmentKey_CameraIntrinsicMatrix,
                                         attachmentModeOut: nil) as? Data,
              data.count == MemoryLayout<matrix_float3x3>.size else { return empty("intrinsics-not-reported") }
        // SIMD columns have padding; copy the platform matrix before serializing its nine entries.
        var matrix = matrix_identity_float3x3
        _ = withUnsafeMutableBytes(of: &matrix) { data.copyBytes(to: $0) }
        var answer = empty("")
        answer["intrinsicMatrix"] = (0..<3).flatMap { row in (0..<3).map { column in Double(matrix[column][row]) } }
        answer["referenceFrame"] = "sample-buffer"
        if let description = CMSampleBufferGetFormatDescription(sample) {
            let dimensions = CMVideoFormatDescriptionGetDimensions(description)
            answer["referenceDimensions"] = [Int(dimensions.width), Int(dimensions.height)]
        }
        return answer
    }
}
