// @ref LLP 1069.002 — the file picker on Apple. `showPicker(id)` presents
// PHPicker on iOS (no permission, no usage string; the `ExactMediaPicker`
// module's, LLP 1047.001.000 E) or a document picker for non-media or
// unrestricted input, and an NSOpenPanel sheet on macOS (D8). Native hosts don't enforce user activation (ruled, Q2). Each chosen
// file is copied into `app:/tmp/picked/` before `change` fires (D4), a HEIC
// photo becoming a JPEG when `accept` names images but not HEIC (D6,
// Safari's rule), with its pixel size and a video's duration (D3). Under
// the agent the request is held for `type @t <path>…` / `tap @t cancel`
// (D9), whose files take the same path.
import Foundation
import ImageIO
import CoreImage
import AVFoundation
import UniformTypeIdentifiers
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

#if os(iOS)
/// What the core asks of the photo picker's module, which links PhotosUI.
package protocol MediaPickerCapability: AnyObject {
    /// Present the photo picker over `controller` for a media-only
    /// `accept`. `done` runs once, on the main queue, with each chosen file
    /// staged in the temporary directory and the name to give it (none
    /// when cancelled), and why a chosen file could not be staged, if one
    /// could not; the caller removes the staged files.
    func present(over controller: UIViewController, accept: [String], multiple: Bool,
                 done: @escaping ([(url: URL, name: String)], String?) -> Void)
}

/// The photo picker's module, installed by the composition.
package enum MediaPickerLink {
    package static var installed: MediaPickerCapability?
}
#endif

/// The app's directories behind `app:/`, so an `image` source can read the
/// app's own file (D7): told by the library at boot and with each picked
/// name, so a photo kept in `app:/data` shows after a relaunch with nothing
/// picked (recipes F18).
enum AppFiles {
    private static let lock = NSLock()
    private static var roots: [String: URL] = [:]
    static func learn(_ reply: [String: Any]) {
        guard let r = reply["roots"] as? [String: String] else { return }
        lock.lock(); defer { lock.unlock() }
        for (name, path) in r { roots[name] = URL(fileURLWithPath: path, isDirectory: true) }
    }
    /// The roots the library has now (`appRoots`), before the first frame's
    /// images load and again once storage is configured.
    @discardableResult static func learn(_ runtime: Runtime) -> [String: String] {
        let reply = try? JSONSerialization.jsonObject(with: Data(runtime.agent("{\"op\":\"appRoots\"}").utf8))
        let object = reply as? [String: Any] ?? [:]
        learn(object)
        return object["roots"] as? [String: String] ?? [:]
    }
    /// Where an `app:/data|cache|tmp/…` path names, for a file the host
    /// writes there; nil for `..` or a root. A file the app names is read
    /// through `open` or `file`, never through this.
    static func url(_ path: String) -> URL? {
        guard let at = place(path) else { return nil }
        return at.names.reduce(at.root) { $0.appendingPathComponent($1) }
    }
    /// The regular file `path` names, opened through no symbolic link
    /// (`AppFileWalk`), as `readFile` and a share open it; nil for any other.
    static func open(_ path: String) -> FileHandle? {
        guard let at = place(path) else { return nil }
        return AppFileWalk.open(at.root, at.names)
    }
    /// The URL of the file `open` finds, for a reader that takes a URL (a
    /// player, picture in picture); nil where `open` refuses.
    static func file(_ path: String) -> URL? {
        guard let at = place(path), let handle = AppFileWalk.open(at.root, at.names) else { return nil }
        try? handle.close()
        return at.names.reduce(at.root) { $0.appendingPathComponent($1) }
    }
    private static func place(_ path: String) -> (root: URL, names: [String])? {
        guard path.hasPrefix("app:/") else { return nil }
        let parts = path.dropFirst(5).split(separator: "/", omittingEmptySubsequences: false).map(String.init)
        guard parts.count >= 2, !parts.dropFirst().contains(where: { $0.isEmpty || $0 == "." || $0 == ".." }) else { return nil }
        lock.lock(); let root = roots[parts[0]]; lock.unlock()
        return root.map { ($0, Array(parts.dropFirst())) }
    }
}

