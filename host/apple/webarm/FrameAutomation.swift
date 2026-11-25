// Script, input and prompts for a partitioned frame's guest (@ref LLP
// 1020.002 D9), by WebDriver BiDi's names: `@exact/browser-automation`'s
// package calls `exact_web_automation`, after the runner admitted the call
// under `browser.automation`. Only a partitioned frame is always the web
// view's own top-level browsing context, so only it is driven; a picture
// (D7) is any frame's.
import CryptoKit
import Foundation
import WebKit

#if os(macOS)
import AppKit
#else
import UIKit
#endif

struct GuestEvaluation: Decodable {
    let script: String
    let viewPoint: [Double]?
}

extension WebArm {
    func evaluate(token: UInt32, request: GuestEvaluation) {
        guard pdf == nil else { return sendReply(token: token, kind: 2, text: "a PDF frame has no document to script") }
        guard direct ? !serving : guestFrame != nil else {
            return sendReply(token: token, kind: 2, text: "guest frame is not ready")
        }
        let web = webView, frame = direct ? nil : guestFrame
        guard let point = request.viewPoint else {
            web.evaluateJavaScript(request.script, in: frame, in: world) { [weak self] in self?.evaluated(token: token, $0) }
            return
        }
        guard point.count == 2 else { return sendReply(token: token, kind: 2, text: "guest tap needs two coordinates") }
        #if os(macOS)
        let left = 0.0, top = 0.0
        let scale = Double(web.pageZoom * web.magnification)
        #else
        let inset = web.scrollView.adjustedContentInset
        let left = Double(inset.left), top = Double(inset.top)
        let scale = Double(web.pageZoom * web.scrollView.zoomScale)
        #endif
        // WebKit's visual viewport may be panned. Measure it in the main
        // document, which also owns the sandbox wrapper's iframe rectangle.
        let arguments: [String: Any] = ["x": (point[0] - left) / scale, "y": (point[1] - top) / scale, "wrapped": !direct]
        let map = """
        const x0 = x + visualViewport.pageLeft - scrollX, y0 = y + visualViewport.pageTop - scrollY;
        if (!wrapped) return [x0, y0];
        const box = document.getElementById('exact-frame').getBoundingClientRect();
        return [x0 - box.left, y0 - box.top];
        """
        web.callAsyncJavaScript(map, arguments: arguments, in: nil, in: world) { [weak self, weak web] result in
            guard let self, let web else { return }
            guard self.built === web, frame === (self.direct ? nil : self.guestFrame),
                  case .success(let value) = result, let mapped = value as? [Double], mapped.count == 2 else {
                return self.sendReply(token: token, kind: 2, text: "guest tap could not map the point into the frame")
            }
            web.callAsyncJavaScript("return " + request.script, arguments: ["guestPoint": mapped], in: frame, in: self.world) {
                [weak self] in self?.evaluated(token: token, $0)
            }
        }
    }
}

/// The arms automation can reach: each knows its element id and the
/// temporary folder of its session's module (`owner`), which the package's
/// context resolves `app:/tmp/` in: the app's, so another app's frames are
/// never reached.
enum Automations {
    nonisolated(unsafe) static let arms = NSHashTable<WebArm>.weakObjects()
    static let world = WKContentWorld.world(name: "exact.automation")
    /// A result's JSON, at most this many bytes (the clone's limit).
    static let limit = 64 * 1024

    static let missingRealm = ["error": "no such frame", "message": "the script realm is no longer live"]

    static func run(_ request: [String: Any], message: AutomationMessage?, finish: @escaping ([String: Any]) -> Void) {
        if request["op"] as? String == "releaseScriptChannels", let session = request["channelSession"] as? String,
           let owner = request["owner"] as? String, !owner.isEmpty {
            FrameAutomation.closeMessageQueues(owner: owner, session: session)
            for arm in arms.allObjects where arm.owner == owner { arm.automation?.releaseChannels(session) }
            return finish([:])
        }
        var options = request
        let id: String
        if let raw = request["target"], !(raw is NSNull) {
            guard let target = raw as? [String: Any], ["realm", "context", "sandbox"].allSatisfy({
                target[$0] == nil || target[$0] is NSNull || target[$0] is String
            }) else { return finish(["error": "invalid argument", "message": "use a string context and optional sandbox, or a string realm"]) }
            guard (request["id"] == nil || request["id"] is NSNull), (request["sandbox"] == nil || request["sandbox"] is NSNull),
                  ["evaluate", "callFunction"].contains(request["op"] as? String ?? "") else {
                return finish(["error": "invalid argument", "message": "use one script target"])
            }
            if let realm = target["realm"] as? String {
                guard target["context"] as? String == nil, target["sandbox"] as? String == nil else {
                    return finish(["error": "invalid argument", "message": "a realm selects its own context and sandbox"])
                }
                let owner = request["owner"] as? String ?? ""
                for arm in arms.allObjects where !arm.invalidated && !owner.isEmpty && arm.owner == owner {
                    if arm.pdf == nil, arm.browser != nil, arm.built != nil, let automation = arm.automation, let name = automation.name(of: realm) {
                        options["sandbox"] = name.isEmpty ? nil : name
                        return automation.run(options, message: message, reply: finish)
                    }
                }
                return finish(missingRealm)
            }
            guard let context = target["context"] as? String else {
                return finish(["error": "invalid argument", "message": "a script target needs context or realm"])
            }
            id = context
            options["sandbox"] = target["sandbox"]
        } else {
            guard let context = request["id"] as? String else {
                return finish(["error": "invalid argument", "message": "a script needs an iframe id or a target"])
            }
            id = context
        }
        let found = find(owner: request["owner"] as? String ?? "", id: id)
        guard found.count == 1, let arm = found.first else {
            return finish(["error": "NotFoundError", "message": "no unique live iframe with id \(id)"])
        }
        let automation = arm.automation ?? FrameAutomation(arm: arm)
        arm.automation = automation
        automation.run(options, message: message, reply: finish)
    }

    static func find(owner: String, id: String) -> [WebArm] {
        arms.allObjects.filter { !$0.invalidated && !owner.isEmpty && $0.owner == owner && $0.element == id }
    }
}

final class FrameAutomation {
    weak var arm: WebArm?
    /// The frame is the agent's (`controller: "agent"`) until a person's
    /// pointer or key input reaches it.
    private(set) var controlled = false
    private var actingScripts = Set<Int>()

    private func scriptActed(_ number: Int) {
        if actingScripts.remove(number) != nil { take() }
    }

    private func stopWatchingAct(_ number: Int) {
        actingScripts.remove(number)
        if actingScripts.isEmpty && !controlled { unwatch() }
    }
    private var preload: [(id: String, script: WKUserScript, session: String?, seededWorld: String?)] = []
    private final class Messages: NSObject, WKScriptMessageHandlerWithReply {
        let queue: ScriptMessageQueue
        private var flushing: [ObjectIdentifier: WKWebView] = [:]
        let handler = "_message" + UUID().uuidString.replacingOccurrences(of: "-", with: "")
        private struct Capability {
            let channel: String
            var pending = 0
            var admitted = false
        }
        private var issued: [String: Capability] = [:]
        func reserve(_ channel: String) -> String {
            let key = issued.first(where: { $0.value.channel == channel })?.key ?? UUID().uuidString
            issued[key, default: Capability(channel: channel)].pending += 1
            return key
        }

        func finish(_ keys: [String], admitted: Bool) {
            for key in keys {
                guard var value = issued[key] else { continue }
                value.pending -= 1
                value.admitted = value.admitted || admitted
                issued[key] = value.admitted || value.pending > 0 ? value : nil
            }
        }
        var isEmpty: Bool { issued.isEmpty }
        let world: WKContentWorld
        let route: String
        private var deliver: AutomationMessage?
        weak var arm: WebArm?
        private let views = NSHashTable<WKWebView>.weakObjects()
        private weak var current: WKWebView?
        private let controllers = NSHashTable<WKUserContentController>.weakObjects()

        init(_ arm: WebArm, _ world: WKContentWorld, _ route: String, _ session: String, _ deliver: @escaping AutomationMessage) {
            self.arm = arm
            self.world = world
            self.route = route
            self.deliver = deliver
            let key = SessionKey(owner: arm.owner, session: session)
            let channels = FrameAutomation.sessions[key] ?? SessionChannels()
            FrameAutomation.sessions[key] = channels
            queue = channels.queue
            super.init()
            channels.receivers.add(self)
        }

