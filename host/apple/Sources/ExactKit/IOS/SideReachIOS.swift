// LLP 1188 D2: until LLP 1123's activation gives a scroller the kernel's reach, a route's vertical scroller whose edge
// meets its route's side clearance reaches across that clearance, as a list UIKit draws does (`GroupedListView.sideReach`).
// Its scroll view then ends where a hand-built screen's does, and UIKit lays its vertical indicator out there. Its box,
// its content and its CSS scroll position stay as they were. Activation deletes this file and what reads `sideInset`.
#if os(iOS) || os(tvOS)
import UIKit

extension Presenter {
    /// How far `v`, a scroller in `route`, reaches past its left and right edges (D13): across a side that the route
    /// clears of the window's own safe area (not of what Exact or an app adds to a controller's, which UIKit keeps a
    /// scroll view's indicator inside) and that its edge meets, where its author's `-exact-safe-area-reach` allows it.
    /// None where its content, or something it is drawn through, ends at that edge itself or would cut the strip off.
    /// A pane of the route is another owner, whose scrollers do not reach. tvOS's side clearance is overscan, not a bar's.
    func sideReach(of v: NodeView, in route: NodeView) -> UIEdgeInsets {
        #if os(tvOS)
        return .zero
        #else
        guard let scroll = v.scroll, scroll.scrollsY, v.scrollHoldsContent, v.zoomRange == nil,
              // One that pans sideways keeps its port, and one whose author cuts wider content off keeps that edge
              max(scroll.contentSize.width, v.content.width) <= v.scrollFloorWidth(scroll) + 0.5,
              collections.snapshot(v.id)?.horizontal != true,
              case .edges(let clear)? = navigation.covers[route.id], clear.left > 0 || clear.right > 0,
              let window = v.window?.safeAreaInsets else { return .zero }
        let box = v.convert(v.bounds, to: route), off = RouteGeometry.clearanceOff(route)
        let side = { (edge: UIRectEdge, bit: Int, clear: CGFloat, window: CGFloat, gap: CGFloat) -> CGFloat in
            off & bit == 0 && clear > 0 && abs(clear - window) < 0.5 && abs(gap - clear) < 0.5 && v.mayReach(edge) ? clear : 0
        }
        var reach = UIEdgeInsets(top: 0, left: side(.left, 8, clear.left, window.left, box.minX),
                                 bottom: 0, right: side(.right, 2, clear.right, window.right, route.bounds.width - box.maxX))
        guard reach != .zero, !v.cornerSizes(in: v.bounds).contains(where: { $0.width > 0 }) else { return .zero }
        func passes(_ c: UIView, _ node: NodeView?) -> Bool {
            guard c.transform.isIdentity, c.layer.boxMask == nil, node?.clipPath == nil, node?.boxFilter == nil else { return false }
            let border = { (side: String) in node.map { $0.number("border_width_" + side, $0.number("border_width")) > 0 } ?? false }
            if border("left") { reach.left = 0 }
            if border("right") { reach.right = 0 }
            guard c !== route else { return true }
            guard node?.materialView == nil else { return false }
            guard c !== v else { return true }
            if c is UIScrollView || c is UIVisualEffectView || c.next is UIViewController { return false }
            if c.clipsToBounds {
                let clip = c.convert(c.bounds, to: route)
                if clip.minX > box.minX - reach.left + 0.5 { reach.left = 0 }
                if clip.maxX < box.maxX + reach.right - 0.5 { reach.right = 0 }
            }
            return true
        }
        return Self.climb(from: v, to: route, passes) && passes(route, route) ? reach : .zero
        #endif
    }
}

extension NodeView {
    /// Whether its author's `-exact-safe-area-reach` lets it reach past `edge` (D13): `auto`, unsaid, does.
    fileprivate func mayReach(_ edge: UIRectEdge) -> Bool {
        style["safe_area_reach"]?.string != "edges" || edgeName("safe_area_reach_edges", edge) != nil
    }

    /// What its scroll view's own `contentInset` holds of its reach: all of one it has before the kernel gives it any.
    package var sideInset: UIEdgeInsets { reachGiven ? .zero : reach }

    /// The side reach the host gives it meanwhile (`Presenter.sideReach`).
    func setSideReach(_ sides: UIEdgeInsets) {
        guard !reachGiven, sides != reach else { return }
        moveOrPlace { reachScroll(sides) }
    }
}
#endif