final class Picker: NSObject {
    unowned let session: ExactSession
    /// Inputs whose picker is showing, with the session generation that
    /// showed it: a second `showPicker` does nothing, and a replacement
    /// generation's input with the same id opens its own.
    private var open: [UInt32: Int] = [:]
    /// Chosen documents under `startAccessingSecurityScopedResource`, until
    /// the session ends (LLP 1069.010 D1).
    var scoped: [URL] = []
    /// The holds of document pickers a drive shows for real (`--touch
    /// platform`), by element: what the person's choice answers.
    var documentTickets: [UInt32: UInt64] = [:]
    #if canImport(UIKit)
    /// Those pickers, by element: what `back` or the agent's answer closes.
    var documentShown: [UInt32: UIViewController] = [:]
    private var requests: [ObjectIdentifier: Request] = [:]
    /// Exporting document pickers showing (LLP 1069.010 D3): the element
    /// the outcome goes to, and the scratch directory to remove.
    var exports: [ObjectIdentifier: (UInt32, URL)] = [:]
    /// Document pickers showing (LLP 1069.010 D2): the element, the command.
    var documentRequests: [ObjectIdentifier: (UInt32, String)] = [:]
    #endif
    init(session: ExactSession) { self.session = session }

    /// A picker shown for input `view`, in the session generation `issued`.
    struct Request { let view: UInt32; let id: String; let accept: [String]; let multiple: Bool; let issued: GenerationStamp }

    private func object(_ s: String) -> [String: Any] {
        (try? JSONSerialization.jsonObject(with: Data(s.utf8))) as? [String: Any] ?? [:]
    }
    private func quoted(_ s: String) -> String {
        let d = (try? JSONSerialization.data(withJSONObject: [s])) ?? Data("[\"\"]".utf8)
        return String(String(decoding: d, as: UTF8.self).dropFirst().dropLast())
    }

    /// `showPicker(id)`, from any action (D2).
    func show(_ args: [Any]) {
        guard let id = args.first as? String else { session.log("picker: refused: showPicker names an element id"); return }
        if ExactEnv.agentMode {
            let r = object(session.agent("{\"op\":\"showPicker\",\"id\":\(quoted(id))}"))
            if let e = r["error"] as? String { fputs("exact: \(e)\n", stderr) }
            return
        }
        if let request = request(id) { present(request) }
    }

    /// The request to show input `id`'s picker, now marked open; nil when it
    /// is no file input or its picker shows already.
    func request(_ id: String) -> Request? {
        let r = object(session.agent("{\"op\":\"picker\",\"id\":\(quoted(id))}"))
        guard let view = (r["view"] as? NSNumber)?.uint32Value else {
            session.log("picker: refused: no file input with id \"\(id)\""); return nil
        }
        guard open[view] != session.generation else { session.log("picker: \"\(id)\" is already open"); return nil }
        open[view] = session.generation
        return Request(view: view, id: id, accept: r["accept"] as? [String] ?? [], multiple: r["multiple"] as? Bool ?? false,
                       issued: GenerationStamp(session))
    }

    /// The platform's answer to `r`: the files chosen, or none when cancelled.
    /// A reload or plan replacement since it showed numbers nodes afresh, so
    /// `r.view` may now be another input: the answer is dropped.
    func finish(_ r: Request, files: [(url: URL, name: String)]) {
        guard r.issued.session != nil else { return }
        if files.isEmpty { cancel(r.view) } else { deliver(r.view, accept: r.accept, files: files) }
    }

    /// The agent's answer, after the library consumed the hold (D9): `reply`
    /// is the library's, `request` the driver's (its paths, never journalled).
    func answered(_ reply: [String: Any], request: [String: Any]) {
        guard reply["capability"] as? String == "pick", let view = (reply["node"] as? NSNumber)?.uint32Value else { return }
        if reply["answered"] as? String == "cancel" { cancel(view); return }
        let text = request["text"] as? String ?? ""
        var paths = text.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        if !text.contains("\n") { paths = text.split(whereSeparator: \.isWhitespace).map(String.init) }
        let accept = (session.presenter.views[view]?.props["accept"] ?? "").split(separator: ",")
            .map { $0.trimmingCharacters(in: .whitespaces).lowercased() }.filter { !$0.isEmpty }
        deliver(view, accept: accept, files: paths.map { (URL(fileURLWithPath: $0), ($0 as NSString).lastPathComponent) })
    }

    func cancel(_ view: UInt32) {
        open[view] = nil
        session.apply(session.runtime.pickerCancel(view, now: session.now()))
    }

