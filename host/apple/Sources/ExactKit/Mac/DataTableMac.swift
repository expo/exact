// LLP 1115.002, data table columns. The app owns row order. AppKit owns
// the headers and reports a resized column to the same grid that lays out its cells.
#if os(macOS)
import AppKit

private struct DataRow {
    let node: NodeView
    let cells: [NodeView]
}

private struct DataGrid {
    let header: DataRow
    let rows: [DataRow]

    /// The kernel still sizes the projected rows. Other declarations on a
    /// row or header, such as a background or a styled title, need its authored view.
    private static let sizing: Set<String> = ["width", "height", "min_width", "min_height", "max_width", "max_height", "direction"]
    private static func plain(_ node: NodeView) -> Bool {
        (node.style["written"]?.array ?? []).allSatisfy { $0.string.map(sizing.contains) == true }
    }

    init?(_ node: NodeView) {
        guard node.style["appearance"]?.string != "none", node.style["display"] == nil, let size = node.tableSize, size.columns > 0 else { return nil }
        // AppKit has a single header row and rectangular cells. Spanning
        // headers and cells retain the authored table and its AX relationships.
        guard let places = node.presenter?.tableGrid.places else { return nil }
        let written = node.childNodes.flatMap { child -> [NodeView] in
            if child.semanticTag == "tr" { return [child] }
            return ["thead", "tbody", "tfoot"].contains(child.semanticTag) ? child.childNodes : []
        }
        let rows = written.compactMap { row -> (Int, DataRow)? in
            guard row.semanticTag == "tr", let place = places[row.id], place.table == node.id else { return nil }
            let cells = row.childNodes.filter { places[$0.id]?.table == node.id }
            return (place.row, DataRow(node: row, cells: cells))
        }.sorted { $0.0 < $1.0 }.map(\.1)
        guard let first = rows.first, first.node.nodeSuperview?.semanticTag == "thead",
              first.cells.count == size.columns,
              first.cells.allSatisfy({ head in head.headsColumn && Self.plain(head) &&
                  head.childNodes.allSatisfy { $0.isParagraph && Self.plain($0) && $0.childNodes.isEmpty } }),
              node.childNodes.filter({ ["thead", "tbody", "tfoot"].contains($0.semanticTag) }).allSatisfy(Self.plain),
              rows.allSatisfy({ row in Self.plain(row.node) && row.cells.enumerated().allSatisfy { column, cell in
                  guard let place = places[cell.id] else { return false }
                  return place.column == column && place.columns == 1 && place.rows == 1
              } }),
              !rows.dropFirst().contains(where: { $0.node.nodeSuperview?.semanticTag == "thead" }) else { return nil }
        header = first
        self.rows = Array(rows.dropFirst())
    }
}

/// A cell keeps its authored contents, including controls, when AppKit
/// mounts it. Offscreen contents return to their authored row.
final class DataTableCell: NSTableCellView {
    private(set) weak var home: NodeView?
    private(set) weak var content: NodeView?
    override var isFlipped: Bool { true }

    func mount(_ node: NodeView, from row: NodeView) {
        if content !== node { restore() }
        if let former = node.superview as? DataTableCell, former !== self { former.release(node) }
        home = row
        content = node
        if node.superview !== self { addSubview(node) }
        place()
    }

    private func release(_ node: NodeView) {
        if content === node { content = nil; home = nil }
    }

    func place() {
        guard let node = content, node.superview === self else { return }
        let size = node.grid?.layout.size ?? bounds.size
        let frame = NSRect(origin: .zero, size: size)
        if node.frame != frame { node.frame = frame }
    }

    func restore() {
        defer { home = nil; content = nil }
        guard let node = content, node.superview === self else { return }
        if let home, let p = node.presenter, p.views[node.id] === node, p.views[home.id] === home {
            home.container.addSubview(node)
            if let frame = node.grid?.layout { p.applyFrame(node, frame) }
        } else {
            node.removeFromSuperview()
        }
    }
}

