#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// `caret-color` on UIKit is a field's tint, which colours its caret and
/// selection handles (`UIView.tintColor`), in a native and a bare `input`
/// alike, as in a `textarea`; the Mac's is `CaretColorMacTests`. UIKit, so a
/// simulator runs it: bun host/apple/build.mjs --test --ios
final class CaretColorIOSTests: XCTestCase {
    private var window: UIWindow!
    override func tearDown() { window?.isHidden = true; window = nil }

    func testAWrittenCaretColorTintsANativeAndABareInput() throws {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 300))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        let red: [Int] = [255, 59, 48, 255]
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "input", "props": ["value": ""], "style": ["caret_color": red]],
            ["op": "create", "id": 3, "kind": "input", "props": ["value": ""], "style": ["appearance": "none", "caret_color": red]],
            ["op": "create", "id": 4, "kind": "input", "props": ["value": ""], "style": ["appearance": "none"]],
            ["op": "children", "id": 1, "ids": [2, 3, 4]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 300],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 300, "h": 30],
            ["op": "frame", "id": 3, "x": 0, "y": 40, "w": 300, "h": 30],
            ["op": "frame", "id": 4, "x": 0, "y": 80, "w": 300, "h": 30],
        ]))
        let author = UIColor(red: 1, green: 59 / 255, blue: 48 / 255, alpha: 1)
        func rgba(_ color: UIColor?) -> [Int] {
            var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
            color?.resolvedColor(with: window.traitCollection).getRed(&r, green: &g, blue: &b, alpha: &a)
            return [r, g, b, a].map { Int(($0 * 255).rounded()) }
        }
        let native = try XCTUnwrap(p.views[2]?.field), bare = try XCTUnwrap(p.views[3]?.field)
        let plain = try XCTUnwrap(p.views[4]?.field)
        XCTAssertEqual(rgba(native.tintColor), rgba(author))
        XCTAssertEqual(rgba(bare.tintColor), rgba(author), "a bare input's caret is the author's, as a bare textarea's is")
        // A props write (each keystroke's value) keeps it.
        p.apply(wireBatch([["op": "props", "id": 3, "set": ["value": "a"]]]))
        XCTAssertEqual(rgba(bare.tintColor), rgba(author), "a props write keeps the author's caret")
        XCTAssertEqual(rgba(plain.tintColor), rgba(plain.superview?.tintColor), "unsaid, a bare input takes the tint around it")
    }
}
#endif
