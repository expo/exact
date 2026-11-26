#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// A box's picture mirrored past its border box on AppKit (LLP 1123 D13's extension), driven directly (`setExtension`):
/// the extension is a subview of the box behind its children, its picture placed by hand at the border box, painting
/// once and taking no click.
final class BoxExtensionMacTests: XCTestCase {
    private var window: NSWindow?

    override func tearDown() {
        NodeView.extensionMirrors = {
            if #available(macOS 26.0, *) { return true }
            return false
        }()
        window?.close()
        window = nil
        super.tearDown()
    }

    /// A white page 300 by 400 in a window with box 2, 250 by 200 at 25, 100 in `fill`, holding 3, 60 by 60 at 20, 20,
    /// with `style` added to the box's.
    private func presenter(fill: [Int] = [255, 0, 0, 255], style: [String: Any] = [:]) throws -> (Presenter, NodeView, NodeView) {
        _ = NSApplication.shared
        let p = Presenter()
        p.viewport.frame = NSRect(x: 0, y: 0, width: 300, height: 400)
        let window = NSWindow(contentRect: p.viewport.frame, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        self.window = window
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [255, 255, 255, 255]]],
            ["op": "create", "id": 2, "kind": "view", "style": ["background_color": fill].merging(style) { $1 }],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 400],
            ["op": "frame", "id": 2, "x": 25, "y": 100, "w": 250, "h": 200],
            ["op": "frame", "id": 3, "x": 20, "y": 20, "w": 60, "h": 60],
        ]))
        window.orderFront(nil)
        return (p, try XCTUnwrap(p.views[2]), try XCTUnwrap(p.views[3]))
    }

    /// The colour of the window's content at `point`, down from its top, as the bitmap's channels, 0 to 255.
    private func pixel(at point: NSPoint) throws -> [Int] {
        let content = try XCTUnwrap(window?.contentView)
        let rep = try XCTUnwrap(content.bitmapImageRepForCachingDisplay(in: content.bounds))
        content.cacheDisplay(in: content.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / content.bounds.width
        let color = try XCTUnwrap(rep.colorAt(x: Int(point.x * scale), y: Int(point.y * scale)))
        return [color.redComponent, color.greenComponent, color.blueComponent].map { Int(($0 * 255).rounded()) }
    }

    private let outsets = NSEdgeInsets(top: 40, left: 25, bottom: 0, right: 0)

    /// On macOS 26 the picture moves into the extension's content view at the border box and the box's own layer stops
    /// painting it, so a translucent fill paints once; the extension lies behind the child, takes no click, and is no
    /// child of the box for its readers; and ending it gives the picture back to the box.
    func testAnExtendedBoxsPictureIsMirroredPastItsBorderBoxBehindItsChildren() throws {
        guard #available(macOS 26.0, *) else { throw XCTSkip("AppKit's extension view is macOS 26's") }
        let (p, box, child) = try presenter(fill: [255, 0, 0, 128])
        let window = try XCTUnwrap(self.window)
        let inside = try pixel(at: NSPoint(x: 150, y: 250))
        box.setExtension(outsets)
        let e = try XCTUnwrap(box.boxExtension)
        XCTAssertTrue(e.view is NSBackgroundExtensionView)
        XCTAssertEqual(e.view.frame, NSRect(x: -25, y: -40, width: 275, height: 240), "the border box outset")
        XCTAssertEqual(try XCTUnwrap(e.picture).frame, NSRect(x: 25, y: 40, width: 250, height: 200), "the picture at the border box")
        XCTAssertNil(box.layer?.backgroundColor, "the box's own layer stops painting the fill")
        XCTAssertNotNil(e.picture?.layer?.backgroundColor, "its picture paints it")
        XCTAssertEqual(try pixel(at: NSPoint(x: 150, y: 250)), inside, "the translucent fill painted once")
        XCTAssertEqual(box.heldSubviews, [child], "no child of the box for its readers")
        XCTAssertTrue(box.subviews.first === e.view, "behind the child")
        // Reaching as far as it extends, the box takes a click in the strip, and the child one over it
        p.setReach(2, outsets)
        XCTAssertEqual(e.view.frame, NSRect(x: -25, y: -40, width: 275, height: 240), "from the laid-out box")
        XCTAssertTrue(window.contentView?.hitTest(box.convert(NSPoint(x: 100, y: -20), to: nil)) === box, "a click in the strip is the box's")
        XCTAssertTrue(window.contentView?.hitTest(box.convert(NSPoint(x: 50, y: 50), to: nil)) === child)
        p.setReach(2, NSEdgeInsetsZero)
        box.setExtension(NSEdgeInsetsZero)
        XCTAssertNil(box.boxExtension)
        XCTAssertNil(e.view.superview)
        XCTAssertNotNil(box.layer?.backgroundColor, "the fill back on the box")
        XCTAssertEqual(try pixel(at: NSPoint(x: 150, y: 250)), inside)
    }

    /// Before macOS 26 a strip in the box's colour paints around the border box, the picture staying on the box: the
    /// strip shows the fill past the box, and the box's own pixels are as they were.
    func testTheStripPaintsTheColourAroundTheBox() throws {
        NodeView.extensionMirrors = false
        let (_, box, _) = try presenter()
        let inside = try pixel(at: NSPoint(x: 150, y: 250))
        XCTAssertEqual(try pixel(at: NSPoint(x: 150, y: 80)), [255, 255, 255], "the page above the box")
        box.setExtension(outsets)
        let strip = try XCTUnwrap(box.boxExtension?.view as? ExtensionStrip)
        XCTAssertNil(box.boxExtension?.picture)
        XCTAssertNotNil(box.layer?.backgroundColor, "the picture stays on the box")
        XCTAssertEqual(strip.colour, box.layer?.backgroundColor)
        XCTAssertEqual(try pixel(at: NSPoint(x: 150, y: 80)), inside, "the strip paints the fill above the box")
        XCTAssertEqual(try pixel(at: NSPoint(x: 150, y: 250)), inside)
        XCTAssertEqual(try pixel(at: NSPoint(x: 150, y: 50)), [255, 255, 255], "and not past its outsets")
    }

    /// A gradient is part of the picture the extension holds, and a border stays the box's own layer's.
    func testAGradientIsMirroredAndTheBorderIsNot() throws {
        guard #available(macOS 26.0, *) else { throw XCTSkip("AppKit's extension view is macOS 26's") }
        let (_, box, _) = try presenter(style: ["background_image": ["linear": 180, "stops": [0, 0, 255, 0, 255, 1, 0, 0, 255, 255]]])
        box.display()
        XCTAssertNotNil(box.boxGradient, "a gradient layer, which the box's display makes")
        box.setExtension(outsets)
        let picture = try XCTUnwrap(box.boxExtension?.picture?.layer)
        XCTAssertTrue(box.boxGradient?.superlayer === picture, "the gradient in the picture")
        XCTAssertEqual(box.boxGradient?.frame, picture.bounds)
        box.setExtension(NSEdgeInsetsZero)
        XCTAssertTrue(box.boxGradient?.superlayer === box.layer, "back on the box")
        window?.close()
        let black = [0, 0, 0, 255]
        let (_, bordered, _) = try presenter(style: ["border_width": 4, "border_color_top": black, "border_color_right": black,
                                                     "border_color_bottom": black, "border_color_left": black])
        bordered.setExtension(outsets)
        XCTAssertNotNil(bordered.boxExtension)
        XCTAssertTrue((bordered.layer?.borderWidth ?? 0) > 0 || bordered.boxBorder?.superlayer === bordered.layer, "the border on the box")
        XCTAssertNotNil(bordered.boxExtension?.picture?.layer?.backgroundColor, "the fill in the picture")
    }

    /// No extension shows for a box D13 leaves out, each keeping its fill on its own layer: one that clips its
    /// overflow, a scroller and one with a clip path.
    func testWhatShowsNoExtension() throws {
        for (name, style) in [("a clip", ["overflow_x": "hidden", "overflow_y": "hidden"]), ("a scroller", ["overflow_y": "scroll"]),
                              ("a clip path", ["clip_path": "inset(10px)"])] as [(String, [String: Any])] {
            let (_, box, _) = try presenter(style: style)
            box.setExtension(outsets)
            XCTAssertNil(box.boxExtension, name)
            window?.close()
        }
    }

    /// A box that reaches extends from its laid-out border box, not the frame its reach draws.
    func testAReachedBoxExtendsFromItsLaidOutBox() throws {
        let (p, box, _) = try presenter()
        p.setReach(2, NSEdgeInsets(top: 10, left: 0, bottom: 0, right: 0))
        box.setExtension(outsets)
        XCTAssertEqual(box.laidOutBox, NSRect(x: 0, y: 0, width: 250, height: 200))
        XCTAssertEqual(box.boxExtension?.view.frame, NSRect(x: -25, y: -40, width: 275, height: 240))
    }
}
#endif