final class DataTableScrollView: NSScrollView {}

final class DataTableHost {
    private weak var presenter: Presenter?
    private var feeds = [UInt32: DataTableFeed]()
    /// Only rows with cells that AppKit can take out of the authored view tree.
    private var rowChildren = [UInt32: [UInt32]]()
    init(_ presenter: Presenter) { self.presenter = presenter }

    func children(_ row: UInt32, _ want: [NodeView]) {
        if rowChildren[row] != nil { rowChildren[row] = want.map(\.id) }
    }

    func children(of row: NodeView) -> [NodeView]? {
        guard let children = rowChildren[row.id], let p = presenter else { return nil }
        return NodeView.committed(children.compactMap { p.views[$0] }, reader: row)
    }

    func native(_ node: NodeView) -> NSTableView? { feeds[node.id]?.table }

    func authoredRow(of cell: NodeView) -> NodeView? {
        guard cell.isTableCell, let place = presenter?.tableGrid.places[cell.id] else { return nil }
        return feeds[place.table]?.row(at: place.row)
    }

    func shows(_ node: NodeView) -> Bool? {
        guard node.semanticTag == "th" || node.semanticTag == "tr",
              let place = presenter?.tableGrid.places[node.id], let feed = feeds[place.table],
              feed.hasHeader(node) || feed.row(at: place.row) === node else { return nil }
        return !feed.table.isHiddenOrHasHiddenAncestor && !node.cssVisibilityHidden
    }
    func sync(_ batch: Batch? = nil) {
        guard let p = presenter, !p.tableGrid.sizes.isEmpty || !feeds.isEmpty else { return }
        var changed = Set<UInt32>()
        if let batch {
            for op in batch.ops {
                switch op.op {
                case .table, .props, .style, .children, .paragraph, .flow, .frame, .destroy, .create:
                    if p.tableGrid.sizes[op.id] != nil || feeds[op.id] != nil { changed.insert(op.id) }
                    if let owner = p.tableGrid.places[op.id]?.table ?? p.views[op.id]?.owningTable?.id { changed.insert(owner) }
                default: continue
                }
            }
        } else {
            changed.formUnion(p.tableGrid.sizes.keys)
            changed.formUnion(feeds.keys)
        }
        for id in changed {
            let previous = feeds[id]?.rowIDs ?? []
            guard !p.tables.painted, let node = p.views[id], !node.held, let grid = DataGrid(node) else {
                feeds.removeValue(forKey: id)?.remove()
                for row in previous { rowChildren[row] = nil }
                continue
            }
            let current = Set(grid.rows.map { $0.node.id })
            for row in previous.subtracting(current) { rowChildren[row] = nil }
            for row in grid.rows where rowChildren[row.node.id] == nil {
                rowChildren[row.node.id] = row.node.childNodes.map(\.id)
            }
            let feed = feeds[id] ?? DataTableFeed(node)
            feeds[id] = feed
            feed.sync(grid)
        }
    }

    func reset() {
        for feed in feeds.values { feed.remove() }
        feeds.removeAll()
        rowChildren.removeAll()
    }
}

/// Only a divider the person moved gets a stored width. Untouched columns
/// keep following their content. Named windows restore their widths on reopen.
private final class WindowColumnWidths: NSObject {
    private static let windows = NSMapTable<NSWindow, WindowColumnWidths>.weakToStrongObjects()
    private var tables = [String: [String: Double]]()
    private var savedUnder: String?

    static func forWindow(_ window: NSWindow) -> WindowColumnWidths {
        let widths = windows.object(forKey: window) ?? WindowColumnWidths()
        windows.setObject(widths, forKey: window)
        let name = window.frameAutosaveName
        if !name.isEmpty {
            let key = "Exact.tableColumnWidths." + name
            if widths.savedUnder != key {
                let saved = UserDefaults.standard.dictionary(forKey: key) as? [String: [String: Double]] ?? [:]
                widths.tables = widths.savedUnder == nil ? saved.merging(widths.tables) { $0.merging($1) { _, current in current } } : saved
                widths.savedUnder = key
            }
        }
        return widths
    }

