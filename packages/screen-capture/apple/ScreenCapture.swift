#if os(macOS)
// A still never requests permission or activates a window (LLP 1178 D3–D6).
import AppKit
import ExactPackage
import ScreenCaptureKit
import UniformTypeIdentifiers

public final class ScreenCaptureModule: ExactPackage {
    private let context: ExactPackageContext
    private var screens: [[String: Any]]?
    private var pending: [Int: CaptureTurn] = [:]
    private var ended = false
    private let askedKey = "exact.screen-capture.asked"
    private var observer: NSObjectProtocol?

    private let preflight: () -> Bool
    private let content: (@escaping (SCShareableContent?, Error?) -> Void) -> Void

    public convenience init(context: ExactPackageContext) {
        self.init(context: context, preflight: CGPreflightScreenCaptureAccess, content: {
            SCShareableContent.getExcludingDesktopWindows(false, onScreenWindowsOnly: true, completionHandler: $0)
        })
    }

    init(context: ExactPackageContext, preflight: @escaping () -> Bool,
         content: @escaping (@escaping (SCShareableContent?, Error?) -> Void) -> Void) {
        self.context = context; self.preflight = preflight; self.content = content
    }

    private func status() -> [String: Any] {
        let granted = context.agent || preflight()
        return ["available": true, "state": granted ? "granted" :
            (UserDefaults.standard.bool(forKey: askedKey) ? "denied" : "prompt")]
    }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        switch name {
        case "screenRecordingStatus":
            if observer == nil {
                observer = NotificationCenter.default.addObserver(forName: NSApplication.didBecomeActiveNotification,
                    object: nil, queue: .main) { [weak self] _ in self?.context.changed("screen-capture.screenRecordingStatus") }
            }
            reply(status())
        case "requestScreenRecording":
            if !context.agent {
                UserDefaults.standard.set(true, forKey: askedKey)
                _ = CGRequestScreenCaptureAccess()
            }
            context.changed("screen-capture.screenRecordingStatus")
            reply(status())
        case "captureScreens":
            guard context.agent || preflight() else {
                return reply(["error": "NotAllowedError", "message": "requestScreenRecording first"])
            }
            let turn = CaptureTurn()
            let request = context.request ?? 0
            pending[request] = turn
            // Snapshot viewport geometry at the call, even if publication
            // waits behind another capture into the same app directory.
            let fixture = (preferred as? [String: Any])?["screens"] as? [[String: Any]]
            let snapshots = context.agent ? fixtureScreens(fixture) : nativeScreens()
            CaptureQueue.add { [self] done in
                let finish: ([String: Any]) -> Void = { [self] answer in
                    pending.removeValue(forKey: request)
                    reply(answer)
                    done()
                }
                guard turn.held, !ended else { return finish(["error": "AbortError"]) }
                guard context.agent || preflight() else { return finish(["error":"NotAllowedError"]) }
                capture(snapshots, args: args, turn: turn, reply: finish)
            }
        default: reply(["error": "NotSupportedError"])
        }
    }

    public func preferred(_ values: [String: Any]) {
        screens = values["screens"] as? [[String: Any]]
    }
    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { ["error": "NotSupportedError"] }
    public func cancel(_ request: Int) { pending[request]?.held = false }
    public func release(_ name: String) {
        if name == "screenRecordingStatus", let observer { NotificationCenter.default.removeObserver(observer); self.observer = nil }
    }
    public func destroy(_ done: @escaping () -> Void) {
        ended = true
        for turn in pending.values { turn.held = false }
        release("screenRecordingStatus")
        done()
    }

    private func nativeScreens() -> [CaptureScreen] {
        NSScreen.screens.map { screen in
            let id = ExactScreenGeometry.id(screen)
            var record = ExactScreenGeometry.record(screen)
            record["clientRect"] = ExactScreenGeometry.client(screen, view: context.view)
            let mode = CGDisplayCopyDisplayMode(id)
            return CaptureScreen(id: id, record: record,
                width: mode?.pixelWidth ?? 0, height: mode?.pixelHeight ?? 0)
        }.sorted(by: CaptureScreen.ordered)
    }

    private func fixtureScreens(_ preferred: [[String: Any]]?) -> [CaptureScreen] {
        let records = screens ?? preferred ?? [["left":0,"top":0,"width":1440,"height":900,
            "availLeft":0,"availTop":0,"availWidth":1440,"availHeight":900,
            "devicePixelRatio":2,"isPrimary":true,"isInternal":true,"label":"Agent display",
            "clientRect":["x":0,"y":0,"width":1440,"height":900]]]
        return records.map { record in
            let scale = (record["devicePixelRatio"] as? NSNumber)?.doubleValue ?? 1
            func size(_ key: String) -> Int {
                let n = ((record[key] as? NSNumber)?.doubleValue ?? 0) * scale
                return n.isFinite && n > 0 && n < Double(Int32.max) ? Int(n.rounded()) : 0
            }
            var record = record
            record["clientRect"] = record["clientRect"] ?? NSNull()
            return CaptureScreen(id: 0, record: record, width: size("width"), height: size("height"))
        }.sorted(by: CaptureScreen.ordered)
    }

    private func capture(_ screens: [CaptureScreen], args: [String: Any], turn: CaptureTurn,
                         reply: @escaping ([String: Any]) -> Void) {
        guard !screens.isEmpty else { return reply(["error":"EncodingError", "message":"no display"]) }
        var total: UInt64 = 0
        for screen in screens {
            guard screen.width > 0, screen.height > 0 else { return reply(["error":"QuotaExceededError"]) }
            let pixels = UInt64(screen.width) * UInt64(screen.height)
            guard pixels <= 1 << 26 else { return reply(["error":"QuotaExceededError"]) }
            total += pixels
            guard total <= 1 << 27 else { return reply(["error":"QuotaExceededError"]) }
        }
        let (type, quality) = captureEncoding(args)
        let ext = type == .jpeg ? "jpg" : type.preferredFilenameExtension ?? "png"
        let directory = (args["directory"] as? String ?? "").trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        var files: [CaptureFile] = []
        var answers: [[String: Any]] = []
        do {
            for (index, screen) in screens.enumerated() {
                let path = "\(directory)/screen-\(index).\(ext)"
                guard let destination = context.file(path) else { throw CaptureFailure("NotAllowedError", "capture path is not writable") }
                let roots = ["data":context.data, "cache":context.cache, "tmp":context.temporary]
                let name = path.dropFirst(5).split(separator: "/").first.map(String.init) ?? ""
                guard let root = roots[name] else { throw CaptureFailure("NotAllowedError", "unknown app directory") }
                files.append(try CaptureFile(destination, root: root))
                answers.append(["path":path,"width":screen.width,"height":screen.height,
                                "type":type.preferredMIMEType ?? "image/png","screen":screen.record])
            }
        } catch { return reply(failure(error)) }
        let complete: ([SCDisplay]) -> Void = { [self] displays in
            guard turn.held, !ended else { return reply(["error":"AbortError"]) }
            if !context.agent && !preflight() { return reply(["error":"NotAllowedError"]) }
            var remaining = screens.count
            var error: Error?
            let finished: (Error?) -> Void = { result in
                if let result { error = error ?? result }
                remaining -= 1
                guard remaining == 0 else { return }
                guard turn.held, !self.ended else { return reply(["error":"AbortError"]) }
                if let error { return reply(self.failure(error)) }
                do {
                    for (index, file) in files.enumerated() {
                        try file.publish()
                        context.pictureChanged(answers[index]["path"] as! String)
                    }
                    reply(["captures":answers])
                } catch { reply(["error":"EncodingError", "message":"capture publication failed: \(error)"]) }
            }
            for (index, screen) in screens.enumerated() {
                let encoded: (CGImage?, Error?) -> Void = { image, error in
                    guard let image else {
                        return DispatchQueue.main.async { finished(error ?? CaptureFailure("EncodingError", "no captured image")) }
                    }
                    DispatchQueue.global(qos: .userInitiated).async {
                        var failure: Error?
                        do { try files[index].write(encodeCapture(image, type, quality)) }
                        catch { failure = error }
                        DispatchQueue.main.async { finished(failure) }
                    }
                }
                if context.agent { encoded(captureCard(index, screen.width, screen.height), nil); continue }
                guard let display = displays.first(where: { $0.displayID == screen.id }) else {
                    finished(CaptureFailure("EncodingError", "display disconnected before capture")); continue
                }
                let configuration = SCStreamConfiguration()
                configuration.width = screen.width
                configuration.height = screen.height
                configuration.showsCursor = false
                configuration.colorSpaceName = CGColorSpace.sRGB
                SCScreenshotManager.captureImage(contentFilter: SCContentFilter(display: display, excludingWindows: []),
                    configuration: configuration, completionHandler: encoded)
            }
        }
        if context.agent { complete([]) }
        else {
            content { content, error in
                DispatchQueue.main.async {
                    guard let content else { return reply(self.failure(error ?? CaptureFailure("EncodingError", "no displays"))) }
                    complete(content.displays)
                }
            }
        }
    }

    private func failure(_ error: Error) -> [String: Any] {
        (error as? CaptureFailure)?.answer ?? ["error":"EncodingError", "message":error.localizedDescription]
    }
}

