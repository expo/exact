import AVFoundation
import ExactPackage
import Foundation

protocol MediaTake: AnyObject {
    var id: String { get }
    var state: String { get }
    func info() -> [String: Any]
    func pause(_ why: String)
    func resume()
    func finish(_ why: String, error: String?, _ done: @escaping ([String: Any]) -> Void)
}

extension MediaTake {
    func finish(_ why: String, _ done: @escaping ([String: Any]) -> Void) { finish(why, error: nil, done) }
}

private final class VideoSlot { var busy = false }

/// A movie output attached beside the viewfinder and a package's frame output on the track's session.
final class VideoTake: NSObject, MediaTake, ExactTrackConsumer, AVCaptureFileOutputRecordingDelegate {
    let id: String
    private let path: String, url: URL, temporary: URL, context: ExactPackageContext, maxDuration: Double, changed: () -> Void
    private(set) var state = "recording"
    private var reason: String?, failure: String?, result: [String: Any]?, answer: [String: Any]?
    private var recorded = 0.0, since: Double
    private var limit: (() -> Void)?
    private var source: ExactVideoSource?
    private var slot: VideoSlot?
    private var output: AVCaptureMovieFileOutput?
    private var preset: AVCaptureSession.Preset?
    private var ready = false, started = false
    private var finishing = false
    private var waiting: [([String: Any]) -> Void] = []
    var subscription: ExactSubscription?

    init(id: String, path: String, url: URL, context: ExactPackageContext, maxDuration: Double, changed: @escaping () -> Void) {
        self.id = id; self.path = path; self.url = url; self.context = context; self.maxDuration = maxDuration; self.changed = changed
        temporary = url.deletingLastPathComponent().appendingPathComponent(".\(id).mov")
        since = context.now()
        super.init()
    }

    var supportsPause: Bool {
        if context.agent { return true }
        if #available(iOS 18, macOS 10.7, *) { return true }
        return false
    }

    private var duration: Double { recorded + (state == "recording" ? max(0, context.now() - since) : 0) }

    func arm() {
        limit?(); limit = nil
        if context.agent, maxDuration > 0, state == "recording" {
            limit = context.after(max(0, maxDuration - duration)) { [weak self] in self?.finish("max-duration") { _ in } }
        }
    }

    func info() -> [String: Any] {
        var value: [String: Any] = ["id": id, "state": state, "mimeType": "video/mp4", "path": path, "duration": duration]
        value["reason"] = reason; value["error"] = failure; value["result"] = result
        return value
    }

    func live(video source: ExactVideoSource?) {
        guard !context.agent, let source, output == nil, state != "inactive" else { return }
        let slot = source.shared("recording.video") { VideoSlot() }
        guard !slot.busy else {
            DispatchQueue.main.async { self.complete(error: "NotReadableError: the camera already has a recorder") }
            return
        }
        slot.busy = true; self.slot = slot
        let output = AVCaptureMovieFileOutput()
        self.source = source; self.output = output
        if maxDuration > 0 { output.maxRecordedDuration = CMTime(seconds: maxDuration / 1000, preferredTimescale: 60000) }
        var rate = 0.0
        source.configure { session in
            rate = source.device.activeVideoMinFrameDuration.seconds
            if session.sessionPreset == .photo, session.canSetSessionPreset(.high) {
                self.preset = session.sessionPreset
                session.sessionPreset = .high
            }
            if session.canAddOutput(output) { session.addOutput(output) }
        }
        var angle: CGFloat?
        if #available(iOS 17, macOS 14, *) {
            angle = AVCaptureDevice.RotationCoordinator(device: source.device, previewLayer: nil).videoRotationAngleForHorizonLevelCapture
        }
        source.run { [self] session in
            if rate > 0 { source.apply(["frameRate": 1 / rate]) }
            guard session.outputs.contains(output), let connection = output.connection(with: .video), connection.isActive else {
                DispatchQueue.main.async { self.complete(error: "NotSupportedError: the camera cannot attach a movie output") }
                return
            }
            Self.orient(connection, angle: angle)
            DispatchQueue.main.async { self.ready = true; self.start() }
        }
    }

    private func start() {
        guard ready, !started, state == "recording", let output, let source else { return }
        started = true
        source.run { [self] _ in
            do {
                try FileManager.default.createDirectory(at: temporary.deletingLastPathComponent(), withIntermediateDirectories: true)
                try? FileManager.default.removeItem(at: temporary)
                output.startRecording(to: temporary, recordingDelegate: self)
            } catch {
                DispatchQueue.main.async { self.complete(error: "the file did not open: \(error)") }
            }
        }
    }

    func pause(_ why: String) {
        guard state == "recording" else { return }
        guard supportsPause else { return finish(why) { _ in } }
        recorded = duration; state = "paused"; reason = why
        limit?(); limit = nil
        if started, let output, #available(iOS 18, macOS 10.7, *) { source?.run { _ in output.pauseRecording() } }
        changed()
    }

    func resume() {
        guard state == "paused" else { return }
        state = "recording"; reason = nil; since = context.now()
        if started, let output, #available(iOS 18, macOS 10.7, *) { source?.run { _ in output.resumeRecording() } }
        else { start() }
        arm()
        changed()
    }

    func muted(_ why: String) { if why != "idle" { pause(why == "background" ? why : "interrupted") } }
    func unmuted() {}
    func ended() { finish("track-ended") { _ in } }

    func finish(_ why: String, error: String? = nil, _ done: @escaping ([String: Any]) -> Void) {
        if let answer { return done(answer) }
        waiting.append(done)
        guard !finishing else { return }
        finishing = true
        recorded = duration; state = "inactive"; reason = why; failure = error
        limit?(); limit = nil
        if context.agent {
            do {
                try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
                try Self.clip.write(to: url, options: .atomic)
                complete(metadata: ["duration": 300, "width": 16, "height": 16, "frameRate": 10])
            } catch { complete(error: "the file was not written: \(error)") }
        } else if started, let output {
            source?.run { _ in output.stopRecording() }
        } else { complete(error: error ?? "the camera supplied no video", errorCode: error == nil ? "NotReadableError" : "UnknownError") }
        changed()
    }

    func fileOutput(_ output: AVCaptureFileOutput, didFinishRecordingTo fileURL: URL, from connections: [AVCaptureConnection], error: Error?) {
        let error = error as NSError?
        let bounded = error?.code == AVError.maximumDurationReached.rawValue
        let usable = error == nil || bounded || error?.userInfo[AVErrorRecordingSuccessfullyFinishedKey] as? Bool == true
        DispatchQueue.main.async { [self] in
            recorded = output.recordedDuration.seconds * 1000
            state = "inactive"
            finishing = true
            if bounded { reason = "max-duration" }
            guard usable else { return complete(error: error?.localizedDescription ?? "recording failed") }
            let asset = AVURLAsset(url: fileURL)
            // MovieFileOutput writes QuickTime; passthrough remuxes its platform codec into the promised MP4.
            guard let export = AVAssetExportSession(asset: asset, presetName: AVAssetExportPresetPassthrough) else {
                return complete(error: "the movie could not be exported")
            }
            try? FileManager.default.removeItem(at: url)
            export.outputURL = url; export.outputFileType = .mp4
            export.exportAsynchronously { [self] in
                Task {
                    do {
                        guard export.status == .completed else { throw export.error ?? ExactTrackError("UnknownError") }
                        let metadata = try await Self.metadata(url)
                        DispatchQueue.main.async { self.complete(metadata: metadata) }
                    } catch { DispatchQueue.main.async { self.complete(error: "the movie was not written: \(error)") } }
                }
            }
        }
    }

    static func metadata(_ url: URL) async throws -> [String: Any] {
        let asset = AVURLAsset(url: url)
        guard let video = try await asset.loadTracks(withMediaType: .video).first else { throw ExactTrackError("NotReadableError") }
        let (naturalSize, transform) = try await video.load(.naturalSize, .preferredTransform)
        let size = CGRect(origin: .zero, size: naturalSize).applying(transform).size
        return ["duration": try await asset.load(.duration).seconds * 1000, "width": abs(size.width), "height": abs(size.height),
                "frameRate": Double(try await video.load(.nominalFrameRate))]
    }

    static func orient(_ connection: AVCaptureConnection, angle: CGFloat?) {
        if #available(iOS 17, macOS 14, *), let angle, connection.isVideoRotationAngleSupported(angle) {
            connection.videoRotationAngle = angle
        }
    }

    private func complete(metadata: [String: Any] = [:], error: String? = nil, errorCode: String = "UnknownError") {
        guard answer == nil else { return }
        state = "inactive"; reason = reason ?? "track-ended"; failure = failure ?? error
        limit?(); limit = nil
        if let output, let source {
            var rate = 0.0
            source.configure { session in
                rate = source.device.activeVideoMinFrameDuration.seconds
                if session.outputs.contains(output) { session.removeOutput(output) }
                if let preset = self.preset, session.canSetSessionPreset(preset) { session.sessionPreset = preset }
            }
            source.run { _ in if rate > 0 { source.apply(["frameRate": 1 / rate]) } }
        }
        output = nil; source = nil
        slot?.busy = false; slot = nil
        subscription?.cancel(); subscription = nil
        try? FileManager.default.removeItem(at: temporary)
        if let failure {
            reason = "error"
            answer = ["error": errorCode, "message": failure]
        } else {
            var media = metadata
            media["path"] = path; media["mimeType"] = "video/mp4"
            media["size"] = (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? NSNumber)?.intValue ?? 0
            result = media; answer = media
        }
        changed()
        let callbacks = waiting; waiting.removeAll()
        for callback in callbacks { callback(answer!) }
    }
}

