// @exact/camera on Apple (LLP 1133.008 D1-D5, D8, §9): the viewfinder
// `exact-camera` (Viewfinder.swift) over a video track of
// @exact/media-capture's, and `takePhoto` through the capture's photo output,
// written to the app's files.
//
// - A viewfinder is a consumer of its track while it is displayed and its
//   `srcObject` names the track (D3). The runner says when either changes and
//   by whose commit (`displayed`, `bound`); the viewfinder joins the track
//   with that origin, so a lease is taken only for the person's input or the
//   app's own code (§14), and a viewfinder a tool showed shows nothing.
// - The photo output is the process's, one per capture (D3): an
//   `AVCapturePhotoOutput` attached to the capture's session at the first
//   photo, which takes every session's photos in turn. A photo in flight holds
//   its track's lease until it is written.
// - Unsaid settings are the platform's (D2): `AVCapturePhotoSettings()`'s
//   JPEG, flash off, `balanced`; the output's ceiling the largest size it
//   delivers without deferred processing; the capture turned by the rotation
//   coordinator and a front camera's photo not mirrored.
// - Under the agent no capture exists: the viewfinder draws the agent's
//   picture and `takePhoto` copies its file unchanged (§9). The picture is
//   the package's test card, read from this checkout (a drive runs on this
//   Mac or its simulators), or the app asset `prefer extension
//   camera.picture={"path":…}` names.
// - `takePhotoWithCameraApp` on iOS is the system camera UI
//   (`UIImagePickerController` with `.camera`), with its shutter, flash and
//   review; the kept photo is written as JPEG with the camera's metadata. It
//   needs the camera's usage key (the `device.camera` line) and asks the
//   camera's permission as the picker would. The simulator has no camera:
//   `NotFoundError`. Under the agent the runner holds the person's sheet,
//   and `take` writes the agent's picture. macOS has no such UI.
// - Tier 3 (D5): the photo output when it is added (`photoOutputHatch`, on
//   the session's queue), each capture's settings before it is taken
//   (`photoSettingsHatch`, main, may return a replacement), each photo
//   (`photoHatch`, the platform's queue) and each viewfinder's view
//   (`viewfinderHatch`, main).
import AVFoundation
import ExactPackage
import Foundation
import CryptoKit
import ImageIO
#if os(macOS)
import AppKit
#else
import UIKit
#endif

public final class CameraModule: ExactPackage {
    public static var photoOutputHatch: ((AVCapturePhotoOutput) -> Void)?
    public static var photoSettingsHatch: ((AVCapturePhotoSettings) -> AVCapturePhotoSettings?)?
    public static var photoHatch: ((AVCapturePhoto) -> Void)?
    public static var viewfinderHatch: ((ExactPackagePlatformView) -> Void)?

    let context: ExactPackageContext
    private var views: [String: ViewState] = [:]
    private var picture: String?
    private var taken = 0
    private var photos: [[String: Any]] = []

    /// What the runner said of a viewfinder, and what it consumes.
    final class ViewState {
        weak var view: Viewfinder?
        /// Its `srcObject`: nil until its props arrive.
        var src: String?
        /// Values of `srcObject` seen, and the runner's reports of them: a
        /// value not yet reported is not yet the commit's, so its origin is unknown.
        var changes = 0, reports = 0
        var shown = false, madeBy = "app"
        var sub: Sub?
    }

    /// A viewfinder's place among its track's consumers.
    final class Sub: ExactTrackConsumer {
        let track: String, origin: String
        var live = false
        var subscription: ExactSubscription?
        var onLive: (ExactVideoSource?) -> Void = { _ in }
        var onBlank: () -> Void = {}
        var onMuted: () -> Void = {}
        var onUnmuted: () -> Void = {}
        init(track: String, origin: String) { self.track = track; self.origin = origin }
        func live(video source: ExactVideoSource?) { live = true; onLive(source) }
        func muted(_ reason: String) { onMuted() }
        func unmuted() { onUnmuted() }
        func ended() { live = false; onBlank() }
        func leave() { subscription?.cancel(); subscription = nil }
    }

    public init(context: ExactPackageContext) { self.context = context }

    private func say(_ line: String) { context.log("camera: \(line)") }

