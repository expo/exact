#if os(iOS) || os(tvOS)
import UIKit

extension MenuHost {
    /// The order a menu shows its rows in for its trigger's `menuOrder`
    /// `word`; unsaid, UIKit's `.automatic`, which shows a menu that opens
    /// upward last row first (LLP 1115 §2: unsaid, the platform's).
    package static func order(_ word: String?) -> UIContextMenuConfiguration.ElementOrder {
        switch word {
        case "fixed": .fixed
        case "priority": .priority
        default: .automatic
        }
    }
    package func rejectActiveMenuProperties(in pop: NodeView) {
        for row in pop.menuRowsThroughSections {
            row.rejectNativeActiveProperties(on: "UIAction")
        }
    }
    private func opensConfirmation(_ row: NodeView) -> Bool {
        presenter?.invocation(of: row).map { $0.opens && isConfirmation($0.target) } ?? false
    }
    /// The menu grammar, extracted (LLP 1021 D3): button rows, links among
    /// them, become actions, a row that opens another menu its submenu (§5,
    /// "Submenus"), a `role="group"` row an inline section of its own rows
    /// titled by its `aria-label`; any other row is a section boundary. A row's action
    /// presses it, then presents the confirmation or dialog it opens, or any other popover as
    /// if by `opener`, the menu's own invoker (exact2's menu rule, LLP 1021: the menu closing
    /// doesn't take it along). `path` holds the popovers above `pop`, whose rows opened it.
    /// `keeping`: whether a row may keep the menu open at a pick (`keepsOpen`); the keyboard's
    /// edit menu, which UIKit cannot update while it shows, keeps none.
    func items(of pop: NodeView, path: [NodeView] = [], opener: NodeView? = nil, keeping: Bool = true) -> [UIMenuElement] {
        var sections: [(title: String?, items: [UIMenuElement])] = [(nil, [])]
        if path.isEmpty { kept.start(pop, opener: opener) }
        // Under the agent, what UIKit was given, in order (`given`).
        var rows: [GivenRow] = [], headers: Set<String> = []
        defer { if recordsGiven { given[pop.id] = rows; givenHeaders[pop.id] = headers } }
        if recordsGiven {
            if path.isEmpty { presentedMenu = [:] }
            presentedMenu[pop.id] = pop.incarnation
        }
        // The item `row` is, appended to the last section; false for no item.
        func add(_ row: NodeView) -> Bool {
            // A link row its author hides (`display: none`, a call a device cannot make) is no item
            if row.defaultLink != nil, row.style["display"]?.string == "none" { return false }
            row.rejectNativeActiveProperties(on: "UIAction")
            let face = label(of: row)
            if let children = submenu(of: row, path: path + [pop], opener: opener, keeping: keeping) {
                // UIMenu has no disabled state: a disabled or inert opener is a dimmed action.
                // Named by its row, so a rebuild of the open menu keeps it open (`updateKeptMenu`).
                let element: UIMenuElement = row.disabled || row.authoredInert
                    ? UIAction(title: face.title, image: image(of: row), attributes: .disabled) { _ in }
                    : UIMenu(title: face.title, image: image(of: row), identifier: Self.submenuIdentifier(row), children: children)
                element.subtitle = face.subtitle
                kept.shape.append("▸\(row.id).\(row.incarnation)|\(face.title)|\(face.subtitle ?? "")|\(imageKey(of: row))|\(element is UIAction)")
                sections[sections.count - 1].items.append(element)
                rows.append(GivenRow(row, title: element.title, subtitle: element.subtitle))
                return true
            } else if row.handlers.contains("press") || row.defaultLink != nil || opensConfirmation(row) || presenter?.invocation(of: row).map({ $0.opens && $0.target.props["accessibilityRole"] != "menu" }) == true {
                let id = row.id, token = row.incarnation
                let image = image(of: row), keeps = keeping && keepsOpen(row, in: pop, path: path)
                let action = UIAction(title: face.title, image: image) { [weak self] action in
                    if !keeps { self?.barMenuClosed() } // A pick dismisses the menu.
                    // The row this menu showed, not a recycled view's next node,
                    // and still enabled: a disabled or inert button does nothing (HTML)
                    func current() -> NodeView? {
                        guard let row = self?.presenter?.views[id], row.incarnation == token, !row.disabled, !row.authoredInert else { return nil }
                        return row
                    }
                    guard let self, let presenter = self.presenter, let row = current() else { return }
                    // A link follows its link, as a press on it does, unless it opens a popover, as it did before
                    // read as the menu read it, by the row's own inertness: one behind a sheet the menu shows over still acts
                    if row.defaultLink != nil, presenter.invocation(of: row)?.opens != true {
                        // one hidden since the menu was built (`display: none`) follows nothing
                        if row.style["display"]?.string != "none" { presenter.press(id, bubbled: true) }
                        return
                    }
                    if row.handlers.contains("press") { self.presenting(row) { presenter.press(id) } }
                    // Once the menu has gone, what the surviving row opens, from
                    // where UIKit says the menu's presentations come from.
                    guard current() != nil, let invocation = presenter.invocation(of: row), invocation.opens else { return }
                    let target = invocation.target, targetToken = target.incarnation, from = opener?.id
                    if !self.isConfirmation(target), target.props["accessibilityRole"] != "menu", let name = target.props["id"] {
                        return DispatchQueue.main.async {
                            guard current() != nil, target.incarnation == targetToken else { return }
                            if self.isOpen(target) { if invocation.action == .toggle { self.drop(name) } } else { self.agentShow(target, named: name, from: from) }
                        }
                    }
                    #if os(tvOS)
                    let anchorItem: AnyObject? = nil
                    #else
                    let anchorItem: AnyObject? = action.presentationSourceItem
                    #endif
                    DispatchQueue.main.async {
                        guard current() != nil, target.incarnation == targetToken, presenter.views[target.id] === target else { return }
                        _ = self.openConfirmation(from: nil, popover: target, modal: invocation.action == .showModal, anchorItem: anchorItem, dispatchPress: false)
                    }
                }
                action.subtitle = face.subtitle
                if row.props["accessibilityChecked"] == "true" { action.state = .on }
                // As the Mac's menu and iOS's alert have it: an inert row is shown, never chosen.
                if row.disabled || row.authoredInert { action.attributes.insert(.disabled) }
                if row.props["destructive"] == "true" { action.attributes.insert(.destructive) }
                #if os(iOS)
                if keeps { action.attributes.insert(.keepsMenuPresented); kept.keeps = true }
                #endif
                kept.shape.append("\(id).\(token)|\(face.title)|\(face.subtitle ?? "")|\(imageKey(of: row))|\(action.state.rawValue)|\(action.attributes.rawValue)")
                sections[sections.count - 1].items.append(action)
                rows.append(GivenRow(row, title: action.title, subtitle: action.subtitle))
                return true
            }
            return false
        }
        for row in pop.menuRows {
            if row.isMenuSection {
                sections.append((row.menuSectionTitle, []))
                for inner in row.menuRows { _ = add(inner) }
                sections.append((nil, []))
            } else if !add(row), !(sections.last?.items.isEmpty ?? true) {
                sections.append((nil, []))
            }
        }
        let filled = sections.filter { !$0.items.isEmpty }
        kept.shape.append("§" + filled.map { "\($0.title ?? "-"):\($0.items.count)" }.joined(separator: ","))
        if filled.count <= 1, filled.first?.title == nil { return filled.first?.items ?? [] }
        headers = Set(filled.compactMap(\.title).filter { !$0.isEmpty })
        return filled.map { UIMenu(title: $0.title ?? "", options: .displayInline, children: $0.items) }
    }
    /// Whether a pick on `row` leaves its menu open: `keepsMenuPresented` on
    /// the row, else on the menu or submenu it is a row of, as UIKit's
    /// `keepsMenuPresented` attribute. Never for a row that follows a link or
    /// opens something, which would present behind the menu, nor for one that
    /// hides a menu it is in: the author closes it, as the web does.
    /// iOS only: tvOS has no such attribute.
    private func keepsOpen(_ row: NodeView, in pop: NodeView, path: [NodeView]) -> Bool {
        #if os(iOS)
        guard (row.props["keepsMenuPresented"] ?? pop.props["keepsMenuPresented"]) == "true", row.defaultLink == nil else { return false }
        guard let invocation = presenter?.invocation(of: row) else { return true }
        return !invocation.opens && !(path + [pop]).contains { $0 === invocation.target && invocation.action == .hide }
        #else
        return false
        #endif
    }
    static func submenuIdentifier(_ row: NodeView) -> UIMenu.Identifier { UIMenu.Identifier("exact.submenu.\(row.id).\(row.incarnation)") }
    /// The items of the popover `row` opens as a submenu: a menu it shows or
    /// toggles (`Invocation`), not a confirmation, not on `path` (a cycle is
    /// no submenu), with at least one item.
    private func submenu(of row: NodeView, path: [NodeView], opener: NodeView?, keeping: Bool) -> [UIMenuElement]? {
        guard let invocation = presenter?.invocation(of: row), invocation.action == .show || invocation.action == .toggle else { return nil }
        let sub = invocation.target
        guard !isConfirmation(sub), sub.props["accessibilityRole"] == "menu", !path.contains(where: { $0 === sub }) else { return nil }
        let children = items(of: sub, path: path, opener: opener, keeping: keeping)
        return children.isEmpty ? nil : children
    }
}
#endif
