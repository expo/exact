#if os(iOS)
import ExactKit
import UIKit

/// A row's context menu is its collection view's own (LLP 1021 §5.1), as a
/// hand-built list's is: UIKit lifts the row's cell itself, in its
/// background's shape (the rounded rectangle a selected sidebar row shows),
/// over the ground below it, as it lifts the cells of its own lists. The
/// menu host builds the menu and hears it show, end and commit, as it does
/// for a node's own interaction.
extension GroupedListView {
    func collectionView(_ view: UICollectionView, contextMenuConfigurationForItemsAt paths: [IndexPath], point: CGPoint) -> UIContextMenuConfiguration? {
        guard paths.count == 1, let id = source.itemIdentifier(for: paths[0]) else { return nil }
        return host.presenter.groupedMenu(row: id, at: point, in: view)
    }
    func collectionView(_ view: UICollectionView, willDisplayContextMenu configuration: UIContextMenuConfiguration,
                        animator: UIContextMenuInteractionAnimating?) {
        guard let interaction = view.contextMenuInteraction else { return }
        host.presenter.groupedMenus.contextMenuInteraction?(interaction, willDisplayMenuFor: configuration, animator: animator)
    }
    func collectionView(_ view: UICollectionView, willEndContextMenuInteraction configuration: UIContextMenuConfiguration,
                        animator: UIContextMenuInteractionAnimating?) {
        guard let interaction = view.contextMenuInteraction else { return }
        host.presenter.groupedMenus.contextMenuInteraction?(interaction, willEndFor: configuration, animator: animator)
    }
    func collectionView(_ view: UICollectionView, willPerformPreviewActionForMenuWith configuration: UIContextMenuConfiguration,
                        animator: UIContextMenuInteractionCommitAnimating) {
        guard let interaction = view.contextMenuInteraction else { return }
        host.presenter.groupedMenus.contextMenuInteraction?(interaction, willPerformPreviewActionForMenuWith: configuration, animator: animator)
    }
}
#endif
