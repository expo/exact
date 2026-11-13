#if os(macOS)
import AppKit
import ExactTips
import TipKit
import XCTest
@testable import ExactKit

/// LLP 1035.006.000 over `scripts/fixtures/tips.contract` on the Mac: a tip
/// is TipKit's popover, over the toolbar item the window shows for a
/// header's button and over a button in the page; one shows at a time, in
/// document order; pressing the button (`tipDone`) retires its tip for good,
/// and a session made again shows neither.
final class TipsMacTests: XCTestCase {
    private var installed: TipCapability?
    private var windows: [NSWindow] = []
    private var sessions: [ExactSession] = []

    override func setUp() {
        super.setUp()
        installed = TipLink.installed
        // What an earlier run retired is in TipKit's store until it is reset,
        // which TipKit allows only before it is configured.
        try? Tips.resetDatastore()
        ExactTips.install()
    }

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        for window in windows { window.close() }
        windows = []
        TipLink.installed = installed
        super.tearDown()
    }

    private func boot(_ label: String) throws -> ExactSession {
        _ = NSApplication.shared
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_TIPS_PLAN"], "build.mjs --test compiles the tips fixture")
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        let view = ExactView(session: session)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 640, height: 480), styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        windows.append(window)
        let mount = NSView(frame: NSRect(x: 0, y: 0, width: 640, height: 480))
        window.contentView = mount
        view.frame = mount.bounds; mount.addSubview(view)
        window.orderFrontRegardless()
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: CGSize(width: 640, height: 480)).error)
        // The window's owner gives the app its toolbar, as the Mac app's does.
        XCTAssertTrue(view.attachWindowToolbar(to: window))
        window.layoutIfNeeded()
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    private func tips(_ session: ExactSession) -> [String: [String: Any]] {
        let rows = TipLink.installed?.state(owner: session.presenter) ?? []
        return Dictionary(uniqueKeysWithValues: rows.map { ($0["id"] as? String ?? "", $0) })
    }

    private func shows(_ session: ExactSession, _ id: String, over: String) -> Bool {
        tips(session)[id]?["shows"] as? Bool == true && tips(session)[id]?["over"] as? String == over
    }

    /// A TipKit popover's window, on screen.
    private func popoverWindows() -> [NSWindow] {
        NSApp.windows.filter { $0.isVisible && String(describing: type(of: $0)).contains("Popover") }
    }

    /// A session destroyed with its tip showing closes the popover and leaves
    /// nothing of itself in the process's Tips host.
    func testADestroyedSessionClosesItsTipAndKeepsNothing() throws {
        let session = try boot("tips-mac-destroyed")
        let presenter = session.presenter
        waitFor("the Filter tip shows") { self.shows(session, "filter-tip", over: "bar item") }
        session.destroy()
        sessions.removeAll { $0 === session }
        XCTAssertTrue(TipLink.installed?.state(owner: presenter).isEmpty == true, "no tip kept for the destroyed session")
        waitFor("its popover closed") { self.popoverWindows().isEmpty }
    }

    func testATipShowsOverItsToolbarItemThenItsElementAndRetiresWhenDone() throws {
        let session = try boot("tips-mac")
        let filter = try node(session, "filter"), add = try node(session, "add")
        XCTAssertNotNil(session.presenter.toolbar.tipItem(filter), "the header's button is a toolbar item")

        waitFor("the Filter tip over its toolbar item") { self.shows(session, "filter-tip", over: "bar item") }
        XCTAssertFalse(popoverWindows().isEmpty, "TipKit's popover is on screen")
        XCTAssertEqual(tips(session)["add-tip"]?["shows"] as? Bool, false, "one tip at a time, in document order")

        // Pressing Filter is what its tip teaches: it is retired, and Add's shows.
        _ = Agent(session: session).tap(["id": Int(filter.id)])
        waitFor("the Add tip over its button") { self.shows(session, "add-tip", over: "element") }
        // TipKit records the invalidation in its own time; the next tip shows at once.
        waitFor("TipKit retires the Filter tip") { self.tips(session)["filter-tip"]?["status"] as? String == "invalidated" }
        XCTAssertEqual(tips(session)["filter-tip"]?["reason"] as? String, "actionPerformed")

        _ = Agent(session: session).tap(["id": Int(add.id)])
        waitFor("no tip shows once both are done") {
            self.tips(session).values.allSatisfy { $0["shows"] as? Bool == false && $0["status"] as? String == "invalidated" }
        }

        // TipKit remembers: the app made again shows neither.
        let again = try boot("tips-mac-again")
        waitFor("the tips read back retired") { self.tips(again).count == 2 && self.tips(again).values.allSatisfy { $0["status"] as? String == "invalidated" } }
        waitForMainQueue()
        XCTAssertTrue(tips(again).values.allSatisfy { $0["shows"] as? Bool == false }, "a retired tip does not show again")
    }
}
#endif
