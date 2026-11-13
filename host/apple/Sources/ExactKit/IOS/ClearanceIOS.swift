// @ref LLP 1123 D3, D4 — a route's clearance on the edges its scroller takes:
// UIKit insets the scroller by what of the safe area overlaps it, which
// already leaves out the route's padding above it, as if that padding cleared
// the bar. Clearance adds to padding unless the padding reads the inset
// itself, so the scroller's own `contentInset` makes up the rest: the
// padding's points where it does not read the inset, up to the inset. An edge
// the route opts out of (`-exact-safe-area`) is not inset at all: the top as
// under a transparent bar (`insetsNoTop`), the bottom by taking back what
// UIKit adds there.
#if os(iOS) || os(tvOS)
import UIKit

/// One scroller's part of its `contentInset` that clearance set, top and
/// bottom, so a later report moves only that part.
struct ClearanceInset {
    weak var scroll: UIScrollView?
    var top: CGFloat = 0
    var bottom: CGFloat = 0
}

extension NavigationHost {
    /// The edges, as bits top 1, right 2, bottom 4 and left 8, whose
    /// clearance `route` turns off (`-exact-safe-area`).
    static func clearanceOff(_ route: NodeView) -> Int { Int(route.style["safe_area"]?.number ?? 0) & 0xf }

    /// Whether UIKit insets nothing of `route`'s top scroller: under a
    /// transparent bar, or where the route opts out of its top clearance.
    static func insetsNoTop(_ route: NodeView) -> Bool { transparentBar(route) || clearanceOff(route) & 1 != 0 }

    /// What clearance adds to the scroller at each edge of `c` it takes
    /// (`collapseScroll` at the top, `edgeScroll` at the bottom) beyond what
    /// UIKit insets, and what it takes back on an edge the route opts out of.
    func clearanceExtra(_ c: RouteController, scroller node: NodeView, top: Bool) -> CGFloat {
        guard let sv = node.scrollView, c.viewIfLoaded?.window != nil else { return 0 }
        let off = Self.clearanceOff(c.node), credit = Int(c.node.style["inset_credit"]?.number ?? 0)
        let local = c.view.safeAreaInsets, box = node.convert(node.bounds, to: c.node)
        if top {
            guard off & 1 == 0, credit & 1 == 0, !Self.transparentBar(c.node) else { return 0 }
            return min(local.top, max(0, box.minY))
        }
        if off & 4 != 0 { return -sv.safeAreaInsets.bottom }
        guard credit & 4 == 0 else { return 0 }
        return min(local.bottom, max(0, c.node.bounds.maxY - box.maxY))
    }

    /// Set each route scroller's clearance part of its `contentInset` to
    /// `extras` (by scroller), and give back what a scroller no longer
    /// taking an edge had. A scroller at its start stays there.
    func applyClearance(_ extras: [ObjectIdentifier: (UIScrollView, top: CGFloat, bottom: CGFloat)]) {
        for (id, held) in clearance where extras[id] == nil {
            if let sv = held.scroll { move(sv, top: -held.top, bottom: -held.bottom) }
            clearance[id] = nil
        }
        for (id, want) in extras {
            let held = clearance[id] ?? ClearanceInset(scroll: want.0)
            let (top, bottom) = (want.top - held.top, want.bottom - held.bottom)
            guard abs(top) > 0.25 || abs(bottom) > 0.25 else { continue }
            move(want.0, top: top, bottom: bottom)
            clearance[id] = ClearanceInset(scroll: want.0, top: want.top, bottom: want.bottom)
        }
    }

    private func move(_ sv: UIScrollView, top: CGFloat, bottom: CGFloat) {
        let atStart = sv.contentOffset.y <= 0.5 - sv.adjustedContentInset.top
        sv.contentInset.top += top
        sv.contentInset.bottom += bottom
        sv.verticalScrollIndicatorInsets.top += top
        sv.verticalScrollIndicatorInsets.bottom += bottom
        if atStart { sv.contentOffset = PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: -sv.adjustedContentInset.top), in: sv) }
    }
}
#endif