        private final class Registration {
            let remove: () -> Void
            init(_ remove: @escaping () -> Void) { self.remove = remove }
            deinit { remove() }
        }

        func install(_ view: WKWebView) {
            current = view
            let controller = view.configuration.userContentController
            if !views.contains(view) {
                views.add(view)
                let registration = Registration { [weak self, weak controller] in
                    DispatchQueue.main.async { [weak self, weak controller] in
                        guard let self, let controller, self.controllers.contains(controller),
                              !self.views.allObjects.contains(where: { $0.configuration.userContentController === controller }) else { return }
                        controller.removeScriptMessageHandler(forName: self.handler, contentWorld: self.world)
                        self.controllers.remove(controller)
                    }
                }
                // A hatch can retain the controller after the last view using its handler dies.
                objc_setAssociatedObject(view, Unmanaged.passUnretained(self).toOpaque(), registration, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
            }
            if !controllers.contains(controller) {
                controllers.add(controller)
                controller.addScriptMessageHandler(self, contentWorld: world, name: handler)
            }
        }

        func uninstall() {
            for controller in controllers.allObjects { controller.removeScriptMessageHandler(forName: handler, contentWorld: world) }
            controllers.removeAllObjects()
            for view in views.allObjects {
                objc_setAssociatedObject(view, Unmanaged.passUnretained(self).toOpaque(), nil, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
            }
            views.removeAllObjects()
            issued.removeAll()
            flushing.removeAll()
            deliver = nil
        }

        func retire(_ view: WKWebView) {
            guard current === view else { return }
            current = nil
            // WebKit discards queued callbacks when their view dies. Session close can
            // release the view even if its renderer never services the flush.
            let id = ObjectIdentifier(view)
            flushing[id] = view
            view.evaluateJavaScript("void 0", in: nil, in: world) { [weak self] _ in
                self?.flushing.removeValue(forKey: id)
            }
        }

        func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage,
                                   replyHandler: @escaping (Any?, String?) -> Void) {
            guard let view = message.webView, views.contains(view), message.frameInfo.isMainFrame,
                  let parts = message.body as? [String] else { return replyHandler(nil, nil) }
            if parts.first == "bind" {
                let keys = parts.dropFirst()
                guard view === current, !keys.isEmpty, keys.allSatisfy({ issued[$0] != nil }) else { return replyHandler(nil, "channel closed") }
                for key in keys { issued[key]?.admitted = true }
                return replyHandler(true, nil)
            }
            defer { replyHandler(nil, nil) }
            guard parts.count == 4, let channel = issued[parts[0]]?.channel, UUID(uuidString: parts[1]) != nil else { return }
            let start = ScriptMessageQueue.Start(handler: handler, view: ObjectIdentifier(view), turn: parts[2])
            if parts[3].isEmpty { return queue.publish(start) }
            guard parts[3].utf8.count <= Automations.limit else { return }
            guard view === current, let arm, !arm.invalidated, arm.built === view else {
                deliver?(["retiredPost": true])
                return
            }
            let origin = WebArm.spelled(message.frameInfo.securityOrigin)
            let source = ["realm": FrameAutomation.realmId(route, parts[1], origin), "origin": origin, "context": arm.element]
            queue.admit(start, ["channel": channel, "json": parts[3], "source": source]) { [self] in deliver?($0) }
        }
    }

    private struct SessionKey: Hashable { let owner: String, session: String }
    private final class SessionChannels {
        let queue = ScriptMessageQueue()
        let receivers = NSHashTable<Messages>.weakObjects()
    }
    // A package handler can still be running when its last frame goes away.
    nonisolated(unsafe) private static var sessions: [SessionKey: SessionChannels] = [:]

    static func closeMessageQueues(owner: String, session: String) {
        let key = SessionKey(owner: owner, session: session)
        guard let channels = sessions.removeValue(forKey: key) else { return }
        channels.queue.close()
        for receiver in channels.receivers.allObjects { receiver.uninstall() }
    }

    private struct Destination: Hashable { let sandbox: String, session: String }
    private var messages: [Destination: Messages] = [:]
    private var seeds: [String: WKUserScript] = [:]

    // A public realm never reveals the world's private document token. Include WebKit's origin so
    // a token presented from another origin cannot name the original document.
    private static func realmId(_ route: String, _ token: String, _ origin: String) -> String {
        let digest = SHA256.hash(data: Data((origin + "\0" + token).utf8))
        return route + "." + Data(digest).base64EncodedString()
    }

    private var nextId = 0
    private struct Realm {
        let id: String, token: String, origin: String
    }
    private static let binding = "_exact" + UUID().uuidString.replacingOccurrences(of: "-", with: "")
    private var routes: [String: String] = [:]
    private var identified: [String: Realm] = [:]

    func name(of id: String) -> String? {
        guard let separator = id.firstIndex(of: ".") else { return nil }
        return routes.first { $0.value == id[..<separator] }?.key
    }

    // Capture the page's origin before any script on its final content controller runs.
    private var pageSeed: String {
        """
        const \(Self.binding) = Object.freeze({ document, origin: this.origin,
          token: Array.from(crypto.getRandomValues(new Uint8Array(16)), n => n.toString(16).padStart(2, '0')).join('') });
        """
    }

    func installPageSeed(_ controller: WKUserContentController) {
        // WebKit can return a live array. Copy its elements before removing or adding scripts.
        let source = pageSeed
        if controller.userScripts.first?.source == source { return }
        let scripts = controller.userScripts.filter { $0.source != source }
        controller.removeAllUserScripts()
        controller.addUserScript(WKUserScript(source: source, injectionTime: .atDocumentStart, forMainFrameOnly: true, in: .page))
        scripts.forEach(controller.addUserScript)
    }

    // The initial empty document gets no document-start event. Seed it before the view is exposed.
    func seedInitialDocument(_ web: WKWebView) {
        web.evaluateJavaScript(pageSeed, in: nil, in: .page, completionHandler: nil)
    }

    /// The named world's post pairs its token with WebKit's origin in the same frame callback.
    /// A configuration hatch may already have installed scripts in this world.
    private final class Source: NSObject, WKScriptMessageHandlerWithReply {
        weak var view: WKWebView?
        var identity: (String, String)?
        init(_ view: WKWebView) { self.view = view }
        func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage,
                                   replyHandler: @escaping (Any?, String?) -> Void) {
            if identity == nil, message.webView === view, message.frameInfo.isMainFrame,
               let token = message.body as? String, UUID(uuidString: token) != nil {
                identity = (token, WebArm.spelled(message.frameInfo.securityOrigin))
            }
            replyHandler(true, nil)
        }
    }

    private func identify(_ name: String, _ world: WKContentWorld, _ web: WKWebView, _ number: Int, targeted: Bool, then: @escaping (Realm?) -> Void) {
        if !targeted, let known = identified[name] { return then(known) }
        let route = routes[name] ?? UUID().uuidString
        routes[name] = route
        func realm(_ token: String, _ origin: String) -> Realm {
            let value = Realm(id: Self.realmId(route, token, origin), token: token, origin: origin)
            identified[name] = value
            return value
        }
        let bound = "typeof \(Self.binding) !== 'undefined' && \(Self.binding).document === document"
        if name.isEmpty {
            web.evaluateJavaScript("\(bound) ? [\(Self.binding).token, \(Self.binding).origin] : null", in: nil, in: world) { result in
                if case .success(let pair as [String]) = result, pair.count == 2 { then(realm(pair[0], pair[1])) }
                else { then(nil) }
            }
            return
        }
        let controller = web.configuration.userContentController
        let source = Source(web), handler = "_origin" + UUID().uuidString.replacingOccurrences(of: "-", with: "")
        controller.addScriptMessageHandler(source, contentWorld: world, name: handler)
        originHandlers[number] = { controller.removeScriptMessageHandler(forName: handler, contentWorld: world) }
        let token = UUID().uuidString
        func inspect(_ maySeed: Bool) {
            guard self.evaluations[number] != nil else { return }
            let post = "if (!(\(bound))) return false; return await webkit.messageHandlers.\(handler).postMessage(\(Self.binding).token);"
            web.callAsyncJavaScript(post, arguments: [:], in: nil, in: world) { result in
                guard self.evaluations[number] != nil else { return }
                if let (marker, origin) = source.identity {
                    self.originHandlers.removeValue(forKey: number)?()
                    then(realm(marker, origin))
                } else if maySeed, case .success(let found as Bool) = result, !found {
                    let seed = "const \(Self.binding) = { __proto__: null, document, token: '\(token)' }; true"
                    web.evaluateJavaScript(seed, in: nil, in: world) { _ in inspect(false) }
                } else {
                    self.originHandlers.removeValue(forKey: number)?()
                    then(nil)
                }
            }
        }
        inspect(!targeted)
    }

