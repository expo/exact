import ExactKit
#if os(iOS)
import UIKit

/// The contact stays on the collection if UIKit replaces the cell's reorder accessory
/// while laying out a proposed move. UIKit still positions, scrolls and drops the row.
final class GroupedReorderContact: UIGestureRecognizer {
    private weak var finger: UITouch?
    private var offset = CGPoint.zero
    private(set) var following = false

    override func canPrevent(_ other: UIGestureRecognizer) -> Bool { false }
    override func canBePrevented(by other: UIGestureRecognizer) -> Bool { false }
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        guard finger == nil, let touch = touches.first else { return }
        finger = touch
    }
    func target(_ point: CGPoint) {
        guard let finger else { return }
        let at = finger.location(in: view)
        offset = CGPoint(x: point.x - at.x, y: point.y - at.y)
    }
    func continueContact() -> Bool {
        guard let finger, [.began, .moved, .stationary].contains(finger.phase) else { return false }
        following = true
        return true
    }
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        guard following, let finger, touches.contains(finger) else { return }
        let at = finger.location(in: view)
        (view as? GroupedCollectionView)?.updateInteractiveMovementTargetPosition(CGPoint(x: at.x + offset.x, y: at.y + offset.y))
    }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        guard let finger, touches.contains(finger) else { return }
        if following { (view as? GroupedCollectionView)?.endInteractiveMovement() }
        state = .failed
    }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        if let finger, touches.contains(finger) { (view as? GroupedCollectionView)?.abortMovement() }
        state = .failed
    }
    override func reset() {
        following = false
        finger = nil
        offset = .zero
        super.reset()
    }
}

extension GroupedListHost {
    func reorderResponder(for grip: NodeView, accessibility: Bool) -> UIView? {
        guard let id = reorderList(for: grip), let list = lists[id] else { return nil }
        for case let cell as GroupedCell in list.collection.visibleCells where cell.grip === grip {
            return accessibility ? cell.reorderControl : cell
        }
        return nil
    }

    func reorderList(for grip: NodeView) -> UInt32? {
        guard let name = grip.props["reorderFor"], !name.isEmpty else { return nil }
        return lists.values.first { list in
            list.owner.props["id"] == name && list.rows.values.contains { $0.reorderGrip == grip.id }
        }?.owner.id
    }
}

extension GroupedListView {
    func reorderAccessory(_ cell: GroupedCell, _ id: UInt32) {
        let grip = rows[id]?.reorderGrip.flatMap { host.presenter.views[$0] }
        cell.grip = grip
        if grip != nil { cell.accessories.append(.reorder(displayed: .always)) }
    }

    func installReordering() {
        source.reorderingHandlers.canReorderItem = { [weak self] id in self?.rows[id]?.reorderGrip != nil }
        source.reorderingHandlers.didReorder = { [weak self] transaction in
            guard let self, let movement, let id = movingRow else { return }
            let old = transaction.initialSnapshot, next = transaction.finalSnapshot
            let section = old.sectionIdentifier(containingItem: id)
            guard let section, next.sectionIdentifier(containingItem: id) == section,
                  old.sectionIdentifiers == next.sectionIdentifiers,
                  old.itemIdentifiers.filter({ $0 != id }) == next.itemIdentifiers.filter({ $0 != id }) else {
                DispatchQueue.main.async { movement.cancel() }
                return
            }
            drop(in: next)
        }
    }

    /// Returning to the source slot leaves UIKit's snapshot unchanged, so UIKit
    /// sends no transaction. Both releases certify the successor in the final order.
    func drop(in snapshot: NSDiffableDataSourceSnapshot<UInt32, UInt32>) {
        guard let movement, let id = movingRow, let section = snapshot.sectionIdentifier(containingItem: id) else { return }
        let order = snapshot.itemIdentifiers(inSection: section)
        guard let index = order.firstIndex(of: id) else { return }
        let before = order.dropFirst(index + 1).first { rows[$0]?.reorderKey != nil }
        movement.queueDrop(before: before)
    }

    func lift(at path: IndexPath) -> Bool {
        guard movingRow == nil, let id = source.itemIdentifier(for: path), let grip = rows[id]?.reorderGrip,
              let handle = host.presenter.views[grip] else { return false }
        movingRow = id
        proposedMove = false
        movement = host.presenter.reorders.nativeMovement(grip: handle, list: owner.id) { [weak self] in
            guard let self else { return }
            collection.abortMovement()
            movingRow = nil
            movement = nil
            DispatchQueue.main.async { [weak self] in
                guard let self, host.lists[owner.id] === self else { return }
                host.sync()
            }
        }
        if movement == nil { movingRow = nil }
        return movement != nil
    }

    func collectionView(_ collectionView: UICollectionView, targetIndexPathForMoveOfItemFromOriginalIndexPath original: IndexPath,
                        atCurrentIndexPath current: IndexPath, toProposedIndexPath proposed: IndexPath) -> IndexPath {
        guard let section = section(at: original.section), !section.rows.isEmpty else { return original }
        let item: Int
        if proposed.section < original.section { item = 0 }
        else if proposed.section > original.section { item = section.rows.count - 1 }
        else { item = min(proposed.item, section.rows.count - 1) }
        proposedMove = original.item != item
        return IndexPath(item: item, section: original.section)
    }
}
#endif
