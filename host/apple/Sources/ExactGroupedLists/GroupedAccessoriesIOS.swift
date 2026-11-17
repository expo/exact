// A grouped row whose accessory UIKit draws: a standard row, and a row that
// keeps its views only because its text is Markdown, where the cell draws
// the switch, checkmark, chevron or detail button in place of the view its
// author wrote (`GroupedListModel.Row.accessoryView`). One rule holds for
// every such row, whatever the accessory:
//
// - The cell is the row's one accessibility element, named by the row: its
//   label, else the label of its switch or radio, else the words it shows,
//   a text with a label of its own by that label and then its words. A
//   heading among its texts makes the cell a heading, unless the row is a
//   button, whose content has no roles on the web either.
// - A switch or a radio gives the cell its value and traits, and activating
//   the cell flips or checks it. One its author left out of the
//   accessibility tree (`aria-hidden`) gives it nothing.
// - A row that is `aria-hidden` or inert, or is inside a node that is, has a
//   cell that holds no element. (A row that is `aria-hidden` itself keeps its
//   views by the kernel's rule, `GroupedRow::authored_edge`.)
// - The view the accessory replaces leaves layout, and the row's content is
//   laid out in the room UIKit leaves beside the accessory, measured from
//   the row's cell each time it shows.
//
// A row whose text has runs, a link or a press of its own, that holds an
// image with a press of its own, or whose accessory its author hid
// (`visibility`), is no such row: the kernel leaves every part of it as
// written (`Kernel::grouped_list_with`).
import ExactKit
#if os(iOS) || os(tvOS)
import UIKit

extension GroupedListModel.Row {
    /// Whether UIKit draws the row's accessory, so its cell speaks for it.
    var projected: Bool { !custom || accessoryView != nil }
}

extension GroupedListHost {
    /// What UIKit's accessories cover of the Markdown rows that keep them, told as the margins are (`tell`): the
    /// replaced view whole, so it takes no room in the row, and the row's edges by the room its cell leaves.
    /// Telling a cover lays the rows out and tells again, which must find nothing changed: the rows are read in
    /// one order, so a row with no cell takes the same measured row's room each time. A row with no cell whose
    /// accessory this list has not measured keeps the edges it was told for that accessory, and none told for
    /// another: a radio unchecked off screen gives up its checkmark's room.
    func tellAccessories(_ session: ExactSession) {
        var wanted: [UInt32: HostCover] = [:]
        for list in lists.values {
            var slots: [UInt32: String] = [:]
            for row in list.rows.values.filter({ $0.custom && $0.accessoryView != nil }).sorted(by: { $0.view < $1.view }) {
                guard let accessory = row.accessoryView else { continue }
                wanted[accessory] = .whole
                let slot = list.slot(row)
                if let room = list.room(for: row) { wanted[row.view] = .edges(room) }
                else if list.slots[row.view] == slot, let old = covered[row.view] { wanted[row.view] = old }
                if wanted[row.view] != nil { slots[row.view] = slot }
            }
            list.slots = slots
        }
        var covers = wanted.filter { covered[$0.key] != $0.value }.map { ($0.key, Optional($0.value)) }
        covers += covered.keys.filter { wanted[$0] == nil && presenter.views[$0] != nil }.map { ($0, nil) }
        covered = wanted
        if !covers.isEmpty { session.apply(session.runtime.covers(covers)) }
    }

    /// A cell that comes on screen is laid out, its accessory with it, after UIKit has displayed it: the room it
    /// leaves is told on the next turn of the run loop, whether or not the row's height changed.
    func tellSoon() {
        guard !tellOwed else { return }
        tellOwed = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            tellOwed = false
            tell()
        }
    }
}

extension GroupedListView {
    /// Whether UIKit has no switch, so a toggle row shows its state as a checkmark (`accessories`).
    static var switchless: Bool {
        #if os(tvOS)
        true
        #else
        false
        #endif
    }

    /// The accessory as UIKit lays it out: rows with the same one leave their content the same room. A radio
    /// is a checkmark while it is checked and nothing otherwise, and so is a toggle where UIKit has no switch.
    func slot(_ row: GroupedListModel.Row, switchless: Bool = GroupedListView.switchless) -> String {
        let on: Bool
        switch row.accessory {
        case "radio": on = row.checked
        case "toggle" where switchless: on = row.target.flatMap { host.presenter.views[$0] }?.props["checked"] == "true"
        default: return row.accessory
        }
        return on ? "checkmark" : "none"
    }

