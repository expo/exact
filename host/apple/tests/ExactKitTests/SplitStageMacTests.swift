// @ref LLP 1127 D1, LLP 1072 T13 — the split's detail pane beside a batch prepared over display frames: the pane
// shows what the applied views show, and a row's press travels with the batch it made (`Batch.splitPress`).
#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

extension SplitMacTests {
    /// The library's root route shows placeholder A until `load` swaps it for B, a box of many plain texts, so the
    /// swap stages. Its list's rows: two push a book, `row-big` pushes a detail of many texts, `refresh` only counts,
    /// `tidy` removes itself, and `row-link` is a link whose `navigate` opens an unlinked big detail. The header's
    /// `counter` counts, `tally` shows both counts, and `show-early` mounts a list before it, its row `early-z` current.
    private func stagedSource() -> String {
        let lines = (1...16).map { "\"\($0)\"" }.joined(separator: ", ")
        var source = app(tabs: false)
            .replacingOccurrences(of: "notes \"/notes/:id\"", with: "notes \"/notes/:id\"\n    big \"/big/:id\"")
            .replacingOccurrences(of: "  action follow(url: string)\n    nav = go(nav, url)\n", with: """
              action follow(url: string)
                if url == "/to-c"
                  nav = push(nav, path("big", "c"))
                else
                  nav = go(nav, url)

            """)
            .replacingOccurrences(of: "  action count\n    counted = counted + 1\n", with: """
              action count
                counted = counted + 1
              action load
                ready = true
              action refresh
                refreshed = refreshed + 1
              action tidy
                tidied = true
              action showEarly
                early = true
              action openBig
                nav = push(nav, path("big", "x"))

            """)
            .replacingOccurrences(of: "  state counted = 0\n", with: "  state counted = 0\n  state ready = false\n  state refreshed = 0\n  state tidied = false\n  state early = false\n")
        source = replacing("text \"Library\" role=\"heading\"", with: [
            "text \"Library\" role=\"heading\" aria-level=1 flex=1 testId=\"title-library\"",
            "button press=load testId=\"load\"", "  text \"Load\"",
            "button press=count testId=\"counter\"", "  text \"Count\"",
            "text `${counted} ${refreshed}` testId=\"tally\"",
            "button press=showEarly testId=\"show-early\"", "  text \"Early\"",
        ], in: source)
        source = replacing("list testId=\"books\"", with: [
            "if !ready", "  column navigationSplit=\"placeholder\" testId=\"own-a\"", "    text \"Loading\"",
            "if ready", "  column navigationSplit=\"placeholder\" testId=\"own-b\"", "    for n in [\(lines)] key=n", "      text `Pick a book ${n}`",
            "if early", "  list testId=\"early\"", "    button press=follow(path(\"book\", \"z\")) aria-current=\"page\" testId=\"early-z\"", "      text \"Z\"",
            "list testId=\"books\" flex=1 min-height=0",
        ], in: source)
        source = replacing("for id in [\"a\", \"b\", \"c\"] key=id", with: [
            "button press=openBig testId=\"row-big\"", "  text \"Big\"",
            "button press=refresh testId=\"refresh\"", "  text \"Refresh\"",
            "if !tidied", "  button press=tidy testId=\"tidy\"", "    text \"Tidy\"",
            "link href=\"/to-c\" testId=\"row-link\"", "  text \"To C\"",
            "for id in [\"a\", \"b\"] key=id",
        ], in: source)
        source = replacing("link href=path(\"book\", id) testId=`row-${id}`", with: ["button press=follow(path(\"book\", id)) testId=`row-${id}`"], in: source)
        return replacing("if e.name == \"notes\"", with: [
            "if e.name == \"big\"", "  column navigationKey=`${e.id}` position=\"absolute\" inset=0 testId=\"route-big\"",
            "    for n in [\(lines)] key=n", "      text `Line ${n}`",
            "if e.name == \"notes\"",
        ], in: source)
    }

    /// The library's list painted and windowed, sixty rows long, each row a pressable box (a control's row is never
    /// rebound, LLP 1078) whose press pushes one detail of many texts that no link names, so its batch stages. The app
    /// marks row 40 current.
    private func reboundSource() -> String {
        let lines = (1...16).map { "\"\($0)\"" }.joined(separator: ", "), items = (0..<60).map(String.init).joined(separator: ", ")
        var source = app(tabs: false)
            .replacingOccurrences(of: "notes \"/notes/:id\"", with: "notes \"/notes/:id\"\n    big \"/big/:id\"")
            .replacingOccurrences(of: "  action count\n    counted = counted + 1\n", with: "  action count\n    counted = counted + 1\n  action openBig\n    nav = push(nav, path(\"big\", \"x\"))\n")
        source = replacing("list testId=\"books\"", with: ["list virtualized=true estimated-item-height=32 flex=1 min-height=0 testId=\"books\""], in: source)
        source = replacing("for id in [\"a\", \"b\", \"c\"] key=id", with: ["for id in [\(items)] key=id"], in: source)
        source = replacing("link href=path(\"book\", id) testId=`row-${id}`", with: ["row press=openBig aria-current=(id == 40 ? \"page\" : \"false\") testId=`row-${id}`"], in: source)
        return replacing("if e.name == \"notes\"", with: [
            "if e.name == \"big\"", "  column navigationKey=`${e.id}` position=\"absolute\" inset=0 testId=\"route-big\"",
            "    for n in [\(lines)] key=n", "      text `Line ${n}`",
            "if e.name == \"notes\"",
        ], in: source)
    }

