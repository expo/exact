// The dev loop's status pill (DevPill.swift; dtool2's pass 3 #8, theme2's
// plan approval): while the loop's producers start the window shows what it
// said, in a pill outside the page and the kernel's tree; it is gone when the
// plan comes; under the agent there is none.
import XCTest
@testable import ExactKit
#if os(macOS)
import AppKit
#else
import UIKit
#endif

final class DevPillTests: XCTestCase {
    #if os(macOS)
    private var fixture: SheetFixture?
    override func tearDown() { fixture?.close(); fixture = nil; super.tearDown() }

    func testThePillSaysTheLoopsStatusAndGoesWhenThePlanComes() throws {
        let f = try SheetFixture()
        fixture = f
        f.window.orderFrontRegardless()
        f.settle()
        let nodes = f.presenter.views.count
        f.presenter.showDevStatus("Dev loop: building the module producer…")
        let pill = f.presenter.devPill as? NSTextField
        if ExactEnv.agentMode { XCTAssertNil(pill, "never under the agent"); return }
        XCTAssertEqual(pill?.stringValue, "Dev loop: building the module producer…")
        XCTAssertTrue(pill?.superview === f.window.contentView, "outside the page")
        XCTAssertFalse(pill.map { $0.isDescendant(of: f.presenter.viewport) } ?? true)
        XCTAssertEqual(f.presenter.views.count, nodes, "no node of the page's")
        f.presenter.showDevStatus("Dev loop: building the GPU tools…")
        XCTAssertTrue(f.presenter.devPill === pill, "one pill, its text replaced")
        f.presenter.showDevStatus(nil)
        XCTAssertNil(f.presenter.devPill)
        XCTAssertNil(pill?.superview, "gone when the plan comes")
    }

    /// One pill a window, and when the session that shows it leaves, another
    /// session in that window takes it (no new status needed).
    func testASecondSessionInTheWindowTakesThePillWhenTheFirstLeaves() throws {
        if ExactEnv.agentMode || ExactEnv.releaseBuild { throw XCTSkip("no pill under the agent or in a release build") }
        let f = try SheetFixture()
        fixture = f
        f.window.orderFrontRegardless()
        f.settle()
        let other = ExactApp.shared.makeSession(label: "dev-pill-second")
        let view = ExactView(session: other)
        view.frame = NSRect(x: 0, y: 0, width: 100, height: 100)
        try XCTUnwrap(f.window.contentView).addSubview(view)
        defer { ExactApp.shared.showDevStatus(nil); other.destroy() }
        ExactApp.shared.showDevStatus(.loop(.preparing("building the module producer")))
        let pills = { f.window.contentView?.subviews.filter { $0 === f.presenter.devPill || $0 === other.presenter.devPill } ?? [] }
        XCTAssertEqual(pills().count, 1, "one pill a window")
        let owner: ExactSession = f.presenter.devPill != nil ? f.session : other
        let heir: ExactSession = owner === other ? f.session : other
        owner.destroy()
        XCTAssertNotNil(heir.presenter.devPill, "the other session takes it")
        XCTAssertEqual((heir.presenter.devPill as? NSTextField)?.stringValue, "Dev loop: building the module producer…")
    }

    /// What the connection says with the last good plan on screen (LLP
    /// 1159.001 D2): a save that did not apply, and a host to rebuild.
    func testARefusedSaveAndAStaleHostAreSaidInThePill() throws {
        if ExactEnv.agentMode || ExactEnv.releaseBuild { throw XCTSkip("no pill under the agent or in a release build") }
        let f = try SheetFixture()
        fixture = f
        f.window.orderFrontRegardless()
        f.settle()
        defer { ExactApp.shared.showDevStatus(nil) }
        let said = { (f.presenter.devPill as? NSTextField)?.stringValue }
        ExactApp.shared.showDevStatus(.refused("app.contract:41:7 unknown attribute `colour`"))
        XCTAssertEqual(said(), "Edit not applied: app.contract:41:7 unknown attribute `colour`")
        ExactApp.shared.showDevStatus(.stale)
        XCTAssertEqual(said(), "Rebuild this app: its native code changed")
        ExactApp.shared.showDevStatus(nil)
        XCTAssertNil(f.presenter.devPill)
    }

