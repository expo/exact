import AVFoundation
import ExactPackage
import Foundation

private final class Camera: ExactTrackOwner {
    var consumers = 0
    func subscribe(_ id: String, _ consumer: ExactTrackConsumer, start: Bool, cause: ExactTrackCause) throws -> ExactSubscription {
        consumers += 1
        consumer.live(video: nil)
        return ExactSubscription { self.consumers -= 1 }
    }
    func leased(_ id: String) -> Bool { consumers > 0 }
}

@available(iOS 17, macOS 14, *)
private final class MovieConnection: AVCaptureConnection {
    var rotation: CGFloat = 0
    var supported = true
    override func isVideoRotationAngleSupported(_ angle: CGFloat) -> Bool { supported }
    override var videoRotationAngle: CGFloat {
        get { rotation }
        set { rotation = newValue }
    }
}

// Replace only the capture hardware; the recorder still queues and controls its own movie output.
private var captureInterval = CMTime(value: 1, timescale: 30)
private final class TestDevice: AVCaptureDevice {
    override var activeVideoMinFrameDuration: CMTime {
        get { captureInterval }
        set { captureInterval = newValue }
    }
    override var uniqueID: String { "recording-test" }
    override var position: AVCaptureDevice.Position { .unspecified }
}

private final class TestSession: AVCaptureSession {
    var attached: [AVCaptureOutput] = []
    private var preset: AVCaptureSession.Preset = .photo
    override var sessionPreset: AVCaptureSession.Preset {
        get { preset }
        set { preset = newValue; captureInterval = CMTime(value: 1, timescale: 30) }
    }
    override func canSetSessionPreset(_ preset: AVCaptureSession.Preset) -> Bool { true }
    override func beginConfiguration() {}
    override func commitConfiguration() {}
    override func canAddOutput(_ output: AVCaptureOutput) -> Bool { true }
    override func addOutput(_ output: AVCaptureOutput) {
        precondition(output.connection(with: .video)?.isActive == true, "the test output needs an active connection")
        attached.append(output)
    }
    override func removeOutput(_ output: AVCaptureOutput) { attached.removeAll { $0 === output } }
    override var outputs: [AVCaptureOutput] { attached }
}

private final class TestSource: ExactVideoSource {
    let session: AVCaptureSession = TestSession()
    let device = class_createInstance(TestDevice.self, 0) as! AVCaptureDevice
    var work: [() -> Void] = []
    var rates: [Double] = []
    private var draining = false
    private var objects: [String: AnyObject] = [:]
    func configure(stopped: Bool, _ change: @escaping (AVCaptureSession) -> Void) { work.append { change(self.session) } }
    func run(_ body: @escaping (AVCaptureSession) -> Void) { work.append { body(self.session) } }
    func shared<T: AnyObject>(_ key: String, _ make: () -> T) -> T {
        if let object = objects[key] as? T { return object }
        let object = make(); objects[key] = object; return object
    }
    func apply(_ constraints: [String: Any]) {
        let apply = {
            if let rate = constraints["frameRate"] as? Double {
                self.rates.append(rate)
                captureInterval = CMTime(seconds: 1 / rate, preferredTimescale: 60000)
            }
        }
        if draining { apply() } else { work.append(apply) }
    }
    func focus(at point: CGPoint) {}
    @MainActor func drain() async {
        repeat {
            draining = true
            while !work.isEmpty { work.removeFirst()() }
            draining = false
            await withCheckedContinuation { done in DispatchQueue.main.async { done.resume() } }
        } while !work.isEmpty
    }
}

@available(iOS 18, macOS 14, *)
private final class ActiveConnection: AVCaptureConnection {
    override var isActive: Bool { true }
    override func isVideoRotationAngleSupported(_ angle: CGFloat) -> Bool { false }
}

