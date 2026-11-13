// @ref LLP 1078 — the Mac rebinds a virtualized list's retiring row to the
// item the window needs next (RenewMac.swift). A rebound row must be what a
// fresh one would be: here two sessions of the same list scroll alike, one
// rebinding its rows and one building every row anew, and each item's row
// is compared between them, view by view, after state a person leaves on
// rows (a hover, a press, a nested scroller moved, a selection) has scrolled
// out of the window.
#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

final class RowReuseMacTests: XCTestCase {
    private var fixtures: [TableFixture] = []
    override func tearDown() { fixtures.forEach { $0.close() }; fixtures = []; super.tearDown() }

    /// Rows of two kinds of symbol, an animation on every fifth, a faded
    /// title on every third, a paragraph on every fourth, a `:hover` look,
    /// and a horizontal scroller that hears `scroll` and snaps, mandatory, to
    /// boxes whose places differ every third row.
    static func source(count: Int) -> String {
        var doublings = 0
        while 10 << doublings < count { doublings += 1 }
        let derived = (0..<doublings).map { "  derive d\($0 + 1) = \($0 == 0 ? "seed" : "d\($0)").concat(\($0 == 0 ? "seed" : "d\($0)"))" }.joined(separator: "\n")
        return """
        keyframes pulse
          from opacity=0.2
          to opacity=1

        component Rows
          state seed = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
          state opened = 0 - 1
          state scrolls = 0
        \(derived)
          derive rows = (\(doublings == 0 ? "seed" : "d\(doublings)")).map((x, i) => i).slice(0, \(count))
          action open(r: number)
            opened = r
          action scrolled(scrollLeft: number, scrollTop: number)
            scrolls = scrolls + 1
          view
            column width="100%" height="100%"
              text `opened ${opened} scrolled ${scrolls}` testId="status"
              list virtualized=true estimated-item-height=36 flex=1 min-height=0 width="100%" overflow-x="hidden" testId="rows"
                for r in rows key=`k${r}`
                  row press=open(r) testId=`row-${r}` aria-label=`Item ${r}` align-items="center" gap=8 padding="4px 12px" width="100%" box-sizing="border-box"
                    image (r % 2 == 0 ? "symbol:star" : "symbol:heart") width=16 height=16 testId=`icon-${r}` animation=(r % 5 == 0 ? "pulse 2s infinite" : "none")
                    column flex=1 min-width=0 testId=`body-${r}`
                      :hover
                        background-color="#ddeeff"
                      text `Row ${r}` testId=`title-${r}` opacity=(r % 3 == 0 ? 0.5 : 1)
                      if r % 4 == 0
                        text `The fourth row's paragraph, ${r}` font-size=11 testId=`fourth-${r}`
                    row overflow-x="auto" overflow-y="hidden" width="8rem" scroll=scrolled scroll-snap-type="x mandatory" testId=`strip-${r}`
                      text `A strip of words for row ${r}` white-space="nowrap" scroll-snap-align="start" testId=`words-${r}`
                      box width=(r % 3 == 0 ? "3rem" : "5rem") height=8 flex-shrink=0 scroll-snap-align="start"
                      box width="6rem" height=8 flex-shrink=0 scroll-snap-align="start" testId=`stop-${r}`
                      box width="6rem" height=8 flex-shrink=0

        """
    }

    /// What a person could tell of a row, view by view in order, without its ids.
    private func picture(_ row: NodeView, in f: TableFixture) -> [String] {
        var out: [String] = []
        func walk(_ v: NodeView) {
            let props = v.props.sorted { $0.key < $1.key }.map { "\($0.key)=\($0.value)" }.joined(separator: ",")
            let style = v.style.keys.sorted().map { "\($0)=\(v.style[$0]!.key)" }.joined(separator: ",")
            let frame = f.inDocument(v)
            let layer = v.layer
            var line = "\(v.kind) [\(props)] {\(style)} at \(frame.minX.rounded()),\(frame.minY.rounded()) \(frame.width.rounded())x\(frame.height.rounded())"
            line += " hidden \(v.isHidden) alpha \(v.alphaValue) presented \(v.presentedAlpha) transform \(layer.map { CATransform3DIsIdentity($0.transform) } ?? true)"
            line += " translate \(v.translatePx) layout \(v.layoutOffset) scale \(v.scale) rotate \(v.rotate)"
            line += " pressed \(v.pressed) hovered \(v.pressPaint.hovered) down \(v.pressPaint.down) lookBase \(v.pressPaint.lookBase != nil)"
            line += " image \(v.imageSource ?? "-") symbol \(v.hasSymbolLeaf) raster \(v.raster != nil)"
            line += " symbolStyle \(v.symbolView?.cell?.backgroundStyle.rawValue ?? -1) emphasized \(v.inEmphasizedRow)"
            line += " scrolled \(v.scroll?.contentView.bounds.origin ?? .zero)"
            line += " label \(v.accessibilityLabel() ?? "-") text \(v.accessibleText)"
            line += " animations \(layer?.animationKeys()?.filter { !$0.hasPrefix("exact-press") }.sorted() ?? [])"
            line += " hoverPath \(f.presenter.hoveredNodes.contains { $0 === v }) looked \(f.presenter.lookHovered.contains { $0.view === v })"
            out.append(line)
            for child in v.container.subviews.compactMap({ $0 as? NodeView }) { walk(child) }
        }
        walk(row)
        return out
    }