extension VideoTake {
    // The agent's three-frame MP4 from expo/exact#752, 16×16 at 10 fps.
    static let clip = Data(base64Encoded: "AAAAHGZ0eXBtcDQyAAAAAWlzb21tcDQxbXA0MgAAAAFtZGF0AAAAAAAAAUAAAAA1BgUtR1ZK3FxMQz+U78URPNFDqAEAAAMAAQMAAAMAAQIAAeYACwAAAwAAAwABBG4MA5ErAYAAAAA5JbggH94I48b/gn3JiSzDkGtC7AAVHn0Yh2pXegoZHua5g6mhDpEeQnKvseY9w7tuUUAAdYAmZTIwAAAAGgYFFUdWStxcTEM/lO/FETzRQ6gDAAADAAGAAAAAOiW4EAf/3gjjxv+D0JieB2A35Cv0AEQgaTZvNHVF8uS3pHtcpBeHl892l/d7Yzk4xDwcAAVkAUMhoMAAAAAaBgUVR1ZK3FxMQz+U78URPNFDqAMAAAMAAYAAAAA8JbgYB//eCOPG/4LEumJ5DOTohdgAKj26BtZwTUYAOG3eiijluVy/iqDvKq3e0V1TETcb2NoASEAdAcN2AAACtW1vb3YAAABsbXZoZAAAAADnIZ8Q5yGfEQAAAlgAAAC0AAEAAAEAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAJBdHJhawAAAFx0a2hkAAAAAechnxHnIZ8RAAAAAQAAAAAAAAC0AAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAQAAAAAAQAAAAEAAAAAAAJGVkdHMAAAAcZWxzdAAAAAAAAAABAAAAtAAAAAAAAQAAAAABuW1kaWEAAAAgbWRoZAAAAADnIZ8R5yGfEQAAAlgAAAC0VcQAAAAAADFoZGxyAAAAAAAAAAB2aWRlAAAAAAAAAAAAAAAAQ29yZSBNZWRpYSBWaWRlbwAAAAFgbWluZgAAABR2bWhkAAAAAQAAAAAAAAAAAAAAJGRpbmYAAAAcZHJlZgAAAAAAAAABAAAADHVybCAAAAABAAABIHN0YmwAAAChc3RzZAAAAAAAAAABAAAAkWF2YzEAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAEAAQAEgAAABIAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY//8AAAAnYXZjQwFkAAv/4QAMJ2QAC6xXDDeBZgpQAQAEKO48sP34+AAAAAAKZmllbAEAAAAACmNocm0AAAAAABhzdHRzAAAAAAAAAAEAAAADAAAAPAAAAA9zZHRwAAAAACgoKAAAABxzdHNjAAAAAAAAAAEAAAABAAAAAwAAAAEAAAAgc3RzegAAAAAAAAAAAAAAAwAAAHYAAABcAAAAXgAAABRzdGNvAAAAAAAAAAEAAAAs")!
}

