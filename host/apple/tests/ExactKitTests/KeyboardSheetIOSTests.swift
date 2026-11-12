#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1039 D9 and LLP 1008 §9.1 (expo/exact#110): a sheet route's keyboard
/// toolbar sits on the keys under the root's default `resizes-content`, and
/// `exactViewport().keyboardHeight` is the overlap the presenter reads
/// from the keyboard's layout guide, told with the view and back to 0.
final class KeyboardSheetIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?

    override func tearDown() {
        session?.destroy()
        session = nil
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    private func until(_ what: String, _ seconds: Double = 10, file: StaticString = #filePath, line: UInt = #line, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what, file: file, line: line)
    }

    private func named(_ id: String) -> NodeView? { session?.presenter.views.values.first { $0.props["testId"] == id } }

    /// The app's own copy of the viewport fact.
    private func viewport() throws -> [String: Any] {
        let session = try XCTUnwrap(session)
        let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.runtime.agent("{\"op\":\"state\"}").utf8)) as? [String: Any])
        return try XCTUnwrap((state["resources"] as? [String: Any])?["viewport"] as? [String: Any])
    }

    private func fixture() throws -> ExactSession {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "a sheet and the system keyboard need the hosted app's window scene")
        }
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_KEYBOARD_SHEET_PLAN"], "build.mjs --test --ios compiles keyboard-sheet.contract")
        let session = ExactApp.shared.makeSession(label: "keyboard-sheet")
        self.session = session
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = window.screen.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        view.frame = host.view.bounds
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: view.bounds.size).error)
        view.layoutIfNeeded()
        until("the home route is up") { self.named("open")?.window != nil }
        return session
    }

    func testASheetsKeyboardToolbarSitsOnTheKeysAndTheOverlapIsAViewportFact() throws {
        let session = try fixture()
        let p = session.presenter
        XCTAssertEqual(p.keyboardPolicy, "resizes-content")
        XCTAssertEqual(try viewport()["keyboardHeight"] as? Double, 0)
        let scale = try XCTUnwrap(window?.screen.scale)
        until("the display's scale is the app's") { (try? self.viewport()["devicePixelRatio"] as? Double) == Double(scale) }
        p.press(try XCTUnwrap(named("open")).id)
        until("the sheet is presented") { self.named("name")?.field?.window != nil && p.modals.coordinateView != nil }
        let bar = try XCTUnwrap(named("keys"))
        let field = try XCTUnwrap(named("name")?.field)
        let shown = expectation(forNotification: UIResponder.keyboardDidShowNotification, object: nil)
        field.becomeFirstResponder()
        wait(for: [shown], timeout: 10)
        until("the keyboard's overlap is told") { p.keyboardInset > 100 }
        let top = try XCTUnwrap(p.keyboardTop)
        until("the toolbar sits on the keys") { abs(bar.convert(bar.bounds, to: nil).maxY - top) < 1 }
        XCTAssertEqual(bar.keyboardLift, 0, "the sheet's layout ends at the keys: nothing to lift")
        until("the app reads the overlap the agent reports") {
            (try? self.viewport()["keyboardHeight"] as? Double).map { abs($0 - Double(p.keyboardInset)) < 0.5 } ?? false
        }
        let hidden = expectation(forNotification: UIResponder.keyboardDidHideNotification, object: nil)
        field.resignFirstResponder()
        wait(for: [hidden], timeout: 10)
        until("the overlap is 0 again") { (try? self.viewport()["keyboardHeight"] as? Double) == 0 }
        let sheet = try XCTUnwrap(p.modals.coordinateView)
        until("the toolbar is back at the sheet's bottom") {
            abs(bar.convert(bar.bounds, to: nil).maxY - sheet.convert(sheet.bounds, to: nil).maxY + sheet.safeAreaInsets.bottom) < 1
        }
    }

    /// A dialog sheet's editor: the page is not avoided under it (the sheet
    /// ends at the keys itself), and the app still reads what they cover.
    func testADialogSheetsEditorStillTellsTheKeyboardHeight() throws {
        let session = try fixture()
        let p = session.presenter
        p.press(try XCTUnwrap(named("open-note")).id)
        until("the dialog sheet is presented") { self.named("note")?.field?.window != nil && p.menus.sheets.top != nil }
        let field = try XCTUnwrap(named("note")?.field)
        let shown = expectation(forNotification: UIResponder.keyboardDidShowNotification, object: nil)
        field.becomeFirstResponder()
        wait(for: [shown], timeout: 10)
        until("the app reads the keys under a dialog sheet") {
            ((try? self.viewport()["keyboardHeight"] as? Double) ?? 0) > 100
        }
        XCTAssertEqual(p.keyboardInset, 0, "the page under a dialog sheet is not avoided")
        let hidden = expectation(forNotification: UIResponder.keyboardDidHideNotification, object: nil)
        field.resignFirstResponder()
        wait(for: [hidden], timeout: 10)
        until("the overlap is 0 again") { (try? self.viewport()["keyboardHeight"] as? Double) == 0 }
    }
}
#endif