    /// LLP 1078, LLP 1127 D16: a row's credit names the item its press picked, so once the list rebinds that row's
    /// view to another item (`renew`), no row is marked, the app's current one included, as the press still owns the
    /// detail, which keeps its place in the pane.
    func testAStagedPressCreditGoesWithItsItemWhenTheRowsViewShowsAnother() throws {
        let minimum = StagePlan.minimumOps
        StagePlan.minimumOps = 10
        defer { StagePlan.minimumOps = minimum }
        let session = try boot(tabs: false, size: CGSize(width: 1000, height: 500), source: reboundSource())
        session.runtime.useRowReuse(true)
        session.presenter.tables.painted = true
        _ = compose(ExactView(session: session), width: 1000, sidebar: false)
        let split = session.presenter.navigationHost.split, list = try node("books")
        settle("the painted list") { session.presenter.tables.feeds[list.id] == nil && has("row-0") }
        let a = try node("row-0")
        session.presenter.press(a.id)
        XCTAssertNotNil(session.stage, "the detail is prepared over frames")
        session.finishStage()
        XCTAssertTrue(try node("route-big").superview === split.detailPane)
        let marked = try XCTUnwrap((split.summary["marked"] as? Int).flatMap { session.presenter.views[UInt32($0)] }, "a row is marked")
        XCTAssertTrue(a.isDescendant(of: marked), "the pressed one")
        // The row's view leaves the window with nothing focused in it, and comes back as another item
        window?.makeFirstResponder(nil)
        let scroll = try XCTUnwrap(list.scroll), clip = scroll.contentView
        clip.scroll(to: NSPoint(x: 0, y: 1_500))
        scroll.reflectScrolledClipView(clip)
        settle("the pressed row's view rebound") { session.presenter.views[a.id].map { $0.props["testId"] != "row-0" } ?? false }
        XCTAssertTrue(has("row-40"), "the app's current row is mounted")
        XCTAssertTrue(split.summary["marked"] is NSNull, "no item the press did not pick is marked")
        XCTAssertTrue(try node("route-big").superview === split.detailPane, "the detail keeps its pane")
    }

    /// LLP 1127 D1: while a batch that swaps the root route's placeholder A for B is prepared, the pane keeps A,
    /// as the views applied show it, through a commit beside the stage and a row's press that writes no router, and
    /// shows B at the reveal. A row press whose own batch stages marks its row at the reveal, in the table too; a row
    /// whose press writes no router and removes itself leaves the detail's row selected; a link row whose `navigate`
    /// opens an unlinked detail is that detail's row; and a press's row counts only in its own list, so an earlier
    /// list that becomes the split's shows its current row.
    func testAPlaceholderSwapPreparedOverFramesShowsAtItsRevealAndARowsPressTravelsWithItsBatch() throws {
        let minimum = StagePlan.minimumOps
        StagePlan.minimumOps = 10
        defer { StagePlan.minimumOps = minimum }
        let session = try boot(tabs: false, size: CGSize(width: 1000, height: 500), source: stagedSource())
        let columns = compose(ExactView(session: session), width: 1000, sidebar: false)
        let split = session.presenter.navigationHost.split, pane = columns.detailPane
        let feed = try XCTUnwrap(session.presenter.tables.feeds[try node("books").id])
        func index(_ testId: String) -> Int? { (0..<feed.table.numberOfRows).first { feed.row(at: $0).flatMap(feed.pressing)?.props["testId"] == testId } }
        func pressRow(_ testId: String) throws { XCTAssertTrue(feed.pressRow(try XCTUnwrap(index(testId), testId)), testId) }
        let a = try node("own-a")
        XCTAssertTrue(a.superview === pane)

        // The swap stages; beside it a counter's commit and Refresh's press land, A stays, and no AppKit words show.
        session.presenter.press(try node("load").id)
        let swap = try XCTUnwrap(session.stage, "the swap is prepared over frames")
        session.presenter.press(try node("counter").id)
        try pressRow("refresh")
        XCTAssertTrue(session.stage === swap, "still preparing")
        XCTAssertEqual(try node("tally").props["text"], "1 1", "both commits landed beside the stage")
        XCTAssertTrue(a.superview === pane, "the pane keeps the placeholder the views show")
        XCTAssertNil(columns.detail.saysNothing)
        session.finishStage()
        XCTAssertTrue(try node("own-b").superview === pane, "the reveal shows B")

        // A row's press whose own batch stages marks its row at the reveal.
        try pressRow("row-big")
        XCTAssertNotNil(session.stage, "the detail is prepared over frames")
        session.finishStage()
        XCTAssertTrue(try node("route-big").superview === pane)
        let big = try node("row-big")
        XCTAssertEqual(split.summary["row"] as? Int, Int(big.id), "the pressed row is the detail's")
        XCTAssertEqual(feed.table.selectedRow, index("row-big"), "and the table's selected row")

        // With nothing staged, a row whose press writes no router and removes itself leaves the detail's row selected.
        StagePlan.minimumOps = minimum
        try pressRow("tidy")
        XCTAssertFalse(has("tidy"))
        XCTAssertEqual(split.summary["row"] as? Int, Int(big.id))
        XCTAssertEqual(feed.table.selectedRow, index("row-big"))

        // A link row whose `navigate` opens a detail no link names is that detail's row.
        try pressRow("row-link")
        XCTAssertEqual(split.summary["row"] as? Int, Int(try node("row-link").id), "the link's row is the detail's")

        // An earlier list that becomes the split's shows the row it marks current: the press's row was the other list's.
        session.presenter.press(try node("show-early").id)
        XCTAssertEqual(split.summary["row"] as? Int, Int(try node("early-z").id))
    }
}
#endif
