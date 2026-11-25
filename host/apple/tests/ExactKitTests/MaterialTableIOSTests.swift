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
        // UIKit's plain blurs, with no material's wash, are names too (#122).
        for (name, style) in [("thin", UIBlurEffect.Style.systemThinMaterial), ("chrome-dark", .systemChromeMaterialDark), ("prominent", .prominent),
                              ("dark", .dark), ("light", .light), ("extra-light", .extraLight), ("regular", .regular)] {
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

    func testAParagraphsInkStaysAboveItsOwnMaterial() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "a material and its paragraph draw in a scene's window")
        }
        let session = ExactApp.shared.makeSession(label: "material-text")
        defer { session.destroy() }
        let p = session.presenter
        let window = UIWindow(windowScene: scene)
        window.frame = scene.screen.bounds
        window.rootViewController = UIViewController()
        window.rootViewController!.view.addSubview(p.viewport)
        p.viewport.frame = window.bounds
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "Light, depth, and a touch of glass."],
             "style": ["font_size": 17.0, "text_color": [0, 0, 0, 255], "system_colors": ["text_color": "-apple-system-label"]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": window.bounds.width, "h": window.bounds.height],
            ["op": "frame", "id": 2, "x": 20, "y": 100, "w": 300, "h": 100],
        ]))
        let node = try XCTUnwrap(p.views[2])
        func check(_ phase: String, children: Bool = false) throws {
            p.textRasters.settleVisible([node])
            XCTAssertNotNil(node.textRasterLayer, phase)
            let box = try XCTUnwrap(node.textBoxLayer, phase)
            let material = try XCTUnwrap(node.glassSlot ?? node.materialView, phase)
            let layers = try XCTUnwrap(node.layer.sublayers, phase)
            let inkIndex = try XCTUnwrap(layers.firstIndex { $0 === box }, phase)
            var backdrop = material.layer
            while let parent = backdrop.superlayer, parent !== node.layer { backdrop = parent }
            let materialIndex = try XCTUnwrap(layers.firstIndex { $0 === backdrop }, phase)
            XCTAssertGreaterThan(inkIndex, materialIndex, "the ink must not be part of the material's backdrop: \(phase)")
            if children {
                for id: UInt32 in [3, 4, 5] {
                    let child = try XCTUnwrap(p.views[id], phase)
                    let childIndex = try XCTUnwrap(layers.firstIndex { $0 === child.layer }, phase)
                    XCTAssertGreaterThan(childIndex, inkIndex, "child \(id) must paint above the ink: \(phase)")
                }
            }
        }
        for kind in ["glass", "ultra-thin"] {
            node.dropTextRaster()
            node.invalidateText()
            node.applyProps(set: ["backgroundMaterial": kind], clear: [])
            try check("\(kind), material before ink")
            node.applyProps(set: [:], clear: ["backgroundMaterial"])
            node.applyProps(set: ["backgroundMaterial": kind], clear: [])
            try check("\(kind), ink before material")
            window.overrideUserInterfaceStyle = .dark
            node.applyStyle(["font_size": 53.0, "text_color": [255, 255, 255, 255],
                             "system_colors": ["text_color": "-apple-system-label"], "border_radius": 44.0,
                             "border_width": 1.0, "border_color_top": [255, 0, 0, 255]])
            try check("\(kind), changed traits and box paint")
            node.mountTextBox()
            try check("\(kind), remounted ink")
            window.overrideUserInterfaceStyle = .light
        }
        p.apply(wireBatch([
            ["op": "create", "id": 3, "kind": "image"],
            ["op": "create", "id": 4, "kind": "image", "props": ["imageSource": "symbol:sf/star", "symbolName": "star"]],
            ["op": "create", "id": 5, "kind": "view", "props": ["testId": "child"]],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 20, "h": 20],
            ["op": "frame", "id": 4, "x": 20, "y": 0, "w": 20, "h": 20],
            ["op": "frame", "id": 5, "x": 40, "y": 0, "w": 20, "h": 20],
        ]))
        for kind in ["ultra-thin", "backdrop"] {
            node.dropTextRaster()
            node.applyProps(set: [:], clear: ["backgroundMaterial"])
            var style: NodeStyle = ["font_size": 17.0, "text_color": [0, 0, 0, 255]]
            if kind == "backdrop" { style["backdrop_filter"] = [["blur": 12.0]] }
            node.applyStyle(style)
            if kind == "ultra-thin" { node.applyProps(set: ["backgroundMaterial": kind], clear: []) }
            p.apply(wireBatch([["op": "children", "id": 2, "ids": [3, 4, 5]]]))
            XCTAssertTrue(node.container === node, "\(kind) leaves its children beside the material")
            try check("\(kind), children before ink", children: true)
            let raster = try XCTUnwrap(node.specs?.rasterView)
            p.apply(wireBatch([["op": "children", "id": 2, "ids": [5, 3, 4]]]))
            XCTAssertTrue(node.specs?.rasterView === raster, "the second batch places children around mounted ink")
            try check("\(kind), children placed again", children: true)
            XCTAssertEqual(node.subviews.compactMap { ($0 as? NodeView)?.id }, [5, 3, 4])
        }
        if #available(iOS 26.0, *) {
            node.applyStyle(["font_size": 17.0, "text_color": [0, 0, 0, 255]])
            p.apply(wireBatch([
                ["op": "props", "id": 1, "set": ["glassGroup": "12"], "clear": []],
                ["op": "props", "id": 2, "set": ["backgroundMaterial": "glass"], "clear": []],
            ]))
            XCTAssertNotNil(node.glassSlot, "the group's glass has a slot")
            node.dropTextRaster()
            node.invalidateText()
            try check("grouped glass, material before ink")
            node.mountTextBox()
            try check("grouped glass, remounted ink")
        }
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