/// The capture clock labels both data outputs. Each resumed segment begins with the
/// first picture and overlapping sound. Its origin follows the previous PCM sample.
final class MixedWriter: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate {
    let queue = DispatchQueue(label: "exact.recording.mixed-writer")
    private let url: URL
    private let accepted: (Double) -> Void, failed: (String) -> Void
    private let maxDuration: Double, reachedLimit: () -> Void
    private var notifiedLimit = false
    private let ready: (AVAssetWriterInput) -> Bool
    private var asset: AVAssetWriter?, picture: AVAssetWriterInput?, sound: AVAssetWriterInput?
    private var settings: [String: Any]?
    private var canOpen = false, answered = false, closed = false
    private var failure: String?
    private var audioEnd = CMTime.zero, writtenEnd = CMTime.zero, lastPicture = CMTime.invalid
    private(set) var dropped = 0
    private var backlog: [(pcm: CMSampleBuffer, time: CMTime)] = []
    private var retry: DispatchWorkItem?
    private let ingress = NSLock()
    private static let pressureLimit = 2.0
    private var queuedSeconds = 0.0, overloaded = false

    private final class Segment {
        var start: CMTime
        var end = CMTime.positiveInfinity
        var origin: CMTime?, offset = CMTime.zero, audioEnd = CMTime.zero
        var pictures: [CMSampleBuffer] = []
        var firstPicture: CMSampleBuffer?
        var earlyAudio: [CMSampleBuffer] = []
        init(_ start: CMTime) { self.start = start }
    }
    private var segments = [Segment(.negativeInfinity)]
    var mute = ExactAudioMute()

    init(_ url: URL, maxDuration: Double = 0, reachedLimit: @escaping () -> Void = {}, ready: @escaping (AVAssetWriterInput) -> Bool = { $0.isReadyForMoreMediaData },
         accepted: @escaping (Double) -> Void, failed: @escaping (String) -> Void) {
        self.maxDuration = maxDuration; self.reachedLimit = reachedLimit
        self.url = url; self.ready = ready; self.accepted = accepted; self.failed = failed
    }

    func configure(_ settings: [String: Any]?) {
        queue.async {
            self.settings = settings; self.canOpen = true
            self.perform { try self.begin(self.segments[0]) }
        }
    }

    func pause(at time: CMTime) {
        queue.async {
            guard let segment = self.segments.last else { return }
            segment.end = min(segment.end, time.isNumeric ? time : (segment.origin.map { $0 + segment.audioEnd - segment.offset } ?? segment.start))
            segment.pictures.removeAll { $0.presentationTimeStamp >= segment.end }
            if let first = segment.firstPicture, first.presentationTimeStamp >= segment.end { segment.firstPicture = nil }
        }
    }
    func resume(at time: CMTime) {
        queue.async {
            if let previous = self.segments.last, previous.origin == nil && previous.firstPicture == nil {
                for pcm in previous.earlyAudio { self.release(pcm.duration.seconds) }
                previous.earlyAudio.removeAll()
            }
            self.segments.append(Segment(time.isNumeric ? time : .positiveInfinity))
        }
    }

    private func perform(_ work: () throws -> Void) {
        guard !closed, failure == nil else { return }
        do { try work() } catch {
            failure = String(describing: error)
            failed(failure!)
        }
    }

    func captureOutput(_ output: AVCaptureOutput, didDrop sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        dropped += 1
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        video(sampleBuffer)
    }

    /// On the video delegate queue, which is also the writer queue.
    func video(_ buffer: CMSampleBuffer) {
        perform {
            let t = buffer.presentationTimeStamp
            for previous in segments where previous.end <= t && previous.firstPicture == nil && previous.origin == nil {
                for pcm in previous.earlyAudio { release(pcm.duration.seconds) }
                previous.earlyAudio.removeAll()
            }
            guard let segment = segments.last(where: { $0.start <= t && t < $0.end }) else { return }
            try flush()
            if segment.firstPicture == nil && segment.origin == nil { segment.firstPicture = buffer }
            else {
                // Keep the oldest unmatched pictures through delegate skew, within a bounded pixel-buffer pool.
                if segment.pictures.count == 8 { segment.pictures.removeLast(); dropped += 1 }
                segment.pictures.append(buffer)
            }
            try begin(segment)
            try flush()
        }
    }

    /// Charge retained capture buffers before crossing queues. The audio delegate can
    /// continue delivering to other readers while the encoder is busy.
    @discardableResult func audio(_ input: CMSampleBuffer, reading: @autoclosure () -> ExactAudioMute.Reading) -> CMSampleBuffer? {
        guard let desc = input.formatDescription else { return nil }
        let format = AVAudioFormat(cmAudioFormatDescription: desc)
        var now = CMTime.invalid
        func stamp() -> ExactAudioMute.Reading {
            let current = reading()
            now = current.time
            return current
        }
        guard let buffer = masked(input, format, through: stamp()) else {
            queue.async { self.perform { throw CaptureFailure("could not mute microphone samples") } }
            return nil
        }
        appendAudio(buffer, rate: format.sampleRate, through: now)
        return buffer
    }

    func appendAudio(_ buffer: CMSampleBuffer, rate: Double, through clock: CMTime = .invalid) {
        let seconds = Double(buffer.numSamples) / rate
        ingress.lock()
        guard !overloaded else { ingress.unlock(); return }
        if queuedSeconds + seconds > Self.pressureLimit + 1e-8 {
            overloaded = true
            ingress.unlock()
            queue.async { self.perform { throw CaptureFailure("the writer fell behind") } }
            return
        }
        queuedSeconds += seconds
        ingress.unlock()
        queue.async {
            self.perform {
                if let segment = self.segments.last, segment.start == .positiveInfinity, clock.isNumeric {
                    segment.start = max(clock, buffer.presentationTimeStamp + buffer.duration)
                }
                try self.consume(buffer, seconds: seconds)
            }
        }
    }