    /// Evaluations in flight in the current document, by number.
    private var originHandlers: [Int: () -> Void] = [:]
    private var identifying = Set<Int>()
    private var evaluations: [Int: ([String: Any]) -> Void] = [:]
    /// `captureScreenshot` calls in flight, by number: they end with the
    /// frame, as evaluations do, and outlive a navigation.
    private var captures: [Int: ([String: Any]) -> Void] = [:]
    /// A `performActions` in flight: its number and how it ends early.
    private var acting: (id: Int, end: ([String: Any]) -> Void)?
    private var keys: [String: Set<String>] = [:]
    private var buttons: [String: Set<Int>] = [:]
    private var pointers: [String: CGPoint] = [:]
    private var lastClick: (button: Int, at: TimeInterval, count: Int, point: CGPoint)?
    /// Where an agent-controlled frame's downloads go, with no panel.
    private(set) var downloads: (folder: URL, shown: String)?
    /// The dialog the agent answers, held while the frame is its.
    private var prompt: (show: () -> Void, answer: (String?) -> Void, type: String, defaultValue: String?)?
    #if os(macOS)
    private var monitor: Any?
    #else
    private var watcher: PersonTouches?
    #endif

    init(arm: WebArm) { self.arm = arm }

    // MARK: Control

    /// An automation call that acts takes the frame (all but reads and pictures).
    func take() {
        guard !controlled, let arm, let web = arm.built else { return }
        controlled = true
        watch(web)
        arm.browser?.controller = "agent"
        arm.browser?.send("currententrychange")
    }

    /// A person's input reached the frame: input in flight ends, pressed keys
    /// are released, and a held dialog is shown to the person. True when one
    /// was: the input that took the frame is then the dialog's to stop, not
    /// the page's.
    @discardableResult func personTook() -> Bool {
        actingScripts.removeAll()
        guard controlled, let arm else { unwatch(); return false }
        controlled = false
        unwatch()
        acting?.end(["error": "AbortError", "message": "a person took the frame back"])
        acting = nil
        #if os(macOS)
        focus = nil
        #endif
        releaseKeys()
        buttons = [:]
        let held = prompt
        prompt = nil
        held?.show()
        arm.browser?.controller = "person"
        arm.browser?.send("currententrychange")
        return held != nil
    }

    /// The frame unmounted or changed its web view: every call in flight ends.
    func end(_ why: String) {
        if let view = arm?.built {
            removeScripts(preload.map(\.script) + seeds.values, from: view)
            messages.values.forEach { $0.retire(view) }
        }
        let answer: [String: Any] = ["error": "NotFoundError", "message": why]
        endEvaluations(answer)
        let pending = captures.values
        captures = [:]
        pending.forEach { $0(answer) }
        acting?.end(answer)
        acting = nil
        keys = [:]; buttons = [:]; pointers = [:]
        #if os(macOS)
        focus = nil
        #endif
        if let held = prompt { prompt = nil; held.answer(nil) }
        controlled = false
        unwatch()
    }

    /// A new document committed: an evaluation in the old one is aborted, as
    /// BiDi's realm is destroyed; input state is kept.
    func committed() {
        endEvaluations(["error": "AbortError", "message": "the document navigated away"])
    }

    /// The web content process ended: its documents, and their evaluations, with it.
    func terminated() {
        endEvaluations(["error": "AbortError", "message": "the web content process ended"])
    }

    private func endEvaluations(_ answer: [String: Any]) {
        identified.removeAll()
        actingScripts.removeAll()
        if !controlled { unwatch() }
        let pending = evaluations
        let checking = identifying
        let remove = originHandlers.values
        originHandlers = [:]
        remove.forEach { $0() }
        evaluations = [:]
        identifying.removeAll()
        for (number, reply) in pending { reply(checking.contains(number) ? Automations.missingRealm : answer) }
    }

    private func installSeed(_ seed: WKUserScript, on controller: WKUserContentController) {
        let scripts = controller.userScripts.filter { $0 !== seed }
        controller.removeAllUserScripts()
        controller.addUserScript(seed)
        scripts.forEach(controller.addUserScript)
    }

    /// A new web view for the same frame (a partition change): the preload scripts go with it.
    func attached(_ view: WKWebView) {
        let controller = view.configuration.userContentController
        messages.values.forEach { $0.install(view) }
        for seed in seeds.values where !controller.userScripts.contains(where: { $0 === seed }) { installSeed(seed, on: controller) }
        for entry in preload where !controller.userScripts.contains(where: { $0 === entry.script }) { controller.addUserScript(entry.script) }
    }

    /// An agent-controlled frame's dialog: no sheet; `userpromptopened` fires
    /// and `handleUserPrompt` answers it. False when the frame is a person's.
    func hold(type: String, message: String, defaultValue: String?, show: @escaping () -> Void, answer: @escaping (String?) -> Void) -> Bool {
        guard controlled, prompt == nil, let arm else { return false }
        prompt = (show, answer, type, defaultValue)
        arm.emitRecord("userpromptopened", ["type": type, "message": message, "defaultValue": defaultValue ?? ""])
        return true
    }

    // MARK: Calls

    private struct ScriptArguments {
        let source: String
        var finish: ((Bool) -> Void)?
    }

    func run(_ request: [String: Any], message: AutomationMessage?, reply: @escaping ([String: Any]) -> Void) {
        guard let arm else { return reply(["error": "NotFoundError"]) }
        let op = request["op"] as? String ?? ""
        let id = request["id"] as? String ?? ""
        if op == "captureScreenshot" {
            let path = request["path"] as? String ?? ""
            var command: [String: Any] = ["op": op, "path": path, "file": request["pathFile"] as? String ?? ""]
            if let clip = request["clip"] as? [String: Any] { command["clip"] = ["x", "y", "width", "height"].map { (clip[$0] as? NSNumber) ?? 0 } }
            nextId += 1
            let number = nextId
            captures[number] = reply
            return arm.capture(command) { [weak self] width, height, error, message in
                guard let reply = self?.captures.removeValue(forKey: number) else { return }
                reply(error.isEmpty ? ["path": path, "width": width, "height": height] : ["error": error, "message": message])
            }
        }
        guard arm.pdf == nil, arm.browser != nil, let web = arm.built else {
            return reply(["error": "NotSupportedError", "message": "\(op): iframe \(id) has no partition; only a partitioned frame is driven"])
        }
        var arguments = ScriptArguments(source: "...__exactArguments")
        if op == "callFunction" || op == "addPreloadScript" {
            guard let parsed = scriptArguments(request, message: message, preload: op == "addPreloadScript", web: web, reply: reply) else { return }
            arguments = parsed
        }
        switch op {
        case "evaluate", "callFunction":
            script(op, request, arguments, web) { result in arguments.finish?(false); reply(result) }
        // A preload script is a read, as a script without `userActivation`
        // is: it runs in each new document, where it can watch what the page
        // does from its first script on, and leaves the frame the person's.
        case "addPreloadScript":
            guard let world = world(request, reply) else { return }
            nextId += 1
            let script = WKUserScript(source: "(\n\(request["functionDeclaration"] as? String ?? "")\n)(\(arguments.source));", injectionTime: .atDocumentStart, forMainFrameOnly: true, in: world)
            let hasChannels = !(request["arguments"] as? [Any] ?? []).isEmpty
            preload.append(("script-\(nextId)", script, request["channelSession"] as? String, hasChannels ? request["sandbox"] as? String : nil))
            web.configuration.userContentController.addUserScript(script)
            arm.ownScripts = web.configuration.userContentController.userScripts.count
            arguments.finish?(true)
            reply(["script": "script-\(nextId)"])
        case "removePreloadScript":
            let named = request["script"] as? String ?? ""
            guard let index = preload.firstIndex(where: { $0.id == named }) else { return reply(["error": "NotFoundError", "message": "no preload script \(named)"]) }
            let removed = preload.remove(at: index)
            removeScripts([removed.script] + unusedSeeds(), from: web)
            reply([:])
        case "performActions": perform(request["actions"] as? [[String: Any]] ?? [], web, reply)
        case "releaseActions":
            take()
            release(web)
            reply([:])
        case "handleUserPrompt":
            guard let held = prompt else { return reply(["error": "NotFoundError", "message": "no dialog is open in iframe \(id)"]) }
            prompt = nil
            let accept = request["accept"] as? Bool ?? true
            held.answer(!accept ? nil : held.type == "prompt" ? request["userText"] as? String ?? held.defaultValue ?? "" : "")
            reply([:])
        case "setDownloadBehavior":
            take()
            downloads = (request["destinationFolderFile"] as? String).map { (URL(fileURLWithPath: $0, isDirectory: true), request["destinationFolder"] as? String ?? $0) }
            reply([:])
        default: reply(["error": "NotSupportedError", "message": "\(op) is not a call of @exact/browser-automation"])
        }
    }

