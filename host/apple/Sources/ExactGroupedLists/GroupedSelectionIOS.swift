// @ref LLP 1115.000 D11.3, LLP 1127 D7 — which rows of a grouped list are
// selected: a tapped row while its press runs and under the screen it pushed,
// and in a split's sidebar the row whose detail shows (GroupedSidebarIOS.swift).
#if os(iOS) || os(tvOS)
import ExactKit
import UIKit

extension GroupedListHost {
    func radioRow(for radio: NodeView) -> NodeView? {
        var at = radio.superview
        while let view = at {
            if let node = view as? NodeView,
               let row = lists.values.lazy.compactMap({ $0.rows[node.id] }).first {
                return row.custom && row.accessory == "radio" && row.target == radio.id ? node : nil
            }
            at = view.superview
        }
        return nil
    }

    func selectRadioRow(from node: NodeView, pressing target: NodeView?) -> UInt32? {
        var at: UIView? = node
        while let view = at {
            if let node = view as? NodeView {
                guard !node.disabled, !node.inert else { return nil }
                if let list = lists.values.first(where: { $0.rows[node.id] != nil }), let row = list.rows[node.id] {
                    guard row.custom, row.accessory == "radio", target == nil || target === node else { return nil }
                    return list.select(row.view) ? row.view : nil
                }
            }
            at = view.superview
        }
        return nil
    }
}

extension GroupedListView {
    func collectionView(_ view: UICollectionView, shouldSelectItemAt path: IndexPath) -> Bool {
        collectionView(view, shouldHighlightItemAt: path)
    }
    /// A tapped row stays selected while its press runs and, when the press
    /// pushed a screen over the list's, under that screen: it fades as the
    /// reader comes back (`deselect`), as a table view's row does
    /// (`clearsSelectionOnViewWillAppear`; LLP 1115.000 D11.3). A row that
    /// pushed nothing, or opened a sheet, lets go once its press has run.
    func collectionView(_ view: UICollectionView, didSelectItemAt path: IndexPath) {
        let letGo = { for selected in view.indexPathsForSelectedItems ?? [] { view.deselectItem(at: selected, animated: !ExactEnv.agentFreezes) } }
        guard let id = source.itemIdentifier(for: path), pressable(id), let node = host.presenter.views[id] else { return letGo() }
        // For `retainFocus`, selecting a carried row follows its authored parent
        let retains = host.presenter.retainsFocus(node, through: { self.host.logicalParent(of: $0) ?? self.host.presenter.eventParent(of: $0) })
        if !retains { host.presenter.endEditing() }
        // The screen the list is on, read before the press can change it.
        let screen = self.screen
        // A radio's row checks its radio, as a click on the radio does:
        // `input` and `change` carry its value, and the checkmark follows
        // the bound `checked` (LLP 1115.000 D13.2).
        if let row = rows[id], let target = radio(row) { host.presenter.checkRadio(target) }
        guard rows[id]?.pressable == true else { return letGo() }
        // UIKit selected the cell already
        pressSelected(id, on: screen) { host.presenter.press(id) }
    }

    /// The screen the list is on.
    var screen: UIViewController? {
        sequence(first: owner as UIResponder, next: { $0.next }).lazy.compactMap { $0 as? UIViewController }.first
    }

    /// Runs the press of row `id`, its cell selected (by UIKit for a
    /// standard row, by `GroupedListHost.pressSelected` for a custom one), as
    /// UIKit's selection of a tapped row does: through the split's record of
    /// the row whose press made the detail (`listPress`). Then a sidebar's
    /// selection is the detail's row again once the press's batches are
    /// applied, whatever the press did (opened its detail, a sheet, or
    /// nothing); a row whose press pushed a screen over the list's (`screen`,
    /// read before anything the tap ran) stays selected under it, as a table
    /// view's does (`clearsSelectionOnViewWillAppear`, `deselect`); any other
    /// lets go.
    func pressSelected(_ id: UInt32, on screen: UIViewController?, _ press: () -> Void) {
        host.presenter.navigation.listPress(id, in: owner, press)
        if sidebar { return host.presenter.afterBatch { [weak self] in self?.selectDetailRow() } }
        if let screen, let stack = screen.navigationController, stack.viewControllers.contains(screen), stack.topViewController !== screen { return }
        for selected in collection.indexPathsForSelectedItems ?? [] { collection.deselectItem(at: selected, animated: !ExactEnv.agentFreezes) }
    }

    /// The list's screen is coming back into view: its selected rows fade
    /// with the transition, so the fade follows a back swipe's finger, and a
    /// swipe let go of early leaves the row selected under the screen that
    /// stayed. With no transition, at once.
    func deselect(alongside coordinator: UIViewControllerTransitionCoordinator?) {
        // A sidebar beside its detail keeps its row (LLP 1127 D3), as its list's
        // screen appears with the split expanding or the list coming over the detail
        if host.presenter.navigation.showsAsSidebar(owner) { return }
        let paths = collection.indexPathsForSelectedItems ?? []
        guard !paths.isEmpty else { return }
        guard let coordinator, !ExactEnv.agentFreezes else {
            for path in paths { collection.deselectItem(at: path, animated: false) }
            return
        }
        let list = collection
        coordinator.animate(alongsideTransition: { _ in
            for path in paths { list.deselectItem(at: path, animated: true) }
        }, completion: { context in
            guard context.isCancelled else { return }
            for path in paths { list.selectItem(at: path, animated: false, scrollPosition: []) }
        })
    }
}
#endif
