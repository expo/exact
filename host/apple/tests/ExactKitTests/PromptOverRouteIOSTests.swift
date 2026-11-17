#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A prompt opened in a page a full-screen route covers. The prompt is
/// UIKit's alert with `PromptField` standing in for the authored input
/// (`IOS/PromptFieldIOS.swift`), and UIKit presents the alert over every
/// presentation, so the dialog's subtree escapes the route's inertness
/// (`ModalHost.inertBehind`), as HTML's topmost modal dialog escapes the
/// inertness of what it covers. A person's keystroke, the agent's `type`
/// and the node's `inert` agree that the field is reachable; a plain input
/// in the same page stays inert.
final class PromptOverRouteIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?
    private var kept: Presenter?

    override func tearDown() {
        if let p = kept, window?.windowScene != nil {
            p.menus.reset()
            waitFor("the prompt's alert dismisses") { p.menus.nativeConfirmation == nil && !p.menus.inTransition }
            p.modals.closeTop(animated: false)
            waitFor("the route dismisses") { !p.modals.inTransition }
        }
        session?.destroy()
        session = nil
        kept = nil
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    /// A page holding a plain input (21), the prompt `rename` (2) with its
    /// input (20), Save (3) and Cancel (9), and a full-screen route (30),
    /// presented. The wire tree has no runner plan behind it, so the
    /// presenter's callbacks into a session are cleared (CustomInvokerIOSTests).
    private func coveredPage(_ scene: UIWindowScene) throws -> Presenter {
        let session = ExactApp.shared.makeSession(label: "prompt-over-route")
        self.session = session
        let p = session.presenter
        kept = p
        p.session = nil
        p.onIntrinsic = nil; p.onCovers = nil; p.onClipboard = nil; p.onClose = nil
        p.onPress = nil; p.onFocus = nil; p.onBlur = nil; p.onFocusEvent = nil; p.onKey = nil
        p.buttonFace = { id in var face = ButtonFace(); face.title = [3: "Save", 9: "Cancel"][id]; return face }
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        let controller = UIViewController()
        window.rootViewController = controller
        window.makeKeyAndVisible()
        self.window = window
        p.viewport.frame = controller.view.bounds
        controller.view.addSubview(p.viewport)
        let closes = ["type": "button", "accessibilityRole": "button", "commandfor": "rename", "command": "close"]
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "props": ["id": "rename", "semanticTag": "dialog",
                                                                "accessibilityRole": "alertdialog", "accessibilityLabel": "Rename"]],
            ["op": "create", "id": 3, "kind": "control", "handlers": ["press"], "props": closes],
            ["op": "create", "id": 9, "kind": "control", "handlers": [], "props": closes],
            ["op": "create", "id": 20, "kind": "input", "handlers": ["input", "change"], "props": ["value": "Untitled"]],
            ["op": "create", "id": 21, "kind": "input", "handlers": ["input"], "props": ["value": ""]],
            ["op": "create", "id": 30, "kind": "view", "props": ["navigationPresentation": "fullscreen"]],
            ["op": "children", "id": 2, "ids": [20, 3, 9]],
            ["op": "children", "id": 1, "ids": [21, 2, 30]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 402.0, "h": 874.0],
            ["op": "frame", "id": 2, "x": 20.0, "y": 200.0, "w": 300.0, "h": 200.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 80.0, "w": 120.0, "h": 40.0],
            ["op": "frame", "id": 9, "x": 140.0, "y": 80.0, "w": 120.0, "h": 40.0],
            ["op": "frame", "id": 20, "x": 0.0, "y": 20.0, "w": 260.0, "h": 32.0],
            ["op": "frame", "id": 21, "x": 20.0, "y": 40.0, "w": 300.0, "h": 40.0],
            ["op": "frame", "id": 30, "x": 0.0, "y": 0.0, "w": 402.0, "h": 874.0],
        ]))
        p.viewport.layoutIfNeeded()
        let route = try XCTUnwrap(p.views[30])
        let navigation = UIViewController()
        navigation.view.addSubview(route)
        p.modals.present(route, navigation: navigation, from: controller, preceding: nil, owner: controller)
        waitFor("the full-screen route settles") { !p.modals.inTransition }
        return p
    }

    func testAPromptOverAFullScreenRouteTakesTextAndThePageBehindItStaysInert() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a presentation completes only in a scene's window") }
        let p = try coveredPage(scene)
        let input = try XCTUnwrap(p.views[20]), plain = try XCTUnwrap(p.views[21])
        XCTAssertTrue(input.inert, "the prompt's input, closed, is in the page the route covers")
        XCTAssertTrue(plain.inert)

        p.dialogCommand("showModal", "rename")
        waitFor("the prompt's alert presents") { input.headerField != nil && !p.menus.inTransition }
        let field = try XCTUnwrap(input.headerField)
        XCTAssertNotNil(field.window, "UIKit shows the alert over the route")
        XCTAssertFalse(input.inert, "the alert is over the route, so its input is reachable")
        XCTAssertTrue(plain.inert, "the rest of the page stays behind the route")

        let delegate = try XCTUnwrap(field.delegate)
        let whole = NSRange(location: 0, length: (field.text ?? "").utf16.count)
        XCTAssertEqual(delegate.textField?(field, shouldChangeCharactersIn: whole, replacementString: "P"), true,
                       "a person's keystroke in the alert's field is the input's")

        let agent = Agent(session: try XCTUnwrap(session))
        let typed = agent.type(["id": Int(input.id), "text": "Plan A"])
        XCTAssertNil(typed["error"], "\(typed)")
        XCTAssertEqual(field.text, "Plan A")
        XCTAssertEqual(input.fieldText, "Plan A")

        let refused = agent.type(["id": Int(plain.id), "text": "Plan B"])
        XCTAssertEqual(refused["error"] as? String, "view \(plain.id) is inert")
        XCTAssertEqual(plain.fieldText, "")
        let plainField = try XCTUnwrap(plain.field)
        XCTAssertFalse(plain.textFieldShouldBeginEditing(plainField), "a person cannot start editing it either")
    }
}
#endif