    private func removeScripts(_ scripts: [WKUserScript], from web: WKWebView) {
        guard !scripts.isEmpty else { return }
        let controller = web.configuration.userContentController
        let kept = controller.userScripts.filter { present in !scripts.contains(where: { $0 === present }) }
        controller.removeAllUserScripts()
        kept.forEach(controller.addUserScript)
        arm?.ownScripts = controller.userScripts.count
    }

    private func unusedSeeds() -> [WKUserScript] {
        let needed = Set(preload.compactMap(\.seededWorld))
        return seeds.keys.filter { !needed.contains($0) }.compactMap { seeds.removeValue(forKey: $0) }
    }

    func releaseChannels(_ session: String) {
        messages = messages.filter { $0.key.session != session }
        let removed = preload.filter { $0.session == session }.map(\.script)
        preload.removeAll { $0.session == session }
        let scripts = removed + unusedSeeds()
        if let web = arm?.built { removeScripts(scripts, from: web) }
    }

    /// A ChannelValue becomes a function in the named world. The JSON string crosses WebKit once,
    /// after the guest checked its size, so a cyclic or oversized value throws at the call to post.
    private func scriptArguments(_ request: [String: Any], message: AutomationMessage?, preload: Bool, web: WKWebView, reply: ([String: Any]) -> Void) -> ScriptArguments? {
        let args = request["arguments"] as? [Any] ?? []
        let properties: [[String: Any]?] = args.map { value in
            guard let object = value as? [String: Any], object.count == 2, object["type"] as? String == "channel",
                  let props = object["value"] as? [String: Any], props.count == 1, props["channel"] is String else { return nil }
            return props
        }
        guard properties.contains(where: { $0 != nil }) || (preload && !args.isEmpty) else { return ScriptArguments(source: preload ? "" : "...__exactArguments") }
        guard let name = request["sandbox"] as? String, !name.isEmpty else {
            reply(["error": "NotSupportedError", "message": "script.message requires a named sandbox"])
            return nil
        }
        guard let arm, let world = world(request, reply) else { return nil }
        guard let message else {
            reply(["error": "NotSupportedError", "message": "script.message has no receiver"])
            return nil
        }
        if preload && properties.contains(where: { $0 == nil }) {
            reply(["error": "invalid argument", "message": "a ChannelValue needs value: { channel: string }"])
            return nil
        }
        var expressions: [String] = []
        var reserved: [String] = []
        let route = routes[name] ?? UUID().uuidString
        let destination = Destination(sandbox: name, session: request["channelSession"] as? String ?? "")
        let sink = messages[destination] ?? Messages(arm, world, route, destination.session, message)
        for (i, props) in properties.enumerated() {
            guard let channel = props?["channel"] as? String else { expressions.append("__exactArguments[\(i)]"); continue }
            let capability = sink.reserve(channel)
            reserved.append(capability)
            expressions.append("""
            ((token, send, capability) => {
              let turn = null;
              return value => {
                const json = JSON.stringify(value);
                if (json === undefined) throw new TypeError('script.message needs JSON');
                if (new TextEncoder().encode(json).length > \(Automations.limit)) throw new DOMException('script.message exceeds 65536 bytes', 'QuotaExceededError');
                if (turn === null) {
                  turn = Array.from(crypto.getRandomValues(new Uint32Array(4))).join('.');
                  queueMicrotask(() => { const done = turn; turn = null; send([capability, token, done, '']); });
                }
                send([capability, token, turn, json]);
              };
            })(\(Self.binding).token, webkit.messageHandlers.\(sink.handler).postMessage.bind(webkit.messageHandlers.\(sink.handler)), '\(capability)')
            """)
        }
        routes[name] = route
        messages[destination] = sink
        sink.install(web)
        if preload, seeds[name] == nil {
            // The document's private lexical token is installed before any channel preload.
            let seed = WKUserScript(source: """
            const \(Self.binding) = Object.freeze({document, token: (() => {
              const bytes = crypto.getRandomValues(new Uint8Array(16));
              const hex = Array.from(bytes, n => n.toString(16).padStart(2, '0')).join('');
              return [hex.slice(0,8),hex.slice(8,12),hex.slice(12,16),hex.slice(16,20),hex.slice(20)].join('-');
            })()});
            """, injectionTime: .atDocumentStart, forMainFrameOnly: true, in: world)
            seeds[name] = seed
            installSeed(seed, on: web.configuration.userContentController)
        }
        // Concurrent calls can reserve the same channel. A refusal releases only this call's reservation.
        var finished = false
        let values = expressions.joined(separator: ", ")
        // The function can post and then await forever. Confirm it was handed out before guest code
        // can run, so aborting the call cannot revoke a message still on its way from the old document.
        let keys = reserved.map { "'\($0)'" }.joined(separator: ",")
        let source = preload ? values : "...(await webkit.messageHandlers.\(sink.handler).postMessage(['bind',\(keys)]) && [\(values)])"
        return ScriptArguments(source: source) { [weak self] admitted in
            guard !finished else { return }
            finished = true
            sink.finish(reserved, admitted: admitted)
            if sink.isEmpty, self?.messages[destination] === sink {
                self?.messages.removeValue(forKey: destination)
                sink.uninstall()
            }
        }
    }

    /// The sandbox's world, or the page's without one. The arm's own worlds
    /// (`exact.*`) are refused: they hold its message handlers.
    private func world(_ request: [String: Any], _ reply: ([String: Any]) -> Void) -> WKContentWorld? {
        guard let name = request["sandbox"] as? String else { return .page }
        guard !name.isEmpty, !name.hasPrefix("exact.") else {
            reply(["error": "SyntaxError", "message": "\(name.isEmpty ? "an empty name" : name) is not a sandbox: a name, not one beginning exact."])
            return nil
        }
        return WKContentWorld.world(name: name)
    }

    /// `evaluate` takes an expression, `callFunction` a function and its
    /// arguments; the value comes back as its JSON, at most 64 KB.
    private func script(_ op: String, _ request: [String: Any], _ arguments: ScriptArguments, _ web: WKWebView, _ reply: @escaping ([String: Any]) -> Void, retry: Bool = true) {
        guard let world = world(request, reply) else { return }
        let target = (request["target"] as? [String: Any])?["realm"] as? String
        if target == nil && retry && request["userActivation"] as? Bool == true { take() }
        nextId += 1
        let number = nextId
        if target != nil { identifying.insert(number) }
        if target != nil && request["userActivation"] as? Bool == true {
            actingScripts.insert(number)
            watch(web)
        }
        evaluations[number] = { [weak self] answer in
            self?.identifying.remove(number)
            self?.stopWatchingAct(number)
            reply(answer)
        }
        identify(request["sandbox"] as? String ?? "", world, web, number, targeted: target != nil) { [weak self, weak web] realm in
            guard let self, let web, self.evaluations[number] != nil else { return }
            guard let realm, target == nil || realm.id == target else {
                let error = target == nil ? ["error": "InvalidStateError", "message": "the document's realm could not be identified"] : Automations.missingRealm
                self.evaluations.removeValue(forKey: number)?(error)
                return
            }
            if request["userActivation"] as? Bool != true { self.identifying.remove(number) }
            self.execute(op, request, arguments, web, world, realm, number) { [weak self, weak web] in
                guard let self, let web, let answer = self.evaluations.removeValue(forKey: number) else { return }
                if target != nil { return answer(Automations.missingRealm) }
                self.identified.removeValue(forKey: request["sandbox"] as? String ?? "")
                if retry { self.script(op, request, arguments, web, answer, retry: false) }
                else { answer(["error": "AbortError", "message": "the document navigated away"]) }
            }
        }
    }

