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
