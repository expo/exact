// LLP 1123 D12: a scroller's own bars lie on its scrollport and do not scroll. The host mounts
// them in a holder over the scroller's scroll view or collection, at the scroller's laid-out box
// and clipped to it, so its content scrolls under them while they keep their place. A touch on a
// bar goes to the bar and any other to the scroll view; the holder is no accessibility element,
// so each bar's controls are accessibility elements once, where they are drawn.
#if os(iOS) || os(tvOS)
import UIKit

/// The view that holds a scroller's bars over its scroll view, at its padding box and clipped to it.
package final class ScrollportHolder: UIView {
    /// A bar it holds, held weakly: a bar's view the pool parks or an exit ends lets it go.
    final class Bar {
        weak var view: NodeView?
        init(_ view: NodeView) { self.view = view }
    }
    /// The bars it holds, in order.
    var bars: [Bar] = []

    package override init(frame: CGRect) {
        super.init(frame: frame)
        clipsToBounds = true
        autoresizingMask = [.flexibleWidth, .flexibleHeight]
    }

    @available(*, unavailable)
    package required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    // A touch goes to its bars in their paint order, and one on none of them to the scroll view beneath
    package override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        guard !isHidden, isUserInteractionEnabled, self.point(inside: point, with: event) else { return nil }
        return NodeView.hitChildren(in: self, at: point, with: event)
    }
}

package enum ScrollportBars {
    /// Replaces which bars lie on their parents' scrollports, by id, and mounts the children of
    /// each parent that changes again: activation supplies them from the kernel
    /// (`NodeArena::on_scrollport`, stage 5), and tests set them directly.
    package static func set(_ bars: Set<UInt32>, in presenter: Presenter) {
        guard let viewport = presenter.viewport as? Viewport, viewport.scrollportBars != bars else { return }
        let changed = viewport.scrollportBars.symmetricDifference(bars)
        viewport.scrollportBars = bars
        for (id, children) in viewport.childLists where !changed.isDisjoint(with: children) {
            if let parent = presenter.views[id] { presenter.placeChildren(parent, children) }
        }
    }

    /// `parent`'s children as its children op lists them less its bars on its scrollport, which
    /// this mounts in its holder in their order; the rest are the caller's to place in its
    /// container. A scroller's list is kept, to mount it again when its bars change (`set`).
    static func place(_ parent: NodeView, _ ids: [UInt32], in presenter: Presenter) -> [UInt32] {
        guard let viewport = presenter.viewport as? Viewport else { return ids }
        if parent.scrollView != nil || viewport.childLists[parent.id] != nil { viewport.childLists[parent.id] = ids.isEmpty ? nil : ids }
        let on = viewport.scrollportBars
        let bars = on.isEmpty ? [] : ids.filter(on.contains)
        guard !bars.isEmpty || parent.scrollportHolder != nil else { return ids }
        for id in bars where presenter.flats.isFlat(id) { presenter.flats.promote(id) }
        mount(bars.compactMap { presenter.views[$0] }, keeping: Set(ids), in: parent, presenter)
        return bars.isEmpty ? ids : ids.filter { !on.contains($0) }
    }

    /// Keeps `parent`'s holder at its padding box, the bars' coordinates its border box's, and over its
    /// scroll view or collection, which a backend may just have mounted or a restyle moved.
    package static func raise(in parent: NodeView) {
        guard let holder = parent.scrollportHolder else { return }
        let uniform = parent.number("border_width")
        let border = UIEdgeInsets(top: parent.number("border_width_top", uniform), left: parent.number("border_width_left", uniform),
                                  bottom: parent.number("border_width_bottom", uniform), right: parent.number("border_width_right", uniform))
        let frame = parent.laidOutBox.inset(by: border)
        if holder.frame != frame { holder.frame = frame }
        if holder.bounds.origin != frame.origin { holder.bounds.origin = frame.origin }
        parent.bringSubviewToFront(holder)
    }

    private static func mount(_ bars: [NodeView], keeping ids: Set<UInt32>, in parent: NodeView, _ presenter: Presenter) {
        let holder = parent.scrollportHolder ?? {
            let made = ScrollportHolder(frame: parent.bounds)
            parent.addSubview(made)
            parent.scrollportHolder = made
            return made
        }()
        // A move between the holder and the container stays under the bars' authored parent: a press
        // held in a bar goes on (`NodeView.containerMoves`)
        GlassGroups.moving(in: parent) {
            // One no longer a bar but still a child moves into the container, where the caller places
            // it; one no longer a child goes, as the container's children do, unless it leaves with
            // its exit, is parked or is retained, which the holder keeps until it goes
            for old in holder.bars.compactMap(\.view) where !bars.contains(old) && old.superview === holder {
                if ids.contains(old.id) {
                    parent.container.addSubview(old)
                } else if !presenter.pool.isParked(old), !presenter.isLeaving(old), !presenter.modals.retainsRemovedView(old) {
                    old.removeFromSuperview()
                }
            }
            for (i, bar) in bars.enumerated() where bar.superview !== holder || holder.bars.firstIndex(where: { $0.view === bar }) != i {
                holder.insertSubview(bar, at: min(i, holder.subviews.count))
            }
        }
        holder.bars = bars.map(ScrollportHolder.Bar.init)
        if bars.isEmpty, holder.subviews.isEmpty {
            holder.removeFromSuperview()
            parent.scrollportHolder = nil
        } else {
            raise(in: parent)
        }
    }
}
#endif