    private func masked(_ sample: CMSampleBuffer, _ format: AVAudioFormat, through clock: @autoclosure () -> ExactAudioMute.Reading) -> CMSampleBuffer? {
        let start = sample.presentationTimeStamp, count = sample.numSamples
        let cuts = mute.silence(from: start, frames: count, rate: Int32(format.sampleRate), through: clock())
        guard !cuts.isEmpty else { return sample }
        guard let pcm = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(count)) else { return nil }
        pcm.frameLength = pcm.frameCapacity
        guard CMSampleBufferCopyPCMDataIntoAudioBufferList(sample, at: 0, frameCount: Int32(count), into: pcm.mutableAudioBufferList) == noErr else { return nil }
        for cut in cuts { Self.silence(pcm, cut.lowerBound, cut.upperBound) }
        guard let masked = Writer.sample(pcm, at: 0) else { return nil }
        return try? Self.retime(masked, start)
    }

    private func release(_ seconds: Double) { ingress.withLock { queuedSeconds -= seconds } }
    private func consume(_ buffer: CMSampleBuffer, seconds: Double) throws {
        let t = buffer.presentationTimeStamp
        guard let desc = buffer.formatDescription else { throw CaptureFailure("missing PCM format") }
        let rate = Int32(AVAudioFormat(cmAudioFormatDescription: desc).sampleRate)
        let end = t + CMTime(value: Int64(buffer.numSamples), timescale: rate)
        var unused = seconds
        defer { release(unused) }
        for segment in segments where segment.start < end && t < segment.end {
            func index(_ edge: CMTime) -> Int {
                guard edge > t else { return 0 }
                guard edge < end else { return buffer.numSamples }
                return Int(CMTimeConvertScale(edge - t, timescale: rate, method: .roundTowardPositiveInfinity).value)
            }
            let first = index(segment.start), last = index(segment.end)
            guard last > first else { continue }
            let part = try Self.slice(buffer, first: first, count: last - first)
            unused -= part.duration.seconds
            if segment.origin != nil { try enqueue(part, segment, rate) }
            else {
                segment.earlyAudio.append(part)
                try begin(segment)
            }
        }
        try flush()
    }

    private static func silence(_ buffer: AVAudioPCMBuffer, _ first: Int, _ last: Int) {
        guard last > first else { return }
        let stride = Int(buffer.format.streamDescription.pointee.mBytesPerFrame)
        for plane in UnsafeMutableAudioBufferListPointer(buffer.mutableAudioBufferList) {
            if let data = plane.mData { memset(data + first * stride, 0, (last - first) * stride) }
        }
    }

    private func begin(_ segment: Segment) throws {
        guard canOpen, segment.origin == nil, let frame = segment.firstPicture,
              let audio = segment.earlyAudio.first, let desc = audio.formatDescription else { return }
        let start = max(frame.presentationTimeStamp, audio.presentationTimeStamp)
        let rate = Int32(AVAudioFormat(cmAudioFormatDescription: desc).sampleRate)
        guard let overlapping = segment.earlyAudio.first(where: { $0.presentationTimeStamp + $0.duration > start }) else {
            for sample in segment.earlyAudio {
                release(sample.duration.seconds)
                if asset == nil { try discardBeforePicture(sample.duration.seconds) }
            }
            segment.earlyAudio.removeAll()
            return
        }
        let offset = CMTimeConvertScale(max(.zero, start - overlapping.presentationTimeStamp), timescale: rate, method: .roundTowardPositiveInfinity)
        let origin = overlapping.presentationTimeStamp + offset
        if frame.duration.isNumeric, frame.duration > .zero, origin >= frame.presentationTimeStamp + frame.duration {
            segment.firstPicture = segment.pictures.isEmpty ? nil : segment.pictures.removeFirst(); dropped += 1
            return try begin(segment)
        }
        if asset == nil {
            for pcm in segment.earlyAudio {
                try discardBeforePicture(min(pcm.duration.seconds, max(0, (origin - pcm.presentationTimeStamp).seconds)))
            }
            try open(frame, audio: desc)
        }
        segment.origin = origin
        segment.offset = audioEnd
        for sample in segment.earlyAudio {
            try enqueue(sample, segment, rate)
        }
        segment.earlyAudio.removeAll()
        try flush()
    }

    private func open(_ frame: CMSampleBuffer, audio: CMFormatDescription) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        writer.movieFragmentInterval = CMTime(value: 1, timescale: 1)
        guard let desc = frame.formatDescription, let stream = CMAudioFormatDescriptionGetStreamBasicDescription(audio)?.pointee else {
            throw CaptureFailure("the capture supplied no format")
        }
        let size = CMVideoFormatDescriptionGetDimensions(desc)
        var video = settings ?? [AVVideoCodecKey: AVVideoCodecType.h264, AVVideoWidthKey: Int(size.width), AVVideoHeightKey: Int(size.height)]
        // A fragment can be decoded once the following key frame closes it.
        var compression = video[AVVideoCompressionPropertiesKey] as? [String: Any] ?? [:]
        compression[AVVideoMaxKeyFrameIntervalDurationKey] = 1.0
        compression[AVVideoAllowFrameReorderingKey] = false
        video[AVVideoCompressionPropertiesKey] = compression
        let picture = AVAssetWriterInput(mediaType: .video, outputSettings: video, sourceFormatHint: desc)
        let sound = AVAssetWriterInput(mediaType: .audio, outputSettings: [AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: stream.mSampleRate, AVNumberOfChannelsKey: stream.mChannelsPerFrame,
            AVEncoderAudioQualityKey: AVAudioQuality.max.rawValue,
            AVEncoderBitRateKey: min(512000, min(224000, Int(stream.mSampleRate) * 6) * Int(stream.mChannelsPerFrame))], sourceFormatHint: audio)
        for input in [picture, sound] {
            input.expectsMediaDataInRealTime = true
            guard writer.canAdd(input) else { throw CaptureFailure("the MP4 cannot encode the captured format") }
            writer.add(input)
        }
        guard writer.startWriting() else { throw writer.error ?? CaptureFailure("the MP4 did not open") }
        writer.startSession(atSourceTime: .zero)
        asset = writer; self.sound = sound; self.picture = picture
    }

    private func appendVideo(_ sample: CMSampleBuffer, at time: CMTime) throws {
        guard let picture else { return }
        guard ready(picture), !lastPicture.isNumeric || lastPicture < time else { dropped += 1; return }
        guard picture.append(try Self.retime(sample, time)) else { throw asset?.error ?? CaptureFailure("the writer refused video") }
        lastPicture = time
    }

    static func slice(_ sample: CMSampleBuffer, first: Int, count: Int) throws -> CMSampleBuffer {
        if first == 0 && count == sample.numSamples { return sample }
        guard let desc = sample.formatDescription else { throw CaptureFailure("missing PCM format") }
        let format = AVAudioFormat(cmAudioFormatDescription: desc)
        guard let pcm = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(count)) else {
            throw CaptureFailure("could not allocate microphone samples")
        }
        pcm.frameLength = pcm.frameCapacity
        guard CMSampleBufferCopyPCMDataIntoAudioBufferList(sample, at: Int32(first), frameCount: Int32(count),
                  into: pcm.mutableAudioBufferList) == noErr, let part = Writer.sample(pcm, at: 0) else {
            throw CaptureFailure("could not split the microphone at a capture boundary")
        }
        return try retime(part, sample.presentationTimeStamp + CMTime(value: Int64(first), timescale: Int32(format.sampleRate)))
    }

    private func enqueue(_ sample: CMSampleBuffer, _ segment: Segment, _ rate: Int32) throws {
        if maxDuration > 0, audioEnd.seconds * 1000 >= maxDuration { release(sample.duration.seconds); return }
        let origin = segment.origin!
        let skip = max(0, min(Int64(sample.numSamples), CMTimeConvertScale(origin - sample.presentationTimeStamp, timescale: rate, method: .roundTowardPositiveInfinity).value))
        release(Double(skip) / Double(rate))
        guard skip < sample.numSamples else { return }
        let pcm = try Self.slice(sample, first: Int(skip), count: sample.numSamples - Int(skip))
        let at = CMTimeConvertScale(segment.offset + pcm.presentationTimeStamp - origin, timescale: rate, method: .roundHalfAwayFromZero)
        audioEnd = at + pcm.duration
        segment.audioEnd = audioEnd
        backlog.append((pcm, at))
    }

    private static func retime(_ sample: CMSampleBuffer, _ time: CMTime) throws -> CMSampleBuffer {
        var timing = CMSampleTimingInfo()
        guard CMSampleBufferGetSampleTimingInfo(sample, at: 0, timingInfoOut: &timing) == noErr else { throw CaptureFailure("missing capture timestamp") }
        timing.presentationTimeStamp = time; timing.decodeTimeStamp = .invalid
        var result: CMSampleBuffer?
        guard CMSampleBufferCreateCopyWithNewTiming(allocator: nil, sampleBuffer: sample, sampleTimingEntryCount: 1,
            sampleTimingArray: &timing, sampleBufferOut: &result) == noErr, let result else { throw CaptureFailure("could not retime a capture buffer") }
        return result
    }

    private var discardedLead = 0.0
    private func discardBeforePicture(_ seconds: Double) throws {
        discardedLead += seconds
        if discardedLead > Self.pressureLimit + 1e-8 { throw CaptureFailure("the writer fell behind") }
    }

    private var wroteAudio = false
    private func flush() throws {
        guard let sound, let picture else { return }
        while let (pcm, at) = backlog.first, ready(sound) {
            let sample = try Self.retime(pcm, at)
            guard sound.append(sample) else { throw asset?.error ?? CaptureFailure("the writer refused audio") }
            backlog.removeFirst()
            release(sample.duration.seconds)
            wroteAudio = true; writtenEnd = sample.presentationTimeStamp + sample.duration
        }
        // Commit pictures only over accepted sound. Even a stalled audio input can
        // then finish at its last sample, and a pause has one seam for both tracks.
        for segment in segments {
            guard let origin = segment.origin else { continue }
            let end = min(writtenEnd, segment.audioEnd)
            if let first = segment.firstPicture, segment.offset < end, ready(picture) {
                try appendVideo(first, at: segment.offset)
                if lastPicture == segment.offset { segment.firstPicture = nil }
            }
            while segment.firstPicture == nil, let pending = segment.pictures.first {
                let at = segment.offset + pending.presentationTimeStamp - origin
                guard at < end else { break }
                segment.pictures.removeFirst()
                try appendVideo(pending, at: at)
            }
        }
        if !answered, wroteAudio, lastPicture.isNumeric { answered = true; accepted(writtenEnd.seconds * 1000) }
        if answered, !notifiedLimit, maxDuration > 0, writtenEnd.seconds * 1000 >= maxDuration {
            notifiedLimit = true; reachedLimit()
        }
        if (!backlog.isEmpty || segments.contains(where: { $0.origin != nil && $0.firstPicture != nil })), retry == nil {
            let work = DispatchWorkItem {
                self.retry = nil
                self.perform { try self.flush() }
            }
            retry = work
            queue.asyncAfter(deadline: .now() + .milliseconds(5), execute: work)
        }
    }

    func finish(publish: Bool, done: @escaping (String?, Int) -> Void) {
        queue.async {
            self.closed = true
            let deadline = DispatchTime.now() + Self.pressureLimit
            func drain() {
                self.retry?.cancel(); self.retry = nil
                guard publish, let writer = self.asset, self.answered else {
                    self.asset?.cancelWriting()
                    return done(self.failure ?? "the capture did not start", self.dropped)
                }
                do { try self.flush() } catch { self.failure = String(describing: error) }
                if !self.backlog.isEmpty, writer.status == .writing, DispatchTime.now() < deadline {
                    self.queue.asyncAfter(deadline: .now() + .milliseconds(5)) { drain() }
                    return
                }
                if !self.backlog.isEmpty { self.failure = self.failure ?? "the writer fell behind" }
                self.retry?.cancel(); self.retry = nil
                self.picture?.markAsFinished(); self.sound?.markAsFinished()
                writer.endSession(atSourceTime: self.writtenEnd)
                writer.finishWriting {
                    self.queue.async {
                        self.asset = nil; self.sound = nil; self.picture = nil
                        self.backlog.removeAll(); self.segments.removeAll()
                        done(writer.error?.localizedDescription ?? self.failure, self.dropped)
                    }
                }
            }
            drain()
        }
    }
}

