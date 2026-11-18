#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// The presence ops on AppKit (LLP 1063): `exit` keeps a leaving view where
/// it was, above its old siblings, inert and out of every lookup, while the
/// engine's values still reach it, until the host's `destroy` ends it; a
/// `layout` present places the box outermost, from its top-left corner.
final class PresenceMacTests: XCTestCase {
    /// 1 → [2 → [3], 4], each row 50 points tall.
    private func fixture() -> Presenter {
        _ = NSApplication.shared
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "create", "id": 4, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2, 4]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 400.0, "h": 50.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0],
            ["op": "frame", "id": 4, "x": 0.0, "y": 50.0, "w": 400.0, "h": 50.0],
        ]))
        return p
    }

    func testResetEndsExitsBeforeAnIDIsReused() throws {
        let p = fixture()
        let old = try XCTUnwrap(p.views[2])
        let child = try XCTUnwrap(p.views[3])
        p.apply(wireBatch([["op": "exit", "id": 2]]))
        p.reset()
        XCTAssertTrue(p.leaving.isEmpty)
        XCTAssertNil(old.superview)
        XCTAssertNil(child.superview?.superview)
        p.apply(wireBatch([
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "roots", "ids": [2]],
        ]))
        let fresh = try XCTUnwrap(p.views[2])
        p.apply(wireBatch([["op": "destroy", "id": 2]]))
        XCTAssertNil(p.views[2])
        XCTAssertNil(fresh.superview)
    }

    func testPresenceInspectionKeepsTheSurfaceAndGhostUntilDestroy() throws {
        let p = fixture()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400),
                              styleMask: [.borderless], backing: .buffered, defer: false)
        window.contentView = p.viewport
        p.apply(wireBatch([["op": "present", "id": 2, "property": "layout", "x": 0.0, "y": 10.0, "w": 0.0, "h": -25.0]]))
        let moved = try XCTUnwrap(p.presenceObservation().first { $0["id"] as? Int == 2 })
        XCTAssertEqual(moved["h"] as? CGFloat, 25)
        XCTAssertEqual(moved["exiting"] as? Bool, false)
        p.apply(wireBatch([
            ["op": "exit", "id": 2],
            ["op": "present", "id": 2, "property": "opacity", "x": 0.25, "y": 0.0],
        ]))
        let ghost = try XCTUnwrap(p.presenceObservation().first { $0["id"] as? Int == 2 })
        XCTAssertEqual(ghost["opacity"] as? CGFloat, 0.25)
        XCTAssertEqual(ghost["exiting"] as? Bool, true)
        p.apply(wireBatch([["op": "destroy", "id": 2]]))
        XCTAssertFalse(p.presenceObservation().contains { $0["id"] as? Int == 2 })
    }

    func testALayoutSpringCannotGiveTheSurfaceANegativeSize() throws {
        let p = fixture()
        let v = try XCTUnwrap(p.views[3])
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 4.0, "y": 10.0, "w": -150.0, "h": -100.0]]))
        let surface = try XCTUnwrap(v.surface)
        XCTAssertEqual(surface.bounds.size, .zero)
        XCTAssertEqual(surface.frame.origin, .zero)
        XCTAssertEqual(v.bounds.size, CGSize(width: 100, height: 50))
    }

    func testABoxClosingToZeroWidthShowsTheWidthItClosesFrom() throws {
        let p = fixture()
        let v = try XCTUnwrap(p.views[3])
        p.apply(wireBatch([["op": "style", "id": 3, "style": ["overflow_x": "hidden", "overflow_y": "hidden"]]]))
        // Laid out closed, 200 wide on screen: the difference carries what
        // no ratio of a zero width could (GitHub #940).
        p.apply(wireBatch([
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 0.0, "h": 50.0],
            ["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 200.0, "h": 0.0],
        ]))
        let surface = try XCTUnwrap(v.surface)
        XCTAssertEqual(surface.frame, CGRect(x: 0, y: 0, width: 200, height: 50))
        XCTAssertEqual(surface.clip.path?.boundingBox.width ?? 0, 200, accuracy: 1e-9)
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 100.0, "h": 0.0]]))
        XCTAssertEqual(surface.frame.width, 100)
        XCTAssertEqual(surface.clip.path?.boundingBox.width ?? 0, 100, accuracy: 1e-9)
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0]]))
        XCTAssertNil(v.surface)
        // A frame after the layout op in its batch resizes the surface.
        p.apply(wireBatch([["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 200.0, "h": 50.0]]))
        p.apply(wireBatch([
            ["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 200.0, "h": 0.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 0.0, "h": 50.0],
        ]))
        XCTAssertEqual(v.surface?.frame.width, 200)
    }

    func testAnExitKeepsTheViewWhereItWasInertAndUnnamedUntilItsDestroy() throws {
        let p = fixture()
        let (parent, leaving, inner, sibling) = try (XCTUnwrap(p.views[1]), XCTUnwrap(p.views[2]), XCTUnwrap(p.views[3]), XCTUnwrap(p.views[4]))
        p.apply(wireBatch([
            ["op": "exit", "id": 2],
            ["op": "children", "id": 1, "ids": [4]],
            ["op": "frame", "id": 4, "x": 0.0, "y": 0.0, "w": 400.0, "h": 50.0],
        ]))
        XCTAssertNil(p.views[2], "out of every lookup by id")
        XCTAssertNil(p.views[3], "its subtree too")
        XCTAssertTrue(leaving.superview === parent, "still in the window")
        XCTAssertTrue(inner.superview === leaving)
        XCTAssertTrue(parent.subviews.first === leaving, "the ghost keeps its tree slot")
        XCTAssertGreaterThan(leaving.paintZPosition, sibling.paintZPosition)
        XCTAssertEqual(parent.paintRank, 1)
        XCTAssertEqual(leaving.frame, NSRect(x: 0, y: 0, width: 400, height: 50), "its last box")
        XCTAssertEqual(sibling.frame.minY, 0, "its sibling takes its place")
        XCTAssertTrue(leaving.routeInert)
        XCTAssertTrue(leaving.isAccessibilityHidden())
        p.apply(wireBatch([["op": "present", "id": 2, "property": "opacity", "x": 0.25, "y": 0.0]]))
        XCTAssertEqual(leaving.alphaValue, 0.25, accuracy: 1e-6, "the engine animates it")
        p.apply(wireBatch([["op": "destroy", "id": 2]]))
        XCTAssertNil(leaving.superview, "the exit ended")
        XCTAssertEqual(parent.paintRank, parent.rank)
        XCTAssertTrue(p.views[4] === sibling)
    }

    func testALeavingSvgIsDrawnUntilItsDestroy() throws {
        let p = fixture()
        let red = [255, 0, 0, 255]
        let scene: [String: Any] = ["box": [0, 0, 50, 50], "t": [1, 0, 0, 1, 0, 0],
            "els": [["id": 1, "n": 1, "o": 1, "p": [0, 0, 0, 1, 50, 0, 1, 50, 50, 1, 0, 50, 3], "f": red]]]
        // 5 leaves inside 2; 6 leaves itself.
        p.apply(wireBatch([
            ["op": "create", "id": 5, "kind": "svg"],
            ["op": "create", "id": 6, "kind": "svg"],
            ["op": "children", "id": 2, "ids": [3, 5]],
            ["op": "children", "id": 1, "ids": [2, 4, 6]],
            ["op": "frame", "id": 5, "x": 100.0, "y": 0.0, "w": 50.0, "h": 50.0],
            ["op": "frame", "id": 6, "x": 0.0, "y": 100.0, "w": 50.0, "h": 50.0],
            ["op": "svg", "id": 5, "scene": scene],
            ["op": "svg", "id": 6, "scene": scene],
        ]))
        let (inner, lone) = try (XCTUnwrap(p.views[5]), XCTUnwrap(p.views[6]))
        func drawn(_ view: NodeView) throws -> [Int] {
            let ctx = try XCTUnwrap(CGContext(data: nil, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            ctx.translateBy(x: -25, y: -25)
            try XCTUnwrap(view.layer).render(in: ctx)
            let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
            return (0..<4).map { Int(bytes[$0]) }
        }
        XCTAssertEqual(try drawn(inner), red)
        XCTAssertEqual(try drawn(lone), red)
        p.apply(wireBatch([
            ["op": "exit", "id": 2],
            ["op": "exit", "id": 6],
            ["op": "children", "id": 1, "ids": [4]],
        ]))
        XCTAssertEqual(try drawn(inner), red, "an svg inside a leaving view")
        XCTAssertEqual(try drawn(lone), red, "a leaving svg")
        p.apply(wireBatch([["op": "destroy", "id": 2], ["op": "destroy", "id": 6]]))
        XCTAssertEqual(try drawn(inner), [0, 0, 0, 0], "the exit ended")
        XCTAssertEqual(try drawn(lone), [0, 0, 0, 0])
    }

    func testAncestorExitDoesNotRetireOrdinaryDialogsOrPopovers() throws {
        for popover in [false, true] {
            let f = try SheetFixture(page: """
            component Proof
              view
                column testId="panel" width="100%" height="100%"
                  button testId="open" \(popover ? "popovertarget=\"held\"" : "commandfor=\"held\" command=\"show-modal\"")
                    text "Open"
                  \(popover ? "column popover=\"manual\"" : "dialog") id="held" testId="held" width=240 padding=20
                    input testId="field" value="Edit"
            """)
            defer { f.close() }
            try f.press("open")
            let panel = try f.node("panel"), held = try f.node("held"), field = try f.node("field")
            let container = try XCTUnwrap(held.superview)
            XCTAssertFalse(held.isDescendant(of: panel))
            f.presenter.apply(wireBatch([["op": "exit", "id": Int(panel.id)]]))
            XCTAssertTrue(f.presenter.views[held.id] === held)
            XCTAssertTrue(f.presenter.views[field.id] === field)
            if !popover { XCTAssertTrue(held.superview === container) }
            XCTAssertFalse(try XCTUnwrap(f.presenter.leaving[panel.id]).members.contains { $0 === held })
            if !popover { XCTAssertTrue(f.presenter.dialogs.active === held) }
            // A popover may close when its source retires; that does not retire the popover's nodes.
        }
    }

    func testALayoutPresentMovesTheBoxAndSizesOnlyItsSurface() throws {
        let p = fixture()
        let v = try XCTUnwrap(p.views[3])
        let clips = { v.clipsToBounds }
        let blue = [0, 0, 255, 255]
        p.apply(wireBatch([["op": "style", "id": 3, "style": ["background_color": blue, "border_radius_top_left": 6, "border_radius_top_right": 6, "border_radius_bottom_left": 6, "border_radius_bottom_right": 6, "overflow_x": "hidden", "overflow_y": "hidden"]]]))
        XCTAssertTrue(clips())
        // Grown from 25 to 50 high and moved down 10: it starts where it was.
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 4.0, "y": 10.0, "w": 0.0, "h": -25.0]]))
        // The layer's transform is about its origin, the flipped view's top-left.
        let corner = { (x: CGFloat, y: CGFloat) in CGPoint(x: x, y: y).applying(v.layer!.affineTransform()) }
        let mask = { v.layer?.mask }
        // Moved, never scaled: its content keeps its laid-out size.
        XCTAssertEqual(corner(0, 0).x, 4, accuracy: 1e-9)
        XCTAssertEqual(corner(0, 0).y, 10, accuracy: 1e-9)
        XCTAssertEqual(corner(100, 50).x, 104, accuracy: 1e-9)
        XCTAssertEqual(corner(100, 50).y, 60, accuracy: 1e-9)
        XCTAssertEqual(v.bounds.size, CGSize(width: 100, height: 50), "laid out at its final size")
        // Its surface is the shown size, from its top-left corner, and its
        // own is off; its children are clipped to the shown box.
        let surface = try XCTUnwrap(v.surface)
        XCTAssertEqual(surface.frame, CGRect(x: 0, y: 0, width: 100, height: 25))
        XCTAssertEqual(surface.backgroundColor?.components?.map { Double($0) }, [0, 0, 1, 1])
        XCTAssertEqual(surface.cornerRadius, 6)
        XCTAssertFalse(clips(), "the mask clips instead: the surface may outgrow the frame")
        XCTAssertTrue(mask() === surface.clip)
        XCTAssertEqual(surface.clip.path?.boundingBox.height ?? 0, 25, accuracy: 1e-9)
        XCTAssertEqual(surface.clip.path?.boundingBox.width ?? 0, 100, accuracy: 1e-9)
        // Shrinking past the frame: the surface is larger than the view.
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 0.0, "h": 25.0]]))
        XCTAssertEqual(surface.frame.height, 75)
        // At rest the node paints its own surface and clip again.
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0]]))
        XCTAssertNil(v.surface)
        XCTAssertNil(surface.superlayer)
        XCTAssertTrue(clips())
        XCTAssertNil(mask())
        XCTAssertTrue(v.layer!.affineTransform().isIdentity)
    }

    func testRemovingScrollingDuringAResizeRestoresVisibleOverflow() throws {
        let p = fixture(), card = try XCTUnwrap(p.views[2]), child = try XCTUnwrap(p.views[3])
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 500, height: 400),
                              styleMask: [.borderless], backing: .buffered, defer: false)
        window.contentView = p.viewport
        window.orderFront(nil)
        defer { window.orderOut(nil) }
        p.apply(wireBatch([
            ["op": "style", "id": 2, "style": ["border_radius": 20,
                "overflow_x": "auto", "overflow_y": "auto"]],
            ["op": "frame", "id": 3, "x": 380.0, "y": 0.0, "w": 100.0, "h": 50.0],
            ["op": "present", "id": 2, "property": "layout", "h": 100.0],
        ]))
        window.displayIfNeeded()
        waitForMainQueue()
        card.applySurface()
        let surface = try XCTUnwrap(card.surface), scroll = try XCTUnwrap(card.scroll)
        XCTAssertTrue(scroll.layer?.mask === surface.clip)
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["border_radius": 20,
            "overflow_x": "visible", "overflow_y": "visible"]]]))
        p.apply(wireBatch([["op": "present", "id": 2, "property": "layout", "h": 75.0]]))
        XCTAssertNotNil(card.surface, "the resize is still active")
        XCTAssertNil(card.scroll)
        XCTAssertTrue(child.superview === card)
        XCTAssertGreaterThan(child.frame.maxX, card.bounds.maxX)
        XCTAssertNil(scroll.layer?.mask, "the detached scroller releases the mask")
        XCTAssertNil(surface.clipped, "visible overflow has no clipping target")
        XCTAssertNil(card.layer?.mask, "the surface must not fall back to masking the host")
        XCTAssertFalse(card.clipsToBounds)
        p.apply(wireBatch([["op": "present", "id": 2, "property": "layout", "h": 0.0]]))
        XCTAssertNil(card.surface)
        XCTAssertNil(card.layer?.mask)
        XCTAssertFalse(card.clipsToBounds)
    }

    func testNativeContentClipsInsideAResizingBorder() throws {
        let p = fixture()
        for (id, kind, props) in [(10, "textarea", ["value": "Content"]),
                                  (11, "image", ["imageSource": "symbol:sf/square.fill", "symbolName": "square.fill"])] {
            p.apply(wireBatch([
                ["op": "create", "id": id, "kind": kind, "props": props,
                 "style": ["appearance": "none", "border_width": 6, "border_radius": 40,
                           "overflow_x": "hidden", "overflow_y": "hidden"]],
                ["op": "roots", "ids": [1, id]],
                ["op": "frame", "id": id, "x": 0.0, "y": 0.0, "w": 200.0, "h": 200.0],
                ["op": "present", "id": id, "property": "layout", "h": -100.0],
            ]))
            let node = try XCTUnwrap(p.views[UInt32(id)])
            let native = try XCTUnwrap(kind == "textarea" ? node.textAreaScroll : node.symbolClip)
            let surface = try XCTUnwrap(node.surface)
            XCTAssertTrue(native.superview === node.clipBox, "\(kind) belongs inside the padding-edge clip")
            XCTAssertTrue(native.superview?.layer?.mask === surface.clip)
            XCTAssertFalse(try XCTUnwrap(surface.clip.path).contains(CGPoint(x: 7, y: 7)))
            // Native content created after the clip must enter it too.
            if kind == "textarea" {
                node.textAreaScroll?.removeFromSuperview()
                node.makeTextArea()
            } else {
                node.clearSymbol()
                node.updateSymbol()
            }
            let replacement = try XCTUnwrap(kind == "textarea" ? node.textAreaScroll : node.symbolClip)
            XCTAssertTrue(replacement.superview === node.clipBox)
            p.apply(wireBatch([["op": "present", "id": id, "property": "layout", "h": 0.0]]))
            XCTAssertTrue(replacement.superview === node, "native content returns when the surface ends")
        }
    }

    func testASurfaceTheLayerCannotSayIsDrawnUprightAtTheShownSize() throws {
        let p = fixture()
        let v = try XCTUnwrap(p.views[3])
        let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 255])
        // Sides that differ: drawn, as the node's own `draw(_:)` draws them.
        p.apply(wireBatch([["op": "style", "id": 3, "style": ["border_width": 4, "border_color_top": red, "border_color_right": red, "border_color_bottom": blue, "border_color_left": red]]]))
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 0.0, "h": -25.0]]))
        let surface = try XCTUnwrap(v.surface)
        XCTAssertNotNil(surface.drawn)
        let (w, h) = (100, 25)
        let ctx = try XCTUnwrap(CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        // Its backing store, drawn as on screen, then composited.
        surface.displayIfNeeded()
        XCTAssertNotNil(surface.contents)
        surface.render(in: ctx)
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        let pixel = { (x: Int, y: Int) in (0..<4).map { Int(bytes[(y * w + x) * 4 + $0]) } }
        // The first row in memory is the top: red there, blue at the bottom.
        XCTAssertEqual(pixel(50, 1), red)
        XCTAssertEqual(pixel(50, h - 2), blue)
    }
}
#endif
