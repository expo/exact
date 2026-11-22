#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

extension SplitMacTests {
    func testWindowedSectionsExposeTheirHeadersAndLinksAsDistinctTableRows() throws {
        let books = (0..<64).map { "\"book-\($0)\"" }.joined(separator: ", ")
        let base = app(tabs: false).replacingOccurrences(of: "list testId=\"books\"", with: "list testId=\"books\" virtualized=true estimated-item-height=84")
        let bare = replacing("text `Book ${id}`", with: [], in: replacing("link href=path(\"book\", id)", with: [], in: base))
        let source = replacing("for id in [\"a\", \"b\", \"c\"] key=id", with: [
            "for id in [\(books)] key=id",
            "  section",
            "    header height=24",
            "      text `Shelf ${id}`",
            "    link href=path(\"book\", id) height=30 testId=`row-${id}`",
            "      text `Book ${id}`",
            "    link href=path(\"book\", `other-${id}`) height=30",
            "      text `Companion to ${id}`",
        ], in: bare)
        let session = try boot(tabs: false, size: CGSize(width: 1000, height: 500), source: source)
        _ = compose(ExactView(session: session), width: 1000, sidebar: false, window: ExactAgentWindow.self)
        window?.level = .agentBehind
        window?.orderFront(nil)
        let p = session.presenter, list = try node("books")
        let scroll = try XCTUnwrap(list.scroll)
        func jump(_ end: Bool) {
            let clip = scroll.contentView
            let endY = max(0, (scroll.documentView?.frame.height ?? 0) - clip.bounds.height)
            clip.scroll(to: CGPoint(x: 0, y: end ? endY : 0))
            scroll.reflectScrolledClipView(clip)
        }
        settle("first section mounted") { self.has("row-book-0") }
        XCTAssertTrue(p.navigationHost.split.active)
        let snapshot = try XCTUnwrap(p.collections.snapshot(list.id))
        XCTAssertEqual(snapshot.count, 64)
        XCTAssertLessThan(snapshot.rows.count, snapshot.count)
        jump(true)
        settle("last section mounted") { self.has("row-book-63") }
        XCTAssertFalse(has("row-book-0"))
        let feed = try XCTUnwrap(p.tables.feeds[list.id], "the split list uses AppKit while the runner windows its sections")
        XCTAssertEqual(feed.table.numberOfRows, 192)
        XCTAssertTrue(feed.tableView(feed.table, isGroupRow: 189))
        XCTAssertFalse(feed.tableView(feed.table, shouldSelectRow: 189))
        XCTAssertTrue(feed.tableView(feed.table, shouldSelectRow: 190))
        XCTAssertTrue(try XCTUnwrap(feed.table.rowView(atRow: 189, makeIfNecessary: true)).isGroupRowStyle)
        let last = try node("row-book-63")
        let cell = try XCTUnwrap(feed.table.view(atColumn: 0, row: 190, makeIfNecessary: true))
        XCTAssertTrue(last.isDescendant(of: cell))
        // The first jump uses estimates for sections the runner has never measured.
        settle("last section visible at the measured end") {
            jump(true)
            guard let document = scroll.documentView,
                  let row = feed.row(at: 190) else { return false }
            let lastFrame = last.convert(last.bounds, to: document)
            return abs(scroll.contentView.bounds.maxY - document.bounds.maxY) <= 1 &&
                lastFrame.maxY <= scroll.contentView.bounds.maxY && abs(lastFrame.minY - row.kernel.minY) <= 1
        }
        let tap = Agent(session: session).tap(["id": Int(last.id)])
        XCTAssertEqual(tap["delivery"] as? String, "platform", "\(tap)")
        settle("last link opened its detail") { self.has("route-book-book-63") }
        XCTAssertEqual(feed.table.selectedRow, 190)
        jump(false)
        settle("first section remounted") { self.has("row-book-0") }
        XCTAssertTrue(feed.tableView(feed.table, isGroupRow: 0))
        XCTAssertEqual(feed.table.selectedRow, 190)
    }
}
#endif
