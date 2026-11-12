// CSS scroll snap on UIKit (LLP 1008 §9): UIKit
// projects where a drag comes to rest; the nearest captured snap position
// replaces it.
#if os(iOS) || os(tvOS)
import UIKit

extension NodeView {
    /// CSS's mandatory snap on either axis, with start, center and end alignment. UIKit supplies
    /// the projected resting offset and owns the resulting deceleration.
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
        targetContentOffset.pointee = snappedOffset(targetContentOffset.pointee, in: scrollView)
    }

    /// The native resting position, optionally constrained to the snap area
    /// holding a target the agent reveals.
    func snappedOffset(_ proposed: CGPoint, in scrollView: UIScrollView, revealing target: UIView? = nil) -> CGPoint {
        snapTarget(proposed, in: scrollView, revealing: target) ?? proposed
    }

    /// Nil when there is no snap area, including when a mandatory scroller is empty.
    func snapTarget(_ proposed: CGPoint, in scrollView: UIScrollView, revealing target: UIView? = nil) -> CGPoint? {
        let type = style["scroll_snap_type"]?.string
        guard type == "x mandatory" || type == "y mandatory" else { return nil }
        let vertical = type == "y mandatory"
        let inset = scrollView.adjustedContentInset
        let leading = vertical ? inset.top : inset.left
        let trailing = vertical ? inset.bottom : inset.right
        let before = number(vertical ? "border_width_top" : "border_width_left", number("border_width"))
        let after = number(vertical ? "border_width_bottom" : "border_width_right", number("border_width"))
        let viewport = (vertical ? scrollView.bounds.height : scrollView.bounds.width) - leading - trailing
        let port = viewport - before - after
        guard port > 0 else { return nil }
        let maximum = max(0, (vertical ? scrollView.contentSize.height : scrollView.contentSize.width) - viewport)
        let wanted = (vertical ? proposed.y : proposed.x) + leading
        var positions: [CGFloat] = [], targetPositions: [CGFloat] = []
        func visit(_ view: UIView) {
            for case let node as NodeView in view.subviews where !node.isHidden {
                let align = node.style["scroll_snap_align"]?.string
                if align == "start" || align == "end" || align == "center" {
                    let rect = node.convert(node.bounds, to: scrollView)
                    let (lo, hi, size) = vertical ? (rect.minY - before, rect.maxY - before, rect.height) : (rect.minX - before, rect.maxX - before, rect.width)
                    // Oversized areas stay free while they cover the snapport (CSS Snap §5.2.2).
                    let wide = size >= port
                    let at = wide ? lo : align == "end" ? hi - port : align == "center" ? (lo + hi - port) / 2 : lo
                    let start = min(maximum, max(0, at))
                    let end = wide ? min(maximum, max(start, hi - port)) : start
                    let position = min(end, max(start, wanted))
                    positions.append(position)
                    if let target, target === node || target.isDescendant(of: node) { targetPositions.append(position) }
                }
                // Hidden owns snap areas even without a UIScrollView; clip does not (CSS Overflow §3.1).
                let ownsAreas = ["overflow_x", "overflow_y"].contains { ["auto", "scroll", "hidden"].contains(node.style[$0]?.string ?? "visible") }
                if node.scrollView == nil && !ownsAreas { visit(node.container) }
            }
        }
        visit(scrollView)
        func nearer(_ a: CGFloat, _ b: CGFloat) -> Bool {
            let da = abs(a - wanted), db = abs(b - wanted)
            return da < db || (da == db && a < b)
        }
        let mounted = positions.min(by: nearer)
        let distance = mounted.map { abs($0 - wanted) } ?? .infinity
        // The row query uses border-box coordinates; mounted candidates use the padding edge.
        if distance > 0, targetPositions.isEmpty,
           presenter?.collections.entries[id]?.snapshot.horizontal == !vertical,
           let position = presenter?.session?.runtime.collectionSnap(id, proposed: Double(wanted + before),
                port: Double(port), maximum: Double(maximum + before), distance: Double(distance)) {
            positions.append(min(maximum, max(0, CGFloat(position) - before)))
        }
        let choices = targetPositions.isEmpty ? positions : targetPositions
        guard let nearest = choices.min(by: nearer) else { return nil }
        var result = proposed
        if vertical { result.y = nearest - leading } else { result.x = nearest - leading }
        return result
    }
}
#endif