    // MARK: the names

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        switch name {
        case "takePhoto": takePhoto(args, reply)
        case "takePhotoWithCameraApp": cameraApp(args, tap: (preferred as? [String: Any])?["tap"] as? String, reply)
        case "getPhotoCapabilities": reply(capabilities(args))
        case "picture": reply(["path": picture ?? ""])
        default: reply(["error": "NotSupportedError"])
        }
    }

    /// Under the agent: the picture `prefer extension camera.picture` names (D8 item 9).
    public func preferred(_ values: [String: Any]) {
        let named = (values["picture"] as? [String: Any])?["path"] as? String
        let next = named?.isEmpty == false ? named : nil
        guard next != picture else { return }
        picture = next
        for (id, v) in views where v.sub?.live == true {
            context.tracks.lookup(v.sub?.track ?? "")?.picture = agentPicture()
            v.sub?.leave(); v.sub = nil; reconcile(id)
        }
    }

    public func destroy(_ done: @escaping () -> Void) {
        for v in views.values { v.sub?.leave() }
        views = [:]
        if cameraAppUp { endCameraApp("the session ended") }
        done()
    }

    // MARK: viewfinders (D3, D8 items 1 and 2)

    public func view(_ tag: String, id: String, props: [String: String], events: ExactPackageEvents) throws -> ExactPackageView {
        guard tag == "exact-camera" else { throw ExactPackageRefusal("no view \(tag)") }
        let v = Viewfinder(id: id, module: self, events: events)
        state(id).view = v
        try v.setProps(props)
        Self.viewfinderHatch?(v.view)
        return v
    }

    public func displayed(_ id: String, _ shown: Bool, origin: String) {
        let v = state(id)
        v.shown = shown
        if shown { v.madeBy = origin }
        reconcile(id)
    }

    public func bound(_ id: String, _ prop: String, origin: String) {
        guard prop == "srcObject" else { return }
        let v = state(id)
        v.reports += 1
        v.madeBy = origin
        reconcile(id)
    }

    private func state(_ id: String) -> ViewState {
        if let v = views[id] { return v }
        let v = ViewState()
        views[id] = v
        return v
    }

    /// A viewfinder's props: a new `srcObject` waits for the runner's report of it.
    func props(_ id: String, _ props: [String: String]) {
        let v = state(id)
        if let src = props["srcObject"] {
            if src != v.src { v.src = src; v.changes += 1 }
        } else {
            v.src = ""
        }
        reconcile(id)
    }

    func gone(_ id: String) {
        views[id]?.sub?.leave()
        views[id] = nil
    }

    /// Viewfinder `id` consumes its track while it is made, displayed and bound to a video track (D3).
    private func reconcile(_ id: String) {
        guard let v = views[id] else { return }
        let src = v.src ?? ""
        let want = v.view != nil && v.shown && !src.isEmpty && v.reports >= v.changes && context.tracks.lookup(src)?.kind == "video"
        if let sub = v.sub, !want || sub.track != src { sub.leave(); v.sub = nil; v.view?.blank() }
        // Shown again by the person or the app, a viewfinder that took no lease takes one (§14).
        if want, let sub = v.sub, !sub.live, v.madeBy == "app", sub.origin != "app" { sub.leave(); v.sub = nil }
        guard want, v.sub == nil else { return }
        let sub = Sub(track: src, origin: v.madeBy)
        var source: ExactVideoSource?
        let show = { [weak self, weak v, weak sub] in
            guard let self, let v, let sub, v.sub === sub, sub.live else { return }
            let facing = (self.context.tracks.lookup(sub.track)?.state?()["settings"] as? [String: Any])?["facingMode"] as? String
            if self.context.agent {
                let picture = self.agentPicture()
                v.view?.show(picture: picture, mirrored: facing == "user")
                // The track's frame is the picture drawn, unmirrored, as a sensor's is (LLP 1133.027 D17 item 2).
                self.context.tracks.lookup(sub.track)?.picture = picture
            } else if let source { v.view?.show(source: source) }
        }
        sub.onLive = { s in source = s; show() }
        sub.onUnmuted = show
        sub.onMuted = { [weak v] in v?.view?.mute() }
        sub.onBlank = { [weak v] in v?.view?.blank() }
        v.sub = sub
        do {
            sub.subscription = try context.tracks.subscribe(src, sub, start: false, origin: v.madeBy)
            say("viewfinder \(id) consumes \(src) (\(v.madeBy) origin)")
        } catch {
            v.sub = nil
            say("viewfinder \(id): \(error)")
        }
    }

    // MARK: photos (D1, D2)

    private func takePhoto(_ args: [String: Any], _ reply: @escaping ([String: Any]) -> Void) {
        guard let id = args["track"] as? String, let entry = context.tracks.lookup(id) else { return reply(["error": "NotFoundError"]) }
        guard entry.kind == "video" else { return reply(["error": "NotSupportedError"]) }
        let s = entry.state?() ?? [:]
        // A track that is not shown takes no photo (D1): ended, disabled, or muted (`idle` among them).
        guard s["readyState"] as? String == "live", s["enabled"] as? Bool == true, s["muted"] as? Bool == false else {
            say("takePhoto \(id): InvalidStateError (state=\(s["readyState"] ?? "unknown"), enabled=\(s["enabled"] ?? false), muted=\(s["muteReason"] ?? "none"))")
            return reply(["error": "InvalidStateError"])
        }
        let origin = context.origin
        let took = { [self] (path: String, data: Data) in took(path, data, track: id, origin: origin) }
        let written = { [self] (ext: String) in place(args, ext) }
        if context.agent {
            guard let data = agentPictureData(), let (path, url) = written("jpg") else { return reply(["error": "UnknownError"]) }
            do { try data.write(to: url) } catch { return reply(["error": "UnknownError"]) }
            took(path, data)
            return reply(Self.photo(path: path, data: data))
        }
        guard let source = entry.video?() else { return reply(["error": "InvalidStateError"]) }
        // A photo in flight is a consumer: its lease holds until the file is written (D3).
        let holder = Sub(track: id, origin: "app")
        holder.subscription = try? context.tracks.subscribe(id, holder, start: false, origin: "app")
        let output = source.shared("camera.photo") { PhotoOutput(source) }
        output.capture(args: args, source: source) { [self] result in
            switch result {
            case .failure(let failure):
                holder.leave()
                reply(["error": failure.code])
            case .success(let data):
                let ext = Self.isHEIF(data) ? "heic" : "jpg"
                guard let (path, url) = written(ext) else { holder.leave(); return reply(["error": "UnknownError"]) }
                DispatchQueue.global(qos: .userInitiated).async {
                    let ok = (try? data.write(to: url)) != nil
                    DispatchQueue.main.async {
                        holder.leave()
                        guard ok else { return reply(["error": "UnknownError"]) }
                        took(path, data)
                        reply(Self.photo(path: path, data: data))
                    }
                }
            }
        }
    }

    /// A photo written: journaled, and under the agent listed for `state.extensions` (§9, Disclosure); one from the
    /// camera app names no track.
    private func took(_ path: String, _ data: Data, track: String?, origin: String) {
        say("took \(path) (\(data.count) bytes\(context.agent ? ", the agent's picture" : ""), sha256 \(Self.sha256(data)))")
        photos.append(["path": path, "sha256": Self.sha256(data), "track": track ?? NSNull(), "origin": origin])
        context.state(["photos": photos])
    }

    /// Where a photo of format `ext` goes: the call's `path` with the extension, else `app:/tmp/camera/<n>.<ext>`.
    private func place(_ args: [String: Any], _ ext: String) -> (String, URL)? {
        let path: String
        if let p = args["path"] as? String, !p.isEmpty { path = p.hasSuffix(".\(ext)") ? p : "\(p).\(ext)" } else { taken += 1; path = "app:/tmp/camera/\(taken).\(ext)" }
        guard let url = context.file(path) else { return nil }
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        return (path, url)
    }

    // MARK: the camera app (`takePhotoWithCameraApp`)

    /// Whether the system's camera UI is up, or asked for, for a call: one at a time.
    private var cameraAppUp = false
    /// The runner's number for that call, which a `cancel` names.
    private var cameraAppRequest: Int?
    /// Moved when the call ends unanswered (let go, or the session's end): an answer to the camera's permission asked
    /// before then shows nothing.
    private var cameraAppTurn = 0
    #if os(iOS)
    private var cameraAppDelegate: CameraAppDelegate?
    private weak var cameraAppPicker: UIImagePickerController?
    /// What the camera app reads of the platform; a test stands in for a camera and its permission.
    static var cameraAppPlatform = CameraAppPlatform()
    #endif

    /// The send let go (LLP 1133.016 D15 item 2): its camera UI goes, and nothing answers.
    public func cancel(_ request: Int) {
        guard cameraAppUp, request == cameraAppRequest else { return }
        endCameraApp("let go")
    }

    /// The camera app's call ends unanswered: the picker is dismissed and a pending permission's answer ignored.
    private func endCameraApp(_ why: String) {
        cameraAppTurn += 1
        cameraAppUp = false
        cameraAppRequest = nil
        #if os(iOS)
        cameraAppDelegate = nil
        cameraAppPicker?.presentingViewController?.dismiss(animated: true)
        cameraAppPicker = nil
        #endif
        say("camera app: \(why)")
    }

    /// The photo the person takes and keeps in the system's camera UI (`UIImagePickerController` with `.camera`), as
    /// `takePhoto` answers one; `tap` is the agent's on the runner's sheet.
    private func cameraApp(_ args: [String: Any], tap: String?, _ reply: @escaping ([String: Any]) -> Void) {
        guard !cameraAppUp else { return reply(["error": "InvalidStateError", "message": "the camera is showing"]) }
        let origin = context.origin
        if context.agent {
            guard tap == "take" else {
                say("camera app: cancelled (the agent's sheet)")
                return reply(["error": "AbortError"])
            }
            guard let data = agentPictureData(), let (path, url) = place(args, "jpg") else { return reply(["error": "UnknownError"]) }
            do { try data.write(to: url) } catch { return reply(["error": "UnknownError"]) }
            took(path, data, track: nil, origin: origin)
            return reply(Self.photo(path: path, data: data))
        }
        #if os(iOS)
        let platform = Self.cameraAppPlatform
        // The simulator has none (Apple: "test your camera app on actual devices").
        guard platform.available() else {
            say("camera app: this device has no camera")
            return reply(["error": "NotFoundError"])
        }
        // Without the key iOS ends the app at the camera's first use; the `device.camera` line writes it.
        guard platform.described() else {
            say("camera app: the app has no NSCameraUsageDescription")
            return reply(["error": "NotAllowedError", "message": "iOS shows the camera's purpose: grant `device.camera <purpose>`"])
        }
        cameraAppRequest = context.request
        switch platform.status() {
        case .authorized: showCameraApp(args, origin: origin, reply)
        case .notDetermined:
            cameraAppUp = true
            let turn = cameraAppTurn
            platform.ask { [weak self] granted in
                DispatchQueue.main.async {
                    guard let self, turn == self.cameraAppTurn else { return }
                    self.cameraAppUp = false
                    if granted { self.showCameraApp(args, origin: origin, reply) } else { reply(["error": "NotAllowedError"]) }
                }
            }
        default:
            say("camera app: the camera is not allowed")
            reply(["error": "NotAllowedError"])
        }
        #else
        reply(["error": "NotSupportedError"])
        #endif
    }

    #if os(iOS)
    private func showCameraApp(_ args: [String: Any], origin: String, _ reply: @escaping ([String: Any]) -> Void) {
        // The session's own window, key or not (LLP 1133.027 D17 item 3), and what it presents now.
        var top = context.presentation?.rootViewController
        while let presented = top?.presentedViewController { top = presented }
        guard let top else { return reply(["error": "InvalidStateError", "message": "no window to show the camera in"]) }
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.mediaTypes = ["public.image"]
        switch args["facingMode"] as? String {
        case "user" where UIImagePickerController.isCameraDeviceAvailable(.front): picker.cameraDevice = .front
        case "environment" where UIImagePickerController.isCameraDeviceAvailable(.rear): picker.cameraDevice = .rear
        default: break
        }
        let delegate = CameraAppDelegate { [weak self] result in
            guard let self else { return }
            cameraAppDelegate = nil
            cameraAppPicker = nil
            cameraAppUp = false
            cameraAppRequest = nil
            switch result {
            case .failure(let failure):
                say("camera app: \(failure.code == "AbortError" ? "cancelled" : failure.code)")
                reply(["error": failure.code])
            case .success(let data):
                guard let (path, url) = place(args, "jpg") else { return reply(["error": "UnknownError"]) }
                DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                    let ok = (try? data.write(to: url)) != nil
                    DispatchQueue.main.async {
                        guard let self else { return }
                        guard ok else { return reply(["error": "UnknownError"]) }
                        self.took(path, data, track: nil, origin: origin)
                        reply(Self.photo(path: path, data: data))
                    }
                }
            }
        }
        picker.delegate = delegate
        cameraAppDelegate = delegate
        cameraAppPicker = picker
        cameraAppUp = true
        say("camera app shown")
        top.present(picker, animated: true)
    }
    #endif

    /// `Photo`: its size as it is shown, its orientation applied.
    static func photo(path: String, data: Data) -> [String: Any] {
        var width = 0, height = 0
        if let source = CGImageSourceCreateWithData(data as CFData, nil),
           let p = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any] {
            width = p[kCGImagePropertyPixelWidth] as? Int ?? 0
            height = p[kCGImagePropertyPixelHeight] as? Int ?? 0
            if let o = p[kCGImagePropertyOrientation] as? Int, o >= 5 { swap(&width, &height) }
        }
        return ["path": path, "type": isHEIF(data) ? "image/heic" : "image/jpeg", "size": data.count, "width": width, "height": height]
    }

    /// The journal names each photo's SHA-256, which a drive compares with the agent's picture (§9).
    static func sha256(_ data: Data) -> String { SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined() }

    private static func isHEIF(_ data: Data) -> Bool { data.count > 12 && String(decoding: data[4..<12], as: UTF8.self).hasPrefix("ftyphei") }

    private func capabilities(_ args: [String: Any]) -> [String: Any] {
        guard let id = args["track"] as? String, let entry = context.tracks.lookup(id) else { return ["error": "NotFoundError"] }
        guard entry.kind == "video" else { return ["error": "NotSupportedError"] }
        let settings = entry.state?()["settings"] as? [String: Any] ?? [:]
        let fixed = { (n: Any?) -> [String: Any] in let v = (n as? NSNumber)?.intValue ?? 0; return ["min": v, "max": v, "step": 1] }
        if context.agent {
            let back = settings["facingMode"] as? String == "environment"
            return ["redEyeReduction": "never", "imageWidth": fixed(settings["width"]), "imageHeight": fixed(settings["height"]),
                    "fillLightMode": back ? ["auto", "off", "flash"] : ["off"]]
        }
        guard let source = entry.video?() else {
            return ["redEyeReduction": "never", "imageWidth": fixed(settings["width"]), "imageHeight": fixed(settings["height"]), "fillLightMode": []]
        }
        return source.shared("camera.photo") { PhotoOutput(source) }.capabilities(source.device)
    }

    // MARK: the agent's picture (§9)

    /// The package's test card in this checkout, beside this file's own directory.
    private static let card = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("fixture/card.jpg")

    private func agentPictureURL() -> URL? {
        guard let picture else { return Self.card }
        var roots: [String] = []
        if let assets = ProcessInfo.processInfo.environment["EXACT_ASSETS"] { roots.append(assets) }
        if let resources = Bundle.main.resourcePath { roots.append(resources) }
        roots.append(Bundle.main.bundlePath)
        return roots.map { URL(fileURLWithPath: $0).appendingPathComponent(picture) }.first { FileManager.default.fileExists(atPath: $0.path) }
    }

    func agentPictureData() -> Data? { agentPictureURL().flatMap { try? Data(contentsOf: $0) } }

    func agentPicture() -> CGImage? {
        guard let url = agentPictureURL(), let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        let options: [CFString: Any] = [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceCreateThumbnailWithTransform: true,
                                        kCGImageSourceThumbnailMaxPixelSize: 2048]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }
}

