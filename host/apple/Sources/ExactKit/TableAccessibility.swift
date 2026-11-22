// HTML's table and description list to the platform's accessibility (LLP
// 1153 D7.3): the roles WebKit serves the same elements as (HTML-AAM), and
// where the kernel's table formation placed each row and cell (`Batch::table`),
// which UIKit's data table and AppKit's `AXTable` answer from.
//
// A description list has no platform equivalent beyond a list: UIKit's
// container types and AppKit's roles have no term or definition, so a `dl`
// is a list holding its terms and definitions, as WebKit serves one.
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The kernel's table formation as the host last sent it: each table's
/// size and drawn order, each row's and cell's place, from 0.
final class TableGrid {
    struct Size: Equatable {
        var rows: Int, columns: Int
        /// The table's children's ids in the order it draws them (CSS 2.2
        /// §17.2: the first `thead` first, the first `tfoot` last); nil when
        /// that is the order written.
        var order: [UInt32]?
    }
    struct Place: Equatable {
        /// The view whose table formation placed it.
        var table: UInt32
        var row: Int, rows: Int, column: Int, columns: Int
        var rowRange: NSRange { NSRange(location: row, length: rows) }
        var columnRange: NSRange { NSRange(location: column, length: columns) }
        func covers(row r: Int) -> Bool { (row..<row + rows).contains(r) }
        func covers(column c: Int) -> Bool { (column..<column + columns).contains(c) }
    }
    private(set) var sizes: [UInt32: Size] = [:]
    private(set) var places: [UInt32: Place] = [:]
    #if os(macOS)
    /// Each table's column elements and header, kept so that AppKit's
    /// clients are served the same objects from one read to the next.
    var columns: [UInt32: [TableColumnElement]] = [:]
    var headers: [UInt32: TableHeaderElement] = [:]
    #endif

    /// A `table` op: `id`'s size or place, or neither.
    func apply(_ id: UInt32, _ payload: [String: Any]) {
        sizes[id] = nil
        places[id] = nil
        let numbers = { (key: String) in (payload[key] as? [NSNumber])?.map(\.intValue) }
        if let size = numbers("size"), size.count == 2 {
            sizes[id] = Size(rows: size[0], columns: size[1], order: (payload["order"] as? [NSNumber])?.map(\.uint32Value))
        } else if let p = numbers("place"), p.count == 4, let table = (payload["table"] as? NSNumber)?.uint32Value {
            places[id] = Place(table: table, row: p[0], rows: p[1], column: p[2], columns: p[3])
        }
    }
    func forget(_ id: UInt32) {
        sizes[id] = nil
        places[id] = nil
        #if os(macOS)
        columns[id] = nil
        headers[id] = nil
        #endif
    }
}