final class MixedTake: NSObject, MediaTake, ExactMovieAudioConsumer {
    let id: String
    private let path: String, url: URL, stage: URL, context: ExactPackageContext, maxDuration: Double, changed: () -> Void
    private let pictures = AVCaptureVideoDataOutput()
    private var microphoneInput: AVCaptureDeviceInput?, samples: AVCaptureAudioDataOutput?
    private var begin: ((AVCaptureSession) -> Void)?
    private var observers: [NSObjectProtocol] = []
    private var admissionRoutes: (AVCaptureConnection, AVCaptureConnection)?
    private var source: ExactVideoSource?, slot: VideoSlot?, preset: AVCaptureSession.Preset?
    private var videoLease: ExactSubscription?, audioLease: ExactSubscription?
    private final class CameraConsumer: ExactTrackConsumer {
        weak var take: MixedTake?
        var interrupted = false
        func live(video source: ExactVideoSource?) { take?.live(video: source) }
        func muted(_ why: String) {
            take?.writer.mute.discontinuity()
            interrupted = true
            if take?.admitted == true { take?.handoff?(true) }
            take?.pause(why == "background" ? why : "interrupted")
        }
        func unmuted() { interrupted = false; take?.handoff?(false) }
        func ended() { take?.ended() }
    }
    private let camera = CameraConsumer()
    private var handoff: ((Bool) -> Void)?
    func useMicrophoneHistory(_ history: ExactAudioMute) { writer.mute = history }
    func cameraInterruption(_ handoff: @escaping (Bool) -> Void) { self.handoff = handoff }
    private var captureSession: AVCaptureSession?
    private var sawCamera = false, sawMicrophone = false, admitted = false
    private var timer: (() -> Void)?, admissionTimer: DispatchWorkItem?
    private var startReply: (([String: Any]) -> Void)?
    private var waiting: [([String: Any]) -> Void] = []
    private var answer: [String: Any]?, result: [String: Any]?, reason: String?, failure: String?
    private(set) var state = "recording"
    private var since: Double, elapsed = 0.0
    private var audioInterrupted = false, agentNeedsPicture = true
    private let attached = Flag()
    private let agentLock = NSLock()
    private var agentRecording = true, agentFrames = 0
    private var agentSource: ExactAudioSource?, agentTap: ExactSubscription?
    private var detachWaiters: [() -> Void] = []
    private lazy var writer = MixedWriter(stage, maxDuration: maxDuration, reachedLimit: { [weak self] in
        DispatchQueue.main.async { self?.finish("max-duration") { _ in } }
    }, accepted: { [weak self] prefix in
        DispatchQueue.main.async { self?.confirm(prefix) }
    }, failed: { [weak self] message in
        DispatchQueue.main.async { self?.finish("error", error: message) { _ in } }
    })

