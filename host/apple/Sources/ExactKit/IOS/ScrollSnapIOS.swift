// CSS scroll snap on UIKit (LLP 1008 §9, "Scroll snap"): UIKit projects
// where a drag comes to rest and owns the deceleration there; the snap
// position the rule picks (`ScrollSnap.swift`) replaces the projection.
#if os(iOS) || os(tvOS)
import UIKit

extension NodeView {
    package func scrollViewWillEndDragging(_ scrollView: UIScrollView, withVelocity velocity: CGPoint, targetContentOffset: UnsafeMutablePointer<CGPoint>) {
        guard scrollView === self.scrollView else { return }
        // `scrolldragend` (Charlie, 2026-11-10): UIKit's velocity, points per
        // millisecond and positive as the offset grows, as the web's
        // scrollLeft/scrollTop, and the rest decided below, in those coordinates.
        defer {
            if handlers.contains("scrolldragend"), velocity.x.isFinite, velocity.y.isFinite {
                let rest = targetContentOffset.pointee
                presenter?.scrollDrag(id, (velocity, CGPoint(x: rest.x, y: rest.y + scrollTopInset(scrollView))))
            }
        }
        #if os(iOS)
        // A painted swipe row's full swipe decides where it rests (PaintedSwipesIOS.swift).
        defer { if swipeOwner { presenter?.swipeActions.painted.willEnd(self, scrollView, target: targetContentOffset) } }
        #endif
        targetContentOffset.pointee = snappedOffset(targetContentOffset.pointee, in: scrollView, release: scrollView.contentOffset,
                                                    drag: (dragStart(scrollView), velocity))
    }

    /// Where the drag that is ending began: its pan's translation, which
    /// moved the offset the other way, from where the offset is.
    private func dragStart(_ scrollView: UIScrollView) -> CGPoint {
        let moved = scrollView.panGestureRecognizer.translation(in: scrollView)
        return CGPoint(x: scrollView.contentOffset.x + moved.x, y: scrollView.contentOffset.y + moved.y)
    }

    /// UIKit's paging (`isPagingEnabled`, SwiftUI's `.paging`) for a scroller
    /// whose snap it is (LLP 1008 §9, as amended for #219): one mandatory
    /// axis, a snapport that is the whole scrollport (no inset, border or
    /// `scroll-padding`), every snap position (alignment and `scroll-margin`
    /// applied) at a multiple of the snapport, every area
    /// `scroll-snap-stop: always`, and a position at every multiple UIKit
    /// pages to, from the start to the content's end: UIKit rests only at
    /// those, so a missing page would be a rest with no area (a virtualized
    /// list's unmounted pages are missing, and it snaps by the rule). UIKit
    /// then moves one page a flick and settles as its pagers do. Set as a
    /// drag begins.
    func pagesByViewport(_ scrollView: UIScrollView) -> Bool {
        guard let kind = ScrollSnap.Kind(style["scroll_snap_type"]?.string), kind.mandatory, kind.x != kind.y,
              scrollView.adjustedContentInset == .zero else { return false }
        let vertical = kind.y, port = vertical ? scrollView.bounds.height : scrollView.bounds.width
        let sides = vertical ? ["top", "bottom"] : ["left", "right"]
        guard port > 0, sides.allSatisfy({ number("border_width_\($0)", number("border_width")) == 0 && (style["scroll_padding_\($0)"]?.length(basis: port) ?? 0) == 0 }) else { return false }
        let content = vertical ? scrollView.contentSize.height : scrollView.contentSize.width
        let count = Int((content / port).rounded())
        guard count > 0, abs(content - CGFloat(count) * port) < 0.5 else { return false }
        var pageAt = Set<Int>()
        for area in snapAreas(vertical, in: scrollView, port: port, range: 0...CGFloat(count - 1) * port).areas {
            let page = (area.from / port).rounded()
            guard area.stop, area.to - area.from < 0.5, abs(area.from - page * port) < 0.5 else { return false }
            pageAt.insert(Int(page))
        }
        return pageAt == Set(0..<count)
    }

    /// As a drag begins: UIKit's paging on while `pagesByViewport` holds,
    /// and off once it no longer does where it was on. A scroller with no
    /// snap is left as a hatch may have set it.
    func pageIfPager(_ scrollView: UIScrollView) {
        #if !os(tvOS) // tvOS's scroll views have no paging: focus moves them
        guard let sv = scrollView as? ScrollView, sv === self.scrollView else { return }
        let pages = pagesByViewport(sv)
        if pages || sv.snapPages { sv.isPagingEnabled = pages; sv.snapPages = pages }
        #endif
    }

