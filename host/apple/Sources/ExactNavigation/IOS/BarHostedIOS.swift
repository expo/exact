// @ref LLP 1141.004 D3 — a hosted item: an element of a header that the bar
// has no object of its own for, shown in `UIBarButtonItem(customView:)` by
// the element's own views, lifted out of the header while the bar shows it.
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
final class HostedStand: UIView, HoldsLiftedNodes {
    /// The element's parent in the authored tree.
    weak var parent: NodeView?
}

/// A hosted item's custom view: the element at its laid-out size, inside
/// UIKit's title-item inset where the item shares the bar's glass and shows
/// words.
final class HostedItemView: UIView {
    let stand = HostedStand()
    let node: NodeView
    var inset: CGFloat = 0 {
        didSet { if inset != oldValue { resize() } }
    }
    /// The element's laid-out size as last shown: its frame's far edges in
    /// the block. The bridge sends no margins, so a trailing or bottom
    /// margin is not kept (LLP 1141.004 D3).
    private(set) var size = CGSize.zero
    private var relay: MountRelay?

    init(node: NodeView, parent: NodeView) {
        self.node = node
        super.init(frame: .zero)
        stand.parent = parent
        addSubview(stand)
        if let presenter = node.presenter { relay = MountRelay(presenter, mount: self) }
    }
    required init?(coder: NSCoder) { nil }

    /// Follow the element's laid-out size, and its logical ancestors' `aria-hidden`
    /// and `inert`, which UIKit does not see past the lift; whether the size changed.
    func refit() -> Bool {
        let hidden = node.inert || node.authoredLine.contains { $0.props["accessibilityElementsHidden"] == "true" }
        if accessibilityElementsHidden != hidden { accessibilityElementsHidden = hidden }
        let laid = CGSize(width: max(0, node.frame.maxX), height: max(0, node.frame.maxY))
        guard laid != size else { return false }
        size = laid
        resize()
        return true
    }

    /// UIKit sizes a bar item's custom view by its frame where it has no
    /// constraints, and reads `intrinsicContentSize` where it does: both.
    private func resize() {
        let width = size.width + 2 * inset
        if frame.size != CGSize(width: width, height: size.height) { frame.size = CGSize(width: width, height: size.height) }
        invalidateIntrinsicContentSize()
        setNeedsLayout()
    }

    override var intrinsicContentSize: CGSize { CGSize(width: size.width + 2 * inset, height: size.height) }

    override func layoutSubviews() {
        super.layoutSubviews()
        let frame = CGRect(x: inset, y: (bounds.height - size.height) / 2, width: size.width, height: size.height)
        if stand.frame != frame { stand.frame = frame }
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
        weak var route: RouteController?
        var room: CGSize
        /// What the overflow menu's entry was made from (`name`).
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
        let entry = Hosted(item: UIBarButtonItem(customView: view), view: view, route: c, room: room(in: c, inset: i.hostedInset))
        hosted[i.id] = entry
        style(i)
        _ = view.refit()
        presenter.owners.publish()
        return entry.item
    }

    private func style(_ i: HeaderShape.Item) {
        guard let entry = hosted[i.id] else { return }
        entry.item.accessibilityIdentifier = i.testId
        entry.view.inset = i.hostedInset
        if #available(iOS 26.0, *) { entry.item.hidesSharedBackground = !i.hostedGlass }
        name(i.id)
    }

    /// What the overflow menu does for a hosted element: press it, or flip
    /// a switch in it.
    private struct Entry {
        let node: NodeView, name: String, on: Bool?, disabled: Bool, invokes: String
        var source: String { "\(node.id)/\(name)/\(on.map(String.init) ?? "")/\(disabled)/\(invokes)" }
    }

