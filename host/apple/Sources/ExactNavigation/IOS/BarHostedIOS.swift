// @ref LLP 1141.004 D3 — a hosted item: a header's readout (`role="status"`),
// the one element the bar has no object of its own for, shown in
// `UIBarButtonItem(customView:)` by its own views, lifted out of the header
// while the bar shows it.
//
// The element is given a block that fits its content inside the room the bar
// gives an item (`barItemBlocks`, told with the other blocks by `OwnersIOS`),
// and its laid-out size is the custom view's. The header is covered by a bar
// (`HostCover.bar`), which takes it out of the page and lays out only the
// blocks inside it.
//
// The element keeps its place in the authored tree: its parent, for the walks
// that follow that tree (`LiftedNodes.parent(of:)`: events, `aria-hidden`,
// focus), and its place among that parent's children, which the header rule
// reads (`recordedIDs(of:)`) so the next projection finds it where it was.
#if os(iOS)
import UIKit
import ExactKit

/// What holds a hosted element's view, at the kernel's frames: the block's
/// own space.
final class HostedStand: UIView, HoldsBarReadout {
    /// The element's parent in the authored tree.
    weak var parent: NodeView?
}

/// A readout's custom view: the element at its laid-out size.
final class HostedItemView: UIView {
    let stand = HostedStand()
    let node: NodeView
    /// The element's laid-out size as last shown: its frame's far edges in
    /// the block. The bridge sends no margins, so a trailing or bottom
    /// margin is not kept (LLP 1141.004 D3).
    private(set) var size = CGSize.zero
    private var relay: MountRelay?
    private var readout: UIAccessibilityElement?

    init(node: NodeView, parent: NodeView) {
        self.node = node
        super.init(frame: .zero)
        stand.parent = parent
        addSubview(stand)
        if let presenter = node.presenter { relay = MountRelay(presenter, mount: self) }
        // A bar item stays bar-sized at the accessibility text sizes, where a long press shows it
        // larger (LLP 1141.004 D3); UIKit shows the viewer only at those sizes.
        addInteraction(UILargeContentViewerInteraction())
    }
    required init?(coder: NSCoder) { nil }

    /// Follow the element's laid-out size and accessibility. UIKit does not
    /// see `aria-hidden` and `inert` on logical ancestors past the lift.
    /// Returns whether the size changed.
    func refit() -> Bool {
        let hidden = node.inert || node.authoredLine.contains { $0.props["accessibilityElementsHidden"] == "true" }
        if accessibilityElementsHidden != hidden { accessibilityElementsHidden = hidden }
        updateReadout(hidden: hidden)
        let laid = CGSize(width: max(0, node.frame.maxX), height: max(0, node.frame.maxY))
        guard laid != size else { return false }
        size = laid
        resize()
        return true
    }

    private func updateReadout(hidden: Bool) {
        guard !hidden, !node.cssVisibilityHidden, node.props["accessibilityRole"] == "status" else {
            readout = nil
            largeContent(nil)
            return
        }
        let authoredLabel = node.authoredLabel
        let label = authoredLabel ?? node.exposedContentName
        let trimmed = label.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            readout = nil
            largeContent(nil)
            return
        }
        let element = readout ?? UIAccessibilityElement(accessibilityContainer: self)
        if element.accessibilityLabel != label { element.accessibilityLabel = label }
        let text = authoredLabel == nil ? "" : node.exposedText.trimmingCharacters(in: .whitespacesAndNewlines)
        let value = text.isEmpty || text == trimmed ? nil : text
        if element.accessibilityValue != value { element.accessibilityValue = value }
        // The viewer shows what VoiceOver says: the name, then the words where they differ.
        largeContent(value.map { "\(label), \($0)" } ?? label)
        if readout == nil {
            element.accessibilityTraits = .staticText
            element.accessibilityFrameInContainerSpace = bounds
            readout = element
        }
    }

    /// The Large Content Viewer's title, or nil for none.
    private func largeContent(_ title: String?) {
        if showsLargeContentViewer != (title != nil) { showsLargeContentViewer = title != nil }
        if largeContentTitle != title { largeContentTitle = title }
    }

    override var accessibilityElements: [Any]? {
        get {
            guard node.props["accessibilityRole"] == "status" else { return super.accessibilityElements }
            let stops: [NSObject] = readout.map { [$0] } ?? []
            return stops + Self.readoutControls(in: stand)
        }
        set { super.accessibilityElements = newValue }
    }

    /// The readout speaks for its passive content. Its controls keep their
    /// own stops, in the order their UIKit containers expose them.
    private static func readoutControls(in object: NSObject) -> [NSObject] {
        let view = object as? UIView
        if let view, view.isHidden || view.alpha <= 0.01 || view.accessibilityElementsHidden { return [] }
        if object.isAccessibilityElement {
            let traits: UIAccessibilityTraits = [.button, .link, .adjustable, .toggleButton, .searchField]
            let control = object is UIControl || object.accessibilityRespondsToUserInteraction
                || !object.accessibilityTraits.intersection(traits).isEmpty
            return control ? [object] : []
        }
        let children: [Any]
        if let declared = object.accessibilityElements {
            children = declared
        } else {
            let count = object.accessibilityElementCount()
            if count > 0, count != NSNotFound {
                children = (0..<count).compactMap { object.accessibilityElement(at: $0) }
            } else {
                children = NodeView.accessibilityOrder(view?.subviews ?? [])
            }
        }
        return children.compactMap { $0 as? NSObject }.flatMap { readoutControls(in: $0) }
    }

    /// UIKit sizes a bar item's custom view by its frame where it has no
    /// constraints, and reads `intrinsicContentSize` where it does: both.
    private func resize() {
        if frame.size != size { frame.size = size }
        invalidateIntrinsicContentSize()
        setNeedsLayout()
    }

    override var intrinsicContentSize: CGSize { size }

    override func layoutSubviews() {
        super.layoutSubviews()
        let frame = CGRect(x: 0, y: (bounds.height - size.height) / 2, width: size.width, height: size.height)
        if stand.frame != frame { stand.frame = frame }
        readout?.accessibilityFrameInContainerSpace = bounds
    }
}

