#if os(iOS)
import UIKit

final class ArrangementPaneController: UIViewController {
    let node: NodeView
    let room: () -> CGSize
    let changed: () -> Void
    package init(node: NodeView, room: @escaping () -> CGSize, changed: @escaping () -> Void) {
        self.node = node; self.room = room; self.changed = changed
        super.init(nibName: nil, bundle: nil)
    }
    package required init?(coder: NSCoder) { nil }
    override func loadView() {
        view = PaneView(node: node, room: room)
        view.clipsToBounds = true
    }
    func contentChanged() {
        guard let pane = viewIfLoaded as? PaneView, pane.refresh() else { return }
        pane.invalidateIntrinsicContentSize()
        parent?.view.setNeedsLayout()
    }
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        changed()
    }
    override func viewSafeAreaInsetsDidChange() {
        super.viewSafeAreaInsetsDidChange()
        changed()
    }

    private final class PaneView: UIView {
        private let node: NodeView
        private let room: () -> CGSize
        private struct Measurement {
            let block: HostBlock
            let stamp: UInt64
            let size: CGSize?
        }
        private var intrinsic: Measurement?
        private var fitting: Measurement?
        private var measuring = false

        package init(node: NodeView, room: @escaping () -> CGSize) {
            self.node = node; self.room = room
            super.init(frame: .zero)
        }
        package required init?(coder: NSCoder) { nil }

        override var intrinsicContentSize: CGSize {
            // The enclosing element's room is independent of the pane's allocation.
            intrinsic = measure(.region(room()), previous: intrinsic)
            return intrinsic?.size ?? CGSize(width: UIView.noIntrinsicMetric, height: UIView.noIntrinsicMetric)
        }

        override func sizeThatFits(_ size: CGSize) -> CGSize {
            fit(size, horizontal: .fittingSizeLevel, vertical: .fittingSizeLevel) ?? .zero
        }

        override func systemLayoutSizeFitting(_ targetSize: CGSize) -> CGSize {
            systemLayoutSizeFitting(targetSize, withHorizontalFittingPriority: .fittingSizeLevel, verticalFittingPriority: .fittingSizeLevel)
        }

        override func systemLayoutSizeFitting(_ targetSize: CGSize, withHorizontalFittingPriority horizontal: UILayoutPriority,
                                             verticalFittingPriority vertical: UILayoutPriority) -> CGSize {
            fit(targetSize, horizontal: horizontal, vertical: vertical)
                ?? super.systemLayoutSizeFitting(targetSize, withHorizontalFittingPriority: horizontal, verticalFittingPriority: vertical)
        }

        private func fit(_ size: CGSize, horizontal: UILayoutPriority, vertical: UILayoutPriority) -> CGSize? {
            let axis = { (length: CGFloat, priority: UILayoutPriority) -> HostBlock.Axis in
                let finite = max(0, min(length, CGFloat(Float.greatestFiniteMagnitude)))
                return priority == .required ? .size(finite) : .region(finite)
            }
            fitting = measure(HostBlock(width: axis(size.width, horizontal), height: axis(size.height, vertical)), previous: fitting)
            return fitting?.size
        }

        private func measure(_ block: HostBlock, previous: Measurement?) -> Measurement {
            let held = previous?.block == block ? previous?.size : nil
            guard !measuring, let presenter = node.presenter, presenter.views[node.id] === node,
                  let session = presenter.session, !Owner.shared.isServingMain else {
                return Measurement(block: block, stamp: 0, size: held)
            }
            let stamp = session.runtime.layoutStamp()
            if stamp != 0, let previous, previous.block == block, previous.stamp == stamp { return previous }
            measuring = true
            defer { measuring = false }
            guard let measured = session.runtime.measureBlock(node.id, in: block) else {
                // An initial allocation or pending layout may not be measurable yet. Retry after its batch.
                return Measurement(block: block, stamp: 0, size: held)
            }
            return Measurement(block: block, stamp: stamp, size: measured.used)
        }

        func refresh() -> Bool {
            let before = (intrinsic?.size, fitting?.size)
            if intrinsic != nil { intrinsic = measure(.region(room()), previous: intrinsic) }
            if let last = fitting { fitting = measure(last.block, previous: last) }
            // Allocation reports advance the layout stamp too; only a different answer invalidates UIKit.
            return before.0 != intrinsic?.size || before.1 != fitting?.size
        }
    }
}

