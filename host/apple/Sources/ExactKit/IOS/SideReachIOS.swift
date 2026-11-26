// LLP 1188 D2: until LLP 1123's activation gives a scroller the kernel's reach, a route's vertical scroller whose edge
// meets its route's side clearance reaches across that clearance, as a list UIKit draws does (`GroupedListView.sideReach`).
// A row nested in it that scrolls sideways reaches as far, and a picture that mirrors itself mirrors to the route's edge.
// Its scroll view then ends where a hand-built screen's does, and UIKit lays its vertical indicator out there. Its box,
// its content and its CSS scroll position stay as they were. Activation deletes this file and what reads `sideInset`.
#if os(iOS) || os(tvOS)
import UIKit

extension Presenter {
    /// How far `v`, a scroller in `route`, reaches past its left and right edges (D13): across a side that the route
    /// clears of UIKit's own safe area (`platformSides`) and that its edge meets, where its author's `-exact-safe-area-reach` allows it.
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
              let window = platformSides(of: route) else { return .zero }
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

    /// The side safe area UIKit itself gives `route`'s controller: the window's, and what UIKit's containers add (a split's
    /// sidebar over its detail, which a hand-built detail's scroll view runs under), not what Exact or an app adds to a
    /// controller's (`additionalSafeAreaInsets`, up its controllers), which UIKit keeps a scroll view's indicator inside.
    func platformSides(of route: NodeView) -> UIEdgeInsets? {
        guard let window = route.window else { return nil }
        var sides = UIEdgeInsets(top: 0, left: window.safeAreaInsets.left, bottom: 0, right: window.safeAreaInsets.right)
        guard let c = ViewPresentation.controller(for: route, layout: true), let view = c.viewIfLoaded else { return sides }
        var added = UIEdgeInsets.zero
        for p in sequence(first: c, next: { $0.parent }) {
            added.left += p.additionalSafeAreaInsets.left
            added.right += p.additionalSafeAreaInsets.right
        }
        sides.left = max(sides.left, view.safeAreaInsets.left - added.left)
        sides.right = max(sides.right, view.safeAreaInsets.right - added.right)
        return sides
    }

    /// How far `v`, a box that mirrors its picture (`-exact-safe-area-extend`), extends past its left and right edges
    /// while the kernel gives it no extension of its own: on each edge it names (`boxExtendEdges`) whose side its route
    /// clears of UIKit's own safe area (`platformSides`) and that its edge meets, to the route's edge, as UIKit's
    /// extension view mirrors into the safe area its view overlaps. Nothing between it and the route may cut that strip
    /// off; a scroller's scroll view clips at the reach it has or this pass gives it (`sideReach`).
    func sideExtension(of v: NodeView, in route: NodeView) -> UIEdgeInsets {
        #if os(tvOS)
        return .zero
        #else
        guard let named = v.props["boxExtendEdges"]?.split(separator: " ").map({ $0 == "1" }), named.count == 4,
              case .edges(let clear)? = navigation.covers[route.id], let window = platformSides(of: route) else { return .zero }
        let box = v.convert(v.bounds, to: route), off = RouteGeometry.clearanceOff(route)
        let side = { (on: Bool, bit: Int, clear: CGFloat, window: CGFloat, gap: CGFloat) -> CGFloat in
            on && off & bit == 0 && clear > 0 && abs(clear - window) < 0.5 && abs(gap - clear) < 0.5 ? clear : 0
        }
        var out = UIEdgeInsets(top: 0, left: side(named[3], 8, clear.left, window.left, box.minX),
                               bottom: 0, right: side(named[1], 2, clear.right, window.right, route.bounds.width - box.maxX))
        guard out != .zero else { return .zero }
        func passes(_ c: UIView, _ node: NodeView?) -> Bool {
            guard c.transform.isIdentity, c.layer.boxMask == nil, node?.clipPath == nil, node?.boxFilter == nil else { return false }
            guard c !== v, c !== route else { return true }
            if c.next is UIViewController { return false }
            guard c.clipsToBounds else { return true }
            var clip = c.convert(c.bounds, to: route)
            if let owner = c.superview as? NodeView, owner.scroll === c {
                let reach = sideReach(of: owner, in: route)
                clip = owner.convert(owner.bounds, to: route).inset(by: UIEdgeInsets(top: 0, left: -reach.left, bottom: 0, right: -reach.right))
            }
            if clip.minX > box.minX - out.left + 0.5 { out.left = 0 }
            if clip.maxX < box.maxX + out.right - 0.5 { out.right = 0 }
            return true
        }
        return Self.climb(from: v, to: route, passes) ? out : .zero
        #endif
    }

    /// How far `v`, a row that scrolls only sideways inside a route's vertical scroller, reaches past its left and right
    /// edges: as far as that scroller reaches (`sideReach`) on each side the row's edge meets the scroller's, so its
    /// items scroll under the side and its first and last rest clear of it, as a collection view nested in a vertical
    /// scroll view does (LLP 1123 C33). Its scroll view takes the reach as its content inset (`setSideReach`), as the
    /// vertical scroller does, since the vertical one's inset leaves it none of UIKit's safe area there. Nothing between
    /// the two may turn, mask, border or clip that strip off.
    func nestedSideReach(of v: NodeView, in route: NodeView) -> UIEdgeInsets {
        #if os(tvOS)
        return .zero
        #else
        guard let scroll = v.scroll, scroll.scrollsX, !scroll.scrollsY, v.zoomRange == nil,
              let outer = sequence(first: v.superview, next: { $0?.superview }).lazy.compactMap({ $0 as? NodeView }).first(where: { $0.scroll != nil }),
              outer !== route, outer.isDescendant(of: route) else { return .zero }
        let around = sideReach(of: outer, in: route)
        guard around != .zero, !v.cornerSizes(in: v.bounds).contains(where: { $0.width > 0 }) else { return .zero }
        let box = v.convert(v.bounds, to: route), port = outer.convert(outer.bounds, to: route)
        var reach = UIEdgeInsets(top: 0, left: abs(box.minX - port.minX) < 0.5 && v.mayReach(.left) ? around.left : 0,
                                 bottom: 0, right: abs(box.maxX - port.maxX) < 0.5 && v.mayReach(.right) ? around.right : 0)
        func passes(_ c: UIView, _ node: NodeView?) -> Bool {
            guard c.transform.isIdentity, c.layer.boxMask == nil, node?.clipPath == nil, node?.boxFilter == nil,
                  node?.materialView == nil else { return false }
            let border = { (side: String) in node.map { $0.number("border_width_" + side, $0.number("border_width")) > 0 } ?? false }
            if border("left") { reach.left = 0 }
            if border("right") { reach.right = 0 }
            guard c !== v, c !== outer else { return true }
            if c is UIScrollView && c !== outer.scroll || c.next is UIViewController { return false }
            // The scroller's own scroll view clips at the reach just found, whichever of the two this pass reaches first
            if c.clipsToBounds, c !== outer.scroll {
                let clip = c.convert(c.bounds, to: route)
                if clip.minX > box.minX - reach.left + 0.5 { reach.left = 0 }
                if clip.maxX < box.maxX + reach.right - 0.5 { reach.right = 0 }
            }
            return true
        }
        return reach != .zero && Self.climb(from: v, to: outer, passes) ? reach : .zero
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
