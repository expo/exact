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
    var devices: [AVCaptureInput] = []
    var operations: [String] = []
    var beforeAttach: () -> Void = {}
    var routes: [AVCaptureConnection] = [], removed: [AVCaptureConnection] = []
    override var connections: [AVCaptureConnection] { routes }
    override func removeConnection(_ connection: AVCaptureConnection) {
        removed.append(connection)
        routes.removeAll { $0 === connection }
    }
    override var inputs: [AVCaptureInput] { devices }
    var providesClock = true
    override var synchronizationClock: CMClock? { providesClock ? CMClockGetHostTimeClock() : nil }
    override func canAddInput(_ input: AVCaptureInput) -> Bool { true }
    override func addInput(_ input: AVCaptureInput) { beforeAttach(); devices.append(input); operations.append("+mic") }
    override func removeInput(_ input: AVCaptureInput) { devices.removeAll { $0 === input }; operations.append("-mic") }
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
        if output is AVCaptureMovieFileOutput { precondition(output.connection(with: .video)?.isActive == true) }
        attached.append(output); operations.append(output is AVCaptureMovieFileOutput ? "+movie" : "+samples")
    }
    override func removeOutput(_ output: AVCaptureOutput) { attached.removeAll { $0 === output }; operations.append(output is AVCaptureMovieFileOutput ? "-movie" : "-samples") }
    override var outputs: [AVCaptureOutput] { attached }
}

private final class TestSource: ExactVideoSource {
    let session: AVCaptureSession = TestSession()
    let device = class_createInstance(TestDevice.self, 0) as! AVCaptureDevice
    var work: [() -> Void] = []
    var rates: [Double] = []
    private var draining = false
    private var objects: [String: AnyObject] = [:]
    func configure(stopped: Bool, _ change: @escaping (AVCaptureSession) -> Void) {
        work.append {
            change(self.session)
            self.movieAudioRoutes.restrictConnections(in: self.session)
        }
    }
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
    override var isEnabled: Bool { get { true } set {} }
    override func isVideoRotationAngleSupported(_ angle: CGFloat) -> Bool { false }
}

private final class MicPort: AVCaptureInput.Port {
    var owner: AVCaptureInput!
    override var input: AVCaptureInput { owner }
}

private final class MicConnection: AVCaptureConnection {
    var port: MicPort!
    var inactive = false
    var destination: AVCaptureOutput?
    override var output: AVCaptureOutput? { destination }
    var delivering = true
    override var isActive: Bool { !inactive }
    override var inputPorts: [AVCaptureInput.Port] { [port] }
    override var isEnabled: Bool { get { delivering } set { delivering = newValue } }
}

private final class MixedOwner: ExactTrackOwner {
    let mic = class_createInstance(AVCaptureDeviceInput.self, 0) as! AVCaptureDeviceInput
    let camera = TestSource()
    let samples = AVCaptureAudioDataOutput()
    var initiallyEnabled = true
    var engineOwnsMic = true
    var micReader: ExactMovieAudioConsumer?
    var leases = 0
    func subscribe(_ id: String, _ reader: ExactTrackConsumer, start: Bool, cause: ExactTrackCause) throws -> ExactSubscription {
        if id == "mic" {
            micReader = reader as? ExactMovieAudioConsumer
            engineOwnsMic = false
            micReader!.prepareMicrophone { _ in precondition(!self.engineOwnsMic) }
            micReader!.microphone(mic, samples: samples, enabled: initiallyEnabled)
        } else { reader.live(video: camera) }
        leases += 1
        return ExactSubscription {
            if id == "mic" { precondition(self.camera.session.inputs.isEmpty); self.engineOwnsMic = true }
            self.leases -= 1
        }
    }
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

    @MainActor static func inspectMixed(_ url: URL) async throws {
        let asset = AVURLAsset(url: url)
        let video = try await asset.loadTracks(withMediaType: .video), audio = try await asset.loadTracks(withMediaType: .audio)
        precondition(video.count == 1 && audio.count == 1)
        let v = try await video[0].load(.timeRange), a = try await audio[0].load(.timeRange)
        let rate = try await video[0].load(.nominalFrameRate)
        precondition(rate.isFinite && rate > 0)
        precondition(v.start == a.start && a.duration.seconds > 0 && abs(v.duration.seconds - a.duration.seconds) <= 1 / Double(rate))
        let formats = try await audio[0].load(.formatDescriptions)
        precondition(CMFormatDescriptionGetMediaSubType(formats[0]) == kAudioFormatMPEG4AAC)
        print("PASS Apple mixed MP4, starts \(v.start.seconds)/\(a.start.seconds) s, durations \(v.duration.seconds)/\(a.duration.seconds) s at \(rate) fps")
    }

