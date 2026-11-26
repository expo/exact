// @ref LLP 1083.000 D4 (amended 2026-11-26) — the room a shown tab bar keeps for the rows the
// author writes between the tab panels and the root tablist.
//
// The tablist UIKit's tab bar replaces is covered whole, so the routes reach under the bar; a row
// written between the panels and it would lie at the page's bottom, under the bar. The kernel
// names such rows from the authored tree (`RootTabs.rowsAbove`). While the bar shows along the
// page's bottom the tablist keeps the bar's room (`HostCover.room`), so CSS lays the rows out on
// the bar; the panel siblings walk lifts each row whose box clears the bar, and each tab's stack
// is cleared of them, since the tab controller's own safe area would grow the bar.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

extension NavigationHost {
    /// The room `list` keeps for the rows above it while the tab bar shows, or nil where it keeps
    /// none and is covered whole: the room laid out, none under the whole cover, grown or shrunk
    /// by how far the lowest row's box still reaches past the top of what UIKit keeps at the
    /// bottom of the tabs (the bar, its accessory and the home indicator). In the column the
    /// kernel's rule asks for, the room moves the rows by its own height.
    func barRoom(for list: NodeView) -> CGFloat? {
        guard let rows = rowsAbove(list), let held = keptBarHeight(), let view = presenter.session?.view,
              let end = rows.compactMap({ rowBox($0, in: view)?.maxY }).max() else { return nil }
        var room: CGFloat = 0
        if case .room(let asked)? = covers[list.id] {
            // Until the layout has the room asked for, the rows' boxes are an older layout's: keep asking for it
            guard abs(list.bounds.height - asked) < 0.5 else { return asked }
            room = asked
        }
        return min(held, max(0, room + end - (view.bounds.maxY - held)))
    }

    /// The rows the kernel names between the panels and `list`, the root tablist, that the bar's
    /// room moves (`RootTabs.rowsAbove`), or nil where there are none.
    func rowsAbove(_ list: NodeView) -> [UInt32]? {
        guard let root = container, let tabs = presenter.rootTabs[root.id], tabs.tablist == list.id, !tabs.rowsAbove.isEmpty else { return nil }
        return tabs.rowsAbove
    }

    /// Where a row named above stands while the bar shows, as the panel siblings walk lifts it:
    /// the top of the bar's room, in `view`'s coordinates, and the rows; nil while the bar hides
    /// or no room is kept.
    func rowsOnBar(in view: UIView) -> (top: CGFloat, rows: Set<UInt32>)? {
        guard let list = adoptedTablist.flatMap({ presenter.views[$0] }), case .room? = covers[list.id],
              let rows = rowsAbove(list), let held = keptBarHeight(), let page = presenter.session?.view else { return nil }
        return (page.convert(CGPoint(x: 0, y: page.bounds.maxY - held), to: view).y, Set(rows))
    }

    /// Clears each tab's stack of the rows on the shown bar, which reach `raised` above the
    /// page's bottom, past the bottom safe area the stack has without this clearance (the bar's,
    /// and what an app's hatch adds). The tab controller's own safe area would grow the bar, so a
    /// hand-built screen insets its tabs' controllers for a bar of its own above the tab bar. A
    /// stack it no longer clears, and one written under another identity, gives its part back.
    func clearRowsOnStacks(_ raised: CGFloat, as id: ContributionsIOS.Identity) {
        let stacks = raised > 0 && !siblings.rowStacksHeld ? primaryStacks : []
        for entry in siblings.rowStacks where entry.identity != id || !stacks.contains(where: { $0 === entry.stack }) {
            entry.stack.flatMap { ContributionsIOS.existing(on: $0) }?.write(entry.identity, .zero)
        }
        // A stack's insets are read only where its ledger is not stepping through its writes, when they are
        // settled; one that is gets its part from the pass that follows
        let settled = { (stack: UIViewController) in ContributionsIOS.existing(on: stack)?.writing != true }
        // The bottom safe area a stack on screen has without this clearance; one off screen is taken to have the
        // same
        let own = { (stack: UIViewController) -> CGFloat? in
            guard let view = stack.viewIfLoaded, view.window != nil else { return nil }
            return view.safeAreaInsets.bottom - (ContributionsIOS.existing(on: stack)?.effect(of: id).bottom ?? 0)
        }
        siblings.rowStacks = stacks.map { .init(stack: $0, identity: id) }
        guard let shown = stacks.lazy.filter(settled).compactMap(own).first else { return }
        for stack in stacks where settled(stack) {
            ContributionsIOS.of(stack).write(id, UIEdgeInsets(top: 0, left: 0, bottom: max(0, raised - (own(stack) ?? shown)), right: 0))
        }
    }

    /// Gives back what each stack holds of Exact's clearance for the rows on the bar, before the
    /// stacks leave Exact's container, and holds off any more (`rowStacksHeld`) until the caller
    /// lets go once they have left or stayed: a write UIKit answers with a safe-area pass on the
    /// spot would otherwise put the clearance back.
    func releaseRowStacks() {
        siblings.rowStacksHeld = true
        let entries = siblings.rowStacks
        siblings.rowStacks = []
        for entry in entries { entry.stack.flatMap { ContributionsIOS.existing(on: $0) }?.write(entry.identity, .zero) }
    }

    /// The box `id`, a row named above, paints over, in `view`'s coordinates, where it paints
    /// something in the page: nil for a row that is hidden, empty, or held by another host (a
    /// flight, an arrangement). A row that is a flat leaf has its view by now (`clearBesideRoutes`).
    private func rowBox(_ id: UInt32, in view: UIView) -> CGRect? {
        guard let node = presenter.views[id], !node.isHidden, !heldElsewhere(node, under: logicalParent(of: node) ?? node) else { return nil }
        let box = paintedBox(node, in: view)
        return box.isNull || box.isEmpty ? nil : box
    }

    /// The height UIKit keeps at the bottom of Exact's own tabs while their bar shows along the
    /// page's bottom: from the top of the bar, or of the accessory above it, to the page's bottom.
    /// Nil while the bar hides, stands at the side or the top, or an app's container owns the tabs.
    func keptBarHeight() -> CGFloat? {
        guard let tabs = tabController as? TabController, tabOwner === tabs, let view = presenter.session?.view,
              tabs.viewIfLoaded?.window != nil else { return nil }
        let bar = tabs.tabBar
        if #available(iOS 18.0, tvOS 18.0, *) {
            guard !tabs.isTabBarHidden else { return nil }
        }
        #if os(iOS)
        guard !bar.traitCollection.placesVerticalBar else { return nil }
        #endif
        guard !bar.isHidden, bar.window != nil else { return nil }
        let frame = bar.convert(bar.bounds, to: view)
        guard frame.width > frame.height, abs(frame.maxY - view.bounds.maxY) < 1 else { return nil }
        var top = frame.minY
        #if os(iOS)
        if #available(iOS 26.0, *), let content = tabs.bottomAccessory?.contentView, content.window != nil {
            top = min(top, content.convert(content.bounds, to: view).minY)
        }
        #endif
        return max(0, view.bounds.maxY - top)
    }
}
#endif
