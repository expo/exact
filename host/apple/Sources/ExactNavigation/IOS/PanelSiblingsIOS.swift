// @ref LLP 1083.000 D4 (amended 2026-11-18) — a node beside the tab panels, nested below the
// navigation root, paints over the native tab container from a layer just after it.
//
// The container stands for the panels among the root's own children, so a node nested beside
// them, in a child of the root that holds them, is inside a view under the container. UIKit
// paints a view over another only as a later sibling or inside one, so such a node is lifted
// out of its parent into `PanelSiblingsLayer`, the container's sibling, into a stand that
// takes the clip the parent's children are under, or else its bounds, and its place on the
// page: the kernel's frames, in the parent's coordinates, apply there unchanged. The walks
// that follow the authored tree go from a lifted node to its parent (`LiftedNodes.parent`),
// and Tab and VoiceOver read it on its side of the routes.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// The layer just after the tab container that holds the nodes lifted over it. It takes no
/// touch itself, and tests no bounds: a lifted node's overflow takes touches, as a node's does.
final class PanelSiblingsLayer: UIView, HoldsLiftedNodes {
    weak var siblings: PanelSiblings?

    // Its nodes that read after the routes, by their own frames rather than their stands' (LLP 1083.000 D4: paint and
    // reading order stay apart)
    override var accessibilityElements: [Any]? {
        get { super.accessibilityElements ?? siblings?.readAfter() }
        set { super.accessibilityElements = newValue }
    }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        guard !isHidden, isUserInteractionEnabled else { return nil }
        for child in NodeView.hitOrder(subviews) {
            if let hit = child.hitTest(convert(point, to: child), with: event) { return hit }
        }
        return nil
    }
}

