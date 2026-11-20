// @ref LLP 1127 D7 — beside its detail, a split's list is UIKit's sidebar
// list (`UICollectionLayoutListConfiguration.Appearance.sidebar`): no
// separators, no disclosure chevrons, UIKit's sidebar ground, and the row
// whose detail shows selected, as a hand-built split's sidebar keeps it.
// Collapsed, the list is its authored style again.
#if os(iOS) || os(tvOS)
import ExactKit
import UIKit

extension GroupedListView {
    /// The list's appearance in its layout: the sidebar's beside a detail, else the authored one.
    var listAppearance: UICollectionLayoutListConfiguration.Appearance {
        #if os(iOS)
        if sidebar { return .sidebar }
        #endif
        return model.appearance
    }

    /// Whether the list shows as UIKit's sidebar now
    /// (`NavigationHosting.showsAsSidebar`); whether that changed since its last update.
    func followSidebar() -> Bool {
        let beside = host.presenter.navigation.showsAsSidebar(owner)
        guard beside != sidebar else { return false }
        sidebar = beside
        return true
    }

    /// The sidebar's row whose detail shows is selected, and no other. UIKit
    /// keeps a sidebar's selection, where a table's default
    /// `clearsSelectionOnViewWillAppear` cleared it as the split expanded
    /// again (LLP 1127 §1, item 7). While the runner cannot say which row
    /// that is, the selection stays.
    func selectDetailRow() {
        guard sidebar, let shown = host.presenter.navigation.sidebarSelection(owner) else { return }
        let wanted = shown.flatMap { source.indexPath(for: $0) }
        let current = collection.indexPathsForSelectedItems ?? []
        for path in current where path != wanted { collection.deselectItem(at: path, animated: false) }
        if let wanted, !current.contains(wanted) { collection.selectItem(at: wanted, animated: false, scrollPosition: []) }
    }
}
#endif