    init(id: String, path: String, url: URL, context: ExactPackageContext, maxDuration: Double, changed: @escaping () -> Void) {
        self.id = id; self.path = path; self.url = url; self.context = context; self.changed = changed; self.maxDuration = maxDuration
        since = context.now()
        stage = url.deletingLastPathComponent().appendingPathComponent(".\(UUID().uuidString).mp4")
    }

    private var duration: Double { elapsed + (state == "recording" ? max(0, context.now() - since) : 0) }
    private var stamp: CMTime {
        if context.agent { return CMTime(value: Int64((context.now() * 48).rounded()), timescale: 48000) }
        return captureSession?.synchronizationClock.map(CMClockGetTime) ?? .invalid
    }

    func info() -> [String: Any] {
        var value: [String: Any] = ["id": id, "state": state, "path": path, "mimeType": "video/mp4", "duration": duration]
        value["reason"] = reason; value["error"] = failure; value["result"] = result
        return value
    }

    func subscribe(video: String, audio: String, reply: @escaping ([String: Any]) -> Void) {
        startReply = reply
        do {
            guard context.foreground else { throw ExactTrackError("NotAllowedError") }
            camera.take = self
            videoLease = try context.tracks.subscribe(video, camera, start: true, origin: context.origin)
            guard sawCamera, state == "recording", context.agent || source != nil else { throw ExactTrackError("NotReadableError") }
            if let source {
                let slot = source.shared("recording.video") { VideoSlot() }
                guard !slot.busy else { throw ExactTrackError("NotReadableError") }
                slot.busy = true; self.slot = slot
            }
            audioLease = try context.tracks.subscribe(audio, self, start: true, origin: context.origin)
            guard sawMicrophone, state == "recording" else { throw ExactTrackError("NotReadableError") }
            if context.agent {
                writer.configure(nil)
                agentSource?.advance(to: context.now() + 1 / 48.0)
            } else {
                guard let source, let microphoneInput, let samples, let begin else { throw ExactTrackError("NotReadableError") }
                setup(source, microphoneInput, samples, begin)
            }
            let timeout = DispatchWorkItem { [weak self] in
                guard let self, !self.admitted else { return }
                let reply = self.startReply
                self.startReply = nil
                let message = "the capture supplied no usable video and audio"
                self.finish("error", error: message) { _ in }
                reply?(["error": "NotReadableError", "message": message])
            }
            admissionTimer = timeout
            DispatchQueue.main.asyncAfter(deadline: .now() + 5, execute: timeout)
        } catch {
            let code = (error as? ExactTrackError)?.code ?? "NotReadableError"
            closeGraph {
                self.state = "inactive"
                self.startReply = nil
                reply(["error": code])
            }
        }
    }

    func live(video source: ExactVideoSource?) { sawCamera = true; if self.source == nil { self.source = source; captureSession = source?.session } }
    func prepareMicrophone(_ begin: @escaping (AVCaptureSession) -> Void) { self.begin = begin }

    func microphone(_ input: AVCaptureDeviceInput?, samples: AVCaptureAudioDataOutput?, enabled: Bool) {
        if !context.agent { writer.mute.enabled(enabled, at: ExactAudioMute.Reading(clock: captureSession?.synchronizationClock)) }
        sawMicrophone = true; self.microphoneInput = input; self.samples = samples
    }

    @discardableResult func movieAudio(_ sample: CMSampleBuffer) -> CMSampleBuffer? {
        writer.audio(sample, reading: ExactAudioMute.Reading(clock: captureSession?.synchronizationClock))
    }