/// One parent's lifted nodes on one side of the panels in its paint order, in the parent's
/// content coordinates: the stand takes the clip the parent's children are under, or else the
/// parent's bounds, and from the parent's place on the page its position, 2-D transform, opacity
/// and visibility.
final class PanelSiblingsStand: UIView, HoldsLiftedNodes {
    weak var parent: NodeView?
    let after: Bool
    /// The tint and its adjustment the stand was given, which UIKit reads back dimmed behind an alert
    private var carriedAccent: UIColor?, carriedAdjustment = UIView.TintAdjustmentMode.automatic
    /// Its place in the layer: its parent's entry in a depth-first walk of the tree for the
    /// nodes before the panels, its parent's exit for those after them.
    var key: Int
    init(parent: NodeView, after: Bool, key: Int) {
        self.parent = parent; self.after = after; self.key = key
        super.init(frame: .zero)
    }
    required init?(coder: NSCoder) { nil }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        // Not for alpha: CSS opacity does not change hit testing (`NodeView.hitTest`)
        guard !isHidden, isUserInteractionEnabled, !clipsToBounds || bounds.contains(point) else { return nil }
        for child in NodeView.hitOrder(subviews) {
            if let hit = child.hitTest(convert(point, to: child), with: event) { return hit }
        }
        return nil
    }

    /// In the layer, at the parent's place: the exact 2-D affine map of every view between the
    /// parent's content view and the layer, from where the content's origin and a step along
    /// each axis land.
    func place(in host: UIView, mount: UIView) {
        guard let parent else { return }
        // The clip the parent's children are under, the nearest view from their content view up to the node that
        // clips (the content view, a clip box, the node); with none, the parent's bounds
        let content = parent.container
        let clipper = sequence(first: content as UIView, next: { $0 === parent ? nil : $0.superview }).first { $0.clipsToBounds }
        var b = clipper.map { $0.convert($0.bounds, to: content) } ?? content.bounds
        // A one-axis clip (`clip` beside `visible`, CSS Overflow 3 §3.1) holds the clipped axis only, so on the other
        // the stand spans the layer, which covers the mount (and its content where the mount scrolls), rather than
        // measuring what its nodes paint, which a hover or a press can outgrow between placements; a map with no inverse
        // (a zero scale) clips to the box
        if let clipper, clipper === parent.clipBox {
            let span = host.convert(host.bounds, to: content), finite = [span.minX, span.minY, span.width, span.height].allSatisfy(\.isFinite)
            let overflowX = parent.style["overflow_x"]?.string ?? "visible", overflowY = parent.style["overflow_y"]?.string ?? "visible"
            if finite, overflowX == "clip", overflowY == "visible" { b = CGRect(x: b.minX, y: span.minY, width: b.width, height: span.height) }
            if finite, overflowY == "clip", overflowX == "visible" { b = CGRect(x: span.minX, y: b.minY, width: span.width, height: b.height) }
        }
        let o = content.convert(b.origin, to: host)
        let x = content.convert(CGPoint(x: b.minX + 1, y: b.minY), to: host)
        let y = content.convert(CGPoint(x: b.minX, y: b.minY + 1), to: host)
        let map = CGAffineTransform(a: x.x - o.x, b: x.y - o.y, c: y.x - o.x, d: y.y - o.y, tx: 0, ty: 0)
        if bounds != b { bounds = b }
        if transform != map { transform = map }
        let center = content.convert(CGPoint(x: b.midX, y: b.midY), to: host)
        if self.center != center { self.center = center }
        var alpha: CGFloat = 1, hidden = false, unread = false, inert = false
        // The nearest `color-scheme` the chain sets, which UIKit carries down a view's own ancestors
        var scheme = UIUserInterfaceStyle.unspecified
        var at: UIView? = content
        while let view = at, view !== mount {
            alpha *= view.alpha
            hidden = hidden || view.isHidden
            unread = unread || view.accessibilityElementsHidden
            inert = inert || !view.isUserInteractionEnabled
            if scheme == .unspecified { scheme = view.overrideUserInterfaceStyle }
            at = view.superview
        }
        if overrideUserInterfaceStyle != scheme { overrideUserInterfaceStyle = scheme }
        // The parent's `accent-color` and `-exact-tint-adjustment`, as authored: UIKit's `tintColor` and
        // `tintAdjustmentMode` read dimmed while an alert dims the views behind it, which a copy would keep after it
        let tint = parent.ownAccent, adjustment = parent.authoredTintAdjustment
        if carriedAccent !== tint { carriedAccent = tint; tintColor = tint }
        if carriedAdjustment != adjustment { carriedAdjustment = adjustment; tintAdjustmentMode = adjustment }
        if self.alpha != alpha { self.alpha = alpha }
        if isHidden != hidden { isHidden = hidden }
        if accessibilityElementsHidden != unread { accessibilityElementsHidden = unread }
        if isUserInteractionEnabled == inert { isUserInteractionEnabled = !inert }
        if clipsToBounds != (clipper != nil) { clipsToBounds = clipper != nil }
        let corners = (clipper ?? content).layer
        if layer.cornerRadius != corners.cornerRadius { layer.cornerRadius = corners.cornerRadius }
        if layer.maskedCorners != corners.maskedCorners { layer.maskedCorners = corners.maskedCorners }
        if layer.cornerCurve != corners.cornerCurve { layer.cornerCurve = corners.cornerCurve }
    }
}

/// The nodes a navigation root lifts over its tab container, the stands that hold them, and
/// the child lists of the nodes that hold panels.
final class PanelSiblings: LiftedNodes {
    unowned let navigation: NavigationHost
    var presenter: Presenter { navigation.presenter }
    init(navigation: NavigationHost) { self.navigation = navigation }

    /// One node to lift: its parent; the parent's entry and exit in a depth-first walk of the
    /// tree, which nest the stands as the tree nests the parents; the side of the holders of
    /// panels it paints on (its stand), and the side it reads on (tree order).
    struct Lift { let node: NodeView, parent: NodeView, enter: Int, exit: Int, after: Bool, readsAfter: Bool }
    private struct Lifted { weak var node: NodeView?; weak var parent: NodeView?; var enter: Int; var exit: Int; var readsAfter: Bool }
    private struct Key: Hashable { let parent: UInt32, after: Bool }