    /// The edges of `row`'s cell that UIKit keeps from its content, beyond the cell's margins: measured from the
    /// cell, laid out, when the row has one on screen; else what a row with the same accessory last measured in
    /// this list. A cell UIKit keeps hidden once it has scrolled away is still the row's, and is configured
    /// again only when it next shows: it may hold the accessories of an older row (iOS 27.0).
    func room(for row: GroupedListModel.Row) -> HostCover.Edges? {
        guard let cell = cell(row.view) as? UICollectionViewListCell, !cell.isHidden else { return rooms[slot(row)] }
        cell.layoutIfNeeded()
        let content = cell.contentView, frame = content.convert(content.bounds, to: cell)
        let margin = content.layoutMargins, outer = cell.layoutMargins
        let room = HostCover.Edges(top: 0, right: max(0, cell.bounds.maxX - frame.maxX + margin.right - outer.right),
                                   bottom: 0, left: max(0, frame.minX + margin.left - outer.left))
        rooms[slot(row)] = room
        // The row was carried before this layout, which may have moved the content view it is in.
        if let node = host.presenter.views[row.view], node.superview === content, let place = carried[row.view] {
            let drawn = node.grid.map { Self.cellFrame($0, scale: host.presenter.gridScale).minX } ?? place.frame.minX
            let x = seated(CGRect(x: drawn, y: 0, width: 0, height: 0), row.view, in: cell).minX
            if node.frame.minX != x { node.frame.origin.x = x }
        }
        return room
    }

    /// A carried row's frame in its cell's content view. Beside a kept accessory the row is laid out in the
    /// cell's own space, and UIKit moves the content view clear of an accessory at its left.
    func seated(_ drawn: CGRect, _ id: UInt32, in cell: UICollectionViewListCell) -> CGRect {
        rows[id]?.accessoryView == nil ? drawn : drawn.offsetBy(dx: -cell.contentView.frame.minX, dy: 0)
    }

    /// A row custom only for its Markdown keeps UIKit's accessory: the view its author wrote for it is hidden
    /// while the cell draws it, and shown again when the row is carried no longer (`forget`).
    func carryAccessory(_ id: UInt32) {
        let target = rows[id]?.accessoryView
        let node = target.flatMap { host.presenter.views[$0] }
        if carried[id]?.accessory?.node !== node {
            if let old = carried[id]?.accessory { old.node.isHidden = old.hidden }
            carried[id]?.accessory = node.map { ($0, $0.hiddenByHost) }
        }
        node?.isHidden = true
    }

    /// The texts a row shows, in order, less the view its accessory replaces and what is not shown or is left
    /// out of the accessibility tree.
    private func texts(_ node: NodeView, less accessory: UInt32?) -> [NodeView] {
        if node.id == accessory || node.isHidden || node.style["display"]?.string == "none"
            || node.props["accessibilityElementsHidden"] == "true" { return [] }
        if node.kind == "text" { return [node] }
        return node.container.subviews.compactMap { $0 as? NodeView }.flatMap { texts($0, less: accessory) }
    }

    /// The words a row shows, text by text as a standard cell's title and value are read. A text with a name of
    /// its own (`aria-label`, `aria-labelledby`) says it and then what it shows, as it does by itself
    /// (`NodeView.spokenValue`). A standard row's texts show its cell's strings.
    private func words(_ node: NodeView, _ row: GroupedListModel.Row) -> String {
        let parts = texts(node, less: row.accessoryView)
        var shown = parts.map(\.accessibleText)
        if !row.custom {
            let cell = [row.title, row.secondary].compactMap { $0 }.filter { !$0.isEmpty }
            if cell.count != parts.count { return cell.joined(separator: ", ") }
            shown = cell
        }
        return zip(parts, shown).map { text, shown in
            guard let name = text.authoredLabel else { return shown }
            return shown.isEmpty || shown == name ? name : "\(name), \(shown)"
        }.filter { !$0.isEmpty }.joined(separator: ", ")
    }