    func width(_ table: String, _ column: String) -> CGFloat? {
        guard let width = tables[table]?[column], width.isFinite, width >= 0 else { return nil }
        return CGFloat(width)
    }

    func set(_ width: CGFloat, table: String, column: String) {
        tables[table, default: [:]][column] = Double(width)
        if let savedUnder { UserDefaults.standard.set(tables, forKey: savedUnder) }
    }

    static func tableKey(_ node: NodeView) -> String {
        var path = [String]()
        var next: NodeView? = node
        while let view = next {
            if let name = view.props["id"] ?? view.props["testId"] {
                path.append("id:" + name)
                break
            }
            let parent = view.nodeSuperview
            let index = parent?.childNodes.firstIndex(where: { $0 === view }) ?? 0
            path.append(String(index))
            next = parent
        }
        return path.joined(separator: "/")
    }
}

private final class DataTableFeed: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    weak var node: NodeView?
    let table = NSTableView()
    private let scroll = DataTableScrollView()
    private var columns = [UInt32: NSTableColumn]()
    private var rows = [DataRow]()
    private var headers = [NodeView]()
    private var resized = Set<UInt32>()
    private var columnKeys = [UInt32: String]()
    private lazy var tableKey = node.map(WindowColumnWidths.tableKey) ?? ""
    private var measured = [UInt32: CGSize]()
    private var projected = [NodeView]()
    private let naturalHeaderHeight: CGFloat
    private var syncing = false
    private let mounted = NSHashTable<DataTableCell>.weakObjects()

    init(_ node: NodeView) {
        self.node = node
        naturalHeaderHeight = table.headerView?.frame.height ?? 0
        super.init()
        table.style = .plain
        table.allowsColumnReordering = false
        table.allowsColumnResizing = true
        table.columnAutoresizingStyle = .noColumnAutoresizing
        table.backgroundColor = .clear
        table.selectionHighlightStyle = .none
        table.delegate = self
        table.dataSource = self
        scroll.drawsBackground = false
        scroll.borderType = .noBorder
        scroll.documentView = table
        node.container.addSubview(scroll)
    }

    private func report(_ widths: [(UInt32, CGFloat?)]) {
        guard !widths.isEmpty, let session = node?.presenter?.session else { return }
        session.whenIdle(bound: true) { [weak session] in
            guard let session else { return }
            session.applyUnlessEmpty(session.runtime.tableColumnWidths(widths))
        }
    }

    var rowIDs: Set<UInt32> { Set(rows.map { $0.node.id }) }

    func row(at index: Int) -> NodeView? {
        let body = index - 1
        return rows.indices.contains(body) ? rows[body].node : nil
    }

    func hasHeader(_ view: NodeView) -> Bool { headers.contains { $0 === view } }

    func remove() {
        syncing = true
        for row in projected { row.sectionInTable = false }
        table.delegate = nil
        table.dataSource = nil
        for cell in mounted.allObjects { cell.restore() }
        if let p = node?.presenter {
            for row in rows where p.views[row.node.id] === row.node {
                p.orderChildren(row.node.childNodes, in: row.node.container)
            }
        }
        scroll.removeFromSuperview()
        report(resized.map { ($0, nil) })
        node?.presenter?.onIntrinsic?(measured.keys.compactMap { id in node?.presenter?.views[id] == nil ? nil : (id, nil) })
    }

    func sync(_ grid: DataGrid) {
        guard let node else { return }
        syncing = true
        defer { syncing = false }
        let former = rows
        rows = grid.rows
        let logical = grid.header.cells
        let rightToLeft = logical.count > 1 && logical[0].frame.midX > logical[1].frame.midX
        let physical = rightToLeft ? Array(logical.reversed()) : logical
        let headersChanged = !headers.elementsEqual(physical, by: { $0 === $1 })
        headers = physical
        let project = [grid.header.node] + rows.map(\.node)
        let keeping = Set(project.map(ObjectIdentifier.init))
        for row in projected where !keeping.contains(ObjectIdentifier(row)) { row.sectionInTable = false }
        for row in project where !row.sectionInTable { row.sectionInTable = true }
        projected = project
        var sizes = [(UInt32, CGSize?)]()
        let ids = Set(headers.map(\.id))
        for (id, column) in columns where !ids.contains(id) {
            table.removeTableColumn(column)
            columns[id] = nil
            columnKeys[id] = nil
            if measured.removeValue(forKey: id) != nil, node.presenter?.views[id] != nil { sizes.append((id, nil)) }
            if resized.remove(id) != nil { report([(id, nil)]) }
        }
        let state = node.window.map(WindowColumnWidths.forWindow)
        var restored = [(UInt32, CGFloat?)]()
        var titleCounts = [String: Int]()
        for (index, head) in headers.enumerated() {
            let key = String(head.id)
            let title = head.accessibleText
            let occurrence = titleCounts[title, default: 0]
            titleCounts[title] = occurrence + 1
            let columnKey = (head.props["id"] ?? head.props["testId"]).map { "id:" + $0 } ?? "title:" + String(occurrence) + ":" + title
            columnKeys[head.id] = columnKey
            let column = columns[head.id] ?? NSTableColumn(identifier: .init(key))
            column.minWidth = 0
            if column.title != title { column.title = title }
            let natural = CGSize(width: column.headerCell.cellSize.width, height: naturalHeaderHeight)
            if natural.width > 0, natural.height > 0, measured[head.id] != natural {
                sizes.append((head.id, natural))
                measured[head.id] = natural
            }
            let direction = head.props["accessibilitySort"]
            if direction != nil, column.sortDescriptorPrototype == nil {
                column.sortDescriptorPrototype = NSSortDescriptor(key: key, ascending: true)
            } else if direction == nil, column.sortDescriptorPrototype != nil { column.sortDescriptorPrototype = nil }
            if columns[head.id] == nil {
                columns[head.id] = column
                table.addTableColumn(column)
            }
            if let from = table.tableColumns.firstIndex(of: column), from != index { table.moveColumn(from, toColumn: index) }
            let width = head.grid?.layout.width ?? head.frame.width
            if !resized.contains(head.id) {
                if let stored = state?.width(tableKey, columnKey) {
                    resized.insert(head.id)
                    if column.width != stored { column.width = stored }
                    restored.append((head.id, stored))
                } else if column.width != width { column.width = width }
            }
        }
        syncSort()

        if !sizes.isEmpty { node.presenter?.onIntrinsic?(sizes) }
        report(restored)
        if table.accessibilityLabel() != node.authoredLabel { table.setAccessibilityLabel(node.authoredLabel) }
        let first = grid.header.node.convert(grid.header.node.bounds, to: node.container)
        let last = rows.last?.node ?? grid.header.node
        let bottom = last.convert(last.bounds, to: node.container).maxY
        let gap = headers.count > 1 ? headers[1].frame.minX - headers[0].frame.maxX : 0
        let next = rows.first?.node
        let gapY = next.map { $0.convert($0.bounds, to: node.container).minY - first.maxY } ?? 0
        let spacing = NSSize(width: max(0, gap), height: max(0, gapY))
        if table.intercellSpacing != spacing { table.intercellSpacing = spacing }
        let left = headers[0].convert(headers[0].bounds, to: node.container).minX - spacing.width / 2
        let width = table.tableColumns.reduce(CGFloat.zero) { $0 + $1.width + spacing.width }
        let bounds = NSRect(x: left, y: first.minY, width: width, height: max(0, bottom - first.minY))
        if scroll.frame != bounds { scroll.frame = bounds }
        if let header = table.headerView, header.frame.height != first.height + spacing.height / 2 {
            header.setFrameSize(NSSize(width: header.frame.width, height: first.height + spacing.height / 2))
        }
        let rowsChanged = former.count != rows.count || !zip(former, rows).allSatisfy {
            $0.node === $1.node && $0.cells.elementsEqual($1.cells, by: { $0 === $1 })
        }
        if rowsChanged || headersChanged { table.reloadData() }
        else if !rows.isEmpty {
            let changed = IndexSet(rows.indices.filter { abs(table.rect(ofRow: $0).height - (rows[$0].node.frame.height + table.intercellSpacing.height)) > 0.5 })
            if !changed.isEmpty { table.noteHeightOfRows(withIndexesChanged: changed) }
        }
        scroll.tile()
        table.layoutSubtreeIfNeeded()
        for cell in mounted.allObjects { cell.place() }
    }

    func numberOfRows(in tableView: NSTableView) -> Int { rows.count }
    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat { rows[row].node.frame.height }
    func tableView(_ tableView: NSTableView, shouldSelectRow row: Int) -> Bool { false }

    func tableView(_ tableView: NSTableView, viewFor column: NSTableColumn?, row index: Int) -> NSView? {
        guard let column, rows.indices.contains(index), let id = UInt32(column.identifier.rawValue),
              let indexOfColumn = node?.presenter?.tableGrid.places[id]?.column else { return nil }
        let row = rows[index]
        guard row.cells.indices.contains(indexOfColumn) else { return nil }
        let key = NSUserInterfaceItemIdentifier("ExactDataCell")
        let cell = tableView.makeView(withIdentifier: key, owner: self) as? DataTableCell ?? DataTableCell()
        cell.identifier = key
        cell.mount(row.cells[indexOfColumn], from: row.node)
        mounted.add(cell)
        return cell
    }

    func tableView(_ tableView: NSTableView, didRemove rowView: NSTableRowView, forRow row: Int) {
        for index in 0..<rowView.numberOfColumns { (rowView.view(atColumn: index) as? DataTableCell)?.restore() }
    }

    private func syncSort() {
        let sorted = headers.compactMap { head -> NSSortDescriptor? in
            let order = head.props["accessibilitySort"]
            guard order == "ascending" || order == "descending" else { return nil }
            return NSSortDescriptor(key: String(head.id), ascending: order == "ascending")
        }
        guard table.sortDescriptors != sorted else { return }
        let previous = syncing
        syncing = true
        table.sortDescriptors = sorted
        syncing = previous
    }

    func tableView(_ tableView: NSTableView, sortDescriptorsDidChange oldDescriptors: [NSSortDescriptor]) {
        guard !syncing else { return }
        if let key = tableView.sortDescriptors.first?.key, let id = UInt32(key),
           let head = headers.first(where: { $0.id == id }), head.pressable { head.presenter?.press(head.id) }
        syncSort()
    }

    func tableView(_ tableView: NSTableView, didClick column: NSTableColumn) {
        guard column.sortDescriptorPrototype == nil, let id = UInt32(column.identifier.rawValue),
              let head = headers.first(where: { $0.id == id }), head.pressable else { return }
        head.presenter?.press(head.id)
    }

    func tableViewColumnDidResize(_ notification: Notification) {
        guard !syncing, let column = notification.userInfo?["NSTableColumn"] as? NSTableColumn,
              let id = UInt32(column.identifier.rawValue), columns[id] === column else { return }
        resized.insert(id)
        if let window = node?.window, let key = columnKeys[id] {
            WindowColumnWidths.forWindow(window).set(column.width, table: tableKey, column: key)
        }
        report([(id, column.width)])
    }
}
#endif