    private(set) var layer: PanelSiblingsLayer?
    #if os(iOS)
    /// A parent's long press, double tap and pointer listeners hear a touch on a lifted node.
    private var relay: MountRelay?
    #endif
    private var stands: [Key: PanelSiblingsStand] = [:]
    private var lifted: [UInt32: Lifted] = [:]
    /// The ids each node that holds panels, and each panel, was last given (flat leaves
    /// included): a lifted node's place among its siblings, which its parent's view no longer shows.
    private var records: [UInt32: [UInt32]] = [:]

    var isEmpty: Bool { lifted.isEmpty }

    /// The parent a view in a stand is under in the authored tree.
    func parent(of view: UIView) -> NodeView? { (view.superview as? PanelSiblingsStand)?.parent }

    /// Whether a stand holds `node`.
    func holds(_ node: NodeView) -> Bool { node.superview is PanelSiblingsStand && lifted[node.id]?.node === node }

    /// Whether `parent` lifted `node`, which `parent`'s children op then leaves where it is;
    /// another parent's op that takes it moves it there.
    func holds(_ node: NodeView, under parent: NodeView) -> Bool { holds(node) && lifted[node.id]?.parent === parent }

    /// The nodes `parent` lifted.
    func lifted(from parent: NodeView) -> [NodeView] {
        lifted.values.compactMap { $0.parent === parent ? $0.node : nil }.filter(holds)
    }

    /// `parent`'s children as last placed, by id: its record, else a panel's list the navigation
    /// keeps, else its flat leaves' list, else its views.
    func placedIDs(of parent: NodeView) -> [UInt32] {
        records[parent.id] ?? navigation.logicalChildren[parent.id] ?? presenter.flats.children(of: parent.id) ?? Self.nodes(in: parent.container).map(\.id)
    }

    /// The nodes among `view`'s subviews, in their order.
    static func nodes(in view: UIView) -> [NodeView] { view.subviews.compactMap { $0 as? NodeView } }

    /// Whether `box` is beside the routes: no route's box meets it where CSS lays the routes out
    /// (`routes`), so the web shows it, and the routes' views (`drawn`), which UIKit may size to
    /// the page, do not cover it, or cover it only to their side, where the container's safe area
    /// clears them of it (`NavigationHost.sideReach`). A row above or below routes drawn over the
    /// whole page is not beside them: the native bars span the page there and keep their place
    /// over it.
    static func beside(_ box: CGRect, _ routes: [CGRect], drawn: [CGRect]) -> Bool {
        guard !routes.contains(where: { $0.intersects(box) }) else { return false }
        return !drawn.contains { $0.intersects(box) } || routes.contains { $0.minY < box.maxY && $0.maxY > box.minY }
    }

    func recordedIDs(of parent: NodeView) -> [UInt32]? { records[parent.id] }

    /// A children op on `parent`, before the presenter places it. A node that holds panels, or
    /// a panel, keeps the list. A lifted node the op drops goes back into the parent first, so
    /// the op removes it by its own rules, unless it is leaving: an exit ghost stays in its
    /// stand until its `destroy`.
    func children(_ parent: NodeView, _ ids: [UInt32], _ want: [NodeView]) {
        guard !navigation.tabPanels.isEmpty, parent !== navigation.container else { return }
        if records[parent.id] != nil || navigation.holdsPanel(parent) { records[parent.id] = ids }
        guard !lifted.isEmpty else { return }
        let wanted = Set(want.map(ObjectIdentifier.init))
        for (id, entry) in lifted where entry.parent === parent {
            guard let node = entry.node, holds(node) else { lifted[id] = nil; continue }
            if wanted.contains(ObjectIdentifier(node)) || presenter.isLeaving(node) { continue }
            putBack(node, into: parent)
            lifted[id] = nil
        }
    }