    private func execute(_ op: String, _ request: [String: Any], _ prepared: ScriptArguments, _ web: WKWebView, _ world: WKContentWorld, _ realm: Realm, _ number: Int, stale: @escaping () -> Void) {
        let awaited = request["awaitPromise"] as? Bool ?? true
        let value = op == "evaluate"
            ? "(\n\(request["expression"] as? String ?? "")\n)"
            : "(\n\(request["functionDeclaration"] as? String ?? "")\n)(\(prepared.source))"
        let settle = awaited ? "await __exactValue" : "(__exactValue && typeof __exactValue.then === 'function' ? {} : __exactValue)"
        // An exception is caught in the guest and answered as text, `!` then
        // its name and message: WebKit's error for one names it only under a
        // key outside its headers. Text, as no object is, is never taken for a
        // thenable, and reading the exception may throw too.
        // Encode the value as a JavaScript string literal so the guard introduces no parameter in the guest's scope.
        let token = String(decoding: try! JSONEncoder().encode(realm.token), as: UTF8.self)
        let valid = "typeof \(Self.binding) !== 'undefined' && \(Self.binding).document === document && \(Self.binding).token === \(token)"
        let body = "try {\nconst __exactValue = \(value);\nconst __exactText = JSON.stringify(\(settle));\nreturn typeof __exactText === 'string' ? __exactText : 'null';\n} catch (__exactError) {\ntry {\nreturn '!' + String(__exactError?.name ?? 'Error') + '\\n' + String(__exactError?.message ?? __exactError);\n} catch {\nreturn '!Error\\nan exception that cannot be read as text';\n}\n}"
        let arguments = request["arguments"] as? [Any] ?? []
        let acts = request["userActivation"] as? Bool == true
        if acts, (request["target"] as? [String: Any])?["realm"] is String {
            // A dialog cannot establish admission. Acknowledge the live world before taking control,
            // then check it again on execution in case the document changed during the round trip.
            // Parse the function during admission. Its arguments stay with the native call.
            let prepare = "if (!(\(valid))) return false; void (async () => {\n\(body)\n}); return true"
            web.callAsyncJavaScript(prepare, arguments: [:], in: nil, in: world) { [weak self, weak web] result in
                guard let self, let web, self.evaluations[number] != nil else { return }
                switch result {
                case .success(let admitted as Bool) where admitted:
                    self.identifying.remove(number)
                    self.scriptActed(number)
                    self.invoke("if (!(\(valid))) return true;\n" + body, arguments, web, world, realm, number, stale: stale)
                case .failure(let error):
                    self.stopWatchingAct(number)
                    self.evaluations.removeValue(forKey: number)?(Self.failure(error))
                default:
                    self.stopWatchingAct(number)
                    stale()
                }
            }
        } else {
            invoke("if (!(\(valid))) return false;\n" + body, arguments, web, world, realm, number, stale: stale)
        }
    }

    private func invoke(_ body: String, _ arguments: [Any], _ web: WKWebView, _ world: WKContentWorld,
                        _ realm: Realm, _ number: Int, stale: @escaping () -> Void) {
        web.callAsyncJavaScript(body, arguments: ["__exactArguments": arguments], in: nil, in: world) { [weak self, weak web] result in
            guard let self, self.evaluations[number] != nil else { return }
            if case .failure(let error) = result, Self.unparsed(error), let web {
                return self.parsed(body, web, world, number)
            }
            if case .success(let ended as Bool) = result, !ended { return stale() }
            guard let reply = self.evaluations.removeValue(forKey: number) else { return }
            switch result {
            case .success(let ended as Bool) where ended: reply(["error": "AbortError", "message": "the document navigated away"])
            case .success(let text as String) where text.hasPrefix("!"):
                let thrown = text.dropFirst().split(separator: "\n", maxSplits: 1, omittingEmptySubsequences: false)
                reply(Self.thrown(String(thrown[0]), thrown.count > 1 ? String(thrown[1]) : ""))
            case .success(let text as String):
                guard text.utf8.count <= Automations.limit else {
                    return reply(["error": "QuotaExceededError", "message": "the result is \(text.utf8.count) bytes of JSON; at most \(Automations.limit)"])
                }
                reply(["realm": realm.id, "origin": realm.origin, "result": (try? JSONSerialization.jsonObject(with: Data(text.utf8), options: .fragmentsAllowed)) ?? NSNull()])
            case .success: reply(["realm": realm.id, "origin": realm.origin, "result": NSNull()])
            case .failure(let error): reply(Self.failure(error))
            }
        }
    }

    /// WebKit fails a call whose promise it collected unsettled with the same
    /// error as one whose script does not parse (`callAsyncJavaScript`'s
    /// "result unreachable"). Such a promise can never settle, and BiDi's
    /// call then waits, so the call is left to end with its document or
    /// frame; only a script that does not parse is refused.
    private func parsed(_ body: String, _ web: WKWebView, _ world: WKContentWorld, _ number: Int) {
        web.evaluateJavaScript("typeof async function (__exactArguments) {\n\(body)\n}", in: nil, in: world) { [weak self] result in
            guard case .failure(let error) = result, Self.unparsed(error),
                  let reply = self?.evaluations.removeValue(forKey: number) else { return }
            reply(Self.failure(error))
        }
    }

    private static func unparsed(_ error: Error) -> Bool {
        let ns = error as NSError
        return ns.domain == WKError.errorDomain && ns.code == WKError.javaScriptExceptionOccurred.rawValue
    }

    /// The guest's exception by its own name (`TypeError`), else `Error`.
    static func thrown(_ name: String, _ message: String) -> [String: Any] {
        ["error": name.hasSuffix("Error") && !name.contains(" ") ? name : "Error", "message": message]
    }
    /// What WebKit refused before the guest ran: the script does not parse
    /// (every exception it throws is caught in it), or another failure, by
    /// WebKit's public description. Where the source fails to parse, WebKit
    /// names the cause only under a key outside its headers.
    static func failure(_ error: Error) -> [String: Any] {
        if unparsed(error) { return ["error": "SyntaxError", "message": "the script does not parse"] }
        return ["error": "Error", "message": (error as NSError).localizedDescription]
    }

    // MARK: Input

    /// BiDi's ticks: each source's nth action is the nth tick's, dispatched
    /// together; a tick lasts as long as its longest `pause`, `pointerMove`
    /// or `scroll`.
    private func perform(_ sources: [[String: Any]], _ web: WKWebView, _ reply: @escaping ([String: Any]) -> Void) {
        guard acting == nil else { return reply(["error": "InvalidStateError", "message": "performActions is already running in this frame"]) }
        #if os(macOS)
        guard web.window != nil else { return reply(["error": "InvalidStateError", "message": "the frame is in no window"]) }
        #endif
        if let out = outside(sources, web) { return reply(["error": "move target out of bounds", "message": out]) }
        take()
        nextId += 1
        let number = nextId
        var finished = false
        let end: ([String: Any]) -> Void = { answer in guard !finished else { return }; finished = true; reply(answer) }
        acting = (number, end)
        let ticks = sources.map { ($0["actions"] as? [[String: Any]])?.count ?? 0 }.max() ?? 0
        func tick(_ t: Int) {
            guard acting?.id == number, let web = arm?.built else { return }
            guard t < ticks else { return settle(web) { [weak self] in if self?.acting?.id == number { self?.acting = nil }; end([:]) } }
            var longest = 0.0
            #if os(macOS)
            // The page has the window's focus while the input lasts; `settle`
            // gives it back to what had it.
            let window = web.window
            if focus == nil { focus = .some(window?.firstResponder) }
            if window?.firstResponder !== web { window?.makeFirstResponder(web) }
            #endif
            var typed = false
            for source in sources {
                guard let actions = source["actions"] as? [[String: Any]], t < actions.count else { continue }
                let action = actions[t], duration = Self.bounded(action["duration"], 0...60_000) / 1000
                longest = max(longest, duration)
                typed = typed || source["type"] as? String == "key"
                dispatch(source["type"] as? String ?? "none", source["id"] as? String ?? "", action, duration, web)
            }
            let next = { if longest > 0 { DispatchQueue.main.asyncAfter(deadline: .now() + longest) { tick(t + 1) } } else { tick(t + 1) } }
            // A key waits for the page to have handled the one before: WebKit
            // turns a key into text after the page's answer, and drops one
            // that comes while the last is still out.
            if typed { pace(web, next) } else { DispatchQueue.main.async(execute: next) }
        }
        tick(0)
    }