    /// The state a person leaves on rows near the top before scrolling
    /// away: the pointer over item 2, item 3 held down, item 4's strip
    /// snapped to its second box, item 5's wheeled and waiting to snap,
    /// item 6 selected (in a table, drawn emphasized).
    private func leaveState(_ f: TableFixture, table: Bool) throws {
        f.presenter.hover(try f.node("title-2"), true)
        try f.node("row-3").pressed = true
        let snapped = try f.node("strip-4"), strip = try XCTUnwrap(snapped.scroll)
        let stop = try f.node("stop-4").convert(NSPoint.zero, to: strip.documentView)
        strip.contentView.scroll(to: NSPoint(x: stop.x, y: 0))
        strip.reflectScrolledClipView(strip.contentView)
        snapped.snapAfterWheel(from: .zero, now: true)
        XCTAssertGreaterThan(strip.contentView.bounds.minX, 0, "item 4's strip rests on its second box")
        let wheeled = try f.node("strip-5"), waiting = try XCTUnwrap(wheeled.scroll)
        waiting.contentView.scroll(to: NSPoint(x: 20, y: 0))
        waiting.reflectScrolledClipView(waiting.contentView)
        wheeled.snapAfterWheel(from: .zero, now: false)
        if table {
            f.table.selectRowIndexes([6], byExtendingSelection: false)
            (f.table.rowView(atRow: 6, makeIfNecessary: true)?.view(atColumn: 0) as? TableRowCell)?.backgroundStyle = .emphasized
        }
        f.settle()
    }

    func testAReboundRowShowsItsItemAsAFreshRowDoes() throws {
        for table in [true, false] {
            let reused = try TableFixture(count: 2_000, table: table, source: Self.source(count: 2_000), reuse: true)
            let fresh = try TableFixture(count: 2_000, table: table, source: Self.source(count: 2_000), reuse: false)
            fixtures += [reused, fresh]
            try leaveState(reused, table: table)
            try leaveState(fresh, table: table)
            XCTAssertTrue(try reused.node("body-2").pressPaint.hovered,
                          "the hover look holds before the scroll")
            XCTAssertNotEqual(reused.status, "opened -1 scrolled 0", "the strips' scrolls were heard")
            let rows = reused.mounted
            let held = [("hovered", 2), ("pressed", 3), ("strip", 4), ("selected", 6)].map { ($0.0, rows[$0.1]!) }

            // Which item each view showed: a view seen with two items was rebound.
            var shown: [ObjectIdentifier: Int] = [:], rebound = Set<ObjectIdentifier>(), freshViews: [ObjectIdentifier: Int] = [:]
            var compared = 0
            func compare(_ note: String) throws {
                let a = reused.mounted, b = fresh.mounted
                for (item, row) in a {
                    if let was = shown.updateValue(item, forKey: ObjectIdentifier(row)), was != item { rebound.insert(ObjectIdentifier(row)) }
                }
                for (item, row) in b {
                    let was = freshViews.updateValue(item, forKey: ObjectIdentifier(row))
                    XCTAssertTrue(was == nil || was == item, "\(note): the fresh list never shows a view for two items")
                }
                let both = Set(a.keys).intersection(b.keys)
                XCTAssertGreaterThan(both.count, 10, "\(note): both lists mount the rows that show")
                for item in both.sorted() {
                    let left = picture(a[item]!, in: reused), right = picture(b[item]!, in: fresh)
                    XCTAssertEqual(left.count, right.count, "\(note): item \(item) has as many views")
                    for (l, r) in zip(left, right) where l != r {
                        XCTFail("\(note) (table \(table)): item \(item) rebound\(rebound.contains(ObjectIdentifier(a[item]!)) ? "" : " (not)") differs\n reused: \(l)\n fresh:  \(r)")
                    }
                    compared += 1
                }
            }
            try compare("at the top")
            for y in stride(from: CGFloat(300), through: 4_000, by: 337) {
                reused.scroll(to: y); fresh.scroll(to: y)
                try compare("at \(y)")
            }
            for y in stride(from: CGFloat(4_000), through: 0, by: -411) {
                reused.scroll(to: y); fresh.scroll(to: y)
                try compare("back at \(y)")
            }
            reused.scroll(to: 0); fresh.scroll(to: 0)
            try compare("at the top again")
            XCTAssertGreaterThan(compared, 200)
            XCTAssertGreaterThan(rebound.count, 20, "rows were rebound, not built")
            // The views that held the state were among them: each now shows
            // another item, as a fresh view for that item would.
            for (name, view) in held {
                XCTAssertTrue(rebound.contains(ObjectIdentifier(view)), "the \(name) row's view was rebound (destroyed: \(reused.presenter.views[view.id] !== view))")
            }
            XCTAssertEqual(reused.status, fresh.status, "a scroller put back at its start told its new item nothing")
            if table {
                XCTAssertEqual(reused.table.selectedRow, 6, "the selection is the item's, not the view's")
                XCTAssertEqual(reused.table.selectedRow, fresh.table.selectedRow)
            }
        }
    }