    /// The child lists a batch gives the nodes that hold panels and the panels, which the
    /// projection knows only after the batch's children ops ran.
    func record(_ batch: Batch, holders: Set<UInt32>) {
        for op in batch.ops where op.op == .children && holders.contains(op.id) { records[op.id] = op.ids }
        records = records.filter { record in holders.contains(record.key) || lifted.values.contains { $0.parent?.id == record.key } }
    }

    /// Lift `wanted` and put back what was lifted and no longer is, then order and place the stands.
    func apply(_ wanted: [Lift], in mount: UIView, after owner: UIView) {
        NodeView.containerMoves += 1
        defer { NodeView.containerMoves -= 1 }
        let ids = Set(wanted.map(\.node.id))
        for (id, entry) in lifted where !ids.contains(id) && entry.node.map({ holds($0) }) != true { lifted[id] = nil }
        // What the stands hold and the walk no longer wants goes back, tracked or not (a flight's landing);
        // an exit ghost plays its exit where it is
        for stand in stands.values {
            for node in Self.nodes(in: stand) where !ids.contains(node.id) && !presenter.isLeaving(node) {
                if let parent = stand.parent { putBack(node, into: parent) } else { node.removeFromSuperview() }
                lifted[node.id] = nil
            }
        }
        if !wanted.isEmpty { mountLayer(in: mount, after: owner) }
        // A node lifted into a layer out of the window would leave it
        guard wanted.isEmpty || layer?.superview === mount else { return }
        for lift in wanted {
            if records[lift.parent.id] == nil { records[lift.parent.id] = placedIDs(of: lift.parent) }
            let stand = self.stand(lift.parent, after: lift.after, key: lift.after ? lift.exit : lift.enter)
            lifted[lift.node.id] = Lifted(node: lift.node, parent: lift.parent, enter: lift.enter, exit: lift.exit, readsAfter: lift.readsAfter)
            if lift.node.superview !== stand { stand.addSubview(lift.node) }
        }
        // A stand or the layer goes when it holds nothing, never while it holds a ghost
        for (key, stand) in stands where stand.subviews.isEmpty {
            stand.removeFromSuperview()
            stands[key] = nil
        }
        order()
        place(in: mount)
        if stands.isEmpty { dropLayer() }
    }

    /// Every node in a stand back in its parent, as the tabs retire, whether or not it is
    /// tracked (one another host put back there, a flight's landing).
    func putAllBack() {
        guard layer != nil || !lifted.isEmpty || !records.isEmpty else { return }
        NodeView.containerMoves += 1
        defer { NodeView.containerMoves -= 1 }
        for stand in stands.values { retire(stand) }
        lifted = [:]
        stands = [:]
        dropLayer()
        records = [:]
    }

    /// The stands at their parents' places, at the end of a batch.
    func place() {
        guard let root = navigation.container, !isEmpty else { return }
        place(in: navigation.ownerMount(root))
    }

    /// The stands at their parents' places, after a batch moved them.
    func place(in mount: UIView) {
        guard let layer, layer.superview === mount else { return }
        // The mount's extent: its content's too where it scrolls (a scrolling navigation root)
        let content = (mount as? UIScrollView)?.contentSize ?? .zero
        let frame = CGRect(origin: .zero, size: CGSize(width: max(mount.bounds.width, content.width), height: max(mount.bounds.height, content.height)))
        if layer.frame != frame { layer.frame = frame }
        for stand in stands.values { stand.place(in: layer, mount: mount) }
    }

