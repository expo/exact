#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

extension NavigationHost {
    /// The route's last in-flow scroller can reach under the tab bar and home indicator
    /// (LLP 1116 D2). Its bottom inset takes the route's cover so in-flow content clears
    /// the bar once. The cover still places an absolute footer above the bar.
    /// A cover-fit page handles its own edges, except in a sheet that owns its safe area.
    func reachBottom(_ c: RouteController, presented: Bool) {
        // Until a scroller reaches the bottom edge the route's bottom cover
        // pads its in-flow content above the bars, and its content box is
        // the authored one: a scroller there ends that far above it. The
        // cover it has without one, so a scroller that does not grow as the
        // cover goes is at the edge either way.
        let peers = arrangementScrollers(in: c, top: false)
        let peer = contentScroller(of: c).flatMap { node in peers.contains(where: { $0 === node }) ? node : nil } ?? peers.first
        let found = presented || presenter.viewportFit != "cover" ? (Self.bottomScroller(in: c.node, cover: c.bottomClearance, lifting: c.liftedBottomRows) ?? peer) : nil
        let target = RouteGeometry.clearanceOff(c.node) & 4 != 0 ? nil : found
        let projects = !c.nativeContentScrollEdges.contains(.bottom)
        // Collapse has already chosen the adjustment for a shared scroll view. A
        // bottom-only scroller takes UIKit's adjustment, including when it just gave up the top.
        let shown = shownScroll(target)
        let needsAdjustment = shown != nil && target !== c.collapseScroll && shown?.contentInsetAdjustmentBehavior != .automatic
        guard needsAdjustment || c.edgeScroll !== target || (projects && c.contentScrollView(for: .bottom) !== c.automaticBottomScroll) else { return }
        if let old = c.edgeScroll, old !== target, let sv = old.scrollView, old !== c.collapseScroll, !arrangementInsets(sv, in: c) { sv.contentInsetAdjustmentBehavior = .never }
        if projects, let previous = c.contentScrollView(for: .bottom), previous !== target?.scrollView,
           previous !== c.collapseScrollView, !arrangementInsets(previous, in: c) { previous.contentInsetAdjustmentBehavior = .never }
        c.edgeScroll = target
        // A scroller at its start stays there as UIKit begins to inset it.
        // One made in this batch can hold a safe area from before its first
        // frame (at the route's top, under the bar): UIKit insets it by that
        // first, then, given it below, takes the inset away and moves the
        // offset by as much, scrolling its content out of its port.
        let atStart = needsAdjustment && shown.map { abs($0.contentOffset.y + $0.adjustedContentInset.top) <= 0.5 } == true
        if needsAdjustment { shown?.contentInsetAdjustmentBehavior = .automatic }
        if atStart, let sv = shown, sv.contentOffset.y != -sv.adjustedContentInset.top { sv.contentOffset = PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: -sv.adjustedContentInset.top), in: sv) }
        // tvOS has no bottom bar whose edge follows a scroller.
        #if !os(tvOS)
        c.projectContentScrollView(c.automaticBottomScroll, for: .bottom)
        #endif
        presenter.session?.log("navigation: route \(c.key) \(target == nil ? "keeps its content above the bottom edge" : "scrolls its content to the bottom edge")")
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
    static func bottomScroller(in route: NodeView, cover: CGFloat = 0, lifting: [NodeView] = []) -> NodeView? {
        let flowing = RouteGeometry.inFlow
        // The last in-flow child spans the bottom edge alone when every other
        // ends above its top, as children stacked down a block or a column
        // do. Beside it in a row or a grid, a sibling reaches the edge too
        // (a column with a Save button at its foot) and keeps the cover. The
        // laid-out frames say so; the host is not sent the flex direction.
        func alone(_ last: NodeView, among children: [NodeView]) -> Bool {
            children.allSatisfy { $0 === last || $0.frame.maxY <= last.frame.minY + 0.5 }
        }
        var parent = route
        while true {
            let children = parent.container.subviews.compactMap { $0 as? NodeView }.filter { child in flowing(child) && !lifting.contains(where: { $0 === child }) }
            guard let last = children.last, alone(last, among: children) else { return nil }
            // Each reaches the foot of its parent's content box: the route's
            // child that box less the cover the kernel may pad it by, a
            // deeper one its parent's bottom, with nothing (a padding, a
            // border) under it.
            let foot = parent === route ? parent.contentBox().maxY : parent.bounds.maxY
            if parent !== route, parent.contentBox().maxY < foot - 0.5 { return nil }
            let gap = foot - parent.convert(last.bounds, from: last).maxY
            if gap > 0.5 && !(parent === route && abs(gap - cover) <= 0.5) { return nil }
            if last.scrollView != nil { return ["scroll", "auto"].contains(last.style["overflow_y"]?.string ?? "") ? last : nil }
            parent = last
        }
    }
}
#endif