    /// A paging drag ended: UIKit sends no `scrollViewWillEndDragging` then,
    /// so its `scrolldragend` is sent here, with the page the rule picks.
    func pagedDragEnded(_ scrollView: UIScrollView) {
        #if os(tvOS)
        return
        #else
        guard scrollView === self.scrollView, scrollView.isPagingEnabled else { return }
        let pan = scrollView.panGestureRecognizer.velocity(in: scrollView)
        let velocity = CGPoint(x: -pan.x / 1000, y: -pan.y / 1000), reach = 0.99 / (1 - 0.99), at = scrollView.contentOffset
        let rest = snappedOffset(CGPoint(x: at.x + velocity.x * reach, y: at.y + velocity.y * reach), in: scrollView, release: at,
                                 drag: (dragStart(scrollView), velocity))
        // The rule's rest is recorded either way, for a re-snap after layout (`resnap`).
        guard handlers.contains("scrolldragend") else { return }
        presenter?.scrollDrag(id, (velocity, CGPoint(x: rest.x, y: rest.y + scrollTopInset(scrollView))))
        #endif
    }

    /// UIKit's deceleration for a scroller: its fast rate where a snap is
    /// mandatory, as a paging scroller's, else its normal one.
    var snapDeceleration: UIScrollView.DecelerationRate {
        ScrollSnap.Kind(style["scroll_snap_type"]?.string)?.mandatory == true ? .fast : .normal
    }

    /// Where a scroll projected to rest at `proposed` rests, on each axis
    /// `scroll-snap-type` names: optionally only at the snap areas holding a
    /// target the agent reveals, and, released at `release`, not past an
    /// `always` area's position; a `drag` that began at `start` and lifted
    /// at `velocity` (points per millisecond) flicks on past where it began
    /// (`ScrollSnap.rest`).
    func snappedOffset(_ proposed: CGPoint, in scrollView: UIScrollView, revealing target: UIView? = nil, release: CGPoint? = nil,
                       drag: (start: CGPoint, velocity: CGPoint)? = nil) -> CGPoint {
        snapTarget(proposed, in: scrollView, revealing: target, release: release, drag: drag) ?? proposed
    }

    /// `snappedOffset`'s, or `nil` when no axis snaps: no snap type, no snap
    /// area, or a `proximity` snap too far from every area.
    func snapTarget(_ proposed: CGPoint, in scrollView: UIScrollView, revealing target: UIView? = nil, release: CGPoint? = nil,
                    drag: (start: CGPoint, velocity: CGPoint)? = nil, keeping: Bool = false) -> CGPoint? {
        guard let kind = ScrollSnap.Kind(style["scroll_snap_type"]?.string) else { return nil }
        var result = proposed, snapped = false
        let x = drag.map { ($0.start.x, ScrollSnap.way($0.velocity.x)) }, y = drag.map { ($0.start.y, ScrollSnap.way($0.velocity.y)) }
        if kind.x, let r = rest(false, kind, proposed.x, in: scrollView, revealing: target, release: release?.x ?? proposed.x, flick: x, keeping: keeping) {
            result.x = r; snapped = true
        }
        if kind.y, let r = rest(true, kind, proposed.y, in: scrollView, revealing: target, release: release?.y ?? proposed.y, flick: y, keeping: keeping) {
            result.y = r; snapped = true
        }
        return snapped ? result : nil
    }

    /// CSS Scroll Snap 1 §5.5: a mandatory scroller at rest on a snap area
    /// follows it when a batch moves it, and when it went, rests on the
    /// position nearest (`ScrollSnap.resnap`).
    func resnap() {
        guard let sv = scrollView, !sv.isTracking, !sv.isDecelerating, ScrollSnap.Kind(style["scroll_snap_type"]?.string)?.mandatory == true,
              let to = snapTarget(sv.contentOffset, in: sv, keeping: true).map({ PixelGrid.offset($0, in: sv) }),
              abs(to.x - sv.contentOffset.x) > 0.5 || abs(to.y - sv.contentOffset.y) > 0.5 else { return }
        sv.contentOffset = to
    }