    /// The switch or radio `row`'s cell speaks for, unless its author left it out of the accessibility tree.
    private func heard(_ row: GroupedListModel.Row) -> NodeView? {
        guard row.projected, ["toggle", "radio"].contains(row.accessory),
              let control = row.target.flatMap({ host.presenter.views[$0] }),
              control.props["accessibilityElementsHidden"] != "true" else { return nil }
        return control
    }

    /// Whether row `id`, or a node between it and the list, is `aria-hidden`: its cell is inside none of them.
    private func excluded(_ id: UInt32) -> Bool {
        var at: UIView? = host.presenter.views[id]
        while let view = at, view !== owner {
            let node = view as? NodeView
            if node?.props["accessibilityElementsHidden"] == "true" { return true }
            at = node.flatMap { carried[$0.id]?.parent } ?? view.superview
        }
        return false
    }

    /// The name and traits of a cell that speaks for `row`.
    func accessibility(_ cell: UICollectionViewCell, _ id: UInt32, _ row: GroupedListModel.Row) {
        let node = host.presenter.views[id], control = heard(row)
        cell.accessibilityLabel = node?.authoredLabel ?? control?.authoredLabel ?? node.map { words($0, row) }
        cell.accessibilityTraits.remove([.button, .link, .selected, .header])
        if row.pressable { cell.accessibilityTraits.insert(.button) } // whatever it opens, as UIKit's cell
        else if let node, texts(node, less: row.accessoryView).contains(where: { $0.props["accessibilityRole"] == "heading" }) {
            cell.accessibilityTraits.insert(.header)
        }
        let current = node?.props["accessibilityCurrent"].map { !["", "false"].contains($0) } ?? false
        if row.accessory == "checkmark" || (row.accessory == "radio" && row.checked && control != nil)
            || node?.props["accessibilitySelected"] == "true" || current { cell.accessibilityTraits.insert(.selected) }
    }

    /// What a cell says of its row and lets through to it. An inert row's cell takes no touch, by an authored
    /// `inert` only. A cell keeps what it was configured with until a batch changes its row, and a row inserted
    /// while a sheet is up (an expense its form saves) is configured under the sheet, which keeps touches and
    /// VoiceOver off the list by itself.
    func interact(_ cell: UICollectionViewCell, _ id: UInt32) {
        let node = host.presenter.views[id], row = rows[id]
        assign(cell, \.accessibilityIdentifier, node?.props["testId"])
        let inert = carried[id]?.inert ?? node?.authoredInert ?? false
        assign(cell, \.isUserInteractionEnabled, !inert)
        assign(cell, \.accessibilityElementsHidden, inert || excluded(id))
        let radio = row.flatMap { $0.accessory == "radio" ? heard($0) : nil }
        #if os(iOS)
        if let cell = cell as? GroupedCell {
            cell.menuSource = row?.custom == false ? node : nil
            host.presenter.syncGroupedMenu(on: cell)
            cell.speaksForRow = row?.projected == true
            cell.checkRadio = radio == nil ? nil : { [weak self] in
                guard let self, let row = rows[id], self.radio(row) != nil else { return false }
                return select(id)
            }
        }
        #endif
        // A detail button is enabled while its press reaches the row's button, which a batch may change
        // without changing the row.
        if let cell = cell as? GroupedCell, let row, row.projected, row.accessory == "detail" {
            let enabled = detailTarget(row) != nil
            #if os(iOS)
            cell.detailEnabled = enabled
            #else
            if cell.detailEnabled != enabled { cell.accessories = accessories(row, cell: cell) }
            #endif
        }
        if let row, row.projected { accessibility(cell, id, row) }
        else {
            if cell.accessibilityLabel != nil { cell.accessibilityLabel = nil }
            cell.accessibilityTraits.remove([.button, .link, .selected, .header])
        }
        // Reading a list cell's traits searches its subviews: only a cell marked before or now does.
        let unavailable = node?.disabled == true || node?.ariaUnavailable == true || radio?.disabled == true || radio?.ariaUnavailable == true
        if let cell = cell as? GroupedCell, !unavailable, !cell.markedNotEnabled { return }
        (cell as? GroupedCell)?.markedNotEnabled = unavailable
        if unavailable { cell.accessibilityTraits.insert(.notEnabled) } else { cell.accessibilityTraits.remove(.notEnabled) }
    }
}
#endif