extension NodeView {
    var semanticTag: String { props["semanticTag"] ?? "" }
    /// Its size, when it is a `table` the kernel laid out as one.
    var tableSize: TableGrid.Size? {
        semanticTag == "table" ? presenter?.tableGrid.sizes[id] : nil
    }
    /// Its place, when it is a `tr`, `td` or `th` that the `table` above it
    /// placed (a row outside any table is placed in an anonymous one).
    var tablePlace: TableGrid.Place? {
        guard ["tr", "td", "th"].contains(semanticTag), let place = presenter?.tableGrid.places[id],
              owningTable?.id == place.table else { return nil }
        return place
    }
    var isTableCell: Bool { semanticTag == "td" || semanticTag == "th" }
    /// A `th` heads its row with `scope="row"`, else its column, as the
    /// other hosts read it.
    var headsRow: Bool { semanticTag == "th" && props["scope"] == "row" }
    var headsColumn: Bool { semanticTag == "th" && props["scope"] != "row" }
    /// The nearest `table` above it.
    var owningTable: NodeView? {
        var next = superview
        while let view = next {
            if let node = view as? NodeView, node.semanticTag == "table" { return node }
            next = view.superview
        }
        return nil
    }
    /// The cell it is or is in, of the nearest table above it.
    var owningTableCell: NodeView? {
        var next: PlatformView? = self
        while let view = next {
            if let node = view as? NodeView {
                if node.semanticTag == "table" { return nil }
                if node.isTableCell, node.tablePlace != nil { return node }
            }
            next = view.superview
        }
        return nil
    }
    /// Its rows and cells, each with its place: the views below it that it
    /// placed, a table inside one of its cells aside.
    var tableParts: [(view: NodeView, place: TableGrid.Place)] {
        guard tableSize != nil, let grid = presenter?.tableGrid else { return [] }
        var parts: [(NodeView, TableGrid.Place)] = []
        var stack = childNodes
        while let node = stack.popLast() {
            if node.semanticTag == "table" { continue }
            if let place = grid.places[node.id], place.table == id { parts.append((node, place)) }
            stack += node.childNodes
        }
        return parts.sorted { ($0.1.row, $0.1.column) < ($1.1.row, $1.1.column) }
    }
    var tableRows: [NodeView] { tableParts.filter { $0.view.semanticTag == "tr" }.map(\.view) }
    var tableCells: [(view: NodeView, place: TableGrid.Place)] { tableParts.filter { $0.view.isTableCell } }
    /// The cell covering `row` and `column` of this table.
    func tableCell(row: Int, column: Int) -> NodeView? {
        tableCells.first { $0.place.covers(row: row) && $0.place.covers(column: column) }?.view
    }
    /// The column headers of this table over `columns`, or all of them.
    func columnHeaders(over columns: NSRange? = nil) -> [NodeView] {
        tableCells.filter { cell in
            cell.view.headsColumn && (columns.map { NSIntersectionRange($0, cell.place.columnRange).length > 0 } ?? true)
        }.map(\.view)
    }
    /// The row headers of this table beside `rows`, or all of them.
    func rowHeaders(beside rows: NSRange? = nil) -> [NodeView] {
        tableCells.filter { cell in
            cell.view.headsRow && (rows.map { NSIntersectionRange($0, cell.place.rowRange).length > 0 } ?? true)
        }.map(\.view)
    }
}

#if os(macOS)
extension NodeView {
    /// The AppKit role and subrole of a table's and a description list's
    /// elements: `AXTable`, `AXRow` and `AXCell` (a `th` too, its `scope`
    /// making it a column's or a row's header in ARIA, which AppKit says by
    /// the header elements of a table and its cells, not a role); a row
    /// group is no element of its own. A `dl` is a list with WebKit's
    /// description-list subrole, its `dt` and `dd` groups with the term's
    /// and the description's.
    var tableRole: (role: NSAccessibility.Role, subrole: NSAccessibility.Subrole?)? {
        switch semanticTag {
        case "table": return (.table, nil)
        case "tr": return (.row, nil)
        case "th", "td": return (.cell, nil)
        case "ul", "ol": return (.list, nil)
        case "li", "blockquote": return (.group, nil)
        case "dl": return (.list, NSAccessibility.Subrole(rawValue: "AXDescriptionList"))
        case "dt": return (.group, NSAccessibility.Subrole(rawValue: "AXTerm"))
        case "dd": return (.group, NSAccessibility.Subrole(rawValue: "AXDescription"))
        default: return nil
        }
    }

    /// `children`, a table's, in the order it draws them: each by the place
    /// among the table's own children of the view it is or is in.
    func inDrawnOrder(_ children: [Any]?) -> [Any]? {
        guard let order = tableSize?.order, let children else { return children }
        let rank = Dictionary(order.enumerated().map { ($1, $0) }, uniquingKeysWith: { a, _ in a })
        func key(_ element: Any, _ at: Int) -> Int {
            var next = (element as? NSView) ?? ((element as? NSAccessibilityElement)?.accessibilityParent() as? NSView)
            while let view = next, view.superview !== container { next = view.superview }
            return (next as? NodeView).flatMap { rank[$0.id] } ?? at
        }
        return children.enumerated().map { (key($1, $0), $0, $1) }.sorted { ($0.0, $0.1) < ($1.0, $1.1) }.map(\.2)
    }