    private func rest(_ vertical: Bool, _ kind: ScrollSnap.Kind, _ proposed: CGFloat, in scrollView: UIScrollView, revealing target: UIView?, release: CGFloat,
                      flick: (start: CGFloat, way: CGFloat)?, keeping: Bool) -> CGFloat? {
        // The snapport is the padding box less UIKit's insets and
        // `scroll-padding` (§4.1); positions are where its start is in the
        // content, which starts at the border box: an offset plus `shift`.
        let size = vertical ? scrollView.bounds.height : scrollView.bounds.width
        let inset = scrollView.adjustedContentInset
        let padding = { (side: String) in self.style["scroll_padding_\(side)"]?.length(basis: size) ?? 0 }
        let border = { (side: String) in self.number("border_width_\(side)", self.number("border_width")) }
        let (first, last) = vertical ? ("top", "bottom") : ("left", "right")
        let lead = (vertical ? inset.top : inset.left), trail = (vertical ? inset.bottom : inset.right)
        let shift = lead + border(first) + padding(first)
        let port = size - lead - trail - border(first) - border(last) - padding(first) - padding(last)
        guard port > 0 else { return nil }
        let content = vertical ? scrollView.contentSize.height : scrollView.contentSize.width
        let range = (-lead + shift)...max(-lead, content + trail - size) + shift
        let found = snapAreas(vertical, in: scrollView, port: port, range: range, holding: target), held = found.held
        var areas = found.areas
        let wanted = proposed + shift
        // A virtualized list's rows past its mounted ones (LLP 1008 §9, as amended for #219).
        let distance = areas.map { abs(min($0.to, max($0.from, wanted)) - wanted) }.min() ?? .infinity
        if distance > 0, held.isEmpty, let entry = presenter?.collections.entries[id], entry.snapshot.horizontal != vertical,
           let position = presenter?.session?.runtime.collectionSnap(id, proposed: Double(wanted), port: Double(port),
                minimum: Double(range.lowerBound), maximum: Double(range.upperBound), distance: Double(distance)) {
            areas.append(ScrollSnap.Area(from: CGFloat(position), to: CGFloat(position), stop: false))
        }
        let axis = vertical ? 1 : 0, sv = scrollView as? ScrollView
        func pick() -> CGFloat? {
            keeping
                ? ScrollSnap.resnap(areas, rested: sv?.snapRested[axis] ?? .init(), current: wanted, mandatory: kind.mandatory, port: port)
                : ScrollSnap.rest(held.isEmpty ? areas : held, projected: wanted, release: release + shift, start: flick.map { $0.start + shift },
                                  way: flick?.way ?? 0, mandatory: kind.mandatory || !held.isEmpty, port: port)
        }
        guard var rest = pick() else { return nil }
        // An unmounted `always` row between the release and the rest holds it too.
        if held.isEmpty, !keeping, abs(rest - release - shift) > 0.5, presenter?.collections.entries[id]?.snapshot.horizontal == !vertical,
           let stop = presenter?.session?.runtime.collectionSnapStop(id, release: Double(release + shift), rest: Double(rest), port: Double(port),
                                                                     minimum: Double(range.lowerBound), maximum: Double(range.upperBound)) {
            areas.append(ScrollSnap.Area(from: CGFloat(stop), to: CGFloat(stop), stop: true))
            rest = pick() ?? rest
        }
        sv?.snapRested[axis] = ScrollSnap.Rested(rest, in: areas)
        return rest - shift
    }

    /// The snap areas along one axis in `scrollView`'s content, their
    /// positions where the snapport's start rests, and those of them holding
    /// `target`.
    private func snapAreas(_ vertical: Bool, in scrollView: UIScrollView, port: CGFloat, range: ClosedRange<CGFloat>,
                           holding target: UIView? = nil) -> (areas: [ScrollSnap.Area], held: [ScrollSnap.Area]) {
        var areas: [ScrollSnap.Area] = [], held: [ScrollSnap.Area] = []
        func visit(_ view: UIView) {
            for case let node as NodeView in view.subviews where !node.isHidden {
                if let align = ScrollSnap.align(node.style["scroll_snap_align"]?.string, vertical: vertical) {
                    // The area is the border box grown by `scroll-margin` (§4.3).
                    let rect = IntoView.inset(node.convert(node.bounds, to: scrollView), style: node.style, family: "scroll_margin", outward: true)
                    let area = ScrollSnap.area(lo: vertical ? rect.minY : rect.minX, hi: vertical ? rect.maxY : rect.maxX, align: align,
                                               stop: node.style["scroll_snap_stop"]?.string == "always", port: port, range: range, id: node.id)
                    areas.append(area)
                    if let target, target === node || target.isDescendant(of: node) { held.append(area) }
                }
                // A nested scroll container captures its own snap areas, `hidden`
                // included without a UIScrollView; `clip` does not (CSS Overflow 3 §3.1).
                if node.scrollView == nil && !ScrollSnap.ownsAreas(node.style["overflow_x"]?.string, node.style["overflow_y"]?.string) { visit(node.container) }
            }
        }
        visit(scrollView)
        return (areas, held)
    }
}
#endif
