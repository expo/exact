#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

extension NavigationHost {
    /// The route's last in-flow scroller can reach under the tab bar and home indicator
    /// (LLP 1116 D2). Its bottom inset takes the route's cover so in-flow content clears
    /// the bar once. The cover still places an absolute footer above the bar.
    func reachBottom(_ c: RouteController, in stack: UIViewController, presented: Bool) {
        let target = bottomEdgeScroller(of: c, in: stack, presented: presented)
        let projects = !c.nativeContentScrollEdges.contains(.bottom)
        // Collapse has already chosen the adjustment for a shared scroll view. A bottom-only scroller takes UIKit's
        // adjustment, including when it just gave up the top, unless the route has no top to inset (`insetsNoTop`):
        // UIKit's adjustment would inset that top too, and the bottom's clearance comes unadjusted. Either way it
        // follows the route as it changes, while the scroller stays mounted.
        let shown = shownScroll(target)
        let arranged = shown.map { arrangementInsets($0, in: c) } ?? false
        let wanted: UIScrollView.ContentInsetAdjustmentBehavior = RouteGeometry.insetsNoTop(c.node) && !arranged ? .never : .automatic
        let needsAdjustment = shown != nil && target !== c.collapseScroll && shown?.contentInsetAdjustmentBehavior != wanted
        guard needsAdjustment || c.edgeScroll !== target || (projects && c.contentScrollView(for: .bottom) !== c.automaticBottomScroll) else { return }
        if let old = c.edgeScroll, old !== target, let sv = old.scrollView, old !== c.collapseScroll, !arrangementInsets(sv, in: c) { sv.contentInsetAdjustmentBehavior = .never }
        if projects, let previous = c.contentScrollView(for: .bottom), previous !== target?.scrollView,
           previous !== c.collapseScrollView, !arrangementInsets(previous, in: c) { previous.contentInsetAdjustmentBehavior = .never }
        c.edgeScroll = target
        // A scroller at its start stays there as its adjustment changes, either
        // way; one scrolled keeps its offset, its content where it was. One
        // made in this batch can hold a safe area from before its first frame
        // (at the route's top, under the bar): UIKit insets it by that first,
        // then, given it below, takes the inset away and moves the offset by
        // as much, scrolling its content out of its port.
        if needsAdjustment, let sv = shown {
            let atStart = abs(sv.contentOffset.y + sv.adjustedContentInset.top) <= 0.5
            sv.contentInsetAdjustmentBehavior = wanted
            if atStart, sv.contentOffset.y != -sv.adjustedContentInset.top {
                sv.contentOffset = PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: -sv.adjustedContentInset.top), in: sv)
            }
        }
        // tvOS has no bottom bar whose edge follows a scroller.
        #if !os(tvOS)
        c.projectContentScrollView(c.automaticBottomScroll, for: .bottom)
        #endif
        presenter.session?.log("navigation: route \(c.key) \(target == nil ? "keeps its content above the bottom edge" : "scrolls its content to the bottom edge")")
    }

    /// The scroller that takes the route's bottom edge, as a hand-built screen's scroll view spans the home indicator
    /// with UIKit's inset, whatever the document's `viewport-fit` (LLP 1123 D3, D13); a launch's first route pass runs
    /// before the document reports its fit. On a cover page, a route that ends above its tab bar takes none.
    private func bottomEdgeScroller(of c: RouteController, in stack: UIViewController, presented: Bool) -> NodeView? {
        let cover = !presented && presenter.viewportFit == "cover", tabBar = bottomTabBar(c, in: stack)
        guard RouteGeometry.clearanceOff(c.node) & 4 == 0, !cover || tabBar != .endsAbove else { return nil }
        let peers = arrangementScrollers(in: c, top: false)
        let peer = contentScroller(of: c).flatMap { node in peers.contains(where: { $0 === node }) ? node : nil } ?? peers.first
        let padding = RouteGeometry.clearancePadding(c.node, edge: 2, inset: c.bottomClearance)
        guard let found = Self.bottomScroller(in: c.node, cover: padding, lifting: c.liftedBottomRows, scale: presenter.gridScale) ?? peer else { return nil }
        // A separate scroller with an open foot draws through a cover page's tab bar with a band
        // (LLP 1131 D2). Inside a clipping wrapper the wrapper must grow with it.
        if #available(iOS 26.0, tvOS 26.0, *), cover, tabBar == .reached, found !== c.collapseScroll,
           found.edgeEffectHidden(.bottom) || ControlBars(presenter).bars(of: found, in: c.node, top: false)?.isEmpty == true { return nil }
        return found
    }

    enum BottomTabBar { case none, reached, endsAbove }

    /// The tab bar at the foot of the route's stack, and whether the route reaches its band. An iPad's tabs at the top
    /// or in a sidebar leave the foot to the screen, as a route that hides the bar does.
    func bottomTabBar(_ c: RouteController, in stack: UIViewController) -> BottomTabBar {
        guard let tabs = stack.tabBarController, let container = tabs.viewIfLoaded else { return .none }
        if #available(iOS 18.0, tvOS 18.0, *), tabs.isTabBarHidden { return .none }
        guard let bar = RouteGeometry.shownBar(tabs.tabBar), bar.bounds.width > bar.bounds.height,
              let holder = stack.viewIfLoaded, holder.window === bar.window else { return .none }
        // Frames that agree within half a device pixel agree, as the kernel's observations judge them (LLP 1123 D9)
        let held = bar.convert(bar.bounds, to: container), half = 0.5 / max(presenter.gridScale, 1)
        guard abs(held.maxY - container.bounds.maxY) <= half else { return .none }
        // UIKit takes a pushed route's edges before attaching its view. Its authored
        // frame is already in the destination's coordinates.
        let attached = (c.bootSurface ?? c.viewIfLoaded).flatMap { $0.window === bar.window ? $0 : nil }
        guard attached != nil || !Self.hidesBar(c) else { return .none }
        let route = attached != nil ? c.node.convert(c.node.bounds, to: container) : holder.convert(c.node.frame, to: container)
        return route.maxY > held.minY + half ? .reached : .endsAbove
    }

    /// The scroller at a route's bottom edge: its last in-flow child, or
    /// that child's last, down through boxes that alone span their parent's
    /// bottom edge (a sheet's component whose root column holds its header
    /// and then its `scroll`), as a hand-built screen's scroll view pinned to
    /// its view's bottom is at the edge however deep it sits. A box that ends
    /// short of its parent's bottom (a padding, a border or a margin under
    /// it), or has a sibling beside it in a row or a grid, keeps the route's
    /// cover. One that scrolls vertically only: UIKit's bottom inset would
    /// let a sideways carousel scroll up and down. `cover` is the route's
    /// bottom cover, which the route's child may end above.
    static func bottomScroller(in route: NodeView, cover: CGFloat = 0, lifting: [NodeView] = [], scale: CGFloat) -> NodeView? {
        let flowing = RouteGeometry.inFlow
        // The last in-flow child spans the bottom edge alone when every other
        // ends above its top, as children stacked down a block or a column
        // do. Beside it in a row or a grid, a sibling reaches the edge too
        // (a column with a Save button at its foot) and keeps the cover. The
        // laid-out frames say so, not one a transform moves (a control bar
        // riding the navigation bar); the host is not sent the flex direction.
        // Frames that agree within half a device pixel of the page's display
        // (`scale`) agree, as the kernel's observations judge them (LLP 1123
        // D9); a route being pushed may not be in a window to say its own
        let half = 0.5 / max(scale, 1)
        func alone(_ last: NodeView, among children: [FlowChild]) -> Bool {
            let top = ControlBars.laid(last).minY
            return children.allSatisfy { child in
                switch child {
                case .view(let view): view === last || ControlBars.laid(view).maxY <= top + half
                case .leaf(let leaf): leaf.frame.maxY <= top + half
                }
            }
        }
        // The scroller and each box around it keep their foot where layout put
        // it, its bottom still at the bottom: a transform of their own that
        // scales, turns, flips vertically or moves one draws the native scroll
        // view away from the edge whose band it would take. Its drawn bottom
        // corners are read, not `frame`, which a transform leaves undefined.
        func keepsFoot(_ node: NodeView) -> Bool {
            let t = node.transform
            guard CATransform3DIsAffine(node.layer.transform), abs(t.b) < 0.00001, abs(t.c) < 0.00001, t.d > 0,
                  let holder = node.superview else { return false }
            let foot = ControlBars.laid(node).maxY
            return [node.bounds.minX, node.bounds.maxX].allSatisfy {
                abs(node.convert(CGPoint(x: $0, y: node.bounds.maxY), to: holder).y - foot) <= half
            }
        }
        var parent = route
        while true {
            let children = orderedChildren(of: parent).filter { child in
                switch child {
                case .view(let view): flowing(view) && !lifting.contains(where: { $0 === view })
                case .leaf(let leaf): leaf.isInFlow
                }
            }
            guard case .view(let last)? = children.last, alone(last, among: children), keepsFoot(last) else { return nil }
            // Each reaches the foot of its parent's content box where layout
            // put it (its drawn foot is `keepsFoot`'s): the route's child that
            // box less the cover the kernel may pad it by, a deeper one its
            // parent's bottom, with nothing (a padding, a border) under it.
            let foot = parent === route ? parent.contentBox().maxY : parent.bounds.maxY
            if parent !== route, parent.contentBox().maxY < foot - 0.5 { return nil }
            let gap = foot - parent.convert(ControlBars.laid(last), from: last.superview).maxY
            if gap > 0.5 && !(parent === route && abs(gap - cover) <= 0.5) { return nil }
            if last.scrollView != nil { return ["scroll", "auto"].contains(last.style["overflow_y"]?.string ?? "") ? last : nil }
            parent = last
        }
    }
}
#endif
