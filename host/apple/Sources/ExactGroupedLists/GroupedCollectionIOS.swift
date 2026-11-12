import ExactKit
#if os(iOS) || os(tvOS)
import UIKit

/// A grouped list's collection view, by type, for the agent's wheel.
final class GroupedCollectionView: UICollectionView, GroupedScroller {
    /// The offset, which UIKit itself also sets (keeping the list at its
    /// start as a safe area reaches it and insets it): inside an animation
    /// UIKit has open it moves only if the last frame showed the list's node,
    /// as a `ScrollView`'s does (`Presenter.moveOrPlace`).
    override var bounds: CGRect {
        get { super.bounds }
        set {
            guard UIView.inheritedAnimationDuration > 0, let node = superview as? NodeView, let presenter = node.presenter else { return super.bounds = newValue }
            presenter.moveOrPlace(node) { super.bounds = newValue }
        }
    }
    /// The safe area at the sides the collection reaches over (`sideReach`):
    /// the window's, which the host keeps from the views it lays out (LLP
    /// 1123), or a fold's half band. UIKit insets the cards from it, as in a
    /// hand-built list.
    var sideSafeArea: (left: CGFloat, right: CGFloat) = (0, 0) {
        didSet {
            guard sideSafeArea != oldValue else { return }
            safeAreaInsetsDidChange()
            collectionViewLayout.invalidateLayout()
        }
    }
    override var safeAreaInsets: UIEdgeInsets {
        var insets = super.safeAreaInsets
        insets.left = max(insets.left, sideSafeArea.left)
        insets.right = max(insets.right, sideSafeArea.right)
        return insets
    }

    /// The node that is section `index`'s card, read for its `box-shadow`.
    var card: (Int) -> NodeView? = { _ in nil }
    /// One layer per outer shadow a card casts, under every cell.
    private var casters: [CALayer] = []

    override func layoutSubviews() {
        super.layoutSubviews()
        castCardShadows()
    }

    /// A section's `box-shadow` is its card's. UIKit draws the card as its
    /// cells' backgrounds, so layers under the cells cast it from the card's
    /// outline: the frames its first and last cells span, with its node's
    /// corners. The cells are opaque and hide it inside. As CSS paints a
    /// list, the first shadow is on top.
    private func castCardShadows() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        var used = 0
        for section in 0..<numberOfSections {
            let count = numberOfItems(inSection: section)
            guard count > 0, let node = card(section) else { continue }
            let shadows = node.boxShadows.filter { !$0.inset }
            guard !shadows.isEmpty,
                  let first = collectionViewLayout.layoutAttributesForItem(at: IndexPath(item: 0, section: section)),
                  let last = collectionViewLayout.layoutAttributesForItem(at: IndexPath(item: count - 1, section: section))
            else { continue }
            let outline = first.frame.union(last.frame)
            for shadow in shadows.reversed() {
                let reach = 1.5 * shadow.blur + max(abs(shadow.offset.width), abs(shadow.offset.height)) + abs(shadow.spread) + 1
                guard outline.insetBy(dx: -reach, dy: -reach).intersects(bounds) else { continue }
                if used == casters.count { casters.append(CALayer()) }
                let caster = casters[used]
                used += 1
                if caster.superlayer !== layer { layer.insertSublayer(caster, at: 0) }
                caster.frame = outline
                let shape = CGRect(origin: .zero, size: outline.size).insetBy(dx: -shadow.spread, dy: -shadow.spread)
                caster.shadowPath = node.roundedPath(in: shape).cgPath
                caster.shadowColor = shadow.color
                caster.shadowOpacity = 1
                caster.shadowOffset = shadow.offset
                // Core Animation's radius is a deviation; CSS's blur is twice it.
                caster.shadowRadius = shadow.blur / 2
            }
        }
        for caster in casters[used...] { caster.removeFromSuperlayer() }
        casters.removeSubrange(used...)
    }
}

#endif