    /// Why a pointer move or a wheel would land outside the frame's viewport
    /// (BiDi's "move target out of bounds"), checked before any input goes.
    private func outside(_ sources: [[String: Any]], _ web: WKWebView) -> String? {
        #if os(macOS)
        let scale = Double(web.pageZoom * web.magnification)
        #else
        let scale = Double(web.pageZoom)
        #endif
        let width = Double(web.bounds.width) / scale, height = Double(web.bounds.height) / scale
        let within = { (p: CGPoint) in p.x >= 0 && p.y >= 0 && p.x < width && p.y < height }
        for source in sources {
            let id = source["id"] as? String ?? ""
            var at = pointers[id] ?? .zero
            for action in source["actions"] as? [[String: Any]] ?? [] {
                let x = Self.bounded(action["x"], -1_000_000...1_000_000), y = Self.bounded(action["y"], -1_000_000...1_000_000)
                switch (source["type"] as? String, action["type"] as? String) {
                case ("pointer", "pointerMove"):
                    at = action["origin"] as? String == "pointer" ? CGPoint(x: at.x + x, y: at.y + y) : CGPoint(x: x, y: y)
                    if !within(at) { return "(\(at.x), \(at.y)) is outside the frame's \(Int(width)) × \(Int(height)) viewport" }
                case ("wheel", "scroll"):
                    if !within(CGPoint(x: x, y: y)) { return "a scroll at (\(x), \(y)) is outside the frame's \(Int(width)) × \(Int(height)) viewport" }
                default: break
                }
            }
        }
        return nil
    }

    /// A number from data code, finite and within `range` (0 when absent or not finite).
    static func bounded(_ value: Any?, _ range: ClosedRange<Double>) -> Double {
        let n = (value as? NSNumber)?.doubleValue ?? 0
        return n.isFinite ? min(max(n, range.lowerBound), range.upperBound) : 0
    }

    #if os(macOS)
    /// The events automation made, by type and time: WebKit sends a key the
    /// page left unhandled on through `NSApp.sendEvent`, where the take-back
    /// monitor drops it, so it neither counts as a person's nor reaches the
    /// app's menus (⌘W, ⌘Q).
    private var made: [(type: NSEvent.EventType, at: TimeInterval)] = []

    private func made(_ event: NSEvent) -> NSEvent {
        made.append((event.type, event.timestamp))
        if made.count > 64 { made.removeFirst(made.count - 64) }
        return event
    }

    private func ours(_ event: NSEvent) -> Bool { made.contains { $0.type == event.type && $0.at == event.timestamp } }

    /// What had the window's focus before input began: nil until then.
    private var focus: NSResponder??
    #endif

    #if os(macOS)
    /// The window's focus back to `previous`, or to the window when that is gone.
    static func restore(_ previous: NSResponder?, in window: NSWindow) {
        if let previous, previous.acceptsFirstResponder, (previous as? NSView)?.window === window || previous === window { window.makeFirstResponder(previous) } else { window.makeFirstResponder(nil) }
    }
    #endif

    /// `then`, once the page has answered a script sent after the input
    /// (WebKit answers in order), or after half a second, since a dialog the
    /// input opened holds the answer.
    private func pace(_ web: WKWebView, _ then: @escaping () -> Void) {
        var done = false
        let once = { guard !done else { return }; done = true; then() }
        web.evaluateJavaScript("0", in: nil, in: Automations.world) { _ in once() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5, execute: once)
    }

    /// The input is dispatched: once the page has answered (`pace`), the
    /// window's focus goes back to what had it (D9, "Focus").
    private func settle(_ web: WKWebView, _ then: @escaping () -> Void) {
        pace(web) { [weak self, weak web] in
            #if os(macOS)
            if let self, let web, let window = web.window, let previous = self.focus {
                self.focus = nil
                if previous !== web, window.firstResponder === web { Self.restore(previous, in: window) }
            }
            #endif
            then()
        }
    }

    private func dispatch(_ type: String, _ source: String, _ action: [String: Any], _ duration: TimeInterval, _ web: WKWebView) {
        let kind = action["type"] as? String ?? ""
        let number = { (key: String) in Self.bounded(action[key], -1_000_000...1_000_000) }
        let token = acting?.id
        switch (type, kind) {
        case ("pointer", "pointerMove"):
            let from = pointers[source] ?? .zero
            let to = action["origin"] as? String == "pointer" ? CGPoint(x: from.x + number("x"), y: from.y + number("y")) : CGPoint(x: number("x"), y: number("y"))
            let steps = duration > 0 ? min(60, max(1, Int(duration / 0.016))) : 1
            for step in 1...steps {
                let f = Double(step) / Double(steps), point = CGPoint(x: from.x + (to.x - from.x) * f, y: from.y + (to.y - from.y) * f)
                // A move scheduled within the tick is dropped once the call ended (a person took the frame, it unmounted).
                let move = { [weak self] in guard let self, self.acting?.id == token, self.arm?.invalidated == false, let web = self.arm?.built else { return }; self.pointers[source] = point; self.mouse("move", source, point, button: self.buttons[source]?.min() ?? -1, web) }
                if step == steps && duration == 0 { move() } else { DispatchQueue.main.asyncAfter(deadline: .now() + duration * f, execute: move) }
            }
        case ("pointer", "pointerDown"):
            let button = Int(Self.bounded(action["button"], 0...31))
            buttons[source, default: []].insert(button)
            mouse("down", source, pointers[source] ?? .zero, button: button, web)
        case ("pointer", "pointerUp"):
            let button = Int(Self.bounded(action["button"], 0...31))
            guard buttons[source]?.remove(button) != nil else { return }
            mouse("up", source, pointers[source] ?? .zero, button: button, web)
        case ("key", "keyDown"):
            let value = action["value"] as? String ?? ""
            keys[source, default: []].insert(value)
            key(value, down: true, web)
        case ("key", "keyUp"):
            let value = action["value"] as? String ?? ""
            guard keys[source]?.remove(value) != nil else { return }
            key(value, down: false, web)
        case ("wheel", "scroll"):
            wheel(CGPoint(x: number("x"), y: number("y")), dx: number("deltaX"), dy: number("deltaY"), web)
        default: break
        }
    }

    /// Every pressed key and button goes up, last pressed first, and the state is cleared.
    private func release(_ web: WKWebView) {
        for (source, pressed) in buttons { for button in pressed.sorted().reversed() { mouse("up", source, pointers[source] ?? .zero, button: button, web) } }
        buttons = [:]
        releaseKeys()
    }

    private func releaseKeys() {
        guard let web = arm?.built else { keys = [:]; return }
        let pressed = keys.values.flatMap { $0 }
        keys = [:]
        for value in pressed.sorted().reversed() { key(value, down: false, web) }
    }

    /// The modifiers held now, by WebDriver's key values.
    private var held: Set<String> { Set(keys.values.flatMap { $0 }) }

    private func clickCount(_ button: Int, _ point: CGPoint) -> Int {
        let now = ProcessInfo.processInfo.systemUptime
        let count: Int
        if let last = lastClick, last.button == button, now - last.at < 0.5, abs(last.point.x - point.x) < 4, abs(last.point.y - point.y) < 4 { count = last.count + 1 } else { count = 1 }
        lastClick = (button, now, count, point)
        return count
    }