    // A table as `NSTableView` and WebKit serve one (the `NSAccessibilityTable`
    // protocol): its rows in the order it draws them, its columns, its
    // headers, and each cell's index ranges.
    package override func accessibilityRowCount() -> Int { tableSize?.rows ?? super.accessibilityRowCount() }
    package override func accessibilityColumnCount() -> Int { tableSize?.columns ?? super.accessibilityColumnCount() }
    package override func accessibilityRows() -> [Any]? { tableSize == nil ? super.accessibilityRows() : tableRows }
    package override func accessibilityVisibleRows() -> [Any]? { tableSize == nil ? super.accessibilityVisibleRows() : tableRows }
    package override func accessibilityColumns() -> [Any]? { tableSize == nil ? super.accessibilityColumns() : tableColumnElements }
    package override func accessibilityVisibleColumns() -> [Any]? { tableSize == nil ? super.accessibilityVisibleColumns() : tableColumnElements }
    /// A table's header: a group of its column headers, as WebKit serves
    /// a table's (`AccessibilityTableHeaderContainer`).
    package override func accessibilityHeader() -> Any? {
        guard tableSize != nil, let grid = presenter?.tableGrid else { return super.accessibilityHeader() }
        guard !columnHeaders().isEmpty else { return nil }
        if let kept = grid.headers[id] { return kept }
        let header = TableHeaderElement(self)
        grid.headers[id] = header
        return header
    }
    /// A table's column headers, or those over a cell's columns.
    package override func accessibilityColumnHeaderUIElements() -> [Any]? {
        if tableSize != nil { return columnHeaders() }
        guard isTableCell, let place = tablePlace, let table = owningTable else { return super.accessibilityColumnHeaderUIElements() }
        return table.columnHeaders(over: place.columnRange).filter { $0 !== self }
    }
    /// A table's row headers, or those beside a cell's rows.
    package override func accessibilityRowHeaderUIElements() -> [Any]? {
        if tableSize != nil { return rowHeaders() }
        guard isTableCell, let place = tablePlace, let table = owningTable else { return super.accessibilityRowHeaderUIElements() }
        return table.rowHeaders(beside: place.rowRange).filter { $0 !== self }
    }
    package override func accessibilityCell(forColumn column: Int, row: Int) -> Any? {
        tableSize == nil ? super.accessibilityCell(forColumn: column, row: row) : tableCell(row: row, column: column)
    }
    package override func accessibilityIndex() -> Int {
        guard semanticTag == "tr", let place = tablePlace else { return super.accessibilityIndex() }
        return place.row
    }
    package override func accessibilityRowIndexRange() -> NSRange {
        guard isTableCell, let place = tablePlace else { return super.accessibilityRowIndexRange() }
        return place.rowRange
    }
    package override func accessibilityColumnIndexRange() -> NSRange {
        guard isTableCell, let place = tablePlace else { return super.accessibilityColumnIndexRange() }
        return place.columnRange
    }
    /// Core-AAM maps a custom sort order to AppKit's unknown direction.
    /// An unsorted header omits the attribute.
    private var headerSort: (direction: NSAccessibilitySortDirection, value: NSAccessibility.SortDirectionValue)? {
        guard let sort = props["accessibilitySort"] else { return nil }
        if let role = props["accessibilityRole"] {
            guard role == "columnheader" || role == "rowheader" else { return nil }
        } else if semanticTag != "th" { return nil }
        switch sort {
        case "ascending": return (.ascending, .ascending)
        case "descending": return (.descending, .descending)
        case "other": return (.unknown, .unknown)
        default: return nil
        }
    }
    package override func accessibilitySortDirection() -> NSAccessibilitySortDirection {
        headerSort?.direction ?? super.accessibilitySortDirection()
    }
    // What another process (VoiceOver) is served goes through AppKit's
    // attribute protocol, which does not serve a view's table accessors by
    // themselves (measured on macOS 27: none of them in
    // `accessibilityAttributeNames()`). `NodeView`'s attribute overrides
    // serve them through these.
    static let tableAttributes: [NSAccessibility.Attribute] = [
        .rows, .visibleRows, .columns, .visibleColumns, .rowCount, .columnCount, .header,
        .columnHeaderUIElements, .rowHeaderUIElements, .index, .rowIndexRange, .columnIndexRange,
    ]
    private static let tableAttributeNames = Set((tableAttributes + [.sortDirection]).map(\.rawValue))
    /// `names` with the table attributes this node has, and none it lacks.
    /// AppKit asks for the names at every accessibility setter and getter
    /// of a view that overrides them, so this filter runs thousands of
    /// times when a batch makes thousands of views: one hash per name, not
    /// a comparison with each table attribute.
    func withTableAttributes(_ names: [NSAccessibility.Attribute]) -> [NSAccessibility.Attribute] {
        var kept = names.filter { !Self.tableAttributeNames.contains($0.rawValue) }
        if headerSort != nil { kept.append(.sortDirection) }
        guard tableSize != nil || tablePlace != nil else { return kept }
        return kept + Self.tableAttributes.filter { tableAttribute($0) != nil }
    }
    func tableAttribute(_ attribute: NSAccessibility.Attribute) -> Any? {
        if attribute == .sortDirection { return headerSort?.value.rawValue }
        if tableSize != nil {
            switch attribute {
            case .rows: return accessibilityRows()
            case .visibleRows: return accessibilityVisibleRows()
            case .columns: return accessibilityColumns()
            case .visibleColumns: return accessibilityVisibleColumns()
            case .rowCount: return accessibilityRowCount()
            case .columnCount: return accessibilityColumnCount()
            case .header: return accessibilityHeader()
            case .columnHeaderUIElements: return accessibilityColumnHeaderUIElements()
            case .rowHeaderUIElements: return accessibilityRowHeaderUIElements()
            default: return nil
            }
        }
        guard tablePlace != nil else { return nil }
        switch (attribute, isTableCell) {
        case (.index, false): return accessibilityIndex()
        case (.rowIndexRange, true): return NSValue(range: accessibilityRowIndexRange())
        case (.columnIndexRange, true): return NSValue(range: accessibilityColumnIndexRange())
        case (.columnHeaderUIElements, true): return accessibilityColumnHeaderUIElements()
        case (.rowHeaderUIElements, true): return accessibilityRowHeaderUIElements()
        default: return nil
        }
    }
    package override func accessibilityParameterizedAttributeNames() -> [NSAccessibility.ParameterizedAttribute] {
        let names = super.accessibilityParameterizedAttributeNames()
        return tableSize == nil || names.contains(.cellForColumnAndRow) ? names : names + [.cellForColumnAndRow]
    }
    /// `AXCellForColumnAndRow`'s parameter is the column and the row.
    package override func accessibilityAttributeValue(_ attribute: NSAccessibility.ParameterizedAttribute, forParameter parameter: Any?) -> Any? {
        guard attribute == .cellForColumnAndRow, tableSize != nil, let at = parameter as? [NSNumber], at.count == 2 else {
            return super.accessibilityAttributeValue(attribute, forParameter: parameter)
        }
        return accessibilityCell(forColumn: at[0].intValue, row: at[1].intValue)
    }
    /// A table's columns, made again when their number changes.
    private var tableColumnElements: [TableColumnElement] {
        guard let size = tableSize, let grid = presenter?.tableGrid else { return [] }
        if let kept = grid.columns[id], kept.count == size.columns { return kept }
        let columns = (0..<size.columns).map { TableColumnElement(self, index: $0) }
        grid.columns[id] = columns
        return columns
    }
}