/// Whether an arranged pane, laid out with zero clearance, ends at the bottom of its block (LLP 1123 D3), as last
/// measured: the block it was measured in and the layout stamp it was measured at.
struct BottomReach {
    let block: HostBlock, stamp: UInt64, reaches: Bool

    /// `end` is the pane's bottom in the block it was last told, `told`, and `slack` is what the route pads below the
    /// arrangement now. While the route pads, the pane is told `slack` less than it would be told without the padding,
    /// so its layout cannot answer: a pane that fills the shorter block and stops short of the longer one would take
    /// the edge, the route would stop padding, the pane would give the edge back, and so on until the stack overflows.
    /// It is measured in the unpadded block instead, with the whole slack added to its height, as a side-by-side split
    /// gives it. A stacked split may give it less; misjudging that block can keep the route padding, never alternate.
    /// Unmeasured (the runtime busy), it keeps the answer it holds for that block, else gives the edge up.
    static func reaches(_ pane: NodeView, end: CGFloat, told: HostBlock, slack: CGFloat, held: inout BottomReach?) -> Bool {
        guard case .size(let height) = told.height, abs(height - end) < 1 else { return false }
        if slack < 1 { return true }
        let block = HostBlock(width: told.width, height: .size(height + slack))
        guard let presenter = pane.presenter, presenter.views[pane.id] === pane, let runtime = presenter.session?.runtime,
              !Owner.shared.isServingMain else { return held?.block == block && held?.reaches == true }
        let stamp = runtime.layoutStamp()
        if let held, held.block == block, held.stamp == stamp, stamp != 0 { return held.reaches }
        guard let measured = runtime.measureBlock(pane.id, in: block) else { return held?.block == block && held?.reaches == true }
        let reaches = abs(height + slack - measured.box.margins.top - measured.box.size.height) < 1
        held = BottomReach(block: block, stamp: stamp, reaches: reaches)
        return reaches
    }
}

