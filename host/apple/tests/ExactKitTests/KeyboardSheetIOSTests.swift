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

    /// LLP 1123 D2 and D7: while UIKit presents a sheet, everything in it is drawn where it is laid out. The form is
    /// Shelf's Add Book: its field takes focus as the sheet opens, so the keyboard rises during the presentation and
    /// shortens the route from its bottom, and its tab list is a segmented control. Laid out inside UIKit's setup of
    /// the presentation, that control's animation runs UIKit's layout of the sheet early, which holds the list at the
    /// size it had then until the animation ends, so the content is drawn off its place and then jumps. Then the route
    /// ends at the keys.
    func testASheetsContentIsDrawnWhereItIsLaidOutWhileUIKitPresentsIt() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "a sheet and the system keyboard need the hosted app's window scene")
        }
        let session = try fixture(scene)
        let p = session.presenter
        var risen = false
        let rising = NotificationCenter.default.addObserver(forName: UIResponder.keyboardWillChangeFrameNotification, object: nil, queue: nil) { _ in
            if p.modalHost.owner?.isBeingPresented == true { risen = true }
        }
        defer { NotificationCenter.default.removeObserver(rising) }
        // Each frame the sheet is on screen while UIKit presents it: where each of the route's node views is drawn
        // against the drawn sheet, to compare with where it is laid out once the presentation lands
        var drawn: [(frame: Int, id: UInt32, at: CGPoint)] = [], frames = 0
        let each = EachFrame {
            guard let owner = p.modalHost.owner, owner.isBeingPresented, let sheet = p.modalHost.coordinateView,
                  let route = p.modalHost.routes.last?.node, route.window != nil, let window = sheet.window,
                  let shown = sheet.layer.presentation()?.convert(CGPoint.zero, to: nil), shown.y < window.bounds.height - 1 else { return }
            frames += 1
            func walk(_ view: UIView) {
                guard !view.isHidden, view.alpha > 0 else { return }
                // A view moved in this turn (a row carried into its cell) is drawn under its old parent until the
                // turn commits
                if let node = view as? NodeView, let layer = node.layer.presentation(), layer.superlayer?.model() === node.layer.superlayer {
                    let at = layer.convert(CGPoint.zero, to: nil)
                    drawn.append((frames, node.id, CGPoint(x: at.x - shown.x, y: at.y - shown.y)))
                }
                view.subviews.forEach(walk)
            }
            walk(route)
        }
        let link = CADisplayLink(target: each, selector: #selector(EachFrame.tick))
        link.add(to: .main, forMode: .common)
        p.press(try XCTUnwrap(named("open-form")).id)
        until("the presentation begins") { p.modalHost.owner?.isBeingPresented == true }
        let owner = try XCTUnwrap(p.modalHost.owner)
        until("the presentation lands") { !owner.isBeingPresented }
        link.invalidate()
        XCTAssertTrue(risen, "the keyboard rose while the sheet was being presented")
        XCTAssertGreaterThan(frames, 0, "frames were drawn while the sheet slid up")
        let sheet = try XCTUnwrap(p.modalHost.coordinateView), route = try XCTUnwrap(p.modalHost.routes.last?.node)
        until("then the route ends at the keys") {
            guard let top = p.keyboardTop else { return false }
            return abs(route.convert(route.bounds, to: nil).maxY - top) < 1
        }
        let laid = Dictionary(p.views.values.filter { $0.window != nil }.map { ($0.id, $0.convert(CGPoint.zero, to: sheet)) }) { a, _ in a }
        let off = drawn.compactMap { d in laid[d.id].map { (d, max(abs(d.at.x - $0.x), abs(d.at.y - $0.y))) } }.max { $0.1 < $1.1 }
        if let (d, distance) = off {
            let node = p.views[d.id]
            XCTAssertLessThan(distance, 1, "\(node?.kind ?? "?") \(d.id) \(node?.props["testId"] ?? "") was drawn at \(d.at) on frame \(d.frame) of \(frames), laid out at \(laid[d.id] ?? .zero)")
        }
        XCTAssertNotNil(off, "the route's node views were sampled")
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
/// A display link's target, which a run of the run loop calls each frame.
private final class EachFrame: NSObject {
    private let body: () -> Void
    init(_ body: @escaping () -> Void) { self.body = body }
    @objc func tick() { body() }
}
#endif