    /// A focused row is pinned: it stays mounted with its item and its
    /// focus wherever the list scrolls, and is never rebound.
    func testAFocusedRowKeepsItsItemAndItsFocus() throws {
        let f = try TableFixture(count: 2_000, table: false, source: Self.source(count: 2_000))
        fixtures.append(f)
        let row = try f.node("row-5")
        XCTAssertTrue(f.window.makeFirstResponder(row), "a row that presses takes the focus")
        f.settle()
        for y in stride(from: CGFloat(500), through: 6_000, by: 731) {
            f.scroll(to: y)
            XCTAssertTrue(f.presenter.views[row.id] === row, "at \(y): the focused row is mounted")
            XCTAssertEqual(row.props["testId"], "row-5", "at \(y): and still shows its item")
            XCTAssertTrue(f.window.firstResponder === row, "at \(y): and holds the focus")
        }
    }

    /// An image floating in Exact's picture-in-picture window, whose row is
    /// rebound to another item: the old item's image left the document, so
    /// the window closes, as at a destroy, and never shows the new item's
    /// picture. A rebound image of another source shows the new one.
    func testAFloatingImagesWindowClosesWhenItsRowIsRebound() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("row-reuse-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        for (name, color) in [("even.png", NSColor.systemRed), ("odd.png", NSColor.systemBlue)] {
            let rep = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 8, pixelsHigh: 8, bitsPerSample: 8, samplesPerPixel: 4,
                                                     hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0))
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
            color.setFill(); NSRect(x: 0, y: 0, width: 8, height: 8).fill()
            NSGraphicsContext.restoreGraphicsState()
            try XCTUnwrap(rep.representation(using: .png, properties: [:])).write(to: root.appendingPathComponent(name))
        }
        let previous = ExactApp.shared.assetRoot
        ExactApp.shared.assetRoot = root
        defer { ExactApp.shared.assetRoot = previous }
        let source = Self.source(count: 2_000).replacingOccurrences(
            of: #"image (r % 2 == 0 ? "symbol:star" : "symbol:heart")"#, with: #"image (r % 2 == 0 ? "even.png" : "odd.png")"#)
        let f = try TableFixture(count: 2_000, table: false, source: source)
        fixtures.append(f)
        func spin(_ note: String, _ done: () -> Bool) {
            let deadline = Date(timeIntervalSinceNow: 5)
            while !done() && Date() < deadline { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01)) }
            XCTAssertTrue(done(), note)
        }
        let picture = try f.node("icon-4"), row = try XCTUnwrap(f.mounted[4])
        spin("item 4's picture loads") { picture.raster != nil }
        PictureInPictureHost.request(picture, .init(["", "panel"]))
        spin("it floats") { PictureInPictureHost.floating.contains { $0.node === picture } }
        for y in stride(from: CGFloat(300), through: 4_000, by: 337) { f.scroll(to: y) }
        XCTAssertTrue(f.presenter.views[row.id] === row && f.item(row) != 4, "item 4's row was rebound to item \(f.item(row))")
        XCTAssertFalse(PictureInPictureHost.floating.contains { $0.node === picture }, "the old item's window closed with its image")
        let item = f.item(row), wanted = item % 2 == 0 ? "even.png" : "odd.png"
        XCTAssertEqual(picture.imageSource, wanted)
        spin("the new item's picture loads") { picture.raster != nil }
    }
}
#endif