    /// Copy each file in, converting per D6, describe it (D3), fire `change`.
    func deliver(_ view: UInt32, accept: [String], files: [(url: URL, name: String)]) {
        open[view] = nil
        var lines: [String] = []
        for f in files {
            let ext = (f.name as NSString).pathExtension.lowercased()
            let mime = UTType(filenameExtension: ext)?.preferredMIMEType ?? "application/octet-stream"
            let convert = Picker.converts(mime, accept: accept)
            let stored = convert ? (f.name as NSString).deletingPathExtension + ".jpg" : f.name
            let reply = object(session.agent("{\"op\":\"pickedPath\",\"name\":\(quoted(stored))}"))
            AppFiles.learn(reply)
            guard let path = reply["path"] as? String, let file = reply["file"] as? String else {
                session.log("picker: refused: no app:/tmp/picked/ for \(f.name)"); cancel(view); return
            }
            let target = URL(fileURLWithPath: file)
            do {
                try FileManager.default.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
                try? FileManager.default.removeItem(at: target)
                try Picker.copy(f.url, to: target, accept: accept, mime: mime)
            } catch {
                try? FileManager.default.removeItem(at: target)
                session.log("picker: refused: \(f.name): \(error.localizedDescription)"); cancel(view); return
            }
            let type = convert ? "image/jpeg" : mime
            let bytes = (try? target.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
            var size = ["", "", ""]
            if type.hasPrefix("image/"), let (w, h) = Picker.pixels(target) { size = ["\(w)", "\(h)", ""] }
            else if type.hasPrefix("video/") { size = Picker.video(target) }
            let name = f.name.replacingOccurrences(of: "\t", with: " ").replacingOccurrences(of: "\n", with: " ")
            lines.append(([path, name, type, "\(bytes)"] + size).joined(separator: "\t"))
        }
        session.apply(session.runtime.picked(view, lines.joined(separator: "\n"), now: session.now()))
    }

    static func converts(_ mime: String, accept: [String]) -> Bool {
        let heic = mime == "image/heic" || mime == "image/heif"
        let keeps = accept.contains { ["*/*", "image/*", "image/heic", "image/heif", ".heic", ".heif"].contains($0) }
        return heic && !keeps && accept.contains { $0.hasPrefix("image/") }
    }

    static func copy(_ source: URL, to target: URL, accept: [String], mime: String? = nil) throws {
        let type = mime ?? UTType(filenameExtension: source.pathExtension)?.preferredMIMEType ?? "application/octet-stream"
        if converts(type, accept: accept) { try jpeg(from: source, to: target) }
        else { try FileManager.default.copyItem(at: source, to: target) }
    }

    /// Pixels, orientation applied, from the header alone.
    static func pixels(_ url: URL) -> (Int, Int)? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let p = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let w = p[kCGImagePropertyPixelWidth] as? Int, let h = p[kCGImagePropertyPixelHeight] as? Int else { return nil }
        let orientation = p[kCGImagePropertyOrientation] as? Int ?? 1
        return (5...8).contains(orientation) ? (h, w) : (w, h)
    }

    /// A video's pixel size (the track's transform applied) and seconds.
    static func video(_ url: URL) -> [String] {
        let asset = AVURLAsset(url: url)
        var out = ["", "", ""]
        if let track = asset.tracks(withMediaType: .video).first {
            let s = track.naturalSize.applying(track.preferredTransform)
            out[0] = "\(Int(abs(s.width).rounded()))"; out[1] = "\(Int(abs(s.height).rounded()))"
        }
        let seconds = CMTimeGetSeconds(asset.duration)
        if seconds.isFinite { out[2] = "\(seconds)" }
        return out
    }

