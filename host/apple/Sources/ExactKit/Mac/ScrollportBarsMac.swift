// LLP 1123 D12 on AppKit: a scroller's own bars lie on its scrollport and do not scroll. The host mounts them in a
// holder over the scroller's scroll view, at the scroller's laid-out padding box and clipped to it, so its content
// scrolls under them while they keep their place. A click on a bar goes to the bar and any other to the scroll view;
// the holder is no accessibility element, so each bar's controls are accessibility elements once, where they lie.
#if os(macOS)
import AppKit
import ExactCore

/// The view that holds a scroller's bars over its scroll view, at its padding box and clipped to it.
package final class ScrollportHolder: NSView {
    /// A bar it holds, held weakly: a bar's view an exit ends lets it go.
    final class Bar {
        weak var view: NodeView?
        init(_ view: NodeView) { self.view = view }
    }
    /// The bars it holds, in order.
    var bars: [Bar] = []

    package override init(frame: NSRect) {
        super.init(frame: frame)
        clipsToBounds = true
    }

    @available(*, unavailable)
    package required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    package override var isFlipped: Bool { true }
    package override func isAccessibilityElement() -> Bool { false }

    // A click goes to its bars, the last drawn first, and one on none of them to the scroll view beneath
    package override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden, frame.contains(point) else { return nil }
        let inside = convert(point, from: superview)
        for bar in subviews.reversed() {
            if let hit = bar.hitTest(inside) { return hit }
        }
        return nil
    }
}

package enum ScrollportBars {
    /// Which bars lie on their parents' scrollports, and the children op each scroller last had, by presenter. A list
    /// is held by its scroller's view, weakly, and goes once the view is no longer the presenter's (`place`).
    final class State {
        var bars: Set<UInt32> = []
        var childLists: [ObjectIdentifier: ChildList] = [:]
    }

    /// A scroller's last children op, and the scroller, held weakly.
    final class ChildList {
        weak var parent: NodeView?
        let ids: [UInt32]
        init(_ parent: NodeView, _ ids: [UInt32]) { (self.parent, self.ids) = (parent, ids) }
    }

    nonisolated(unsafe) static let states = NSMapTable<Presenter, State>.weakToStrongObjects()
    nonisolated(unsafe) static let holders = NSMapTable<NodeView, ScrollportHolder>.weakToWeakObjects()

    private static func state(of presenter: Presenter) -> State {
        if let state = states.object(forKey: presenter) { return state }
        let made = State()
        states.setObject(made, forKey: presenter)
        return made
    }

    /// Replaces which bars lie on their parents' scrollports, by id, and places the children of each parent that
    /// changes again: activation supplies them from the kernel (`NodeArena::on_scrollport`, stage 5), and tests set
    /// them directly.
    package static func set(_ bars: Set<UInt32>, in presenter: Presenter) {
        let state = state(of: presenter)
        guard state.bars != bars else { return }
        let changed = state.bars.symmetricDifference(bars)
        state.bars = bars
        for list in Array(state.childLists.values) {
            guard let parent = list.parent, presenter.views[parent.id] === parent, !changed.isDisjoint(with: list.ids) else { continue }
            let children = list.ids
            var op = BatchOp(op: .children, nodeID: parent.id)
            op.ids = children
            var reparented = Set<UInt32>()
            presenter.placeChildren(op, reparented: &reparented)
        }
    }

    /// `parent`'s children as its children op lists them less its bars on its scrollport, which this mounts in its
    /// holder in their order; the rest are the caller's to place in its container. A scroller's list is kept, to place
    /// it again when its bars change (`set`).
    static func place(_ parent: NodeView, _ ids: [UInt32], in presenter: Presenter) -> [UInt32] {
        // The lists are kept from the first, so bars set after their scroller's children mounted place them again
        let state = state(of: presenter)
        let key = ObjectIdentifier(parent)
        if parent.scroll != nil || state.childLists[key] != nil { state.childLists[key] = ids.isEmpty ? nil : ChildList(parent, ids) }
        // A destroyed or renewed scroller's list goes, which its view may outlive
        state.childLists = state.childLists.filter { _, list in list.parent.map { presenter.views[$0.id] === $0 } ?? false }
        let bars = state.bars.isEmpty ? [] : ids.filter(state.bars.contains)
        guard !bars.isEmpty || holders.object(forKey: parent) != nil else { return ids }
        mount(bars.compactMap { presenter.views[$0] }, keeping: Set(ids), in: parent, presenter)
        return bars.isEmpty ? ids : ids.filter { !state.bars.contains($0) }
    }

    /// Keeps `parent`'s holder at its padding box, the bars' coordinates its border box's, and over its scroll view.
    package static func raise(in parent: NodeView) {
        guard let holder = holders.object(forKey: parent) else { return }
        let uniform = parent.number("border_width")
        let border = (top: parent.number("border_width_top", uniform), left: parent.number("border_width_left", uniform),
                      bottom: parent.number("border_width_bottom", uniform), right: parent.number("border_width_right", uniform))
        let box = parent.laidOutBox
        let frame = CGRect(x: box.minX + border.left, y: box.minY + border.top,
                           width: max(0, box.width - border.left - border.right), height: max(0, box.height - border.top - border.bottom))
        if holder.frame != frame { holder.frame = frame }
        if holder.bounds.origin != frame.origin { holder.setBoundsOrigin(frame.origin) }
        if parent.subviews.last !== holder { parent.addSubview(holder, positioned: .above, relativeTo: nil) }
    }

    private static func mount(_ bars: [NodeView], keeping ids: Set<UInt32>, in parent: NodeView, _ presenter: Presenter) {
        let holder = holders.object(forKey: parent) ?? {
            let made = ScrollportHolder(frame: parent.bounds)
            parent.addSubview(made)
            holders.setObject(made, forKey: parent)
            return made
        }()
        // One no longer a bar but still a child moves into the container, where the caller places it; one no longer a
        // child goes, as the container's children do, unless it leaves with its exit
        for old in holder.bars.compactMap(\.view) where !bars.contains(old) && old.superview === holder {
            if ids.contains(old.id) { parent.container.addSubview(old) } else if !presenter.isLeaving(old) { old.removeFromSuperview() }
        }
        for (i, bar) in bars.enumerated() where bar.superview !== holder || holder.bars.firstIndex(where: { $0.view === bar }) != i {
            let above = i > 0 ? bars[i - 1] : nil
            holder.addSubview(bar, positioned: above == nil ? .below : .above, relativeTo: above)
            presenter.regrid(bar)
        }
        holder.bars = bars.map(ScrollportHolder.Bar.init)
        if bars.isEmpty, holder.subviews.isEmpty {
            holder.removeFromSuperview()
            holders.removeObject(forKey: parent)
        } else {
            raise(in: parent)
        }
    }
}
#endif