/// A table's column as AppKit's table serves one (`AXColumn`): its index,
/// the cells in it from the top, and its header.
final class TableColumnElement: NSAccessibilityElement, InspectionOwned {
    private weak var table: NodeView?
    private let index: Int
    init(_ table: NodeView, index: Int) {
        self.table = table
        self.index = index
        super.init()
    }
    var inspectionViewId: UInt32? { table?.id }
    private var cells: [NodeView] { table?.tableCells.filter { $0.place.covers(column: index) }.map(\.view) ?? [] }
    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .column }
    override func accessibilityIndex() -> Int { index }
    override func accessibilityParent() -> Any? { table }
    override func accessibilityChildren() -> [Any]? { cells }
    override func accessibilityHeader() -> Any? { table?.columnHeaders(over: NSRange(location: index, length: 1)).first }
    override func accessibilityFrame() -> NSRect { cells.map { $0.accessibilityFrame() }.reduce(NSRect.null) { $0.union($1) } }
    override func accessibilityWindow() -> Any? { table?.window }
    override func accessibilityTopLevelUIElement() -> Any? { table?.window }
    // Served through the attribute protocol too, which serves neither from
    // the accessors alone (measured on macOS 27).
    override func accessibilityAttributeNames() -> [NSAccessibility.Attribute] {
        super.accessibilityAttributeNames().filter { $0 != .index && $0 != .header } + [.index, .header]
    }
    override func accessibilityAttributeValue(_ attribute: NSAccessibility.Attribute) -> Any? {
        switch attribute {
        case .index: return accessibilityIndex()
        case .header: return accessibilityHeader()
        default: return super.accessibilityAttributeValue(attribute)
        }
    }
}

