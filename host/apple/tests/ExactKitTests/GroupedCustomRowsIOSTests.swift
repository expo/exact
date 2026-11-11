#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

/// Custom grouped rows keep their views and input inside UIKit cells.
/// A simulator runs the native layout, highlight and selection assertions.
final class GroupedCustomRowsIOSTests: GroupedListFixture {
    override func tearDown() {
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    /// A stationary finger in window coordinates, independent of a carried
    /// row's frame and parent.
    private final class RowTouch: UITouch {
        let target: UIView
        var point: CGPoint
        init(_ target: UIView, at point: CGPoint = CGPoint(x: 100, y: 30)) {
            self.target = target; self.point = target.convert(point, to: nil); super.init()
        }
        override var view: UIView? { target }
        override func location(in view: UIView?) -> CGPoint { view.map { $0.convert(point, from: nil) } ?? point }
    }

    /// A custom row first, a standard row of the same symbol after it.
    func customFirst() -> GroupedListModel {
        GroupedListModel(style: "inset-grouped", sections: [
            .init(view: 2, header: nil, footer: nil, rows: [
                Row(view: 21, custom: true, pressable: true),
                Row(view: 10, symbol: "person.circle", title: "Profile", accessory: "disclosure", pressable: true),
                Row(view: 11, title: "Notifications", secondary: "On", accessory: "disclosure", pressable: true),
            ]),
        ])
    }

    func testACustomRowsSymbolIsAListCellsImageAndItsLineStartsUnderItsContent() throws {
        let p = presenter { self.customFirst() }
        // The kernel's boxes for a row of a leading symbol (the sheet's 24 by
        // 20, 16 in), its content inset 56 past it, spanning its card.
        p.apply(wireBatch([
            ["op": "create", "id": 30, "kind": "image", "props": ["imageSource": "symbol:person", "symbolName": "person.circle"], "style": ["font_size": 17.0, "object_fit": "contain"]],
            ["op": "create", "id": 31, "kind": "image", "props": ["imageSource": "symbol:forward-chevron", "symbolName": "chevron.forward"], "style": ["font_size": 17.0]],
            ["op": "children", "id": 21, "ids": [30, 31]],
            ["op": "frame", "id": 21, "x": 0.0, "y": 0.0, "w": 370.0, "h": 61.0],
            ["op": "frame", "id": 30, "x": 16.0, "y": 20.5, "w": 24.0, "h": 20.0],
            ["op": "frame", "id": 31, "x": 344.0, "y": 23.0, "w": 10.0, "h": 14.0],
        ]))
        let row = try XCTUnwrap(p.views[21]), leaf = try XCTUnwrap(p.views[30])
        row.kernelContentBox = CGRect(x: 56, y: 0, width: 298, height: 61)
        // The next batch carries the row again, which notes its line and lays
        // the list out again for it.
        p.apply(wireBatch([["op": "props", "id": 21, "set": ["testId": "custom"]]]))
        let l = try list(p)
        l.collection.layoutIfNeeded()
        let custom = try cell(p, 21), standard = try cell(p, 10)
        XCTAssertTrue(row.superview === custom.contentView)
        XCTAssertTrue(leaf.listSymbol, "the first child, a symbol")
        XCTAssertFalse(try XCTUnwrap(p.views[31]).listSymbol, "an accessory's chevron is no leading symbol")
        // UIKit draws a cell's image at the large scale of the body's size
        // (its `preferredSymbolConfiguration`), at its own size, centred.
        func images(_ v: UIView) -> [UIImageView] { (v as? UIImageView).map { [$0] } ?? v.subviews.flatMap(images) }
        let theirs = try XCTUnwrap(images(standard.contentView).first { $0.frame.width > 20 })
        let ours = try XCTUnwrap(leaf.symbolView)
        let drawn = try XCTUnwrap(ours.image).size
        XCTAssertEqual(drawn.width, theirs.bounds.width, accuracy: 0.5, "the symbol as wide as the standard row's")
        XCTAssertEqual(drawn.height, theirs.bounds.height, accuracy: 0.5, "and as tall")
        XCTAssertEqual(ours.contentMode, .center)
        XCTAssertFalse(ours.clipsToBounds, "its own size, past the sheet's box")
        XCTAssertEqual(ours.convert(ours.bounds, to: custom).midX, theirs.convert(theirs.bounds, to: standard).midX, accuracy: 0.5,
                       "centred where the standard row's is")
        // The line under the custom row starts under its content, where the
        // standard row's starts under its text.
        func lines(_ v: UIView) -> [UIView] { (String(describing: type(of: v)).contains("Separator") ? [v] : []) + v.subviews.flatMap(lines) }
        let found = lines(l.collection).map { $0.convert($0.bounds, to: l.collection) }
        let under = { (cell: UICollectionViewCell) -> CGRect? in
            let frame = cell.convert(cell.bounds, to: l.collection)
            return found.first { abs($0.maxY - frame.maxY) < 1.5 }.map { $0.offsetBy(dx: -frame.minX, dy: 0) }
        }
        let mine = try XCTUnwrap(under(custom), "a line under the custom row")
        let theirLine = try XCTUnwrap(under(standard), "a line under the standard row")
        XCTAssertEqual(mine.minX, 56, accuracy: 0.5, "under its content")
        XCTAssertEqual(mine.minX, theirLine.minX, accuracy: 0.5, "as the standard row's")
        // Its content moves in a batch that keeps its height: the line follows.
        row.kernelContentBox = CGRect(x: 16, y: 0, width: 338, height: 61)
        p.apply(wireBatch([["op": "props", "id": 21, "set": ["testId": "moved"]]]))
        l.collection.layoutIfNeeded()
        let moved = lines(l.collection).map { $0.convert($0.bounds, to: l.collection) }
        let frame = custom.convert(custom.bounds, to: l.collection)
        let line = try XCTUnwrap(moved.first { abs($0.maxY - frame.maxY) < 1.5 })
        XCTAssertEqual(line.minX - frame.minX, 16, accuracy: 0.5, "the line where the content now starts")
    }

    /// A leading symbol stands at the row's inset, its content's edge, and the
    /// line starts at the text after it, as UIKit's does beside a cell's image;
    /// an accessory after the symbol is no text.
    func testTheLineStartsAtTheTextAfterALeadingSymbol() throws {
        let p = presenter { self.customFirst() }
        p.apply(wireBatch([
            ["op": "create", "id": 30, "kind": "image", "props": ["imageSource": "symbol:person", "symbolName": "person.circle"], "style": ["font_size": 17.0, "object_fit": "contain"]],
            ["op": "create", "id": 32, "kind": "text", "props": ["text": "Profile"]],
            ["op": "create", "id": 31, "kind": "image", "props": ["imageSource": "symbol:forward-chevron", "symbolName": "chevron.forward"], "style": ["font_size": 17.0]],
            ["op": "children", "id": 21, "ids": [30, 32, 31]],
            ["op": "frame", "id": 21, "x": 0.0, "y": 0.0, "w": 370.0, "h": 61.0],
            ["op": "frame", "id": 30, "x": 16.0, "y": 20.5, "w": 24.0, "h": 20.0],
            ["op": "frame", "id": 32, "x": 55.0, "y": 21.0, "w": 200.0, "h": 19.0],
            ["op": "frame", "id": 31, "x": 344.0, "y": 23.0, "w": 10.0, "h": 14.0],
        ]))
        let row = try XCTUnwrap(p.views[21])
        row.kernelContentBox = CGRect(x: 16, y: 0, width: 338, height: 61)
        p.apply(wireBatch([["op": "props", "id": 21, "set": ["testId": "custom"]]]))
        XCTAssertEqual(try XCTUnwrap(try list(p).lineStart(21)), 55, accuracy: 0.5, "at the text, past the symbol")
        p.apply(wireBatch([["op": "children", "id": 21, "ids": [30, 31]]]))
        XCTAssertEqual(try XCTUnwrap(try list(p).lineStart(21)), 16, accuracy: 0.5, "a chevron is no text: the content's edge")
    }

    /// Right to left the kernel mirrors the row's insets, and the line starts
    /// past its symbol from the right; a row the model drops takes its mark
    /// with it.
    func testRightToLeftTheLineIsTheKernelsAndADroppedRowTakesItsMark() throws {
        var dropped = false
        let p = presenter(style: ["direction": "rtl"]) { dropped ? GroupedListModel(style: "inset-grouped", sections: [.init(view: 2, header: nil, footer: nil, rows: [
            Row(view: 10, symbol: "person.circle", title: "Profile", accessory: "disclosure", pressable: true),
        ])]) : self.customFirst() }
        p.apply(wireBatch([
            ["op": "create", "id": 30, "kind": "image", "props": ["imageSource": "symbol:person", "symbolName": "person.circle"], "style": ["font_size": 17.0, "object_fit": "contain"]],
            ["op": "children", "id": 21, "ids": [30]],
            ["op": "frame", "id": 21, "x": 0.0, "y": 0.0, "w": 370.0, "h": 61.0],
            ["op": "frame", "id": 30, "x": 330.0, "y": 20.5, "w": 24.0, "h": 20.0],
        ]))
        let row = try XCTUnwrap(p.views[21]), image = try XCTUnwrap(p.views[30])
        row.kernelContentBox = CGRect(x: 16, y: 0, width: 298, height: 61)
        let l = try list(p)
        XCTAssertEqual(l.collection.effectiveUserInterfaceLayoutDirection, .rightToLeft, "the list's direction")
        p.apply(wireBatch([["op": "props", "id": 21, "set": ["testId": "custom"]]]))
        l.collection.collectionViewLayout.invalidateLayout()
        l.collection.layoutIfNeeded()
        // Where UIKit draws it is not read here: this fixture's window has no
        // scene, and the list layout's environment stays left to right in it.
        XCTAssertEqual(try XCTUnwrap(try list(p).lineStart(21)), 56, accuracy: 0.5, "from the right, past the symbol")
        XCTAssertTrue(image.listSymbol)
        dropped = true
        p.apply(wireBatch([["op": "props", "id": 21, "set": ["testId": "dropped"]]]))
        XCTAssertFalse(image.listSymbol, "the row the model dropped takes its mark")
    }

    /// An image its author sized or fitted is left as laid out, and a mark
    /// goes with the list.
    func testAnAuthoredImageIsLeftAsLaidOutAndAMarkGoesWithTheList() throws {
        let p = presenter { self.customFirst() }
        p.apply(wireBatch([
            ["op": "create", "id": 30, "kind": "image", "props": ["imageSource": "symbol:person", "symbolName": "person.circle"], "style": ["font_size": 17.0, "object_fit": "cover"]],
            ["op": "children", "id": 21, "ids": [30]],
            ["op": "frame", "id": 30, "x": 16.0, "y": 20.5, "w": 24.0, "h": 20.0],
        ]))
        let image = try XCTUnwrap(p.views[30])
        XCTAssertFalse(image.listSymbol, "its author's fit")
        p.apply(wireBatch([["op": "style", "id": 30, "style": ["font_size": 17.0, "object_fit": "contain", "written": ["width", "height"]]],
                           ["op": "frame", "id": 30, "x": 16.0, "y": 14.0, "w": 32.0, "h": 32.0]]))
        XCTAssertFalse(image.listSymbol, "its author's size")
        // The sheet's slot, at whatever size its ramps give it.
        p.apply(wireBatch([["op": "style", "id": 30, "style": ["font_size": 17.0, "object_fit": "contain"]],
                           ["op": "frame", "id": 30, "x": 16.0, "y": 16.5, "w": 32.11, "h": 26.76]]))
        XCTAssertTrue(image.listSymbol, "the sheet's slot")
        // A fit its author wrote, even the sheet's own value, is theirs.
        p.apply(wireBatch([["op": "style", "id": 30, "style": ["font_size": 17.0, "object_fit": "contain", "written": ["object_fit"]]]]))
        XCTAssertFalse(image.listSymbol, "its author's contain")
        p.apply(wireBatch([["op": "style", "id": 30, "style": ["font_size": 17.0, "object_fit": "contain"]]]))
        XCTAssertTrue(image.listSymbol)
        // At an accessibility size too, drawn at UIKit's smaller size there.
        window.traitOverrides.preferredContentSizeCategory = .accessibilityLarge
        window.layoutIfNeeded() // traits reach the views at layout
        p.apply(wireBatch([["op": "props", "id": 21, "set": ["testId": "large"]]]))
        XCTAssertTrue(image.listSymbol, "an accessibility size")
        let base = UIImage.SymbolConfiguration(pointSize: 33)
        XCTAssertEqual(image.symbolShape(base), base.applying(UIImage.SymbolConfiguration(pointSize: 25.14)).applying(UIImage.SymbolConfiguration(scale: .large)))
        window.traitOverrides.preferredContentSizeCategory = .extraExtraExtraLarge
        window.layoutIfNeeded()
        p.apply(wireBatch([["op": "props", "id": 21, "set": ["testId": "xxxl"]]]))
        XCTAssertTrue(image.listSymbol, "the largest size short of them")
        host(p).lists[1]?.remove()
        XCTAssertFalse(image.listSymbol, "gone with the list")
    }

    /// A cell UIKit gives to another custom row takes that row's height,
    /// which is no resize: the layout already knows each row's height. When
    /// it was one, two cells handed back and forth between rows of two
    /// heights at a long list's bottom edge laid the list out again on every
    /// turn, and Hackers' feed spun a core at rest (LLP 1141.001 D13).
    func testACellTakingAnotherRowsHeightLaysNothingOutAgain() throws {
        let p = presenter {
            var m = self.model()
            m.sections[0].header = nil
            m.sections[0].footer = nil
            m.sections[0].rows[0] = Row(view: 10, custom: true, pressable: true)
            m.sections[0].rows[1] = Row(view: 11, custom: true, pressable: true)
            return m
        }
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 16.0, "y": 0.0, "w": 354.0, "h": 85.0],
                           ["op": "frame", "id": 11, "x": 16.0, "y": 85.0, "w": 354.0, "h": 79.0]]))
        let l = try list(p)
        // (Not `cell`: its XCTUnwrap records a failure while a cell is on its way.)
        let listCell = { (id: UInt32) -> UICollectionViewListCell? in l.collection.layoutIfNeeded(); return l.cell(id) as? UICollectionViewListCell }
        waitFor("the cells take their rows' heights and the list is laid out") {
            [10, 11].map { (listCell($0) as? GroupedCell)?.height } == [85, 79] && !l.resized
        }
        let a = try XCTUnwrap(try cell(p, 10) as? GroupedCell), b = try XCTUnwrap(try cell(p, 11) as? GroupedCell)
        let pathA = try XCTUnwrap(l.collection.indexPath(for: a)), pathB = try XCTUnwrap(l.collection.indexPath(for: b))
        // UIKit shows each cell for the other's row, then gives them back.
        l.collectionView(l.collection, willDisplay: a, forItemAt: pathB)
        l.collectionView(l.collection, willDisplay: b, forItemAt: pathA)
        XCTAssertTrue(try XCTUnwrap(p.views[11]).superview === a.contentView)
        XCTAssertEqual(a.height, 79, "the cell takes its new row's height")
        XCTAssertFalse(l.resized, "which the layout already has")
        l.collectionView(l.collection, willDisplay: a, forItemAt: pathA)
        l.collectionView(l.collection, willDisplay: b, forItemAt: pathB)
        XCTAssertFalse(l.resized)
        // A row whose own height changes still lays the list out again.
        p.apply(wireBatch([["op": "frame", "id": 11, "x": 16.0, "y": 85.0, "w": 354.0, "h": 120.0]]))
        waitFor("row 11's cell grows to 120") { listCell(11).map { abs($0.bounds.height - 120) <= 0.5 } ?? false }
    }

    /// A row inserted at the top of a list of custom rows: the shown rows
    /// stay in their cells through the batch, as a collection view's cells
    /// do under an insert, and go home in the order the batch gave them.
    func testARowInsertedAboveCustomRowsLeavesTheShownOnesInTheirCells() throws {
        var inserted = false
        let p = presenter {
            var m = self.model()
            m.sections[0].rows[0] = Row(view: 10, custom: true, pressable: true)
            m.sections[0].rows[1] = Row(view: 11, custom: true, pressable: true)
            if inserted { m.sections[0].rows.insert(Row(view: 14, custom: true, pressable: true), at: 0) }
            return m
        }
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 16.0, "y": 0.0, "w": 354.0, "h": 85.0],
                           ["op": "frame", "id": 11, "x": 16.0, "y": 85.0, "w": 354.0, "h": 79.0]]))
        let l = try list(p)
        let a = try cell(p, 10), row = try XCTUnwrap(p.views[10])
        XCTAssertTrue(row.superview === a.contentView)
        inserted = true
        let insert = wireBatch([
            ["op": "create", "id": 14, "kind": "button", "props": ["testId": "row14"], "handlers": ["press"], "style": [:]],
            ["op": "children", "id": 4, "ids": [14, 10, 11, 12]],
            ["op": "frame", "id": 14, "x": 16.0, "y": 0.0, "w": 354.0, "h": 70.0],
            ["op": "frame", "id": 10, "x": 16.0, "y": 70.0, "w": 354.0, "h": 85.0],
            ["op": "frame", "id": 11, "x": 16.0, "y": 155.0, "w": 354.0, "h": 79.0],
        ])
        host(p).prepare(insert.ops)
        XCTAssertTrue(row.superview === a.contentView, "a frame and its parent's children leave it in its cell")
        p.apply(insert)
        XCTAssertTrue(row.superview === a.contentView, "nor did the batch move it")
        XCTAssertEqual(row.frame.minY, 0, "at its cell's top")
        XCTAssertTrue(try XCTUnwrap(p.views[14]).superview === (try cell(p, 14)).contentView, "the new row in its own cell")
        // Home, each is where the batch put it, in its order.
        l.restore()
        let group = try XCTUnwrap(p.views[4]).container
        XCTAssertEqual(group.subviews.compactMap { ($0 as? NodeView)?.id }, [14, 10, 11, 12])
        XCTAssertEqual(row.frame.minY, 70, "and its box")
        // A prop on the row itself takes it home alone for the batch.
        p.apply(wireBatch([["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 402.0, "h": 874.0]]))
        let b = try cell(p, 11)
        host(p).prepare(wireBatch([["op": "props", "id": 10, "set": ["testId": "first"]]]).ops)
        XCTAssertTrue(row.superview === group, "the row read at home")
        XCTAssertTrue(try XCTUnwrap(p.views[11]).superview === b.contentView, "its neighbour still in its cell")
    }

    func testAHeldCustomRowSurvivesSelectiveMaintenanceAndReleasesOnce() throws {
        for appearance in ["inset-grouped", "plain"] {
            let p = presenter { self.model(appearance, custom: true) }
            let row = try XCTUnwrap(p.views[21]), l = try list(p), native = try cell(p, 21)
            var presses: [UInt32] = [], selections: [[IndexPath]] = []
            p.onPress = { id in presses.append(id); selections.append(l.collection.indexPathsForSelectedItems ?? []) }
            let touch = RowTouch(row), contactBefore = row.pressEpoch
            row.touchesBegan([touch], with: nil)
            let contact = try XCTUnwrap(row.pressContact)
            XCTAssertTrue(native.isHighlighted)
            let props = wireBatch([["op": "props", "id": 21, "set": ["testId": "held"]]])
            host(p).prepare(props.ops)
            XCTAssertTrue(row.superview === p.views[5]?.container, "the changed row is read at home")
            XCTAssertTrue(row.pressed && native.isHighlighted, "maintenance keeps the contact and its highlight")
            p.apply(props)
            for ops: [[String: Any]] in [
                [["op": "style", "id": 21, "style": ["text_color": [20, 20, 20, 255]]]],
                [["op": "style", "id": 5, "style": ["text_color": [30, 30, 30, 255]]]],
                [["op": "children", "id": 5, "ids": [20, 21]],
                 ["op": "frame", "id": 21, "x": 16.0, "y": 52.0, "w": 354.0, "h": 61.0]],
                [["op": "props", "id": 1, "set": ["refreshing": "false"]]],
            ] {
                p.apply(wireBatch(ops))
                XCTAssertTrue(row.pressContact === contact)
                XCTAssertFalse(contact.cancelled)
                XCTAssertEqual(row.pressEpoch, contactBefore, "maintenance keeps the contact's epoch")
                XCTAssertTrue(row.superview === (try cell(p, 21)).contentView)
                XCTAssertTrue(row.pressed)
                XCTAssertTrue(try cell(p, 21).isHighlighted)
            }
            row.touchesEnded([touch], with: nil)
            row.touchesEnded([touch], with: nil)
            row.touchesCancelled([touch], with: nil)
            XCTAssertEqual(presses, [21])
            XCTAssertEqual(selections, [[IndexPath(item: 1, section: 1)]])
            XCTAssertFalse(row.pressed)
            XCTAssertFalse(try cell(p, 21).isHighlighted)
            XCTAssertEqual(l.collection.indexPathsForSelectedItems ?? [], [])
            window.isHidden = true
        }
    }

    func testAuthoredFeedbackReplacesTheCellHighlightAndTheFingerStillFollows() throws {
        let p = presenter { self.model(custom: true) }
        let row = try XCTUnwrap(p.views[21]), native = try cell(p, 21)
        var presses: [UInt32] = []; p.onPress = { presses.append($0) }
        let touch = RowTouch(row), inside = touch.point
        let unpressedBounds = row.bounds, unpressedFrame = row.frame
        row.touchesBegan([touch], with: nil)
        XCTAssertTrue(native.isHighlighted)
        p.apply(wireBatch([["op": "style", "id": 21, "style": ["press_scale": 0.9, "press_background_color": [255, 0, 0, 255]]]]))
        XCTAssertFalse(native.isHighlighted, "the author owns the pressed look")
        XCTAssertTrue(row.pressPaint.down)
        XCTAssertEqual(row.press.to, 0.9, accuracy: 0.001)
        p.apply(wireBatch([["op": "props", "id": 21, "set": ["testId": "authored"]]]))
        XCTAssertTrue(row.pressed && row.pressPaint.down)
        XCTAssertEqual(row.bounds.width, unpressedBounds.width, accuracy: 0.001)
        XCTAssertEqual(row.bounds.height, unpressedBounds.height, accuracy: 0.001)
        XCTAssertEqual(row.transform.a, 0.9, accuracy: 0.001)
        touch.point = native.contentView.convert(CGPoint(x: unpressedFrame.maxX + 8, y: unpressedFrame.midY), to: nil)
        row.touchesMoved([touch], with: nil)
        XCTAssertTrue(row.pressPaint.outside, "a finger beyond the unpressed row leaves its authored feedback")
        XCTAssertTrue(!row.pressPaint.down || row.pressPaint.releaseHeld, "outside paint clears or holds its young release until it has been shown")
        p.apply(wireBatch([["op": "style", "id": 21, "style": [:]]]))
        XCTAssertFalse(native.isHighlighted, "a style batch cannot put an outside finger back inside")
        touch.point = inside
        row.touchesMoved([touch], with: nil)
        XCTAssertTrue(native.isHighlighted, "the platform look returns with the finger")
        row.touchesCancelled([touch], with: nil)
        row.touchesEnded([touch], with: nil)
        XCTAssertEqual(presses, [])
        XCTAssertFalse(native.isHighlighted)
        waitFor("the authored feedback clears") { !row.pressPaint.down && row.press.to == 1 && !row.press.releaseHeld }
        row.handlers = ["press", "pointerdown", "pointerup"]
        let painted = RowTouch(row)
        row.touchesBegan([painted], with: nil)
        p.apply(wireBatch([["op": "style", "id": 21, "style": ["background_color": [0, 200, 0, 255]]]]))
        XCTAssertTrue(row.pressed)
        XCTAssertFalse(native.isHighlighted, "pointer handlers keep the author's pressed state")
        XCTAssertEqual(row.style["background_color"], .array([.number(0), .number(200), .number(0), .number(255)]))
        row.touchesEnded([painted], with: nil)
        XCTAssertEqual(presses, [21])
    }

    func testALabelSelectsItsCustomRowAndANestedButtonKeepsItsOwnPress() throws {
        let p = presenter { self.model(custom: true) }
        p.apply(wireBatch([
            ["op": "create", "id": 30, "kind": "text", "props": ["text": "Label"], "style": [:]],
            ["op": "create", "id": 31, "kind": "button", "handlers": ["press"], "style": ["appearance": "none"]],
            ["op": "children", "id": 21, "ids": [30, 31]],
            ["op": "frame", "id": 30, "x": 0.0, "y": 0.0, "w": 200.0, "h": 61.0],
            ["op": "frame", "id": 31, "x": 240.0, "y": 0.0, "w": 100.0, "h": 61.0],
        ]))
        let row = try XCTUnwrap(p.views[21]), label = try XCTUnwrap(p.views[30]), button = try XCTUnwrap(p.views[31])
        let native = try cell(p, 21), l = try list(p)
        var presses: [UInt32] = [], selections: [[IndexPath]] = []
        p.onPress = { presses.append($0); selections.append(l.collection.indexPathsForSelectedItems ?? []) }
        let labelTouch = RowTouch(label)
        label.touchesBegan([labelTouch], with: nil)
        XCTAssertTrue(row.pressed && native.isHighlighted)
        p.apply(wireBatch([["op": "props", "id": 21, "set": ["testId": "forwarded"]]]))
        label.touchesEnded([labelTouch], with: nil)
        let buttonTouch = RowTouch(button, at: CGPoint(x: 20, y: 30))
        button.touchesBegan([buttonTouch], with: nil)
        XCTAssertTrue(button.pressed)
        XCTAssertFalse(row.pressed || native.isHighlighted)
        p.apply(wireBatch([["op": "style", "id": 5, "style": ["opacity": 1.0]]]))
        button.touchesEnded([buttonTouch], with: nil)
        XCTAssertEqual(presses, [21, 31])
        XCTAssertEqual(selections, [[IndexPath(item: 1, section: 1)], []])
        XCTAssertFalse(row.pressed || button.pressed || native.isHighlighted)
    }

    func testCancellationRemovalDisableAndAuthoredMovesCannotLeakAPress() throws {
        for reason in ["cancel", "disabled", "inert", "removed", "reparented", "cell-ended", "window-detached", "navigation", "list-removed"] {
            var removed = false
            let p = presenter {
                var m = self.model(custom: true)
                if removed { m.sections[1].rows.removeAll { $0.view == 21 } }
                return m
            }
            let row = try XCTUnwrap(p.views[21]), native = try cell(p, 21), l = try list(p)
            var presses: [UInt32] = []; p.onPress = { presses.append($0) }
            let touch = RowTouch(row)
            row.touchesBegan([touch], with: nil)
            XCTAssertTrue(row.pressed && native.isHighlighted, reason)
            switch reason {
            case "cancel": row.touchesCancelled([touch], with: nil)
            case "disabled", "inert":
                let id = reason == "disabled" ? 21 : 5
                p.apply(wireBatch([["op": "props", "id": id, "set": [reason: "true"]]]))
                p.apply(wireBatch([["op": "props", "id": id, "set": [:], "clear": [reason]]]))
            case "removed":
                removed = true
                p.apply(wireBatch([["op": "children", "id": 5, "ids": [20]], ["op": "destroy", "id": 21]]))
            case "reparented":
                removed = true
                p.apply(wireBatch([["op": "children", "id": 4, "ids": [10, 11, 12, 21]], ["op": "children", "id": 5, "ids": [20]]]))
            case "cell-ended":
                l.collectionView(l.collection, didEndDisplaying: native, forItemAt: IndexPath(item: 1, section: 1))
            case "window-detached":
                p.viewport.removeFromSuperview()
                XCTAssertNil(row.window)
                XCTAssertTrue(row.superview === native.contentView, "window removal keeps the row in its cell")
            case "navigation": p.apply(wireBatch([["op": "roots", "ids": [1]]]))
            default: l.remove()
            }
            XCTAssertFalse(row.pressed || native.isHighlighted, reason)
            row.touchesEnded([touch], with: nil)
            row.touchesCancelled([touch], with: nil)
            XCTAssertEqual(presses, [], reason)
            window.isHidden = true
        }
    }

    func testAReboundRowKeepsItsNewHighlightWhenOldCallbacksArrive() throws {
        let p = presenter { self.model(custom: true) }
        let row = try XCTUnwrap(p.views[21]), native = try cell(p, 21)
        var presses: [UInt32] = []; p.onPress = { presses.append($0) }
        let old = RowTouch(row)
        row.touchesBegan([old], with: nil)
        let incarnation = row.incarnation
        row.rebind(21)
        row.handlers = ["press"]
        row.applyStyle([:])
        XCTAssertNotEqual(row.incarnation, incarnation)
        XCTAssertFalse(row.pressed || native.isHighlighted)
        let fresh = RowTouch(row)
        row.touchesBegan([fresh], with: nil)
        row.touchesEnded([old], with: nil)
        row.touchesCancelled([old], with: nil)
        XCTAssertTrue(row.pressed && native.isHighlighted)
        row.touchesEnded([fresh], with: nil)
        XCTAssertEqual(presses, [21])
        XCTAssertFalse(row.pressed || native.isHighlighted)
    }

    func testAPressHandlerStartingAnotherContactKeepsTheNewHighlight() throws {
        let p = presenter { self.model(custom: true) }
        let row = try XCTUnwrap(p.views[21]), native = try cell(p, 21)
        var presses: [UInt32] = [], fresh: RowTouch?
        p.onPress = { id in
            presses.append(id)
            if presses.count == 1 {
                let next = RowTouch(row)
                fresh = next
                row.touchesBegan([next], with: nil)
            }
        }
        let old = RowTouch(row)
        row.touchesBegan([old], with: nil)
        row.touchesEnded([old], with: nil)
        row.touchesEnded([old], with: nil)
        row.touchesCancelled([old], with: nil)
        XCTAssertTrue(row.pressed && native.isHighlighted)
        row.touchesEnded([try XCTUnwrap(fresh)], with: nil)
        XCTAssertEqual(presses, [21, 21])
        XCTAssertFalse(row.pressed || native.isHighlighted)
    }

    func testADefaultLinkSelectsItsCellAndActivatesOnceWithAuthoredFeedback() throws {
        final class Delegate: ExactSessionDelegate {
            var opened: (() -> Void)?
            var urls: [String] = []
            func exactSession(_ session: ExactSession, command name: String, args: [Any]) {
                if name == "openURL", let url = args.first as? String { urls.append(url); opened?() }
            }
        }
        let delegate = Delegate(), session = ExactApp.shared.makeSession(delegate: delegate, label: "grouped-link-contact")
        defer { session.destroy() }
        let p = presenter(on: session.presenter) { self.model("plain", custom: true) }
        let row = try XCTUnwrap(p.views[21]), native = try cell(p, 21), l = try list(p)
        row.handlers = []
        row.applyProps(set: ["href": "https://example.com/row"], clear: [])
        var selections: [[IndexPath]] = []
        delegate.opened = { selections.append(l.collection.indexPathsForSelectedItems ?? []) }
        for authored in [false, true] {
            p.apply(wireBatch([["op": "style", "id": 21, "style": authored ? ["press_scale": 0.9] : [:]]]))
            let touch = RowTouch(row)
            row.touchesBegan([touch], with: nil)
            XCTAssertEqual(native.isHighlighted, !authored)
            if authored { XCTAssertEqual(row.press.to, 0.9, accuracy: 0.001) }
            p.apply(wireBatch([["op": "props", "id": 21, "set": ["testId": "link-held"]]]))
            row.touchesEnded([touch], with: nil)
            row.touchesEnded([touch], with: nil)
            XCTAssertFalse(row.pressed || native.isHighlighted)
        }
        XCTAssertEqual(delegate.urls, Array(repeating: "https://example.com/row", count: 2))
        XCTAssertEqual(selections, Array(repeating: [IndexPath(item: 1, section: 1)], count: 2))
    }

    func testSelectingACustomRowKeepsTheTouchesFocusDecision() throws {
        let p = presenter { self.model(custom: true) }
        let row = try XCTUnwrap(p.views[21])
        row.handlers.insert("focus")
        row.applyProps(set: ["tabIndex": "0"], clear: [])
        var focused: [UInt32] = [], pressed: [UInt32] = []
        p.onFocus = { focused.append($0) }
        p.onPress = { pressed.append($0) }
        let touch = RowTouch(row)
        row.touchesBegan([touch], with: nil)
        row.touchesEnded([touch], with: nil)
        XCTAssertEqual(focused, [21])
        XCTAssertTrue(row.isFirstResponder)
        XCTAssertEqual(pressed, [21])
        XCTAssertFalse(try cell(p, 21).isHighlighted)
    }

    func testCustomRowsSelectThroughRuntimeBatchesPushAndCancelledReturn() throws {
        guard let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first else {
            throw missing("scene", "UIKit must deliver the list's return appearance and navigation completion")
        }
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_NAVIGATION_BARS_PLAN"], "compile scripts/fixtures/navigation-bars.contract")
        let plan = try Data(contentsOf: URL(fileURLWithPath: path))
        ExactGroupedLists.install()
        let session = ExactApp.shared.makeSession(label: "custom-rows-navigation")
        defer { session.destroy() }
        let view = ExactView(session: session), root = UIViewController()
        window = UIWindow(windowScene: scene)
        window.rootViewController = root
        window.makeKeyAndVisible()
        root.view.addSubview(view)
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = root.view.bounds
        view.layoutIfNeeded()
        let p = session.presenter, nav = try XCTUnwrap(p.navigation.primaryNavigation)
        func node(_ testId: String) throws -> NodeView {
            try XCTUnwrap(p.views.values.first { $0.props["testId"] == testId }, testId)
        }
        func settle(_ count: Int) {
            waitFor("the navigation stack has \(count) screens and its top is mounted after the native transition") {
                nav.viewControllers.count == count && nav.transitionCoordinator == nil && !p.navigation.inTransition
                    && nav.topViewController?.viewIfLoaded?.window === self.window
            }
        }
        XCTAssertTrue(session.navigate("/rows"))
        settle(2)
        let rowsController = try XCTUnwrap(nav.topViewController as? RouteController)
        let l = try XCTUnwrap(host(p).lists[try node("rows-list").id])
        l.collection.layoutIfNeeded()
        let selected = { l.collection.indexPathsForSelectedItems ?? [] }
        var presses: [UInt32] = [], selections: [[IndexPath]] = []
        let dispatch = p.onPress
        p.onPress = { id in presses.append(id); selections.append(selected()); dispatch?(id) }
        let stay = try node("rows-custom-stay"), stayTouch = RowTouch(stay)
        XCTAssertTrue(stay.isDescendant(of: l.collection), "the kernel chose a custom row")
        stay.touchesBegan([stayTouch], with: nil)
        p.apply(wireBatch([["op": "props", "id": stay.id, "set": ["accessibilityLabel": "Held custom row"]]]))
        stay.touchesEnded([stayTouch], with: nil)
        stay.touchesEnded([stayTouch], with: nil)
        XCTAssertEqual(presses, [stay.id])
        XCTAssertEqual(selections, [[try XCTUnwrap(l.source.indexPath(for: stay.id))]])
        XCTAssertEqual(selected(), [])
        let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any])
        XCTAssertEqual((state["slots"] as? [String: Any])?["stays"] as? Int ?? state["stays"] as? Int, 1)

        let open = try node("rows-custom-open"), openTouch = RowTouch(open)
        XCTAssertTrue(open.isDescendant(of: l.collection))
        let openPath = try XCTUnwrap(l.source.indexPath(for: open.id))
        open.touchesBegan([openTouch], with: nil)
        open.touchesEnded([openTouch], with: nil)
        settle(3)
        XCTAssertEqual(presses, [stay.id, open.id])
        XCTAssertEqual(selected(), [openPath], "selected under the screen its press pushed")
        XCTAssertFalse(l.cell(open.id)?.isHighlighted == true)
        let cancelled = CancelledReturn()
        l.deselect(alongside: cancelled)
        XCTAssertTrue(cancelled.animationRan && cancelled.completed)
        XCTAssertEqual(selected(), [openPath], "a cancelled return restores selection without another press")
        XCTAssertEqual(presses, [stay.id, open.id])
        var returned = false
        let appeared = rowsController.appeared
        rowsController.appeared = { appeared?(); returned = true }
        defer { rowsController.appeared = appeared }
        nav.popViewController(animated: false)
        settle(2)
        waitFor("UIKit finished the list's return appearance") { returned }
        XCTAssertEqual(selected(), [])

        let link = try node("rows-custom-link"), linkTouch = RowTouch(link)
        XCTAssertNotNil(link.defaultLink)
        XCTAssertTrue(link.isDescendant(of: l.collection))
        link.touchesBegan([linkTouch], with: nil)
        link.touchesEnded([linkTouch], with: nil)
        link.touchesEnded([linkTouch], with: nil)
        waitFor("the default link opens the detail") {
            p.views.values.contains { $0.props["testId"] == "route-detail" }
                && (nav.topViewController as? RouteController)?.node.props["testId"] == "route-detail"
                && nav.transitionCoordinator == nil && !p.navigation.inTransition
                && nav.topViewController?.viewIfLoaded?.window === self.window
        }
        XCTAssertEqual(presses, [stay.id, open.id], "the link uses its default navigation, with no press handler")
        XCTAssertFalse(link.pressed)
    }

    /// Calls the list's cancelled-transition callbacks without a UIKit gesture.
    private final class CancelledReturn: NSObject, UIViewControllerTransitionCoordinator {
        let isAnimated = true, initiallyInteractive = true, isInterruptible = true, isInteractive = true, isCancelled = true
        let presentationStyle: UIModalPresentationStyle = .none
        let transitionDuration: TimeInterval = 0.3
        let percentComplete: CGFloat = 0.4, completionVelocity: CGFloat = 0
        let completionCurve: UIView.AnimationCurve = .easeInOut
        let containerView = UIView(), targetTransform = CGAffineTransform.identity
        var animationRan = false, completed = false
        func viewController(forKey key: UITransitionContextViewControllerKey) -> UIViewController? { nil }
        func view(forKey key: UITransitionContextViewKey) -> UIView? { nil }
        func animate(alongsideTransition animation: ((UIViewControllerTransitionCoordinatorContext) -> Void)?,
                     completion: ((UIViewControllerTransitionCoordinatorContext) -> Void)?) -> Bool {
            animation?(self); animationRan = true
            completion?(self); completed = true
            return true
        }
        func animateAlongsideTransition(in view: UIView?, animation: ((UIViewControllerTransitionCoordinatorContext) -> Void)?,
                                        completion: ((UIViewControllerTransitionCoordinatorContext) -> Void)?) -> Bool {
            animate(alongsideTransition: animation, completion: completion)
        }
        func notifyWhenInteractionChanges(_ handler: @escaping (UIViewControllerTransitionCoordinatorContext) -> Void) {}
        func notifyWhenInteractionEnds(_ handler: @escaping (UIViewControllerTransitionCoordinatorContext) -> Void) {}
    }

    /// A swipe row inserted while a sheet is up (r25's contacts: the
    /// contact its form saves) offers its actions once the sheet closes.
    /// The sheet made it inert as it was mounted, and the batches after
    /// change nothing in it, so its row keeps what it read then: only the
    /// row's own `inert` may take its actions away.
    func testASwipeRowInsertedUnderASheetSwipesOnceItCloses() throws {
        var rows = [Row(view: 30, title: "Plain")]
        ExactGroupedLists.install()
        let p = Presenter()
        host(p).model = { $0 == 1 ? GroupedListModel(sections: [.init(view: 2, header: nil, footer: nil, rows: rows)]) : nil }
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let screen = scene.map { UIWindow(windowScene: $0) } ?? UIWindow()
        screen.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        let owner = UIViewController(), background = UIViewController()
        screen.rootViewController = owner
        screen.makeKeyAndVisible()
        window = screen
        addTeardownBlock { owner.presentedViewController?.dismiss(animated: false); screen.isHidden = true }
        p.viewport.frame = owner.view.bounds
        owner.view.addSubview(p.viewport)
        owner.addChild(background)
        background.view.frame = owner.view.bounds
        owner.view.addSubview(background.view)
        background.didMove(toParent: owner)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "list", "props": ["listStyle": "inset-grouped"], "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 2, "kind": "view", "props": ["semanticTag": "section"], "style": [:]],
            ["op": "create", "id": 4, "kind": "view", "props": [:], "style": [:]],
            ["op": "create", "id": 30, "kind": "view", "props": [:], "style": [:]],
            ["op": "create", "id": 90, "kind": "view", "props": ["navigationPresentation": "modal"], "style": [:]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [4]], ["op": "children", "id": 4, "ids": [30]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 402.0, "h": 874.0],
            ["op": "frame", "id": 30, "x": 16.0, "y": 0.0, "w": 354.0, "h": 52.0],
        ]))
        background.view.addSubview(try XCTUnwrap(p.views[1]))
        p.modals.present(try XCTUnwrap(p.views[90]), navigation: UIViewController(), from: background, preceding: nil, owner: owner)
        XCTAssertTrue(try XCTUnwrap(p.views[1]).inert, "the sheet makes the list inert")

        rows.insert(Row(view: 21, custom: true), at: 0)
        p.apply(wireBatch([
            ["op": "create", "id": 21, "kind": "view", "props": ["swipeContent": "body", "swipeTrailing": "delete"],
             "style": ["overflow_x": "scroll", "overflow_y": "hidden"]],
            ["op": "create", "id": 22, "kind": "view", "props": ["id": "body"], "style": [:]],
            ["op": "create", "id": 23, "kind": "button", "props": ["id": "delete", "accessibilityLabel": "Delete", "destructive": "true"],
             "handlers": ["press"], "style": [:]],
            ["op": "children", "id": 4, "ids": [21, 30]], ["op": "children", "id": 21, "ids": [22, 23]],
            ["op": "frame", "id": 21, "x": 16.0, "y": 0.0, "w": 386.0, "h": 52.0],
            ["op": "frame", "id": 22, "x": 0.0, "y": 0.0, "w": 370.0, "h": 52.0],
            ["op": "frame", "id": 23, "x": 370.0, "y": 0.0, "w": 60.0, "h": 52.0],
            ["op": "frame", "id": 30, "x": 16.0, "y": 52.0, "w": 354.0, "h": 52.0],
        ]))
        p.modals.closeTop(animated: false)
        XCTAssertFalse(try XCTUnwrap(p.views[21]).inert, "the sheet closed")
        // A batch that changes nothing in the row keeps it as it is.
        p.apply(wireBatch([["op": "props", "id": 4, "set": ["testId": "rows"], "clear": [String]()]]))
        let trailing = try list(p).swipeActions(at: IndexPath(item: 0, section: 0), leading: false)
        XCTAssertEqual(trailing?.actions.count, 1, "its Delete, as the rows that were there before the sheet")
        // Its cell, configured under the sheet, takes a tap and is an element
        // (r28's expenses: a saved expense neither opened nor was read until relaunch).
        try list(p).collection.layoutIfNeeded()
        let cell = try XCTUnwrap(list(p).cell(21))
        XCTAssertTrue(cell.isUserInteractionEnabled, "a tap reaches it")
        XCTAssertFalse(cell.accessibilityElementsHidden, "VoiceOver reads it")
    }

    /// A custom row's peek is its list's own context menu, as a hand-built
    /// list's is: the row has no interaction of its own, and the collection
    /// view asks for the menu and lifts the cell itself, in its background's
    /// shape over the ground below it (`GroupedListMenusIOS`).
    func testACustomRowsPeekIsItsListsOwnContextMenu() throws {
        if ExactEnv.agentPaints { throw XCTSkip("the agent paints the popover (D4)") }
        let p = presenter(props: ["listStyle": "plain"]) { self.model("plain", custom: true) }
        p.apply(wireBatch([
            ["op": "create", "id": 30, "kind": "view", "props": ["popover": "auto", "id": "row-menu", "accessibilityRole": "menu"]],
            ["op": "create", "id": 31, "kind": "button", "handlers": ["press"]],
            ["op": "children", "id": 30, "ids": [31]],
            ["op": "children", "id": 3, "ids": [5, 30]],
            ["op": "props", "id": 21, "set": ["contextPopover": "row-menu"]],
            ["op": "frame", "id": 30, "x": 0.0, "y": 0.0, "w": 200.0, "h": 44.0],
            ["op": "frame", "id": 31, "x": 0.0, "y": 0.0, "w": 200.0, "h": 44.0],
        ]))
        p.buttonFace = { _ in var face = ButtonFace(); face.title = "Copy"; return face }
        p.menus.context.sync()
        let row = try XCTUnwrap(p.views[21]), cell = try cell(p, 21)
        XCTAssertTrue(row.superview === cell.contentView, "carried into its cell")
        XCTAssertTrue(row.interactions.compactMap { $0 as? UIContextMenuInteraction }.isEmpty, "no interaction of its own")
        let collection = try XCTUnwrap(sequence(first: cell.superview, next: { $0?.superview }).compactMap { $0 as? UICollectionView }.first)
        XCTAssertNotNil(collection.contextMenuInteraction, "the list's own context menu")
        let delegate = try XCTUnwrap(collection.delegate), path = try XCTUnwrap(collection.indexPath(for: cell))
        let configuration = try XCTUnwrap(delegate.collectionView?(collection, contextMenuConfigurationForItemsAt: [path], point: cell.convert(CGPoint(x: 20, y: 10), to: collection)))
        XCTAssertTrue(p.menus.context.open?.source === row)
        XCTAssertNotNil(p.menus.context.menu(try XCTUnwrap(p.menus.context.open)))
        XCTAssertFalse(delegate.responds(to: #selector(UICollectionViewDelegate.collectionView(_:contextMenuConfiguration:highlightPreviewForItemAt:))),
                       "UIKit lifts the cell itself")
        delegate.collectionView?(collection, willEndContextMenuInteraction: configuration, animator: nil)
        XCTAssertNil(p.menus.context.open)
    }
}
#endif