    /// The layer just after the container (LLP 1083.000 D4): at the container's rank, over it
    /// by its subview place, under the root's later children of that rank or a higher one.
    func mountLayer(in mount: UIView, after owner: UIView) {
        guard owner.superview === mount else { return }
        let layer = self.layer ?? PanelSiblingsLayer()
        if self.layer == nil {
            layer.siblings = self
            self.layer = layer
            #if os(iOS)
            relay = MountRelay(presenter, mount: layer)
            #endif
        }
        let at = mount.subviews.firstIndex(of: owner)!
        guard layer.superview !== mount || !mount.subviews.indices.contains(at + 1) || mount.subviews[at + 1] !== layer else { return }
        mount.insertSubview(layer, aboveSubview: owner)
        // Ranked among the mount's children once it is one of them
        layer.setPaintForeground(true, aboveAuthored: false)
    }

    private func dropLayer() {
        layer?.removeFromSuperview()
        layer = nil
        #if os(iOS)
        relay = nil
        #endif
    }

    private func stand(_ parent: NodeView, after: Bool, key place: Int) -> PanelSiblingsStand {
        let key = Key(parent: parent.id, after: after)
        if let stand = stands[key], stand.parent === parent { stand.key = place; return stand }
        if let old = stands[key] { retire(old) }
        let stand = PanelSiblingsStand(parent: parent, after: after, key: place)
        stands[key] = stand
        layer?.addSubview(stand)
        return stand
    }

    /// A stand gone, its views back in its parent: its nodes in the parent's order, then what
    /// else it holds, a flight's slot, so the flying view lands in the parent. With no parent
    /// left, the nodes leave it.
    private func retire(_ stand: PanelSiblingsStand) {
        if let parent = stand.parent {
            for node in Self.nodes(in: stand) { putBack(node, into: parent) }
            for view in stand.subviews { parent.container.addSubview(view) }
        } else {
            for node in Self.nodes(in: stand) { node.removeFromSuperview() }
        }
        stand.removeFromSuperview()
    }

    /// The stands nested as the tree nests their parents, in a depth-first walk's order: a
    /// parent's nodes before the panels at its entry, those after them at its exit, so an outer
    /// parent's earlier nodes come first and its later ones last. In each stand, its parent's order.
    private func order() {
        guard let layer else { return }
        let sorted = stands.values.sorted { $0.key < $1.key }
        for (i, stand) in sorted.enumerated() where layer.subviews.firstIndex(of: stand) != i { layer.insertSubview(stand, at: i) }
        for stand in sorted {
            guard let parent = stand.parent else { continue }
            let place = index(of: parent)
            let nodes = Self.nodes(in: stand)
            let wanted = nodes.sorted { (place[$0.id] ?? Int.max) < (place[$1.id] ?? Int.max) }
            for (i, node) in wanted.enumerated() where stand.subviews.firstIndex(of: node) != i { stand.insertSubview(node, at: i) }
        }
    }

    private func index(of parent: NodeView) -> [UInt32: Int] {
        Dictionary(placedIDs(of: parent).enumerated().map { ($1, $0) }, uniquingKeysWith: { a, _ in a })
    }

