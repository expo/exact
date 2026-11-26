#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// A box that does not scroll reaches past its laid-out border box on AppKit (LLP 1123 D13): its view is drawn past it,
/// painting and taking clicks there, its children where layout put them, and its accessibility frame and content box
/// keep the laid-out box.
final class BoxReachMacTests: XCTestCase {
    private var window: NSWindow?

    override func tearDown() {
        window?.close()
        window = nil
        super.tearDown()
    }

    /// A page 300 by 400 in a window: 2, blue, 300 by 100 at 100 down, holding 3 at 10, 10.
    private func presenter() throws -> Presenter {
        _ = NSApplication.shared
        let p = Presenter()
        p.viewport.frame = NSRect(x: 0, y: 0, width: 300, height: 400)
        let window = NSWindow(contentRect: p.viewport.frame, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        self.window = window
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "style": ["background_color": [51, 102, 153, 255]]],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": [255, 255, 255, 255]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 400],
            ["op": "frame", "id": 2, "x": 0, "y": 100, "w": 300, "h": 100],
            ["op": "frame", "id": 3, "x": 10, "y": 10, "w": 50, "h": 20],
        ]))
        window.orderFront(nil)
        return p
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

    func testABoxReachesPastItsBoxAndItsChildrenStayWhereLayoutPutThem() throws {
        let p = try presenter()
        let window = try XCTUnwrap(self.window)
        let box = try XCTUnwrap(p.views[2]), child = try XCTUnwrap(p.views[3])
        let resting = child.convert(child.bounds, to: nil)
        p.setReach(2, NSEdgeInsets(top: 47, left: 0, bottom: 0, right: 0))
        XCTAssertEqual(box.frame, NSRect(x: 0, y: 53, width: 300, height: 147), "drawn 47 past its box's top")
        XCTAssertEqual(box.laidOutBox, NSRect(x: 0, y: 0, width: 300, height: 100))
        XCTAssertEqual(box.layoutBounds.origin, .zero, "the content box pointers read from")
        XCTAssertEqual(child.convert(child.bounds, to: nil), resting, "its child where layout put it")
        // The strip paints its background and takes the clicks there
        XCTAssertEqual(try pixel(at: NSPoint(x: 150, y: 70)), try pixel(at: NSPoint(x: 150, y: 150)), "its background in the strip")
        XCTAssertNotEqual(try pixel(at: NSPoint(x: 150, y: 40)), try pixel(at: NSPoint(x: 150, y: 150)), "and not above it")
        let strip = p.root.convert(NSPoint(x: 150, y: 70), to: nil)
        XCTAssertTrue(window.contentView?.hitTest(strip) === box, "a click in the strip is the box's")
        // Its accessibility frame is its laid-out box
        XCTAssertEqual(box.accessibilityFrame(), window.convertToScreen(p.root.convert(NSRect(x: 0, y: 100, width: 300, height: 100), to: nil)))
        // A new frame is drawn past by the same reach
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 0.0, "y": 100.0, "w": 300.0, "h": 120.0]]))
        XCTAssertEqual(box.frame, NSRect(x: 0, y: 53, width: 300, height: 167))
        XCTAssertEqual(child.convert(child.bounds, to: nil), resting)
        // Reach back to zero draws the box at its laid-out frame
        p.setReach(2, NSEdgeInsetsZero)
        XCTAssertEqual(box.frame, NSRect(x: 0, y: 100, width: 300, height: 120))
        XCTAssertEqual(box.bounds.origin, .zero)
        XCTAssertEqual(child.convert(child.bounds, to: nil), resting)
        // A box that starts to scroll keeps no reach of its own: its scroll view reaches instead
        p.setReach(2, NSEdgeInsets(top: 47, left: 0, bottom: 0, right: 0))
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["overflow_y": "auto", "background_color": [51, 102, 153, 255]]],
                           ["op": "frame", "id": 2, "x": 0.0, "y": 100.0, "w": 300.0, "h": 120.0]]))
        XCTAssertNotNil(box.scroll)
        XCTAssertEqual(box.frame, NSRect(x: 0, y: 100, width: 300, height: 120), "at its laid-out frame")
        XCTAssertEqual(box.bounds.origin, .zero)
        XCTAssertEqual(box.scroll?.frame, NSRect(x: 0, y: -47, width: 300, height: 167), "its scroll view reaching")
    }

    /// A scroller reaches with its scroll view: it grows past the box, keeping CSS `scrollTop` and `scrollLeft`, takes
    /// the clicks in the reached strip, and goes back to the box at a reach of zero.
    func testAScrollerReachesWithItsScrollView() throws {
        let p = try presenter()
        let window = try XCTUnwrap(self.window)
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["overflow_x": "scroll", "overflow_y": "scroll", "background_color": [51, 102, 153, 255]]],
                           ["op": "content", "id": 2, "w": 900, "h": 1000], ["op": "frame", "id": 2, "x": 0.0, "y": 100.0, "w": 300.0, "h": 100.0]]))
        let box = try XCTUnwrap(p.views[2]), scroll = try XCTUnwrap(box.scroll)
        XCTAssertEqual(scroll.frame, box.bounds)
        scroll.contentView.scroll(to: NSPoint(x: 30, y: 50))
        scroll.reflectScrolledClipView(scroll.contentView)
        let offset = { [scroll.contentView.bounds.origin.x + scroll.contentInsets.left, scroll.contentView.bounds.origin.y + scroll.contentInsets.top] }
        let start = offset()
        p.setReach(2, NSEdgeInsets(top: 47, left: 10, bottom: 0, right: 0))
        XCTAssertEqual(scroll.frame, NSRect(x: -10, y: -47, width: 310, height: 147), "grown past the box")
        XCTAssertEqual(box.frame, NSRect(x: 0, y: 100, width: 300, height: 100), "the box at its laid-out frame")
        XCTAssertEqual(offset(), start, "CSS scrollLeft and scrollTop kept")
        XCTAssertFalse(box.clipsToBounds, "the clip left to the scroll view")
        let strip = box.convert(NSPoint(x: 150, y: -20), to: nil)
        XCTAssertTrue(window.contentView?.hitTest(strip)?.isDescendant(of: scroll) ?? false, "a click in the strip is the scroller's")
        p.setReach(2, NSEdgeInsetsZero)
        XCTAssertEqual(scroll.frame, box.bounds)
        XCTAssertEqual(offset(), start)
        // At the end of its range the offset AppKit clamps to the reached port's range under the old insets comes back
        // with D14's write, whose insets follow the reach; and the reach adds no range of its own: a document no larger
        // than the box stays the box's size
        scroll.contentView.scroll(to: NSPoint(x: 600, y: 900))
        scroll.reflectScrolledClipView(scroll.contentView)
        let end = offset()
        p.setReach(2, NSEdgeInsets(top: 47, left: 10, bottom: 0, right: 0))
        ScrollInsetsMac.write(scroll, in: try XCTUnwrap(window.contentView), terms: ScrollTerms(reserved: [47, 0, 0, 10]), rows: .init())
        XCTAssertEqual(offset(), end, "the offset from before the reach")
        ScrollInsetsMac.release(scroll)
        p.setReach(2, NSEdgeInsetsZero)
        p.apply(wireBatch([["op": "content", "id": 2, "w": 300, "h": 100]]))
        p.setReach(2, NSEdgeInsets(top: 10, left: 10, bottom: 10, right: 10))
        XCTAssertEqual(try XCTUnwrap(scroll.documentView).frame.size, NSSize(width: 300, height: 100), "the laid-out port's floor")
    }
}
#endif