/// The hosted items of every route's bar, by element.
final class BarHosted: LiftedNodes {
    unowned let navigation: NavigationHost
    var presenter: Presenter { navigation.presenter }
    init(navigation: NavigationHost) { self.navigation = navigation }

    private struct Hosted {
        let item: UIBarButtonItem
        let view: HostedItemView
        let parentID: UInt32
        weak var route: RouteController?
        var room: CGSize
        /// The name the overflow menu's entry was made from (`name`).
        var menu = ""
    }
    private var hosted: [UInt32: Hosted] = [:]
    /// Each parent's children as the tree last gave them, flat leaves
    /// included, while a child of it is lifted.
    private var homes: [UInt32: [UInt32]] = [:]

    /// The tallest a hosted item is: a bar item's height in the iOS 26 bar.
    static let height: CGFloat = 44

    /// The item for `i` on `c`'s bar, made once and kept, its element lifted
    /// into it.
    func item(_ i: HeaderShape.Item, in c: RouteController) -> UIBarButtonItem {
        let node = presenter.views[i.id]
        if let entry = hosted[i.id], entry.view.node === node {
            hosted[i.id]?.route = c
            style(i)
            return entry.item
        }
        release(i.id)
        guard let node, let parent = presenter.logicalParent(of: node.id).flatMap({ presenter.views[$0] }) else { return UIBarButtonItem() }
        if homes[parent.id] == nil { homes[parent.id] = presenter.flats.children(of: parent.id) ?? NodeView.committed(parent.container.subviews).map(\.id) }
        let view = HostedItemView(node: node, parent: parent)
        NodeView.containerMoves += 1
        view.stand.addSubview(node)
        NodeView.containerMoves -= 1
        presenter.flats.containerChanged(parent.id)
        let entry = Hosted(item: UIBarButtonItem(customView: view), view: view, parentID: parent.id, route: c, room: room(in: c))
        hosted[i.id] = entry
        style(i)
        _ = view.refit()
        presenter.owners.publish()
        return entry.item
    }

    private func style(_ i: HeaderShape.Item) {
        guard let entry = hosted[i.id] else { return }
        entry.item.accessibilityIdentifier = i.testId
        // A readout presses nothing: it is content on the bar, with no glass
        // (D4); a material it writes is its own.
        if #available(iOS 26.0, *) { entry.item.hidesSharedBackground = true }
        name(i.id)
    }

    /// In UIKit's overflow menu a custom view shows nothing: a readout is
    /// there by its name, dimmed (D3), made again as the name changes.
    private func name(_ id: UInt32) {
        guard let entry = hosted[id] else { return }
        let name = entry.view.node.accessibleName
        guard name != entry.menu else { return }
        hosted[id]?.menu = name
        entry.item.menuRepresentation = name.isEmpty ? nil : UIAction(title: name, attributes: .disabled) { _ in }
    }

    /// The room the bar gives a readout: a third of its width, so a leading
    /// and a trailing one leave a third for an inline title, and a bar
    /// item's height.
    private func room(in c: RouteController?) -> CGSize {
        // A bar not laid out yet has no width: the viewport's stands in.
        let bar = c?.navigationController?.navigationBar.bounds.width ?? 0
        let width = bar > 0 ? bar : presenter.viewport.bounds.width
        return CGSize(width: max(0, width / 3), height: Self.height)
    }

    /// `c`'s bar shows the hosted items `ids`: each other one of its own goes
    /// back, and a kept one is given its block again when the bar's width
    /// changed.
    func show(_ ids: [UInt32], in c: RouteController) {
        for (id, entry) in hosted where (entry.route === c || entry.route == nil) && !ids.contains(id) { release(id) }
        var moved = false
        for id in ids {
            guard let entry = hosted[id] else { continue }
            let room = room(in: c)
            if room != entry.room { hosted[id]?.room = room; moved = true }
            if entry.view.refit() { c.navigationController?.navigationBar.setNeedsLayout() }
        }
        if moved { presenter.owners.publish() }
    }

