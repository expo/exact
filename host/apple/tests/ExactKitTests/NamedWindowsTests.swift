#if os(macOS)
import AppKit
#endif
import XCTest
@testable import ExactKit

/// LLP 1030.004 D1, D2: a link to one of the app's paths whose `target` is
/// `_blank` or a name goes to the host's window for it, read on every press
/// path; in place without one, as a host with no windows has it. A window
/// opened at a route is rooted there and its page reads its name.
final class NamedWindowsTests: XCTestCase {
    private final class Delegate: ExactSessionDelegate {
        var opened: [String] = []
        func exactSession(_ session: ExactSession, command name: String, args: [Any]) {
            if name == "openURL", let href = args.first as? String { opened.append(href) }
        }
    }

    private func boot(_ delegate: Delegate, window: (name: String, at: String)? = nil) throws -> ExactSession {
        let session = ExactApp.shared.makeSession(delegate: delegate, label: "named-windows")
        if let window { session.launchWindow(name: window.name, at: window.at, rooted: true) }
        XCTAssertNil(session.boot(plan: try fixturePlan("NAMED_WINDOWS"), size: CGSize(width: 390, height: 844)).error)
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, testId)
    }

    private func routes(_ session: ExactSession) -> [String] {
        session.presenter.views.values.compactMap { $0.props["testId"] }.filter { $0.hasPrefix("route-") }.sorted()
    }

    func testATargetOpensTheWindowItNamesOnEveryPressPath() throws {
        let delegate = Delegate()
        let session = try boot(delegate)
        defer { session.destroy() }
        var windows: [[String]] = []
        session.openWindow = { windows.append([$0, $1]); XCTAssertFalse($2) }
        func press(_ testId: String) throws {
            let link = try node(session, testId)
            XCTAssertTrue(link.activateLink(try XCTUnwrap(link.defaultLink)), testId)
        }
        try press("named")
        try press("blank")
        try press("odd")
        try press("button")
        let paragraph = try XCTUnwrap(session.presenter.views.values.first { $0.inlineText.contains { $0.props["testId"] == "inline" } })
        XCTAssertTrue(paragraph.activateInline(try XCTUnwrap(paragraph.inlineText.first { $0.props["testId"] == "inline" }).id))
        XCTAssertEqual(windows, [["/compose", "compose"], ["/compose", "_blank"], ["/compose", "_blank"], ["/settings", "settings"], ["/compose", "compose"]])
        XCTAssertEqual(routes(session), ["route-inbox"], "nothing navigated in place")
        XCTAssertEqual(try node(session, "name").props["text"], "main")

        // A keyword that is the window itself navigates in place.
        try press("top")
        XCTAssertEqual(routes(session), ["route-compose", "route-inbox"])
        XCTAssertEqual(windows.count, 5)
        // A URL that is none of the app's paths leaves the app, whatever its target.
        XCTAssertTrue(session.follow("https://example.com/", target: "compose"))
        XCTAssertEqual(delegate.opened, ["https://example.com/"])
    }

    func testWithoutWindowsATargetNavigatesInPlace() throws {
        let session = try boot(Delegate())
        defer { session.destroy() }
        let link = try node(session, "named")
        XCTAssertTrue(link.activateLink(try XCTUnwrap(link.defaultLink)))
        XCTAssertEqual(routes(session), ["route-compose", "route-inbox"])
    }

    func testPanelLinkRequestsItsOwnRouteAndNamesARefusalWithoutAPresenter() throws {
        let session = try boot(Delegate())
        defer { session.destroy() }
        let link = try node(session, "hud")
        var requests: [[String]] = []
        session.openPanel = { requests.append([$0, $1]) }
        #if os(iOS)
        XCTAssertFalse(link.activateKey(" "))
        XCTAssertTrue(link.activateKey("Enter"))
        #else
        session.presenter.press(link.id)
        #endif
        XCTAssertEqual(requests, [["/compose", "shortcuts"]])
        for name in ["main", "_self", "_blank", "window-32", ""] {
            session.presentRoutePanel(["/compose", name])
        }
        session.presentRoutePanel(["/missing", "shortcuts"])
        XCTAssertEqual(requests.count, 1)
        session.openPanel = nil
        session.presenter.press(link.id)
        XCTAssertTrue(session.agent(#"{"op":"logs"}"#).contains("panel unsupported"))
        XCTAssertEqual(routes(session), ["route-inbox"])
    }

    func testAWindowOpenedAtARouteIsRootedThereAndNamed() throws {
        let session = try boot(Delegate(), window: ("compose", "/compose"))
        defer { session.destroy() }
        XCTAssertEqual(routes(session), ["route-compose"], "the route alone, no ancestor beneath it")
        XCTAssertEqual(try node(session, "name").props["text"], "compose")
        XCTAssertTrue(session.shows("/compose"))
        XCTAssertTrue(session.reroot("/settings"))
        XCTAssertEqual(routes(session), ["route-settings"])
        XCTAssertFalse(session.shows("/compose"))
    }

    #if os(macOS)
    @MainActor
    func testHUDShowsTheRouteAboveItsMaterialAndTakesOnlyItsOwnEscape() throws {
        let session = try boot(Delegate(), window: ("shortcuts", "/compose"))
        defer { session.destroy() }
        let rect = NSRect(x: 0, y: 0, width: 390, height: 270)
        let panel = RoutePanel(contentRect: rect, behind: true)
        let view = ExactView(session: session)
        panel.contentView = view
        XCTAssertEqual(panel.level, .agentBehind)
        XCTAssertTrue(panel.requestedFloating)
        XCTAssertTrue(panel.styleMask.isSuperset(of: [.nonactivatingPanel, .hudWindow]))
        XCTAssertTrue(panel.collectionBehavior.isSuperset(of: [.canJoinAllSpaces, .fullScreenAuxiliary, .canJoinAllApplications]))
        XCTAssertTrue(panel.isExcludedFromWindowsMenu)
        XCTAssertEqual(panel.tabbingMode, .disallowed)
        XCTAssertFalse(panel.canBecomeMain)
        XCTAssertFalse(panel.hidesOnDeactivate)
        XCTAssertTrue(panel.becomesKeyOnlyIfNeeded)
        let root = try node(session, "route-compose")
        XCTAssertTrue(root.standsOnPanelGround)
        XCTAssertNil(root.layer?.backgroundColor.flatMap { $0.alpha > 0 ? $0 : nil })
        root.layoutResize = CGSize(width: 18, height: 12)
        root.applySurface()
        let surface = try XCTUnwrap(root.surface)
        XCTAssertNil(surface.backgroundColor.flatMap { $0.alpha > 0 ? $0 : nil })
        root.layoutResize = .zero
        root.applySurface()
        XCTAssertTrue(view.subviews.first is NSVisualEffectView)
        XCTAssertEqual((view.subviews.first as? NSVisualEffectView)?.material, .hudWindow)
        XCTAssertFalse(session.presenter.viewport.drawsBackground)
        panel.orderFront(nil)
        let escape = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                                                    windowNumber: panel.windowNumber, context: nil, characters: "\u{1b}",
                                                    charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53))
        final class CloseDelegate: NSObject, NSWindowDelegate {
            var attempts = 0
            func windowShouldClose(_ sender: NSWindow) -> Bool { attempts += 1; return attempts > 1 }
        }
        let delegate = CloseDelegate()
        panel.delegate = delegate
        panel.sendEvent(escape)
        XCTAssertEqual(delegate.attempts, 1)
        XCTAssertTrue(panel.isVisible)
        panel.sendEvent(escape)
        XCTAssertFalse(panel.isVisible)
    }

    @MainActor
    func testDriverEditsAPanelFieldWithoutTakingTheKeyboard() throws {
        let session = try boot(Delegate(), window: ("shortcuts", "/compose"))
        let panel = RoutePanel(contentRect: NSRect(x: 10, y: 10, width: 390, height: 270), behind: true)
        panel.contentView = ExactView(session: session)
        panel.orderFront(nil)
        defer { panel.close(); session.destroy() }
        let field = try node(session, "filter")
        let result = Agent(session: session).type(["id": Int(field.id), "text": "driver text"])
        XCTAssertEqual(result["value"] as? String, "driver text")
        XCTAssertEqual(field.field?.stringValue, "driver text")
        XCTAssertFalse(panel.canBecomeKey)
        XCTAssertFalse(panel.isKeyWindow)
    }

    @MainActor
    func testLiveFramesLeavesACoveredPanelAtTheDrivenLevel() throws {
        let session = try boot(Delegate(), window: ("shortcuts", "/compose"))
        let panel = RoutePanel(contentRect: NSRect(x: 10, y: 10, width: 390, height: 270), behind: true)
        panel.contentView = ExactView(session: session)
        panel.orderFront(nil)
        let cover = NSWindow(contentRect: panel.frame, styleMask: [], backing: .buffered, defer: false)
        cover.isReleasedWhenClosed = false
        cover.level = .agentBehind
        cover.order(.above, relativeTo: panel.windowNumber)
        defer { cover.close(); panel.close(); session.destroy() }
        waitFor("the HUD is covered") { !panel.screenOcclusionState.contains(.visible) }
        var reply: [String: Any]?
        Agent(session: session).liveFrames(["live": 100]) { reply = $0 }
        XCTAssertEqual(panel.level, .agentBehind)
        XCTAssertEqual(reply?["reason"] as? String, "panel is covered")
    }

    func testReturnOpensAFocusedPanelLink() throws {
        let session = try boot(Delegate())
        defer { session.destroy() }
        var opened: [String] = []
        session.openPanel = { opened = [$0, $1] }
        let key = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                                                windowNumber: 0, context: nil, characters: "\r",
                                                charactersIgnoringModifiers: "\r", isARepeat: false, keyCode: 36))
        let opener = try node(session, "hud")
        XCTAssertEqual(opener.accessibilityURL()?.relativeString, "/compose")
        opener.keyDown(with: key)
        XCTAssertEqual(opened, ["/compose", "shortcuts"])
    }

    /// D5: Settings…, a `Meta+,` link with no `target`, asks for the window
    /// named `settings` as it stands; pressed as a link where the host opens
    /// no windows.
    @MainActor
    func testSettingsOpensTheSettingsWindow() throws {
        let session = try boot(Delegate())
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 390, height: 844), styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = session.presenter.root
        defer { window.close(); session.destroy() }
        let bar = DevMenu.makeMenu(shortcuts: session.presenter.shortcuts, documents: false, develop: false)
        let app = try XCTUnwrap(bar.items.first?.submenu)
        session.presenter.shortcuts.sync()
        let settings = try XCTUnwrap(app.items.first { $0.title == "Settings…" }, "\(app.items.map(\.title))")
        var windows: [[String]] = []
        session.openWindow = { windows.append([$0, $1, "\($2)"]) }
        app.performActionForItem(at: app.index(of: settings))
        XCTAssertEqual(windows, [["/settings", "settings", "true"]])
        XCTAssertEqual(routes(session), ["route-inbox"])
        session.openWindow = nil
        app.performActionForItem(at: app.index(of: settings))
        XCTAssertEqual(routes(session), ["route-settings"])
    }
    #endif
}