    /// Back into its parent's view, before the next of its siblings there in the parent's
    /// order, else after its last node; no children op runs, so a parent that is an exit ghost
    /// keeps it, with its flat leaves, which are laid in again over it.
    private func putBack(_ node: NodeView, into parent: NodeView) {
        let content = parent.container
        let ids = placedIDs(of: parent)
        // Its siblings by id among the nodes in the parent's view, where an exit ghost's nodes stay after the presenter releases them
        let there = Dictionary(Self.nodes(in: content).map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let next = ids.firstIndex(of: node.id).flatMap { i in ids[(i + 1)...].lazy.compactMap { there[$0] }.first }
        if let next { content.insertSubview(node, belowSubview: next) }
        else if let last = Self.nodes(in: content).last { content.insertSubview(node, aboveSubview: last) }
        else { content.addSubview(node) }
        presenter.flats.containerChanged(parent.id)
    }

    // MARK: Reading order (LLP 1083.000 D4: paint and reading order stay apart, as in CSS)

    /// The lifted nodes that read before the routes, and those that read after them, in the
    /// tree's order: by their parents' entry in a depth-first walk on the first side, their
    /// exit on the second, then their parent's order. A node whose stand is hidden, or that is
    /// inert, is not read.
    func reading() -> (before: [NodeView], after: [NodeView]) {
        var places: [UInt32: [UInt32: Int]] = [:]
        let entries = lifted.values.compactMap { entry -> (Lifted, NodeView, Int)? in
            guard let node = entry.node, holds(node), let parent = entry.parent, node.superview?.isHidden == false, !node.authoredInert else { return nil }
            if places[parent.id] == nil { places[parent.id] = index(of: parent) }
            return (entry, node, places[parent.id]?[node.id] ?? Int.max)
        }
        func sorted(_ side: [(Lifted, NodeView, Int)], by key: (Lifted) -> Int) -> [NodeView] {
            side.sorted { key($0.0) != key($1.0) ? key($0.0) < key($1.0) : $0.2 < $1.2 }.map(\.1)
        }
        return (sorted(entries.filter { !$0.0.readsAfter }, by: \.enter), sorted(entries.filter(\.0.readsAfter), by: \.exit))
    }

    func focusChildren(of view: UIView) -> [UIView] {
        guard !isEmpty else { return view.subviews }
        if view is PanelSiblingsLayer { return reading().after }
        if let owner = (navigation.primaryOwner ?? navigation.bootOwner)?.viewIfLoaded, view === owner { return reading().before + view.subviews }
        return view.subviews
    }

    func accessibilityElements(of view: UIView) -> [Any]? {
        guard !isEmpty, let root = navigation.container, view === navigation.ownerMount(root) else { return nil }
        return accessibilityElements(of: view, owner: (navigation.primaryOwner ?? navigation.bootOwner)?.viewIfLoaded)
    }

    func absoluteNodes(after view: UIView, in parent: UIView) -> [NodeView] {
        guard let layer, layer.superview === parent, let at = parent.subviews.firstIndex(of: view),
              let mine = parent.subviews.firstIndex(of: layer), mine > at else { return [] }
        return layer.subviews.flatMap(Self.nodes(in:)).filter { $0.layoutPosition == "absolute" }
    }

    /// The mount's accessibility children while a lifted node reads before the routes: UIKit
    /// reads a container's children by top edge, then leading edge, so those nodes are listed
    /// ahead of the container, whose ties then keep the tree's order; nil otherwise, which
    /// leaves the mount's own list.
    private func accessibilityElements(of mount: UIView, owner: UIView?) -> [Any]? {
        guard let layer, layer.superview === mount, let owner, owner.superview === mount else { return nil }
        let read = reading().before.filter(Self.shown)
        guard !read.isEmpty else { return nil }
        var views: [UIView] = []
        for view in mount.subviews {
            if view === owner { views += read + [owner] } else { views.append(view) }
        }
        return NodeView.accessibilityOrder(views)
    }

    /// The layer's accessibility children: the lifted nodes that read after the routes, in UIKit's
    /// order by their own frames (one with no area read through to its children), so the stands'
    /// frames, which take their parents' clips, do not order them. The mount lists those before the
    /// routes ahead of the container, and the layer after it.
    func readAfter() -> [Any] { NodeView.accessibilityOrder(reading().after.filter(Self.shown)) }

    /// Whether a lifted node's stand shows it to VoiceOver.
    private static func shown(_ node: NodeView) -> Bool {
        node.superview.map { !$0.isHidden && !$0.accessibilityElementsHidden } ?? false
    }
}

extension NavigationHost {
    /// The nodes below the root that hold panels, and the panels.
    var panelHolders: Set<UInt32> {
        guard let root = container else { return [] }
        var ids = Set(tabPanels)
        for panel in tabPanels {
            var at = presenter.logicalParent(of: panel)
            while let id = at, id != root.id, ids.insert(id).inserted { at = presenter.logicalParent(of: id) }
        }
        return ids
    }

