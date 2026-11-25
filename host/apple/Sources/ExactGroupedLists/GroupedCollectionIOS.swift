import ExactKit
#if os(iOS) || os(tvOS)
import UIKit

/// A grouped list's collection view, by type, for the agent's wheel.
final class GroupedCollectionView: UICollectionView, GroupedScroller, BandConsumer, SafeIndicators {
    #if os(iOS)
    weak var reorderList: GroupedListView?
    private var contact: GroupedReorderContact?
    private var interactive = false

    func offerReorder(_ offered: Bool) {
        if offered && contact == nil {
            let contact = GroupedReorderContact()
            contact.cancelsTouchesInView = false
            contact.delaysTouchesEnded = false
            addGestureRecognizer(contact)
            self.contact = contact
        } else if !offered, let contact {
            removeGestureRecognizer(contact)
            self.contact = nil
        }
    }
    override func beginInteractiveMovementForItem(at path: IndexPath) -> Bool {
        guard reorderList?.lift(at: path) == true else { return false }
        interactive = super.beginInteractiveMovementForItem(at: path)
        if !interactive { reorderList?.movement?.cancel() }
        return interactive
    }
    override func updateInteractiveMovementTargetPosition(_ position: CGPoint) {
        if contact?.following == false { contact?.target(position) }
        if interactive { super.updateInteractiveMovementTargetPosition(position) }
    }
    override func cancelInteractiveMovement() {
        guard interactive, contact?.continueContact() != true else { return }
        abortMovement()
    }
    func abortMovement() {
        guard interactive else { return }
        interactive = false
        super.cancelInteractiveMovement()
        reorderList?.movement?.cancel()
    }
    override func endInteractiveMovement() {
        guard interactive else { return }
        interactive = false
        super.endInteractiveMovement()
        if let list = reorderList, !list.proposedMove { list.drop(in: list.source.snapshot()) }
    }
    #endif
    /// The offset, which UIKit itself also sets (keeping the list at its
    /// start as a safe area reaches it and insets it): inside an animation
    /// UIKit has open it moves only if the last frame showed the list's node,
    /// as a `ScrollView`'s does (`Presenter.moveOrPlace`).
    override var bounds: CGRect {
        get { super.bounds }
        set {
            guard UIView.inheritedAnimationDuration > 0, let node = owner, let presenter = node.presenter else { return super.bounds = newValue }
            presenter.moveOrPlace(node) { super.bounds = newValue }
        }
    }
    /// The list's node, which the collection is the scroll view of.
    private var owner: NodeView? { superview as? NodeView }
    /// Its indicators take its safe area where UIKit's adjustment gives none, `sideSafeArea` included, as a plain
    /// scroller's do (`SafeIndicatorTerm`).
    private var safeIndicators = SafeIndicatorTerm()
    var writtenWhole = false { didSet { safeIndicators.sync(self) } }
    override var automaticallyAdjustsScrollIndicatorInsets: Bool { didSet { safeIndicators.sync(self) } }
    override func safeAreaInsetsDidChange() {
        super.safeAreaInsetsDidChange()
        safeIndicators.sync(self)
    }
    /// An inset UIKit moves outside a layout of the list reaches the arrangement, as a plain scroller's does
    /// (`ScrollView.adjustedContentInsetDidChange`).
    override func adjustedContentInsetDidChange() {
        super.adjustedContentInsetDidChange()
        owner?.scrollInsetMoved(self)
    }
    /// The safe area at the sides the collection reaches over: the kernel's
    /// side reach once it has given one (LLP 1123 D13), else the list's own
    /// (`sideReach`), the window's or a fold's half band. UIKit insets the
    /// cards from it or its own safe area, whichever is deeper, as in a
    /// hand-built list.
    var sideSafeArea: (left: CGFloat, right: CGFloat) = (0, 0) {
        didSet {
            guard sideSafeArea != oldValue else { return }
            safeAreaInsetsDidChange()
            collectionViewLayout.invalidateLayout()
        }
    }
    /// The depth the cards are inset from on each side, the side safe area the layout uses (`safeAreaInsets`: UIKit's
    /// or `sideSafeArea`, whichever is deeper), top right bottom left, which D14's writer leaves to the layout.
    package var consumedBand: [CGFloat] { [0, safeAreaInsets.right, 0, safeAreaInsets.left] }
    override var safeAreaInsets: UIEdgeInsets {
        var insets = super.safeAreaInsets
        insets.left = max(insets.left, sideSafeArea.left)
        insets.right = max(insets.right, sideSafeArea.right)
        return insets
    }

