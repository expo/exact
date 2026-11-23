#if os(iOS)
import XCTest
@testable import ExactKit
import UIKit

/// A text control's `overflow` is its editor's: a textarea's text view and
/// an input's field scroll their own text, as a web `<textarea>` does, so
/// the node makes no scroll view of its own over them, and a touch on the
/// field lands on the editor and takes the focus.
final class FieldOverflowIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?
    override func tearDown() { session?.destroy(); session = nil; window?.isHidden = true; window = nil; super.tearDown() }

    func testATouchOnAScrollingTextControlLandsOnItsEditorAndFocusesIt() throws {
        let session = ExactApp.shared.makeSession(label: "field-overflow")
        self.session = session
        let p = session.presenter
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 800)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        self.window = window
        p.viewport.frame = window.bounds
        window.rootViewController?.view.addSubview(p.viewport)
        // CSS computes `overflow-x` to `auto` beside `overflow-y: auto`.
        let scrolls: [String: Any] = ["overflow_x": "auto", "overflow_y": "auto", "written": ["overflow_y"]]
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "textarea", "props": ["value": ""], "handlers": ["input", "focus"], "style": scrolls],
            ["op": "create", "id": 3, "kind": "input", "props": ["value": ""], "handlers": ["input", "focus"], "style": scrolls],
            ["op": "children", "id": 1, "ids": [2, 3]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0],
            ["op": "frame", "id": 2, "x": 20.0, "y": 20.0, "w": 240.0, "h": 66.0],
            ["op": "frame", "id": 3, "x": 20.0, "y": 120.0, "w": 240.0, "h": 36.0],
        ]))
        p.viewport.layoutIfNeeded()
        let area = try XCTUnwrap(p.views[2]), input = try XCTUnwrap(p.views[3])
        let editors: [(NodeView, UIView?)] = [(area, area.textArea), (input, input.field)]
        for (node, editor) in editors {
            let editor = try XCTUnwrap(editor, "\(node.kind)")
            XCTAssertNil(node.scroll, "\(node.kind): the editor scrolls its text; the node makes no scroll view over it")
            let middle = node.convert(CGPoint(x: node.bounds.midX, y: node.bounds.midY), to: nil)
            let hit = try XCTUnwrap(window.hitTest(middle, with: nil), "\(node.kind)")
            // What a tap focuses: the nearest view from the hit that can take it.
            let focusable = sequence(first: hit, next: \.superview).first { $0.canBecomeFirstResponder }
            XCTAssertTrue(focusable === editor, "\(node.kind): the touch lands on \(hit), outside its editor")
            XCTAssertTrue(focusable?.becomeFirstResponder() == true && editor.isFirstResponder, "\(node.kind) takes the focus")
        }
    }
}
#endif