    func holdsPanel(_ node: NodeView) -> Bool {
        tabPanels.contains(node.id) || tabPanels.contains { presenter.views[$0]?.isDescendant(of: node) == true }
    }

    /// One walk of the root's children and, within each child that holds panels, down to the
    /// routes (LLP 1083.000 D4). It answers the rows below the panels that stay in CSS flow
    /// while UIKit's container fills the page and the tab bar is hidden, whose highest top
    /// bounds the routes' bottom safe area (`bottom`; `rows` are those that are the root's own
    /// children, which `clearBesideRoutes` raises over the container). Below the root it
    /// answers the nodes that paint over the container (`lifts`): those their parent paints
    /// after the children that hold the panels where a route's box meets them, those beside the
    /// routes (`PanelSiblings.beside`), and those rows, each by the box it paints over
    /// (`paintedBox`). Inside a shown panel, its routes are those children. A node lifted
    /// already is among its parent's children as last placed (`PanelSiblings.placedIDs`); a flat
    /// leaf that would lift is answered in `promote`, to be given a view first.
    func panelSiblings(in area: CGRect, routes: [CGRect], drawn: [CGRect], skipping owned: Set<UInt32>, hidden: Bool? = nil)
        -> (bottom: CGFloat, rows: [NodeView], lifts: [PanelSiblings.Lift], promote: [UInt32]) {
        guard !ExactEnv.authoredChrome, let root = container, tabOwner != nil, !routes.isEmpty else { return (0, [], [], []) }
        var barHidden = false
        if let tabs = tabController, tabOwner === tabs {
            if #available(iOS 18.0, tvOS 18.0, *) { barHidden = hidden ?? tabs.isTabBarHidden } else { barHidden = hidden ?? tabs.tabBar.isHidden }
        }
        let panels = tabPanels.compactMap { presenter.views[$0] }
        // The panels whose routes are shown: a hidden tab's panel is hidden and inert on the web
        let shown = Set(primaryRoutes.filter { $0.viewIfLoaded?.window != nil }.compactMap { logicalParent(of: $0.node)?.id })
        var bottom: CGFloat = 0, rows: [NodeView] = [], lifts: [PanelSiblings.Lift] = [], promote: [UInt32] = []
        var tick = 0
        func isRoute(_ node: NodeView) -> Bool { node.props["navigationKey"] != nil }
        /// A child of `parent` as its order has it: its view, or a flat leaf's id and box in `parent`.
        enum Child { case view(NodeView), leaf(UInt32, CGRect) }
        func visit(_ parent: NodeView?, _ children: [Child]) {
            let enter = tick
            tick += 1
            var level: [(node: NodeView, after: Bool, readsAfter: Bool)] = []
            defer {
                let exit = tick
                tick += 1
                if let parent { lifts += level.map { .init(node: $0.node, parent: parent, enter: enter, exit: exit, after: $0.after, readsAfter: $0.readsAfter) } }
            }
            let inPanel = parent.map { p in panels.contains { $0 === p } } ?? false
            let holders = children.indices.filter { i in
                guard case let .view(child) = children[i] else { return false }
                return inPanel ? isRoute(child) : panels.contains { $0 === child || $0.isDescendant(of: child) }
            }
            guard let firstHolder = holders.first else { return }
            let views = { (i: Int) -> NodeView? in if case let .view(v) = children[i] { return v } else { return nil } }
            let end = holders.compactMap(views).map { $0.convert($0.bounds, to: root).maxY }.max() ?? 0
            // The parent's paint order (LLP 1083.000 §2.4): rank, then tree order; a flat leaf is rank 0
            let rank = { (i: Int) -> Int64 in views(i)?.paintRank ?? 0 }
            let painted = holders.map { (rank($0), $0) }
            let firstPainted = painted.min { $0 < $1 }!, lastPainted = painted.max { $0 < $1 }!
            for i in children.indices where !holders.contains(i) {
                let box: CGRect, inFlow: Bool
                switch children[i] {
                case let .view(child):
                    guard !owned.contains(child.id), !isRoute(child), !child.isHidden else { continue }
                    box = paintedBox(child, in: root)
                    inFlow = RouteGeometry.inFlow(child)
                case let .leaf(_, layout):
                    guard let parent else { continue }
                    box = parent.container.convert(layout, to: root)
                    inFlow = true
                }
                guard !box.isEmpty, box.intersects(area) else { continue }
                var below = false
                if barHidden, !inPanel, inFlow, box.minY >= end - 0.5, routes.contains(where: { $0.minX < box.maxX && $0.maxX > box.minX }) {
                    bottom = max(bottom, area.maxY - box.minY)
                    below = true
                }
                guard let parent else {
                    if below, let child = views(i) { rows.append(child) }
                    continue
                }
                let key = (rank(i), i)
                let over = key > lastPainted && routes.contains(where: { $0.intersects(box) })
                guard over || below || PanelSiblings.beside(box, routes, drawn: drawn) else { continue }
                switch children[i] {
                case let .view(child):
                    // In its parent's view, or in one of its parent's stands (lifted, or put back there by a flight's landing)
                    guard child.superview === parent.container || siblings.parent(of: child) === parent, !heldElsewhere(child, under: parent) else { continue }
                    level.append((child, key > firstPainted, i > firstHolder))
                case let .leaf(id, _):
                    promote.append(id)
                }
            }
            for i in holders {
                guard let branch = views(i) else { continue }
                if panels.contains(where: { $0 === branch }) {
                    guard shown.contains(branch.id) else { continue }
                } else if owned.contains(branch.id) || inPanel { continue }
                visit(branch, siblings.placedIDs(of: branch).compactMap { id in
                    presenter.views[id].map(Child.view) ?? presenter.flats.paintedBox(of: id).flatMap { $0.isEmpty ? nil : .leaf(id, $0) }
                })
            }
        }
        visit(nil, PanelSiblings.nodes(in: ownerMount(root)).map(Child.view))
        return (bottom, rows, lifts, promote)
    }