    /// D6: JPEG at quality 0.9, orientation applied, with no metadata copied
    /// (so no location). An HDR photo is written with an ISO 21496-1 gain
    /// map where ImageIO can encode it, preserving the color profile.
    /// A rejected HDR encode retries ImageIO's SDR rendition, then Core
    /// Image's Display P3 tone map if ImageIO still writes HDR (LLP 1100 D13).
    static func jpeg(from: URL, to: URL,
                     thumbnail: (CGImageSource, Int, CFDictionary?) -> CGImage? = CGImageSourceCreateThumbnailAtIndex,
                     finalize: (CGImageDestination) -> Bool = CGImageDestinationFinalize) throws {
        var accepted = false
        defer { if !accepted { try? FileManager.default.removeItem(at: to) } }
        guard let source = CGImageSourceCreateWithURL(from as CFURL, nil), let (w, h) = pixels(from)
        else { throw CocoaError(.fileReadCorruptFile) }
        func encode(_ image: CGImage, options: [CFString: Any]) throws {
            guard let destination = CGImageDestinationCreateWithURL(to as CFURL, UTType.jpeg.identifier as CFString, 1, nil)
            else { throw CocoaError(.fileWriteUnknown) }
            CGImageDestinationAddImage(destination, image, options as CFDictionary)
            guard finalize(destination) else { throw CocoaError(.fileWriteUnknown) }
        }
        func write(hdr: Bool) throws {
            let options = jpegOptions(maxPixel: max(w, h), hdr: hdr)
            guard let image = thumbnail(source, 0, options.decode as CFDictionary)
            else { throw CocoaError(.fileReadCorruptFile) }
            try encode(image, options: options.encode)
        }
        @available(iOS 18, macOS 15, tvOS 18, *)
        func isSDRResult(_ result: CGImageSource) -> Bool {
            guard !isHDR(result), let image = CGImageSourceCreateImageAtIndex(result, 0, nil)
            else { return false }
            // CIImage reports 0 for unknown headroom, 1 for SDR.
            // https://developer.apple.com/documentation/coreimage/ciimage/contentheadroom
            let headroom = CIImage(cgImage: image).contentHeadroom
            return headroom == 1 || (headroom == 0 && image.colorSpace.map(CGColorSpaceUsesITUR_2100TF) != true)
        }
        var wantsHDR = false
        var sdrFallback: Data?
        if #available(iOS 18, macOS 15, tvOS 18, *) { wantsHDR = isHDR(source) }
        if #available(iOS 18, macOS 15, tvOS 18, *), wantsHDR,
           (try? write(hdr: true)) != nil,
           let result = CGImageSourceCreateWithURL(to as CFURL, nil) {
            if CGImageSourceCopyAuxiliaryDataInfoAtIndex(result, 0, kCGImageAuxiliaryDataTypeISOGainMap) != nil,
               isHDR(result) { accepted = true; return }
            // An ignored gain-map request can still produce a usable SDR JPEG.
            if isSDRResult(result) { sdrFallback = try Data(contentsOf: to) }
        }
        do {
            try write(hdr: false)
            if #available(iOS 18, macOS 15, tvOS 18, *) {
                guard let result = CGImageSourceCreateWithURL(to as CFURL, nil)
                else { throw CocoaError(.fileWriteUnknown) }
                if !isSDRResult(result) {
                    // ImageIO can finalize a JPEG while ignoring both SDR requests.
                    guard wantsHDR, isHDR(result),
                          let image = CIImage(contentsOf: from, options: [.toneMapHDRtoSDR: true, .applyOrientationProperty: true]),
                          let working = CGColorSpace(name: CGColorSpace.extendedLinearDisplayP3),
                          let output = CGColorSpace(name: CGColorSpace.displayP3)
                    else { throw CocoaError(.fileWriteUnknown) }
                    let context = CIContext(options: [.workingColorSpace: working])
                    guard let bitmap = context.createCGImage(image, from: image.extent, format: .RGBA8, colorSpace: output)
                    else { throw CocoaError(.fileWriteUnknown) }
                    try encode(bitmap, options: [kCGImageDestinationLossyCompressionQuality: 0.9])
                    guard let converted = CGImageSourceCreateWithURL(to as CFURL, nil), isSDRResult(converted)
                    else { throw CocoaError(.fileWriteUnknown) }
                }
            }
        } catch {
            guard let sdrFallback else { throw error }
            try sdrFallback.write(to: to, options: .atomic)
        }
        accepted = true
    }

    static func jpegOptions(maxPixel: Int, hdr: Bool) -> (decode: [CFString: Any], encode: [CFString: Any]) {
        var decode: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixel,
        ]
        var encode: [CFString: Any] = [kCGImageDestinationLossyCompressionQuality: 0.9]
        if #available(iOS 18, macOS 15, tvOS 18, *) {
            decode[kCGImageSourceDecodeRequest] = hdr ? kCGImageSourceDecodeToHDR : kCGImageSourceDecodeToSDR
            encode[kCGImageDestinationEncodeRequest] = hdr ? kCGImageDestinationEncodeToISOGainmap : kCGImageDestinationEncodeToSDR
        }
        return (decode, encode)
    }

    /// A gain map of either kind, or a PQ or HLG transfer (LLP 1100 D4).
    static func isHDR(_ source: CGImageSource) -> Bool {
        if CGImageSourceCopyAuxiliaryDataInfoAtIndex(source, 0, kCGImageAuxiliaryDataTypeHDRGainMap) != nil { return true }
        if #available(iOS 18, macOS 15, tvOS 18, *),
           CGImageSourceCopyAuxiliaryDataInfoAtIndex(source, 0, kCGImageAuxiliaryDataTypeISOGainMap) != nil { return true }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)?.colorSpace.map(isHDRSpace) ?? false
    }

    /// `accept` as Apple's types: no filter is `.item`; `image/*` and
    /// `video/*` their families, a MIME type or extension its UTType.
    static func types(_ accept: [String]) -> [UTType] {
        if accept.isEmpty || accept.contains("*/*") { return [.item] }
        return accept.compactMap { token in
            switch token {
            case "image/*": return .image
            case "video/*": return .movie
            case let t where t.hasPrefix("."): return UTType(filenameExtension: String(t.dropFirst()))
            default: return UTType(mimeType: token)
            }
        }
    }
}

