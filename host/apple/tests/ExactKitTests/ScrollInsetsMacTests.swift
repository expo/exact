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
        XCTAssertEqual(ScrollInsetsMac.insets(terms: .init(reserved: [-12, nil, nil, nil]), rows: rows, band: band),
                       ScrollInsetsMac(content: [0, 0, 0, 0], scrollers: [0, 0, 0, 0]), "a negative reservation occupies nothing")
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

    /// A window with full-size content under its title bar holding a presenter's `scroll` node, its scroll view at the
    /// node's frame, 320 by 240 by default, with content 3,000 points each way.
    private func titled(_ extra: [[String: Any]] = [], style: [String: Any] = ["overflow_x": "scroll", "overflow_y": "scroll"])
        throws -> (owner: NSView, presenter: Presenter, node: NodeView, scroll: ChainingScrollView)
    {
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 240), styleMask: [.titled, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        self.window = window
        let owner = try XCTUnwrap(window.contentView)
        owner.layoutSubtreeIfNeeded()
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": style], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 320, "h": 240], ["op": "content", "id": 1, "w": 3000, "h": 3000],
        ] + extra))
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        owner.addSubview(node)
        node.frame = owner.bounds
        node.layoutSubtreeIfNeeded()
        return (owner, p, node, scroll)
    }

    /// A write claims the scroll view: until its release, the host's chrome writer and the page's writer, which
    /// zeros a page's content insets, leave it, and the release gives back its adjustment and insets as they were,
    /// keeping CSS `scrollTop`, after which those writers write it again.
    func testAClaimKeepsTheHostsOtherWritersOffUntilItsRelease() throws {
        let (owner, p, _, scroll) = try titled()
        let band = owner.safeAreaInsets.top
        scroll.tile()
        XCTAssertEqual(scroll.scrollerInsets.top, band, accuracy: 0.01, "the chrome writer's, unclaimed")
        let before = (scroll.automaticallyAdjustsContentInsets, scroll.contentInsets, scroll.scrollerInsets)
        scroll.contentView.scroll(to: NSPoint(x: 40, y: 100))
        scroll.reflectScrolledClipView(scroll.contentView)
        let offset = { [scroll.contentView.bounds.origin.y + scroll.contentInsets.top, scroll.contentView.bounds.origin.x + scroll.contentInsets.left] }
        let start = offset()
        var rows = ScrollInsetsMac.Rows()
        rows.authored = [10, 0, 0, 0]
        ScrollInsetsMac.write(scroll, in: owner, terms: .init(), rows: rows)
        XCTAssertNotNil(ScrollInsetsMac.claim(of: scroll))
        let written = scroll.scrollerInsets
        XCTAssertEqual(written.top, -10, accuracy: 0.01)
        scroll.tile()
        XCTAssertTrue(NSEdgeInsetsEqual(scroll.scrollerInsets, written), "the chrome writer leaves a claimed scroller")

        let page = p.viewport
        owner.addSubview(page)
        page.frame = owner.bounds
        ScrollInsetsMac.write(page, in: owner, terms: .init(), rows: rows)
        page.tile()
        XCTAssertEqual(page.contentInsets.top, band + 10, accuracy: 0.01, "the page's writer leaves a claimed page")

        ScrollInsetsMac.release(scroll)
        XCTAssertNil(ScrollInsetsMac.claim(of: scroll))
        XCTAssertEqual(scroll.automaticallyAdjustsContentInsets, before.0)
        XCTAssertTrue(NSEdgeInsetsEqual(scroll.contentInsets, before.1) && NSEdgeInsetsEqual(scroll.scrollerInsets, before.2), "its own back")
        XCTAssertEqual(offset()[0], start[0], accuracy: 0.5, "CSS scrollTop kept")
        XCTAssertEqual(offset()[1], start[1], accuracy: 0.5, "and scrollLeft")
        ScrollInsetsMac.release(page)
        page.tile()
        XCTAssertEqual(page.contentInsets.top, 0, "the page's writer again")
        XCTAssertEqual(page.scrollerInsets.top, band, accuracy: 0.01, "and the chrome writer")
    }

    /// The band is the scroller's own overlap with the area outside its owner's safe area at its final frame: none
    /// for an interior scroller, a side's along that side, on either axis, and a scroller nested in a scrolled
    /// document takes the band once that scroll brings it under the title bar.
    func testTheBandIsTheScrollersOwnOverlapAtItsFinalFrame() throws {
        let (owner, _, node, scroll) = try titled()
        let band = owner.safeAreaInsets.top
        node.frame = NSRect(x: 0, y: 0, width: 320, height: 100)
        node.layoutSubtreeIfNeeded()
        ScrollInsetsMac.write(scroll, in: owner, terms: .init(), rows: .init())
        XCTAssertTrue(NSEdgeInsetsEqual(scroll.contentInsets, NSEdgeInsetsZero), "an interior scroller, below the title bar")

        owner.additionalSafeAreaInsets = NSEdgeInsets(top: 0, left: 30, bottom: 0, right: 0)
        node.frame = owner.bounds
        node.layoutSubtreeIfNeeded()
        scroll.contentView.scroll(to: NSPoint(x: 200, y: 0))
        scroll.reflectScrolledClipView(scroll.contentView)
        let left = scroll.contentView.bounds.origin.x + scroll.contentInsets.left
        ScrollInsetsMac.write(scroll, in: owner, terms: .init(), rows: .init())
        XCTAssertEqual(scroll.contentInsets.left, 30, accuracy: 0.01, "the side's band")
        XCTAssertEqual(scroll.contentInsets.top, band, accuracy: 0.01)
        XCTAssertEqual(scroll.contentView.bounds.origin.x + scroll.contentInsets.left, left, accuracy: 0.5, "CSS scrollLeft kept")
        owner.additionalSafeAreaInsets = NSEdgeInsetsZero

        let inner = NSScrollView(frame: NSRect(x: 0, y: 200, width: 320, height: 100))
        inner.documentView = NSView(frame: NSRect(x: 0, y: 0, width: 320, height: 1000))
        try XCTUnwrap(scroll.documentView).addSubview(inner)
        scroll.contentView.scroll(to: NSPoint(x: 0, y: -scroll.contentInsets.top))
        scroll.reflectScrolledClipView(scroll.contentView)
        ScrollInsetsMac.write(inner, in: owner, terms: .init(), rows: .init())
        XCTAssertEqual(inner.contentInsets.top, 0, "200 points down the document, clear of the title bar")
        scroll.contentView.scroll(to: NSPoint(x: 0, y: 200))
        scroll.reflectScrolledClipView(scroll.contentView)
        ScrollInsetsMac.write(inner, in: owner, terms: .init(), rows: .init())
        XCTAssertEqual(inner.contentInsets.top, band, accuracy: 0.01, "scrolled to the window's top, under the title bar")
    }

    /// Legacy and overlay scrollers take the same insets, the vertical one from the band down, and a table's header
    /// stays AppKit's: the clip view accommodates it below the content inset, which the writer neither writes nor
    /// counts.
    func testScrollerStylesAndAHeaderAreAppKits() throws {
        let (owner, _, _, scroll) = try titled()
        let band = owner.safeAreaInsets.top
        for style in [NSScroller.Style.overlay, .legacy] {
            scroll.scrollerStyle = style
            ScrollInsetsMac.write(scroll, in: owner, terms: .init(), rows: .init())
            scroll.tile()
            XCTAssertEqual(scroll.contentInsets.top, band, accuracy: 0.01, "\(style.rawValue)")
            XCTAssertEqual(try XCTUnwrap(scroll.verticalScroller).frame.minY, band, accuracy: 0.01, "\(style.rawValue)")
        }

        let table = NSTableView(frame: NSRect(x: 0, y: 0, width: 320, height: 2000))
        table.addTableColumn(NSTableColumn(identifier: .init("name")))
        let list = NSScrollView(frame: owner.bounds)
        list.documentView = table
        owner.addSubview(list)
        list.tile()
        let header = try XCTUnwrap(table.headerView)
        XCTAssertGreaterThan(header.frame.height, 0)
        ScrollInsetsMac.write(list, in: owner, terms: .init(), rows: .init())
        list.tile()
        XCTAssertTrue(list.contentView.automaticallyAdjustsContentInsets, "the clip view's own accommodation")
        XCTAssertEqual(list.contentInsets.top, band, accuracy: 0.01, "the band alone, not the header")
        let top = header.convert(header.bounds, to: owner)
        XCTAssertEqual(top.maxY, owner.safeAreaRect.maxY, accuracy: 0.5, "the header just under the title bar")
    }

    /// A zoomed child's placement is one of the held terms on a claimed scroller: the band adds to it, a new placement
    /// is written with the band, and the release gives the view the zoom's placement as it is then.
    func testAZoomsPlacementIsOneOfTheClaimsTerms() throws {
        let (owner, p, node, scroll) = try titled([
            ["op": "create", "id": 2, "kind": "view"], ["op": "children", "id": 1, "ids": [2]],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 100, "h": 100], ["op": "content", "id": 1, "w": 100, "h": 100],
        ], style: ["overflow_x": "scroll", "overflow_y": "scroll", "min_zoom": 1, "max_zoom": 4])
        let band = owner.safeAreaInsets.top
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 0, "y": 0, "w": 100, "h": 100]]))
        let zoom = try XCTUnwrap(scroll.zoom, "a zoomable scroller")
        let placed = zoom.placement
        XCTAssertTrue(NSEdgeInsetsEqual(scroll.contentInsets, placed), "unclaimed, the placement is the content inset")
        ScrollInsetsMac.write(scroll, in: owner, terms: .init(), rows: .init())
        XCTAssertEqual(scroll.contentInsets.top, placed.top + band, accuracy: 0.01, "the placement and the band")
        XCTAssertEqual(scroll.contentInsets.left, placed.left, accuracy: 0.01)
        scroll.magnification = 2
        node.placeZoom()
        XCTAssertNotEqual(zoom.placement.top, placed.top, "placed again at twice the size")
        XCTAssertEqual(scroll.contentInsets.top, zoom.placement.top + band, accuracy: 0.01, "written with the band")
        ScrollInsetsMac.release(scroll)
        XCTAssertTrue(NSEdgeInsetsEqual(scroll.contentInsets, zoom.placement), "the zoom's own placement, at twice the size")
    }

    /// At twice the magnification the offset is kept in the scroll view's points, the insets' units: the clip's 100 in
    /// the document is 200, and stays 200 as a 40-point inset comes.
    func testAMagnifiedScrollerKeepsItsOffsetInItsOwnPoints() throws {
        let (owner, _, _, scroll) = try titled()
        owner.additionalSafeAreaInsets = NSEdgeInsets(top: -owner.safeAreaInsets.top, left: 0, bottom: 0, right: 0)
        owner.layoutSubtreeIfNeeded()
        scroll.allowsMagnification = true
        scroll.magnification = 2
        ScrollInsetsMac.write(scroll, in: owner, terms: .init(), rows: .init())
        scroll.contentView.scroll(to: NSPoint(x: 0, y: 100))
        scroll.reflectScrolledClipView(scroll.contentView)
        let points = { scroll.contentView.bounds.origin.y * scroll.magnification + scroll.contentInsets.top }
        XCTAssertEqual(points(), 200, accuracy: 0.5)
        var rows = ScrollInsetsMac.Rows()
        rows.authored = [40, 0, 0, 0]
        ScrollInsetsMac.write(scroll, in: owner, terms: .init(), rows: rows)
        XCTAssertEqual(scroll.contentInsets.top, 40, accuracy: 0.01)
        XCTAssertEqual(points(), 200, accuracy: 0.5, "the offset kept in points")
    }

    /// A claim made while a zoom placed the content insets, released after the zoom has gone, leaves none: the placement
    /// was the zoom's, not the view's own.
    func testReleasingAfterTheZoomHasGoneLeavesNoPlacement() throws {
        let (owner, p, node, scroll) = try titled([
            ["op": "create", "id": 2, "kind": "view"], ["op": "children", "id": 1, "ids": [2]],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 100, "h": 100], ["op": "content", "id": 1, "w": 100, "h": 100],
        ], style: ["overflow_x": "scroll", "overflow_y": "scroll", "min_zoom": 1, "max_zoom": 4])
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 0, "y": 0, "w": 100, "h": 100]]))
        XCTAssertNotNil(scroll.zoom)
        ScrollInsetsMac.write(scroll, in: owner, terms: .init(), rows: .init())
        node.dropZoom(scroll)
        XCTAssertNil(scroll.zoom)
        ScrollInsetsMac.release(scroll)
        XCTAssertTrue(NSEdgeInsetsEqual(scroll.contentInsets, NSEdgeInsetsZero), "no zoom's placement back")
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