private final class CaptureTurn { var held = true }
private struct CaptureScreen {
    let id: CGDirectDisplayID
    let record: [String: Any]
    let width: Int
    let height: Int
    static func ordered(_ a: Self, _ b: Self) -> Bool {
        let number = { (s: Self, key: String) in (s.record[key] as? NSNumber)?.doubleValue ?? 0 }
        return (number(a, "left"), number(a, "top")) < (number(b, "left"), number(b, "top"))
    }
}

private func captureCard(_ index: Int, _ width: Int, _ height: Int) -> CGImage? {
    guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
        bytesPerRow: width * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue), let data = context.data else { return nil }
    let bytes = data.assumingMemoryBound(to: UInt8.self)
    for pixel in 0..<(width * height) {
        bytes[pixel * 4] = UInt8(32 + (index % 6) * 30)
        bytes[pixel * 4 + 1] = 64; bytes[pixel * 4 + 2] = 128; bytes[pixel * 4 + 3] = 255
    }
    let glyphs = [[7,5,5,5,7],[2,6,2,2,7],[7,1,7,4,7],[7,1,7,1,7],[5,5,7,1,1],
                  [7,4,7,1,7],[7,4,7,5,7],[7,1,1,1,1],[7,5,7,5,7],[7,5,7,1,7],[0,5,2,5,0]]
    for (at, char) in "\(index) \(width)x\(height)".enumerated() {
        guard let glyph = char == "x" ? 10 : char.wholeNumberValue else { continue }
        for (y, row) in glyphs[glyph].enumerated() { for x in 0..<3 where row & (4 >> x) != 0 {
            for dy in 0..<4 { for dx in 0..<4 {
                let (px, py) = (8 + at * 16 + x * 4 + dx, 8 + y * 4 + dy)
                if px < width && py < height { for c in 0..<4 { bytes[(py * width + px) * 4 + c] = 255 } }
            }}
        }}
    }
    return context.makeImage()
}

#endif
