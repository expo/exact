#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// A scroller's own bars lie on its scrollport on AppKit (LLP 1123 D12): mounted in a holder over its scroll view, they
/// keep their place as its content scrolls, take the clicks on them and leave the rest to the scroll view.
final class ScrollportBarsMacTests: XCTestCase {
    private var window: NSWindow?

    override func tearDown() {
        window?.close()
        window = nil
        super.tearDown()
    }

    func testABarOnTheScrollportKeepsItsPlaceAsTheContentScrolls() throws {
        _ = NSApplication.shared
        let p = Presenter()
        p.viewport.frame = NSRect(x: 0, y: 0, width: 300, height: 400)
        let window = NSWindow(contentRect: p.viewport.frame, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        self.window = window
        ScrollportBars.set([3], in: p)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 3, "kind": "view", "handlers": ["press"], "style": ["background_color": [0, 0, 0, 255]]],
            ["op": "create", "id": 4, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3, 4]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 400],
            ["op": "frame", "id": 2, "x": 0, "y": 50, "w": 300, "h": 300], ["op": "content", "id": 2, "w": 300, "h": 1000],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 300, "h": 44], ["op": "frame", "id": 4, "x": 0, "y": 44, "w": 300, "h": 900],
        ]))
        window.orderFront(nil)
        let scroller = try XCTUnwrap(p.views[2]), bar = try XCTUnwrap(p.views[3]), content = try XCTUnwrap(p.views[4])
        let scroll = try XCTUnwrap(scroller.scroll)
        let holder = try XCTUnwrap(bar.superview as? ScrollportHolder, "the bar in its scroller's holder")
        XCTAssertTrue(holder.superview === scroller && scroller.subviews.last === holder, "over the scroll view")
        XCTAssertTrue(content.isDescendant(of: scroll), "the content in the scroll view")
        XCTAssertEqual(holder.frame, scroller.bounds, "at the padding box")
        let barAt = bar.convert(bar.bounds, to: nil), contentAt = content.convert(content.bounds, to: nil)
        scroll.contentView.scroll(to: NSPoint(x: 0, y: 100))
        scroll.reflectScrolledClipView(scroll.contentView)
        XCTAssertEqual(bar.convert(bar.bounds, to: nil), barAt, "the bar keeps its place")
        XCTAssertNotEqual(content.convert(content.bounds, to: nil), contentAt, "the content scrolls under it")
        XCTAssertTrue(window.contentView?.hitTest(bar.convert(NSPoint(x: 150, y: 20), to: nil)) === bar, "a click on the bar is the bar's")
        XCTAssertTrue(window.contentView?.hitTest(scroller.convert(NSPoint(x: 150, y: 200), to: nil))?.isDescendant(of: scroll) ?? false,
                      "one below it the scroll view's")
        // No longer a bar, it goes back among the scroller's content
        ScrollportBars.set([], in: p)
        XCTAssertTrue(bar.isDescendant(of: scroll), "back in the scroll view")
        XCTAssertNil(holder.superview, "the holder gone")
        // Named again after its scroller's children mounted, it moves onto the scrollport
        ScrollportBars.set([3], in: p)
        XCTAssertTrue(bar.superview is ScrollportHolder, "on the scrollport again")
    }
}
#endif
