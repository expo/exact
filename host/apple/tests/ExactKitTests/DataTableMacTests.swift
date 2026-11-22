#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

final class DataTableMacTests: XCTestCase {
    private func withWindow(_ plan: Data, named name: String? = nil, painted: Bool = false,
                            _ body: (Presenter, NSWindow, NodeView) throws -> Void) throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "data-table")
        let size = CGSize(width: 500, height: 300)
        let window = NSWindow(contentRect: CGRect(origin: .zero, size: size), styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        if let name { XCTAssertTrue(window.setFrameAutosaveName(name)) }
        defer {
            session.destroy()
            if name != nil { window.setFrameAutosaveName("") }
            window.close()
        }
        let presenter = session.presenter
        presenter.tables.painted = painted
        window.contentView = ExactView(session: session)
        window.layoutIfNeeded()
        XCTAssertNil(session.boot(plan: plan, size: size).error)
        presenter.settlePump()
        let people = try XCTUnwrap(presenter.views.values.first { $0.props["testId"] == "people" })
        try body(presenter, window, people)
    }

    func testPressingAHeaderExposesTheNewSortDirectionAndRowOrder() throws {
        let plan = try TableFixture.plan("""
        component App
          state step = 0
          derive direction = step == 0 ? "ascending" : (step == 1 ? "descending" : (step == 2 ? "other" : "none"))
          derive names = step == 0 ? ["Ada", "Lin"] : ["Lin", "Ada"]
          action sort
            step = (step + 1) % 4
          view
            table testId="people"
              thead
                tr
                  th testId="name" press=sort aria-sort=direction
                    text "Name"
                  th testId="team"
                    text "Team"
              tbody
                for name in names key=name
                  tr
                    td testId=name
                      text name
                    td
                      text "Engineering"

        """)
        try withWindow(plan, painted: true) { presenter, _, people in
            let name = try XCTUnwrap(presenter.views.values.first { $0.props["testId"] == "name" })
            func firstName() -> String? { people.tableCell(row: 1, column: 0)?.props["testId"] }
            let team = try XCTUnwrap(presenter.views.values.first { $0.props["testId"] == "team" })
            func direction(_ node: NodeView) -> String? {
                node.accessibilityAttributeValue(.sortDirection) as? String
            }
            XCTAssertEqual(firstName(), "Ada")
            XCTAssertEqual(name.accessibilitySortDirection(), .ascending)
            XCTAssertTrue(name.accessibilityAttributeNames().contains(.sortDirection))
            XCTAssertEqual(direction(name), NSAccessibility.SortDirectionValue.ascending.rawValue)
            XCTAssertFalse(team.accessibilityAttributeNames().contains(.sortDirection))
            XCTAssertNil(direction(team))
            func pressHeader() {
                XCTAssertTrue(name.accessibilityPerformPress())
                let pressed = expectation(description: "AppKit delivered the header press")
                DispatchQueue.main.async { pressed.fulfill() }
                wait(for: [pressed], timeout: 2)
                presenter.settlePump()
            }
            pressHeader()
            XCTAssertEqual(firstName(), "Lin")
            XCTAssertEqual(name.accessibilitySortDirection(), .descending)
            XCTAssertEqual(direction(name), NSAccessibility.SortDirectionValue.descending.rawValue)
            pressHeader()
            XCTAssertEqual(direction(name), NSAccessibility.SortDirectionValue.unknown.rawValue)
            pressHeader()
            XCTAssertFalse(name.accessibilityAttributeNames().contains(.sortDirection))
            XCTAssertNil(direction(name))
        }
    }

    private static let people = """
        component App
          state up = true
          derive names = up ? ["Ada", "Lin"] : ["Lin", "Ada"]
          action sort
            up = !up
          view
            table testId="people"
              thead
                tr
                  th testId="name" press=sort aria-sort=(up ? "ascending" : "descending")
                    text "Name"
                  th
                    text "Team"
              tbody
                for name in names key=name
                  tr
                    td testId=name
                      text name
                      text "Writes about public transit and neighborhood parks."
                    td
                      text "Engineering"

        """

    func testAnAuthoredSortableTableHasResizableAppKitColumns() throws {
        try withWindow(TableFixture.plan(Self.people)) { presenter, window, people in
            let name = try XCTUnwrap(presenter.views.values.first { $0.props["testId"] == "name" })
            func firstName() -> String? { people.tableCell(row: 1, column: 0)?.props["testId"] }
            XCTAssertEqual(firstName(), "Ada")
            presenter.press(name.id)
            presenter.settlePump()
            XCTAssertEqual(firstName(), "Lin")
            func table(in view: NSView) -> NSTableView? {
                if let found = view as? NSTableView { return found }
                return view.subviews.lazy.compactMap { table(in: $0) }.first
            }
            let native = try XCTUnwrap(table(in: presenter.viewport), "A sortable two-column table needs an AppKit header")
            XCTAssertEqual(native.tableColumns.map(\.title), ["Name", "Team"])
            XCTAssertEqual(native.numberOfRows, 2)
            XCTAssertNotNil(native.headerView)
            XCTAssertTrue(native.allowsColumnResizing)
            let first = try XCTUnwrap(native.tableColumns.first)
            let lin = try XCTUnwrap(presenter.views.values.first { $0.props["testId"] == "Lin" })
            let cell = try XCTUnwrap(native.view(atColumn: 0, row: 0, makeIfNecessary: true))
            XCTAssertTrue(lin.isDescendant(of: cell), "the row contains the authored cell")
            let header = try XCTUnwrap(native.headerView)
            let headerBox = header.headerRect(ofColumn: 0)
            let middle = header.convert(NSPoint(x: headerBox.midX, y: headerBox.midY), to: nil)
            let content = try XCTUnwrap(window.contentView)
            let hit = try XCTUnwrap(content.hitTest(middle))
            XCTAssertTrue(hit === header || hit.isDescendant(of: header), "the visible header receives the click")
            let previousHeight = lin.bounds.height
            first.width = 100
            presenter.settlePump()
            XCTAssertEqual(lin.grid?.layout.width ?? 0, 100, accuracy: 0.5)
            XCTAssertGreaterThan(lin.bounds.height, previousHeight, "text wraps at the resized column width")
            let point = header.convert(NSPoint(x: header.headerRect(ofColumn: 0).midX, y: header.bounds.midY), to: nil)
            func mouse(_ type: NSEvent.EventType, _ point: NSPoint) throws -> NSEvent {
                try XCTUnwrap(NSEvent.mouseEvent(with: type, location: point, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: type == .leftMouseUp ? 0 : 1))
            }
            NSApp.postEvent(try mouse(.leftMouseUp, point), atStart: true)
            header.mouseDown(with: try mouse(.leftMouseDown, point))
            presenter.settlePump()
            XCTAssertEqual(firstName(), "Ada")
            XCTAssertEqual(first.width, 100, accuracy: 0.5)
            XCTAssertEqual(native.sortDescriptors.first?.ascending, true)
            let edge = header.convert(NSPoint(x: header.headerRect(ofColumn: 0).maxX - 1, y: header.bounds.midY), to: nil)
            let end = NSPoint(x: edge.x + 36, y: edge.y)
            NSApp.postEvent(try mouse(.leftMouseUp, end), atStart: true)
            NSApp.postEvent(try mouse(.leftMouseDragged, end), atStart: true)
            header.mouseDown(with: try mouse(.leftMouseDown, edge))
            presenter.settlePump()
            XCTAssertGreaterThan(first.width, 120)
            XCTAssertEqual(lin.bounds.width, first.width, accuracy: 0.5)
            XCTAssertEqual(firstName(), "Ada", "dragging the divider does not sort")
        }
    }

    func testReversingColumnsKeepsEachNativeHeaderOverItsCells() throws {
        let plan = try TableFixture.plan(Self.people.replacingOccurrences(of: "table testId=", with: "table direction=\"rtl\" testId="))
        try withWindow(plan) { p, _, people in
            let table = try XCTUnwrap(p.dataTables.native(people))
            XCTAssertEqual(table.tableColumns.map(\.title), ["Team", "Name"])
            let authorCell = try XCTUnwrap(people.tableCell(row: 1, column: 0))
            let nativeCell = try XCTUnwrap(table.view(atColumn: 1, row: 0, makeIfNecessary: true))
            XCTAssertTrue(authorCell.isDescendant(of: nativeCell))
            p.tables.painted = true
            p.settlePump()
            XCTAssertNil(p.dataTables.native(people))
            let row = try XCTUnwrap(authorCell.nodeSuperview)
            XCTAssertTrue(row.childNodes.first === authorCell)
            XCTAssertFalse(row.isHidden)
        }
    }

    func testTheTableKeepsAnAuthoredHeaderBackground() throws {
        let source = Self.people.replacingOccurrences(of: "th testId=", with: "th background-color=\"#205080\" testId=")
        try withWindow(TableFixture.plan(source)) { p, _, people in
            XCTAssertNil(p.dataTables.native(people))
            let name = try XCTUnwrap(people.tableCell(row: 0, column: 0))
            XCTAssertFalse(name.isHiddenOrHasHiddenAncestor)
        }
    }

    func testOnlyTheNamedWindowThatWasResizedRestoresTheWidth() throws {
        let plan = try TableFixture.plan(Self.people)
        let resized = UUID().uuidString, other = UUID().uuidString
        defer {
            for name in [resized, other] {
                UserDefaults.standard.removeObject(forKey: "Exact.tableColumnWidths." + name)
                NSWindow.removeFrame(usingName: name)
            }
        }
        func open(_ name: String, _ body: (NSTableColumn, NodeView) -> Void) throws {
            try withWindow(plan, named: name) { p, _, node in
                let table = try XCTUnwrap(p.dataTables.native(node))
                body(try XCTUnwrap(table.tableColumns.first), try XCTUnwrap(node.tableCell(row: 1, column: 0)))
            }
        }
        try open(resized) { column, _ in column.width = 113 }
        try open(other) { column, cell in
            XCTAssertNotEqual(column.width, 113)
            XCTAssertEqual(cell.frame.width, column.width, accuracy: 0.5)
        }
        try open(resized) { column, cell in
            XCTAssertEqual(column.width, 113)
            XCTAssertEqual(cell.frame.width, 113, accuracy: 0.5)
        }
    }
}
#endif
