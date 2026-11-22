#if os(macOS)
import AppKit
#else
import UIKit
#endif
import XCTest
@testable import ExactKit

/// LLP 1153 D7.3: a table is the platform's table, not only its roles.
/// `tests/fixtures/table-access.contract` writes `tfoot, thead, tbody, tfoot`,
/// which the table draws, and the kernel counts, as head, body, second foot,
/// first foot (CSS 2.2 §17.2); a row header spans two rows and each foot's
/// cell both columns. Its second table is empty.
final class TableAccessibilityTests: XCTestCase {
    private var session: ExactSession!
    #if os(macOS)
    private var window: NSWindow!
    #else
    private var window: UIWindow!
    #endif

    override func tearDown() {
        session?.destroy()
        #if os(macOS)
        window?.contentView = nil
        #else
        window?.isHidden = true
        #endif
        window = nil
        super.tearDown()
    }

    private func boot(_ key: String = "TABLE_ACCESS") throws -> Presenter {
        let plan = try fixturePlan(key)
        session = ExactApp.shared.makeSession(label: "table-access")
        let size = CGSize(width: 400, height: 600)
        let view = session.presenter.viewport
        view.frame = CGRect(origin: .zero, size: size)
        #if os(macOS)
        window = NSWindow(contentRect: CGRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = view
        #else
        window = UIWindow(frame: CGRect(origin: .zero, size: size))
        let controller = UIViewController()
        controller.view = view
        window.rootViewController = controller
        window.makeKeyAndVisible()
        #endif
        XCTAssertNil(session.boot(plan: plan, size: size).error)
        session.presenter.syncAccessibility()
        return session.presenter
    }
    private func node(_ p: Presenter, _ testId: String) throws -> NodeView {
        try XCTUnwrap(p.views.values.first { $0.props["testId"] == testId }, testId)
    }
    private func ids(_ elements: [Any]?) -> [String] {
        (elements ?? []).map { ($0 as? NodeView)?.props["testId"] ?? "\(type(of: $0))" }
    }
    /// The words the agent's walk reaches, in its order: AppKit's navigation
    /// order, UIKit's containment order.
    private func words(_ p: Presenter) -> [String] {
        let elements = p.axElements(roots: [p.viewport])["elements"] as? [[String: Any]] ?? []
        return elements.compactMap { $0["name"] as? String }.filter { !$0.isEmpty }
    }
    private static let drawn = ["Moves", "Move", "Power", "Tackle", "40", "Ember", "50", "60", "Foot 2", "Foot 1"]

    func testProseListMarkersFollowTheItemAndRenumberAfterAnInsertion() throws {
        let p = try boot("PROSE_LIST")
        let retained = try node(p, "retained")
        func label(_ item: NodeView) -> String? {
            (item.proseMarker?.string as? NSAttributedString)?.string
        }
        XCTAssertEqual(label(retained), "2.")
        let left = try XCTUnwrap(retained.proseMarker)
        XCTAssertLessThan(left.frame.maxX, retained.bounds.minX)
        XCTAssertNil(try node(p, "bare").proseMarker)
        let right = try node(p, "right")
        XCTAssertEqual(label(right), "•")
        let bullet = try XCTUnwrap(right.proseMarker)
        XCTAssertGreaterThan(bullet.frame.minX, right.bounds.maxX)
        XCTAssertEqual(label(try node(p, "nested")), "◦")
        let change = try node(p, "change")
        session.apply(session.runtime.press(change.id, now: session.now()))
        XCTAssertEqual(label(retained), "3.")
        session.apply(session.runtime.press(change.id, now: session.now()))
        XCTAssertEqual(label(retained), "2.")
    }

    #if os(macOS)
    func testATableServesItsRowsColumnsAndHeadersAndEachCellItsPlace() throws {
        let p = try boot()
        let table = try node(p, "moves")
        XCTAssertEqual(table.accessibilityRole(), .table)
        XCTAssertEqual(table.accessibilityRowCount(), 6)
        XCTAssertEqual(table.accessibilityColumnCount(), 2)
        XCTAssertEqual(ids(table.accessibilityRows()), ["r-head", "r-tackle", "r-ember", "r-sixty", "r-foot2", "r-foot1"])
        XCTAssertEqual(try node(p, "r-foot1").accessibilityIndex(), 5)
        let ember = try node(p, "ember")
        XCTAssertEqual(ember.accessibilityRowIndexRange(), NSRange(location: 2, length: 2))
        XCTAssertEqual(ember.accessibilityColumnIndexRange(), NSRange(location: 0, length: 1))
        XCTAssertEqual(try node(p, "foot1").accessibilityRowIndexRange(), NSRange(location: 5, length: 1))
        XCTAssertEqual(try node(p, "foot1").accessibilityColumnIndexRange(), NSRange(location: 0, length: 2))
        XCTAssertEqual(ids(table.accessibilityColumnHeaderUIElements()), ["h-move", "h-power"])
        XCTAssertEqual(ids(table.accessibilityRowHeaderUIElements()), ["tackle", "ember"])
        XCTAssertEqual(ids(try node(p, "forty").accessibilityColumnHeaderUIElements()), ["h-power"])
        XCTAssertEqual(ids(try node(p, "sixty").accessibilityRowHeaderUIElements()), ["ember"], "a row header spans its rowspan")
        XCTAssertEqual(ids([table.accessibilityCell(forColumn: 0, row: 3) as Any]), ["ember"])
        XCTAssertEqual(ids([table.accessibilityCell(forColumn: 1, row: 3) as Any]), ["sixty"])

        let columns = try XCTUnwrap(table.accessibilityColumns() as? [NSAccessibilityElement])
        XCTAssertEqual(columns.count, 2)
        XCTAssertEqual(columns[0].accessibilityRole(), .column)
        XCTAssertEqual(columns[1].accessibilityIndex(), 1)
        XCTAssertEqual(ids(columns[0].accessibilityChildren()), ["h-move", "tackle", "ember", "foot2", "foot1"])
        XCTAssertEqual(ids([columns[1].accessibilityHeader() as Any]), ["h-power"])
        XCTAssertTrue(columns[0] === table.accessibilityColumns()?.first as AnyObject, "the same column at each read")
        let header = try XCTUnwrap(table.accessibilityHeader() as? NSAccessibilityElement)
        XCTAssertEqual(ids(header.accessibilityChildren()), ["h-move", "h-power"])
        // What another process (VoiceOver) is served: AppKit's attribute protocol.
        let served = Set(table.accessibilityAttributeNames())
        for name: NSAccessibility.Attribute in [.rows, .visibleRows, .columns, .rowCount, .columnCount, .header, .columnHeaderUIElements, .rowHeaderUIElements] {
            XCTAssertTrue(served.contains(name), name.rawValue)
        }
        XCTAssertEqual(table.accessibilityAttributeValue(.rowCount) as? Int, 6)
        XCTAssertEqual(ids(table.accessibilityAttributeValue(.rows) as? [Any]), ids(table.accessibilityRows()))
        XCTAssertTrue(table.accessibilityParameterizedAttributeNames().contains(.cellForColumnAndRow))
        XCTAssertEqual(ids([table.accessibilityAttributeValue(.cellForColumnAndRow, forParameter: [1, 3]) as Any]), ["sixty"])
        XCTAssertEqual((columns[0].accessibilityAttributeValue(.index) as? NSNumber)?.intValue, 0)
        XCTAssertEqual((ember.accessibilityAttributeValue(.rowIndexRange) as? NSValue)?.rangeValue, NSRange(location: 2, length: 2))
        XCTAssertEqual(ids(ember.accessibilityAttributeValue(.columnHeaderUIElements) as? [Any]), ["h-move"])
        XCTAssertEqual(try node(p, "r-foot1").accessibilityAttributeValue(.index) as? Int, 5)
        XCTAssertFalse(Set(try node(p, "forty").accessibilityAttributeNames()).contains(.rows), "a cell is no table")

        let empty = try node(p, "empty")
        XCTAssertEqual(empty.accessibilityRole(), .table)
        XCTAssertEqual(empty.accessibilityRowCount(), 0)
        XCTAssertEqual(empty.accessibilityColumnCount(), 0)
        XCTAssertEqual(ids(empty.accessibilityRows()), [])
        XCTAssertNil(empty.accessibilityHeader())
    }

    /// VoiceOver's order, AppKit's navigation order, is the table's drawn
    /// order: the first `tfoot` last.
    func testVoiceOverVisitsATablesRowsInTheOrderItDrawsThem() throws {
        let p = try boot()
        XCTAssertEqual(words(p), Self.drawn)
        let table = try node(p, "moves")
        let cells = p.axElements(roots: [table])["elements"] as? [[String: Any]] ?? []
        let ember = try XCTUnwrap(cells.first { $0["testId"] as? String == "ember" })
        XCTAssertEqual((ember["native"] as? [String: Any])?["item"] as? [Int], [2, 2, 0, 1])
        let root = try XCTUnwrap(cells.first { $0["testId"] as? String == "moves" })
        XCTAssertEqual((root["native"] as? [String: Any])?["collection"] as? [Int], [6, 2])
    }
    #else
    func testATableIsUIKitsDataTableAndEachCellSaysWhereItStands() throws {
        let p = try boot()
        let table = try node(p, "moves")
        XCTAssertEqual(table.accessibilityContainerType, .dataTable)
        XCTAssertEqual(table.accessibilityRowCount(), 6)
        XCTAssertEqual(table.accessibilityColumnCount(), 2)
        XCTAssertEqual(ids([table.accessibilityDataTableCellElement(forRow: 3, column: 0) as Any]), ["ember"])
        XCTAssertEqual(ids([table.accessibilityDataTableCellElement(forRow: 3, column: 1) as Any]), ["sixty"])
        XCTAssertEqual(ids([table.accessibilityDataTableCellElement(forRow: 5, column: 1) as Any]), ["foot1"])
        XCTAssertNil(table.accessibilityDataTableCellElement(forRow: 6, column: 0))
        XCTAssertEqual(ids(table.accessibilityHeaderElements(forColumn: 1)), ["h-power"])
        XCTAssertEqual(ids(table.accessibilityHeaderElements(forRow: 3)), ["ember"], "a row header spans its rowspan")

        let ember = try node(p, "ember")
        XCTAssertTrue(ember.isAccessibilityElement, "a cell of words is one element")
        XCTAssertEqual(ember.accessibilityLabel, "Ember")
        XCTAssertTrue(ember.accessibilityTraits.contains(.header), "a th is a header")
        XCTAssertEqual(ember.accessibilityRowRange(), NSRange(location: 2, length: 2))
        XCTAssertEqual(ember.accessibilityColumnRange(), NSRange(location: 0, length: 1))
        let forty = try node(p, "forty")
        XCTAssertTrue(forty.isAccessibilityElement)
        XCTAssertEqual(forty.accessibilityLabel, "40")
        XCTAssertFalse(forty.accessibilityTraits.contains(.header))
        let words = try XCTUnwrap(forty.container.subviews.compactMap { $0 as? NodeView }.first)
        XCTAssertEqual(words.accessibilityRowRange(), NSRange(location: 1, length: 1), "a node in a cell stands where its cell does")
        XCTAssertEqual(try node(p, "foot1").accessibilityColumnRange(), NSRange(location: 0, length: 2))
        XCTAssertEqual(table.accessibilityRowRange().location, NSNotFound)

        let empty = try node(p, "empty")
        XCTAssertEqual(empty.accessibilityContainerType, .dataTable)
        XCTAssertEqual(empty.accessibilityRowCount(), 0)
        XCTAssertEqual(empty.accessibilityColumnCount(), 0)
        XCTAssertNil(empty.accessibilityDataTableCellElement(forRow: 0, column: 0))
    }

    /// The table's elements are listed in the order it draws them, the
    /// first `tfoot` last, which is also the order of their frames.
    func testATablesElementsFollowItsDrawnOrder() throws {
        let p = try boot()
        XCTAssertEqual(words(p), Self.drawn)
        let table = try node(p, "moves")
        let elements = p.axElements(roots: [table])["elements"] as? [[String: Any]] ?? []
        let ember = try XCTUnwrap(elements.first { $0["testId"] as? String == "ember" })
        XCTAssertEqual((ember["native"] as? [String: Any])?["item"] as? [Int], [2, 2, 0, 1])
        let root = try XCTUnwrap(elements.first { $0["testId"] as? String == "moves" })
        XCTAssertEqual((root["native"] as? [String: Any])?["collection"] as? [Int], [6, 2])
    }
    #endif
}
