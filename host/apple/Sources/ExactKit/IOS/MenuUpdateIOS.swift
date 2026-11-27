// A menu that stays open at a pick (`keepsMenuPresented`) shows what the pick
// changed, as a hand-built UIKit menu with `keepsMenuPresented` rows updates
// its checkmarks through `UIContextMenuInteraction.updateVisibleMenu(_:)`.
// UIKit redraws the menu only for new elements, so the open menu is built again
// from its rows and given back whole; its open submenus are found again by
// their identifiers (`submenuIdentifier`), which keeps them open.
#if os(iOS) || os(tvOS)
import UIKit

/// The menu `items(of:)` last built at the top: its popover, the invoker or
/// source it opened from, whether a row of it stays open at a pick, and its
/// shape, which a rebuild compares so that a batch that changed none of its
/// rows leaves the open menu as it is.
final class KeptMenu {
    private(set) weak var popover: NodeView?
    private(set) weak var opener: NodeView?
    private(set) var incarnation: UInt64 = 0
    var keeps = false
    var shape: [String] = []
    func start(_ pop: NodeView, opener: NodeView?) {
        popover = pop; self.opener = opener; incarnation = pop.incarnation
        keeps = false; shape = []
    }
}

extension MenuHost {
    /// The identifier of the top of every menu Exact gives UIKit, so an
    /// update leaves a menu UIKit made itself (a bar's Back history) alone.
    static let menuIdentifier = UIMenu.Identifier("exact.menu")

    /// A pull-down's rows as its menu shows them. Under a heading outside a bar
    /// the first untitled section's rows are the heading's own, since UIKit
    /// draws an inline section's title only over rows of its own.
    func shown(_ items: [UIMenuElement], of target: NodeView, heading: String, headed: Bool) -> [UIMenuElement] {
        if recordsGiven, !heading.isEmpty { givenHeaders[target.id, default: []].insert(heading) }
        guard headed, let first = items.first as? UIMenu, first.options.contains(.displayInline), first.title.isEmpty else { return items }
        return first.children + items.dropFirst()
    }

    /// After a batch: the open menu with a row that stays open, built again
    /// and given to UIKit when its rows changed. An invoker or source that no
    /// longer opens it leaves it alone, as `sync` has already dismissed it.
    func updateKeptMenu() {
        #if os(iOS)
        guard kept.keeps, let pop = kept.popover, let opener = kept.opener, live(pop), pop.incarnation == kept.incarnation,
              let shown = keptPresentation(of: pop, from: opener) else { return }
        let before = kept.shape
        let items = self.shown(items(of: pop, opener: opener), of: pop, heading: shown.heading, headed: shown.headed)
        guard kept.shape != before else { return }
        let children = shown.headed ? [UIMenu(title: shown.heading, options: .displayInline, children: items)] : items
        var submenus: [UIMenu.Identifier: UIMenu] = [:]
        func index(_ elements: [UIMenuElement]) {
            for case let menu as UIMenu in elements { submenus[menu.identifier] = menu; index(menu.children) }
        }
        index(children)
        for interaction in shown.interactions {
            interaction.updateVisibleMenu { Self.updated($0, children: children, submenus: submenus) }
        }
        #endif
    }

    #if os(iOS)
    /// Where the menu of `pop` opened from `opener` shows now: the
    /// interactions that may show it, and the heading its rows are under.
    private func keptPresentation(of pop: NodeView, from opener: NodeView) -> (interactions: [UIContextMenuInteraction], heading: String, headed: Bool)? {
        if let interaction = context.keptInteraction(showing: pop, from: opener) { return ([interaction], "", false) }
        let inBar = barMenu.map { $0.popover === pop && $0.invoker === opener } ?? false
        guard eligible(opener, inBar: inBar), presenter?.invocation(of: opener)?.target === pop else { return nil }
        let heading = pop.props["accessibilityLabel"] ?? "", headed = !heading.isEmpty && !inBar
        if inBar, let bar = barMenu?.bar() { return (Self.menuInteractions(of: bar), heading, headed) }
        let button: UIButton? = nativeMenus[opener.id].flatMap { $0.isMenuOpen && $0.popover === pop ? $0 : nil }
            ?? opener.subviews.lazy.compactMap { $0 as? MenuButton }.first { $0.isMenuOpen && $0.popover === pop }
        return button?.contextMenuInteraction.map { ([$0], heading, headed) }
    }
    #endif

    /// What a visible menu becomes: Exact's menu takes the new children and
    /// an open submenu the one built for it; UIKit calls this for the menu
    /// and then for each open submenu. Any other menu is left as it is.
    static func updated(_ visible: UIMenu, children: [UIMenuElement], submenus: [UIMenu.Identifier: UIMenu]) -> UIMenu {
        if visible.identifier == menuIdentifier { return visible.replacingChildren(children) }
        return submenus[visible.identifier] ?? visible
    }
}
#endif
