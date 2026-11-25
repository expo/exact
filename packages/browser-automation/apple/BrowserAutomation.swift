// @exact/browser-automation on Apple (LLP 1020.002 D9): each name goes to the
// iframe arm (`exact_web_automation`), which holds the frames. The runner has
// admitted the call under `browser.automation` before it reaches here. A frame
// is found by its `id` among the frames of this app: the arm knows each frame
// by its session's temporary directory, so a call never reaches another app's frames. An
// arm not yet loaded holds no frame: NotFoundError.
import ExactPackage
import Foundation

public final class BrowserAutomationModule: ExactPackage {
    private typealias Done = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32) -> Void
    private typealias Message = @convention(block) (NSDictionary) -> Void
    private typealias Automate = @convention(c) (UnsafePointer<UInt8>?, UInt32, UnsafeMutableRawPointer?, Done?) -> Void
    private final class Pending { let reply: ([String: Any]) -> Void; init(_ r: @escaping ([String: Any]) -> Void) { reply = r } }
    private let context: ExactPackageContext

    private var ended = false
    private typealias Hold = @convention(block) (Bool) -> Void
    private var reportedRetiredPost = false
    private let session = UUID().uuidString

    public init(context: ExactPackageContext) {
        self.context = context
    }

    public func destroy(_ done: @escaping () -> Void) {
        ended = true
        later("releaseScriptChannels", [:], preferred: nil) { _ in done() }
    }

    private func post(_ message: [String: Any]) {
        guard !ended else { return }
        if message["retiredPost"] as? Bool == true {
            if !reportedRetiredPost {
                reportedRetiredPost = true
                context.invoke("script.message", "script.message", ["retiredPost": true]) { _ in }
            }
            return
        }
        if message["overflow"] as? Bool == true {
            context.invoke("script.message", "script.message", ["overflow": true]) { _ in }
            return
        }
        var params = message
        guard let block = params.removeValue(forKey: "holdDelivery") else { return }
        let hold = unsafeBitCast(block as AnyObject, to: Hold.self)
        hold(true)
        context.invoke("script.message", "script.message", ["params": params]) { _ in
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(50)) { hold(false) }
        }
    }

    /// The arm, if this process already loaded it (`RTLD_NOLOAD` never loads it).
    private static func automate() -> Automate? {
        #if os(macOS)
        let path = Bundle.main.executableURL?.deletingLastPathComponent().appendingPathComponent("libexact_web.dylib").path ?? ""
        #else
        let directory = Bundle.main.privateFrameworksPath ?? Bundle.main.bundlePath
        let wrapped = directory + "/ExactWeb.framework/ExactWeb"
        let path = FileManager.default.fileExists(atPath: wrapped) ? wrapped : directory + "/libexact_web.dylib"
        #endif
        guard let library = dlopen(path, RTLD_NOW | RTLD_NOLOAD), let symbol = dlsym(library, "exact_web_automation") else { return nil }
        return unsafeBitCast(symbol, to: Automate.self)
    }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        let id = args["id"] as? String ?? ""
        guard let automate = Self.automate() else {
            if (args["target"] as? [String: Any])?["realm"] is String {
                return reply(["error": "no such frame", "message": "the script realm is no longer live"])
            }
            return reply(["error": "NotFoundError", "message": "\(name): no live iframe with id \(id)"])
        }
        var request = args
        request["op"] = name
        request["channelSession"] = session
        // The files are this package's to resolve, never the caller's to name.
        request["pathFile"] = nil
        request["destinationFolderFile"] = nil
        request["owner"] = context.temporary.standardizedFileURL.path
        // A path option is the app's own file (`app:/`), admitted by the runner against the app's `fs.write` lines.
        for option in ["path", "destinationFolder"] {
            guard let path = args[option] as? String else { continue }
            guard let file = context.file(path) else { return reply(["error": "SyntaxError", "message": "\(name): \(path) names no app file"]) }
            request["\(option)File"] = file.path
        }
        guard JSONSerialization.isValidJSONObject(request), var body = try? JSONSerialization.data(withJSONObject: request) else {
            return reply(["error": "SyntaxError", "message": "\(name): its arguments are not JSON"])
        }
        body.insert(2, at: 0)
        let deliver: Message = { [weak self] message in
            if let params = message as? [String: Any] { self?.post(params) }
        }
        let callbacks: NSArray = [Pending(reply), deliver as AnyObject]
        let pending = Unmanaged.passRetained(callbacks).toOpaque()
        let done: Done = { context, bytes, length in
            let callbacks = Unmanaged<NSArray>.fromOpaque(context!).takeRetainedValue()
            let pending = callbacks.object(at: 0) as! Pending
            let data = Data(UnsafeBufferPointer(start: bytes, count: Int(length)))
            let answer = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? ["error": "UnknownError", "message": "the web arm's answer was not JSON"]
            pending.reply(answer)
        }
        body.withUnsafeBytes { automate($0.bindMemory(to: UInt8.self).baseAddress, UInt32(body.count), pending, done) }
    }
}
