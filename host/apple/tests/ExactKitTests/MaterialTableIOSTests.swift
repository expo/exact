#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class MaterialTableIOSTests: XCTestCase {
    func testEveryMaterialIsAUIKitOne() throws {
        try MaterialTable.check(1) { Materials.blurStyle($0) != nil }
    }

    /// Named materials reach their styles; `backdrop-filter` alone draws
    /// `.light` (Charlie, 2026-09-27; LLP 1053.000 §3).
    func testNamedMaterialsAndABareBlurDrawTheirStyles() {
        XCTAssertEqual(Backdrop.material, .light)
        for (name, style) in [("thin", UIBlurEffect.Style.systemThinMaterial), ("chrome-dark", .systemChromeMaterialDark), ("prominent", .prominent)] {
            XCTAssertEqual(Materials.blurStyle(Materials.platform(name)!.apple), style, name)
        }
        XCTAssertEqual(Materials.platform("sidebar")?.standIn, true, "an AppKit material draws a stand-in")
    }

    func testSaturationKeepsTheDeclaredMaterialApproximation() {
        let p = Presenter()
        let node = NodeView(id: 1, kind: "view", presenter: p)
        node.applyStyle(["backdrop_filter": [["saturate": 0]]])
        XCTAssertEqual(node.materialRequest, "backdrop")
        XCTAssertTrue(node.materialView is BackdropEffectView)
        XCTAssertEqual((node.materialView as? BackdropEffectView)?.operations, [.saturate(0)])
        node.applyStyle([:])
        XCTAssertNil(node.materialRequest)
        XCTAssertNil(node.materialView)
    }

    /// `-exact-glass-tint` is the glass's `tintColor`, and a changed tint
    /// makes the effect again; unset, the glass is untinted. Interactivity
    /// follows a press unless `glassInteractive` says otherwise.
    func testGlassTakesItsTintAndItsInteractiveOverride() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("UIGlassEffect is iOS 26's") }
        let p = Presenter()
        let node = NodeView(id: 1, kind: "view", presenter: p)
        node.applyProps(set: ["backgroundMaterial": "glass"], clear: [])
        let glass = { try XCTUnwrap(node.materialView?.effect as? UIGlassEffect) }
        XCTAssertNil(try glass().tintColor, "untinted by default")
        XCTAssertFalse(try glass().isInteractive)
        node.applyStyle(["glass_tint": [232, 192, 99, 230]])
        XCTAssertEqual(try XCTUnwrap(try glass().tintColor).cgColor.alpha, 230 / 255, accuracy: 0.01)
        node.applyStyle(["glass_tint": [10, 12, 16, 140]])
        XCTAssertEqual(try XCTUnwrap(try glass().tintColor).cgColor.alpha, 140 / 255, accuracy: 0.01, "a changed tint makes the effect again")
        node.applyStyle([:])
        XCTAssertNil(try glass().tintColor, "a cleared tint leaves it untinted")
        node.handlers = ["press"]
        XCTAssertTrue(try glass().isInteractive, "a pressable box's glass reacts")
        node.applyProps(set: ["glassInteractive": "false"], clear: [])
        XCTAssertFalse(try glass().isInteractive, "unless it says not to")
        node.handlers = []
        node.applyProps(set: ["glassInteractive": "true"], clear: [])
        XCTAssertTrue(try glass().isInteractive, "or says so without a press")
    }

    /// `menuOrder` is UIKit's `preferredMenuElementOrder`; unsaid, `.automatic`.
    func testAMenuOrderIsUIKitsElementOrder() {
        XCTAssertEqual(MenuHost.order(nil), .automatic)
        for (word, order) in [("fixed", UIContextMenuConfiguration.ElementOrder.fixed), ("priority", .priority), ("automatic", .automatic)] {
            XCTAssertEqual(MenuHost.order(word), order, word)
        }
    }
}
#endif
