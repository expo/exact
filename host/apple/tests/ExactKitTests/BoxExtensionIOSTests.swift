#if os(iOS)
@testable import ExactImages
@testable import ExactKit
@testable import ExactEffects
@testable import ExactSurfaces
@testable import ExactSvg
import UIKit
import XCTest

/// A box's picture mirrored past its border box (LLP 1123 D13's extension), driven directly (`setExtension`): the
/// extension is a child of the box's view behind its children, its picture placed by hand at the border box, its strip
/// showing the picture's edge and taking no touch. UIKit in a scene's window, so the hosted bundle runs it.
final class BoxExtensionIOSTests: XCTestCase {
    // A canvas's overlay and a filter's picture (LLP 1047.001 D4)
    override class func setUp() { super.setUp(); ExactSurfaces.install(); ExactEffects.install() }
    private var window: UIWindow?
    private let blue = [0, 0, 255, 255]

    override func tearDown() {
        window?.isHidden = true
        window = nil
        NodeView.extensionMirrors = { if #available(iOS 26, *) { return true }; return false }()
        super.tearDown()
    }

    /// A red box 250 by 200 points, 100 points down a white 300-by-400 window that takes presses, holding a
    /// 60-point child that takes them too, with `style` added to the box's (`red` its fill, nil for none) and the child
    /// at `child`.
    private func presenter(_ scene: UIWindowScene, box style: [String: Any] = [:], red: [Int]? = [255, 0, 0, 255],
                           child: CGRect = CGRect(x: 20, y: 20, width: 60, height: 60), childStyle: [String: Any] = [:]) -> Presenter {
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 300, height: 400)
        let host = UIViewController()
        window.rootViewController = host
        window.makeKeyAndVisible()
        let p = Presenter()
        p.viewport.frame = host.view.bounds
        host.view.addSubview(p.viewport)
        self.window = window
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "handlers": ["press"], "style": ["background_color": [255, 255, 255, 255]]],
            // Named, so neither is a flat layer in its parent's (LLP 1068 §6.1)
            ["op": "create", "id": 2, "kind": "view", "props": ["testId": "box"], "style": style.merging(red.map { ["background_color": $0] } ?? [:]) { a, _ in a }],
            ["op": "create", "id": 3, "kind": "view", "handlers": ["press"], "props": ["testId": "child"], "style": childStyle],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 400],
            ["op": "frame", "id": 2, "x": 0, "y": 100, "w": 250, "h": 200],
            ["op": "frame", "id": 3, "x": child.minX, "y": child.minY, "w": child.width, "h": child.height],
        ]))
        window.layoutIfNeeded()
        for view in p.views.values { view.layer.displayIfNeeded() }
        return p
    }

    /// A translucent box, so a point painted twice would show darker, with a black shadow 20 points right and up,
    /// which the extension covers above the box and leaves at its right.
    func testAnExtendedBoxsPictureIsMirroredPastItsBorderBoxBehindItsChildren() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the extension view draws in a scene's window") }
        guard #available(iOS 26, *) else { throw XCTSkip("UIBackgroundExtensionView needs iOS 26") }
        let p = presenter(scene, box: ["box_shadow": [["o": [20.0, -20.0], "b": 0.0, "c": [0, 0, 0, 255]]]], red: [255, 0, 0, 128],
                          childStyle: ["background_color": blue])
        let window = try XCTUnwrap(window)
        let root = try XCTUnwrap(p.views[1]), box = try XCTUnwrap(p.views[2]), child = try XCTUnwrap(p.views[3])
        // The child with a negative `z-index`, under its rank-zero siblings and still over the picture
        p.apply(wireBatch([["op": "rank", "id": 3, "rank": -4_294_967_290]]))
        XCTAssertLessThan(child.layer.zPosition, 0)
        let depth = child.layer.zPosition
        let above = box.convert(CGPoint(x: 150, y: -10), to: window), inside = box.convert(CGPoint(x: 150, y: 150), to: window)
        let right = box.convert(CGPoint(x: 260, y: 50), to: window)
        let before = capture(window)
        XCTAssertEqual(pixel(before, at: above), [0, 0, 0], "the shadow shows above the box before it extends")
        XCTAssertEqual(pixel(before, at: right), [0, 0, 0], "and at its right")
        let painted = pixel(before, at: inside)
        box.setExtension(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        window.layoutIfNeeded()
        let e = try XCTUnwrap(box.boxExtension), picture = try XCTUnwrap(e.picture)
        XCTAssertTrue(box.subviews.first === child && box.subviews.last === e.view, "the box's first subview is still its child")
        // A view the host inserts at the back of the box later (a text's, a material's) still shows over the picture
        let late = UIView(frame: CGRect(x: 200, y: 20, width: 40, height: 40))
        late.backgroundColor = UIColor(red: 0, green: 1, blue: 0, alpha: 1)
        box.insertSubview(late, at: 0)
        XCTAssertEqual(e.view.frame, CGRect(x: 0, y: -40, width: 250, height: 240), "the border box outset by the bound")
        XCTAssertEqual(picture.frame, CGRect(x: 0, y: 40, width: 250, height: 200), "the picture placed by hand at the border box")
        XCTAssertNil(box.layer.backgroundColor, "the box's own layer paints none of the picture")
        XCTAssertEqual(rgb(picture.layer.backgroundColor), [255, 0, 0], "the picture's fill is the box's")
        let after = capture(window)
        XCTAssertEqual(pixel(after, at: inside), painted, "the box is painted once, as before")
        XCTAssertTrue(near(pixel(after, at: above), painted), "the strip shows the picture alone, the shadow under it masked: \(pixel(after, at: above))")
        XCTAssertEqual(pixel(after, at: right), [0, 0, 0], "the shadow still shows outside the extension")
        XCTAssertLessThan(e.view.layer.zPosition, child.layer.zPosition, "under the lowest child")
        XCTAssertEqual(pixel(after, at: child.convert(CGPoint(x: 30, y: 30), to: window)), [0, 0, 255], "the child shows over the picture")
        XCTAssertEqual(pixel(after, at: late.convert(CGPoint(x: 20, y: 20), to: window)), [0, 255, 0], "a view inserted later shows over the picture")
        late.removeFromSuperview()
        // A touch in the strip reaches what lies under it, and the child keeps its own
        XCTAssertTrue(window.hitTest(above, with: nil) === root, "the strip takes no touch: \(String(describing: window.hitTest(above, with: nil)))")
        XCTAssertTrue(window.hitTest(child.convert(CGPoint(x: 30, y: 30), to: window), with: nil) === child, "the child keeps its touches")
        let hit = window.hitTest(inside, with: nil)
        XCTAssertFalse(hit.map { $0 === e.view || $0.isDescendant(of: e.view) } ?? false, "a point in the box does not reach the extension")
        // A batch that moves the box moves the extension and its picture with it
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 0, "y": 120, "w": 250, "h": 180]]))
        XCTAssertEqual(e.view.frame, CGRect(x: 0, y: -40, width: 250, height: 220))
        XCTAssertEqual(picture.frame, CGRect(x: 0, y: 40, width: 250, height: 180))
        // Ended, the picture is the box's own again and the shadow shows above it
        box.setExtension(.zero)
        XCTAssertNil(box.boxExtension)
        XCTAssertNil(e.view.superview)
        XCTAssertEqual(child.layer.zPosition, depth, "the child's depth as before")
        XCTAssertEqual(rgb(box.layer.backgroundColor), [255, 0, 0], "the box paints its own fill again")
        box.layer.displayIfNeeded()
        XCTAssertEqual(pixel(capture(window), at: box.convert(CGPoint(x: 150, y: -10), to: window)), [0, 0, 0], "the shadow shows above the box again")
    }

    /// What a box holds leaves its extension view out: the view stays the box's last subview as children are added,
    /// `heldSubviews` holds the children alone, UIKit's focus search is never offered it, a child lifted away goes back
    /// among the same siblings at the index it left, as a menu's and a dialog's do, whether the extension came or went
    /// meanwhile, and a menu row's image that extends still leaves its popover menu-shaped.
    func testWhatABoxHoldsLeavesItsExtensionOut() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the extension view draws in a scene's window") }
        let p = presenter(scene)
        let window = try XCTUnwrap(window)
        let root = try XCTUnwrap(p.views[1]), box = try XCTUnwrap(p.views[2]), child = try XCTUnwrap(p.views[3])
        box.setExtension(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        let mirror = try XCTUnwrap(box.boxExtension?.view)
        p.apply(wireBatch([["op": "create", "id": 4, "kind": "view", "props": ["testId": "later"]], ["op": "children", "id": 2, "ids": [3, 4]],
                           ["op": "frame", "id": 4, "x": 100, "y": 20, "w": 40, "h": 40]]))
        let later = try XCTUnwrap(p.views[4])
        XCTAssertTrue(box.subviews.last === mirror, "the extension view stays last")
        let past = UIView()
        box.addSubview(past)
        XCTAssertTrue(box.subviews.last === mirror, "and goes back last when a view is added past it")
        past.removeFromSuperview()
        XCTAssertEqual(box.heldSubviews.map(ObjectIdentifier.init), [child, later].map(ObjectIdentifier.init))
        XCTAssertFalse(FocusSearch.items(root, in: window.bounds).contains { ($0 as? UIView).map { $0 === mirror || $0.isDescendant(of: mirror) } == true })
        // Lifted away while the extension ends, and back between the same siblings
        let index = try XCTUnwrap(box.subviews.firstIndex(of: child))
        child.removeFromSuperview()
        box.setExtension(.zero)
        box.insertSubview(child, at: min(index, box.subviews.count))
        XCTAssertEqual(box.subviews.map(ObjectIdentifier.init), [child, later].map(ObjectIdentifier.init))
        // And while it comes back
        let at = try XCTUnwrap(box.subviews.firstIndex(of: later))
        later.removeFromSuperview()
        box.setExtension(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        box.insertSubview(later, at: min(at, box.subviews.count))
        XCTAssertEqual(box.heldSubviews.map(ObjectIdentifier.init), [child, later].map(ObjectIdentifier.init))
        XCTAssertTrue(box.subviews.last === box.boxExtension?.view)
        // A menu row of text and an image, the image extending
        p.apply(wireBatch([
            ["op": "create", "id": 10, "kind": "view", "props": ["popover": "auto", "id": "menu", "accessibilityRole": "menu", "accessibilityLabel": "Menu"]],
            ["op": "create", "id": 11, "kind": "button", "handlers": ["press"], "props": ["popovertarget": "menu", "popovertargetaction": "hide"]],
            ["op": "create", "id": 12, "kind": "text", "props": ["text": "Pair"]],
            ["op": "create", "id": 13, "kind": "image", "props": ["testId": "glyph"], "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "children", "id": 11, "ids": [12, 13]], ["op": "children", "id": 10, "ids": [11]], ["op": "children", "id": 1, "ids": [2, 10]],
            ["op": "frame", "id": 10, "x": 0, "y": 0, "w": 220, "h": 60], ["op": "frame", "id": 11, "x": 0, "y": 0, "w": 200, "h": 30],
            ["op": "frame", "id": 12, "x": 30, "y": 0, "w": 160, "h": 20], ["op": "frame", "id": 13, "x": 0, "y": 0, "w": 20, "h": 20],
        ]))
        defer { p.menus.reset() }
        let menu = try XCTUnwrap(p.views[10]), glyph = try XCTUnwrap(p.views[13])
        XCTAssertTrue(menu.menuShaped)
        glyph.setExtension(UIEdgeInsets(top: 0, left: 10, bottom: 0, right: 0))
        XCTAssertNotNil(glyph.boxExtension, "the image extends")
        XCTAssertTrue(menu.menuShaped, "and its row is still a menu item")
    }

    /// No strip shows for a box D13 leaves out (`extensionAllowed`), each keeping its clip, children and fill as they
    /// were: one that clips its overflow, which would clip the strip as a view that clips to its bounds clips its
    /// subviews; a scroller, one rounded and one scrolling on both axes, whose clip is its scroll view's; one with an
    /// authored clip path or mask image; a fill the platform's vibrancy draws; a text, which paints its ground with its
    /// glyphs wherever it does not raster them, one clipping its lines and one rastering them now; and a box whose edge
    /// is what the host draws or hosts for it, a canvas, an iframe, a video, a native module, a 2D canvas, an SVG, a field
    /// and a control. An unclipped box around any of them extends instead.
    func testWhatShowsNoStrip() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the extension view draws in a scene's window") }
        let p = presenter(scene)
        let red: [Int] = [255, 0, 0, 255]
        let square: [[Any]] = [["M", [0, 0]], ["L", [80, 0]], ["L", [80, 80]], ["L", [0, 80]], ["Z", []]]
        let hidden: [String: Any] = ["overflow_x": "hidden", "overflow_y": "hidden"]
        p.apply(wireBatch([
            ["op": "create", "id": 4, "kind": "view", "props": ["testId": "clipping"], "style": hidden.merging(["background_color": red]) { a, _ in a }],
            ["op": "create", "id": 40, "kind": "view", "props": ["testId": "clipped"], "style": ["background_color": blue]],
            ["op": "create", "id": 5, "kind": "view", "props": ["testId": "rounded"],
             "style": ["overflow_x": "hidden", "overflow_y": "auto", "border_radius": 12.0, "background_color": red]],
            ["op": "create", "id": 6, "kind": "view", "props": ["testId": "both"], "style": ["overflow_x": "auto", "overflow_y": "auto", "background_color": red]],
            ["op": "create", "id": 7, "kind": "view", "props": ["testId": "path"], "style": ["clip_path": ["commands": square], "background_color": red]],
            ["op": "create", "id": 8, "kind": "text", "props": ["text": "A long label that will not fit in its box"],
             "style": hidden.merging(["font_size": 17, "background_color": red]) { a, _ in a }],
            ["op": "create", "id": 9, "kind": "text", "props": ["text": "A label"], "style": ["font_size": 17, "background_color": red]],
            ["op": "create", "id": 10, "kind": "canvas", "style": ["background_color": red]],
            ["op": "create", "id": 11, "kind": "iframe", "style": ["background_color": red]],
            ["op": "create", "id": 12, "kind": "view", "props": ["testId": "masked"],
             "style": ["mask_image": ["linear": 90, "stops": [0, 0, 0, 0, 0, 1, 0, 0, 0, 255]], "background_color": red]],
            ["op": "create", "id": 13, "kind": "view", "props": ["backgroundMaterial": "ultra-thin"], "style": ["overflow_x": "hidden", "overflow_y": "hidden"]],
            ["op": "create", "id": 14, "kind": "view", "props": ["testId": "vibrant"],
             "style": ["background_color": red, "system_colors": ["background_color": "-exact-fill"]]],
            ["op": "children", "id": 4, "ids": [40]], ["op": "children", "id": 13, "ids": [14]],
            ["op": "children", "id": 1, "ids": [2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]],
            ["op": "frame", "id": 4, "x": 0, "y": 0, "w": 100, "h": 60], ["op": "frame", "id": 40, "x": 20, "y": -30, "w": 60, "h": 60],
            ["op": "frame", "id": 5, "x": 100, "y": 0, "w": 100, "h": 60], ["op": "frame", "id": 6, "x": 200, "y": 0, "w": 100, "h": 60],
            ["op": "frame", "id": 7, "x": 0, "y": 100, "w": 80, "h": 80], ["op": "frame", "id": 8, "x": 100, "y": 100, "w": 100, "h": 30],
            ["op": "frame", "id": 9, "x": 100, "y": 140, "w": 100, "h": 30], ["op": "frame", "id": 10, "x": 0, "y": 320, "w": 100, "h": 60],
            ["op": "frame", "id": 11, "x": 120, "y": 320, "w": 100, "h": 60],
            ["op": "frame", "id": 12, "x": 0, "y": 200, "w": 100, "h": 60],
            ["op": "frame", "id": 13, "x": 120, "y": 200, "w": 160, "h": 100], ["op": "frame", "id": 14, "x": 20, "y": 20, "w": 100, "h": 60],
        ]))
        let ids: [UInt32] = [4, 5, 6, 7, 8, 9, 10, 11, 12, 14]
        let views = try ids.map { try XCTUnwrap(p.views[$0], "\($0)") }
        for view in views {
            view.layer.displayIfNeeded()
            view.setExtension(UIEdgeInsets(top: 0, left: 0, bottom: 20, right: 0))
            XCTAssertNil(view.boxExtension, "\(view.props["testId"] ?? view.kind) shows no strip")
        }
        let (clipping, rounded, both, path, clippedText, text) = (views[0], views[1], views[2], views[3], views[4], views[5])
        let child = try XCTUnwrap(p.views[40])
        XCTAssertTrue(clipping.overflowClips)
        XCTAssertTrue(clipping.clipsToBounds && clipping.clipBox == nil && child.superview === clipping, "the box clips its children itself, as before")
        XCTAssertEqual(rgb(clipping.layer.backgroundColor), [255, 0, 0], "and keeps its own fill")
        XCTAssertTrue(rounded.clipsToBounds, "a scroller's box keeps its clip")
        XCTAssertEqual(try XCTUnwrap(rounded.scrollView).layer.cornerRadius, 0, "and its scroll view's corners as they were")
        XCTAssertNotNil(both.scrollView, "a view scrolling on both axes, whose own overflow clip is its scroll view's")
        XCTAssertNotNil(path.style["clip_path"])
        XCTAssertEqual(rgb(path.layer.backgroundColor), [255, 0, 0], "a box with a clip path keeps its own fill")
        XCTAssertTrue(clippedText.overflowClips)
        XCTAssertFalse(text.drawsPaint, "a text that rasters its glyphs, its ground on its layer")
        XCTAssertEqual(rgb(text.layer.backgroundColor), [255, 0, 0], "and keeps it")
        XCTAssertNotNil(views[8].style["mask_image"], "a box with a mask image")
        XCTAssertNotNil(views[9].vibrancyView, "a fill the platform's vibrancy draws")
        // Nor for a video or a native module, whose snapshot an agent's picture draws into the box's backing store, a 2D
        // canvas's bitmap, an SVG's scene, or a field's or a control's native view
        for kind in ["video", "native", "canvas2d", "svg", "input", "textarea", "control"] {
            let native = NodeView(id: 90, kind: kind, presenter: p)
            native.applyStyle(["background_color": [255, 0, 0, 255]])
            native.setExtension(UIEdgeInsets(top: 0, left: 0, bottom: 20, right: 0))
            XCTAssertNil(native.boxExtension, kind)
        }
    }

    /// A gradient is part of the picture the strip mirrors, and the box's border stays its own layer's, over the mirror.
    func testAGradientIsMirroredAndTheBorderIsNot() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the extension view draws in a scene's window") }
        guard #available(iOS 26, *) else { throw XCTSkip("UIBackgroundExtensionView needs iOS 26") }
        let black = [0, 0, 0, 255]
        let p = presenter(scene, box: ["background_image": ["linear": 180, "stops": [0, 0, 255, 0, 255, 1, 0, 0, 255, 255]],
                                       "border_width": 4, "border_color_top": black, "border_color_right": black,
                                       "border_color_bottom": black, "border_color_left": black])
        let window = try XCTUnwrap(window)
        let box = try XCTUnwrap(p.views[2])
        box.setExtension(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        window.layoutIfNeeded()
        let picture = try XCTUnwrap(box.boxExtension?.picture)
        XCTAssertTrue(box.boxGradient?.superlayer === picture.layer, "the gradient is the picture's")
        XCTAssertTrue(box.layer.borderWidth > 0 || box.boxBorder?.superlayer === box.layer, "the border is the box's own layer's")
        let image = capture(window)
        let strip = pixel(image, at: box.convert(CGPoint(x: 125, y: -10), to: window))
        XCTAssertTrue(strip[1] > 200 && strip[2] < 60, "the strip mirrors the gradient's green top: \(strip)")
        XCTAssertEqual(pixel(image, at: box.convert(CGPoint(x: 125, y: 2), to: window)), [0, 0, 0], "the border shows over the picture")
        // The CPU capture a canvas takes of a child keeps that order too, the extension under the box's own paint
        let bitmap = try XCTUnwrap(Bitmap.blank(width: 250, height: 200))
        Capture.draw(box, scale: 1, into: bitmap)
        let captured = try XCTUnwrap(bitmap.image)
        XCTAssertEqual(rgba(captured, at: CGPoint(x: 125, y: 2)), [0, 0, 0, 255], "the captured border shows over the picture")
        let inside = rgba(captured, at: CGPoint(x: 125, y: 10))
        XCTAssertTrue(inside[1] > 150 && inside[3] == 255, "and the captured picture inside it: \(inside)")
        XCTAssertNotNil(box.boxExtension, "a capture leaves the extension")
    }

    /// A box inside a canvas's overlay shows no strip, placed by the canvas's surface or not: the surface may draw it from
    /// a capture, which holds no UIKit mirror (LLP 1014 D5). Moved in, its extension ends as it paints; moved out, the
    /// extension asked for comes back; and it stays off while the canvas comes to clip one axis, its overlay moving into
    /// a clip box with the box in it.
    func testABoxUnderACanvasDoesNotExtend() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the extension view draws in a scene's window") }
        let p = presenter(scene)
        let box = try XCTUnwrap(p.views[2])
        p.apply(wireBatch([
            ["op": "create", "id": 4, "kind": "canvas", "props": ["testId": "canvas"]],
            ["op": "children", "id": 1, "ids": [2, 4]],
            ["op": "frame", "id": 4, "x": 0, "y": 320, "w": 300, "h": 80],
        ]))
        let canvas = try XCTUnwrap(p.views[4])
        XCTAssertNotNil(canvas.overlay, "the canvas holds its children in an overlay")
        box.setExtension(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        XCTAssertNotNil(box.boxExtension)
        p.apply(wireBatch([["op": "children", "id": 1, "ids": [4]], ["op": "children", "id": 4, "ids": [2]]]))
        box.layer.displayIfNeeded()
        XCTAssertTrue(box.canvasAbove === canvas)
        XCTAssertNil(box.boxExtension, "under the canvas, no strip")
        p.apply(wireBatch([["op": "style", "id": 4, "style": ["overflow_x": "clip"]]]))
        box.layer.displayIfNeeded()
        XCTAssertNotNil(canvas.clipBox, "the canvas clips one axis through a clip box")
        XCTAssertTrue(box.canvasAbove === canvas)
        XCTAssertNil(box.boxExtension, "still under the canvas, no strip")
        p.apply(wireBatch([["op": "style", "id": 4, "style": [:]], ["op": "children", "id": 4, "ids": []], ["op": "children", "id": 1, "ids": [2, 4]]]))
        box.layer.displayIfNeeded()
        XCTAssertNil(box.canvasAbove)
        XCTAssertNotNil(box.boxExtension, "out from under it, the extension asked for comes back")
    }

    /// A box under an ancestor with a filter shows no strip, since the host may show the ancestor's subtree as a picture,
    /// a capture, and does or not as it clips (LLP 1055.000 D14): a filter set outside a batch draws its first picture
    /// with the colour strip gone, a lone `drop-shadow()`, which the ancestor's layer casts itself, holds it off too, and
    /// the filter gone, the extension asked for comes back. Nor does a box with a filter of its own show a strip.
    func testABoxUnderAFilterOrWithItsOwnDoesNotExtend() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the extension view draws in a scene's window") }
        let identity: [Double] = [1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0]
        let program: [Double] = [0, 0, 0, 0, 1, 5, -1, -3, 0, 0, 10000, 10000, 0] + identity
        guard SvgFilterGPU.runs(program.map(Float.init)) else { throw missing("metal", "no GPU filter path") }
        // The colour strip, which a capture holds, so a picture drawn with it shows it
        NodeView.extensionMirrors = false
        let p = presenter(scene)
        let root = try XCTUnwrap(p.views[1]), box = try XCTUnwrap(p.views[2])
        box.setExtension(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        XCTAssertTrue(box.boxExtension?.view is ExtensionStrip)
        let white: BatchValue = [255, 255, 255, 255]
        root.applyStyle(["background_color": white, "filter": .object(["p": .array(program.map(BatchValue.number))])])
        try XCTUnwrap(p.boxFilters).render()
        XCTAssertEqual(root.boxFilter?.snapshots, true, "the root shows as a picture")
        XCTAssertNil(box.boxExtension, "under the filter, no strip")
        let picture = UIImage(cgImage: try XCTUnwrap(root.boxFilter?.picture.sublayers?.first?.contents as! CGImage?))
        let (w, h) = (picture.size.width, picture.size.height)
        XCTAssertEqual(rgba(picture, at: CGPoint(x: w * 0.4, y: h * 0.2)), [255, 255, 255, 255], "no strip above the box in the picture")
        XCTAssertEqual(rgba(picture, at: CGPoint(x: w * 0.4, y: h * 0.5)), [255, 0, 0, 255], "the box's own fill")
        // A lone `drop-shadow()`: the layer's own shadow, no picture, and still no strip
        let shadow: [Double] = [-6, -6, 12, 12, 1, 6, -1, -3, -6, -6, 12, 12, 0, 2, 2, 0, 0, 0, 0, 0, 0.5]
        root.applyStyle(["background_color": white, "filter": .object(["p": .array(shadow.map(BatchValue.number))])])
        box.layer.displayIfNeeded()
        XCTAssertEqual(root.boxFilter?.snapshots, false, "the root casts its shadow itself")
        XCTAssertNil(box.boxExtension, "under the shadow's filter, no strip either")
        root.applyStyle(["background_color": white])
        box.layer.displayIfNeeded()
        XCTAssertNotNil(box.boxExtension, "the filter gone, the extension asked for comes back")
        // Nor does a box with a filter of its own
        box.applyStyle(box.style.merging(["filter": .object(["p": .array(program.map(BatchValue.number))])]) { _, b in b })
        box.layer.displayIfNeeded()
        XCTAssertNotNil(box.boxFilter)
        XCTAssertNil(box.boxExtension, "its own filter holds the strip off")
    }

    /// An ancestor that clips one axis fits its clip to what its children paint (CSS Overflow 3 §3.1), the strip among
    /// it, as the strip comes and goes.
    func testAnAncestorClippingOneAxisFitsTheStrip() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the extension view draws in a scene's window") }
        let p = presenter(scene)
        // The box at the root's top, so its strip lies outside the root's bounds
        p.apply(wireBatch([["op": "style", "id": 1, "style": ["background_color": [255, 255, 255, 255], "overflow_x": "clip"]],
                           ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 250, "h": 200]]))
        let root = try XCTUnwrap(p.views[1]), box = try XCTUnwrap(p.views[2])
        let clip = try XCTUnwrap(root.clipBox, "the root clips its children on one axis")
        let top = { clip.convert(clip.bounds, to: box).minY }
        XCTAssertGreaterThan(top(), -40)
        box.setExtension(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        XCTAssertLessThanOrEqual(top(), -40, "its clip reaches the strip on the axis it leaves visible")
        box.setExtension(.zero)
        XCTAssertGreaterThan(top(), -40, "and leaves it once it goes")
    }

    /// While a layout transition's surface paints the box, its border with its picture, the strip is hidden and the
    /// picture's own layers, a fitted image's among them, go back on the box over the surface; when it ends the strip
    /// shows again with the picture in it; and an extension that ends meanwhile leaves the image over the surface.
    func testALayoutTransitionHidesTheStripUntilItEnds() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the extension view draws in a scene's window") }
        guard #available(iOS 26, *) else { throw XCTSkip("UIBackgroundExtensionView needs iOS 26") }
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let png = try XCTUnwrap(UIGraphicsImageRenderer(size: CGSize(width: 80, height: 40), format: format).image { context in
            UIColor.red.setFill(); context.fill(CGRect(x: 0, y: 0, width: 80, height: 40))
        }.pngData())
        let p = Presenter(), loader = RasterLoader(), resolver = AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()))
        defer { loader.shutdown() }
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 300, height: 400)
        p.viewport.frame = window.bounds; window.addSubview(p.viewport); window.makeKeyAndVisible()
        self.window = window
        let image = NodeView(id: 1, kind: "image", presenter: p)
        p.views[1] = image; p.viewport.addSubview(image)
        image.frame = CGRect(x: 0, y: 100, width: 80, height: 40)
        image.applyStyle(["background_color": [0, 0, 255, 255]])
        image.loadGeneration = 1
        loader.load(image, source: "data:image/png;base64," + png.base64EncodedString(), resolver: resolver)
        waitFor("the image's raster") { image.raster != nil }
        image.layer.displayIfNeeded()
        let fitted = try XCTUnwrap(image.imageLayer, "the image's picture is a layer")
        image.setExtension(UIEdgeInsets(top: 20, left: 0, bottom: 0, right: 0))
        let e = try XCTUnwrap(image.boxExtension), picture = try XCTUnwrap(e.picture)
        XCTAssertTrue(fitted.superlayer === picture.layer, "the fitted image is the picture's")
        XCTAssertFalse(e.view.isHidden)
        image.layoutResize = CGSize(width: 0, height: 10)
        image.applySurface()
        image.layer.displayIfNeeded()
        let surface = try XCTUnwrap(image.surface)
        let layers = image.layer.sublayers ?? []
        XCTAssertTrue(e.view.isHidden, "the surface paints the box meanwhile")
        XCTAssertTrue(image.pictureHost === image.layer)
        XCTAssertTrue(fitted.superlayer === image.layer, "on the box while the surface paints it")
        XCTAssertLessThan(try XCTUnwrap(layers.firstIndex { $0 === surface }), try XCTUnwrap(layers.firstIndex { $0 === fitted }), "over the surface")
        image.layoutResize = .zero
        image.applySurface()
        image.layer.displayIfNeeded()
        XCTAssertNil(image.surface)
        XCTAssertTrue(image.boxExtension === e && !e.view.isHidden, "the strip shows again")
        XCTAssertTrue(fitted.superlayer === picture.layer, "back in the picture")
        // Ended while a transition's surface paints the box, the extension leaves the image over the surface
        image.layoutResize = CGSize(width: 0, height: 10)
        image.applySurface()
        image.layer.displayIfNeeded()
        image.setExtension(.zero)
        let surface2 = try XCTUnwrap(image.surface), after = image.layer.sublayers ?? []
        XCTAssertNil(image.boxExtension)
        XCTAssertTrue(fitted.superlayer === image.layer)
        XCTAssertLessThan(try XCTUnwrap(after.firstIndex { $0 === surface2 }), try XCTUnwrap(after.firstIndex { $0 === fitted }), "still over the surface")
        image.layoutResize = .zero
        image.applySurface()
    }

    /// A CSS animation of the background colour plays where the fill paints: on the picture while the box extends, so
    /// the mirror follows it, and on the box's own layer again after; a strip in one colour, before iOS 26, waits for it.
    func testABackgroundColourAnimationPlaysWhereTheFillPaints() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the extension view draws in a scene's window") }
        guard #available(iOS 26, *) else { throw XCTSkip("UIBackgroundExtensionView needs iOS 26") }
        let p = presenter(scene)
        let box = try XCTUnwrap(p.views[2])
        let spec: [String: Any] = ["id": "paint", "k": "backgroundColor", "s": 0.0, "dl": 0.0, "d": 1.0, "n": 100.0,
                                   "t": [0.0, 1.0], "v": [[255, 0, 0, 255], [0, 0, 255, 255]], "c": [], "fill": 3.0]
        p.boxAnimations.animations(2, ["specs": [spec]], layer: box.layer, clock: 0)
        let animates = { (layer: CALayer) in layer.animationKeys()?.contains { (layer.animation(forKey: $0) as? CAPropertyAnimation)?.keyPath == "backgroundColor" } == true }
        XCTAssertTrue(animates(box.layer))
        box.setExtension(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        let picture = try XCTUnwrap(box.boxExtension?.picture)
        XCTAssertTrue(animates(picture.layer) && !animates(box.layer), "on the picture, which the strip mirrors")
        box.setExtension(.zero)
        XCTAssertTrue(animates(box.layer) && !animates(picture.layer), "back on the box")
        // The colour strip cannot follow it: it waits, and the batch whose `animations` op ends it looks again
        NodeView.extensionMirrors = false
        box.setExtension(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        XCTAssertNil(box.boxExtension)
        p.apply(wireBatch([["op": "animations", "id": 2, "specs": []]]))
        XCTAssertFalse(animates(box.layer))
        XCTAssertNotNil(box.boxExtension, "a strip once the animation is gone")
        // And a batch looks again at its start for an animation that went with no `animations` op, as the clock's seek
        // or a `visibility` change takes one off
        p.boxAnimations.animations(2, ["specs": [spec]], layer: box.layer, clock: 0)
        box.applyExtension()
        XCTAssertNil(box.boxExtension)
        p.boxAnimations.animations(2, ["specs": []], layer: box.layer, clock: 0)
        p.apply(wireBatch([]))
        XCTAssertNotNil(box.boxExtension, "the next batch gives the strip back")
    }

    /// An extension ends while the box draws its own picture into its backing store (a background clipped to the padding
    /// box, which Core Animation cannot say) and comes back when it no longer does, with no new outsets. A capture's
    /// display decides it as any other does, and the copy takes the box as that leaves it: a box that comes to draw itself
    /// (a border whose sides differ in colour) ends its extension when a capture is the first to paint it, and the copy
    /// shows the box's own fill and border, no picture over them. The CPU copy draws no strip past the border box, which
    /// UIKit's extension view mirrors itself.
    func testAnExtensionEndsWhileTheBoxDrawsItselfAndComesBack() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the extension view draws in a scene's window") }
        guard #available(iOS 26, *) else { throw XCTSkip("UIBackgroundExtensionView needs iOS 26") }
        let p = presenter(scene)
        let root = try XCTUnwrap(p.views[1]), box = try XCTUnwrap(p.views[2])
        box.setExtension(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        XCTAssertNotNil(box.boxExtension)
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["background_color": [255, 0, 0, 255], "background_clip": "padding-box"]]]))
        box.layer.displayIfNeeded()
        XCTAssertTrue(box.drawsPaint)
        XCTAssertNil(box.boxExtension, "drawn into its own backing store, the box has no strip")
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["background_color": [255, 0, 0, 255]]]]))
        box.layer.displayIfNeeded()
        XCTAssertFalse(box.drawsPaint)
        XCTAssertNotNil(box.boxExtension, "its layers paint it again, and the extension asked for comes back")
        let take = {
            let bitmap = try XCTUnwrap(Bitmap.blank(width: 300, height: 400))
            Capture.draw(root, scale: 1, into: bitmap)
            return try XCTUnwrap(bitmap.image)
        }
        XCTAssertEqual(rgba(try take(), at: CGPoint(x: 120, y: 200)), [255, 0, 0, 255], "the picture in the box")
        let (black, blue) = ([0, 0, 0, 255], [0, 0, 255, 255])
        p.apply(wireBatch([["op": "style", "id": 2, "style": [
            "background_color": [255, 0, 0, 255], "border_width": 4, "border_color_top": black, "border_color_right": blue,
            "border_color_bottom": blue, "border_color_left": blue,
        ]]]))
        XCTAssertNotNil(box.boxExtension, "kept until the box paints")
        let captured = try take()
        XCTAssertNil(box.boxExtension, "ended as the capture painted the box")
        XCTAssertTrue(box.boxDrawn, "the box draws itself")
        XCTAssertEqual(rgba(captured, at: CGPoint(x: 120, y: 102)), black, "the box's own top border, no picture over it")
        XCTAssertEqual(rgba(captured, at: CGPoint(x: 120, y: 200)), [255, 0, 0, 255], "the box's own fill")
    }

    /// Before iOS 26, only the background colour extends, as a strip painted once around the border box, taking no
    /// touches; a box with no background colour has none.
    func testBeforeIOS26TheColourExtendsAsAStrip() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the strip draws in a scene's window") }
        NodeView.extensionMirrors = false
        let p = presenter(scene, red: [255, 0, 0, 128])
        let window = try XCTUnwrap(window)
        let root = try XCTUnwrap(p.views[1]), box = try XCTUnwrap(p.views[2])
        let above = box.convert(CGPoint(x: 150, y: -10), to: window), inside = box.convert(CGPoint(x: 150, y: 150), to: window)
        let painted = pixel(capture(window), at: inside)
        box.setExtension(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        window.layoutIfNeeded()
        let e = try XCTUnwrap(box.boxExtension)
        XCTAssertTrue(e.view is ExtensionStrip && e.picture == nil)
        XCTAssertEqual(rgb(box.layer.backgroundColor), [255, 0, 0], "the box keeps its picture")
        let image = capture(window)
        XCTAssertEqual(pixel(image, at: inside), painted, "the box is painted once, as before")
        XCTAssertTrue(near(pixel(image, at: above), painted), "the strip in the box's colour, once: \(pixel(image, at: above))")
        XCTAssertTrue(window.hitTest(above, with: nil) === root, "the strip takes no touch")
        box.setExtension(.zero)
        XCTAssertNil(e.view.superview)
        window.isHidden = true
        let q = presenter(scene, red: nil)
        let bare = try XCTUnwrap(q.views[2])
        bare.setExtension(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        XCTAssertNil(bare.boxExtension, "no colour, no strip")
    }

    /// Before iOS 26, the strip around the border box in the box's colour, the whole border box left to the box's own
    /// fill, as an extension view leaves it to its content.
    func testTheColourStripPaintsAroundTheBorderBoxOnly() throws {
        let strip = ExtensionStrip(frame: CGRect(x: 0, y: 0, width: 100, height: 140))
        strip.colour = UIColor(red: 1, green: 0, blue: 0, alpha: 1).cgColor
        strip.place(around: CGRect(x: 0, y: 40, width: 100, height: 100))
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        format.opaque = false
        let image = UIGraphicsImageRenderer(bounds: strip.bounds, format: format).image { strip.layer.render(in: $0.cgContext) }
        XCTAssertEqual(pixel(image, at: CGPoint(x: 50, y: 20)), [255, 0, 0], "the strip has the box's colour")
        XCTAssertEqual(alpha(image, at: CGPoint(x: 50, y: 90)), 0, "the border box is left to the box")
    }

    /// A box that goes while it extends takes its extension with it, the record and its view, as the box alone holds
    /// them (`NodeView.extensions` holds neither): a box the batch destroys, which `forget()` leaves extended, keeps its
    /// extension no longer than itself.
    func testABoxThatGoesTakesItsExtensionWithIt() throws {
        guard #available(iOS 26, *) else { throw XCTSkip("UIBackgroundExtensionView needs iOS 26") }
        let p = Presenter()
        weak var box: NodeView?, record: BoxExtension?, mirror: UIView?
        autoreleasepool {
            let extended = NodeView(id: 50, kind: "view", presenter: p)
            extended.frame = CGRect(x: 0, y: 0, width: 100, height: 100)
            extended.applyStyle(["background_color": .array([.number(255), .number(0), .number(0), .number(255)])])
            extended.setExtension(UIEdgeInsets(top: 20, left: 0, bottom: 0, right: 0))
            (box, record, mirror) = (extended, extended.boxExtension, extended.boxExtension?.view)
            XCTAssertNotNil(record)
        }
        waitFor("the box goes") { box == nil }
        XCTAssertNil(record, "its extension's record with it")
        XCTAssertNil(mirror, "and the extension view")
        withExtendedLifetime(p) {}
    }

    /// The picture and the colour strip take the box's `dynamic-range-limit`, as its own layers do: an HDR fill asks
    /// for the range the limit allows on the layer that paints it.
    func testThePictureAndTheStripTakeTheBoxsColourRange() throws {
        guard #available(iOS 26, *) else { throw XCTSkip("preferredDynamicRange and UIBackgroundExtensionView need iOS 26") }
        let p = Presenter()
        let hdr: BatchValue = ["cs": [["s": "srgb-linear", "v": [4, 4, 4, 1]]], "c": [255, 255, 255, 255]]
        let box = NodeView(id: 50, kind: "view", presenter: p)
        box.frame = CGRect(x: 0, y: 0, width: 100, height: 100)
        box.applyStyle(["background_color": hdr, "dynamic_range_limit": .string("constrained")])
        box.setExtension(UIEdgeInsets(top: 20, left: 0, bottom: 0, right: 0))
        let picture = try XCTUnwrap(box.boxExtension?.picture)
        XCTAssertEqual(picture.layer.preferredDynamicRange, .constrainedHigh, "the picture paints the HDR fill")
        box.setExtension(.zero)
        NodeView.extensionMirrors = false
        box.setExtension(UIEdgeInsets(top: 20, left: 0, bottom: 0, right: 0))
        let strip = try XCTUnwrap(box.boxExtension?.view as? ExtensionStrip)
        XCTAssertEqual(strip.strip.preferredDynamicRange, .constrainedHigh, "and the strip its colour")
    }

    /// An extended box's inset shadow stays in the picture it was put in as the box paints its layers again, its shadows
    /// cast with them: no cast moves the caster, which a cast that inserts it again would put back under a layer laid
    /// beneath it.
    func testAnExtendedBoxsInsetShadowStaysWhereItIs() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the extension view draws in a scene's window") }
        guard #available(iOS 26, *) else { throw XCTSkip("UIBackgroundExtensionView needs iOS 26") }
        let p = presenter(scene, box: ["box_shadow": [["o": [0.0, 0.0], "b": 4.0, "s": 2.0, "c": [0, 0, 0, 255], "i": true]]])
        let box = try XCTUnwrap(p.views[2])
        box.setExtension(UIEdgeInsets(top: 20, left: 0, bottom: 0, right: 0))
        let caster = try XCTUnwrap(box.insetCaster), picture = try XCTUnwrap(box.boxExtension?.picture)
        XCTAssertTrue(caster.superlayer === picture.layer, "the inset shadow paints with the picture")
        let beneath = CALayer()
        picture.layer.insertSublayer(beneath, below: caster)
        box.applyBoxLayer()
        XCTAssertTrue(caster.superlayer === picture.layer)
        let order = try XCTUnwrap(picture.layer.sublayers)
        XCTAssertLessThan(try XCTUnwrap(order.firstIndex(of: beneath)), try XCTUnwrap(order.firstIndex(of: caster)), "the cast leaves the caster in place")
    }

    /// The window as it shows, at 1x (docs/testing.md, "Pixels").
    private func capture(_ window: UIWindow) -> UIImage {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        return UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { _ in window.drawHierarchy(in: window.bounds, afterScreenUpdates: true) }
    }

    /// Within 3 per channel: the mirror is UIKit's, sampled through its own layers
    private func near(_ a: [Int], _ b: [Int]) -> Bool { a.count == b.count && zip(a, b).allSatisfy { abs($0 - $1) <= 3 } }

    private func rgb(_ colour: CGColor?) -> [Int]? {
        guard let srgb = colour?.converted(to: CGColorSpace(name: CGColorSpace.sRGB)!, intent: .defaultIntent, options: nil),
              let c = srgb.components, c.count >= 3 else { return nil }
        return c.prefix(3).map { Int(($0 * 255).rounded()) }
    }

    private func pixel(_ image: UIImage, at point: CGPoint) -> [Int] { Array(rgba(image, at: point).prefix(3)) }

    private func alpha(_ image: UIImage, at point: CGPoint) -> Int? { rgba(image, at: point).last }

    private func rgba(_ image: UIImage, at point: CGPoint) -> [Int] {
        guard let cg = image.cgImage else { return [] }
        let x = Int(point.x * image.scale), y = Int(point.y * image.scale)
        var data = [UInt8](repeating: 0, count: 4)
        let context = CGContext(data: &data, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
                                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        context?.draw(cg, in: CGRect(x: -x, y: y - cg.height + 1, width: cg.width, height: cg.height))
        return data.map { Int($0) }
    }
}
#endif
