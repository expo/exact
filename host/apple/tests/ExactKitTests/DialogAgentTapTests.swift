#if os(macOS) || os(iOS)
import XCTest
@testable import ExactKit
#if os(iOS)
import UIKit
@testable import ExactDialogs
#else
import AppKit
#endif

final class DialogAgentTapTests: XCTestCase {
    #if os(iOS)
    private lazy var scene = hostedWindowScene()
    #endif

    func testPageTapBehindSheet() throws {
        #if os(iOS)
        guard scene != nil else { throw missing("scene", "the dialog needs a window to present in") }
        #endif
        try exercise("sheet")
    }
    func testPageTapBehindPaintedDialog() throws {
        #if os(iOS)
        guard scene != nil else { throw missing("scene", "the dialog needs a window to present in") }
        #endif
        try exercise("painted")
    }

    private func exercise(_ shape: String) throws {
        let session = ExactApp.shared.makeSession(label: "dialog-agent-tap")
        let p = session.presenter, view = ExactView(session: session)
        #if os(iOS)
        let scene = try XCTUnwrap(scene)
        let window = UIWindow(windowScene: scene), host = UIViewController()
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        view.frame = host.view.bounds
        defer { window.isHidden = true }
        #else
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 500, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = view
        window.makeKeyAndOrderFront(nil)
        p.dialogs.painted = shape == "painted"
        defer { window.close() }
        #endif
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("DIALOG_SHEET_READS"), size: view.bounds.size).error)
        func node(_ name: String) throws -> NodeView {
            try XCTUnwrap(p.views.values.first { $0.props["id"] == name }, name)
        }
        func events() throws -> String {
            let state = session.runtime.agent(#"{"op":"state"}"#)
            let reply = try JSONSerialization.jsonObject(with: Data(state.utf8)) as? [String: Any]
            return try XCTUnwrap((reply?["slots"] as? [String: Any])?["taps"] as? String)
        }
        let agent = Agent(session: session), page = try node("page-button"), dialog = try node("tap-\(shape)")
        let pageTap = ["id": Int(page.id)]
        p.dialogCommand("showModal", "tap-\(shape)")
        #if os(iOS)
        waitFor("dialog presentation") { !p.menus.inTransition && host.presentedViewController != nil }
        let controller = try XCTUnwrap(host.presentedViewController as? DialogSheetController)
        XCTAssertEqual(controller.topLayer, shape == "painted")
        let field = try XCTUnwrap(try node("\(shape)-field").field)
        XCTAssertTrue(field.becomeFirstResponder())
        #else
        let front = try XCTUnwrap(dialog.window)
        XCTAssertEqual(front === window, shape == "painted")
        let field = try XCTUnwrap(try node("\(shape)-field").field)
        XCTAssertTrue(front.makeFirstResponder(field))
        let editor = try XCTUnwrap(field.currentEditor()), pageFocus = window.firstResponder
        #endif
        #if os(iOS)
        waitFor("the test window can receive touches") { Agent.frontRefusal(window) == nil }
        func aim(_ target: NodeView) -> [String: Any] {
            agent.tap(["id": Int(target.id), "aim": ["press": true]])
        }
        let refusedAim = aim(page)
        XCTAssertTrue((refusedAim["error"] as? String)?.contains("inert") == true, "\(refusedAim)")
        #endif
        let refusal = agent.tap(pageTap)
        XCTAssertTrue((refusal["error"] as? String)?.contains("inert") == true, "\(refusal)")
        waitForMainQueue()
        XCTAssertEqual(try events(), "")
        #if os(iOS)
        XCTAssertTrue(field.isFirstResponder)
        XCTAssertTrue(p.dialogHost.isOpen(dialog))
        #else
        XCTAssertTrue(front.firstResponder === editor)
        XCTAssertTrue(window.firstResponder === pageFocus)
        XCTAssertTrue(p.dialogs.active === dialog)
        #endif
        let save = try node("\(shape)-save")
        #if os(iOS)
        let saveAim = aim(save)
        XCTAssertNotNil(saveAim["aim"], "\(saveAim)")
        #endif
        let saved = agent.tap(["id": Int(save.id)])
        XCTAssertNil(saved["error"], "\(saved)")
        #if os(iOS)
        waitFor("dialog dismissal") { !p.menus.inTransition && host.presentedViewController == nil }
        #else
        XCTAssertNil(p.dialogs.active)
        #endif
        waitForMainQueue()
        XCTAssertEqual(try events(), "saveclose")
        #if os(iOS)
        let pageAim = aim(page)
        XCTAssertNotNil(pageAim["aim"], "\(pageAim)")
        #endif
        let tapped = agent.tap(pageTap)
        XCTAssertNil(tapped["error"], "\(tapped)")
        XCTAssertEqual(try events(), "saveclosepage")
    }
}
#endif
