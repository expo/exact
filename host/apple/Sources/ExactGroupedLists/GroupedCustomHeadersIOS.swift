// A custom section header on UIKit (LLP 1084 D7): a `header` that holds more
// than one `text` (an image beside a name) keeps its own views, carried into
// UIKit's header as a custom row's are into its cell, at the kernel's
// layout. UIKit still owns the header: its place, its pinning in a plain
// list and the ground it draws behind a pinned header.
import ExactKit
#if os(iOS) || os(tvOS)
import UIKit

/// A section's header: UIKit's own, or a custom header's views at the
/// kernel's height.
final class GroupedHeaderView: UICollectionViewListCell {
    var height: CGFloat?
    /// Whether it speaks for a custom header's views: one heading named by
    /// the header's texts, as UIKit's own header is one element.
    var speaks = false
    override var isAccessibilityElement: Bool {
        get { speaks || super.isAccessibilityElement }
        set { super.isAccessibilityElement = newValue }
    }
    override var accessibilityTraits: UIAccessibilityTraits {
        get { speaks ? super.accessibilityTraits.union(.header) : super.accessibilityTraits }
        set { super.accessibilityTraits = newValue }
    }
    override func preferredLayoutAttributesFitting(_ attributes: UICollectionViewLayoutAttributes) -> UICollectionViewLayoutAttributes {
        guard let height else { return super.preferredLayoutAttributesFitting(attributes) }
        let fitted = attributes.copy() as! UICollectionViewLayoutAttributes
        fitted.size.height = height
        return fitted
    }
}

extension GroupedListModel.Section {
    var hasHeader: Bool { header != nil || headerView != nil }
}

extension GroupedListView {
    /// Section `index`'s custom header in `view`; false, with `view` given
    /// back to UIKit's own content, for a section with none.
    func show(customHeader index: Int, in view: GroupedHeaderView) -> Bool {
        guard let section = section(at: index), section.headerView != nil else {
            for case let node as NodeView in view.contentView.subviews { sendHome(node) }
            if view.height != nil {
                view.accessibilityLabel = nil
                view.accessibilityIdentifier = nil
                view.accessibilityElementsHidden = false
            }
            view.height = nil
            view.speaks = false
            return false
        }
        view.contentConfiguration = nil
        view.marginsIgnoreSafeArea()
        carry(header: section, into: view, sized: false)
        return true
    }

    /// A custom header's own views in UIKit's header, the header's box
    /// where the kernel put it in the list's content box, as tall as the
    /// kernel laid it out.
    func carry(header section: GroupedListModel.Section, into view: GroupedHeaderView, sized: Bool = true) {
        guard let id = section.headerView, let node = host.presenter.views[id] else { return }
        for case let other as NodeView in view.contentView.subviews where other !== node { sendHome(other) }
        if carried[id] == nil, let parent = node.superview {
            noteHome(parent)
            carried[id] = (parent, node.frame, node.authoredInert, nil, nil)
        }
        guard let place = carried[id] else { return }
        let drawn = node.grid.map { Self.cellFrame($0, scale: host.presenter.gridScale) }
            ?? CGRect(origin: CGPoint(x: place.frame.minX, y: 0), size: place.frame.size)
        // Never invalidated here, as a custom row's cell is not (`carry`).
        view.height = drawn.height
        if headerHeights.updateValue(drawn.height, forKey: id).map({ $0 != drawn.height }) ?? (sized && abs(view.frame.height - drawn.height) > 0.5) {
            resized = true
        }
        if node.superview !== view.contentView {
            view.contentView.addSubview(node)
            host.presenter.requestTextPublication()
        }
        node.frame = drawn.offsetBy(dx: shift(of: view), dy: 0)
        // One heading, unless it holds a control, which stays its own element.
        let name = node.authoredLabel ?? section.header
        view.speaks = name != nil && !section.headerParts
        view.accessibilityLabel = view.speaks ? name : nil
        view.accessibilityIdentifier = node.props["testId"]
        view.accessibilityElementsHidden = node.props["accessibilityElementsHidden"] == "true" || place.inert
    }

    /// How far the list's content box starts from `view`'s content: UIKit
    /// insets an inset-grouped list's header as it does its card, and the
    /// kernel lays a header out across the section.
    private func shift(of view: UICollectionViewListCell) -> CGFloat {
        reach.left + horizontalPadding.left - view.frame.minX - view.contentView.frame.minX
    }

    /// Where a carried view is drawn when layout places it at `place`: a
    /// row in its cell, a header in UIKit's header.
    func seat(_ node: NodeView, at place: GridPlace) -> CGRect {
        let frame = Self.cellFrame(place, scale: host.presenter.gridScale)
        guard let header = node.superview?.superview as? GroupedHeaderView else { return frame }
        return frame.offsetBy(dx: shift(of: header), dy: 0)
    }

    /// Every visible custom header carried again, after a batch.
    func carryHeaders() {
        let kind = UICollectionView.elementKindSectionHeader
        for path in collection.indexPathsForVisibleSupplementaryElements(ofKind: kind) {
            guard let section = section(at: path.section), section.headerView != nil,
                  let view = collection.supplementaryView(forElementKind: kind, at: path) as? GroupedHeaderView else { continue }
            carry(header: section, into: view)
        }
        let live = Set(model.sections.compactMap(\.headerView))
        headerHeights = headerHeights.filter { live.contains($0.key) }
    }

    func collectionView(_ view: UICollectionView, willDisplaySupplementaryView supplementary: UICollectionReusableView,
                        forElementKind kind: String, at path: IndexPath) {
        guard let header = supplementary as? GroupedHeaderView, let section = section(at: path.section) else { return }
        carry(header: section, into: header)
        if resized { host.presenter.requestProjectionSync() }
    }

    func collectionView(_ view: UICollectionView, didEndDisplayingSupplementaryView supplementary: UICollectionReusableView,
                        forElementOfKind kind: String, at path: IndexPath) {
        guard let header = supplementary as? GroupedHeaderView else { return }
        for case let node as NodeView in header.contentView.subviews where carried[node.id] != nil { sendHome(node) }
    }
}
#endif
