import Network
import WebKit
import XCTest
#if os(macOS)
import AppKit
#else
import UIKit
#endif

#if os(macOS) || os(iOS)
/// A partitioned frame driven through the web arm's `exact_web_automation`
/// (LLP 1020.002 stage 4, D9), as `@exact/browser-automation`'s package calls
/// it once the runner admitted the call: scripts in their worlds, preload
/// scripts, input and whether the page trusts it, prompts, downloads, a
/// person taking the frame back, and calls ending with their document or
/// frame, against pages a loopback server in the test serves.
final class IframeAutomationTests: XCTestCase {
    private typealias EventFn = @convention(c) (UnsafeMutableRawPointer?, UInt32, UInt32, UnsafePointer<UInt8>?, UInt32) -> Void
    private typealias ReplyFn = @convention(c) (UnsafeMutableRawPointer?, UInt32, UInt32, UInt32, UnsafePointer<UInt8>?, UInt32) -> Void
    private typealias ConfiguringFn = @convention(c) (UnsafeMutableRawPointer?, UnsafeMutableRawPointer?) -> Void
    private typealias SetConfiguringFn = @convention(c) (UnsafeMutableRawPointer?, ConfiguringFn?) -> Void
    private typealias CreateFn = @convention(c) (UInt32, UnsafeMutableRawPointer?, EventFn?, ReplyFn?) -> UnsafeMutableRawPointer?
    private typealias ViewFn = @convention(c) (UnsafeMutableRawPointer?) -> UnsafeMutableRawPointer?
    private typealias SetFn = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32, UInt32) -> Void
    private typealias PostFn = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32) -> Void
    private typealias DestroyFn = @convention(c) (UnsafeMutableRawPointer?) -> Void
    private typealias DoneFn = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32) -> Void
    private typealias Message = @convention(block) (NSDictionary) -> Void
    private typealias AutomateFn = @convention(c) (UnsafePointer<UInt8>?, UInt32, UnsafeMutableRawPointer?, DoneFn?) -> Void

    private final class Seen { var records: [(event: String, record: [String: Any])] = [] }
    private final class Answer { var value: [String: Any]? }

    private struct Arm {
        let create: CreateFn, setConfiguring: SetConfiguringFn, view: ViewFn, setSrc: SetFn, configure: PostFn, destroy: DestroyFn, automate: AutomateFn
    }

    private func arm() throws -> Arm {
        guard let path = ProcessInfo.processInfo.environment["EXACT_TEST_WEB_MODULE"] else {
            throw missing("module", "no WebKit arm: name a built one with EXACT_TEST_WEB_MODULE")
        }
        let library = try XCTUnwrap(dlopen(path, RTLD_NOW | RTLD_LOCAL), String(cString: dlerror()))
        func symbol<T>(_ name: String, _: T.Type) throws -> T { unsafeBitCast(try XCTUnwrap(dlsym(library, name), "missing \(name)"), to: T.self) }
        return Arm(create: try symbol("exact_web_create", CreateFn.self), setConfiguring: try symbol("exact_web_set_configuring", SetConfiguringFn.self), view: try symbol("exact_web_platform_view", ViewFn.self),
                   setSrc: try symbol("exact_web_set_src", SetFn.self), configure: try symbol("exact_web_configure", PostFn.self),
                   destroy: try symbol("exact_web_destroy", DestroyFn.self), automate: try symbol("exact_web_automation", AutomateFn.self))
    }

    private static let event: EventFn = { context, _, kind, bytes, length in
        let seen = Unmanaged<Seen>.fromOpaque(context!).takeUnretainedValue()
        let data = Data(bytes: bytes ?? UnsafePointer(bitPattern: 1)!, count: Int(length))
        guard kind == 4 || kind == 5, let value = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let name = value["event"] as? String, let record = value["record"] as? [String: Any] else { return }
        seen.records.append((name, record))
    }
    private static let reply: ReplyFn = { _, _, _, _, _, _ in }
    private static let done: DoneFn = { context, bytes, length in
        let callbacks = Unmanaged<NSArray>.fromOpaque(context!).takeRetainedValue()
        let answer = callbacks.object(at: 0) as! Answer
        answer.value = (try? JSONSerialization.jsonObject(with: Data(bytes: bytes!, count: Int(length)))) as? [String: Any] ?? [:]
    }

    private func json(_ value: [String: Any], _ fn: PostFn, _ handle: UnsafeMutableRawPointer) {
        let data = try! JSONSerialization.data(withJSONObject: value)
        data.withUnsafeBytes { fn(handle, $0.bindMemory(to: UInt8.self).baseAddress, UInt32(data.count)) }
    }

    private func spin(until done: () -> Bool, _ seconds: TimeInterval = 15) {
        let deadline = Date(timeIntervalSinceNow: seconds)
        while !done() && Date() < deadline { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.02)) }
    }

    private func pause(_ seconds: TimeInterval) { spin(until: { false }, seconds) }

    private func say(_ text: String) {
        print(text)
        XCTContext.runActivity(named: text) { _ in }
    }

    private static let owner = "/tmp/iframe-automation-owner"

    /// A call as the package makes it; its answer once the arm gives one.
    private func call(_ arm: Arm, _ op: String, _ options: [String: Any] = [:], id: String = "tab", owner: String = owner, message: Message? = nil) -> Answer {
        let answer = Answer()
        var request = options
        request["op"] = op; request["owner"] = owner
        if request["target"] == nil { request["id"] = id }
        let callbacks: NSArray = message.map { [answer, $0 as AnyObject] as NSArray } ?? [answer]
        var data = try! JSONSerialization.data(withJSONObject: request)
        if message != nil { data.insert(2, at: 0) }
        let context = Unmanaged.passRetained(callbacks).toOpaque()
        data.withUnsafeBytes { arm.automate($0.bindMemory(to: UInt8.self).baseAddress, UInt32(data.count), context, Self.done) }
        return answer
    }

    private func answered(_ answer: Answer, _ seconds: TimeInterval = 15) -> [String: Any] {
        spin(until: { answer.value != nil }, seconds)
        return answer.value ?? ["error": "timed out"]
    }

    private func run(_ arm: Arm, _ op: String, _ options: [String: Any] = [:], id: String = "tab", owner: String = owner, message: Message? = nil) -> [String: Any] {
        answered(call(arm, op, options, id: id, owner: owner, message: message))
    }

    private func value(_ arm: Arm, _ expression: String, sandbox: String? = nil) -> Any? {
        var options: [String: Any] = ["expression": expression]
        if let sandbox { options["sandbox"] = sandbox }
        return run(arm, "evaluate", options)["result"]
    }

    #if os(macOS)
    private var windows: [NSWindow] = []
    override func tearDown() { windows.forEach { $0.close() }; windows = [] }
    #else
    private var windows: [UIWindow] = []
    override func tearDown() { windows.forEach { $0.isHidden = true }; windows = [] }
    #endif

    private func show(_ view: AnyObject) {
        #if os(macOS)
        let view = view as! NSView
        view.frame = CGRect(x: 0, y: 0, width: 400, height: 300)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView!.addSubview(view)
        window.makeKeyAndOrderFront(nil)
        windows.append(window)
        #else
        let view = view as! UIView
        view.frame = CGRect(x: 0, y: 0, width: 400, height: 300)
        let window = hostedWindowScene().map { UIWindow(windowScene: $0) } ?? UIWindow()
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 300)
        window.addSubview(view)
        window.isHidden = false
        windows.append(window)
        #endif
    }

    private func mount(_ arm: Arm, _ seen: Seen, partition: String? = "memory-automation", path: String? = "/input", id: String = "tab", configuring: ConfiguringFn? = nil) -> (UnsafeMutableRawPointer, WKWebView) {
        let handle = arm.create(11, Unmanaged.passUnretained(seen).toOpaque(), Self.event, Self.reply)!
        if let configuring { arm.setConfiguring(handle, configuring) }
        var configuration: [String: Any] = ["element": id, "owner": Self.owner]
        if let partition { configuration["partition"] = partition }
        json(configuration, arm.configure, handle)
        let web = Unmanaged<AnyObject>.fromOpaque(arm.view(handle)!).takeUnretainedValue() as! WKWebView
        show(web)
        guard let path else { return (handle, web) }
        Array("\(baseURL)\(path)".utf8).withUnsafeBufferPointer { arm.setSrc(handle, $0.baseAddress, UInt32($0.count), 1) }
        if partition != nil { spin { seen.records.contains { $0.event == "navigatesuccess" } } } else { pause(1) }
        pause(0.2)
        return (handle, web)
    }

    /// Where pictures and downloads go, removed after the class.
    private static let folder = FileManager.default.temporaryDirectory.appendingPathComponent("iframe-automation-\(ProcessInfo.processInfo.processIdentifier)", isDirectory: true)
    override class func tearDown() { try? FileManager.default.removeItem(at: folder); super.tearDown() }

    nonisolated(unsafe) private static var server: AutomationPages?
    private var baseURL: String { "http://127.0.0.1:\(Self.server!.port)" }
    override func setUpWithError() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        if Self.server == nil { Self.server = try AutomationPages() }
    }

    private func controller(_ seen: Seen) -> String? { seen.records.last { $0.event == "currententrychange" }?.record["controller"] as? String }

    /// `evaluate` and `callFunction` in the page's world or a sandbox's, a
    /// value as its JSON, an exception by its own name; a read leaves the
    /// frame the person's, `userActivation` takes it.
    func testScriptsRunInTheirWorlds() throws {
        let arm = try arm(), seen = Seen()
        let (handle, _) = mount(arm, seen)
        defer { arm.destroy(handle) }
        XCTAssertEqual(value(arm, "document.title") as? String, "Input")
        XCTAssertNil(run(arm, "evaluate", ["expression": "window.mark = 1"])["error"])
        XCTAssertEqual(value(arm, "typeof window.mark") as? String, "number")
        XCTAssertEqual(value(arm, "typeof window.mark", sandbox: "t3") as? String, "undefined", "a sandbox is an isolated world")
        XCTAssertEqual(value(arm, "document.title", sandbox: "t3") as? String, "Input", "with the page's DOM")
        XCTAssertEqual(run(arm, "callFunction", ["functionDeclaration": "(a, b) => ({ sum: a + b.n })", "arguments": [2, ["n": 3]]])["result"] as? [String: Int], ["sum": 5])
        XCTAssertEqual(value(arm, "new Promise(r => setTimeout(() => r(7), 50))") as? Int, 7)
        XCTAssertEqual(run(arm, "evaluate", ["expression": "new Promise(() => {})", "awaitPromise": false])["result"] as? [String: Int], [:])
        XCTAssert(value(arm, "undefined") is NSNull)
        let thrown = run(arm, "evaluate", ["expression": "nope()"])
        XCTAssertEqual(thrown["error"] as? String, "ReferenceError", "\(thrown)")
        XCTAssertEqual(thrown["message"] as? String, "Can't find variable: nope")
        let late = run(arm, "callFunction", ["functionDeclaration": "() => { throw new TypeError('late') }"])
        XCTAssertEqual([late["error"] as? String, late["message"] as? String], ["TypeError", "late"])
        let plain = run(arm, "evaluate", ["expression": "Promise.reject('plain')"])
        XCTAssertEqual([plain["error"] as? String, plain["message"] as? String], ["Error", "plain"])
        XCTAssertEqual(run(arm, "evaluate", ["expression": "1 +"])["error"] as? String, "SyntaxError", "a script that does not parse")
        XCTAssertEqual(run(arm, "evaluate", ["expression": "'x'.repeat(70000)"])["error"] as? String, "QuotaExceededError")
        XCTAssertEqual(run(arm, "evaluate", ["expression": "1", "sandbox": "exact.browser"])["error"] as? String, "SyntaxError", "the arm's own worlds are refused")
        let activation = value(arm, "String(navigator.userActivation?.isActive)") as? String
        say("iframe automation: navigator.userActivation.isActive in an evaluation = \(activation ?? "nil")")
        XCTAssertNotEqual(controller(seen), "agent", "a read leaves the frame the person's")
        XCTAssertNil(run(arm, "evaluate", ["expression": "1", "userActivation": true])["error"])
        spin { self.controller(seen) == "agent" }
        XCTAssertEqual(controller(seen), "agent", "userActivation takes the frame")
        XCTAssertEqual(run(arm, "evaluate", ["expression": "1"], owner: "/tmp/another-app")["error"] as? String, "NotFoundError", "another app's frames are not found")
        XCTAssertEqual(run(arm, "evaluate", ["expression": "1"], id: "other")["error"] as? String, "NotFoundError")
    }

    func testApprovalCanOnlyReachTheDocumentAndWorldThatAsked() throws {
        let arm = try arm(), seen = Seen()
        let (handle, web) = mount(arm, seen)
        defer { arm.destroy(handle) }
        let request = ["context": "tab", "sandbox": "wallet"]
        func ask() -> [String: Any] {
            run(arm, "evaluate", ["target": request, "expression": "'approve?'"])
        }
        let first = ask()
        let old = try XCTUnwrap(first["realm"] as? String)
        XCTAssertEqual(first["origin"] as? String, baseURL)
        let page = try XCTUnwrap(run(arm, "evaluate", ["expression": "true"])["realm"] as? String)
        XCTAssertNotEqual(page, old)
        XCTAssertEqual(ask()["realm"] as? String, old)
        let answer = "value => window.decision = value"
        func decide(_ realm: String, _ value: String) -> [String: Any] {
            run(arm, "callFunction", ["target": ["realm": realm], "functionDeclaration": answer, "arguments": [value]])
        }
        XCTAssertEqual(decide(old, "live")["result"] as? String, "live")
        XCTAssertEqual(value(arm, "typeof decision") as? String, "undefined")
        let act = call(arm, "evaluate", ["target": ["realm": old], "userActivation": true, "expression": "prompt('Approve?')"])
        spin { seen.records.contains { $0.event == "userpromptopened" } }
        XCTAssertNil(run(arm, "handleUserPrompt", ["accept": true, "userText": "yes"])["error"])
        XCTAssertEqual(answered(act)["result"] as? String, "yes")
        XCTAssertEqual(controller(seen), "agent")
        let held = call(arm, "evaluate", ["target": ["realm": old], "expression": "new Promise(r => { window.waiting = r })"])
        spin { self.value(arm, "typeof window.waiting", sandbox: "wallet") as? String == "function" }
        let before = seen.records.count
        web.reload()
        spin { seen.records.dropFirst(before).contains { $0.event == "navigatesuccess" } }
        XCTAssertEqual(answered(held)["error"] as? String, "AbortError")
        XCTAssertEqual(decide(old, "late")["error"] as? String, "no such frame")
        XCTAssertEqual(decide(page, "late")["error"] as? String, "no such frame")
        XCTAssertEqual(value(arm, "typeof decision", sandbox: "wallet") as? String, "undefined")
        let fresh = ask()
        XCTAssertNotEqual(fresh["realm"] as? String, old)
        XCTAssertEqual(fresh["origin"] as? String, baseURL)
        XCTAssertEqual(run(arm, "callFunction", ["target": request, "functionDeclaration": answer, "arguments": ["frame"]])["result"] as? String, "frame")
    }

    /// The document can change before the native commit reaches the arm. Keep its host lookup live
    /// across the reload so only the check in the target world can stop the old call.
    func testRealmCheckRunsInTheTargetDocument() throws {
        let arm = try arm(), seen = Seen()
        let (handle, web) = mount(arm, seen)
        defer { arm.destroy(handle) }
        let realm = try XCTUnwrap(run(arm, "evaluate", ["expression": "window.decision = 'old'"])["realm"] as? String)
        let delegate = web.navigationDelegate
        web.navigationDelegate = nil
        defer { web.navigationDelegate = delegate }
        _ = try XCTUnwrap(web.reload())
        spin { !web.isLoading }
        XCTAssertFalse(web.isLoading)
        let late = run(arm, "evaluate", ["target": ["realm": realm], "userActivation": true, "expression": "window.decision = 'late'"])
        XCTAssertEqual(late["error"] as? String, "no such frame")
        XCTAssertNotEqual(controller(seen), "agent")
        XCTAssertEqual(value(arm, "typeof window.decision") as? String, "undefined")
        let current = run(arm, "evaluate", ["target": ["context": "tab"], "expression": "window.decision = 'frame'"])
        XCTAssertEqual(current["result"] as? String, "frame")
        XCTAssertNotEqual(current["realm"] as? String, realm)
    }

    nonisolated(unsafe) private static var sharedController: WKUserContentController?

    func testTwoFramesCanShareAContentController() throws {
        let arm = try arm(), first = Seen(), second = Seen()
        let shared = WKUserContentController()
        Self.sharedController = shared
        defer { Self.sharedController = nil }
        let hatch: ConfiguringFn = { _, config in
            Unmanaged<WKWebViewConfiguration>.fromOpaque(config!).takeUnretainedValue().userContentController = IframeAutomationTests.sharedController!
        }
        let (a, _) = mount(arm, first, configuring: hatch)
        defer { arm.destroy(a) }
        let scripts = shared.userScripts.count
        let (b, _) = mount(arm, second, id: "other", configuring: hatch)
        defer { arm.destroy(b) }
        XCTAssertEqual(shared.userScripts.count, scripts)
        let expression = "(window.__exactToken = 'guest', __exactToken)"
        let left = run(arm, "evaluate", ["expression": expression])
        let right = run(arm, "evaluate", ["expression": expression], id: "other")
        XCTAssertEqual(left["result"] as? String, "guest")
        XCTAssertEqual(right["result"] as? String, "guest")
        XCTAssertNotEqual(try XCTUnwrap(left["realm"] as? String), try XCTUnwrap(right["realm"] as? String))
    }

    func testTheInitialEmptyDocumentHasItsOwnOpaqueRealm() throws {
        let arm = try arm(), seen = Seen()
        let (handle, web) = mount(arm, seen, path: nil)
        defer { arm.destroy(handle) }
        let ready = expectation(description: "external view configuration finished")
        web.evaluateJavaScript("window.origin = 'traced'") { _, _ in ready.fulfill() }
        wait(for: [ready], timeout: 5)
        let reply = run(arm, "evaluate", ["expression": "window.origin"])
        XCTAssertEqual(reply["result"] as? String, "traced")
        XCTAssertEqual(reply["origin"] as? String, "null")
        let realm = try XCTUnwrap(reply["realm"] as? String)
        XCTAssertEqual(run(arm, "evaluate", ["target": ["realm": realm], "expression": "42"])["result"] as? Int, 42)
    }

    func testAConfigurationHatchCannotChangeTheReportedOrigin() throws {
        let arm = try arm(), seen = Seen()
        let (handle, _) = mount(arm, seen, configuring: { _, config in
            let c = Unmanaged<WKWebViewConfiguration>.fromOpaque(config!).takeUnretainedValue()
            c.userContentController = WKUserContentController()
            for world in [WKContentWorld.page, .world(name: "wallet")] {
                c.userContentController.addUserScript(WKUserScript(source: "window.origin = 'traced'",
                    injectionTime: .atDocumentStart, forMainFrameOnly: true, in: world))
            }
        })
        defer { arm.destroy(handle) }
        for name in [nil, "wallet"] as [String?] {
            var options: [String: Any] = ["expression": "window.origin"]
            options["sandbox"] = name
            let reply = run(arm, "evaluate", options)
            XCTAssertEqual(reply["result"] as? String, "traced")
            XCTAssertEqual(reply["origin"] as? String, baseURL)
        }
    }

    func testOriginAndRealmFollowTheExecutingDocumentAcrossHistory() throws {
        let arm = try arm(), seen = Seen()
        let (handle, web) = mount(arm, seen)
        defer { arm.destroy(handle) }
        let second = try AutomationPages()
        let other = "http://127.0.0.1:\(second.port)"
        let first = run(arm, "evaluate", ["expression": "window.pageMarker = 17"])
        let original = try XCTUnwrap(first["realm"] as? String)
        XCTAssertEqual(first["origin"] as? String, baseURL)
        let delegate = web.navigationDelegate
        web.navigationDelegate = nil
        defer { web.navigationDelegate = delegate; withExtendedLifetime(second) {} }

        func checkOrigin(_ origin: String) {
            for name in [nil, "approval"] as [String?] {
                var options: [String: Any] = ["expression": "location.origin"]
                options["sandbox"] = name
                let response = run(arm, "evaluate", options)
                XCTAssertEqual(response["result"] as? String, origin)
                XCTAssertEqual(response["origin"] as? String, origin)
            }
        }
        _ = try XCTUnwrap(web.load(URLRequest(url: URL(string: other + "/other")!)))
        spin { !web.isLoading }
        checkOrigin(other)
        let target = ["target": ["realm": original], "expression": "window.pageMarker"] as [String: Any]
        XCTAssertEqual(run(arm, "evaluate", target)["error"] as? String, "no such frame")
        _ = try XCTUnwrap(web.goBack())
        spin { !web.isLoading }
        var restored: Any?
        let read = expectation(description: "WebKit's restored document")
        web.evaluateJavaScript("window.pageMarker") { value, _ in restored = value; read.fulfill() }
        wait(for: [read], timeout: 5)
        let reply = run(arm, "evaluate", target)
        if restored as? Int == 17 {
            XCTAssertEqual(reply["result"] as? Int, 17)
            XCTAssertEqual(reply["origin"] as? String, baseURL)
        } else { XCTAssertEqual(reply["error"] as? String, "no such frame") }
        checkOrigin(baseURL)
    }

    func testChannelArgumentsUseOnlyTheNamedWorldAndKeepThePostingDocument() throws {
        let arm = try arm(), seen = Seen()
        let (handle, web) = mount(arm, seen)
        defer { arm.destroy(handle) }
        let received = Seen()
        let deliver: Message = { params in
            received.records.append(("script.message", params as! [String: Any]))
        }
        let channel = ["type": "channel", "value": ["channel": "playback"]] as [String: Any]
        var options: [String: Any] = ["arguments": [channel], "channelSession": "first session",
                                      "functionDeclaration": "post => { post('pause'); return 1 }"]
        for op in ["callFunction", "addPreloadScript"] {
            XCTAssertEqual(run(arm, op, options, message: deliver)["error"] as? String, "NotSupportedError")
        }
        options["sandbox"] = "video"
        XCTAssertEqual(value(arm, "(window.post = () => ++window.calls, window.calls = 0, true)") as? Bool, true)
        let first = run(arm, "callFunction", options, message: deliver)
        let realm = try XCTUnwrap(first["realm"] as? String)
        spin { received.records.count == 1 }
        let message = try XCTUnwrap(received.records.first?.record)
        XCTAssertEqual(message["channel"] as? String, "playback")
        XCTAssertEqual(message["json"] as? String, "\"pause\"")
        XCTAssertEqual(message["source"] as? [String: String], ["realm": realm, "context": "tab", "origin": baseURL])
        XCTAssertEqual(value(arm, "window.calls") as? Int, 0)
        options["functionDeclaration"] = "post => { try { post('é'.repeat(32768)); } catch(e) { return e.name } }"
        XCTAssertEqual(run(arm, "callFunction", options, message: deliver)["result"] as? String, "QuotaExceededError")
        let pageCanary = "window.calls = 0; window.post = window.playback = () => ++window.calls; addEventListener('message', window.post); Object.prototype.pageCanary = true"
        web.configuration.userContentController.addUserScript(WKUserScript(source: pageCanary, injectionTime: .atDocumentStart, forMainFrameOnly: true, in: .page))
        options["functionDeclaration"] = "post => { globalThis.didFirst = true; post(({play: true}).pageCanary ?? 'play') }"
        let firstScript = try XCTUnwrap(run(arm, "addPreloadScript", options, message: deliver)["script"] as? String)
        let script = try XCTUnwrap(web.configuration.userContentController.userScripts.last?.source)
        let handlerRange = try XCTUnwrap(script.range(of: "_message[A-Fa-f0-9]+", options: .regularExpression))
        let handler = String(script[handlerRange])
        XCTAssertEqual(value(arm, "typeof globalThis.webkit?.messageHandlers?.\(handler)") as? String, "undefined")
        // A raw WebKit post without the issued channel's capability does not enter its receiver.
        XCTAssertEqual(value(arm, "(webkit.messageHandlers.\(handler).postMessage(['unissued','\(UUID().uuidString)','1']), true)", sandbox: "video") as? Bool, true)
        let other = Seen()
        var second = options
        second["channelSession"] = "another session"
        second["functionDeclaration"] = "post => post('other')"
        XCTAssertEqual(run(arm, "callFunction", second, message: { other.records.append(("message", $0 as! [String: Any])) })["error"] as? String, nil)
        spin { other.records.count == 1 }
        XCTAssertEqual(received.records.count, 1)
        let before = seen.records.count
        web.reload()
        spin { seen.records.dropFirst(before).contains { $0.event == "navigatesuccess" } && received.records.count == 2 }
        XCTAssertEqual(other.records.count, 1)

        let fresh = try XCTUnwrap(received.records.last?.record["source"] as? [String: String])
        XCTAssertEqual(received.records.last?.record["json"] as? String, "\"play\"")
        XCTAssertEqual(value(arm, "window.calls") as? Int, 0)
        // WebKit schedules the worlds independently. Run the registered preload again after
        // reading the page's own function so this case observes the page-first order.
        XCTAssertEqual(value(arm, "typeof window.playback") as? String, "function")
        XCTAssertEqual(value(arm, "(() => { \(script) return true })()", sandbox: "video") as? Bool, true)
        spin { received.records.count == 3 }
        XCTAssertEqual(received.records.last?.record["json"] as? String, "\"play\"")
        XCTAssertEqual(value(arm, "window.calls") as? Int, 0)
        XCTAssertEqual(fresh["origin"], baseURL)
        XCTAssertNotEqual(fresh["realm"], realm)
        XCTAssertEqual(run(arm, "evaluate", ["target": ["realm": try XCTUnwrap(fresh["realm"])], "expression": "2"])["result"] as? Int, 2)
        XCTAssertEqual(run(arm, "evaluate", ["target": ["realm": realm], "expression": "3"])["error"] as? String, "no such frame")
        XCTAssertNil(run(arm, "releaseScriptChannels", ["channelSession": "first session"])["error"])
        XCTAssertEqual(run(arm, "removePreloadScript", ["script": firstScript])["error"] as? String, "NotFoundError")
        XCTAssertNil(run(arm, "addPreloadScript", second, message: { other.records.append(("message", $0 as! [String: Any])) })["error"])
        web.reload()
        spin { other.records.count == 2 }
        XCTAssertEqual(value(arm, "typeof globalThis.didFirst", sandbox: "video") as? String, "undefined")
        XCTAssertEqual(received.records.count, 3)
    }

    func testQueuedPauseSurvivesTheLastOwnerOfItsView() throws {
        for replace in [false, true] {
            let arm = try arm(), seen = Seen(), player = Seen()
            var handle: UnsafeMutableRawPointer?
            var target: WKWebView?
            weak var retired: WKWebView?
            defer { if let handle { arm.destroy(handle) } }
            let realm: String = try autoreleasepool {
                (handle, target) = mount(arm, seen)
                retired = target
                let channel: [String: Any] = ["type": "channel", "value": ["channel": "playback"]]
                let args: [String: Any] = ["sandbox": "player", "arguments": [channel], "channelSession": "player",
                                           "functionDeclaration": "post => { window.send = post }"]
                return try XCTUnwrap(run(arm, "callFunction", args, message: { record in
                    if record["json"] as? String == "0" {
                        target?.removeFromSuperview()
                        target = nil
                        if replace {
                            self.json(["element": "next", "partition": "next", "owner": Self.owner], arm.configure, handle!)
                        } else {
                            arm.destroy(handle!)
                            handle = nil
                        }
                    } else { player.records.append(("post", record as! [String: Any])) }
                })["realm"] as? String)
            }
            autoreleasepool {
                target?.evaluateJavaScript("send(0); send('pause')", in: nil, in: .world(name: "player"), completionHandler: nil)
                spin { !player.records.isEmpty }
            }
            spin { autoreleasepool { retired == nil } }
            XCTAssertEqual(player.records.first?.record["json"] as? String, "\"pause\"")
            XCTAssertEqual(player.records.first?.record["source"] as? [String: String], ["origin": baseURL, "context": "tab", "realm": realm])
            XCTAssertNil(retired)
        }
    }

    private typealias DeliveryHold = @convention(block) (Bool) -> Void

    private func channel(_ session: String, _ function: String) -> [String: Any] {
        ["channelSession": session, "sandbox": "player", "functionDeclaration": function,
         "arguments": [["type": "channel", "value": ["channel": "playback"]]]]
    }

    // A post on another session crosses the same WebKit connection after the preceding turn.
    private func waitForPostedTurn(_ arm: Arm) {
        var reached = false
        let args = channel("checkpoint", "post => post(0)")
        XCTAssertNil(run(arm, "callFunction", args, message: { _ in reached = true })["error"])
        spin { reached }
        XCTAssertTrue(reached)
        XCTAssertNil(run(arm, "releaseScriptChannels", ["channelSession": "checkpoint"])["error"])
    }

    func testHeldSessionAcrossAnEmptyFrameSet() throws {
        let arm = try arm(), first = Seen(), second = Seen()
        let args = channel("player session", "post => post(1)")
        defer { _ = run(arm, "releaseScriptChannels", ["channelSession": "player session"]) }
        var held: DeliveryHold?
        weak var retired: WKWebView?
        weak var firstReceiver: Seen?
        try autoreleasepool {
            let (handle, web) = mount(arm, first)
            defer { web.removeFromSuperview(); arm.destroy(handle) }
            retired = web
            let receiver = Seen()
            firstReceiver = receiver
            XCTAssertNil(run(arm, "callFunction", args, message: { record in
                receiver.records.append(("post", [:]))
                if let block = record["holdDelivery"] {
                    held = unsafeBitCast(block as AnyObject, to: DeliveryHold.self)
                    held?(true)
                }
            })["error"])
            spin { held != nil }
            _ = try XCTUnwrap(held)
        }
        spin { autoreleasepool { retired == nil && firstReceiver == nil } }
        XCTAssertNil(retired)
        XCTAssertNil(firstReceiver)
        let (handle, _) = mount(arm, second)
        defer { arm.destroy(handle) }
        var values: [String] = []
        XCTAssertNil(run(arm, "callFunction", args, message: { record in
            if let json = record["json"] as? String { values.append(json) }
        })["error"])
        waitForPostedTurn(arm)
        XCTAssertEqual(values, [])
        held?(false)
        spin { !values.isEmpty }
        XCTAssertEqual(values, ["1"])
    }

    func testRetiredPostsDoNotDisplaceAnyOfThe32WaitingPosts() throws {
        let arm = try arm(), seen = Seen()
        let (handle, web) = mount(arm, seen)
        defer { arm.destroy(handle) }
        let args = channel("full queue", "post => { window.send = post; post(0) }")
        defer { _ = run(arm, "releaseScriptChannels", ["channelSession": "full queue"]) }
        var held: DeliveryHold?, values: [Int] = [], refused = 0, overflows = 0
        XCTAssertNil(run(arm, "callFunction", args, message: { record in
            if record["retiredPost"] as? Bool == true { refused += 1 }
            if record["overflow"] as? Bool == true { overflows += 1 }
            if let json = record["json"] as? String, let value = Int(json) {
                values.append(value)
                if value == 0, let block = record["holdDelivery"] {
                    held = unsafeBitCast(block as AnyObject, to: DeliveryHold.self)
                    held?(true)
                }
            }
        })["error"])
        spin { held != nil }
        let release = try XCTUnwrap(held)
        XCTAssertNil(run(arm, "evaluate", ["sandbox": "player", "expression": "Array.from({length: 32}, (_, i) => send(i + 1)).length"])["error"])
        waitForPostedTurn(arm)
        json(["element": "next", "partition": "next", "owner": Self.owner], arm.configure, handle)
        web.evaluateJavaScript("send(33); send(34)", in: nil, in: .world(name: "player"), completionHandler: nil)
        spin { refused == 2 }
        XCTAssertEqual(refused, 2)
        XCTAssertEqual(values, [0])
        release(false)
        spin { values.last == 32 }
        XCTAssertEqual(values, Array(0...32))
        XCTAssertEqual(overflows, 0)
    }

    func testOnlyTheFrameThatStillOwnsTheViewAcceptsNewPosts() throws {
        let arm = try arm(), seen = Seen()
        let (handle, web) = mount(arm, seen)
        defer { arm.destroy(handle) }
        let channel: [String: Any] = ["type": "channel", "value": ["channel": "playback"]]
        let args: [String: Any] = ["sandbox": "player", "arguments": [channel], "channelSession": "parked player",
                                   "functionDeclaration": "post => { window.send = post }"]
        var values: [String] = [], drops = 0
        let result = run(arm, "callFunction", args, message: { record in
            if let json = record["json"] as? String { values.append(json) }
            if record["retiredPost"] as? Bool == true { drops += 1 }
        })
        XCTAssertNil(result["error"])
        XCTAssertNil(run(arm, "addPreloadScript", args, message: { _ in })["error"])
        let script = try XCTUnwrap(web.configuration.userContentController.userScripts.last?.source)
        let range = try XCTUnwrap(script.range(of: "_message[A-Fa-f0-9]+", options: .regularExpression))
        let handler = script[range]
        let unknown = "webkit.messageHandlers.\(handler).postMessage(['unissued','\(UUID().uuidString)','turn','99'])"
        XCTAssertEqual(value(arm, "(\(unknown), true)", sandbox: "player") as? Bool, true)
        waitForPostedTurn(arm)
        XCTAssertTrue(values.isEmpty)
        func post(_ n: Int) {
            web.evaluateJavaScript("send(\(n))", in: nil, in: .world(name: "player"), completionHandler: nil)
        }
        post(1)
        spin { values.count == 1 }
        let parent = try XCTUnwrap(web.superview)
        web.removeFromSuperview()
        post(2)
        spin { values.count == 2 }
        parent.addSubview(web)
        post(3)
        spin { values.count == 3 }
        seen.records.removeAll()
        json(["element": "next", "partition": "next", "owner": Self.owner], arm.configure, handle)
        post(4)
        spin { drops > 0 }
        spin { seen.records.contains { $0.event == "navigatesuccess" } }
        XCTAssertTrue(seen.records.contains { $0.event == "navigatesuccess" })
        var successor = args
        successor["functionDeclaration"] = "post => post(5)"
        XCTAssertNil(run(arm, "callFunction", successor, id: "next", message: { record in
            if let json = record["json"] as? String { values.append(json) }
        })["error"])
        spin { values.last == "5" }
        XCTAssertEqual(drops, 1)
        XCTAssertEqual(values, ["1", "2", "3", "5"])
        XCTAssertNil(run(arm, "releaseScriptChannels", ["channelSession": "parked player"])["error"])
    }

    func testClosingASessionReleasesItsUnmountedReceiverBeforeTheFlush() throws {
        let arm = try arm(), seen = Seen()
        weak var retained: Seen?
        autoreleasepool {
            let (handle, web) = mount(arm, seen)
            let receiver = Seen()
            retained = receiver
            let channel: [String: Any] = ["type": "channel", "value": ["channel": "playback"]]
            let args: [String: Any] = ["sandbox": "player", "arguments": [channel], "channelSession": "closing",
                                       "functionDeclaration": "post => { window.send = post }"]
            XCTAssertNil(run(arm, "callFunction", args, message: { receiver.records.append(("post", $0 as! [String: Any])) })["error"])
            web.removeFromSuperview()
            arm.destroy(handle)
        }
        autoreleasepool {
            XCTAssertNotNil(retained)
            XCTAssertNil(run(arm, "releaseScriptChannels", ["channelSession": "closing"])["error"])
        }
        XCTAssertNil(retained)
    }

    func testClosingSessionsRemovesTheirDocumentStartScripts() throws {
        let arm = try arm(), seen = Seen()
        let (handle, web) = mount(arm, seen)
        let controller = web.configuration.userContentController
        let baseline = controller.userScripts.count
        let plain = try XCTUnwrap(run(arm, "addPreloadScript", ["sandbox": "plain", "channelSession": "plain session",
            "functionDeclaration": "() => {}"])["script"] as? String)
        XCTAssertNil(run(arm, "releaseScriptChannels", ["channelSession": "plain session"])["error"])
        XCTAssertEqual(controller.userScripts.count, baseline)
        XCTAssertEqual(run(arm, "removePreloadScript", ["script": plain])["error"] as? String, "NotFoundError")
        let channel: [String: Any] = ["type": "channel", "value": ["channel": "playback"]]
        func options(_ session: String, sandbox: String) -> [String: Any] {
            ["sandbox": sandbox, "channelSession": session, "arguments": [channel], "functionDeclaration": "post => {}"]
        }
        let keep = options("other", sandbox: "shared")
        XCTAssertNotNil(run(arm, "addPreloadScript", keep, message: { _ in })["script"])
        let sharedCount = controller.userScripts.count
        for sandbox in ["shared", "wallet", "video"] {
            let args = options("closing", sandbox: sandbox)
            let key = try XCTUnwrap(run(arm, "addPreloadScript", args, message: { _ in })["script"] as? String)
            XCTAssertNil(run(arm, "releaseScriptChannels", ["channelSession": "closing"])["error"])
            XCTAssertEqual(run(arm, "removePreloadScript", ["script": key])["error"] as? String, "NotFoundError")
            XCTAssertEqual(controller.userScripts.count, sharedCount)
        }
        // A refused call must release the native handler's only reference to its receiver.
        weak var retained: Seen?
        autoreleasepool {
            let receiver = Seen()
            retained = receiver
            var args = options("refused", sandbox: "shared")
            args["functionDeclaration"] = "invalid !"
            XCTAssertEqual(run(arm, "callFunction", args, message: { receiver.records.append(("message", $0 as! [String: Any])) })["error"] as? String, "SyntaxError")
        }
        XCTAssertNil(retained)
        autoreleasepool {
            let receiver = Seen()
            retained = receiver
            var args = options("unbound", sandbox: "shared")
            args["functionDeclaration"] = "missingFunction"
            XCTAssertEqual(run(arm, "callFunction", args, message: { receiver.records.append(("message", $0 as! [String: Any])) })["error"] as? String, "ReferenceError")
        }
        XCTAssertNil(retained, "resolving the callee failed before its channel argument was handed out")
        var pending: Answer?
        autoreleasepool {
            let receiver = Seen()
            retained = receiver
            var args = options("pending", sandbox: "shared")
            args["functionDeclaration"] = "post => { post('pause'); return window.pending = new Promise(() => {}) }"
            pending = call(arm, "callFunction", args, message: { receiver.records.append(("message", $0 as! [String: Any])) })
            spin { !receiver.records.isEmpty }
            XCTAssertEqual(receiver.records.first?.record["json"] as? String, "\"pause\"")
        }
        web.reload()
        XCTAssertEqual(answered(try XCTUnwrap(pending))["error"] as? String, "AbortError")
        XCTAssertNotNil(retained, "a post already on its way can still reach its receiver")
        XCTAssertNil(run(arm, "releaseScriptChannels", ["channelSession": "pending"])["error"])
        XCTAssertNil(retained)
        arm.destroy(handle)
        XCTAssertEqual(controller.userScripts.count, baseline)
    }

    /// A preload script runs at document start in each new document until
    /// it is removed; the arm's own scripts stay. Adding or removing one is a
    /// read: the frame stays the person's.
    func testPreloadScriptsRunAtDocumentStart() throws {
        let arm = try arm(), seen = Seen()
        let (handle, web) = mount(arm, seen)
        defer { arm.destroy(handle) }
        let own = web.configuration.userContentController.userScripts.count
        let added = run(arm, "addPreloadScript", ["functionDeclaration": "() => { window.early = document.readyState }"])
        let script = try XCTUnwrap(added["script"] as? String, "\(added)")
        let reload = { seen.records.removeAll(); web.reload(); self.spin { seen.records.contains { $0.event == "navigatesuccess" } } }
        reload()
        XCTAssertEqual(value(arm, "window.early") as? String, "loading")
        XCTAssertNil(run(arm, "addPreloadScript", ["functionDeclaration": "() => { window.isolated = 1 }", "sandbox": "t3"])["error"])
        XCTAssertNil(run(arm, "removePreloadScript", ["script": script])["error"])
        XCTAssertEqual(run(arm, "removePreloadScript", ["script": script])["error"] as? String, "NotFoundError")
        reload()
        XCTAssertEqual(value(arm, "typeof window.early") as? String, "undefined")
        XCTAssertEqual(value(arm, "window.isolated", sandbox: "t3") as? Int, 1)
        XCTAssertEqual(web.configuration.userContentController.userScripts.count, own + 1)
        XCTAssertEqual(controller(seen), "person", "a preload script took the frame")
    }

    /// Input reaches the page: a click, typed text, a key with a modifier
    /// held, and a wheel. Whether the page trusts it is measured (D9).
    func testInputReachesThePage() throws {
        #if os(iOS)
        guard hostedWindowScene() != nil else { throw missing("scene", "WebKit lays out a page only in an app's window scene") }
        #endif
        let arm = try arm(), seen = Seen()
        let (handle, web) = mount(arm, seen)
        defer { arm.destroy(handle) }
        #if os(macOS)
        let window = try XCTUnwrap(web.window)
        final class Focusable: NSView { override var acceptsFirstResponder: Bool { true } }
        let focus = Focusable()
        window.contentView!.addSubview(focus)
        window.makeFirstResponder(focus)
        #endif
        let click: [[String: Any]] = [["type": "pointer", "id": "mouse", "actions": [
            ["type": "pointerMove", "x": 60, "y": 20], ["type": "pointerDown", "button": 0], ["type": "pointerUp", "button": 0]]]]
        XCTAssertEqual(run(arm, "performActions", ["actions": click]) as NSDictionary, [:] as NSDictionary)
        spin { (self.value(arm, "log.filter(e => e.type === 'click').length") as? Int ?? 0) > 0 }
        let clicked = value(arm, "JSON.stringify(log.filter(e => ['pointerdown', 'mousedown', 'mouseup', 'click'].includes(e.type)))") as? String ?? ""
        say("iframe automation: a click's events \(clicked)")
        XCTAssert(clicked.contains("\"click\""), clicked)
        let typing: [[String: Any]] = [["type": "pointer", "id": "mouse", "actions": [
            ["type": "pointerMove", "x": 100, "y": 70], ["type": "pointerDown", "button": 0], ["type": "pointerUp", "button": 0]]],
            ["type": "key", "id": "keys", "actions": [["type": "pause"], ["type": "pause"], ["type": "pause"],
            ["type": "keyDown", "value": "H"], ["type": "keyUp", "value": "H"], ["type": "keyDown", "value": "i"], ["type": "keyUp", "value": "i"],
            ["type": "keyDown", "value": "!"], ["type": "keyUp", "value": "!"]]]]
        XCTAssertEqual(run(arm, "performActions", ["actions": typing]) as NSDictionary, [:] as NSDictionary)
        spin { self.value(arm, "document.getElementById('field').value") as? String == "Hi!" }
        XCTAssertEqual(value(arm, "document.getElementById('field').value") as? String, "Hi!", value(arm, "JSON.stringify(log.filter(e => e.type.startsWith('key') || e.type === 'input'))") as? String ?? "")
        let keyed = value(arm, "JSON.stringify(log.filter(e => e.type === 'keydown' || e.type === 'input').slice(0, 2))") as? String ?? ""
        say("iframe automation: typed events \(keyed)")
        let chord: [[String: Any]] = [["type": "key", "id": "keys", "actions": [
            ["type": "keyDown", "value": "\u{E008}"], ["type": "keyDown", "value": "\u{E012}"], ["type": "keyUp", "value": "\u{E012}"], ["type": "keyUp", "value": "\u{E008}"]]]]
        XCTAssertEqual(run(arm, "performActions", ["actions": chord]) as NSDictionary, [:] as NSDictionary)
        spin { (self.value(arm, "log.filter(e => e.type === 'keydown' && e.key === 'ArrowLeft').length") as? Int ?? 0) > 0 }
        XCTAssertEqual(value(arm, "log.filter(e => e.type === 'keydown' && e.key === 'ArrowLeft').map(e => e.shift).join()") as? String, "true", "Shift held for ArrowLeft")
        let scroll: [[String: Any]] = [["type": "wheel", "id": "wheel", "actions": [["type": "scroll", "x": 100, "y": 200, "deltaX": 0, "deltaY": 120]]]]
        XCTAssertEqual(run(arm, "performActions", ["actions": scroll]) as NSDictionary, [:] as NSDictionary)
        spin { (self.value(arm, "document.getElementById('list').scrollTop") as? Double ?? 0) > 0 }
        let wheel = value(arm, "JSON.stringify([document.getElementById('list').scrollTop, log.filter(e => e.type === 'wheel').slice(0, 1)])") as? String ?? ""
        say("iframe automation: a wheel \(wheel)")
        XCTAssertGreaterThan(value(arm, "document.getElementById('list').scrollTop") as? Double ?? 0, 0, wheel)
        let trusted = value(arm, "[...new Set(log.map(e => e.type + ':' + e.trusted))].join(' ')") as? String ?? ""
        say("iframe automation: isTrusted by event type: \(trusted)")
        #if os(macOS)
        XCTAssert(window.firstResponder === focus, "the focus went back to what had it: \(String(describing: window.firstResponder))")
        #else
        // The events the arm dispatches are untrusted; the `input` that its `execCommand('insertText')` makes is the editor's own.
        XCTAssertFalse(trusted.split(separator: " ").contains { $0.hasSuffix(":true") && !$0.hasPrefix("input:") }, "iOS input is script-dispatched: \(trusted)")
        #endif
        XCTAssertNil(run(arm, "releaseActions")["error"])
    }

    /// An agent-controlled frame's dialog fires `userpromptopened` and waits
    /// for `handleUserPrompt`; with no dialog open the call is refused.
    func testPromptsOfAnAgentControlledFrame() throws {
        let arm = try arm(), seen = Seen()
        let (handle, _) = mount(arm, seen)
        defer { arm.destroy(handle) }
        XCTAssertEqual(run(arm, "handleUserPrompt")["error"] as? String, "NotFoundError")
        XCTAssertNil(run(arm, "releaseActions")["error"], "an acting call takes the frame")
        // WebKit sends a script's answer after the task it ran in, and a dialog
        // that task's timer opened holds it: the call is answered once the dialog is.
        let asking = call(arm, "evaluate", ["expression": "setTimeout(() => { window.answer = prompt('Name?', 'Ada') }, 0), 1"])
        spin { seen.records.contains { $0.event == "userpromptopened" } }
        let opened = try XCTUnwrap(seen.records.last { $0.event == "userpromptopened" }?.record)
        XCTAssertEqual(opened["type"] as? String, "prompt")
        XCTAssertEqual(opened["message"] as? String, "Name?")
        XCTAssertEqual(opened["defaultValue"] as? String, "Ada")
        XCTAssertNil(run(arm, "handleUserPrompt", ["accept": true, "userText": "Grace"])["error"])
        XCTAssertEqual(answered(asking)["result"] as? Int, 1)
        spin { self.value(arm, "window.answer") as? String == "Grace" }
        XCTAssertEqual(value(arm, "window.answer") as? String, "Grace")
        let confirming = call(arm, "evaluate", ["expression": "setTimeout(() => { window.sure = confirm('Sure?') }, 0), 1"])
        spin { seen.records.filter { $0.event == "userpromptopened" }.count == 2 }
        XCTAssertNil(run(arm, "handleUserPrompt", ["accept": false])["error"])
        XCTAssertNil(answered(confirming)["error"])
        spin { self.value(arm, "String(window.sure)") as? String == "false" }
        XCTAssertEqual(value(arm, "String(window.sure)") as? String, "false")
    }

    /// A clipped picture reads the frame's viewport with a script first. A
    /// dialog holding that script when the frame unmounts leaves the call
    /// to end with the frame (`NotFoundError`).
    func testAClippedPictureHeldByADialogEndsWithItsFrame() throws {
        let arm = try arm(), seen = Seen()
        let (handle, _) = mount(arm, seen)
        XCTAssertNil(run(arm, "releaseActions")["error"], "an acting call takes the frame")
        _ = call(arm, "evaluate", ["expression": "setTimeout(() => alert('Held'), 0), 1"])
        spin { seen.records.contains { $0.event == "userpromptopened" } }
        let file = Self.folder.appendingPathComponent("held.png")
        let shot = call(arm, "captureScreenshot", ["path": "app:/tmp/held.png", "pathFile": file.path, "clip": ["x": 0, "y": 0, "width": 100, "height": 50]])
        pause(0.2)
        XCTAssertNil(shot.value, "the dialog holds the viewport's script")
        arm.destroy(handle)
        let ended = answered(shot, 5)
        XCTAssertEqual(ended["error"] as? String, "NotFoundError", "\(ended)")
    }

    /// An agent-controlled frame's download goes to the folder, with no panel.
    func testDownloadsGoToTheFolder() throws {
        let arm = try arm(), seen = Seen()
        // A cold WebKit threat lookup can hold the download policy past the test. Both pages are served on loopback.
        let (handle, _) = mount(arm, seen, configuring: { _, config in
            let configuration = Unmanaged<WKWebViewConfiguration>.fromOpaque(config!).takeUnretainedValue()
            configuration.preferences.isFraudulentWebsiteWarningEnabled = false
        })
        defer { arm.destroy(handle) }
        let folder = Self.folder.appendingPathComponent("downloads", isDirectory: true)
        try? FileManager.default.removeItem(at: folder)
        XCTAssertNil(run(arm, "setDownloadBehavior", ["destinationFolder": "app:/tmp/downloads", "destinationFolderFile": folder.path])["error"])
        XCTAssertNil(run(arm, "evaluate", ["expression": "location.href = '/download', 1"])["error"])
        spin { seen.records.contains { $0.event == "download" && $0.record["state"] as? String == "completed" } }
        let record = try XCTUnwrap(seen.records.last { $0.event == "download" && $0.record["state"] as? String == "completed" }?.record)
        XCTAssertEqual(record["path"] as? String, "app:/tmp/downloads/report.txt", "\(record)")
        XCTAssertEqual(try? String(contentsOf: folder.appendingPathComponent("report.txt"), encoding: .utf8), "a report\n")
    }

    /// A call ends with its document (`AbortError`) or its frame
    /// (`NotFoundError`); an unpartitioned frame is not driven, but any
    /// frame's picture is taken.
    func testCallsEndWithTheirDocumentOrFrame() throws {
        let arm = try arm(), seen = Seen()
        let (handle, _) = mount(arm, seen)
        let navigated = call(arm, "evaluate", ["expression": "new Promise(r => { window.held = r })"])
        pause(0.2)
        XCTAssertNil(navigated.value)
        XCTAssertNil(run(arm, "evaluate", ["expression": "location.href = '/other', 1"])["error"])
        XCTAssertEqual(answered(navigated)["error"] as? String, "AbortError")
        let file = Self.folder.appendingPathComponent("shot.png")
        let shot = run(arm, "captureScreenshot", ["path": "app:/tmp/shot.png", "pathFile": file.path, "clip": ["x": 0, "y": 0, "width": 100, "height": 50]])
        XCTAssertEqual(shot["path"] as? String, "app:/tmp/shot.png", "\(shot)")
        XCTAssertGreaterThan(shot["width"] as? Int ?? 0, 99, "\(shot)")
        // A promise the page holds: WebKit drops a call whose promise is collected.
        let unmounted = call(arm, "evaluate", ["expression": "new Promise(r => { window.held = r })"])
        pause(0.2)
        arm.destroy(handle)
        let ended = answered(unmounted)
        XCTAssertEqual(ended["error"] as? String, "NotFoundError", "\(ended)")
        XCTAssertEqual(run(arm, "evaluate", ["expression": "1"])["error"] as? String, "NotFoundError")

        let plain = Seen()
        let (unpartitioned, _) = mount(arm, plain, partition: nil, id: "plain")
        defer { arm.destroy(unpartitioned) }
        XCTAssertEqual(run(arm, "evaluate", ["expression": "1"], id: "plain")["error"] as? String, "NotSupportedError")
    }

    /// A promise nothing holds pends, as BiDi's does, though WebKit collects
    /// it and fails the call: the call ends with its document. A script that
    /// does not parse is still refused.
    func testAPromiseNothingHoldsPendsUntilItsDocumentEnds() throws {
        let arm = try arm(), seen = Seen()
        let (handle, _) = mount(arm, seen)
        defer { arm.destroy(handle) }
        let page = call(arm, "evaluate", ["expression": "new Promise(() => {})"])
        let world = call(arm, "evaluate", ["expression": "new Promise(() => {})", "sandbox": "t3"])
        // Garbage, to have WebKit collect both promises.
        let churn = "(() => { let n = 0; for (let i = 0; i < 400; i++) n += new Array(100000).fill(i).length; return n })()"
        for _ in 0..<5 {
            XCTAssertNil(run(arm, "evaluate", ["expression": churn])["error"])
            XCTAssertNil(run(arm, "evaluate", ["expression": churn, "sandbox": "t3"])["error"])
            pause(0.3)
        }
        XCTAssertNil(page.value, "the page's world: \(page.value ?? [:])")
        XCTAssertNil(world.value, "a sandbox: \(world.value ?? [:])")
        XCTAssertEqual(run(arm, "evaluate", ["expression": "1 +"])["error"] as? String, "SyntaxError")
        XCTAssertEqual(run(arm, "evaluate", ["expression": "1 } let __exactArguments = 0; {"])["error"] as? String, "SyntaxError",
                       "parsed with the call's own parameter")
        XCTAssertEqual(run(arm, "evaluate", ["expression": "Promise.reject(Object.create(null))"])["error"] as? String, "Error",
                       "an exception that is no text still answers")
        XCTAssertNil(run(arm, "evaluate", ["expression": "Object.prototype.then = (_, no) => no(new Error('then')), 1"])["error"])
        XCTAssertEqual(run(arm, "evaluate", ["expression": "missing"])["error"] as? String, "ReferenceError", "a page's then is not ours")
        XCTAssertNil(run(arm, "evaluate", ["expression": "location.href = '/other', 1"])["error"])
        XCTAssertEqual(answered(page)["error"] as? String, "AbortError")
        XCTAssertEqual(answered(world)["error"] as? String, "AbortError")
    }

    #if os(macOS)
    /// A key the page leaves unhandled, which WebKit sends on through
    /// `NSApp.sendEvent`, is neither a person's (the frame stays the agent's)
    /// nor the app's menus' (⌘W does not close).
    func testAKeyThePageLeavesIsNotAPersonsNorTheMenus() throws {
        final class Sink: NSObject { var fired = 0; @objc func close(_ sender: Any?) { fired += 1 } }
        let sink = Sink(), menu = NSMenu(), top = NSMenuItem(), sub = NSMenu()
        let close = NSMenuItem(title: "Close", action: #selector(Sink.close(_:)), keyEquivalent: "w")
        close.target = sink
        sub.addItem(close); top.submenu = sub; menu.addItem(top)
        let before = NSApp.mainMenu
        NSApp.mainMenu = menu
        defer { NSApp.mainMenu = before }
        let arm = try arm(), seen = Seen()
        let (handle, web) = mount(arm, seen)
        defer { arm.destroy(handle) }
        XCTAssertNil(run(arm, "releaseActions")["error"])
        spin { self.controller(seen) == "agent" }
        let escape: [[String: Any]] = [["type": "key", "id": "k", "actions": [["type": "keyDown", "value": "\u{E00C}"], ["type": "keyUp", "value": "\u{E00C}"]]]]
        XCTAssertEqual(run(arm, "performActions", ["actions": escape]) as NSDictionary, [:] as NSDictionary)
        pause(0.5)
        XCTAssertEqual(controller(seen), "agent", "an unhandled Escape is not a person's")
        let closeChord: [[String: Any]] = [["type": "key", "id": "k", "actions": [["type": "keyDown", "value": "\u{E03D}"], ["type": "keyDown", "value": "w"],
                                                                                ["type": "keyUp", "value": "w"], ["type": "keyUp", "value": "\u{E03D}"]]]]
        XCTAssertEqual(run(arm, "performActions", ["actions": closeChord]) as NSDictionary, [:] as NSDictionary)
        pause(0.5)
        XCTAssertEqual(sink.fired, 0, "⌘W reached the app's menu")
        XCTAssertTrue(web.window?.isVisible ?? false)
        XCTAssertEqual(controller(seen), "agent")
        let outside: [[String: Any]] = [["type": "pointer", "id": "m", "actions": [["type": "pointerMove", "x": 60, "y": 515], ["type": "pointerDown", "button": 0]]]]
        XCTAssertEqual(run(arm, "performActions", ["actions": outside])["error"] as? String, "move target out of bounds")
    }
    #endif

    #if os(macOS)
    /// A person's click in the frame takes it back: input in flight ends
    /// with `AbortError`, and a held dialog becomes the person's sheet.
    func testAPersonTakesTheFrameBack() throws {
        let arm = try arm(), seen = Seen()
        let (handle, web) = mount(arm, seen)
        defer { arm.destroy(handle) }
        let window = try XCTUnwrap(web.window)
        let long: [[String: Any]] = [["type": "none", "id": "wait", "actions": [["type": "pause", "duration": 5000]]]]
        let acting = call(arm, "performActions", ["actions": long])
        spin { self.controller(seen) == "agent" }
        XCTAssertEqual(controller(seen), "agent")
        _ = call(arm, "evaluate", ["expression": "setTimeout(() => alert('Held'), 0), 1"])
        spin { seen.records.contains { $0.event == "userpromptopened" } }
        XCTAssertNil(window.attachedSheet, "no sheet while the agent has the frame")
        let point = web.convert(NSPoint(x: 200, y: 150), to: nil)
        let person = try XCTUnwrap(NSEvent.mouseEvent(with: .leftMouseDown, location: point, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                                      windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        NSApp.sendEvent(person)
        XCTAssertEqual(answered(acting)["error"] as? String, "AbortError")
        spin { self.controller(seen) == "person" }
        XCTAssertEqual(controller(seen), "person")
        spin { window.attachedSheet != nil }
        let sheet = try XCTUnwrap(window.attachedSheet, "the held dialog, shown to the person")
        window.endSheet(sheet, returnCode: .alertFirstButtonReturn)
        pause(0.3)
        XCTAssertEqual(value(arm, "log.filter(e => e.type === 'mousedown').length") as? Int, 0, "the click that brought the dialog up is the dialog's, not the page's")
    }
    #endif
}

/// Pages on a loopback port: `/input`, a page that logs its events (type,
/// `isTrusted`, key, Shift) into `window.log`, with a button, a field and a
/// scrolling list; `/download`, an attachment; `/other`, a second document.
final class AutomationPages {
    private let listener: NWListener

    var port: UInt16 { listener.port!.rawValue }

    init() throws { listener = try loopbackListener(Self.serve) }

    static let input = """
    <!doctype html><meta name="viewport" content="width=device-width, initial-scale=1"><title>Input</title>
    <style>body{margin:0;font:16px system-ui}button{position:absolute;left:20px;top:5px;width:80px;height:30px}
    input{position:absolute;left:20px;top:55px;width:200px;height:30px}
    #list{position:absolute;left:20px;top:120px;width:200px;height:150px;overflow:auto}#list div{height:40px}</style>
    <button id="button">Button</button><input id="field"><div id="list"></div>
    <script>
    window.log = [];
    for (let i = 0; i < 30; i++) list.append(Object.assign(document.createElement('div'), { textContent: 'Row ' + i }));
    for (const type of ['pointerdown', 'mousedown', 'mouseup', 'click', 'keydown', 'keyup', 'input', 'wheel'])
      addEventListener(type, e => log.push({ type, trusted: e.isTrusted, key: e.key, shift: e.shiftKey }), true);
    </script>
    """

    private static func serve(_ connection: NWConnection) {
        connection.start(queue: .main)
        connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { data, _, _, _ in
            guard let data, let line = String(decoding: data, as: UTF8.self).split(separator: "\r\n").first else { return connection.cancel() }
            let path = line.split(separator: " ").dropFirst().first.map(String.init) ?? "/"
            let (status, type, extra, body): (String, String, String, String) = switch path {
            case "/input": ("200 OK", "text/html; charset=utf-8", "", input)
            case "/other": ("200 OK", "text/html; charset=utf-8", "", "<!doctype html><title>Other</title><p>Other")
            case "/download": ("200 OK", "text/plain", "Content-Disposition: attachment; filename=\"report.txt\"\r\n", "a report\n")
            default: ("404 Not Found", "text/plain", "", "")
            }
            let response = "HTTP/1.1 \(status)\r\nContent-Type: \(type)\r\n\(extra)Content-Length: \(body.utf8.count)\r\nConnection: close\r\n\r\n\(body)"
            connection.send(content: Data(response.utf8), completion: .contentProcessed { _ in connection.cancel() })
        }
    }
}
#endif