    #if os(macOS)
    /// macOS: `NSEvent`s to the web view's own responder methods, in-process,
    /// as a person's arrive there; never posted to the OS (LLP 1020 §3.3).
    private func mouse(_ phase: String, _ source: String, _ css: CGPoint, button: Int, _ web: WKWebView) {
        guard let window = web.window else { return }
        let scale = Double(web.pageZoom * web.magnification)
        let local = NSPoint(x: css.x * scale, y: web.isFlipped ? css.y * scale : web.bounds.height - css.y * scale)
        let type: NSEvent.EventType
        switch (phase, button) {
        case ("move", -1): type = .mouseMoved
        case ("move", 0): type = .leftMouseDragged
        case ("move", 2): type = .rightMouseDragged
        case ("move", _): type = .otherMouseDragged
        case ("down", 0): type = .leftMouseDown
        case ("down", 2): type = .rightMouseDown
        case ("down", _): type = .otherMouseDown
        case (_, 0): type = .leftMouseUp
        case (_, 2): type = .rightMouseUp
        default: type = .otherMouseUp
        }
        let count = phase == "down" ? clickCount(button, css) : phase == "up" ? lastClick?.count ?? 1 : 0
        guard let event = NSEvent.mouseEvent(with: type, location: web.convert(local, to: nil), modifierFlags: Self.flags(held), timestamp: ProcessInfo.processInfo.systemUptime,
                                             windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: count, pressure: phase == "down" ? 1 : 0) else { return }
        _ = made(event)
        switch type {
        case .mouseMoved: web.mouseMoved(with: event)
        case .leftMouseDragged: web.mouseDragged(with: event)
        case .rightMouseDragged: web.rightMouseDragged(with: event)
        case .otherMouseDragged: web.otherMouseDragged(with: event)
        case .leftMouseDown: web.mouseDown(with: event)
        case .rightMouseDown: web.rightMouseDown(with: event)
        case .otherMouseDown: web.otherMouseDown(with: event)
        case .leftMouseUp: web.mouseUp(with: event)
        case .rightMouseUp: web.rightMouseUp(with: event)
        default: web.otherMouseUp(with: event)
        }
    }

    private func key(_ value: String, down: Bool, _ web: WKWebView) {
        guard let window = web.window else { return }
        let named = Keys.special[value]
        if named?.flag != nil {
            guard let event = NSEvent.keyEvent(with: .flagsChanged, location: .zero, modifierFlags: Self.flags(held), timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                                               context: nil, characters: "", charactersIgnoringModifiers: "", isARepeat: false, keyCode: named!.code) else { return }
            return web.flagsChanged(with: made(event))
        }
        let flags = Self.flags(held)
        let plain = named?.characters ?? value.lowercased()
        let characters = named?.characters ?? (flags.contains(.command) || flags.contains(.control) ? plain : value)
        guard let event = NSEvent.keyEvent(with: down ? .keyDown : .keyUp, location: .zero, modifierFlags: flags, timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                                           context: nil, characters: characters, charactersIgnoringModifiers: plain, isARepeat: false, keyCode: named?.code ?? Keys.code(value)) else { return }
        if down { web.keyDown(with: made(event)) } else { web.keyUp(with: made(event)) }
    }

    private func wheel(_ css: CGPoint, dx: Double, dy: Double, _ web: WKWebView) {
        guard let window = web.window, let primary = NSScreen.screens.first else { return }
        let scale = Double(web.pageZoom * web.magnification)
        let local = NSPoint(x: css.x * scale, y: web.isFlipped ? css.y * scale : web.bounds.height - css.y * scale)
        let point = web.convert(local, to: nil), onScreen = window.convertPoint(toScreen: point)
        // A scroll `NSEvent` is made only from a `CGEvent`, whose location is
        // global (y down from the primary screen's top). Whether AppKit then
        // ties it to the window varies, so the location is the one that gives
        // the event the window point WebKit reads.
        func event(at location: NSPoint) -> NSEvent? {
            guard let cg = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2, wheel1: Int32(-dy.rounded()), wheel2: Int32(-dx.rounded()), wheel3: 0) else { return nil }
            cg.location = CGPoint(x: location.x, y: primary.frame.maxY - location.y)
            cg.flags = CGEventFlags(rawValue: UInt64(Self.flags(held).rawValue))
            cg.setIntegerValueField(.mouseEventWindowUnderMousePointer, value: Int64(window.windowNumber))
            cg.setIntegerValueField(.mouseEventWindowUnderMousePointerThatCanHandleThisEvent, value: Int64(window.windowNumber))
            return NSEvent(cgEvent: cg)
        }
        let near = { (e: NSEvent) in abs(e.locationInWindow.x - point.x) < 1 && abs(e.locationInWindow.y - point.y) < 1 }
        guard let scroll = [event(at: onScreen), event(at: point)].compactMap({ $0 }).first(where: near) ?? event(at: onScreen) else { return }
        web.scrollWheel(with: made(scroll))
    }

    static func flags(_ held: Set<String>) -> NSEvent.ModifierFlags {
        held.reduce(into: NSEvent.ModifierFlags()) { $0.formUnion(Keys.special[$1]?.flag ?? []) }
    }

    /// A person's click, key or wheel in the frame, seen before AppKit
    /// dispatches it: the automation's own events go straight to the web
    /// view's responder methods, so a local monitor sees only a person's.
    private func watch(_ web: WKWebView) {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown, .keyDown, .scrollWheel]) { [weak self, weak web] event in
            guard let self else { return event }
            if self.ours(event) { return nil }
            // A wheel's momentum continues a scroll already begun, not a new one.
            guard let web, let window = web.window, event.window === window, !web.isHiddenOrHasHiddenAncestor,
                  event.type != .scrollWheel || event.momentumPhase.isEmpty else { return event }
            let inside = event.type == .keyDown
                ? (window.firstResponder as? NSView).map { $0 === web || $0.isDescendant(of: web) } ?? false
                : (window.contentView.flatMap { $0.hitTest($0.superview?.convert(event.locationInWindow, from: nil) ?? event.locationInWindow) })
                    .map { $0 === web || $0.isDescendant(of: web) } ?? false
            // A click that brings a held dialog up is the dialog's: the page never sees it.
            return inside && self.personTook() ? nil : event
        }
    }

    private func unwatch() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }
    #else
    /// iOS: no public way to make a `UITouch` or a key press, so input is
    /// DOM events dispatched by script in the arm's own world, which the page
    /// sees as `isTrusted: false` (declared).
    private func mouse(_ phase: String, _ source: String, _ css: CGPoint, button: Int, _ web: WKWebView) {
        let count = phase == "down" ? clickCount(button, css) : phase == "up" ? lastClick?.count ?? 1 : 0
        input(["kind": "pointer", "phase": phase, "x": css.x, "y": css.y, "button": button, "count": count, "pointer": source], web)
    }

    private func key(_ value: String, down: Bool, _ web: WKWebView) {
        let named = Keys.special[value]
        input(["kind": "key", "phase": down ? "down" : "up", "key": named?.key ?? value, "code": named?.dom ?? Keys.dom(value), "modifiers": Array(held)], web)
    }

    private func wheel(_ css: CGPoint, dx: Double, dy: Double, _ web: WKWebView) {
        input(["kind": "wheel", "x": css.x, "y": css.y, "dx": dx, "dy": dy], web)
    }

    private func input(_ event: [String: Any], _ web: WKWebView) {
        web.callAsyncJavaScript(Keys.script, arguments: ["e": event], in: nil, in: Automations.world) { _ in }
    }

    /// A person's touch in the frame: the automation never makes one.
    private func watch(_ web: WKWebView) {
        guard watcher == nil else { return }
        let watcher = PersonTouches { [weak self] in self?.personTook() }
        web.addGestureRecognizer(watcher)
        self.watcher = watcher
    }

    private func unwatch() {
        watcher?.view?.removeGestureRecognizer(watcher!)
        watcher = nil
    }
    #endif
}

#if os(iOS)
/// Hears a touch begin and recognizes nothing, so the page's own touches pass.
final class PersonTouches: UIGestureRecognizer, UIGestureRecognizerDelegate {
    let touched: () -> Void
    init(_ touched: @escaping () -> Void) {
        self.touched = touched
        super.init(target: nil, action: nil)
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
        delegate = self
    }
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        touched()
        state = .failed
    }
    func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }
}
#endif