    /// The box `node` paints over, in `root`'s coordinates: its own, or, where it has no area, as CSS
    /// paints a box's overflow, the boxes of its children that are views and of its flat leaves, each
    /// measured so in turn, together within each view from their content view up to the node that
    /// clips (a clip box holds what a one-axis clip shows). A node with a `clip-path` keeps its own
    /// box, so what a shape shows outside an empty box is not counted. Empty where it paints nothing.
    func paintedBox(_ node: NodeView, in root: UIView) -> CGRect {
        let own = node.convert(node.bounds, to: root)
        guard own.isEmpty, node.clipPath == nil else { return own }
        let content = node.container
        let views = PanelSiblings.nodes(in: content).filter { !$0.isHidden }.map { paintedBox($0, in: root) }
        let leaves = (presenter.flats.children(of: node.id) ?? []).compactMap { presenter.flats.paintedBox(of: $0) }.filter { !$0.isEmpty }.map { content.convert($0, to: root) }
        let painted = (views.filter { !$0.isEmpty } + leaves).reduce(CGRect.null) { $0.union($1) }
        return sequence(first: content as UIView, next: { $0 === node ? nil : $0.superview })
            .filter(\.clipsToBounds).reduce(painted) { $0.intersection($1.convert($1.bounds, to: root)) }
    }

    /// Whether another host holds `node`'s view now: a flight, the top layer, a context
    /// menu's preview, an arrangement.
    private func heldElsewhere(_ node: NodeView, under parent: NodeView) -> Bool {
        if presenter.menus.lifted(node) || presenter.flightHost.isFlying(node) { return true }
        #if os(iOS)
        if #available(iOS 27.1, *), presenter.arrangements.owns(node, under: parent) { return true }
        #endif
        return false
    }
}
#endif
