#if os(iOS) || os(tvOS)
import UIKit

extension MenuHost {
    func rejectActiveMenuProperties(in pop: NodeView) {
        for row in pop.menuRowsThroughSections {
            row.rejectNativeActiveProperties(on: "UIAction")
        }
    }
    private func opensConfirmation(_ row: NodeView) -> Bool {
        presenter?.invocation(of: row).map { $0.opens && isConfirmation($0.target) } ?? false
    }
    /// The menu grammar, extracted (LLP 1021 D3): button rows become
    /// actions, a row that opens another menu its submenu (§5,
    /// "Submenus"), a `role="group"` row an inline section of its own rows
    /// titled by its `aria-label`; any other row is a section boundary. A row's action
    /// presses it, then presents the confirmation or dialog it opens, or any other popover as
    /// if by `opener`, the menu's own invoker (exact2's menu rule, LLP 1021: the menu closing
    /// doesn't take it along). `path` holds the popovers above `pop`, whose rows opened it.
    func items(of pop: NodeView, path: [NodeView] = [], opener: NodeView? = nil) -> [UIMenuElement] {
        var sections: [(title: String?, items: [UIMenuElement])] = [(nil, [])]
        // Under the agent, what UIKit was given, in order (`given`).
        var rows: [GivenRow] = [], headers: Set<String> = []
        defer { if recordsGiven { given[pop.id] = rows; givenHeaders[pop.id] = headers } }
        if recordsGiven {
            if path.isEmpty { presentedMenu = [:] }
            presentedMenu[pop.id] = pop.incarnation
        }
        // The item `row` is, appended to the last section; false for no item.
        func add(_ row: NodeView) -> Bool {
            row.rejectNativeActiveProperties(on: "UIAction")
            let face = label(of: row)
            if let children = submenu(of: row, path: path + [pop], opener: opener) {
                // UIMenu has no disabled state: a disabled or inert opener is a dimmed action.
                let element: UIMenuElement = row.disabled || row.authoredInert
                    ? UIAction(title: face.title, image: image(of: row), attributes: .disabled) { _ in }
                    : UIMenu(title: face.title, image: image(of: row), children: children)
                element.subtitle = face.subtitle
                sections[sections.count - 1].items.append(element)
                rows.append(GivenRow(row, title: element.title, subtitle: element.subtitle))
                return true
            } else if row.handlers.contains("press") || opensConfirmation(row) || presenter?.invocation(of: row).map({ $0.opens && $0.target.props["accessibilityRole"] != "menu" }) == true {
                let id = row.id, token = row.incarnation
                let image = image(of: row)
                let action = UIAction(title: face.title, image: image) { [weak self] action in
                    // The row this menu showed, not a recycled view's next node,
                    // and still enabled: a disabled or inert button does nothing (HTML)
                    func current() -> NodeView? {
                        guard let row = self?.presenter?.views[id], row.incarnation == token, !row.disabled, !row.authoredInert else { return nil }
                        return row
                    }
                    guard let self, let presenter = self.presenter, let row = current() else { return }
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
        if filled.count <= 1, filled.first?.title == nil { return filled.first?.items ?? [] }
        headers = Set(filled.compactMap(\.title).filter { !$0.isEmpty })
        return filled.map { UIMenu(title: $0.title ?? "", options: .displayInline, children: $0.items) }
    }
    /// The items of the popover `row` opens as a submenu: a menu it shows or
    /// toggles (`Invocation`), not a confirmation, not on `path` (a cycle is
    /// no submenu), with at least one item.
    private func submenu(of row: NodeView, path: [NodeView], opener: NodeView?) -> [UIMenuElement]? {
        guard let invocation = presenter?.invocation(of: row), invocation.action == .show || invocation.action == .toggle else { return nil }
        let sub = invocation.target
        guard !isConfirmation(sub), sub.props["accessibilityRole"] == "menu", !path.contains(where: { $0 === sub }) else { return nil }
        let children = items(of: sub, path: path, opener: opener)
        return children.isEmpty ? nil : children
    }
}
#endif