/// WebDriver's key values (W3C WebDriver §17.4.2, the normalized key value
/// table): the private-use code points of the named keys, with the DOM's
/// `key` and `code` and the Mac's virtual key code and characters.
enum Keys {
    struct Named { let key: String; let dom: String; let code: UInt16; let characters: String?; var flag: NSEventFlags? = nil }
    #if os(macOS)
    typealias NSEventFlags = NSEvent.ModifierFlags
    #else
    struct NSEventFlags: OptionSet { let rawValue: UInt; static let shift = NSEventFlags(rawValue: 1), control = NSEventFlags(rawValue: 2), option = NSEventFlags(rawValue: 4), command = NSEventFlags(rawValue: 8) }
    #endif
    static func f(_ n: Int) -> String { String(UnicodeScalar(0xF703 + n)!) }
    static let special: [String: Named] = [
        "\u{E003}": Named(key: "Backspace", dom: "Backspace", code: 51, characters: "\u{7F}"),
        "\u{E004}": Named(key: "Tab", dom: "Tab", code: 48, characters: "\t"),
        "\u{E006}": Named(key: "Enter", dom: "Enter", code: 36, characters: "\r"),
        "\u{E007}": Named(key: "Enter", dom: "Enter", code: 36, characters: "\r"),
        "\u{E008}": Named(key: "Shift", dom: "ShiftLeft", code: 56, characters: nil, flag: .shift),
        "\u{E009}": Named(key: "Control", dom: "ControlLeft", code: 59, characters: nil, flag: .control),
        "\u{E00A}": Named(key: "Alt", dom: "AltLeft", code: 58, characters: nil, flag: .option),
        "\u{E00C}": Named(key: "Escape", dom: "Escape", code: 53, characters: "\u{1B}"),
        "\u{E00D}": Named(key: " ", dom: "Space", code: 49, characters: " "),
        "\u{E00E}": Named(key: "PageUp", dom: "PageUp", code: 116, characters: "\u{F72C}"),
        "\u{E00F}": Named(key: "PageDown", dom: "PageDown", code: 121, characters: "\u{F72D}"),
        "\u{E010}": Named(key: "End", dom: "End", code: 119, characters: "\u{F72B}"),
        "\u{E011}": Named(key: "Home", dom: "Home", code: 115, characters: "\u{F729}"),
        "\u{E012}": Named(key: "ArrowLeft", dom: "ArrowLeft", code: 123, characters: "\u{F702}"),
        "\u{E013}": Named(key: "ArrowUp", dom: "ArrowUp", code: 126, characters: "\u{F700}"),
        "\u{E014}": Named(key: "ArrowRight", dom: "ArrowRight", code: 124, characters: "\u{F703}"),
        "\u{E015}": Named(key: "ArrowDown", dom: "ArrowDown", code: 125, characters: "\u{F701}"),
        "\u{E017}": Named(key: "Delete", dom: "Delete", code: 117, characters: "\u{F728}"),
        "\u{E031}": Named(key: "F1", dom: "F1", code: 122, characters: f(1)),
        "\u{E032}": Named(key: "F2", dom: "F2", code: 120, characters: f(2)),
        "\u{E033}": Named(key: "F3", dom: "F3", code: 99, characters: f(3)),
        "\u{E034}": Named(key: "F4", dom: "F4", code: 118, characters: f(4)),
        "\u{E035}": Named(key: "F5", dom: "F5", code: 96, characters: f(5)),
        "\u{E036}": Named(key: "F6", dom: "F6", code: 97, characters: f(6)),
        "\u{E037}": Named(key: "F7", dom: "F7", code: 98, characters: f(7)),
        "\u{E038}": Named(key: "F8", dom: "F8", code: 100, characters: f(8)),
        "\u{E039}": Named(key: "F9", dom: "F9", code: 101, characters: f(9)),
        "\u{E03A}": Named(key: "F10", dom: "F10", code: 109, characters: f(10)),
        "\u{E03B}": Named(key: "F11", dom: "F11", code: 103, characters: f(11)),
        "\u{E03C}": Named(key: "F12", dom: "F12", code: 111, characters: f(12)),
        "\u{E03D}": Named(key: "Meta", dom: "MetaLeft", code: 55, characters: nil, flag: .command),
        "\u{E050}": Named(key: "Shift", dom: "ShiftRight", code: 60, characters: nil, flag: .shift),
        "\u{E051}": Named(key: "Control", dom: "ControlRight", code: 62, characters: nil, flag: .control),
        "\u{E052}": Named(key: "Alt", dom: "AltRight", code: 61, characters: nil, flag: .option),
        "\u{E053}": Named(key: "Meta", dom: "MetaRight", code: 54, characters: nil, flag: .command),
    ]

    /// A printable key's US ANSI virtual key code, shifted symbols on their base key; 0 for another character.
    static func code(_ value: String) -> UInt16 {
        guard let c = value.lowercased().first else { return 0 }
        let keys = "asdfhgzxcv\u{0}bqweryt123465=97-80]ou[ip\u{0}lj'k;\\,/nm.\u{0}\u{0}`"
        let shifted = ["!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6", "&": "7", "*": "8", "(": "9", ")": "0", "_": "-", "+": "=",
                       "{": "[", "}": "]", "|": "\\", ":": ";", "\"": "'", "<": ",", ">": ".", "?": "/", "~": "`"]
        let base = shifted[String(c)]?.first ?? c
        if base == " " { return 49 }
        return keys.firstIndex(of: base).map { UInt16(keys.distance(from: keys.startIndex, to: $0)) } ?? 0
    }

    /// A printable key's DOM `code` on a US layout ("KeyA", "Digit1"), "" for another character.
    static func dom(_ value: String) -> String {
        guard let c = value.uppercased().first, value.count == 1 else { return "" }
        if c.isLetter, c.isASCII { return "Key\(c)" }
        if c.isNumber, c.isASCII { return "Digit\(c)" }
        return ""
    }

    /// iOS's script input: one event as the page's listeners get it, and what
    /// the browser does after an event no listener cancelled (a click's focus,
    /// a printable key's text, a wheel's scroll).
    static let script = """
    const t = (x, y) => document.elementFromPoint(x, y) || document.body;
    const s = globalThis.__exactAutomation ??= { down: null };
    const opts = (x, y) => ({ bubbles: true, cancelable: true, composed: true, clientX: x, clientY: y, view: window });
    if (e.kind === 'pointer') {
      const target = t(e.x, e.y), base = { ...opts(e.x, e.y), button: Math.max(0, e.button), buttons: e.phase === 'up' || e.button < 0 ? 0 : 1, pointerId: 1, pointerType: 'mouse', isPrimary: true, detail: e.count };
      const name = { move: 'move', down: 'down', up: 'up' }[e.phase];
      target.dispatchEvent(new PointerEvent('pointer' + name, base));
      target.dispatchEvent(new MouseEvent('mouse' + name, base));
      if (e.phase === 'down') { s.down = target; if (target.focus && target !== document.body) target.focus(); }
      if (e.phase === 'up' && s.down === target) { target.dispatchEvent(new MouseEvent(e.button === 2 ? 'contextmenu' : 'click', base)); if (e.count === 2) target.dispatchEvent(new MouseEvent('dblclick', base)); }
      if (e.phase === 'up') s.down = null;
      return;
    }
    if (e.kind === 'wheel') {
      const target = t(e.x, e.y);
      if (!target.dispatchEvent(new WheelEvent('wheel', { ...opts(e.x, e.y), deltaX: e.dx, deltaY: e.dy, deltaMode: 0 }))) return;
      for (let n = target; n; n = n.parentElement) { const st = getComputedStyle(n); if (/(auto|scroll)/.test(st.overflowY + st.overflowX) && (n.scrollHeight > n.clientHeight || n.scrollWidth > n.clientWidth)) { n.scrollBy(e.dx, e.dy); return; } }
      scrollBy(e.dx, e.dy);
      return;
    }
    const target = document.activeElement || document.body, m = new Set(e.modifiers);
    const init = { bubbles: true, cancelable: true, composed: true, key: e.key, code: e.code, shiftKey: m.has('\\uE008') || m.has('\\uE050'), ctrlKey: m.has('\\uE009') || m.has('\\uE051'), altKey: m.has('\\uE00A') || m.has('\\uE052'), metaKey: m.has('\\uE03D') || m.has('\\uE053') };
    if (e.phase === 'up') { target.dispatchEvent(new KeyboardEvent('keyup', init)); return; }
    if (!target.dispatchEvent(new KeyboardEvent('keydown', init)) || init.ctrlKey || init.metaKey) return;
    if (e.key.length === 1) { target.dispatchEvent(new KeyboardEvent('keypress', init)); document.execCommand('insertText', false, e.key); }
    else if (e.key === 'Backspace') document.execCommand('delete');
    else if (e.key === 'Delete') document.execCommand('forwardDelete');
    else if (e.key === 'Enter') { if (target.form && target.localName === 'input') target.form.requestSubmit(); else document.execCommand('insertParagraph'); }
    """
}