    /// UIKit changed the margins it gives the list's cells, or showed the
    /// list: with no batch, as when a tab first shows a list built behind
    /// it, so the kernel is told them again (`GroupedListHost.tell`).
    var marginsChanged: () -> Void = {}
    override func layoutMarginsDidChange() {
        super.layoutMarginsDidChange()
        marginsChanged()
    }
    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window != nil { marginsChanged() }
    }

    weak var listNode: NodeView?
    var scrollingNode: NodeView? { listNode?.scrollView === self ? listNode : nil }

    /// VoiceOver's scroll is the reader's, as a drag is, and supersedes a position Exact carries (`dropCarriedScroll`)
    override func accessibilityScroll(_ direction: UIAccessibilityScrollDirection) -> Bool {
        scrollingNode?.dropCarriedScroll()
        return super.accessibilityScroll(direction)
    }

    var card: (Int) -> NodeView? = { _ in nil }
    /// Each section's outer-shadow reach as the layout last decorated it.
    private var decorated: [Int: CGFloat] = [:]
    /// The sections' inset regions as the layout last took them (`insetsReferences`).
    private var referred: [UIContentInsetsReference?] = [nil, nil, nil]
    private var insets: [(clip: CALayer, caster: InsetShadowCaster)] = []

    override func layoutSubviews() {
        super.layoutSubviews()
        paintCardShadows()
    }

    /// How far a section's card's outer shadows reach; nil when it casts none.
    private func outerReach(_ section: Int) -> CGFloat? {
        card(section)?.boxShadows.filter { !$0.inset }.map(\.reach).max()
    }

    /// Gives a shadowed card's section a background decoration that casts its
    /// outer shadows. UIKit orders a collection's views by their items'
    /// `zIndex`, so it lies above the list's own section ground and below the
    /// cells and supplementary views of every section, at whatever index
    /// reuse leaves each view.
    func decorate(_ layout: NSCollectionLayoutSection, at section: Int) {
        referred = insetsReferences
        referInsets(of: layout, referred)
        decorated[section] = outerReach(section)
        guard let reach = decorated[section] else { return }
        let item = NSCollectionLayoutDecorationItem.background(elementKind: CardShadowView.kind)
        item.zIndex = (layout.decorationItems.map(\.zIndex).max() ?? 0) + 1
        // The section's frame grown by the reach: UIKit keeps the view while
        // any of the shadow is in view, as when the card is just off screen.
        item.contentInsets = NSDirectionalEdgeInsets(top: -reach, leading: -reach, bottom: -reach, trailing: -reach)
        layout.decorationItems.append(item)
    }

    /// After a commit: lays the list out again where a card's outer shadow
    /// came, went or changed its reach, or the regions the list's insets
    /// are measured from changed, then casts every card's shadows.
    func castCardShadows() {
        let references = insetsReferences
        if referLayoutInsets(references[0]) || references != referred || (0..<numberOfSections).contains(where: { outerReach($0) != decorated[$0] }) {
            collectionViewLayout.invalidateLayout()
            layoutIfNeeded()
        }
        paintCardShadows()
    }

    /// A section's card, its shadows and its box: the frames its first and
    /// last rows span, including rows outside the viewport.
    private func shadowedCard(_ section: Int) -> (node: NodeView, shadows: [BoxShadowSpec], box: CGRect)? {
        guard section < numberOfSections else { return nil }
        let count = numberOfItems(inSection: section)
        guard count > 0, let node = card(section) else { return nil }
        let shadows = node.boxShadows
        guard !shadows.isEmpty,
              let first = layoutAttributesForItem(at: IndexPath(item: 0, section: section)),
              let last = layoutAttributesForItem(at: IndexPath(item: count - 1, section: section)) else { return nil }
        return (node, shadows, first.frame.union(last.frame))
    }

    /// UIKit paints the card as cell backgrounds: its outer shadows are its
    /// section's decoration, its inset shadows a slice in each visible cell.
    private func paintCardShadows() {
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        for case let view as CardShadowView in subviews {
            let shadowed = shadowedCard(view.section)
            let outer = shadowed?.shadows.filter { !$0.inset } ?? []
            view.caster.isHidden = outer.isEmpty
            guard let node = shadowed?.node, let whole = shadowed?.box, !outer.isEmpty else { continue }
            let box = convert(whole, to: view)
            view.caster.cast(box: box, outline: node.boxShadowShape(in: box), shadows: outer.map {
                (node.boxShadowShape(in: box, spread: $0.spread), $0)
            })
        }
        var used = 0
        for section in 0..<numberOfSections {
            guard let shadowed = shadowedCard(section) else { continue }
            let (node, shadows, box) = shadowed
            let inner = shadows.filter(\.inset)
            guard !inner.isEmpty else { continue }
            // Each visible cell shows its slice above its background and
            // below its content; the caster still uses the whole card.
            for path in indexPathsForVisibleItems where path.section == section {
                guard let cell = cellForItem(at: path) else { continue }
                if used == insets.count {
                    let clip = CALayer(), caster = InsetShadowCaster()
                    clip.masksToBounds = true; clip.addSublayer(caster)
                    insets.append((clip, caster))
                }
                let (clip, caster) = insets[used]; used += 1
                cell.contentView.layer.insertSublayer(clip, at: 0)
                clip.frame = cell.contentView.bounds
                let local = cell.contentView.convert(box, from: self)
                caster.cast(box: local, padding: node.boxShadowShape(in: local), shadows: inner.map {
                    (node.boxShadowShape(in: local, spread: $0.spread, inset: true), $0)
                })
            }
        }
        for inset in insets[used...] { inset.clip.removeFromSuperlayer() }
        insets.removeSubrange(used...)
    }
}

/// A card's outer shadows, as its section's background decoration
/// (`GroupedCollectionView.decorate`).
final class CardShadowView: UICollectionReusableView {
    static let kind = "exact-card-shadow"
    let caster = ShadowCaster()
    private(set) var section = 0

    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        layer.addSublayer(caster)
    }
    required init?(coder: NSCoder) { nil }

    override func apply(_ attributes: UICollectionViewLayoutAttributes) {
        super.apply(attributes)
        section = attributes.indexPath.section
    }
}

#endif