    /// The blocks the hosted elements are laid out in: their content's size,
    /// at most the room the bar gives.
    var blocks: [UInt32: HostBlock] {
        hosted.mapValues { HostBlock(width: .fit($0.room.width), height: .fit($0.room.height)) }
    }

    /// The element of `id` back in its parent, at its place, and its item gone.
    private func release(_ id: UInt32) {
        guard let entry = hosted.removeValue(forKey: id) else { return }
        let node = entry.view.node, parent = entry.view.stand.parent
        if node.superview === entry.view.stand {
            if let parent, presenter.views[parent.id] === parent, presenter.views[node.id] === node {
                putBack(node, into: parent)
            } else {
                node.removeFromSuperview()
            }
        }
        if !hosted.values.contains(where: { $0.parentID == entry.parentID }) { homes[entry.parentID] = nil }
        presenter.owners.publish()
    }

    func reset() {
        for id in hosted.keys { release(id) }
        homes.removeAll()
    }

    private func putBack(_ node: NodeView, into parent: NodeView) {
        NodeView.containerMoves += 1
        defer { NodeView.containerMoves -= 1 }
        let content = parent.container
        let ids = homes[parent.id] ?? []
        let there = Dictionary(NodeView.committed(content.subviews).map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let next = ids.firstIndex(of: node.id).flatMap { i in ids[(i + 1)...].lazy.compactMap { there[$0] }.first }
        if let next { content.insertSubview(node, belowSubview: next) } else { content.addSubview(node) }
        presenter.flats.containerChanged(parent.id)
    }

    // MARK: LiftedNodes

    func parent(of view: UIView) -> NodeView? { (view.superview as? HostedStand)?.parent }
    func pressBox(crossing view: UIView, from start: UIView) -> UIView? { parent(of: view) == nil ? nil : view }

    func holds(_ node: NodeView, under parent: NodeView) -> Bool {
        guard let entry = hosted[node.id] else { return false }
        return entry.view.node === node && node.superview === entry.view.stand && entry.view.stand.parent === parent
    }

    /// A children op on `parent`: its list is kept while a child of it is
    /// lifted, and a lifted element the op drops goes back first, so the op
    /// removes it by its own rules.
    func children(_ parent: NodeView, _ ids: [UInt32], _ want: [NodeView]) {
        guard homes[parent.id] != nil else { return }
        homes[parent.id] = ids
        for (id, entry) in hosted where entry.view.stand.parent === parent {
            let node = entry.view.node
            if want.contains(where: { $0 === node }) || presenter.isLeaving(node) { continue }
            release(id)
        }
    }

    func lifted(from parent: NodeView) -> [NodeView] {
        hosted.values.compactMap { $0.view.stand.parent === parent ? $0.view.node : nil }
    }

    func recordedIDs(of parent: NodeView) -> [UInt32]? { homes[parent.id] }

    /// After a batch: each item follows its element's laid-out size.
    func place() {
        for entry in hosted.values {
            if entry.view.refit() { entry.route?.navigationController?.navigationBar.setNeedsLayout() }
        }
    }

    /// Name each readout's overflow entry again once the batch is applied.
    func refreshOverflow() {
        for id in hosted.keys { name(id) }
    }

    func focusChildren(of view: UIView) -> [UIView] { view.subviews }
    func accessibilityElements(of view: UIView) -> [Any]? { nil }
    func absoluteNodes(after view: UIView, in parent: UIView) -> [NodeView] { [] }
}

/// The navigation's lifted nodes: those beside the tab panels (LLP 1083.000
/// D4) and the hosted items of the bars.
final class LiftedPair: LiftedNodes {
    let first: any LiftedNodes, second: any LiftedNodes
    init(_ first: any LiftedNodes, _ second: any LiftedNodes) { (self.first, self.second) = (first, second) }
    func parent(of view: UIView) -> NodeView? { first.parent(of: view) ?? second.parent(of: view) }
    func pressBox(crossing view: UIView, from start: UIView) -> UIView? {
        first.pressBox(crossing: view, from: start) ?? second.pressBox(crossing: view, from: start)
    }
    func holds(_ node: NodeView, under parent: NodeView) -> Bool { first.holds(node, under: parent) || second.holds(node, under: parent) }
    func children(_ parent: NodeView, _ ids: [UInt32], _ want: [NodeView]) {
        first.children(parent, ids, want)
        second.children(parent, ids, want)
    }
    func lifted(from parent: NodeView) -> [NodeView] { first.lifted(from: parent) + second.lifted(from: parent) }
    func recordedIDs(of parent: NodeView) -> [UInt32]? { first.recordedIDs(of: parent) ?? second.recordedIDs(of: parent) }
    func place() { first.place(); second.place() }
    func focusChildren(of view: UIView) -> [UIView] { first.focusChildren(of: view) }
    func accessibilityElements(of view: UIView) -> [Any]? { first.accessibilityElements(of: view) ?? second.accessibilityElements(of: view) }
    func absoluteNodes(after view: UIView, in parent: UIView) -> [NodeView] { first.absoluteNodes(after: view, in: parent) + second.absoluteNodes(after: view, in: parent) }
}
#endif