    private func setup(_ source: ExactVideoSource, _ input: AVCaptureDeviceInput, _ samples: AVCaptureAudioDataOutput,
                       _ begin: @escaping (AVCaptureSession) -> Void) {
        let writer = writer, pictures = pictures, routes = source.movieAudioRoutes
        pictures.alwaysDiscardsLateVideoFrames = true
        pictures.setSampleBufferDelegate(writer, queue: writer.queue)
        #if os(iOS)
        let cameraReset = NotificationCenter.default.addObserver(forName: AVCaptureSession.runtimeErrorNotification, object: source.session, queue: .main) { [weak self] note in
            guard let error = note.userInfo?[AVCaptureSessionErrorKey] as? NSError, error.domain == AVFoundationErrorDomain,
                  error.code == AVError.mediaServicesWereReset.rawValue else { return }
            self?.finish("media-reset") { _ in }
        }
        let audioReset = NotificationCenter.default.addObserver(forName: AVAudioSession.mediaServicesWereResetNotification, object: nil, queue: .main) { [weak self] _ in
            self?.finish("media-reset") { _ in }
        }
        observers = [cameraReset, audioReset]
        #endif
        let mute = writer.mute
        observers.append(NotificationCenter.default.addObserver(forName: AVCaptureInput.Port.formatDescriptionDidChangeNotification,
            object: nil, queue: nil) { note in
            if let port = note.object as? AVCaptureInput.Port, port.input === input { mute.discontinuity() }
        })
        #if os(macOS)
        observers.append(NotificationCenter.default.addObserver(forName: AVCaptureDevice.wasDisconnectedNotification,
            object: nil, queue: nil) { note in
            if let device = note.object as? AVCaptureDevice, device === input.device { mute.discontinuity() }
        })
        #endif
        var angle: CGFloat?
        if #available(iOS 17, macOS 14, *) {
            angle = AVCaptureDevice.RotationCoordinator(device: source.device, previewLayer: nil).videoRotationAngleForHorizonLevelCapture
        }
        var rate = 0.0
        source.configure { session in
            guard self.attached.value else { return }
            rate = source.device.activeVideoMinFrameDuration.seconds
            if session.sessionPreset == .photo, session.canSetSessionPreset(.high) {
                self.preset = session.sessionPreset; session.sessionPreset = .high
            }
            routes.input = input; routes.samples = samples
            guard session.canAddInput(input) else { return }
            begin(session)
            session.addInput(input)
            for output in [samples as AVCaptureOutput, pictures] where session.canAddOutput(output) { session.addOutput(output) }
            routes.restrictConnections(in: session)
        }
        source.run { session in
            if rate > 0 { source.apply(["frameRate": 1 / rate]) }
            guard self.attached.value else { return }
            guard let connection = pictures.connection(with: .video), self.valid(session, input, samples) else {
                DispatchQueue.main.async { self.finish("error", error: "the session cannot supply camera and microphone samples") { _ in } }
                return
            }
            VideoTake.orient(connection, angle: angle)
            let codec = pictures.availableVideoCodecTypesForAssetWriter(writingTo: .mov).first { $0 == .hevc || $0 == .h264 }
            let supported = pictures.availableVideoCodecTypesForAssetWriter(writingTo: .mp4)
            let settings = codec.flatMap { supported.contains($0) ? pictures.recommendedVideoSettings(forVideoCodecType: $0, assetWriterOutputFileType: .mp4) : nil }
            guard let mic = samples.connection(with: .audio) else {
                DispatchQueue.main.async { self.finish("error", error: "the microphone route was lost during start") { _ in } }
                return
            }
            DispatchQueue.main.async {
                guard self.attached.value else { return }
                self.admissionRoutes = (connection, mic)
                writer.configure(settings)
            }
        }
    }

    private func valid(_ session: AVCaptureSession, _ input: AVCaptureDeviceInput, _ samples: AVCaptureAudioDataOutput) -> Bool {
        guard session.synchronizationClock != nil, session.inputs.contains(input), session.outputs.contains(samples),
              session.outputs.contains(pictures), let camera = pictures.connection(with: .video), camera.isActive, camera.isEnabled,
              let mic = samples.connection(with: .audio), mic.isActive, mic.isEnabled, !mic.inputPorts.isEmpty else { return false }
        return mic.inputPorts.allSatisfy { $0.input === input }
    }

    private func confirm(_ prefix: Double) {
        guard state == "recording", !admitted else { return }
        if !context.agent {
            guard let (video, audio) = admissionRoutes, video.isActive, video.isEnabled, audio.isActive, audio.isEnabled else {
                return finish("error", error: "the capture route was lost during start") { _ in }
            }
        }
        admitted = true; since = context.now()
        if !context.agent { elapsed = prefix }
        admissionTimer?.cancel(); admissionTimer = nil
        let reply = startReply; startReply = nil
        arm()
        reply?(info())
    }

    func captureMicrophone(_ active: Bool, done: @escaping () -> Void) {
        guard let source, let input = microphoneInput, let samples else { return done() }
        guard attached.value else { detachWaiters.append(done); return }
        let handoff = begin
        source.configure { session in
            if active {
                if self.attached.value, !session.inputs.contains(input), session.canAddInput(input) {
                    handoff?(session)
                    session.addInput(input)
                }
            } else if session.inputs.contains(input) { session.removeInput(input) }
        }
        source.run { session in
            let failed = active && self.attached.value && !self.valid(session, input, samples)
            DispatchQueue.main.async {
                if failed { self.finish("error", error: "the microphone could not rejoin the camera") { _ in } }
                done()
            }
        }
    }

    private func arm() {
        timer?(); timer = nil
        if context.agent, maxDuration > 0 {
            timer = context.after(max(0, maxDuration - duration)) { [weak self] in self?.finish("max-duration") { _ in } }
        }
    }

    func pause(_ why: String) {
        guard state == "recording" else { return }
        guard admitted else {
            // Track owners can call back synchronously during subscribe.
            if audioLease == nil { state = "paused"; reason = why; return }
            return finish(why) { _ in }
        }
        pump()
        agentLock.withLock { agentRecording = false }
        writer.pause(at: stamp)
        elapsed = duration; state = "paused"; reason = why
        timer?(); timer = nil; changed()
    }
    func resume() {
        guard state == "paused", admitted, context.foreground, !camera.interrupted, !audioInterrupted else { return }
        if context.agent { agentSource?.advance(to: context.now()) }
        writer.resume(at: stamp)
        agentLock.withLock { agentNeedsPicture = true; agentRecording = true }
        since = context.now(); state = "recording"; reason = nil
        arm(); changed()
    }
    func muted(_ why: String) {
        guard state != "inactive" else { return }
        if why != "idle" { writer.mute.discontinuity() }
        if why == "media-reset" { finish(why) { _ in } }
        else if why != "idle" { audioInterrupted = true; pause(why == "background" ? why : "interrupted") }
    }
    func unmuted() { audioInterrupted = false }
    func ended() { if audioLease != nil { finish("track-ended") { _ in } } }

    func live(_ source: ExactAudioSource?) {
        guard context.agent, let source, agentTap == nil else { return }
        agentSource = source
        agentTap = source.tap { [weak self] pcm, time in
            guard let self else { return }
            do { try self.agentLock.withLock { if self.agentRecording { try self.recordAgent(pcm, time) } } }
            catch { DispatchQueue.main.async { self.finish("error", error: String(describing: error)) { _ in } } }
        }
    }

    private func pump() {
        if context.agent { agentSource?.advance(to: context.now()) }
    }

    /// The driver's picture changes every 100 ms of recorded sound.
    private func recordAgent(_ pcm: AVAudioPCMBuffer, _ time: AVAudioTime) throws {
        let rate = Int(pcm.format.sampleRate), step = max(1, rate / 10)
        let count = min(3 * step - agentFrames, Int(pcm.frameLength))
        guard count > 0 else { return }
        guard let sample = Writer.sample(pcm, at: time.sampleTime) else { throw CaptureFailure("could not package substitute PCM") }
        var offset = 0
        while offset < count {
            if agentNeedsPicture || agentFrames % step == 0 {
                agentNeedsPicture = false
                let frame = try Self.agentPicture(Int((Double(time.sampleTime + Int64(offset)) * 48000 / Double(rate)).rounded()))
                writer.queue.async { self.writer.video(frame) }
            }
            let length = min(count - offset, step - agentFrames % step)
            let part = try MixedWriter.slice(sample, first: offset, count: length)
            writer.appendAudio(part, rate: pcm.format.sampleRate)
            offset += length; agentFrames += length
        }
    }

    static func agentPicture(_ frame: Int) throws -> CMSampleBuffer {
        var pixel: CVPixelBuffer?, format: CMVideoFormatDescription?, sample: CMSampleBuffer?
        guard CVPixelBufferCreate(nil, 16, 16, kCVPixelFormatType_32BGRA, nil, &pixel) == kCVReturnSuccess, let pixel else {
            throw CaptureFailure("could not allocate substitute picture")
        }
        CVPixelBufferLockBaseAddress(pixel, [])
        memset(CVPixelBufferGetBaseAddress(pixel)!, Int32(64 + (frame / 4800) % 3 * 64), CVPixelBufferGetDataSize(pixel))
        CVPixelBufferUnlockBaseAddress(pixel, [])
        var timing = CMSampleTimingInfo(duration: CMTime(value: 1, timescale: 10),
            presentationTimeStamp: CMTime(value: Int64(frame), timescale: 48000), decodeTimeStamp: .invalid)
        guard CMVideoFormatDescriptionCreateForImageBuffer(allocator: nil, imageBuffer: pixel, formatDescriptionOut: &format) == noErr, let format,
              CMSampleBufferCreateReadyWithImageBuffer(allocator: nil, imageBuffer: pixel, formatDescription: format,
                sampleTiming: &timing, sampleBufferOut: &sample) == noErr, let sample else { throw CaptureFailure("could not package substitute picture") }
        return sample
    }

    func finish(_ why: String, error: String? = nil, _ done: @escaping ([String: Any]) -> Void) {
        if let answer { return done(answer) }
        waiting.append(done)
        guard state != "inactive" else { return }
        pump()
        agentLock.withLock { agentRecording = false }
        writer.pause(at: stamp)
        elapsed = duration; state = "inactive"; reason = why; failure = error ?? failure
        admissionTimer?.cancel(); admissionTimer = nil
        timer?(); timer = nil
        closeGraph {
            self.writer.finish(publish: self.admitted) { message, dropped in
                Task {
                    do {
                        guard self.admitted else { throw ExactTrackError("NotReadableError") }
                        if let message, message != "the writer fell behind" { throw CaptureFailure(message) }
                        let metadata = try await Self.metadata(self.stage)
                        guard rename(self.stage.path, self.url.path) == 0 else { throw CaptureFailure("the MP4 could not replace the destination") }
                        DispatchQueue.main.async { self.complete(metadata, message, dropped) }
                    } catch {
                        DispatchQueue.main.async { self.complete(nil, message ?? String(describing: error), dropped) }
                    }
                }
            }
        }
        changed()
    }

    private static func metadata(_ url: URL) async throws -> [String: Any] {
        let asset = AVURLAsset(url: url)
        let video = try await asset.loadTracks(withMediaType: .video), audio = try await asset.loadTracks(withMediaType: .audio)
        guard video.count == 1, audio.count == 1, let format = try await audio[0].load(.formatDescriptions).first,
              CMFormatDescriptionGetMediaSubType(format) == kAudioFormatMPEG4AAC else { throw CaptureFailure("the MP4 is missing a capture track") }
        let picture = try await video[0].load(.timeRange), sound = try await audio[0].load(.timeRange)
        let rate = try await video[0].load(.nominalFrameRate)
        guard rate > 0, sound.duration > .zero, picture.start == .zero, sound.start == .zero,
              abs(picture.duration.seconds - sound.duration.seconds) <= 1 / Double(rate) else {
            throw CaptureFailure("the microphone does not cover the recorded video")
        }
        return try await VideoTake.metadata(url)
    }

    private func closeGraph(_ done: @escaping () -> Void) {
        agentTap?.cancel(); agentTap = nil; agentSource = nil
        attached.value = false
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
        observers.removeAll(); admissionRoutes = nil
        let release = {
            self.slot?.busy = false; self.slot = nil
            self.audioLease?.cancel(); self.audioLease = nil
            self.videoLease?.cancel(); self.videoLease = nil
            self.source = nil; self.captureSession = nil; self.microphoneInput = nil; self.samples = nil; self.begin = nil
            let waiters = self.detachWaiters; self.detachWaiters.removeAll()
            waiters.forEach { $0() }
            done()
        }
        guard let source, let input = microphoneInput, let samples else { return release() }
        let routes = source.movieAudioRoutes, pictures = pictures
        var rate = 0.0
        source.configure { session in
            rate = source.device.activeVideoMinFrameDuration.seconds
            for output in [samples as AVCaptureOutput, pictures] where session.outputs.contains(output) { session.removeOutput(output) }
            if session.inputs.contains(input) { session.removeInput(input) }
            if routes.input === input { routes.input = nil; routes.samples = nil }
            if let preset = self.preset, session.canSetSessionPreset(preset) { session.sessionPreset = preset }
        }
        source.run { _ in
            if rate > 0 { source.apply(["frameRate": 1 / rate]) }
            pictures.setSampleBufferDelegate(nil, queue: nil)
            DispatchQueue.main.async(execute: release)
        }
    }

    private func complete(_ metadata: [String: Any]?, _ message: String?, _ dropped: Int) {
        failure = failure ?? message
        if let failure {
            if reason != "media-reset" { reason = "error" }
            context.log("recording: \(id) \(failure)")
        }
        if dropped > 0 { context.log("recording: \(id) dropped \(dropped) video frames") }
        if var media = metadata {
            media["mimeType"] = "video/mp4"; media["path"] = path
            media["size"] = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
            result = media; answer = media
        } else { answer = ["error": admitted ? "UnknownError" : "NotReadableError", "message": failure ?? "recording failed"] }
        try? FileManager.default.removeItem(at: stage)
        let reply = startReply; startReply = nil
        reply?(answer!)
        changed()
        let callbacks = waiting; waiting = []
        callbacks.forEach { $0(answer!) }
    }
}