    @MainActor static func mixedSession(_ directory: URL, scenario: String) async throws {
        guard #available(macOS 14, iOS 18, *) else { return }
        let owner = MixedOwner(), session = owner.camera.session as! TestSession
        let video = class_createInstance(ActiveConnection.self, 0) as! AVCaptureConnection
        let port = class_createInstance(MicPort.self, 0) as! MicPort
        port.owner = scenario == "wrong-port" ? class_createInstance(AVCaptureDeviceInput.self, 0) as! AVCaptureInput : owner.mic
        let pcm = class_createInstance(MicConnection.self, 0) as! MicConnection
        pcm.port = port; pcm.isEnabled = true; pcm.inactive = scenario == "inactive-pcm"
        let connected: @convention(block) (AnyObject, String) -> AVCaptureConnection? = { output, type in
            guard session.outputs.contains(where: { $0 === output }) else { return nil }
            return type == AVMediaType.audio.rawValue ? pcm : video
        }
        for output in [owner.samples as AVCaptureOutput, AVCaptureVideoDataOutput()] {
            class_replaceMethod(object_getClass(output)!, #selector(AVCaptureOutput.connection(with:)), imp_implementationWithBlock(connected), "@@:@")
        }
        let videoClass: AnyClass = object_getClass(AVCaptureVideoDataOutput())!
        let codecs: @convention(block) (AnyObject, String) -> NSArray = { _, _ in [] }
        class_replaceMethod(videoClass, #selector(AVCaptureVideoDataOutput.availableVideoCodecTypesForAssetWriter(writingTo:)),
                            imp_implementationWithBlock(codecs), "@@:@")
        if ["interruption-reader", "restored-reader", "prehistory", "recovery-handoff", "old-engine", "background-allowed", "overlap-lease", "same-background", "muted-readers", "reader-pause", "readers", "detach-lease", "reset-sdk", "reset-session", "reset-unrelated"].contains(scenario) {
            try await mixedReaders(directory, owner, pcm: pcm, scenario: scenario)
            return
        }
        session.beforeAttach = { precondition(!owner.engineOwnsMic) }
        let tracks = ExactTracks()
        tracks.add(ExactTrackEntry(id: "camera", kind: "video", owner: owner))
        tracks.add(ExactTrackEntry(id: "mic", kind: "audio", owner: owner))
        tracks.add(ExactTrackEntry(id: "stream", kind: "stream", tracks: ["camera", "mic"]))
        let context = ExactPackageContext(agent: false, session: "mock", changed: { _ in }, log: { _ in }, now: { 0 }, tracks: tracks,
            file: { directory.appendingPathComponent(String($0.split(separator: "/").last!)) })
        let destination = directory.appendingPathComponent("take.mp4"), sentinel = Data("previous recording".utf8)
        try sentinel.write(to: destination)
        context.request = 71
        var answer: [String: Any]?
        let module = RecordingModule(context: context)
        var options: [String: Any] = ["stream": "stream", "path": "app:/data/take"]
        let probeMicReady = Flag(), probeVideoReady = Flag()
        // Each scenario has its own process. Keep the encoder's real readiness underneath the stall.
        if ["clock-missing", "clock-present", "max-prefix", "reset-blocked"].contains(scenario) {
            let selector = #selector(getter: AVAssetWriterInput.isReadyForMoreMediaData)
            let method = class_getInstanceMethod(AVAssetWriterInput.self, selector)!
            typealias Ready = @convention(c) (AVAssetWriterInput, Selector) -> Bool
            let native = unsafeBitCast(method_getImplementation(method), to: Ready.self)
            let ready: @convention(block) (AVAssetWriterInput) -> Bool = { input in
                (input.mediaType == .audio ? probeMicReady.value : probeVideoReady.value) && native(input, selector)
            }
            method_setImplementation(method, imp_implementationWithBlock(ready))
        }
        if scenario == "max-prefix" { options["maxDuration"] = 100.0; probeVideoReady.value = false }
        module.later("startMediaRecorder", options, preferred: nil) { answer = $0 }
        if scenario == "port-change" {
            await owner.camera.drain()
            let clock = session.synchronizationClock!
            func now() -> Int { Int(CMTimeConvertScale(CMClockGetTime(clock), timescale: 48000, method: .roundTowardZero).value) }
            let start = now() - 4800
            try sendPicture(owner.camera, at: start)
            owner.micReader!.movieAudio(try packet(start, 4800))
            let deadline = ContinuousClock.now + .seconds(4)
            while answer == nil && ContinuousClock.now < deadline { try await Task.sleep(for: .milliseconds(2)) }
            precondition(answer?["id"] != nil)
            owner.micReader!.microphone(owner.mic, samples: owner.samples, enabled: false)
            owner.micReader!.movieAudio(try packet(now() - 480, 480))
            owner.micReader!.microphone(owner.mic, samples: owner.samples, enabled: true)
            NotificationCenter.default.post(name: AVCaptureInput.Port.formatDescriptionDidChangeNotification, object: port)
            try await Task.sleep(for: .milliseconds(35))
            let after = try pcmSamples(owner.micReader!.movieAudio(try packet(now() - 480, 480))!)
            precondition(after.allSatisfy { $0 == 0 })
            print("PASS Apple mixed microphone port change fences a pending enable")
            return
        }
        if ["early-audio", "early-pts"].contains(scenario) {
            await owner.camera.drain()
            if scenario == "early-pts" { try sendPicture(owner.camera, at: 100800) }
            for i in 0..<200 { owner.micReader!.movieAudio(try packet(i * 480, 480)) }
            precondition(answer == nil)
            owner.micReader!.movieAudio(try packet(96000, 480))
            for _ in 0..<1000 where answer == nil { await owner.camera.drain(); try await Task.sleep(for: .milliseconds(2)) }
            precondition(answer?["error"] as? String == "NotReadableError" && answer?["message"] as? String == "the writer fell behind")
            precondition((try? Data(contentsOf: destination)) == sentinel && owner.leases == 0)
            print("PASS Apple mixed two seconds before the first picture preserves the destination")
            return
        }
        if scenario == "no-audio" {
            await owner.camera.drain()
            try sendPicture(owner.camera, at: 0)
            let deadline = ContinuousClock.now + .seconds(7)
            while answer == nil && ContinuousClock.now < deadline {
                try await Task.sleep(for: .milliseconds(10))
            }
            precondition(answer?["error"] as? String == "NotReadableError")
            precondition((try? Data(contentsOf: destination)) == sentinel)
            await owner.camera.drain()
            precondition(owner.leases == 0)
            print("PASS Apple mixed admission deadline preserves destination")
            return
        }
        if scenario == "max-prefix" {
            await owner.camera.drain()
            let base = Int(CMTimeConvertScale(CMClockGetTime(CMClockGetHostTimeClock()), timescale: 48000, method: .roundTowardZero).value) - 24000
            for i in stride(from: 0, to: 24000, by: 480) {
                if i % 4800 == 0 { try sendPicture(owner.camera, at: base + i) }
                owner.micReader!.movieAudio(try packet(base + i, 480))
            }
            try sendPicture(owner.camera, at: base + 23520)
            probeVideoReady.value = true
            for _ in 0..<1000 where answer == nil { await owner.camera.drain(); try await Task.sleep(nanoseconds: 2_000_000) }
            let args = ["recorder": answer!["id"]!]
            var stopped: [String: Any]?
            module.later("stopMediaRecorder", args, preferred: nil) { stopped = $0 }
            for _ in 0..<1000 where stopped == nil { await owner.camera.drain(); try await Task.sleep(nanoseconds: 2_000_000) }
            precondition(stopped?["duration"] as? Double == 100, "\(String(describing: stopped))")
            print("PASS Apple mixed maxDuration bounds the admission prefix at 100 ms")
            return
        }
        if scenario == "reset-blocked" {
            await owner.camera.drain()
            let now = CMClockGetTime(CMClockGetHostTimeClock())
            let base = Int(CMTimeConvertScale(now, timescale: 48000, method: .roundTowardZero).value) - 9600
            try sendPicture(owner.camera, at: base)
            owner.micReader!.movieAudio(try packet(base, 4800))
            for _ in 0..<2000 where answer == nil { await owner.camera.drain(); try await Task.sleep(for: .milliseconds(2)) }
            probeMicReady.value = false
            owner.micReader!.movieAudio(try packet(base + 4800, 480))
            try sendPicture(owner.camera, at: base + 4800)
            owner.micReader!.muted("media-reset")
            let args = ["recorder": answer!["id"]!]
            var stop: [String: Any]?
            module.later("stopMediaRecorder", args, preferred: nil) { stop = $0 }
            for _ in 0..<2500 where stop == nil { await owner.camera.drain(); try await Task.sleep(for: .milliseconds(2)) }
            module.later("mediaRecorderState", args, preferred: nil) { state in
                precondition(state["reason"] as? String == "media-reset" && stop?["mimeType"] as? String == "video/mp4")
                print("PASS Apple mixed reset keeps its reason and playable prefix")
            }
            return
        }
        if scenario == "clock-missing" || scenario == "clock-present" {
            let now = { Int(CMTimeConvertScale(CMClockGetTime(CMClockGetHostTimeClock()), timescale: 48000, method: .roundHalfAwayFromZero).value) }
            await owner.camera.drain()
            let origin = now() - 4800
            try sendPicture(owner.camera, at: origin)
            owner.micReader!.movieAudio(try packet(origin, 4800) { _ in true })
            for _ in 0..<1000 where answer == nil { try await Task.sleep(nanoseconds: 2_000_000) }
            precondition(answer?["state"] as? String == "recording")
            probeMicReady.value = false
            owner.micReader!.microphone(owner.mic, samples: owner.samples, enabled: false)
            let first = now() + 2
            // Supply the PCM captured while the encoder was opening, so both tracks are continuous.
            var cursor = origin + 4800
            while cursor < first {
                let n = min(480, first - cursor)
                owner.micReader!.movieAudio(try packet(cursor, n) { _ in true })
                cursor += n
            }
            try await Task.sleep(nanoseconds: 110_000_000)
            owner.micReader!.movieAudio(try packet(first, 4800))
            try sendPicture(owner.camera, at: first + 2400)
            owner.micReader!.muted("background")
            session.providesClock = scenario != "clock-missing"
            owner.micReader!.microphone(owner.mic, samples: owner.samples, enabled: true)
            probeMicReady.value = true
            session.providesClock = true
            let args = ["recorder": answer!["id"]!]
            var stopped: [String: Any]?
            module.later("stopMediaRecorder", args, preferred: nil) { stopped = $0 }
            for _ in 0..<1000 where stopped == nil { await owner.camera.drain(); try await Task.sleep(nanoseconds: 2_000_000) }
            precondition(stopped?["mimeType"] as? String == "video/mp4", "stop: \(String(describing: stopped))")
            let output = try decoded(destination)
            precondition(output.count == first + 4800 - origin && output.allSatisfy { abs($0) < 0.001 })
            print("PASS Apple mixed", scenario, "only nonzero input is captured inside disabled interval; decoded peak", output.map(abs).max()!, "frames",output.count, "result",stopped ?? [:])
            return
        }
        if scenario == "staging" {
            let now = { Int(CMTimeConvertScale(CMClockGetTime(session.synchronizationClock!), timescale: 48000, method: .roundHalfAwayFromZero).value) }
            let base = now() - 9600
            var next = 0
            await owner.camera.drain()
            func produce(to end: Int) throws {
                while next < end {
                    if next % 2400 == 0 { try sendPicture(owner.camera, at: base + next) }
                    owner.micReader!.movieAudio(try packet(base + next, 480))
                    next += 480
                }
            }
            try produce(to: 2400)
            for _ in 0..<2000 where answer == nil { try await Task.sleep(nanoseconds: 2_000_000) }
            precondition(answer?["id"] != nil)
            owner.micReader!.microphone(owner.mic, samples: owner.samples, enabled: false)
            let from = now() - base
            try await Task.sleep(nanoseconds: 100_000_000)
            let to = now() - base
            owner.micReader!.microphone(owner.mic, samples: owner.samples, enabled: true)
            // Both changes precede delivery of the buffers spanning them.
            let endFrame = max(240000, to + 144000)
            for end in stride(from: 4800, through: endFrame - 24000, by: 2400) {
                try produce(to: end)
                try await Task.sleep(nanoseconds: 15_000_000)
            }
            let names = try FileManager.default.contentsOfDirectory(atPath: directory.path)
            precondition(names.count == 2 && names.contains("take.mp4"))
            let stage = directory.appendingPathComponent(names.first { $0 != "take.mp4" }!)
            let intact = try Data(contentsOf: destination)
            precondition(intact == sentinel)
            var staged: [Float] = []
            for _ in 0..<100 where staged.count <= to + 1000 {
                staged = (try? decoded(stage)) ?? []
                if staged.count <= to + 1000 { try await Task.sleep(nanoseconds: 10_000_000) }
            }
            precondition(staged.count > to + 1000, "staging has \(staged.count) readable frames, mute \(from)..<\(to)")
            let middle = (from + 1024)..<(to - 1024)
            print("staging interval", from, to, "readable frames", staged.count, "interior peak", staged[middle].map(abs).max()!)
            precondition(staged.count > to + 1000 && staged[middle].allSatisfy { abs($0) < 0.001 })
            precondition(staged[(to + 100)..<(to + 1000)].map(abs).max()! > 0.4)
            try produce(to: endFrame)
            let args = ["recorder": answer!["id"]!]
            module.later("stopMediaRecorder", args, preferred: nil) { _ in }
            await owner.camera.drain()
            let result: [String: Any] = await withCheckedContinuation { done in
                module.later("stopMediaRecorder", args, preferred: nil) { done.resume(returning: $0) }
            }
            precondition(abs((result["duration"] as? Double ?? 0) - Double((endFrame + 479) / 480 * 10)) < 1, "\(result)")
            let finished = try decoded(destination)
            precondition(finished[middle] == staged[middle])
            let finalFiles = try FileManager.default.contentsOfDirectory(atPath: directory.path)
            precondition(finalFiles == ["take.mp4"])
            print("PASS Apple mixed live staging contains silent PCM, existing destination replaced at finish")
            return
        }
        if scenario.hasPrefix("cancel-") {
            let steps = Int(scenario.suffix(1))!
            for _ in 0..<steps { owner.camera.work.removeFirst()() }
            module.cancel(71)
        } else if scenario == "late-inactive" || scenario == "pending-cancel" || scenario == "pending-interrupt" {
            await owner.camera.drain()
            try sendPicture(owner.camera, at: 0)
            precondition(answer == nil)
            if scenario == "late-inactive" { pcm.inactive = true }
            owner.micReader!.movieAudio(try packet(0, 480))
            if scenario == "pending-cancel" { module.cancel(71) }
            if scenario == "pending-interrupt" { owner.micReader!.muted("interrupted") }
        }
        for _ in 0..<2000 where answer == nil {
            await owner.camera.drain()
            try await Task.sleep(nanoseconds: 2_000_000)
        }
        let kept = try Data(contentsOf: destination)
        precondition(answer?["error"] as? String == "NotReadableError", "\(scenario): \(String(describing: answer))")
        precondition(owner.leases == 0 && owner.engineOwnsMic && session.inputs.isEmpty && session.outputs.isEmpty && kept == sentinel)
        let files = try FileManager.default.contentsOfDirectory(atPath: directory.path)
        precondition(files == ["take.mp4"])
        print("PASS Apple mixed \(scenario) preserves the previous file and releases the graph")
    }

    private static func sendPicture(_ camera: TestSource, at frame: Int) throws {
        let picture = camera.session.outputs.compactMap { $0 as? AVCaptureVideoDataOutput }.first!
        let sample = try MixedTake.agentPicture(frame)
        picture.sampleBufferCallbackQueue!.sync {
            picture.sampleBufferDelegate!.captureOutput?(picture, didOutput: sample, from: picture.connection(with: .video)!)
        }
    }

    @MainActor private static func mixedReaders(_ directory: URL, _ owner: MixedOwner, pcm: MicConnection, scenario: String) async throws {
        let graph = owner.camera.session as! TestSession
        var engine: ProcessCapture.EngineTap?, claims = 0
        let process = ProcessCapture(movieInput: { owner.mic }) { _, deliver in
            precondition(graph.inputs.isEmpty && engine == nil)
            engine = deliver
            return ExactSubscription { engine = nil }
        }
        graph.beforeAttach = { precondition(engine == nil) }
        var event: ((String) -> Void)?
        let session = ExactAudioSession { _, _, _, notify in event = notify; claims += 1; return { claims -= 1 } }
        var now = 0.0
        func context(_ name: String, _ tracks: ExactTracks, agent: Bool) -> ExactPackageContext {
            ExactPackageContext(agent: agent, session: name, changed: { _ in }, log: { _ in }, now: { now }, tracks: tracks,
                file: { directory.appendingPathComponent(String($0.split(separator: "/").last!)) }, audioSession: session)
        }
        func call(_ module: ExactPackage, _ name: String, _ args: [String: Any]) async -> [String: Any] {
            await withCheckedContinuation { done in module.later(name, args, preferred: nil) { done.resume(returning: $0) } }
        }
        let a = ExactTracks(), b = ExactTracks(), first = context("movie", a, agent: true)
        let capture = MediaCaptureModule(context: first, audioSource: process, process: process)
        let other = MediaCaptureModule(context: context("other", b, agent: true), audioSource: process, process: process)
        let movie = RecordingModule(context: context("movie", a, agent: false))
        let voice = RecordingModule(context: context("other", b, agent: false))
        let mic = await call(capture, "getUserMedia", ["audio": true])
        let track = (mic["tracks"] as! [[String: Any]])[0]["id"] as! String
        let second = await call(other, "getUserMedia", ["audio": true])
        var sameID: [String: Any]?
        if ["restored-reader", "muted-readers", "same-background", "background-allowed", "prehistory", "recovery-handoff", "old-engine"].contains(scenario) {
            let same = await call(movie, "startMediaRecorder", ["stream": mic["id"]!, "path": "app:/data/same.wav", "mimeType": "audio/wav"])
            sameID = ["recorder": same["id"]!]
        }
        let start = await call(voice, "startMediaRecorder", ["stream": second["id"]!, "path": "app:/data/voice.wav", "mimeType": "audio/wav"])
        let id = ["recorder": start["id"]!]
        let meter: [String: Any] = ["track": track, "interval": 10.0]
        _ = await call(movie, "audioLevel", meter)
        let frames = AVAudioPCMBuffer(pcmFormat: process.format, frameCapacity: 480)!
        frames.frameLength = 480
        for i in 0..<480 { frames.floatChannelData![0][i] = 0.5 }
        engine!(frames, AVAudioTime(sampleTime: 0, atRate: 48000))
        a.add(ExactTrackEntry(id: "camera", kind: "video", owner: owner))
        a.add(ExactTrackEntry(id: "mixed", kind: "stream", tracks: ["camera", track]))
        let originalEngine = engine!
        var mutedFrame: Int64?
        if scenario == "prehistory" {
            _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": false])
            mutedFrame = CMTimeConvertScale(CMClockGetTime(CMClockGetHostTimeClock()), timescale: 48000, method: .roundTowardPositiveInfinity).value
            try await Task.sleep(for: .milliseconds(12))
            _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": true])
        }
        var admitted: [String: Any]?
        movie.later("startMediaRecorder", ["stream": "mixed", "path": "app:/data/take"], preferred: nil) { admitted = $0 }
        if scenario == "reader-pause" {
            // Admission has reserved the input, but the delayed configuration has not taken it yet.
            precondition(engine != nil && graph.inputs.isEmpty)
            for i in 0..<480 { frames.floatChannelData![0][i] = 0.125 }
            engine!(frames, AVAudioTime(sampleTime: 480, atRate: 48000))
        }
        await owner.camera.drain()
        let base = mutedFrame.map { $0 - 480 } ?? (CMTimeConvertScale(CMClockGetTime(graph.synchronizationClock!), timescale: 48000, method: .roundHalfAwayFromZero).value - 960)
        try sendPicture(owner.camera, at: Int(base + 480))
        precondition(admitted == nil)
        let audio = process.movieSamples!
        for i in 0..<480 { frames.floatChannelData![0][i] = 0.25 }
        audio.output.sampleBufferCallbackQueue!.sync { audio.receive(Writer.sample(frames, at: base + 480)!) }
        await owner.camera.drain()
        for _ in 0..<2000 where admitted == nil { try await Task.sleep(nanoseconds: 2_000_000) }
        precondition(admitted?["state"] as? String == "recording" && engine == nil && claims == 1)
        now = 20
        let level = await call(movie, "audioLevel", meter)
        precondition(level["audioLevel"] as? Double == (scenario == "prehistory" ? 0 : 0.25), "same-track meter: \(level)")
        let args = ["recorder": admitted!["id"]!]
        if scenario == "interruption-reader" {
            final class Reader: ExactTrackConsumer {
                var tap: ExactSubscription?, peak: Float?
                func live(_ source: ExactAudioSource?) {
                    tap = source?.tap { [weak self] pcm, _ in self?.peak = pcm.floatChannelData?[0][0] }
                }
                func muted(_ reason: String) {}
                func unmuted() {}
                func ended() {}
            }
            let reader = Reader()
            let lease = try a.subscribe(track, reader, start: true, origin: "app")
            capture.heard("interrupted")
            let held = Writer.sample(frames, at: base + 960)!
            capture.heard("resumed")
            audio.output.sampleBufferCallbackQueue!.sync { audio.receive(held) }
            precondition(reader.peak == 0)
            lease.cancel()
            print("PASS Apple mixed interrupted microphone packet stays silent at a surviving tap")
            return
        }
        if scenario == "restored-reader" {
            movie.later("stopMediaRecorder", args, preferred: nil) { _ in }
            await owner.camera.drain()
            _ = await call(movie, "stopMediaRecorder", args)
            for value in [false, true] { _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": value]) }
            engine!(frames, AVAudioTime(sampleTime: 960, atRate: 48000))
            _ = await call(movie, "stopMediaRecorder", sameID!)
            let wav = try decoded(directory.appendingPathComponent("same.wav"))
            precondition(wav.count == 1440 && wav[960...].allSatisfy { $0 == 0.25 })
            print("PASS Apple mixed surviving standalone tap resumes on engine sample time")
            return
        }
        if ["prehistory", "recovery-handoff", "old-engine"].contains(scenario) {
            if scenario == "recovery-handoff" {
                graph.providesClock = false
                _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": false])
                _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": true])
            }
            if scenario == "old-engine" { originalEngine(frames, AVAudioTime(sampleTime: 480, atRate: 48000)) }
            movie.later("stopMediaRecorder", args, preferred: nil) { _ in }
            await owner.camera.drain()
            _ = await call(movie, "stopMediaRecorder", args)
            if scenario == "recovery-handoff" {
                let timestamp = CMClockGetTime(CMClockGetHostTimeClock())
                try await Task.sleep(for: .milliseconds(12))
                engine!(frames, AVAudioTime(hostTime: CMClockConvertHostTimeToSystemUnits(timestamp), sampleTime: 960, atRate: 48000))
            }
            _ = await call(movie, "stopMediaRecorder", sameID!)
            let wav = try decoded(directory.appendingPathComponent("same.wav"))
            if scenario == "prehistory" {
                let file = try decoded(directory.appendingPathComponent("take.mp4"))
                precondition(wav.count == 960 && wav[480...].allSatisfy { $0 == 0 })
                precondition(file.allSatisfy { abs($0) < 0.001 })
            } else if scenario == "recovery-handoff" {
                precondition(wav.count == 1440 && wav[960...].allSatisfy { $0 == 0 })
            } else { precondition(wav.count == 960) }
            print("PASS Apple mixed", scenario, "reader frames", wav.count)
            return
        }
        if scenario == "background-allowed" {
            #if os(iOS)
            precondition(Bundle.main.object(forInfoDictionaryKey: "ExactCapturesInBackground") as? Bool == true)
            #endif
            first.foreground = false; capture.foreground(false); await owner.camera.drain()
            precondition(!graph.inputs.contains(owner.mic) && engine != nil)
            // In this period the interrupted camera supplies no packets; the independent mic still has a lease.
            if let engine { engine(frames, AVAudioTime(sampleTime: 960, atRate: 48000)) }
            let held = try packet(Int(base + 480), 480)
            audio.output.sampleBufferCallbackQueue!.sync { audio.captureOutput(audio.output, didOutput: held, from: pcm) }
            _ = await call(movie, "stopMediaRecorder", sameID!)
            let wav = try decoded(directory.appendingPathComponent("same.wav"))
            precondition(wav.count == 1440 && engine != nil)
            first.foreground = true; capture.foreground(true); await owner.camera.drain()
            precondition(graph.inputs.contains(owner.mic) && engine == nil)
            movie.later("stopMediaRecorder", args, preferred: nil) { _ in }
            await owner.camera.drain()
            print("PASS Apple mixed background microphone handoff", wav.count)
            return
        }
        if scenario == "overlap-lease" || scenario == "same-background" {
            _ = await call(voice, "stopMediaRecorder", id)
            first.foreground = false
            capture.foreground(false)
            precondition(graph.inputs.contains(owner.mic) && engine == nil)
            if scenario == "overlap-lease" {
                movie.later("stopMediaRecorder", args, preferred: nil) { _ in }
                let more = await call(voice, "startMediaRecorder", ["stream": second["id"]!, "path": "app:/data/new.wav", "mimeType": "audio/wav"])
                precondition(more["id"] != nil && graph.inputs.contains(owner.mic) && engine == nil)
                await owner.camera.drain()
                precondition(graph.inputs.isEmpty && engine != nil)
                _ = await call(voice, "stopMediaRecorder", ["recorder": more["id"]!])
            } else {
                await owner.camera.drain()
                #if os(macOS)
                precondition(graph.inputs.isEmpty && engine != nil)
                engine!(frames, AVAudioTime(sampleTime: 960, atRate: 48000))
                #endif
                _ = await call(movie, "stopMediaRecorder", sameID!)
                let wav = try decoded(directory.appendingPathComponent("same.wav"))
                #if os(macOS)
                precondition(wav.count == 1440 && wav[960...].allSatisfy { abs($0 - 0.25) < 0.001 })
                #else
                precondition(wav.count == 960)
                #endif
                movie.later("stopMediaRecorder", args, preferred: nil) { _ in }
                await owner.camera.drain()
            }
            _ = await call(movie, "stopMediaRecorder", args)
            movie.release("audioLevel")
            precondition(engine == nil && claims == 0)
            print("PASS Apple mixed \(scenario)")
            return
        }
        if let sameID {
            func nowFrame() -> Int64 {
                CMTimeConvertScale(CMClockGetTime(graph.synchronizationClock!), timescale: 48000, method: .roundHalfAwayFromZero).value
            }
            let before = nowFrame()
            _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": false])
            let after = nowFrame()
            try await Task.sleep(nanoseconds: 12_000_000)
            let until = nowFrame()
            _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": true])
            // The capture delegate is late. This buffer straddles the disable and is
            // delivered after re-enabling. The independent track still owns its sound.
            let firstFrame = before - 120
            let packet = Writer.sample(frames, at: firstFrame)!
            audio.output.sampleBufferCallbackQueue!.sync { audio.receive(packet) }
            _ = await call(movie, "stopMediaRecorder", sameID)
            _ = await call(voice, "stopMediaRecorder", id)
            let same = try decoded(directory.appendingPathComponent("same.wav"))
            let independent = try decoded(directory.appendingPathComponent("voice.wav"))
            precondition(same.count == 1440 && independent.count == 1440)
            precondition(independent[960...].allSatisfy { abs($0 - 0.25) < 0.001 })
            let disabled = max(0, Int(after - firstFrame) + 1)..<min(480, Int(until - firstFrame))
            precondition(disabled.count > 100 && disabled.allSatisfy { same[960 + $0] == 0 })
            precondition(same[960...].allSatisfy { $0 == 0 })
            movie.later("stopMediaRecorder", args, preferred: nil) { _ in }
            await owner.camera.drain()
            _ = await call(movie, "stopMediaRecorder", args)
            movie.release("audioLevel")
            precondition(engine == nil && claims == 0)
            print("PASS Apple mixed shared capture PCM, muted and independent WAVs")
            return
        }
        if ["readers", "detach-lease", "reader-pause"].contains(scenario) {
            _ = await call(movie, "pauseMediaRecorder", args)
            if scenario == "reader-pause" {
                for i in 0..<480 { frames.floatChannelData![0][i] = 0.375 }
                audio.output.sampleBufferCallbackQueue!.sync { audio.receive(Writer.sample(frames, at: base + 960)!) }
            }
            _ = await call(movie, "resumeMediaRecorder", args)
            if scenario == "reader-pause" {
                for i in 0..<480 { frames.floatChannelData![0][i] = 0.625 }
                audio.output.sampleBufferCallbackQueue!.sync { audio.receive(Writer.sample(frames, at: base + 1440)!) }
            }
            for i in 0..<480 { frames.floatChannelData![0][i] = 0.375 }

            first.foreground = false
            capture.foreground(false)
            await owner.camera.drain()
            let paused = await call(movie, "mediaRecorderState", args)
            precondition(paused["reason"] as? String == "background" && paused["state"] as? String == "paused")
            pcm.inactive = true
            precondition(graph.inputs.isEmpty && engine != nil)
            if let engine { engine(frames, AVAudioTime(sampleTime: scenario == "reader-pause" ? 1920 : 960, atRate: 48000)) }
            else {
                precondition(pcm.isActive)
                audio.output.sampleBufferCallbackQueue!.sync { audio.receive(Writer.sample(frames, at: base + 960)!) }
            }
        }
        if scenario == "reset-session" { event!("reset") }
        #if os(iOS)
        if scenario == "reset-sdk" { NotificationCenter.default.post(name: AVAudioSession.mediaServicesWereResetNotification, object: nil) }
        #endif
        var saved: [String: Any]?
        movie.later("stopMediaRecorder", args, preferred: nil) { saved = $0 }
        if scenario == "detach-lease" {
            let extra = await call(voice, "startMediaRecorder", ["stream": second["id"]!, "path": "app:/data/extra.wav", "mimeType": "audio/wav"])
            precondition(extra["id"] != nil)
            _ = await call(voice, "stopMediaRecorder", ["recorder": extra["id"]!])
        }
        if scenario == "reset-unrelated" { event!("reset") }
        await owner.camera.drain()
        let result = await call(movie, "stopMediaRecorder", args)
        precondition(saved?["mimeType"] as? String == "video/mp4" && result["mimeType"] as? String == "video/mp4", "\(result)")
        let status = await call(movie, "mediaRecorderState", args)
        precondition(status["reason"] as? String == (["reset-session", "reset-sdk"].contains(scenario) ? "media-reset" : "requested"))
        try await inspectMixed(directory.appendingPathComponent("take.mp4"))
        if !scenario.hasPrefix("reset-") {
            precondition(graph.inputs.isEmpty && engine != nil && claims == 1)
            for i in 0..<480 { frames.floatChannelData![0][i] = 0.75 }
            engine!(frames, AVAudioTime(sampleTime: scenario == "reader-pause" ? 2400 : 1440, atRate: 48000))
        }
        _ = await call(voice, "stopMediaRecorder", id)
        let wav = try decoded(directory.appendingPathComponent("voice.wav"))
        let expected: [Float] = scenario.hasPrefix("reset-") ? [0.5, 0.25] : (scenario == "reader-pause" ? [0.5, 0.125, 0.25, 0.375, 0.625, 0.375, 0.75] : [0.5, 0.25, 0.375, 0.75])
        precondition(wav.count == expected.count * 480)
        for (i, sample) in wav.enumerated() { precondition(abs(sample - expected[i / 480]) < 0.001) }
        movie.release("audioLevel"); capture.destroy {}; other.destroy {}
        precondition(claims == 0 && engine == nil)
        print("PASS Apple mixed \(scenario), \(wav.count) WAV frames")
    }

    @MainActor static func suppliedMicrophone(_ directory: URL, origin: Int64, flushAtDisable: Bool) async throws {
        final class Input: ExactAudioSource {
            var held: (() -> Void)?
            func advance(to milliseconds: Double) { let deliver = held; held = nil; deliver?() }
            let origin: Int64
            init(_ origin: Int64) { self.origin = origin }
            let format = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 1)!
            var send: ((AVAudioPCMBuffer, AVAudioTime) -> Void)?
            func tap(_ block: @escaping (AVAudioPCMBuffer, AVAudioTime) -> Void) -> ExactSubscription {
                precondition(send == nil, "the microphone must have one capture tap")
                send = block
                return ExactSubscription { self.send = nil }
            }
            func packet(at first: Int64, count: Int) {
                let pcm = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(count))!
                pcm.frameLength = pcm.frameCapacity
                for i in 0..<count { pcm.floatChannelData![0][i] = (480..<960).contains(Int(first - origin) + i) ? 0.5 : 0 }
                send!(pcm, AVAudioTime(sampleTime: first, atRate: 48000))
            }
        }
        let tracks = ExactTracks(), input = Input(origin)
        var now = 0.0
        let context = ExactPackageContext(agent: true, session: "supplied", changed: { _ in }, log: { _ in }, now: { now }, tracks: tracks,
            file: { directory.appendingPathComponent(String($0.split(separator: "/").last!)) })
        let capture = MediaCaptureModule(context: context, audioSource: input, substituteOrigin: origin)
        let recording = RecordingModule(context: context)
        func call(_ module: ExactPackage, _ name: String, _ args: [String: Any]) async -> [String: Any] {
            await withCheckedContinuation { reply in module.later(name, args, preferred: nil) { reply.resume(returning: $0) } }
        }
        let stream = await call(capture, "getUserMedia", ["video": true, "audio": true])
        let mic = (stream["tracks"] as! [[String: Any]])[0]["id"] as! String
        tracks.add(ExactTrackEntry(id: "mic-only", kind: "stream", tracks: [mic]))
        let wav = await call(recording, "startMediaRecorder", ["stream": "mic-only", "path": "app:/data/supplied.wav"])
        var movie: [String: Any]?
        recording.later("startMediaRecorder", ["stream": stream["id"]!, "path": "app:/data/supplied.mp4"], preferred: nil) { movie = $0 }
        now = 10; input.packet(at: origin, count: 480)
        let deadline = ContinuousClock.now + .seconds(4)
        while movie == nil && ContinuousClock.now < deadline { try await Task.sleep(for: .milliseconds(2)) }
        precondition(movie?["id"] != nil)
        if flushAtDisable {
            now = 20
            input.held = { input.packet(at: origin + 480, count: 480) }
        }
        _ = await call(capture, "setTrackEnabled", ["track": mic, "enabled": false])
        now = 20
        _ = await call(capture, "setTrackEnabled", ["track": mic, "enabled": true])
        now = 30
        if flushAtDisable { input.packet(at: origin + 960, count: 480) }
        else { input.packet(at: origin + 480, count: 960) }
        for id in [movie!["id"]!, wav["id"]!] { _ = await call(recording, "stopMediaRecorder", ["recorder": id]) }
        for (file, count) in [("supplied.mp4", 1440), ("supplied.wav", 240)] {
            let samples = try decoded(directory.appendingPathComponent(file))
            precondition(samples.count == count && samples.allSatisfy { abs($0) < 0.001 })
        }
        print("PASS Apple mixed supplied microphone and WAV share delayed PCM")
    }