/// UIKit containment for two retained authored panes
@available(iOS 27.1, *)
package final class ArrangementHost {
    private unowned let presenter: Presenter
    private var requests: [UInt32: UIArrangementViewController] = [:]
    private var insetScrollers: [UInt32: UIScrollView] = [:]
    private var pendingTops: [UInt32: CGFloat] = [:]
    private var syncingInsets = false
    private var layoutPending = false
    /// The panes UIKit hides, as last reported covered whole.
    private var toldHidden: Set<UInt32> = []
    private var reaches: [UInt32: BottomReach] = [:]
    private var children: [UInt32: [UInt32]] = [:]
    private(set) var entries: [UInt32: Entry] = [:]

    final class Entry {
        let node: NodeView
        let controller: UIArrangementViewController
        let panes: [NodeView]
        let holders: [ArrangementPaneController]
        package init(node: NodeView, controller: UIArrangementViewController, panes: [NodeView], changed: @escaping () -> Void) {
            self.node = node; self.controller = controller; self.panes = panes
            holders = panes.map { ArrangementPaneController(node: $0, room: { [weak node] in node?.container.bounds.size ?? .zero }, changed: changed) }
            for holder in holders { holder.loadViewIfNeeded() }
            controller.setViewController(holders[0], for: .primary)
            controller.setViewController(holders[1], for: .secondary)
        }
        func isHidden(_ holder: ArrangementPaneController) -> Bool {
            // Read view visibility because an overlay can report hidden while its views remain visible
            var current: UIView? = holder.view
            while let view = current {
                if view.isHidden { return true }
                if view === controller.view { return false }
                current = view.superview
            }
            return true
        }
    }

    package init(_ presenter: Presenter) { self.presenter = presenter }

    /// The policy last applied to this backend, until the next inset sync.
    package func insets(_ node: NodeView) -> Bool {
        guard let scroll = node.scrollView else { return false }
        return insetScrollers[node.id] === scroll
    }

    func set(_ node: UInt32, controller: UIArrangementViewController?) -> Bool {
        guard let root = presenter.views[node] else { return false }
        if controller != nil, paneIDs(of: root) == nil { return false }
        if let controller, entries[node]?.controller !== controller {
            guard controller.parent == nil, controller.viewIfLoaded?.superview == nil,
                  controller.viewController(for: .primary) == nil,
                  controller.viewController(for: .secondary) == nil else { return false }
        }
        if let controller, requests.contains(where: { $0.key != node && $0.value === controller }) { return false }
        requests[node] = controller
        sync()
        return true
    }

    func record(_ node: NodeView, children ids: [UInt32]) {
        if node.props["hatch"] != nil || requests[node.id] != nil || entries[node.id] != nil { children[node.id] = ids }
    }

    package func owns(_ node: NodeView, under parent: NodeView) -> Bool {
        entries[parent.id]?.panes.contains { $0 === node } == true
    }

    func sync() {
        guard !requests.isEmpty || !entries.isEmpty || !insetScrollers.isEmpty else { return }
        for (id, entry) in entries where requests[id] !== entry.controller || presenter.views[id] !== entry.node || entry.panes.contains(where: { presenter.views[$0.id] !== $0 }) || (children[id] != nil && children[id] != entry.panes.map(\.id)) {
            retire(entry); entries[id] = nil
            if presenter.views[id] === entry.node { presenter.placeChildren(entry.node, children[id] ?? entry.panes.map(\.id)) }
            // Its panes are no longer hidden by UIKit: their covers go with the next layout pass.
            layoutChanged()
        }
        for (id, controller) in requests where entries[id] == nil {
            guard let node = presenter.views[id], let owner = ViewPresentation.controllers(for: node, layout: true).first,
                  let ids = paneIDs(of: node) else { continue }
            let panes = ids.compactMap { presenter.views[$0] }
            let entry = Entry(node: node, controller: controller, panes: panes) { [weak self] in self?.layoutChanged() }
            entries[id] = entry
            owner.addChild(controller)
            controller.view.frame = node.container.bounds
            controller.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            node.container.addSubview(controller.view)
            controller.didMove(toParent: owner)
            controller.view.layoutIfNeeded()
            // Keep visible content in its window while changing containment
            for holder in entry.holders { holder.view.addSubview(holder.node) }
        }
        syncInsets()
        refreshContentSizes()
        presenter.owners.publish()
    }

    private func refreshContentSizes() {
        for entry in entries.values {
            for holder in entry.holders { holder.contentChanged() }
        }
    }

    /// Promote flat leaves so both authored children can enter UIKit containment
    private func paneIDs(of node: NodeView) -> [UInt32]? {
        let ids = children[node.id] ?? node.container.subviews.compactMap { ($0 as? NodeView)?.id }
        guard ids.count == 2 else { return nil }
        for id in ids where presenter.views[id] == nil { presenter.flats.promote(id) }
        return ids.allSatisfy { presenter.views[$0] != nil } ? ids : nil
    }

    package func route(of node: NodeView) -> (any RouteSurface)? {
        ViewPresentation.controllers(for: node, layout: true).compactMap { $0 as? any RouteSurface }.first
    }

    private func retire(_ entry: Entry) {
        for pane in entry.panes { reaches[pane.id] = nil }
        let ids = children[entry.node.id] ?? entry.panes.map(\.id)
        let held = entry.holders.filter { $0.node.superview === $0.view }.map(\.node)
        for pane in held where presenter.views[pane.id] !== pane || !ids.contains(pane.id) { pane.removeFromSuperview() }
        for id in ids {
            if let pane = held.first(where: { $0.id == id }), presenter.views[id] === pane { entry.node.container.addSubview(pane) }
        }
        entry.controller.willMove(toParent: nil)
        entry.controller.view.removeFromSuperview()
        entry.controller.removeFromParent()
        entry.controller.setViewController(nil, for: .primary)
        entry.controller.setViewController(nil, for: .secondary)
    }

    /// Follow authored pane order across the UIKit containment boundary.
    func panes(in node: NodeView) -> [NodeView]? {
        guard let entry = entries[node.id] else { return nil }
        return entry.holders.filter { !entry.isHidden($0) && presenter.views[$0.node.id] === $0.node }.map(\.node)
    }

    private func layoutChanged() {
        NodeView.scrollLayoutEpoch &+= 1
        guard !layoutPending else { return }
        layoutPending = true
        // Read UIKit's safe area after its current layout has finished
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            layoutPending = false
            guard !entries.isEmpty || !insetScrollers.isEmpty else { return reportHiddenPanes() }
            syncInsets()
            refreshContentSizes()
            presenter.owners.publish()
            reportHiddenPanes()
            presenter.navigation.coversChanged()
        }
    }

    /// Panes touching a route edge own its inset; interior panes must already clear that edge.
    package func scrollers(in route: NodeView, top: Bool) -> [NodeView] {
        entries.values.flatMap { entry -> [NodeView] in
            let node = entry.node, room = node.contentBox()
            guard node.isDescendant(of: route), node.transform.isIdentity, node.clipPath == nil,
                  top ? room.minY <= 0.5 : node.bounds.maxY - room.maxY <= 0.5,
                  let slack = reachesEdge(node, in: route, top: top) else { return [] }
            // Leave out a pane laid out with no size, so that it neither owns an edge nor keeps its sibling from one: one
            // UIKit shows again is 0×0 until its covers follow
            let holders = entry.holders.filter { !entry.isHidden($0) && !$0.node.isHidden && !$0.view.bounds.isEmpty && !$0.node.bounds.isEmpty }
            let touching = holders.filter { holder in
                let allocation = holder.view.convert(holder.view.bounds, to: node)
                return abs(top ? allocation.minY : node.bounds.maxY - allocation.maxY) < 1
            }
            guard !touching.isEmpty, holders.allSatisfy({ holder in
                touching.contains { $0 === holder } || (top ? holder.view.safeAreaInsets.top : holder.view.safeAreaInsets.bottom) < 1
            }), touching.allSatisfy({ holder in
                // Compare the pane with the block it was last told, not with UIKit's allocation: when a cover's answer
                // resizes the arrangement, UIKit resizes the allocation a layout before the pane follows, and a pane
                // measured against it then gives up the edge, which pads the route and resizes the allocation back.
                // The top compares only the pane's origin, where it sits whatever block it was told.
                let pane = holder.node, frame = pane.convert(pane.bounds, to: holder.view)
                guard let told = presenter.owners.told[pane.id]?.block, case .size = told.height,
                      pane.scrollView != nil, pane.scrollsVertically, pane.transform.isIdentity, pane.clipPath == nil, RouteGeometry.inFlow(pane) else { return false }
                if top { return abs(frame.minY) < 1 }
                return BottomReach.reaches(pane, end: frame.maxY, told: told, slack: slack, held: &reaches[pane.id])
            }) else { return [] }
            return touching.map(\.node)
        }
    }

    /// Whether `node` reaches the route's edge through every box between them, and if so the clearance the route
    /// pads between them on that edge now: zero, or the route's cover where it still reserves the edge.
    private func reachesEdge(_ node: NodeView, in route: NodeView, top: Bool) -> CGFloat? {
        var child = node, slack: CGFloat = 0
        while child !== route {
            guard !child.isHidden, RouteGeometry.inFlow(child) else { return nil }
            var up = child.superview
            while let view = up, !(view is NodeView) {
                guard !(view is UIScrollView), view.transform.isIdentity, view.layer.mask == nil else { return nil }
                up = view.superview
            }
            guard let parent = up as? NodeView, parent.transform.isIdentity, parent.clipPath == nil, !parent.isHidden else { return nil }
            let room = parent.contentBox()
            let rect = child.convert(child.bounds, to: parent)
            let distance = top ? rect.minY - room.minY : room.maxY - rect.maxY
            if parent === route {
                var cover: CGFloat = 0
                if case .edges(let edges)? = presenter.navigation.covers[parent.id] { cover = top ? edges.top : edges.bottom }
                // The route may still reserve this edge, or its scrollers may already own it
                if abs(distance) < 1 { break }
                guard abs(distance - cover) < 1 else { return nil }
                slack = cover
            } else {
                guard abs(distance) < 1, top ? room.minY <= 0.5 : parent.bounds.maxY - room.maxY <= 0.5 else { return nil }
            }
            child = parent
        }
        return slack
    }

    private func syncInsets() {
        NodeView.scrollLayoutEpoch &+= 1
        guard !syncingInsets else { return }
        syncingInsets = true
        defer { syncingInsets = false }
        adoptOwners()
        syncSafeAreas()
        var now: [UInt32: UIScrollView] = [:]
        for route in presenter.navigation.routeSurfaces.values where !RouteGeometry.insetsNoTop(route.node) {
            for node in scrollers(in: route.node, top: true) + scrollers(in: route.node, top: false) {
                now[node.id] = node.scrollView
            }
        }
        let previous = insetScrollers
        insetScrollers = now
        for (id, scroll) in previous where now[id] !== scroll {
            let node = presenter.views[id]
            let saved = pendingTops.removeValue(forKey: id)
            if hiddenPanes.contains(id), let node {
                let top = node.pendingScrollTop.map { CGFloat($0) } ?? saved ?? (scroll.contentOffset.y + node.scrollTopInset(scroll))
                node.hiddenScroll = CGPoint(x: scroll.contentOffset.x, y: top)
            }
            if let node, presenter.navigation.routeSurfaces.values.contains(where: { $0.collapseScroll === node || $0.edgeScroll === node }) { continue }
            let top = scroll.contentOffset.y + (node?.scrollTopInset(scroll) ?? scroll.adjustedContentInset.top)
            scroll.contentInsetAdjustmentBehavior = .never
            node?.scrollOrigin = 0; node?.scrollCollapsed = 0
            scroll.contentOffset.y = top - (node?.scrollTopInset(scroll) ?? 0)
        }
        for (id, scroll) in now {
            guard let node = presenter.views[id] else { continue }
            let top = scroll.contentOffset.y + node.scrollTopInset(scroll)
            if previous[id] !== scroll {
                pendingTops[id] = node.pendingScrollTop.map { CGFloat($0) } ?? node.hiddenScroll?.y ?? top
            }
            if scroll.contentInsetAdjustmentBehavior != .automatic {
                scroll.contentInsetAdjustmentBehavior = .automatic
                scroll.contentOffset.y = top - node.scrollTopInset(scroll)
            }
        }
        for entry in entries.values {
            for holder in entry.holders {
                let scroll = now[holder.node.id]
                if holder.contentScrollView(for: .top) !== scroll || holder.contentScrollView(for: .bottom) !== scroll {
                    holder.setContentScrollView(scroll, for: .all)
                }
            }
        }
        finishScrollHandoffs()
    }

    /// Move each arrangement under the controller that holds its node now. A route adopts its
    /// content after the arrangement is made, and UIKit gives a child controller's view the safe
    /// area of its parent's, so an arrangement left under the root misses the route's bars.
    private func adoptOwners() {
        for entry in entries.values {
            guard let owner = ViewPresentation.controllers(for: entry.node, layout: true).first, entry.controller.parent !== owner else { continue }
            entry.controller.willMove(toParent: nil)
            entry.controller.removeFromParent()
            owner.addChild(entry.controller)
            entry.controller.didMove(toParent: owner)
        }
    }

    /// Forward the enclosing bars where nested UIKit containment omits their safe area. UIKit
    /// hands a route's own child its bars, a layout after the route gains them, so forwarding
    /// them there would give its panes the bars twice for that frame.
    private func syncSafeAreas() {
        for entry in entries.values {
            guard let route = route(of: entry.node), let window = route.surface.window else { continue }
            if entry.controller.parent === route {
                for holder in entry.holders where holder.additionalSafeAreaInsets != .zero { holder.additionalSafeAreaInsets = .zero }
                continue
            }
            for holder in entry.holders where holder.view.window === window {
                let frame = holder.view.convert(holder.view.bounds, to: route.surface)
                let room = route.surface.bounds.inset(by: route.surface.safeAreaInsets)
                let expected = UIEdgeInsets(top: max(0, min(frame.height, room.minY - frame.minY)), left: 0,
                                           bottom: max(0, min(frame.height, frame.maxY - room.maxY)), right: 0)
                let actual = holder.view.safeAreaInsets, extra = holder.additionalSafeAreaInsets
                let needed = UIEdgeInsets(top: max(0, expected.top - max(0, actual.top - extra.top)), left: 0,
                                         bottom: max(0, expected.bottom - max(0, actual.bottom - extra.bottom)), right: 0)
                if needed != extra { holder.additionalSafeAreaInsets = needed }
            }
        }
    }

    private func finishScrollHandoffs() {
        // Finish adoption after UIKit has laid out the pane under its bars
        for (id, top) in pendingTops {
            guard let node = presenter.views[id], let scroll = insetScrollers[id] else { continue }
            if scroll.isTracking || scroll.isDragging || scroll.isDecelerating || (node.pendingScrollTop != nil && node.extras?.pendingScrollTransfer != true) {
                pendingTops[id] = nil
                continue
            }
            let point = PixelGrid.offset(CGPoint(x: scroll.contentOffset.x, y: top - node.scrollTopInset(scroll)), in: scroll)
            if scroll.contentOffset != point { scroll.contentOffset = point }
            guard let route = route(of: node) else { continue }
            let frame = scroll.convert(scroll.bounds, to: route.surface)
            let expected = max(0, route.surface.safeAreaInsets.top - frame.minY)
            if abs(scroll.safeAreaInsets.top - expected) < 1 {
                scroll.contentOffset = PixelGrid.offset(scroll.contentOffset, in: scroll)
                pendingTops[id] = nil
            }
        }
    }

    func cancelScrollHandoff(_ id: UInt32) { pendingTops[id] = nil }

    /// A pane UIKit hides takes no room in the authored layout: it is covered
    /// whole, told with what changed since the last report. A scroller's least
    /// content height follows its cover (`scrollFloor`).
    private func reportHiddenPanes() {
        let now = Set(hiddenPanes)
        guard now != toldHidden else { return }
        let changes = now.subtracting(toldHidden).map { ($0, HostCover.whole as HostCover?) }
            + toldHidden.subtracting(now).filter { presenter.views[$0] != nil }.map { ($0, nil as HostCover?) }
        toldHidden = now
        for (id, _) in changes { presenter.views[id]?.fitScroll() }
        presenter.onCovers?(changes)
    }

    /// Native-hidden panes take no room in the authored edge chain.
    package var hiddenPanes: [UInt32] {
        entries.values.flatMap { entry in
            entry.holders.filter { entry.isHidden($0) }.map { $0.node.id }
        }
    }

    package var blocks: [UInt32: (block: HostBlock, placement: BlockPlacement?)] {
        var out: [UInt32: (block: HostBlock, placement: BlockPlacement?)] = [:]
        for entry in entries.values {
            for holder in entry.holders {
                let bounds = entry.isHidden(holder) ? CGRect.zero : holder.view.bounds
                let rect = bounds.isEmpty ? CGRect.zero : holder.view.convert(bounds, to: entry.node.container)
                out[holder.node.id] = (.size(bounds.size), BlockPlacement(rect: rect, within: entry.node.id, contentOffset: nil))
            }
        }
        return out
    }

    func remove(_ id: UInt32) {
        let request = requests.removeValue(forKey: id)
        let entry = entries.removeValue(forKey: id)
        if let entry { retire(entry) }
        children[id] = nil
        guard request != nil || entry != nil else { return }
        presenter.afterBatch { [weak self] in self?.sync() }
    }

    func reset() {
        for entry in entries.values { retire(entry) }
        entries = [:]; requests = [:]; children = [:]; toldHidden = []
        syncInsets()
    }
}
#endif

#if os(iOS) || os(tvOS)
import UIKit

extension Presenter {
    func arrangementHides(_ id: UInt32) -> Bool {
        #if os(iOS)
        if #available(iOS 27.1, *) { return arrangements.hiddenPanes.contains(id) }
        #endif
        return false
    }
}
#endif
