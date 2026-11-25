#if os(macOS)
import AppKit
import ExactCore
import XCTest
@testable import ExactKit

/// LLP 1123 D14's writer on AppKit (`ScrollInsetsMac`).
final class ScrollInsetsMacTests: XCTestCase {
    private var window: NSWindow?

    override func tearDown() {
        window?.close()
        window = nil
        super.tearDown()
    }

    /// The content inset is the authored and held terms and the occupied depth, with no platform term,
    /// and the scroller insets put each scroller at its indicator inset beyond the content inset, each
    /// edge the nearest scroller's.
    func testTheContentInsetIsTheOccupiedDepthAndTheScrollersTakeTheIndicatorRule() {
        let band: [CGFloat] = [28, 0, 0, 0]
        var rows = ScrollInsetsMac.Rows()
        XCTAssertEqual(ScrollInsetsMac.insets(terms: .init(keyboard: [0, 0, 300, 0]), rows: rows, band: band),
                       ScrollInsetsMac(content: [28, 0, 300, 0], scrollers: [0, 0, 0, 0]), "the band and the keyboard, the scrollers with them")
        rows.authored = [10, 0, 0, 0]
        XCTAssertEqual(ScrollInsetsMac.insets(terms: .init(), rows: rows, held: [0, 0, 6, 0], band: band),
                       ScrollInsetsMac(content: [38, 0, 6, 0], scrollers: [-10, 0, -6, 0]), "the scrollers clear the band, not the authored or held terms")
        rows.indicatorsFollow = false
        XCTAssertEqual(ScrollInsetsMac.insets(terms: .init(), rows: rows, band: band).scrollers, [-38, 0, 0, 0], "under `none`, the edge")
        rows = ScrollInsetsMac.Rows()
        rows.verticalIndicator = [1, 5, 2, 3]
        rows.horizontalIndicator = [4, 9, 6, 7]
        XCTAssertEqual(ScrollInsetsMac.insets(terms: .init(), rows: rows, band: [0, 0, 0, 0]).scrollers, [1, 5, 6, 7],
                       "the top and the vertical scroller's side the vertical scroller's, the rest the horizontal's")
        XCTAssertEqual(ScrollInsetsMac.insets(terms: .init(), rows: rows, band: [0, 0, 0, 0], verticalSide: 3).scrollers, [1, 9, 6, 3])
        rows = ScrollInsetsMac.Rows()
        XCTAssertEqual(ScrollInsetsMac.insets(terms: .init(reserved: [40, nil, nil, nil]), rows: rows, band: band).content[0], 40, "a reservation")
        XCTAssertEqual(ScrollInsetsMac.insets(terms: .init(container: [false, true, true, true]), rows: rows, band: band).content[0], 0, "an ignored edge")
        XCTAssertEqual(ScrollInsetsMac.insets(terms: .init(), rows: rows, band: band, consumed: [8, 0, 0, 0]),
                       ScrollInsetsMac(content: [20, 0, 0, 0], scrollers: [8, 0, 0, 0]), "what a native layout clears, the scrollers still clear the band")
    }

    /// A write turns AppKit's adjustment off, puts the content and the vertical scroller where D14 says
    /// under the window's title bar, and keeps CSS `scrollTop`, or the end of a list that keeps it.
    func testAWriteClearsTheTitleBarAndKeepsTheScrollTop() throws {
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 240), styleMask: [.titled, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        self.window = window
        let owner = try XCTUnwrap(window.contentView)
        owner.layoutSubtreeIfNeeded()
        let band = owner.safeAreaInsets.top
        XCTAssertGreaterThan(band, 0, "a title bar over full-size content")
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["overflow_y": "scroll"]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 320, "h": 240], ["op": "content", "id": 1, "w": 320, "h": 3000],
        ]))
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        owner.addSubview(node)
        node.frame = owner.bounds
        node.layoutSubtreeIfNeeded()
        scroll.automaticallyAdjustsContentInsets = true
        scroll.contentView.scroll(to: NSPoint(x: 0, y: 100))
        scroll.reflectScrolledClipView(scroll.contentView)
        let scrollTop = { scroll.contentView.bounds.origin.y + scroll.contentInsets.top }
        let start = scrollTop()
        var rows = ScrollInsetsMac.Rows()
        rows.authored = [10, 0, 0, 0]
        ScrollInsetsMac.write(scroll, in: owner, terms: .init(), rows: rows)
        XCTAssertFalse(scroll.automaticallyAdjustsContentInsets)
        XCTAssertEqual(scroll.contentInsets.top, band + 10, accuracy: 0.01, "the band and the authored inset")
        scroll.tile()
        XCTAssertEqual(try XCTUnwrap(scroll.verticalScroller).frame.minY, band, accuracy: 0.01, "the scroller clears the band alone")
        XCTAssertEqual(scrollTop(), start, accuracy: 0.5, "CSS scrollTop kept")
        ScrollInsetsMac.write(scroll, in: owner, terms: .init(), rows: .init())
        XCTAssertEqual(scrollTop(), start, accuracy: 0.5, "and kept as the authored inset goes")

        // A list that follows its end and rests there rests at its new end
        node.props["scrollFollowEnd"] = "true"
        let end = { (scroll.documentView?.frame.height ?? 0) + scroll.contentInsets.bottom - scroll.contentView.bounds.height }
        scroll.contentView.scroll(to: NSPoint(x: 0, y: end()))
        scroll.reflectScrolledClipView(scroll.contentView)
        ScrollInsetsMac.write(scroll, in: owner, terms: .init(keyboard: [0, 0, 200, 0]), rows: .init())
        XCTAssertEqual(scroll.contentInsets.bottom, 200, accuracy: 0.01)
        XCTAssertEqual(scroll.contentView.bounds.origin.y, end(), accuracy: 0.5, "at its end under the keyboard")
    }

    /// Out of a window the band is unavailable and nothing is written.
    func testAWriteOutOfAWindowWritesNothing() {
        let owner = NSView(frame: NSRect(x: 0, y: 0, width: 100, height: 100)), scroll = NSScrollView(frame: owner.bounds)
        owner.addSubview(scroll)
        scroll.automaticallyAdjustsContentInsets = true
        ScrollInsetsMac.write(scroll, in: owner, terms: .init(), rows: .init())
        XCTAssertTrue(scroll.automaticallyAdjustsContentInsets)
    }
}
#endif