#if os(tvOS)
extension Picker {
    func present(_ r: Request) {
        // tvOS has no photo or document picker.
        session.log("picker: refused: no picker"); cancel(r.view)
    }
}
#elseif canImport(UIKit)
extension Picker: UIDocumentPickerDelegate {
    func present(_ r: Request) {
        guard let controller = ViewPresentation.presenter(for: session.presenter.views[r.view] ?? session.presenter.root) else {
            session.log("picker: refused: no window"); cancel(r.view); return
        }
        let media = !r.accept.isEmpty && r.accept.allSatisfy { $0.hasPrefix("image/") || $0.hasPrefix("video/") }
        if media {
            // A production plan that can show it is refused at boot without
            // it; a development plan that newly does needs a rebuild.
            guard let link = MediaPickerLink.installed else {
                session.log("picker: refused: this build links no photo picker (media_picker); rebuild"); cancel(r.view); return
            }
            link.present(over: controller, accept: r.accept, multiple: r.multiple) { [weak self] files, failure in
                // A chosen file that did not stage refuses the pick, as one
                // that does not copy in does (`deliver`): never a shorter list.
                if let failure {
                    if r.issued.session != nil { self?.session.log("picker: refused: \(failure)"); self?.cancel(r.view) }
                } else {
                    self?.finish(r, files: files)
                }
                for f in files { try? FileManager.default.removeItem(at: f.url) }
            }
        } else {
            let picker = UIDocumentPickerViewController(forOpeningContentTypes: Picker.types(r.accept), asCopy: true)
            picker.allowsMultipleSelection = r.multiple
            picker.delegate = self
            requests[ObjectIdentifier(picker)] = r
            controller.present(picker, animated: true)
        }
    }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        if exportFinished(controller, urls: urls) || documentFinished(controller, urls: urls) { return }
        guard let r = requests.removeValue(forKey: ObjectIdentifier(controller)) else { return }
        finish(r, files: urls.map { ($0, $0.lastPathComponent) })
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        if exportFinished(controller, urls: nil) || documentFinished(controller, urls: nil) { return }
        guard let r = requests.removeValue(forKey: ObjectIdentifier(controller)) else { return }
        finish(r, files: [])
    }
}
#else
extension Picker {
    static func panel(for r: Request) -> NSOpenPanel {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = r.multiple
        if !r.accept.isEmpty && !r.accept.contains("*/*") { panel.allowedContentTypes = types(r.accept) }
        return panel
    }

    func present(_ r: Request) {
        let panel = Picker.panel(for: r)
        let finish: (NSApplication.ModalResponse) -> Void = { [weak self, weak panel] response in
            guard let self else { return }
            let urls = response == .OK ? panel?.urls ?? [] : []
            let scoped = urls.map { $0.startAccessingSecurityScopedResource() }
            self.finish(r, files: urls.map { ($0, $0.lastPathComponent) })
            for (url, started) in zip(urls, scoped) where started { url.stopAccessingSecurityScopedResource() }
        }
        if let window = session.presenter.root.window { panel.beginSheetModal(for: window, completionHandler: finish) }
        else { panel.begin(completionHandler: finish) }
    }
}
#endif
