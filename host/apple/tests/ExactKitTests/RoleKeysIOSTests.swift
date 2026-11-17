// @ref LLP 1166.000 D2 — the kernel's keys for a group Exact paints, on
// UIKit with a hardware keyboard: an `input type="radio"` in a radio group
// keeps HTML's keys; a menu in the page moves by Up, Down, Home and End and
// sets no stop; and Tab from a member the arrows reached goes on from it.
// tests/fixtures/painted-groups.contract, booted: the kernel decides.
#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class RoleKeysIOSTests: XCTestCase {
    private var window: UIWindow!

    /// The fixture in a key window; `pressed` names each press the presenter heard.
    private func groups() throws -> (Presenter, ExactSession, () -> [String]) {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_PAINTED_GROUPS_PLAN"])
        let session = ExactApp.shared.makeSession(label: "role-keys-ios")
        addTeardownBlock { session.destroy() }
        let controller = UIViewController()
        let scene = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first
        window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: .zero)
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 1200)
        window.rootViewController = controller
        window.makeKeyAndVisible()
        addTeardownBlock { [weak self] in self?.window.isHidden = true }
        session.presenter.viewport.frame = window.bounds
        controller.view.addSubview(session.presenter.viewport)
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: window.bounds.size).error)
        window.layoutIfNeeded()
        let p = session.presenter
        var pressed: [String] = []
        let press = p.onPress
        p.onPress = { [unowned p] id in pressed.append(p.views[id]?.props["testId"] ?? "#\(id)"); press?(id) }
        return (p, session, { pressed })
    }

    private func node(_ p: Presenter, _ testId: String) throws -> NodeView {
        try XCTUnwrap(p.views.values.first { $0.props["testId"] == testId }, testId)
    }

    /// A hardware key's default action at the focus, as `pressesBegan` takes it.
    private func key(_ p: Presenter, _ name: String) -> Bool {
        guard let focus = p.focusedNode else { return false }
        return p.controls.roleKey(focus, name, held: "") || p.controls.radioKey(focus, name, held: "")
    }

    func testInputRadiosInARadiogroupKeepHTMLsKeys() throws {
        let (p, _, pressed) = try groups()
        // Tab from the button before them: the checked input is the group's stop.
        XCTAssertTrue(try node(p, "choose-Month").becomeFirstResponder())
        XCTAssertTrue(p.moveFocus(backward: false))
        XCTAssertEqual(p.focusedNode?.props["testId"], "size-m")
        XCTAssertFalse(p.controls.roleKey(try node(p, "size-m"), "ArrowDown", held: ""), "no painted group's key")
        XCTAssertTrue(key(p, "ArrowDown"))
        XCTAssertEqual(p.focusedNode?.props["testId"], "size-l")
        waitFor("the arrow's change lands") { (try? node(p, "state").props["text"]) == "l 1 0 0" }
        XCTAssertEqual(pressed(), [], "a radio input is checked, never pressed")
    }

    func testAPaintedPageMenuMovesByUpDownHomeAndEnd() throws {
        let (p, _, pressed) = try groups()
        XCTAssertTrue(try node(p, "item-Cut").becomeFirstResponder())
        var walk: [String?] = []
        for name in ["ArrowDown", "ArrowDown", "ArrowDown", "ArrowUp", "Home", "ArrowRight"] {
            _ = key(p, name)
            walk.append(p.focusedNode?.props["testId"])
        }
        XCTAssertEqual(walk, ["item-Copy", "item-Paste", "item-Cut", "item-Paste", "item-Cut", "item-Cut"])
        XCTAssertFalse(key(p, "ArrowLeft"), "Left is not a menu's key")
        XCTAssertEqual(pressed(), [])
    }

    /// From a member that is no stop (`tabindex=-1`), which the arrows
    /// reached and which while focused is its group's only candidate, Tab
    /// goes to the stop after the group and Shift-Tab to the one before it.
    func testTabFromAMemberTheArrowsReachedGoesOnFromIt() throws {
        let (p, _, _) = try groups()
        for (to, backward, expected) in [("stop-B", true, "before"), ("stop-B", false, "after"), ("stop-C", true, "before"), ("stop-C", false, "after")] {
            XCTAssertTrue(try node(p, "stop-A").becomeFirstResponder())
            XCTAssertTrue(key(p, "ArrowRight"))
            if to == "stop-C" { XCTAssertTrue(key(p, "ArrowRight")) }
            XCTAssertEqual(p.focusedNode?.props["testId"], to)
            XCTAssertTrue(p.moveFocus(backward: backward))
            XCTAssertEqual(p.focusedNode?.props["testId"], expected, "\(backward ? "Shift-Tab" : "Tab") from \(to)")
        }
    }
}
#endif