@main struct VideoTest {
    static func rotatedMovie(_ url: URL, _ transform: CGAffineTransform) async throws {
        // After setup, only the writer's serial callback uses these objects.
        nonisolated(unsafe) let writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        nonisolated(unsafe) let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264, AVVideoWidthKey: 32, AVVideoHeightKey: 16,
        ])
        input.transform = transform
        nonisolated(unsafe) let pixels = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32ARGB,
            kCVPixelBufferWidthKey as String: 32, kCVPixelBufferHeightKey as String: 16,
        ])
        writer.add(input)
        precondition(writer.startWriting())
        writer.startSession(atSourceTime: .zero)
        try await withCheckedThrowingContinuation { (done: CheckedContinuation<Void, Error>) in
            var sent = false
            input.requestMediaDataWhenReady(on: DispatchQueue(label: "recording-test.frames")) {
                guard !sent, input.isReadyForMoreMediaData else { return }
                sent = true
                var buffer: CVPixelBuffer?
                precondition(CVPixelBufferPoolCreatePixelBuffer(nil, pixels.pixelBufferPool!, &buffer) == kCVReturnSuccess)
                CVPixelBufferLockBaseAddress(buffer!, [])
                memset(CVPixelBufferGetBaseAddress(buffer!), 0, CVPixelBufferGetDataSize(buffer!))
                CVPixelBufferUnlockBaseAddress(buffer!, [])
                precondition(pixels.append(buffer!, withPresentationTime: .zero))
                input.markAsFinished()
                writer.endSession(atSourceTime: CMTime(value: 1, timescale: 10))
                writer.finishWriting {
                    if let error = writer.error { done.resume(throwing: error) }
                    else { done.resume() }
                }
            }
        }
    }

    @MainActor static func nativeLifecycle(_ directory: URL, delayed: Bool) async {
        guard #available(iOS 18, macOS 14, *) else { return }
        var starts = 0, pauses = 0, resumes = 0, startRates: [Double] = []
        let connection = class_createInstance(ActiveConnection.self, 0) as! AVCaptureConnection
        precondition(connection.isActive)
        let outputClass: AnyClass = object_getClass(AVCaptureMovieFileOutput())!
        let connected: @convention(block) (AnyObject, String) -> AVCaptureConnection? = { _, _ in connection }
        class_replaceMethod(outputClass, #selector(AVCaptureOutput.connection(with:)),
                            imp_implementationWithBlock(connected), "@@:@")
        let start: @convention(block) (AnyObject, URL, AnyObject) -> Void = { _, _, _ in
            starts += 1; startRates.append(1 / captureInterval.seconds)
        }
        class_replaceMethod(outputClass, #selector(AVCaptureMovieFileOutput.startRecording(to:recordingDelegate:)),
                            imp_implementationWithBlock(start), "v@:@@")
        let pause: @convention(block) (AnyObject) -> Void = { _ in pauses += 1 }
        let resume: @convention(block) (AnyObject) -> Void = { _ in resumes += 1 }
        class_replaceMethod(outputClass, #selector(AVCaptureMovieFileOutput.pauseRecording),
                            imp_implementationWithBlock(pause), "v@:")
        class_replaceMethod(outputClass, #selector(AVCaptureMovieFileOutput.resumeRecording),
                            imp_implementationWithBlock(resume), "v@:")
        let source = TestSource()
        let context = ExactPackageContext(agent: false, session: "native", changed: { _ in }, log: { _ in }, now: { 0 })
        let take = VideoTake(id: "native", path: "app:/data/native.mp4", url: directory.appendingPathComponent("native.mp4"),
                             context: context, maxDuration: 5000, changed: {})
        if delayed { take.pause("requested") }
        // The first consumer receives its source before the initial track constraint runs.
        source.apply(["frameRate": 15.0])
        take.live(video: source)
        await source.drain()
        if delayed {
            precondition(take.state == "paused" && starts == 0, "a source arriving during pause must not start recording: \(take.info()), starts \(starts)")
            take.resume()
            await source.drain()
            precondition(starts == 1 && resumes == 0, "resume must start the delayed output once")
            take.pause("requested")
            await source.drain()
            precondition(pauses == 1)
            take.resume()
            await source.drain()
            precondition(starts == 1 && resumes == 1)
        } else {
            precondition(startRates == [15], "recording must preserve the queued track constraint before its first frame: \(take.info()), starts \(startRates), applied \(source.rates)")
            precondition(source.rates == [15, 15], "preset changes must restore the applied rate, not the stale device rate")
        }
        source.apply(["frameRate": 24.0])
        take.fileOutput(source.session.outputs[0] as! AVCaptureFileOutput,
            didFinishRecordingTo: directory.appendingPathComponent("native.mov"), from: [],
            error: NSError(domain: AVFoundationErrorDomain, code: AVError.unknown.rawValue))
        await source.drain()
        precondition(source.session.outputs.isEmpty && source.session.sessionPreset == .photo)
        precondition(captureInterval.seconds == 1.0 / 24, "cleanup must preserve a queued constraint too")

        let emptySource = TestSource()
        let empty = VideoTake(id: "paused", path: "app:/data/paused.mp4", url: directory.appendingPathComponent("paused.mp4"),
                              context: context, maxDuration: 5000, changed: {})
        empty.pause("requested")
        empty.live(video: emptySource)
        var result: [String: Any]?
        empty.finish("requested") { result = $0 }
        await emptySource.drain()
        precondition(result?["error"] as? String == "NotReadableError", "stopping during setup must answer without starting an empty output")
        precondition(starts == 1 && emptySource.session.outputs.isEmpty)
        print("PASS Apple native \(delayed ? "delayed source pause" : "queued frame rate")")
    }

    @MainActor static func main() async throws {
        if CommandLine.arguments.count > 2 {
            await nativeLifecycle(URL(fileURLWithPath: CommandLine.arguments[1]), delayed: CommandLine.arguments[2] == "delayed")
            return
        }
        if #available(iOS 17, macOS 14, *) {
            let connection = MovieConnection(inputPorts: [], output: AVCaptureMovieFileOutput())
            for angle in [CGFloat(90), 180, 270, 0] {
                VideoTake.orient(connection, angle: angle)
                precondition(connection.videoRotationAngle == angle, "a movie must use the camera's capture angle")
            }
            connection.supported = false
            VideoTake.orient(connection, angle: 90)
            precondition(connection.videoRotationAngle == 0)
        }
        precondition(Cameras.frameRate(60, ranges: [(15, 30), (60, 60)]) == 60)
        precondition(Cameras.frameRate(50, ranges: [(15, 30), (60, 60)]) == 60)
        precondition(Cameras.frameRate(24, ranges: [(15, 30)]) == 24)
        precondition(Cameras.frameRate(.nan, ranges: [(15, 30)]) == nil)
        let directory = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let emptyURL = directory.appendingPathComponent("empty.mp4")
        let emptyContext = ExactPackageContext(agent: false, session: "empty", changed: { _ in }, log: { _ in }, now: { 0 })
        let empty = VideoTake(id: "empty", path: "app:/data/empty.mp4", url: emptyURL,
                              context: emptyContext, maxDuration: 5000, changed: {})
        var released = false
        empty.subscription = ExactSubscription { released = true }
        var emptyResult: [String: Any]?
        empty.finish("requested") { emptyResult = $0 }
        precondition(emptyResult?["error"] as? String == "NotReadableError", "an empty take must answer NotReadableError")
        precondition(emptyResult?["message"] as? String == "the camera supplied no video")
        precondition(empty.state == "inactive" && released && !FileManager.default.fileExists(atPath: emptyURL.path))
        empty.finish("requested") { precondition($0["error"] as? String == "NotReadableError") }
        for (index, transform) in [
            CGAffineTransform.identity,
            CGAffineTransform(a: 0, b: 1, c: -1, d: 0, tx: 16, ty: 0),
            CGAffineTransform(a: -1, b: 0, c: 0, d: -1, tx: 32, ty: 16),
            CGAffineTransform(a: 0, b: -1, c: 1, d: 0, tx: 0, ty: 32),
        ].enumerated() {
            let movie = directory.appendingPathComponent("rotation-\(index).mp4")
            try await rotatedMovie(movie, transform)
            let metadata = try await VideoTake.metadata(movie)
            let portrait = index % 2 == 1
            precondition(metadata["width"] as? CGFloat == (portrait ? 16 : 32), "width must include the movie's rotation")
            precondition(metadata["height"] as? CGFloat == (portrait ? 32 : 16), "height must include the movie's rotation")
        }
        let tracks = ExactTracks(), owner = Camera()
        tracks.add(ExactTrackEntry(id: "camera", kind: "video", owner: owner))
        tracks.add(ExactTrackEntry(id: "stream", kind: "stream", tracks: ["camera"]))
        var now = 0.0, at = 0.0
        var timer: (() -> Void)?
        let context = ExactPackageContext(agent: true, session: "video", changed: { _ in }, log: { _ in }, now: { now }, tracks: tracks,
            file: { directory.appendingPathComponent(String($0.split(separator: "/").last!)) },
            after: { ms, _, body in at = now + ms; timer = body; return { timer = nil } })
        let module = RecordingModule(context: context)
        func call(_ name: String, _ args: [String: Any]) -> [String: Any] {
            var answer: [String: Any]?
            module.later(name, args, preferred: nil) { answer = $0 }
            return answer!
        }
        let start = call("startMediaRecorder", ["stream": "stream", "path": "app:/data/clip", "maxDuration": 5000.0])
        precondition(start["state"] as? String == "recording", "a camera stream must start a recorder")
        let id = start["id"] as! String
        now = 2000
        _ = call("pauseMediaRecorder", ["recorder": id])
        precondition(timer == nil)
        now = 12000
        _ = call("resumeMediaRecorder", ["recorder": id])
        precondition(at == 15000)
        now = at; timer!()
        let state = call("mediaRecorderState", ["recorder": id])
        precondition(state["state"] as? String == "inactive" && state["reason"] as? String == "max-duration")
        let result = call("stopMediaRecorder", ["recorder": id])
        precondition(result["mimeType"] as? String == "video/mp4" && owner.consumers == 0)
        let url = directory.appendingPathComponent("clip.mp4")
        let bytes = try Data(contentsOf: url)
        precondition(result["size"] as? Int == bytes.count)
        let metadata = try await VideoTake.metadata(url)
        for key in ["duration", "width", "height", "frameRate"] {
            precondition((result[key] as! NSNumber).doubleValue == (metadata[key] as! NSNumber).doubleValue, "metadata must describe the clip: \(key)")
        }
        print("PASS Apple camera recording, empty take, pause, duration limit, sticky result, lease release and rotated MP4 metadata")
    }
}