    @MainActor static func admission(_ directory: URL, scenario: String) async throws {
        var now = 0.0
        let context = ExactPackageContext(agent: true, session: "admission", changed: { _ in }, log: { _ in }, now: { now },
            file: { directory.appendingPathComponent(String($0.split(separator: "/").last!)) }, audioSession: ExactAudioSession { _, _, _, _ in {} })
        let capture = MediaCaptureModule(context: context), recording = RecordingModule(context: context)
        func call(_ module: ExactPackage, _ name: String, _ args: [String: Any]) async -> [String: Any] {
            await withCheckedContinuation { done in module.later(name, args, preferred: nil) { done.resume(returning: $0) } }
        }
        let options = scenario == "processed" ? ["echoCancellation", "noiseSuppression", "autoGainControl"] : [""]
        for option in options {
            var args: [String: Any] = ["audio": true, "video": true]
            if !option.isEmpty { args["audioConstraints"] = [option: true] }
            let stream = await call(capture, "getUserMedia", args)
            if scenario == "interrupted-foreground" {
                let track = (stream["tracks"] as! [[String: Any]]).first { $0["kind"] as? String == "audio" }!["id"]!
                let take = await call(recording, "startMediaRecorder", ["stream": stream["id"]!, "path": "app:/data/interrupted"])
                let args = ["recorder": take["id"]!]
                capture.heard("interrupted")
                context.foreground = false; capture.foreground(false)
                context.foreground = true; capture.foreground(true)
                let mic = await call(capture, "mediaStreamTrack", ["track": track])
                let state = await call(recording, "resumeMediaRecorder", args)
                precondition(mic["muted"] as? Bool == true && state["state"] as? String == "paused")
                _ = await call(recording, "stopMediaRecorder", args)
                continue
            }
            if ["disable-buffer", "enable-buffer", "standalone-disable"].contains(scenario) {
                let track = (stream["tracks"] as! [[String: Any]]).first { $0["kind"] as? String == "audio" }!["id"]!
                context.tracks.add(ExactTrackEntry(id: "audio", kind: "stream", tracks: [track as! String]))
                let wav = await call(recording, "startMediaRecorder", ["stream": "audio", "path": "app:/data/edge.wav"])
                var takes = [wav], files = ["edge.wav"]
                if scenario != "standalone-disable" {
                    takes.append(await call(recording, "startMediaRecorder", ["stream": stream["id"]!, "path": "app:/data/edge.mp4"]))
                    files.append("edge.mp4")
                }
                now = scenario == "enable-buffer" ? 20 : 5
                _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": false])
                if scenario == "enable-buffer" {
                    now = 30
                    _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": true])
                    now = 40
                }
                for take in takes { _ = await call(recording, "stopMediaRecorder", ["recorder": take["id"]!]) }
                for file in files {
                    let samples = try decoded(directory.appendingPathComponent(file))
                    let from = scenario == "enable-buffer" ? samples.count * 3 / 4 : 0
                    let peak = samples[from...].map(abs).max()!
                    print("PASS Apple mixed", scenario, file, samples.count, peak)
                    precondition(scenario == "standalone-disable" ? peak > 0.09 : peak < 0.001)
                }
                continue
            }
            if ["fractional-join", "short-wav", "fractional-substitute", "fractional-wav", "idle-wav"].contains(scenario) {
                let track = (stream["tracks"] as! [[String: Any]]).first { $0["kind"] as? String == "audio" }!["id"] as! String
                context.tracks.add(ExactTrackEntry(id: "mic-only", kind: "stream", tracks: [track]))
                now = ["fractional-join", "short-wav", "fractional-wav"].contains(scenario) ? 0.03125 : 0
                if scenario == "idle-wav" { _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": false]) }
                let wav = await call(recording, "startMediaRecorder", ["stream": "mic-only", "path": "app:/data/edge.wav"])
                if scenario == "fractional-join" {
                    now = 7.25 / 48
                    _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": false])
                    now = 7.75 / 48
                    let mixed = await call(recording, "startMediaRecorder", ["stream": stream["id"]!, "path": "app:/data/edge.mp4"])
                    now = 1
                    _ = await call(recording, "stopMediaRecorder", ["recorder": mixed["id"]!])
                } else if scenario == "short-wav" { now = 0.035 }
                else if scenario == "fractional-substitute" {
                    let mixed = await call(recording, "startMediaRecorder", ["stream": stream["id"]!, "path": "app:/data/edge.mp4"])
                    _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": false])
                    now = 48.25 / 48
                    _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": true])
                    now = 10
                    _ = await call(recording, "stopMediaRecorder", ["recorder": mixed["id"]!])
                } else { now = scenario == "idle-wav" ? 10 : 0.09375 }
                _ = await call(recording, "stopMediaRecorder", ["recorder": wav["id"]!])
                let samples = try decoded(directory.appendingPathComponent("edge.wav"))
                if scenario == "fractional-join" { precondition(samples[1] == 0) }
                else if ["short-wav", "fractional-wav"].contains(scenario) { precondition(samples == [271.0 / 32768]) }
                else if scenario == "idle-wav" { precondition(samples.count == 80 && samples.allSatisfy { $0 == 0 }) }
                else { precondition(samples[8] == 0 && samples[9] == 0) }
                continue
            }
            if scenario == "substitute-shared" {
                let track = (stream["tracks"] as! [[String: Any]]).first { $0["kind"] as? String == "audio" }!["id"] as! String
                context.tracks.add(ExactTrackEntry(id: "mic-only", kind: "stream", tracks: [track]))
                let wav = await call(recording, "startMediaRecorder", ["stream": "mic-only", "path": "app:/data/shared.wav"])
                let movie = await call(recording, "startMediaRecorder", ["stream": stream["id"]!, "path": "app:/data/mixed.mp4"])
                _ = await call(recording, "audioLevel", ["track": track])
                now = 50
                _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": false])
                now = 200
                let meter = await call(recording, "audioLevel", ["track": track])
                precondition(meter["audioLevel"] as? Double == 0)
                now = 250
                _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": true])
                now = 300
                _ = await call(recording, "stopMediaRecorder", ["recorder": movie["id"]!])
                _ = await call(recording, "stopMediaRecorder", ["recorder": wav["id"]!])
                var signals: [[Float]] = []
                for file in ["shared.wav", "mixed.mp4"] {
                    let url = directory.appendingPathComponent(file), af = try AVAudioFile(forReading: url)
                    let audio = try decoded(url), rate = Int(af.processingFormat.sampleRate)
                    precondition(audio.count == rate * 3 / 10)
                    precondition(audio[(rate / 10)..<(rate / 5)].allSatisfy { abs($0) < 0.001 })
                    precondition(audio[(rate / 4 + 128)...].contains { abs($0) > 0.1 })
                    signals.append(stride(from: 0, to: audio.count, by: rate / 8000).map { audio[$0] })
                    print(file, "frames", audio.count, "peak in muted interior", audio[(rate / 10)..<(rate / 5)].map(abs).max()!)
                }
                for i in 150..<2200 { precondition(abs(signals[0][i] - signals[1][i]) < 0.04) }
                continue
            }
            if scenario == "claimed-call" {
                let claim = context.audioSession.claimCall(video: false, events: { _ in })!
                let answer = await call(recording, "startMediaRecorder", ["stream": stream["id"]!, "path": "app:/data/call"])
                precondition(answer["error"] as? String == "NotSupportedError")
                claim.cancel()
                continue
            }
            if scenario == "inactive-camera" {
                let video = (stream["tracks"] as! [[String: Any]]).first { $0["kind"] as? String == "video" }!
                _ = await call(capture, "setTrackEnabled", ["track": video["id"]!, "enabled": false])
                let answer = await call(recording, "startMediaRecorder", ["stream": stream["id"]!, "path": "app:/data/unleased"])
                precondition(answer["error"] as? String == "NotReadableError")
                continue
            }
            if scenario == "background-press" {
                let audio = (stream["tracks"] as! [[String: Any]]).first { $0["kind"] as? String == "audio" }!
                _ = await call(recording, "audioLevel", ["track": audio["id"]!])
                context.foreground = false; context.origin = "press"
            }
            let track = (stream["tracks"] as! [[String: Any]]).first { $0["kind"] as? String == "audio" }!["id"]!
            if scenario == "substitute-mute" { _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": false]) }
            let start = await call(recording, "startMediaRecorder", ["stream": stream["id"]!, "path": "app:/data/take"])
            if scenario == "substitute-mute" {
                let id = ["recorder": start["id"]!]
                now = 75
                _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": true])
                now = 150
                _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": false])
                context.foreground = false; capture.foreground(false)
                let paused = await call(recording, "mediaRecorderState", id)
                precondition(paused["reason"] as? String == "background")
                now += 3000
                context.foreground = true; capture.foreground(true)
                let resumed = await call(recording, "resumeMediaRecorder", id)
                precondition(resumed["state"] as? String == "recording")
                now += 50
                _ = await call(capture, "setTrackEnabled", ["track": track, "enabled": true])
                now += 100
                let saved = await call(recording, "stopMediaRecorder", id)
                precondition(saved["duration"] as? Double == 300)
                let audio = try decoded(directory.appendingPathComponent("take.mp4"))
                let rate = 48000
                for frames in [0..<(rate * 75 / 1000), (rate * 15 / 100)..<(rate / 5)] {
                    precondition(audio[frames].allSatisfy { abs($0) < 0.001 }, "the substitute microphone leaked at a mute boundary")
                }
                precondition(audio[5000..<6000].contains { abs($0) > 0.1 } && audio[10000..<12000].contains { abs($0) > 0.1 })
            } else if scenario == "reset" {
                let id = ["recorder": start["id"]!]
                now = 50
                capture.heard("interrupted"); capture.heard("reset")
                let result = await call(recording, "stopMediaRecorder", id)
                let state = await call(recording, "mediaRecorderState", id)
                precondition(state["reason"] as? String == "media-reset" && state["state"] as? String == "inactive")
                precondition(result["mimeType"] as? String == "video/mp4")
                let url = directory.appendingPathComponent("take.mp4")
                try await inspectMixed(url)
                let audio = try decoded(url)
                precondition(result["duration"] as? Double == 50 && audio.contains { abs($0) > 0.08 }, "reset must keep the recorded prefix: \(result)")
            } else {
                let expected = scenario == "background-press" ? "NotAllowedError" : "NotSupportedError"
                precondition(start["error"] as? String == expected, "unexpected mixed admission: \(start)")
            }
        }
        print("PASS Apple mixed \(scenario)")
    }

    static func decoded(_ url: URL) throws -> [Float] {
        let file = try AVAudioFile(forReading: url)
        let decoded = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: AVAudioFrameCount(file.length))!
        try file.read(into: decoded)
        return Array(UnsafeBufferPointer(start: decoded.floatChannelData![0], count: Int(decoded.frameLength)))
    }

    static func packet(_ first: Int, _ count: Int, quiet: (Int) -> Bool = { _ in false }) throws -> CMSampleBuffer {
        let pcm = AVAudioPCMBuffer(pcmFormat: AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 1)!,
                                   frameCapacity: AVAudioFrameCount(count))!
        pcm.frameLength = pcm.frameCapacity
        for i in 0..<count {
            let frame = i + first
            pcm.floatChannelData![0][i] = quiet(frame) ? 0 : Float(0.45 * sin(Double(frame) * 2 * .pi * 440 / 48000))
        }
        return Writer.sample(pcm, at: Int64(first))!
    }