/// A table's header (`AXHeader`): a group of its column headers.
final class TableHeaderElement: NSAccessibilityElement, InspectionOwned {
    private weak var table: NodeView?
    init(_ table: NodeView) {
        self.table = table
        super.init()
    }
    var inspectionViewId: UInt32? { table?.id }
    private var headers: [NodeView] { table?.columnHeaders() ?? [] }
    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .group }
    override func accessibilityParent() -> Any? { table }
    override func accessibilityChildren() -> [Any]? { headers }
    override func accessibilityFrame() -> NSRect { headers.map { $0.accessibilityFrame() }.reduce(NSRect.null) { $0.union($1) } }
    override func accessibilityWindow() -> Any? { table?.window }
    override func accessibilityTopLevelUIElement() -> Any? { table?.window }
}
#else
extension NodeView {
    /// UIKit's container type for a table (a data table) and a description
    /// list (a list), which VoiceOver announces on entering it.
    var tableContainerType: UIAccessibilityContainerType? {
        switch semanticTag {
        case "table": return .dataTable
        case "dl", "ul", "ol": return .list
        default: return nil
        }
    }
    /// A cell that holds words alone is one element, read by its words, as
    /// a hand-built data table's cell is; one holding a control or a link
    /// is a container, its elements placed by it
    /// (`accessibilityRowRange`).
    var isTableCellElement: Bool {
        guard isTableCell, tablePlace != nil else { return false }
        func words(_ node: NodeView) -> Bool {
            node.childNodes.allSatisfy { child in
                child.isParagraph ? (child.isAccessibilityElement || !child.accessibilityExposed) && child.headingLevel == nil
                    : child.kind == "view" && !child.actsAsButton && child.props["accessibilityRole"] == nil && words(child)
            }
        }
        return words(self)
    }
    /// The traits a cell element has of its own: static text, a `th` a header.
    var tableCellTraits: UIAccessibilityTraits {
        guard isTableCellElement else { return [] }
        return semanticTag == "th" ? [.staticText, .header] : .staticText
    }
    package override var accessibilityTraits: UIAccessibilityTraits {
        get { super.accessibilityTraits.union(tableCellTraits) }
        set { super.accessibilityTraits = newValue.subtracting(tableCellTraits) }
    }
    /// A table's children as it draws them, when that is not as written:
    /// its `accessibilityElements`, so that VoiceOver and the containment
    /// order agree with the drawing.
    var tableElementsInDrawnOrder: [Any]? {
        guard let order = tableSize?.order, let presenter else { return nil }
        return order.compactMap { presenter.views[$0] }.filter { $0.superview === container }
    }
}

// UIKit's data table (`UIAccessibilityContainerDataTable`): a table says its
// size, its cells and its headers, and every node in a cell where the cell
// stands, as WebKit's elements do, so that VoiceOver reads a cell's place
// wherever in it the cursor is. Any other node answers no table.
extension NodeView: UIAccessibilityContainerDataTable, UIAccessibilityContainerDataTableCell {
    package func accessibilityRowCount() -> Int { tableSize?.rows ?? 0 }
    package func accessibilityColumnCount() -> Int { tableSize?.columns ?? 0 }
    package func accessibilityDataTableCellElement(forRow row: Int, column: Int) -> (any UIAccessibilityContainerDataTableCell)? {
        tableSize == nil ? nil : tableCell(row: row, column: column)
    }
    package func accessibilityHeaderElements(forRow row: Int) -> [any UIAccessibilityContainerDataTableCell]? {
        tableSize == nil ? nil : rowHeaders(beside: NSRange(location: row, length: 1))
    }
    package func accessibilityHeaderElements(forColumn column: Int) -> [any UIAccessibilityContainerDataTableCell]? {
        tableSize == nil ? nil : columnHeaders(over: NSRange(location: column, length: 1))
    }
    package func accessibilityRowRange() -> NSRange {
        owningTableCell?.tablePlace?.rowRange ?? NSRange(location: NSNotFound, length: 0)
    }
    package func accessibilityColumnRange() -> NSRange {
        owningTableCell?.tablePlace?.columnRange ?? NSRange(location: NSNotFound, length: 0)
    }
}
#endif
