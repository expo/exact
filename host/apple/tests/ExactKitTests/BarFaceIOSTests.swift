#if os(iOS)
import UIKit
import XCTest
@testable import ExactImages
@testable import ExactKit
@testable import ExactNavigation

/// A header button's face in the bar: a badge (`BadgeFace`, LLP 1075.003
/// §9.6) is the item's drawn image, and a native button's title is UIKit's,
/// in its authored font (LLP 1141.004 D1).
final class BarFaceIOSTests: XCTestCase {
    private func button(_ children: [[String: Any]], in p: Presenter = Presenter()) -> (Presenter, NodeView) {
        var ops: [[String: Any]] = [
            ["op": "create", "id": 1, "kind": "button", "handlers": ["press"], "props": [:], "style": ["text_color": [0, 0, 0, 255]]],
            ["op": "create", "id": 2, "kind": "view", "handlers": [], "props": [:], "style": ["column_gap": 5]]]
        ops += children
        ops += [["op": "children", "id": 2, "ids": children.map { $0["id"]! }], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]]]
        p.apply(wireBatch(ops))
        return (p, p.views[1]!)
    }
    private let dot: [String: Any] = ["op": "create", "id": 3, "kind": "view", "handlers": [], "props": [:],
                                      "style": ["background_color": [52, 199, 89, 255], "width": 14, "height": 6]]
    private func label(_ style: [String: Any]) -> [String: Any] {
        ["op": "create", "id": 4, "kind": "text", "handlers": [], "props": ["text": "Clock"],
         "style": ["text_color": [142, 142, 147, 255], "font_size": 12].merging(style) { $1 }]
    }

    func testABadgeIsDrawnForThePagesSchemeAtTheSamePointSize() throws {
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "button", "props": ["testId": "badge"]],
            ["op": "create", "id": 2, "kind": "view", "style": ["background_color": [[255, 0, 0, 255], [0, 0, 255, 255]]]],
            ["op": "create", "id": 3, "kind": "text", "props": ["text": "A"]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3]],
            ["op": "roots", "ids": [1]],
        ]))
        let box = try XCTUnwrap(p.views[2])
        box.traitOverrides.userInterfaceStyle = .light
        box.updateTraitsIfNeeded()
        let light = try XCTUnwrap(BadgeFace(box: box))
        box.traitOverrides.userInterfaceStyle = .dark
        box.updateTraitsIfNeeded()
        let dark = try XCTUnwrap(BadgeFace(box: box))
        XCTAssertNotEqual(light.source, dark.source, "a scheme change redraws the bar item's face")
        XCTAssertEqual(light.image.size, CGSize(width: 36, height: 36))
        XCTAssertEqual(dark.image.size, light.image.size)
        let pixel = try XCTUnwrap(SharePictureCase.rgba(try XCTUnwrap(dark.image.cgImage), x: 4, y: 4))
        XCTAssertGreaterThan(pixel.2, 240, "the installed image itself is blue in dark mode")
        XCTAssertLessThan(pixel.0, 15)
    }

    func testBadgeTextSizeAndWeightChangeItsImage() throws {
        let p = Presenter()
        defer { p.reset() }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [255, 255, 255, 255]]],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "AB"],
             "style": ["text_color": [0, 0, 0, 255]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
        ]))
        let box = try XCTUnwrap(p.views[1]), text = try XCTUnwrap(p.views[2])
        let plain = try XCTUnwrap(BadgeFace(box: box))
        text.style["font_size"] = .number(24)
        text.style["written"] = .array([.string("font_size")])
        let large = try XCTUnwrap(BadgeFace(box: box))
        text.style["font_weight"] = .number(800)
        text.style["written"] = .array([.string("font_size"), .string("font_weight")])
        let heavy = try XCTUnwrap(BadgeFace(box: box))
        for (before, after) in [(plain, large), (large, heavy)] {
            XCTAssertNotEqual(before.source, after.source)
            XCTAssertNotEqual(try XCTUnwrap(before.image.pngData()), try XCTUnwrap(after.image.pngData()))
        }
    }

    func testOversizedBadgeTextKeepsTheRoundedCornerClear() throws {
        let p = Presenter()
        defer { p.reset() }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view",
             "style": ["background_color": [255, 255, 255, 255], "border_radius": 18, "overflow": "hidden"]],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "███"],
             "style": ["text_color": [0, 0, 0, 255], "font_size": 72, "written": ["font_size"]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
        ]))
        let badge = try XCTUnwrap(BadgeFace(box: try XCTUnwrap(p.views[1])))
        let image = try XCTUnwrap(badge.image.cgImage)
        let center = try XCTUnwrap(SharePictureCase.rgba(image, x: image.width / 2, y: image.height / 2))
        let corner = try XCTUnwrap(SharePictureCase.rgba(image, x: 1, y: 1))
        XCTAssertEqual(center.0, 0)
        XCTAssertEqual(center.3, 255)
        XCTAssertEqual(corner.3, 0)
    }

    func testABadgeOnAGlassButtonKeepsItsOwnGlass() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bar item glass is iOS 26") }
        let p = Presenter()
        defer { p.reset() }
        let fill: [String: Any] = ["background_color": [[255, 0, 0, 255], [0, 0, 255, 255]], "width": 36, "height": 36]
        p.apply(wireBatch([
            ["op": "create", "id": 100, "kind": "view", "props": ["semanticTag": "header"]],
            ["op": "create", "id": 1, "kind": "button", "props": ["testId": "plain"]],
            ["op": "create", "id": 2, "kind": "view", "style": fill],
            ["op": "create", "id": 3, "kind": "text", "props": ["text": "A"]],
            ["op": "create", "id": 4, "kind": "button", "props": ["testId": "glass", "backgroundMaterial": "glass"]],
            ["op": "create", "id": 5, "kind": "view", "style": fill],
            ["op": "create", "id": 6, "kind": "text", "props": ["text": "A"]],
            ["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 1, "ids": [2]],
            ["op": "children", "id": 5, "ids": [6]], ["op": "children", "id": 4, "ids": [5]],
            ["op": "children", "id": 100, "ids": [1, 4]], ["op": "roots", "ids": [100]],
        ]))
        let shape = try [1, 4].map { HeaderShape.Item(try XCTUnwrap(p.views[$0])) }
        XCTAssertNotNil(shape[0].badge)
        XCTAssertNotNil(shape[1].badge)
        XCTAssertFalse(shape[0].ownGlass)
        XCTAssertTrue(shape[1].ownGlass)
        let items = p.navigationHost.barItems(shape, RouteController(try XCTUnwrap(p.views[100])))
        XCTAssertEqual(items.count, 2)
        XCTAssertTrue(items[0].hidesSharedBackground, "a badge alone is its own shape, with no glass")
        XCTAssertFalse(items[1].hidesSharedBackground, "a badge on a glass button shows glass")
        XCTAssertFalse(items[1].sharesBackground, "and that glass is its own capsule")
    }

    /// A box holding a picture (an avatar's photo) is a badge: its fill until the picture loads, then the
    /// picture clipped to the box's corners. The header the bar draws is hidden, and the picture still loads.
    func testABadgesPictureLoadsInAHeaderTheBarDraws() throws {
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let png = try XCTUnwrap(UIGraphicsImageRenderer(size: CGSize(width: 20, height: 20), format: format).image { context in
            UIColor.red.setFill(); context.fill(CGRect(x: 0, y: 0, width: 20, height: 20))
        }.pngData())
        let source = "data:image/png;base64," + png.base64EncodedString()
        let p = Presenter(), loader = RasterLoader(), resolver = AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()))
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 200, height: 200))
        defer { loader.shutdown(); window.isHidden = true; p.reset() }
        p.viewport.frame = window.bounds; window.addSubview(p.viewport); window.makeKeyAndVisible()
        let round: [String: Any] = ["border_radius_top_left": 18, "border_radius_top_right": 18, "border_radius_bottom_left": 18, "border_radius_bottom_right": 18]
        p.apply(wireBatch([
            ["op": "create", "id": 100, "kind": "view", "props": ["semanticTag": "header"]],
            ["op": "create", "id": 1, "kind": "button", "props": ["testId": "avatar"]],
            ["op": "create", "id": 2, "kind": "view", "style": round.merging(["background_color": [0, 0, 255, 255], "width": 36, "height": 36]) { $1 }],
            ["op": "create", "id": 3, "kind": "image", "props": ["imageSource": source], "style": ["object_fit": "cover", "width": 36, "height": 36]],
            ["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 1, "ids": [2]],
            ["op": "children", "id": 100, "ids": [1]], ["op": "roots", "ids": [100]],
        ]))
        let header = try XCTUnwrap(p.views[100]), box = try XCTUnwrap(p.views[2]), image = try XCTUnwrap(p.views[3])
        defer { image.raster = nil }
        image.frame = CGRect(x: 0, y: 0, width: 36, height: 36)
        let pending = try XCTUnwrap(BadgeFace(box: box), "a box holding only a picture is a badge before it loads")
        XCTAssertNil(pending.picture)
        XCTAssertEqual(pending.pictureNode, 3)
        // As the bar marks it when it stands for the header (`project`).
        header.isHidden = true; image.drawnByBar = true
        let fill = try XCTUnwrap(pending.image.cgImage)
        let blue = try XCTUnwrap(SharePictureCase.rgba(fill, x: fill.width / 2, y: fill.height / 2))
        XCTAssertGreaterThan(blue.2, 240, "the box's fill while the picture loads")

        loader.load(image, source: source, resolver: resolver)
        let deadline = Date(timeIntervalSinceNow: 5)
        while image.raster == nil, Date() < deadline { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.02)) }
        XCTAssertNotNil(image.raster, "a picture in a hidden header the bar draws loads")

        let face = try XCTUnwrap(BadgeFace(box: box))
        XCTAssertNotNil(face.picture)
        XCTAssertNotEqual(face.source, pending.source, "the bar item is drawn again with the picture")
        let drawn = try XCTUnwrap(face.image.cgImage)
        let centre = try XCTUnwrap(SharePictureCase.rgba(drawn, x: drawn.width / 2, y: drawn.height / 2))
        XCTAssertGreaterThan(centre.0, 240, "the picture fills the face")
        XCTAssertLessThan(centre.2, 15)
        let corner = try XCTUnwrap(SharePictureCase.rgba(drawn, x: 0, y: 0))
        XCTAssertEqual(corner.3, 0, "the box's corners clip the picture")
    }

    /// A native button's title size and weight, written on it or on its
    /// text, are its bar item's title font (LLP 1069.011.001 D4): the "●
    /// Clock" glass chip at 12 pt, in every state UIKit draws it in. A
    /// native button whose face says nothing of its font leaves UIKit's.
    func testANativeButtonsAuthoredFontIsItsBarTitlesFont() throws {
        let p = Presenter()
        defer { p.reset() }
        var clock = ButtonFace()
        clock.title = "● Clock"
        clock.style = "glass"
        clock.rows.title = ["font_size": .number(12)]
        var heavy = ButtonFace()
        heavy.title = "Done"
        heavy.rows.title = ["font_weight": .number(600)]
        var plain = ButtonFace()
        plain.title = "Edit"
        p.buttonFace = { [1: clock, 2: heavy][$0] ?? plain }
        let header = NodeView(id: 100, kind: "view", presenter: p)
        header.props["semanticTag"] = "header"
        let buttons = (1...3).map { id -> NodeView in
            let button = NodeView(id: UInt32(id), kind: "control", presenter: p)
            button.props["type"] = "button"
            header.container.addSubview(button)
            return button
        }
        let shape = buttons.map { HeaderShape.Item($0) }
        XCTAssertEqual(shape.map(\.title), ["● Clock", "Done", "Edit"])
        XCTAssertTrue(shape[0].ownGlass)
        let items = p.navigationHost.barItems(shape, RouteController(header)).filter { $0.title != nil }
        XCTAssertEqual(items.count, 3)
        let unsaid = HeaderShape.Item.titleFont(bold: false)
        for state: UIControl.State in [.normal, .highlighted, .disabled] {
            let font = try XCTUnwrap(items[0].titleTextAttributes(for: state)?[.font] as? UIFont, "state \(state.rawValue)")
            XCTAssertEqual(font.pointSize, 12, "the author's size")
            XCTAssertEqual(TextEngine.controlWeight(font), TextEngine.controlWeight(unsaid), "the bar's weight, unsaid")
        }
        let done = try XCTUnwrap(items[1].titleTextAttributes(for: .normal)?[.font] as? UIFont)
        XCTAssertEqual(done.pointSize, unsaid.pointSize, "a size left unsaid is UIKit's")
        XCTAssertEqual(TextEngine.controlWeight(done), 600, "the author's weight")
        XCTAssertNil(items[2].titleTextAttributes(for: .normal), "nothing written: UIKit's title")
        clock.rows.title = ["font_size": .number(13)]
        p.apply(facesBatch())
        XCTAssertNotEqual(HeaderShape.Item(buttons[0]).source, shape[0].source, "a new size makes the item again")
        heavy.rows.title = ["font_weight": .number(650)]
        p.apply(facesBatch())
        XCTAssertNotEqual(HeaderShape.Item(buttons[1]).source, shape[1].source, "and so does a new weight in the same face")
    }

    /// A kept item whose author removes its font has UIKit's again, in
    /// every state, and is still the same item.
    func testAKeptItemLosesARemovedFont() throws {
        let p = Presenter()
        defer { p.reset() }
        var done = ButtonFace()
        done.title = "Done"
        done.rows.title = ["font_size": .number(12)]
        p.buttonFace = { _ in done }
        let header = NodeView(id: 100, kind: "view", presenter: p)
        header.props["semanticTag"] = "header"
        let button = NodeView(id: 1, kind: "control", presenter: p)
        button.props["type"] = "button"
        header.container.addSubview(button)
        let route = RouteController(header)
        let item = try XCTUnwrap(p.navigationHost.barItems([HeaderShape.Item(button)], route).first)
        XCTAssertEqual((item.titleTextAttributes(for: .normal)?[.font] as? UIFont)?.pointSize, 12)
        done.rows.title = [:]
        p.apply(facesBatch())
        let kept = try XCTUnwrap(p.navigationHost.barItems([HeaderShape.Item(button)], route).first)
        XCTAssertTrue(kept === item, "the item is changed in place")
        for state: UIControl.State in [.normal, .highlighted, .disabled, .selected, .focused] {
            XCTAssertNil(kept.titleTextAttributes(for: state)?[.font], "state \(state.rawValue)")
        }
    }

    /// Under Bold Text an authored weight is the kernel's, stepped already
    /// (an authored 400 arrives as 600) and drawn as it comes; a size alone
    /// keeps the bar's weight, which Bold Text makes heavier, as it does
    /// UIKit's own titles beside it.
    func testBoldTextKeepsTheRowsWeightAndStepsTheBarsOwn() throws {
        let p = Presenter()
        defer { p.reset() }
        var sized = ButtonFace()
        sized.title = "● Clock"
        sized.rows.title = ["font_size": .number(12)]
        var weighted = ButtonFace()
        weighted.title = "Edit"
        weighted.rows.title = ["font_weight": .number(600)]
        p.buttonFace = { $0 == 1 ? sized : weighted }
        let buttons = (1...2).map { id -> NodeView in
            let button = NodeView(id: UInt32(id), kind: "control", presenter: p)
            button.props["type"] = "button"
            button.traitOverrides.legibilityWeight = .bold
            button.updateTraitsIfNeeded()
            return button
        }
        let clock = try XCTUnwrap(HeaderShape.Item(buttons[0]).font)
        XCTAssertEqual(clock.pointSize, 12)
        XCTAssertEqual(TextEngine.controlWeight(clock), TextEngine.controlWeight(HeaderShape.Item.titleFont(bold: true)))
        XCTAssertGreaterThan(TextEngine.controlWeight(clock), TextEngine.controlWeight(HeaderShape.Item.titleFont(bold: false)))
        let edit = try XCTUnwrap(HeaderShape.Item(buttons[1]).font, "an authored 400, stepped, is the author's")
        XCTAssertEqual(TextEngine.controlWeight(edit), 600)
    }

    /// The font a row left unsaid keeps (`titleFont`) is the one UIKit
    /// draws a plain bar item's title in, read from a laid-out bar, with
    /// Bold Text and without.
    func testTheUnsaidTitleFontIsUIKits() throws {
        for bold in [false, true] {
            let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
            if bold { window.traitOverrides.legibilityWeight = .bold }
            let root = UIViewController()
            root.navigationItem.rightBarButtonItem = UIBarButtonItem(title: "Edit", style: .plain, target: nil, action: nil)
            let nav = UINavigationController(rootViewController: root)
            window.rootViewController = nav
            window.isHidden = false
            defer { window.isHidden = true }
            nav.view.layoutIfNeeded()
            nav.navigationBar.layoutIfNeeded()
            func labels(_ view: UIView) -> [UILabel] { (view as? UILabel).map { [$0] } ?? view.subviews.flatMap(labels) }
            let edit = try XCTUnwrap(labels(nav.navigationBar).first { $0.text == "Edit" }, "the item's title label")
            let expected = HeaderShape.Item.titleFont(bold: bold)
            XCTAssertEqual(edit.traitCollection.legibilityWeight == .bold, bold)
            XCTAssertEqual(edit.font.pointSize, expected.pointSize, "bold \(bold)")
            XCTAssertEqual(edit.font.fontName, expected.fontName, "bold \(bold)")
            XCTAssertEqual(TextEngine.controlWeight(edit.font), TextEngine.controlWeight(expected), "bold \(bold)")
        }
    }
}
#endif