/// A capture's photo output, the process's (D3): photos in the order they are asked.
final class PhotoOutput: NSObject {
    let output = AVCapturePhotoOutput()
    private var inFlight: [Int64: PhotoDelegate] = [:]

    init(_ source: ExactVideoSource) {
        super.init()
        let output = output, device = source.device
        source.configure { session in
            guard session.canAddOutput(output) else { return }
            session.addOutput(output)
            // The ceiling: the largest size delivered without deferred processing (D2).
            if #available(iOS 16.0, macOS 13.0, *), let largest = device.activeFormat.supportedMaxPhotoDimensions.max(by: { $0.width * $0.height < $1.width * $1.height }) {
                output.maxPhotoDimensions = largest
            }
            CameraModule.photoOutputHatch?(output)
        }
    }

    func capabilities(_ device: AVCaptureDevice) -> [String: Any] {
        var widths: [Int32] = [], heights: [Int32] = []
        if #available(iOS 16.0, macOS 13.0, *) {
            widths = device.activeFormat.supportedMaxPhotoDimensions.map(\.width)
            heights = device.activeFormat.supportedMaxPhotoDimensions.map(\.height)
        }
        let range = { (v: [Int32]) -> [String: Any] in ["min": Int(v.min() ?? 0), "max": Int(v.max() ?? 0), "step": 1] }
        let modes = output.supportedFlashModes.compactMap { ($0 == .auto ? "auto" : $0 == .on ? "flash" : $0 == .off ? "off" : nil) as String? }
        var redEye = "never"
        #if os(iOS)
        if output.isAutoRedEyeReductionSupported { redEye = "controllable" }
        #endif
        return ["redEyeReduction": redEye, "imageWidth": range(widths), "imageHeight": range(heights), "fillLightMode": modes]
    }

    func capture(args: [String: Any], source: ExactVideoSource, done: @escaping (Result<Data, PhotoFailure>) -> Void) {
        let apple = args["apple"] as? [String: Any] ?? [:]
        var settings: AVCapturePhotoSettings
        if apple["codec"] as? String == "hevc", output.availablePhotoCodecTypes.contains(.hevc) {
            settings = AVCapturePhotoSettings(format: [AVVideoCodecKey: AVVideoCodecType.hevc])
        } else {
            settings = AVCapturePhotoSettings()
        }
        // A written word beats the web's (D4): `apple.flashMode` over `fillLightMode`.
        let flash = (apple["flashMode"] as? String) ?? (args["fillLightMode"] as? String).map { $0 == "flash" ? "on" : $0 }
        if let flash, let mode = ["auto": AVCaptureDevice.FlashMode.auto, "on": .on, "off": .off][flash], output.supportedFlashModes.contains(mode) {
            settings.flashMode = mode
        }
        if #available(iOS 16.0, macOS 13.0, *) {
            let sizes = source.device.activeFormat.supportedMaxPhotoDimensions
            let wanted = (apple["maxPhotoDimensions"] as? [String: Any]).map { (($0["width"] as? NSNumber)?.int32Value ?? 0, ($0["height"] as? NSNumber)?.int32Value ?? 0) }
                ?? ((args["imageWidth"] as? NSNumber)?.int32Value).map { w in (w, (args["imageHeight"] as? NSNumber)?.int32Value ?? 0) }
            if let (w, h) = wanted, let size = sizes.first(where: { $0.width >= w && $0.height >= h }) ?? sizes.max(by: { $0.width < $1.width }),
               size.width <= output.maxPhotoDimensions.width, size.height <= output.maxPhotoDimensions.height {
                settings.maxPhotoDimensions = size
            }
        }
        if let word = apple["photoQualityPrioritization"] as? String,
           let p = ["speed": AVCapturePhotoOutput.QualityPrioritization.speed, "balanced": .balanced, "quality": .quality][word] {
            settings.photoQualityPrioritization = p.rawValue <= output.maxPhotoQualityPrioritization.rawValue ? p : output.maxPhotoQualityPrioritization
        }
        #if os(iOS)
        let redEye = (apple["isAutoRedEyeReductionEnabled"] as? Bool) ?? (args["redEyeReduction"] as? Bool)
        if let redEye, output.isAutoRedEyeReductionSupported { settings.isAutoRedEyeReductionEnabled = redEye }
        if #available(iOS 18.0, *), apple["isShutterSoundSuppressionEnabled"] as? Bool == true, output.isShutterSoundSuppressionSupported {
            settings.isShutterSoundSuppressionEnabled = true
        }
        #endif
        if let replaced = CameraModule.photoSettingsHatch?(settings) { settings = replaced }
        let delegate = PhotoDelegate { [weak self] result in
            DispatchQueue.main.async {
                self?.inFlight[settings.uniqueID] = nil
                done(result)
            }
        }
        inFlight[settings.uniqueID] = delegate
        let output = output
        // The capture turned upright as the person holds the device, and a front camera's photo not mirrored (D2).
        var angle: CGFloat?
        if #available(iOS 17.0, macOS 14.0, *) { angle = AVCaptureDevice.RotationCoordinator(device: source.device, previewLayer: nil).videoRotationAngleForHorizonLevelCapture }
        source.run { _ in
            if let connection = output.connection(with: .video) {
                if #available(iOS 17.0, macOS 14.0, *), let angle, connection.isVideoRotationAngleSupported(angle) { connection.videoRotationAngle = angle }
                if connection.isVideoMirroringSupported { connection.automaticallyAdjustsVideoMirroring = false; connection.isVideoMirrored = false }
            }
            guard output.connection(with: .video)?.isActive == true else { return delegate.finish(.failure(PhotoFailure("InvalidStateError"))) }
            output.capturePhoto(with: settings, delegate: delegate)
        }
    }
}