    /// What the overflow menu does for the element: its own action, where it
    /// is a switch or presses (a `press` or a link's `href`); else nothing,
    /// and its entry is its name, dimmed. A command alone opens from the item
    /// in the bar, where the item has a window, so its entry is dimmed too.
    /// What is inside an element is reached in the bar, where it shows.
    private func entries(_ node: NodeView) -> [Entry] {
        let disabled = node.disabled || node.inert || node.cssVisibilityHidden, invocation = presenter.invocation(of: node)
        // Everything a press may do: a link `presenter.press` follows first, and a command.
        let invokes = "\(node.defaultLink ?? "") \(invocation.map { "\($0.target.id) \($0.action)" } ?? "")"
        if let s = presenter.controls.controls[node.id] as? UISwitch {
            return [Entry(node: node, name: node.accessibleName, on: s.isOn, disabled: disabled || !s.isEnabled, invokes: invokes)]
        }
        guard node.handlers.contains("press") || node.defaultLink != nil else { return [] }
        return [Entry(node: node, name: node.accessibleName, on: nil, disabled: disabled, invokes: invokes)]
    }

    /// In UIKit's overflow menu a custom view shows nothing: the item is
    /// there by its element's name (LLP 1141.004 D3), an entry that does
    /// what the element itself does, else a dimmed line. Made again as the
    /// element changes, and an entry chosen acts only while it is still
    /// what the bar that shows offers.
    private func name(_ id: UInt32) {
        guard let entry = hosted[id] else { return }
        let node = entry.view.node, name = node.accessibleName
        let entries = entries(node)
        let source = offered(node, entries)
        guard source != entry.menu else { return }
        hosted[id]?.menu = source
        let actions = entries.map { e in
            UIAction(title: e.name.isEmpty ? name : e.name, attributes: e.disabled ? .disabled : [], state: e.on == true ? .on : .off) { [weak self, weak presenter, weak root = node, weak node = e.node, disabled = e.disabled] _ in
                // Still the item of the bar that shows, its element shown, and
                // offering what was offered: an entry chosen from a menu made
                // before a change does nothing.
                guard let self, let presenter, let root, let node, presenter.views[node.id] === node,
                      let held = self.hosted[root.id], held.view.node === root, root.style["display"]?.string != "none",
                      let route = held.route, route.navigationController?.topViewController === route, route.viewIfLoaded?.window != nil,
                      !self.navigation.isInactiveRoute(containing: route.node, layout: false),
                      self.offered(root, self.entries(root)) == source, !disabled else { return }
                if let s = presenter.controls.controls[node.id] as? UISwitch {
                    // As a tap flips it: the switch's own change.
                    guard s.isEnabled else { return }
                    s.setOn(!s.isOn, animated: false)
                    s.sendActions(for: .valueChanged)
                } else {
                    presenter.press(node.id)
                }
            }
        }
        if let one = actions.first { entry.item.menuRepresentation = one }
        else { entry.item.menuRepresentation = name.isEmpty ? nil : UIAction(title: name, attributes: .disabled) { _ in } }
    }

    /// What the overflow menu offers for `root`: its name and its entries.
    private func offered(_ root: NodeView, _ entries: [Entry]) -> String {
        ([root.accessibleName] + entries.map(\.source)).joined(separator: "\u{1F}")
    }

    /// The room the bar gives a hosted item: a third of its width, so a
    /// leading and a trailing one leave a third for an inline title, and a
    /// bar item's height.
    private func room(in c: RouteController?, inset: CGFloat) -> CGSize {
        // A bar not laid out yet has no width: the viewport's stands in.
        let bar = c?.navigationController?.navigationBar.bounds.width ?? 0
        let width = bar > 0 ? bar : presenter.viewport.bounds.width
        return CGSize(width: max(0, width / 3 - 2 * inset), height: Self.height)
    }

    /// `c`'s bar shows the hosted items `ids`: each other one of its own goes
    /// back, and a kept one is given its block again when the bar's width
    /// changed.
    func show(_ ids: [UInt32], in c: RouteController) {
        for (id, entry) in hosted where (entry.route === c || entry.route == nil) && !ids.contains(id) { release(id) }
        var moved = false
        for id in ids {
            guard let entry = hosted[id] else { continue }
            let room = room(in: c, inset: entry.view.inset)
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
        if let parent, !hosted.values.contains(where: { $0.view.stand.parent === parent }) { homes[parent.id] = nil }
        presenter.owners.publish()
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
        for (id, entry) in hosted {
            name(id)
            if entry.view.refit() { entry.route?.navigationController?.navigationBar.setNeedsLayout() }
        }
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
