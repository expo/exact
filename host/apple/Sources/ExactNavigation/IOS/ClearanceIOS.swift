// @ref LLP 1123 D3, D4 — a route's clearance on the edges its scroller takes:
// UIKit insets the scroller by what of the safe area overlaps it, which
// already leaves out the route's padding above it, as if that padding cleared
// the bar. Clearance adds to padding unless the padding reads the inset
// itself, so the scroller's own `contentInset` makes up the rest: the
// route's padding's points where it does not read the inset, up to the
// inset, and nothing for boxes laid out above the scroller. An edge
// the route opts out of (`-exact-safe-area`) is not inset at all: UIKit's
// inset there is taken back by `RouteSafeArea` (SafeAreaIOS.swift).
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// One scroller's part of its `contentInset` that clearance set, top and
/// bottom, so a later report moves only that part.
struct ClearanceInset {
    weak var scroll: UIScrollView?
    var top: CGFloat = 0
    var bottom: CGFloat = 0
}

extension NavigationHost {
    /// What clearance adds to the scroller at each edge of `c` it takes
    /// (`collapseScroll` at the top, `edgeScroll` at the bottom) beyond what
    /// UIKit insets, and what it takes back on an edge the route opts out of.
    func clearanceExtra(_ c: RouteController, scroller node: NodeView, top: Bool) -> CGFloat {
        guard let sv = node.scrollView, c.surface.window != nil else { return 0 }
        let off = RouteGeometry.clearanceOff(c.node), credit = Int(c.node.style["inset_credit"]?.number ?? 0)
        let local = c.surface.safeAreaInsets, box = node.convert(node.bounds, to: c.node)
        if top {
            guard off & 1 == 0, credit & 1 == 0, !RouteGeometry.transparentBar(c.node) else { return 0 }
            // The box is laid out under the top cover last reported, which the
            // kernel pads: a scroller taken under the bar in this report (a
            // reason above it gone) still sits below the cover it gives up,
            // and that room is UIKit's inset, not padding.
            var padded: CGFloat = 0
            if case .edges(let e)? = covers[c.node.id] { padded = e.top }
            // Only the route's own padding above the cover is made up: a box
            // between the cover and the scroller (a control bar) is content,
            // which UIKit's inset never counts either
            let own = max(0, (c.node.kernelPadding?.first ?? nil).map { $0 - padded } ?? 0)
            return min(local.top, max(0, min(box.minY - padded, own)))
        }
        // An edge the route turns off is `RouteSafeArea`'s, which takes UIKit's inset back there
        if off & 4 != 0 { return 0 }
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