    /// Through the app's own connection: a loop still starting puts its
    /// step in the window's pill, and the envelope that follows takes it away.
    func testTheAppsConnectionShowsTheLoopsStepUntilItsEnvelope() throws {
        if ExactEnv.agentMode || ExactEnv.releaseBuild { throw XCTSkip("no pill under the agent or in a release build") }
        let f = try SheetFixture()
        fixture = f
        f.window.orderFrontRegardless()
        f.settle()
        let server = try DevServerStub()
        defer { server.stop(); ExactApp.shared.disconnect() }
        try server.start(program: String(repeating: "a", count: 64), plan: Data("plan one".utf8))
        server.status = ("503 Service Unavailable", #"{"exact":1,"status":"preparing","message":"building the module producer"}"#)
        ExactApp.shared.connect(server.page)
        let shown = { (f.presenter.devPill as? NSTextField)?.stringValue }
        let until = { (done: () -> Bool) in
            let end = Date(timeIntervalSinceNow: 20)
            while !done() && Date() < end { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01)) }
        }
        until { shown() == "Dev loop: building the module producer…" }
        XCTAssertEqual(shown(), "Dev loop: building the module producer…")
        XCTAssertEqual(ExactApp.shared.connectionStatus, "\(server.page) (dev loop: building the module producer)")
        server.status = nil
        until { f.presenter.devPill == nil }
        XCTAssertNil(f.presenter.devPill, "gone when the envelope comes")
    }
    #else
    private var window: UIWindow?
    private var session: ExactSession?
    override func tearDown() { session?.destroy(); session = nil; window?.isHidden = true; window = nil; super.tearDown() }

    private func makeWindow(_ scene: UIWindowScene) -> (ExactSession, UIWindow) {
        let session = ExactApp.shared.makeSession(label: "dev-pill")
        self.session = session
        let window = UIWindow(windowScene: scene), host = UIViewController()
        window.frame = CGRect(x: 0, y: 0, width: 390, height: 844)
        window.rootViewController = host
        window.makeKeyAndVisible()
        self.window = window
        let view = ExactView(session: session)
        host.view.addSubview(view)
        view.frame = host.view.bounds
        return (session, window)
    }

    func testThePillSaysTheLoopsStatusAndGoesWhenThePlanComes() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else { throw missing("scene", "the pill is in a scene's window") }
        let (session, window) = makeWindow(scene)
        session.presenter.showDevStatus("Dev loop: building the module producer…")
        let pill = session.presenter.devPill as? UILabel
        if ExactEnv.agentMode { XCTAssertNil(pill, "never under the agent"); return }
        XCTAssertEqual(pill?.text?.trimmingCharacters(in: .whitespaces), "Dev loop: building the module producer…")
        XCTAssertTrue(pill?.superview === window, "outside the page")
        XCTAssertLessThanOrEqual(pill?.frame.maxY ?? .infinity, window.bounds.maxY - window.safeAreaInsets.bottom, "above the safe area")
        session.presenter.showDevStatus(nil)
        XCTAssertNil(session.presenter.devPill)
        XCTAssertNil(pill?.superview, "gone when the plan comes")
    }
    #endif

    func testOverrunStaysInItsWindowUntilTheNextInput() throws {
        if ExactEnv.agentMode || ExactEnv.releaseBuild { throw XCTSkip("development window required") }
        #if os(macOS)
        let f = try SheetFixture()
        fixture = f
        f.window.orderFrontRegardless()
        f.settle()
        let session = f.session
        let said = { (session.presenter.devPill as? NSTextField)?.stringValue }
        #else
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else { throw missing("scene", "the pill is in a scene's window") }
        let (session, _) = makeWindow(scene)
        let said = { (session.presenter.devPill as? UILabel)?.text?.trimmingCharacters(in: .whitespaces) }
        #endif
        let warning = "mutation order failed: signOrder took 150 ms of CPU time over its 100 ms turn budget"
        let data = try JSONSerialization.data(withJSONObject: ["ops": [], "budgetWarning": warning] as [String: Any])
        session.apply(Batch.decode(data))
        XCTAssertEqual(said(), warning)
        #if os(macOS)
        XCTAssertEqual((session.presenter.devPill as? NSTextField)?.maximumNumberOfLines, 0)
        #else
        XCTAssertEqual((session.presenter.devPill as? UILabel)?.numberOfLines, 0)
        #endif
        defer { ExactApp.shared.showDevStatus(nil) }
        ExactApp.shared.showDevStatus(.stale)
        XCTAssertEqual(said(), "Rebuild this app: its native code changed")
        ExactApp.shared.showDevStatus(nil)
        XCTAssertEqual(said(), warning)
        session.applyInput(wireBatch([]))
        XCTAssertNil(session.presenter.devPill)
    }

    /// `EXACT_DEV_PILL=0` (ExactEnv.devPill): no pill is drawn, but the session keeps the budget warning and the app its
    /// status, so turning the pill back on shows both as they would have been.
    func testWithoutThePillTheWarningAndStatusAreKept() throws {
        if ExactEnv.agentMode || ExactEnv.releaseBuild { throw XCTSkip("development window required") }
        #if os(macOS)
        let f = try SheetFixture()
        fixture = f
        f.window.orderFrontRegardless()
        f.settle()
        let session = f.session
        let said = { (session.presenter.devPill as? NSTextField)?.stringValue }
        #else
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else { throw missing("scene", "the pill is in a scene's window") }
        let (session, _) = makeWindow(scene)
        let said = { (session.presenter.devPill as? UILabel)?.text?.trimmingCharacters(in: .whitespaces) }
        #endif
        let was = ExactEnv.devPill
        ExactEnv.devPill = false
        defer { ExactEnv.devPill = was; ExactApp.shared.showDevStatus(nil) }
        let warning = "mutation order failed: signOrder took 150 ms of CPU time over its 100 ms turn budget"
        let data = try JSONSerialization.data(withJSONObject: ["ops": [], "budgetWarning": warning] as [String: Any])
        session.apply(Batch.decode(data))
        XCTAssertNil(session.presenter.devPill, "no pill for the warning")
        XCTAssertEqual(session.budgetWarning, warning, "the warning is still the session's")
        ExactApp.shared.showDevStatus(.stale)
        XCTAssertNil(session.presenter.devPill, "no pill for the status")
        XCTAssertEqual(ExactApp.devStatusText, "Rebuild this app: its native code changed")
        ExactEnv.devPill = true
        ExactApp.shared.refreshDevPills()
        XCTAssertEqual(said(), "Rebuild this app: its native code changed")
        ExactApp.shared.showDevStatus(nil)
        XCTAssertEqual(said(), warning)
    }
}
