#if os(macOS)
import AppKit
import WebKit
import XCTest
@testable import ExactKit

final class AgentWebWindowMacTests: XCTestCase {
    nonisolated(unsafe) private static var pages: FramePages?

    override class func setUp() {
        super.setUp()
        NSApplication.shared.setActivationPolicy(.accessory)
    }

    func testACoveredGuestRunsAndPaintsWithoutRaisingItsWindow() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "behind-web")
        defer { session.destroy() }
        let rect = NSRect(x: 10, y: 10, width: 640, height: 500)
        let window = ExactAgentWindow(contentRect: rect, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.level = .agentBehind
        defer { window.close() }
        let web = try mountGuest(session, in: window)
        window.orderFront(nil)
        let cover = NSWindow(contentRect: window.frame, styleMask: [], backing: .buffered, defer: false)
        cover.isReleasedWhenClosed = false
        cover.level = .agentBehind
        cover.order(.above, relativeTo: window.windowNumber)
        defer { cover.close() }
        waitForMainQueue()
        waitFor("the window is physically covered") { !window.screenOcclusionState.contains(.visible) }
        XCTAssertTrue(window.occlusionState.contains(.visible), "WebKit sees a live page in the covered window")
        XCTAssertEqual(window.level, .agentBehind, "mounting a guest must leave the driven window behind other apps")
        try runFramesAndTimers(web)
        XCTAssertEqual(window.level, .agentBehind)
        let captured = expectation(description: "guest painted")
        var image: NSImage?
        web.takeSnapshot(with: nil) { picture, error in
            XCTAssertNil(error)
            image = picture
            captured.fulfill()
        }
        wait(for: [captured], timeout: 10)
        let bitmap = try XCTUnwrap(NSBitmapImageRep(data: try XCTUnwrap(image?.tiffRepresentation)))
        let pixel = try XCTUnwrap(bitmap.colorAt(x: bitmap.pixelsWide / 2, y: bitmap.pixelsHigh / 2)?.usingColorSpace(.deviceRGB))
        XCTAssertGreaterThan(pixel.greenComponent, 0.9)
        XCTAssertLessThan(pixel.redComponent, 0.1)
        let windows = try XCTUnwrap(CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]])
        let guestIndex = try XCTUnwrap(windows.firstIndex { ($0[kCGWindowNumber as String] as? Int) == window.windowNumber })
        let coverIndex = try XCTUnwrap(windows.firstIndex { ($0[kCGWindowNumber as String] as? Int) == cover.windowNumber })
        XCTAssertGreaterThan(guestIndex, coverIndex, "the guest stays below the window covering it")
        XCTAssertEqual(windows[guestIndex][kCGWindowLayer as String] as? Int, -1)
        window.orderOut(nil)
        XCTAssertFalse(window.occlusionState.contains(.visible), "an ordered-out window remains hidden")
    }

    func testAnOrdinaryEmbeddingWindowKeepsItsGuestLiveAndRestoresItsLevel() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "host-owned-web")
        defer { session.destroy() }
        let rect = NSRect(x: 10, y: 10, width: 640, height: 500)
        let window = NSWindow(contentRect: rect, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.level = .agentBehind
        window.orderFront(nil)
        defer { window.close() }
        let cover = NSWindow(contentRect: window.frame, styleMask: [], backing: .buffered, defer: false)
        cover.isReleasedWhenClosed = false
        cover.level = .agentBehind
        cover.order(.above, relativeTo: window.windowNumber)
        defer { cover.close() }
        waitFor("the embedding window is covered") { !window.screenOcclusionState.contains(.visible) }
        let web = try mountGuest(session, in: window)
        waitForMainQueue()
        XCTAssertEqual(window.level, .normal, "a host-owned window retains the guest-liveness fallback")
        defer {
            session.webviews.reset()
            XCTAssertEqual(window.level, .agentBehind, "removing the last guest restores the host's driven level")
        }
        guard window.level == .normal else { return }
        // A locked desktop occludes an ordinary window even after the fallback lifts it.
        waitFor("the lifted embedding window is physically visible", cap: 3, onFailure: { _ in }) {
            window.screenOcclusionState.contains(.visible)
        }
        guard window.screenOcclusionState.contains(.visible) else {
            throw missing("screen", "the ordinary embedding window remains covered after it is raised; unlock the Mac desktop")
        }
        try runFramesAndTimers(web)
    }

    func testACoveredSheetKeepsItsGuestLiveAtTheDrivenLevel() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "sheet-web")
        defer { session.destroy() }
        let rect = NSRect(x: 10, y: 10, width: 640, height: 500)
        let parent = ExactAgentWindow(contentRect: rect, styleMask: [.titled], backing: .buffered, defer: false)
        parent.isReleasedWhenClosed = false
        parent.level = .agentBehind
        parent.orderFront(nil)
        defer { parent.close() }
        let sheet = SheetWindow(contentRect: rect, styleMask: [.titled], backing: .buffered, defer: false)
        sheet.isReleasedWhenClosed = false
        defer { parent.endSheet(sheet); sheet.close() }
        parent.beginSheet(sheet)
        waitFor("the attached sheet") { sheet.isVisible && sheet.sheetParent === parent }
        let level = sheet.level
        let web = try mountGuest(session, in: sheet)
        let cover = NSWindow(contentRect: sheet.frame, styleMask: [], backing: .buffered, defer: false)
        cover.isReleasedWhenClosed = false
        cover.level = .agentBehind
        cover.order(.above, relativeTo: sheet.windowNumber)
        defer { cover.close() }
        waitFor("the sheet is physically covered") { !sheet.screenOcclusionState.contains(.visible) }
        XCTAssertTrue(sheet.occlusionState.contains(.visible), "WebKit sees a live page in the covered sheet")
        try runFramesAndTimers(web)
        XCTAssertEqual(parent.level, .agentBehind)
        XCTAssertEqual(sheet.level, level, "mounting a guest leaves the sheet's level unchanged")
        let windows = try XCTUnwrap(CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]])
        let sheetIndex = try XCTUnwrap(windows.firstIndex { ($0[kCGWindowNumber as String] as? Int) == sheet.windowNumber })
        let coverIndex = try XCTUnwrap(windows.firstIndex { ($0[kCGWindowNumber as String] as? Int) == cover.windowNumber })
        XCTAssertGreaterThan(sheetIndex, coverIndex)
        XCTAssertEqual(windows[sheetIndex][kCGWindowLayer as String] as? Int, -1, "the sheet stays at the driven window-server level")
        sheet.orderOut(nil)
        XCTAssertEqual(sheet.occlusionState, sheet.screenOcclusionState)
    }

    func testOnlyAnOnscreenWindowAtTheDrivenLevelReportsVisible() {
        _ = NSApplication.shared
        let window = ExactAgentWindow(contentRect: NSRect(x: 10, y: 10, width: 300, height: 200),
                                      styleMask: [.titled, .miniaturizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        window.level = .agentBehind
        XCTAssertEqual(window.occlusionState, window.screenOcclusionState)
        window.orderFront(nil)
        XCTAssertTrue(window.occlusionState.contains(.visible))
        window.miniaturize(nil)
        waitFor("the window minimized") { window.isMiniaturized }
        XCTAssertEqual(window.occlusionState, window.screenOcclusionState, "a minimized window uses AppKit's own state")
        window.deminiaturize(nil)
        waitFor("the window restored") { window.isVisible && !window.isMiniaturized }
        XCTAssertTrue(window.occlusionState.contains(.visible))
        window.orderOut(nil)
        window.level = .normal
        XCTAssertEqual(window.occlusionState, window.screenOcclusionState)
    }

    private func mountGuest(_ session: ExactSession, in window: NSWindow) throws -> WKWebView {
        XCTAssertTrue(Agent.windowsBehind, "this regression must run in frozen agent mode before mounting a guest")
        let module = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_TEST_WEB_MODULE"])
        if Self.pages == nil { Self.pages = try FramePages() }
        session.webviews.moduleLocation = { module }
        XCTAssertTrue(session.openIncoming("/?origin=http://127.0.0.1:\(Self.pages!.port)"))
        window.contentView = ExactView(session: session)
        XCTAssertNil(session.boot(plan: try fixturePlan("IFRAME_PARTITION"), size: window.contentView!.bounds.size).error)
        let guest = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "guest" })
        let web = try XCTUnwrap(guest.web as? WKWebView)
        waitFor("the guest loaded") { web.url?.path == "/a" && !web.isLoading }
        return web
    }

    private func runFramesAndTimers(_ web: WKWebView) throws {
        let ran = expectation(description: "animation frames and unclamped timers")
        var result: Result<Any, Error>?
        web.callAsyncJavaScript("""
        document.body.style.background = 'rgb(255,0,0)';
        let frames = 0, timers = 0;
        await Promise.all([
          new Promise(resolve => {
            function frame() { if (++frames === 12) resolve(); else requestAnimationFrame(frame); }
            requestAnimationFrame(frame);
          }),
          new Promise(resolve => {
            const timer = setInterval(() => { if (++timers === 12) { clearInterval(timer); resolve(); } }, 20);
          })
        ]);
        document.body.style.background = 'rgb(0,255,0)';
        await new Promise(requestAnimationFrame);
        return document.visibilityState;
        """, arguments: [:], in: nil, in: .page) { answer in result = answer; ran.fulfill() }
        wait(for: [ran], timeout: 8)
        XCTAssertEqual(try XCTUnwrap(result).get() as? String, "visible")
    }
}
#endif
