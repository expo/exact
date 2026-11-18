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