    static func pausedTail(_ directory: URL) async throws {
        var accepted = false, failure: String?
        let url = directory.appendingPathComponent("prefix.mp4")
        let w = MixedWriter(url, accepted: { _ in accepted = true }, failed: { failure = $0 })
        let t = { (n: Int) in CMTime(value: Int64(n), timescale: 48000) }
        w.configure(nil)
        for i in [0, 4800, 9600] {
            let picture = try MixedTake.agentPicture(i)
            w.queue.sync { w.video(picture) }
        }
        w.audio(try packet(10080, 2400))
        w.queue.sync {}
        precondition(accepted)
        w.pause(at: t(12480))
        // Stop while paused. A late packet in the paused interval cannot extend the file.
        w.pause(at: t(24000))
        w.audio(try packet(15000, 480))
        var at = 24000
        for _ in 0..<32 {
            w.resume(at: t(at))
            w.audio(try packet(at, 3840))
            w.queue.sync {}
            w.pause(at: t(at + 3840))
            at += 4800
        }
        w.queue.sync {}
        precondition(failure == nil)
        let error: String? = await withCheckedContinuation { done in
            w.finish(publish: true) { message, _ in done.resume(returning: message) }
        }
        precondition(error == nil)
        let samples = try decoded(url)
        precondition(samples.count == 2400)
        let asset = AVURLAsset(url: url), reader = try AVAssetReader(asset: asset)
        let track = try await asset.loadTracks(withMediaType: .video)[0]
        let video = AVAssetReaderTrackOutput(track: track, outputSettings: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
        reader.add(video); precondition(reader.startReading())
        let pixel = video.copyNextSampleBuffer()!.imageBuffer!
        CVPixelBufferLockBaseAddress(pixel, .readOnly)
        let channel = CVPixelBufferGetBaseAddress(pixel)!.load(as: UInt8.self)
        CVPixelBufferUnlockBaseAddress(pixel, .readOnly)
        precondition(channel > 175, "the first picture must be the frame covering the first audio sample, got \(channel)")
        print("PASS Apple mixed first picture, paused tail and short segments")
    }

    static func pressure(_ directory: URL) async throws {
        var blocked = false, error: String?
        let writer = MixedWriter(directory.appendingPathComponent("pressure.mp4"),
            ready: { !blocked && $0.isReadyForMoreMediaData }, accepted: { _ in }, failed: { error = $0 })
        writer.configure(nil)
        let frame = try MixedTake.agentPicture(0)
        writer.queue.sync { writer.video(frame) }
        writer.audio(try packet(0, 480))
        writer.queue.sync { blocked = true; writer.video(frame) }
        for i in 1...200 { writer.audio(try packet(i * 480, 480)); writer.queue.sync {} }
        precondition(error == nil)
        writer.audio(try packet(201 * 480, 480))
        writer.queue.sync {}
        precondition(error == "the writer fell behind")
        let drops: Int = await withCheckedContinuation { done in writer.finish(publish: true) { _, n in done.resume(returning: n) } }
        precondition(drops == 1)
        let prefix = directory.appendingPathComponent("pressure.mp4")
        try await inspectMixed(prefix)
        let kept = try decoded(prefix)
        precondition(kept.count == 480)

        let url = directory.appendingPathComponent("dropped.mp4")
        let timing = MixedWriter(url, ready: { $0.mediaType != .video || !blocked ? $0.isReadyForMoreMediaData : false },
                                 accepted: { _ in }, failed: { preconditionFailure($0) })
        timing.configure(nil)
        for i in 0..<4 {
            let pic = try MixedTake.agentPicture(i * 4800)
            timing.audio(try packet(i * 4800, 4800))
            timing.queue.sync { blocked = i == 1; timing.video(pic) }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        let lost: Int = await withCheckedContinuation { done in timing.finish(publish: true) { error, n in
            precondition(error == nil); done.resume(returning: n)
        } }
        precondition(lost == 1)
        let asset = AVURLAsset(url: url)
        let reader = try AVAssetReader(asset: asset)
        let track = try await asset.loadTracks(withMediaType: .video)[0]
        let frames = AVAssetReaderTrackOutput(track: track, outputSettings: nil)
        reader.add(frames); precondition(reader.startReading())
        var positions: [Int] = []
        while let sample = frames.copyNextSampleBuffer() {
            if sample.numSamples > 0 { positions.append(Int((sample.presentationTimeStamp.seconds * 1000).rounded())) }
        }
        precondition(positions == [0, 200, 300], "\(positions)")

        print("PASS Apple mixed two-second audio bound and counted video drop")
    }

    static func fractional(_ dir: URL) async throws {
        let url = dir.appendingPathComponent("fractional.mp4")
        let writer = MixedWriter(url, accepted: { _ in }, failed: { preconditionFailure($0) })
        writer.configure(nil)
        let source = try MixedTake.agentPicture(0)
        var timing = CMSampleTimingInfo(duration: source.duration, presentationTimeStamp: CMTime(value: 1, timescale: 192000), decodeTimeStamp: .invalid)
        var picture: CMSampleBuffer?
        precondition(CMSampleBufferCreateCopyWithNewTiming(allocator: nil, sampleBuffer: source, sampleTimingEntryCount: 1,
                                                           sampleTimingArray: &timing, sampleBufferOut: &picture) == noErr)
        writer.queue.sync { writer.video(picture!) }
        writer.audio(try packet(0, 4800))
        await withCheckedContinuation { done in writer.finish(publish: true) { error, _ in precondition(error == nil); done.resume() } }
        try await inspectMixed(url)
        let samples = try decoded(url)
        precondition(samples.count == 4799)
    }

    static func asymmetric(_ dir: URL) async throws {
        for lag in [0, 9600] {
            var accepted = false
            let w = MixedWriter(dir.appendingPathComponent("skew-\(lag).mp4"), accepted: { _ in accepted = true }, failed: { print($0) })
            w.configure(nil)
            for tick in stride(from: 0, to: 48000 * 6, by: 480) {
                if tick % 4800 == 0 {
                    let pic = try MixedTake.agentPicture(tick)
                    w.queue.sync { w.video(pic) }
                }
                if tick >= lag {
                    w.audio(try packet(tick - lag, 480)); w.queue.sync {}
                }
            }
            precondition(accepted)
            print("PASS Apple mixed direct writer: six seconds delivered; PCM-only delay in samples",lag,"admitted",accepted)
            await withCheckedContinuation { done in w.finish(publish: accepted) { error, _ in precondition(error == nil); done.resume() } }
            let asset = AVURLAsset(url: dir.appendingPathComponent("skew-\(lag).mp4"))
            let reader = try AVAssetReader(asset: asset)
            let track = try await asset.loadTracks(withMediaType: .video)[0]
            let frames = AVAssetReaderTrackOutput(track: track, outputSettings: nil)
            reader.add(frames); precondition(reader.startReading())
            var last = CMTime.zero
            while let sample = frames.copyNextSampleBuffer() {
                if sample.numSamples > 0 { last = sample.presentationTimeStamp }
            }
            precondition(last.seconds >= 5.5, "last picture at \(last.seconds)")

        }
    }
    static func retained(_ dir: URL) async throws {
        final class Ref { weak var buffer: AnyObject?; init(_ buffer: CMSampleBuffer) { self.buffer = buffer } }
        let w = MixedWriter(dir.appendingPathComponent("paused-pictures.mp4"), accepted: { _ in }, failed: { print($0) })
        w.configure(nil); var pending = [Ref]()
        for i in 0..<32 {
            try autoreleasepool {
                let t = i * 960
                if i > 0 { w.resume(at: CMTime(value: Int64(t), timescale: 48000)) }
                let a = try MixedTake.agentPicture(t)
                w.queue.sync { w.video(a) }
                w.audio(try packet(t, 480))
                let tail = try MixedTake.agentPicture(t + 480)
                w.queue.sync { w.video(tail) }
                pending.append(Ref(tail))
                w.pause(at: CMTime(value: Int64(t + 480), timescale: 48000))
            }
        }
        w.queue.sync {}
        precondition(pending.allSatisfy { $0.buffer == nil }, "retained \(pending.filter { $0.buffer != nil }.count)")
        print("PASS Apple mixed released the endpoint pictures at 32 pauses")
        await withCheckedContinuation { done in w.finish(publish: true) { _,_ in done.resume() } }
    }
    static func finishPressure(_ directory: URL) async throws {
        var blocked = false
        let url = directory.appendingPathComponent("tail.mp4")
        let writer = MixedWriter(url, ready: { $0.isReadyForMoreMediaData && ($0.mediaType != .audio || !blocked) },
                                 accepted: { _ in }, failed: { print("capture failure", $0) })
        writer.configure(nil)
        let pic = try MixedTake.agentPicture(0)
        writer.queue.sync { writer.video(pic) }
        writer.audio(try packet(0, 4800))
        writer.queue.sync { blocked = true }
        writer.audio(try packet(4800, 480))
        writer.queue.sync {}
        writer.queue.asyncAfter(deadline: .now() + .milliseconds(400)) { blocked = false }
        let result: String? = await withCheckedContinuation { done in
            writer.finish(publish: true) { error, _ in done.resume(returning: error) }
        }
        let samples = try decoded(url)
        precondition(samples.count == 5280 && result == nil)
        print("PASS Apple mixed stop with 10 ms pending, input recovers after 400 ms; expected 5280 frames; decoded", samples.count, "error", result ?? "nil")
    }

    @MainActor static func main() async throws {
        if CommandLine.arguments.count > 2 {
            let dir = URL(fileURLWithPath: CommandLine.arguments[1])
            switch CommandLine.arguments[2] {
            case "fractional-join", "short-wav", "disable-buffer", "enable-buffer", "standalone-disable", "idle-wav", "fractional-substitute", "fractional-wav", "reset", "interrupted-foreground", "substitute-shared", "substitute-mute", "background-press", "processed", "claimed-call", "inactive-camera":
                try await admission(dir, scenario: CommandLine.arguments[2]); return
            case "supplied-microphone", "offset-microphone", "flush-at-disable":
                try await suppliedMicrophone(dir, origin: CommandLine.arguments[2] == "offset-microphone" ? 480 : 0,
                                             flushAtDisable: CommandLine.arguments[2] == "flush-at-disable"); return
            case "clock-discontinuities": try await discontinuities(dir); return
            case "disjoint": try await disjoint(dir); return
            case "paused-picture": try await lostPicture(dir); return
            case "proof-order": try await proofOrder(dir); return
            case "clock-return": try await lostClock(dir); return
            case "clock-lag": try await clockLag(dir); return
            case "fractional": try await fractional(dir); return
            case "skew": try await asymmetric(dir); return
            case "retained": try await retained(dir); return
            case "finish-pressure": try await finishPressure(dir); return
            case "queued-edge": try await queuedEdge(dir); return
            case "paused-tail": try await pausedTail(dir); return
            case "pressure": try await pressure(dir); return
            case "interruption-reader", "restored-reader", "port-change", "early-audio", "early-pts", "prehistory", "recovery-handoff", "old-engine", "no-audio", "reset-blocked", "max-prefix", "clock-missing", "clock-present", "staging", "wrong-port", "inactive-pcm", "late-inactive", "pending-interrupt", "pending-cancel", "cancel-0", "cancel-1", "cancel-2",
                 "background-allowed", "overlap-lease", "same-background", "muted-readers", "reader-pause", "readers", "detach-lease", "reset-sdk", "reset-unrelated", "reset-session":
                try await mixedSession(dir, scenario: CommandLine.arguments[2]); return
            case "inspect": try await inspectMixed(dir); return
            default: break
            }
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