#if os(iOS)
/// What the camera app reads of the platform: a camera, the usage key, the camera's permission, and asking for it.
struct CameraAppPlatform {
    var available: () -> Bool = { UIImagePickerController.isSourceTypeAvailable(.camera) }
    var described: () -> Bool = { Bundle.main.object(forInfoDictionaryKey: "NSCameraUsageDescription") != nil }
    var status: () -> AVAuthorizationStatus = { AVCaptureDevice.authorizationStatus(for: .video) }
    var ask: (@escaping (Bool) -> Void) -> Void = { AVCaptureDevice.requestAccess(for: .video, completionHandler: $0) }
}

/// The system camera UI's delegate: the kept photo as JPEG with the camera's metadata, or the person's cancel, once.
final class CameraAppDelegate: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
    private var done: ((Result<Data, PhotoFailure>) -> Void)?
    init(_ done: @escaping (Result<Data, PhotoFailure>) -> Void) { self.done = done }

    func finish(_ result: Result<Data, PhotoFailure>) {
        let d = done
        done = nil
        d?(result)
    }

    func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
        picker.dismiss(animated: true)
        finish(Self.kept(info))
    }

    func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
        picker.dismiss(animated: true)
        finish(.failure(PhotoFailure("AbortError")))
    }

    /// The photo the picker's `info` holds, as JPEG.
    static func kept(_ info: [UIImagePickerController.InfoKey: Any]) -> Result<Data, PhotoFailure> {
        guard let image = info[.originalImage] as? UIImage, let data = jpeg(image, metadata: info[.mediaMetadata] as? [String: Any]) else {
            return .failure(PhotoFailure("UnknownError"))
        }
        return .success(data)
    }

    /// `image` as JPEG with the camera's metadata: the sensor's pixels, turned by the EXIF orientation the image has,
    /// as the camera writes them.
    static func jpeg(_ image: UIImage, metadata: [String: Any]?) -> Data? {
        guard let cg = image.cgImage else { return image.jpegData(compressionQuality: 0.9) }
        let out = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(out, "public.jpeg" as CFString, 1, nil) else { return nil }
        var properties = metadata ?? [:]
        properties[kCGImagePropertyOrientation as String] = orientation(image.imageOrientation)
        CGImageDestinationAddImage(destination, cg, properties as CFDictionary)
        return CGImageDestinationFinalize(destination) ? out as Data : nil
    }

    /// `UIImage.Orientation` as EXIF's (`CGImagePropertyOrientation`).
    static func orientation(_ o: UIImage.Orientation) -> Int {
        switch o {
        case .up: return 1
        case .upMirrored: return 2
        case .down: return 3
        case .downMirrored: return 4
        case .leftMirrored: return 5
        case .right: return 6
        case .rightMirrored: return 7
        case .left: return 8
        @unknown default: return 1
        }
    }
}
#endif

struct PhotoFailure: Error {
    let code: String
    init(_ code: String) { self.code = code }
}

/// One capture's delegate: the photo's file bytes, once.
final class PhotoDelegate: NSObject, AVCapturePhotoCaptureDelegate {
    private var done: ((Result<Data, PhotoFailure>) -> Void)?
    init(_ done: @escaping (Result<Data, PhotoFailure>) -> Void) { self.done = done }

    func finish(_ result: Result<Data, PhotoFailure>) {
        let d = done
        done = nil
        d?(result)
    }

    func photoOutput(_ output: AVCapturePhotoOutput, didFinishProcessingPhoto photo: AVCapturePhoto, error: Error?) {
        CameraModule.photoHatch?(photo)
        if error != nil { return finish(.failure(PhotoFailure("UnknownError"))) }
        guard let data = photo.fileDataRepresentation() else { return finish(.failure(PhotoFailure("UnknownError"))) }
        finish(.success(data))
    }

    func photoOutput(_ output: AVCapturePhotoOutput, didFinishCaptureFor resolvedSettings: AVCaptureResolvedPhotoSettings, error: Error?) {
        if error != nil { finish(.failure(PhotoFailure("UnknownError"))) }
    }
}
