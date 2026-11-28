// Beside its detail, the host shows a split's list as UIKit's sidebar list
// (`UICollectionLayoutListConfiguration.Appearance.sidebar`), its separators
// UIKit's and the row whose detail shows selected, as a hand-built split's
// sidebar keeps it, and no disclosure chevron, since a row there selects rather
// than travels; a list of another style also takes UIKit's ground there.
// Collapsed, the list is its authored style again.
//
// @ref LLP 1127 D17 — a list whose author asks for the sidebar style
// (`listStyle="sidebar"`) is UIKit's sidebar list at every width, on UIKit's
// ground for a sidebar list, with no cards.
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

    /// Whether the list is UIKit's sidebar list, its author's or beside a detail: its separators are UIKit's.
    var sidebarList: Bool {
        #if os(iOS)
        listAppearance == .sidebar
        #else
        false
        #endif
    }

    /// Whether the host shows the list as UIKit's sidebar beside a detail where its author asked for another style:
    /// UIKit's ground, which an authored sidebar has its own of, and a layout of its own.
    var hostSidebar: Bool { sidebar && !authoredSidebar }

    /// Whether the author asked for UIKit's sidebar list (`listStyle="sidebar"`, LLP 1127 D17), at every width: its
    /// rows have no cards for a theme's card role to fill.
    var authoredSidebar: Bool {
        #if os(iOS)
        model.appearance == .sidebar
        #else
        false
        #endif
    }

    /// UIKit's ground for a sidebar list: beside the detail none, where the split's sidebar material is the ground, as a
    /// hand-built split clears its lists there (LLP 1127 D3); elsewhere, for an authored sidebar,
    /// `secondarySystemBackground` in both appearances, as UIKit gives a sidebar list's collection view at its first
    /// layout. The sheet's ground is the plain list's, for the hosts with no sidebar list, so the hem past a viewport's
    /// edge takes this one too.
    var sidebarGround: UIColor? {
        #if os(iOS)
        material ? .clear : authoredSidebar ? .secondarySystemBackground : nil
        #else
        nil
        #endif
    }

    /// A standard row's content configuration. From iOS 18 UIKit adapts it to the list's appearance; before that an
    /// authored sidebar's rows take UIKit's sidebar cells, which iOS 18 replaces with that adaptation. A subtitle row
    /// also has its second line as `secondary`; a value beside the title has no sidebar cell.
    func standardContent(_ row: GroupedListModel.Row) -> UIListContentConfiguration {
        #if os(iOS)
        if #unavailable(iOS 18), authoredSidebar, row.subtitle || row.secondary == nil { return row.subtitle ? .sidebarSubtitleCell() : .sidebarCell() }
        #endif
        return row.subtitle ? .subtitleCell() : row.secondary != nil ? .valueCell() : .cell()
    }

    /// Whether UIKit's sidebar material is the list's ground (`NavigationHosting.sidebarMaterial`): its look and its
    /// selection are the sidebar's while it shows as one (`sidebar`), its ground the material's only under that style.
    var material: Bool { host.presenter.navigation.sidebarMaterial(owner) }

    /// Whether the list shows as UIKit's sidebar now
    /// (`NavigationHosting.showsAsSidebar`); whether that changed since its last update. An authored sidebar keeps
    /// its layout through the change, which drops or restores only its rows' disclosure chevrons.
    func followSidebar() -> Bool {
        // Its own box leaves the compiler's ground out on the material, and takes it back
        if material != onMaterial {
            onMaterial.toggle()
            owner.groundChanged()
        }
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
