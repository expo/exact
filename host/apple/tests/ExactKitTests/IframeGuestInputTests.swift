import XCTest
import WebKit
@testable import ExactKit
#if os(macOS)
import AppKit
#else
import UIKit
#endif

#if os(macOS) || os(iOS)
/// The agent's `tap` and `type` into a guest (`GuestInput`) edit a rich-text
/// editor as a person would: text goes in at the caret, not over the whole
/// editable; a press places the caret; a chord's modifiers are the
/// KeyboardEvent's fields; and a target chosen for `type` is one that shows.
final class IframeGuestInputTests: XCTestCase {
    func testTypeAndTapEditAContentEditableGuest() throws {
        let web = WKWebView(frame: CGRect(x: 0, y: 0, width: 400, height: 200))
        #if os(macOS)
        _ = NSApplication.shared
        let window = NSWindow(contentRect: web.frame, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = web
        defer { window.close() }
        #else
        let window = UIWindow(frame: web.frame)
        window.addSubview(web)
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        #endif
        final class Loaded: NSObject, WKNavigationDelegate {
            let ready: XCTestExpectation
            init(_ ready: XCTestExpectation) { self.ready = ready }
            func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) { ready.fulfill() }
        }
        let ready = expectation(description: "guest loaded")
        let loaded = Loaded(ready)
        web.navigationDelegate = loaded
        web.loadHTMLString("""
        <!doctype html><body style="margin:0">
        <input id="hidden" style="display:none">
        <div id="editor" contenteditable style="font:20px monospace;padding:10px">hello world</div>
        <script>
        window.chords = [];
        document.addEventListener('keydown', e => {
          chords.push([e.key, e.metaKey, e.shiftKey].join(' '));
          if (e.metaKey && e.key === 's') e.preventDefault();
        });
        </script>
        """, baseURL: nil)
        wait(for: [ready], timeout: 30)
        defer { web.navigationDelegate = nil }
        // The web arm runs agent scripts in its own world (`WebArm.world`).
        let world = WKContentWorld.world(name: "exact.agent")
        func run(_ script: String, in page: WKContentWorld? = nil) throws -> String {
            let done = expectation(description: "script ran")
            var reply: Result<Any, Error> = .success("")
            web.evaluateJavaScript(script, in: nil, in: page ?? world) { reply = $0; done.fulfill() }
            wait(for: [done], timeout: 10)
            return try XCTUnwrap(try reply.get() as? String)
        }
        func page(_ expression: String) throws -> String { try run("JSON.stringify(\(expression))", in: .page) }

        XCTAssertTrue(try run(GuestInput.type(selector: nil, key: nil, text: "!")).contains("\"ok\":true"))
        XCTAssertEqual(try page("document.getElementById('editor').textContent"), "\"hello world!\"", "the shown editor, after what it holds")
        XCTAssertEqual(try page("document.getElementById('hidden').value"), "\"\"", "a hidden input is never the target")

        // Left of the first character: the caret goes before it.
        _ = try run(GuestInput.tap(selector: nil, x: 11, y: 22))
        _ = try run(GuestInput.type(selector: nil, key: nil, text: ">"))
        XCTAssertEqual(try page("document.getElementById('editor').textContent"), "\">hello world!\"", "typed at the caret the press placed")

        _ = try run(GuestInput.type(selector: nil, key: "Meta+s", text: ""))
        _ = try run(GuestInput.type(selector: nil, key: "Shift+A", text: ""))
        XCTAssertEqual(try page("chords"), "[\"s true false\",\"A false true\"]", "the chord's modifiers are the event's")
        XCTAssertEqual(try page("document.getElementById('editor').textContent"), "\">Ahello world!\"", "a printable key inserts; a Meta chord does not")
    }
}
#endif
