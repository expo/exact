#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactDialogs
@testable import ExactNavigation

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

    private func fixture(_ scene: UIWindowScene) throws -> ExactSession {
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
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "a sheet and the system keyboard need the hosted app's window scene")
        }
        let session = try fixture(scene)
        let p = session.presenter
        XCTAssertEqual(p.keyboardPolicy, "resizes-content")
        XCTAssertEqual(try viewport()["keyboardHeight"] as? Double, 0)
        let scale = try XCTUnwrap(window?.screen.scale)
        until("the display's scale is the app's") { (try? self.viewport()["devicePixelRatio"] as? Double) == Double(scale) }
        p.press(try XCTUnwrap(named("open")).id)
        until("the sheet is presented") { self.named("name")?.field?.window != nil && p.modalHost.coordinateView != nil }
        let bar = try XCTUnwrap(named("keys"))
        let field = try XCTUnwrap(named("name")?.field)
        let shown = expectation(forNotification: UIResponder.keyboardDidShowNotification, object: nil)
        field.becomeFirstResponder()
        wait(for: [shown], timeout: 10)
        until("the keyboard's overlap is told") { p.keyboardInset > 100 }
        let top = try XCTUnwrap(p.keyboardTop), sheet = try XCTUnwrap(p.modalHost.coordinateView)
        // On the keys, or at the sheet's own bottom where UIKit floats it above them, as a form sheet in a regular width
        let rest = { min(top, sheet.convert(sheet.bounds, to: nil).maxY - sheet.safeAreaInsets.bottom) }
        until("the toolbar sits on the keys, or on the sheet's bottom above them") { abs(bar.convert(bar.bounds, to: nil).maxY - rest()) < 1 }
        XCTAssertEqual(bar.keyboardLift, 0, "the sheet's layout ends at the keys: nothing to lift")
        until("the app reads the overlap the agent reports") {
            (try? self.viewport()["keyboardHeight"] as? Double).map { abs($0 - Double(p.keyboardInset)) < 0.5 } ?? false
        }
        let hidden = expectation(forNotification: UIResponder.keyboardDidHideNotification, object: nil)
        field.resignFirstResponder()
        wait(for: [hidden], timeout: 10)
        until("the overlap is 0 again") { (try? self.viewport()["keyboardHeight"] as? Double) == 0 }
        until("the toolbar is back at the sheet's bottom") {
            abs(bar.convert(bar.bounds, to: nil).maxY - sheet.convert(sheet.bounds, to: nil).maxY + sheet.safeAreaInsets.bottom) < 1
        }
    }

    /// LLP 1123 D7: a keyboard that rises while UIKit is still presenting the sheet is taken once the presentation
    /// lands. Until then the route keeps its block and nothing in it animates its place or size: laid out inside the
    /// keyboard's animation, the shortened content animated inside a container UIKit was still presenting, which
    /// drew it out of place until the presentation ended. Then the route ends at the keys.
    func testAKeyboardThatRisesWhileTheSheetPresentsIsTakenWhenThePresentationLands() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "a sheet and the system keyboard need the hosted app's window scene")
        }
        let session = try fixture(scene)
        let p = session.presenter
        p.press(try XCTUnwrap(named("open")).id)
        until("the sheet's field is up while UIKit presents it") {
            self.named("name")?.field?.window != nil && p.modalHost.owner?.isBeingPresented == true
        }
        let owner = try XCTUnwrap(p.modalHost.owner), field = try XCTUnwrap(named("name")?.field)
        let route = try XCTUnwrap(p.modalHost.routes.last?.node), container = try XCTUnwrap(route.superview)
        // What the keyboard takes off the route's block: the route short of its container. UIKit settling the sheet's
        // own size moves both, the route a frame behind, a few points; the keyboard takes about 300
        var risen = false, shortened = Set<CGFloat>(), animated = Set<String>()
        let rising = NotificationCenter.default.addObserver(forName: UIResponder.keyboardWillChangeFrameNotification, object: nil, queue: nil) { _ in
            if owner.isBeingPresented { risen = true }
        }
        defer { NotificationCenter.default.removeObserver(rising) }
        XCTAssertTrue(field.becomeFirstResponder())
        // Exact's own views, which its layout places; UIKit's text input animates its cursor views itself
        func motion(in layer: CALayer) {
            if let node = layer.delegate as? NodeView {
                for key in layer.animationKeys() ?? [] where ["position", "bounds", "bounds.size"].contains((layer.animation(forKey: key) as? CAPropertyAnimation)?.keyPath ?? "") {
                    animated.insert("\(node.kind) \(node.id) \(key)")
                }
            }
            layer.sublayers?.forEach(motion)
        }
        until("the presentation lands") {
            if owner.isBeingPresented { shortened.insert((container.bounds.height - route.bounds.height).rounded()); motion(in: route.layer) }
            return !owner.isBeingPresented
        }
        XCTAssertTrue(risen, "the keyboard rose while the sheet was being presented")
        XCTAssertLessThan(shortened.max() ?? 0, 100, "the keyboard takes nothing off the route's block while UIKit presents it")
        XCTAssertEqual(animated, [], "no view of the route's animates its place or size while UIKit presents it")
        until("then the route ends at the keys") {
            guard let top = p.keyboardTop else { return false }
            return abs(route.convert(route.bounds, to: nil).maxY - top) < 1
        }
    }

    /// A dialog sheet's editor: the page is not avoided under it (the sheet
    /// ends at the keys itself), and the app still reads what they cover.
    func testADialogSheetsEditorStillTellsTheKeyboardHeight() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "a sheet and the system keyboard need the hosted app's window scene")
        }
        let session = try fixture(scene)
        let p = session.presenter
        p.press(try XCTUnwrap(named("open-note")).id)
        until("the dialog sheet is presented") { self.named("note")?.field?.window != nil && p.dialogHost.top != nil }
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
