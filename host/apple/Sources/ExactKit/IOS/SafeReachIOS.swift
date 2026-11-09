// PROTO (safe-area design r6 §2; not landing code). A route's root is laid out in its
// safe area (the kernel's clearance); after each batch, the surfaces that meet the safe area's
// edge reach the allocation's edge on that side as overflow. Their native views are outset with
// the bounds origin shifted, so their subviews stay where layout put them while the view covers
// the band and UIKit's safe area reaches the scroll views inside (LLP 1131: it does not reach a
// view drawn past its superview's box). The design computes reach in the kernel; this prototype
// computes it here from the laid-out frames.
#if os(iOS) || os(tvOS)
import UIKit

/// PROTO (UIKit-first measurement): a box's route and the scroll settings owned by the reach pass
struct SafeReachState {
    /// PROTO (UIKit-first measurement): the route view whose top edge this box reaches or takes,
    /// available without walking ancestors during a frame operation
    weak var route: UIView?
    /// PROTO (UIKit-first measurement): the adjustment behavior set on the scroll view, undone when it stops reaching
    var behavior: UIScrollView.ContentInsetAdjustmentBehavior?
    /// PROTO (UIKit-first measurement): this pass's contribution to `contentInset` on axes UIKit does not adjust
    var inset: UIEdgeInsets = .zero
    /// PROTO (UIKit-first measurement): the scroll view given that inset; a replaced backing view starts from none
    var insetView: ObjectIdentifier?
    /// PROTO (UIKit-first measurement): the main-axis bounce set by this pass, undone when it no longer applies
    var bounce = false
    /// PROTO (UIKit-first measurement): the part of the reach crossing an enclosing scroller's padding, also inset by the composer
    var own: UIEdgeInsets = .zero
}

extension NodeView {
    /// The drawn border box in the superview, excluding the native safe-area reach.
    package var boxFrame: CGRect { frame.inset(by: reach) }
}

extension Presenter {
    /// Every shown route's reach, after a batch's frames.
    func reachSafeEdges() {
        geometry.topReachers.removeAll(keepingCapacity: true) // PROTO (UIKit-first measurement): rebuild the route-to-view ID lists
        for c in navigation.controllers.values where views[c.node.id] === c.node { reachSafeEdges(of: c) }
    }

    private func reachSafeEdges(of c: RouteController) {
        // A retained tab keeps its geometry and UIKit's offset while it is offscreen.
        guard let view = c.viewIfLoaded, view.window != nil else { return }
        guard case .edges(let e)? = navigation.covers[c.node.id] else {
            SafeReach.clear(c.node)
            return
        }
        let a = view.bounds
        // PROTO (rule 4): test contact at the live edge after the resting mode places its boxes
        var edge = e.top
        if ExactSession.protoMode == .resting, edgeFrames[c.node.id]?.isEmpty == false { edge = min(e.top, max(0, view.safeAreaInsets.top)) }
        let s = CGRect(x: a.minX + e.left, y: a.minY + edge, width: max(0, a.width - e.left - e.right), height: max(0, a.height - edge - e.bottom))
        var top: [UInt32] = [] // PROTO (UIKit-first measurement): view IDs reaching or taking this route's top
        let walk = SafeReach(view: view, allocation: a, safe: s, pixel: 1 / max(1, view.traitCollection.displayScale)) {
            top.append($0.id)
            $0.safeReach.route = view
        }
        _ = walk.visit(c.node, allowed: .all)
        walk.applyPlan()
        geometry.topReachers[c.node.id] = top // PROTO (UIKit-first measurement)
    }

    /// PROTO (UIKit-first measurement): a moving top edge's reach (`GeometryOnlyIOS.swift`).
    /// What touches the edge stays the same while it moves; only its depth from the allocation's
    /// top changes. Each box the last walk found reaching or taking the top gets that depth again;
    /// the rest keep theirs until the walk after the edge stops.
    func reachMovingTops() {
        for c in navigation.controllers.values where views[c.node.id] === c.node {
            guard let view = c.viewIfLoaded, view.window != nil, let top = geometry.topReachers[c.node.id] else { continue }
            let a = view.bounds
            // The walk listed a box after the boxes inside it; each is outset before them, as `SafeReach.applyPlan` does
            for id in top.reversed() {
                guard let v = views[id], v.window != nil else { continue }
                // PROTO (UIKit-first measurement): skip writes already made by the frame operation (`PixelGrid.frame`)
                let depth = max(0, v.convert(v.boxBounds.inset(by: v.take), to: view).minY - a.minY)
                if v.take.top > 0, abs(v.take.top - depth) > 0.01 { var take = v.take; take.top = depth; SafeReach.take(take, to: v) }
                if v.reach.top > 0, abs(v.reach.top - depth) > 0.01 { var reach = v.reach; reach.top = depth; SafeReach.apply(reach, to: v) }
            }
        }
    }
}

/// One route's walk: which edges each box touches, which it reaches, and the outset applied.
struct SafeReach {
    let view: UIView, allocation: CGRect, safe: CGRect, pixel: CGFloat
    /// PROTO (UIKit-first measurement): the callback for each box that reaches or takes the top
    /// edge (`Presenter.reachMovingTops`)
    let reachesTop: (NodeView) -> Void
    /// Each box's outset as the walk decides it, a box after the boxes inside it (`applyPlan`).
    let plan = Plan()
    final class Plan { var steps: [(view: NodeView, take: UIEdgeInsets?, reach: UIEdgeInsets)] = [] }

    /// Applies the walk's outsets, each box before the boxes inside it. UIKit infers a view's safe area and margins
    /// from its superview's bounds when the view's own frame changes, and not when only the superview's bounds origin
    /// moves: outset before its superview, a view kept margins measured against the superview's old origin (an
    /// inset-grouped list in the iPhone Duo's inspector column lost its cards' 20-point inset when the Duo unfolded
    /// from a partial fold, until the list was built again). The walk reads boxes, which an outset does not move
    /// (`boxBounds`, less `take`), so it decides every outset before any is applied.
    func applyPlan() {
        for step in plan.steps.reversed() {
            if let take = step.take { Self.take(take, to: step.view) }
            Self.apply(step.reach, to: step.view)
        }
        plan.steps.removeAll()
    }

    /// The edges a box or something inside it reaches.
    @discardableResult
    func visit(_ v: NodeView, allowed: UIRectEdge, edge: CGRect? = nil) -> UIRectEdge {
        guard !v.isHidden else { plan.steps.append((v, nil, .zero)); return [] }
        let safe = edge ?? self.safe
        // Its laid-out box: its bounds less its reach (origin shifted) or less what it takes (origin kept).
        let r = v.convert(v.boxBounds.inset(by: v.take), to: view)
        var touches: UIRectEdge = []
        if allowed.contains(.top), safe.minY > allocation.minY + pixel, abs(r.minY - safe.minY) <= pixel { touches.insert(.top) }
        if allowed.contains(.bottom), safe.maxY < allocation.maxY - pixel, abs(r.maxY - safe.maxY) <= pixel { touches.insert(.bottom) }
        if allowed.contains(.left), safe.minX > allocation.minX + pixel, abs(r.minX - safe.minX) <= pixel { touches.insert(.left) }
        if allowed.contains(.right), safe.maxX < allocation.maxX - pixel, abs(r.maxX - safe.maxX) <= pixel { touches.insert(.right) }
        let decorated = Self.decorated(v)
        let contained = Self.contained(v)
        // What may reach inside it: where it touches, not past a clip that does not extend or a `contain`ed edge,
        // and in a scroller only across its scrolling axis (scrolled content never reaches along it).
        var inside = touches.subtracting(contained)
        // A clip stops what lies inside it only where it clips something: on an edge its laid-out content
        // overflows (a drawer at `left: -280px`), or on every edge for a shape that is not its box's rectangle.
        if Self.clips(v) { inside.subtract(decorated ? .all : Self.overflowing(v, in: view, box: r, pixel: pixel)) }
        if v.scroll != nil {
            let axes = Self.axes(v)
            if axes.y { inside.subtract([.top, .bottom]) }
            if axes.x { inside.subtract([.left, .right]) }
        }
        var below: UIRectEdge = []
        // Inside a scroller, the inner edge of its padding is a safe edge on each edge it touches: a carousel or a
        // row's fill that meets it reaches the allocation's edge, as UIKit's list lays an orthogonal section across
        // the whole collection view while its items start at the content-insets reference.
        var inner = safe
        if v.scroll != nil {
            let u = v.number("border_width")
            if inside.contains(.left) { inner.origin.x = r.minX + v.number("border_width_left", u) + v.number("padding_left"); inner.size.width = safe.maxX - inner.minX }
            if inside.contains(.right) { inner.size.width = r.maxX - v.number("border_width_right", u) - v.number("padding_right") - inner.minX }
        }
        for child in v.container.subviews.compactMap({ $0 as? NodeView }) { below.formUnion(visit(child, allowed: inside, edge: inner)) }
        let leaf = v.container.subviews.allSatisfy { !($0 is NodeView) } && v.scroll == nil
        var leafTake: UIEdgeInsets?
        // A leaf that takes its edges (`-exact-safe-area="ignore …"`, §2.4) is laid out to the allocation's
        // edge with its content: its frame grows, its bounds origin stays, so what fills its bounds (a module's
        // view, its own layer) fills the band, and UIKit's safe area tells a native view where its controls go.
        if leaf {
            let taken = Self.ignored(v).intersection(touches)
            var take = UIEdgeInsets.zero
            if taken.contains(.top) { take.top = r.minY - allocation.minY }
            if taken.contains(.left) { take.left = r.minX - allocation.minX }
            if taken.contains(.bottom) { take.bottom = allocation.maxY - r.maxY }
            if taken.contains(.right) { take.right = allocation.maxX - r.maxX }
            leafTake = take
            // Its ancestors cover what it takes, so UIKit's safe area reaches the view inside it.
            below.formUnion(taken)
        }
        let surface = v.scroll != nil || (Self.fills(v) && !leaf)
        var reach = UIEdgeInsets.zero
        // An edge a leaf takes is laid out to the allocation already: it does not also reach there.
        let takes = leaf ? Self.ignored(v) : []
        for edge in [UIRectEdge.top, .left, .bottom, .right] where touches.contains(edge) && !contained.contains(edge) && !takes.contains(edge) {
            // A decorated box covers for what reaches inside it but paints nothing new: the prototype
            // keeps it in its box (the design separates paint from coverage).
            // At an enclosing scroller's padding edge only a nested scroller reaches (UIKit's orthogonal section spans
            // the collection view); a fill stays inside the padding, as CSS paints it.
            let padded = (edge == .left && safe.minX > self.safe.minX + pixel) || (edge == .right && safe.maxX < self.safe.maxX - pixel)
            guard !decorated, (surface && (v.scroll != nil || !padded)) || below.contains(edge) else { continue }
            switch edge {
            case .top: reach.top = r.minY - allocation.minY
            case .left: reach.left = r.minX - allocation.minX
            case .bottom: reach.bottom = allocation.maxY - r.maxY
            default: reach.right = allocation.maxX - r.maxX
            }
        }
        if reach.top > 0 || v.take.top > 0 { reachesTop(v) } // PROTO (UIKit-first measurement)
        // What of the reach lies inside the safe area: an enclosing scroller's padding, crossed to reach its edge.
        v.safeReach.own = UIEdgeInsets(top: 0, left: reach.left > 0 ? max(0, safe.minX - self.safe.minX) : 0,
                                  bottom: 0, right: reach.right > 0 ? max(0, self.safe.maxX - safe.maxX) : 0)
        plan.steps.append((v, leafTake, reach))
        var reached: UIRectEdge = below
        if reach.top > 0 { reached.insert(.top) }
        if reach.left > 0 { reached.insert(.left) }
        if reach.bottom > 0 { reached.insert(.bottom) }
        if reach.right > 0 { reached.insert(.right) }
        return reached
    }

    /// Outsets `v`'s frame by `reach` and shifts its bounds origin by as much. A reaching scroller's scroll view is
    /// framed to its outset bounds and UIKit adjusts its content on the axes it scrolls (`.scrollableAxes`). Every
    /// setting the pass owns is derived from the current state and undone when that state no longer asks for it, so a
    /// retained view ends as a cold mount would.
    static func apply(_ reach: UIEdgeInsets, to v: NodeView) {
        if reach != v.reach {
            let base = v.frame.inset(by: v.reach)
            v.reach = reach
            v.bounds.origin = CGPoint(x: -reach.left, y: -reach.top)
            v.frame = base.inset(by: UIEdgeInsets(top: -reach.top, left: -reach.left, bottom: -reach.bottom, right: -reach.right))
        }
        guard let sv = v.scroll else { return }
        if sv.frame != v.scrollFrame { sv.frame = v.scrollFrame }
        if reach == .zero {
            if let set = v.safeReach.behavior, sv.contentInsetAdjustmentBehavior == set { sv.contentInsetAdjustmentBehavior = .automatic } // PROTO: return UIKit's default
            v.safeReach.behavior = nil
        } else if v.presenter?.navigation.insetByUIKit(v) != true { // PROTO: leave navigation's adjustment alone
            // UIKit adjusts the axes it counts scrollable; bounce below makes those layout's axes
            let want: UIScrollView.ContentInsetAdjustmentBehavior = .scrollableAxes
            // `.always` is another owner's choice (a dialog sheet's scroller); the rest are this pass's or UIKit's default.
            if sv.contentInsetAdjustmentBehavior != want, sv.contentInsetAdjustmentBehavior != .always { sv.contentInsetAdjustmentBehavior = want }
            v.safeReach.behavior = sv.contentInsetAdjustmentBehavior == want ? want : nil // PROTO: own only the reach policy
        }
        v.fitScroll()
        // Each scroll axis that no enclosing scroller scrolls bounces, so UIKit's own test (`UIScrollView.h`: content
        // past the frame, or bounce) counts exactly the axes layout decided, whatever the native frame; on the other
        // axes the content is no wider than its laid-out port, so never past the outset frame. Only while it reaches.
        let axes = Self.axes(v), outer = Self.outer(v).map(Self.axes)
        // (`fitScroll` already bounces a vertical axis that no enclosing scroller scrolls vertically.)
        let bx = reach != .zero && axes.x && !(outer?.x ?? false)
        if bx != v.safeReach.bounce || (bx && !sv.alwaysBounceHorizontal) {
            if bx { sv.alwaysBounceHorizontal = true } else if v.safeReach.bounce { sv.alwaysBounceHorizontal = false }
            v.safeReach.bounce = bx
        }
        reconcile(v)
    }

    /// The content's place on each edge a scroller reaches: UIKit's adjustment on the axes UIKit adjusts, and
    /// `contentInset` for the part of the reach that is the safe area on the rest: the reach less what of it crosses
    /// an enclosing scroller's padding (`own`), which is the scroll view's safe-area inset at rest. Read from the
    /// reach, it changes in the pass that moves the reach. Read from the scroll view's safe area, it changed when
    /// UIKit gave a new one, against the old reach: a column opening moved a page by the whole old reach (280
    /// points on the iPhone Duo) until the batch that moved the reach. Which axes those are follows UIKit's
    /// rule (`UIScrollView.h`: `.scrollableAxes` adjusts an axis whose content passes the view's size or that
    /// bounces, `.always` every axis). PROTO: a primary `.automatic` scroller also adjusts vertically when its
    /// content fits. The adjusted inset trails a restoring bar; a safe-area change must not fill that lag.
    static func reconcile(_ v: NodeView) {
        guard let sv = v.scroll else { return }
        if v.safeReach.insetView != ObjectIdentifier(sv) { v.safeReach.inset = .zero; v.safeReach.insetView = ObjectIdentifier(sv) }
        let r = v.reach, own = v.safeReach.own, ci = sv.contentInset
        let adjusts = (x: sv.contentSize.width > sv.bounds.width || sv.alwaysBounceHorizontal,
                       y: sv.contentSize.height > sv.bounds.height || sv.alwaysBounceVertical
                           || (sv.contentInsetAdjustmentBehavior == .automatic && v.presenter?.navigation.insetByUIKit(v) == true))
        func part(_ reach: CGFloat, _ own: CGFloat, _ adjusts: Bool) -> CGFloat {
            reach > 0 && !adjusts && sv.contentInsetAdjustmentBehavior != .always && sv.contentInsetAdjustmentBehavior != .never ? max(0, reach - own) : 0
        }
        let want = UIEdgeInsets(top: part(r.top, own.top, adjusts.y) + (r.top > 0 ? own.top : 0),
                                left: part(r.left, own.left, adjusts.x) + (r.left > 0 ? own.left : 0),
                                bottom: part(r.bottom, own.bottom, adjusts.y) + (r.bottom > 0 ? own.bottom : 0),
                                right: part(r.right, own.right, adjusts.x) + (r.right > 0 ? own.right : 0))
        let old = v.safeReach.inset
        guard want != old else { return }
        v.safeReach.inset = want
        // UIKit keeps the offset as an inset changes; content resting at its start stays there (CSS scrollLeft and
        // scrollTop 0), as it does when UIKit's own adjustment arrives.
        let atRest = !sv.isTracking && !sv.isDragging && !sv.isDecelerating // PROTO (UIKit-first measurement): leave a moving scroller's offset to UIKit
        let adj = sv.adjustedContentInset
        let atStart = (x: want.left != old.left && sv.contentOffset.x <= 0.5 - adj.left,
                       y: want.top != old.top && sv.contentOffset.y <= 0.5 - adj.top)
        sv.contentInset = UIEdgeInsets(top: ci.top + want.top - old.top, left: ci.left + want.left - old.left,
                                       bottom: ci.bottom + want.bottom - old.bottom, right: ci.right + want.right - old.right)
        guard atRest else { return }
        let now = sv.adjustedContentInset
        let o = CGPoint(x: atStart.x ? -now.left : sv.contentOffset.x, y: atStart.y ? -now.top : sv.contentOffset.y)
        if o != sv.contentOffset { sv.contentOffset = o } // PROTO: keep the resting edge UIKit reports, including its fractional inset
    }

    /// Whether `v` lies in another scroller's scrolled content (the presenter's viewport is no author's).
    static func nested(_ v: NodeView) -> Bool { outer(v) != nil }

    static func outer(_ v: NodeView) -> NodeView? {
        var up = v.superview
        while let u = up {
            if u is ScrollView, !(u is Viewport) { return u.superview as? NodeView }
            up = u.superview
        }
        return nil
    }

    static func outerMainAxis(_ v: NodeView) -> Axis? { outer(v).map(mainAxis) }

    enum Axis { case x, y }

    /// A scroller's main axis, from style: the one scrollable axis when the other is `hidden` or `clip`; with both
    /// scrollable, the inline axis for a row flex container that does not wrap or a column-flowing grid, else the
    /// block axis.
    static func mainAxis(_ v: NodeView) -> Axis {
        let ox = v.style["overflow_x"]?.string ?? "visible", oy = v.style["overflow_y"]?.string ?? "visible"
        let sx = ox == "scroll" || ox == "auto", sy = oy == "scroll" || oy == "auto"
        if sx != sy { return sx ? .x : .y }
        let display = v.style["display"]?.string
        if display == "flex", !(v.style["flex_direction"]?.string ?? "row").hasPrefix("column"),
           (v.style["flex_wrap"]?.string ?? "nowrap") == "nowrap" { return .x }
        if display == "grid", (v.style["grid_auto_flow"]?.string ?? "row").hasPrefix("column") { return .x }
        return .y
    }

    /// The axes a scroller scrolls, from style and layout alone: its main axis, and an `auto` or `scroll` axis whose
    /// laid-out content overflows its laid-out port (CSS's scrollable overflow). Never the native frame, which reach
    /// outsets: reach does not feed back into what decides it.
    static func axes(_ v: NodeView) -> (x: Bool, y: Bool) {
        guard let sv = v.scroll else { return (false, false) }
        let main = mainAxis(v), port = v.boxBounds.size
        let x = sv.scrollsX && (main == .x || sv.contentSize.width > port.width)
        let y = sv.scrollsY && (main == .y || sv.contentSize.height > port.height)
        return (x, y)
    }

    /// The edges where a clipping box's laid-out content overflows it, by more than a device pixel: what its clip
    /// clips. Nested scrollers and clips hold their own overflow, so only their boxes count.
    static func overflowing(_ v: NodeView, in view: UIView, box: CGRect, pixel: CGFloat) -> UIRectEdge {
        var out: UIRectEdge = []
        func walk(_ n: NodeView) {
            for c in n.container.subviews.compactMap({ $0 as? NodeView }) where !c.isHidden {
                let r = c.convert(c.boxBounds.inset(by: c.take), to: view)
                if r.minY < box.minY - pixel { out.insert(.top) }
                if r.maxY > box.maxY + pixel { out.insert(.bottom) }
                if r.minX < box.minX - pixel { out.insert(.left) }
                if r.maxX > box.maxX + pixel { out.insert(.right) }
                if c.scroll == nil && !clips(c) { walk(c) }
            }
        }
        walk(v)
        return out
    }

    /// Outsets a leaf that takes its edges, its bounds origin kept.
    static func take(_ take: UIEdgeInsets, to v: NodeView) {
        guard take != v.take else { return }
        let base = v.frame.inset(by: v.take)
        v.take = take
        v.frame = base.inset(by: UIEdgeInsets(top: -take.top, left: -take.left, bottom: -take.bottom, right: -take.right))
    }

    /// The edges `-exact-safe-area="ignore …"` takes: comma-separated groups, `ignore` alone taking every edge, `start`
    /// and `end` the inline edges under the box's direction (right to left mirrors them).
    static func ignored(_ v: NodeView) -> UIRectEdge { edges(v, keyword: "ignore") }

    /// The edges `-exact-safe-area="extend …"` mirrors artwork into.
    static func extended(_ v: NodeView) -> UIRectEdge { edges(v, keyword: "extend") }

    static func edges(_ v: NodeView, keyword: String) -> UIRectEdge {
        guard let raw = v.props["safeArea"] else { return [] }
        let rtl = v.effectiveUserInterfaceLayoutDirection == .rightToLeft
        var out: UIRectEdge = []
        for group in raw.split(separator: ",") {
            let words = group.split(separator: " ")
            guard words.first.map(String.init) == keyword else { continue }
            if words.count == 1 { return .all }
            for w in words.dropFirst() {
                switch w {
                case "top": out.insert(.top)
                case "bottom": out.insert(.bottom)
                case "start": out.insert(rtl ? .right : .left)
                case "end": out.insert(rtl ? .left : .right)
                default: break
                }
            }
        }
        return out
    }

    static func clear(_ v: NodeView) {
        take(.zero, to: v)
        apply(.zero, to: v)
        for child in v.container.subviews.compactMap({ $0 as? NodeView }) { clear(child) }
    }

    /// A plain fill: a colour or a material.
    static func fills(_ v: NodeView) -> Bool {
        if v.materialKind != nil { return true }
        guard let c = v.channels("background_color") else { return false }
        return c.count < 4 || c[3] > 0
    }

    static func clips(_ v: NodeView) -> Bool {
        let x = v.style["overflow_x"]?.string ?? "visible", y = v.style["overflow_y"]?.string ?? "visible"
        return v.scroll == nil && (x != "visible" || y != "visible")
    }

    /// Paint that growing would change: a border, a corner radius, a shadow, an image, a clip path, a filter, a transform.
    static func decorated(_ v: NodeView) -> Bool {
        if v.number("border_width") > 0 { return true }
        for side in ["top", "right", "bottom", "left"] where v.number("border_width_" + side) > 0 { return true }
        if v.number("border_radius") > 0 { return true }
        for corner in ["top_left", "top_right", "bottom_left", "bottom_right"] where v.number("border_radius_" + corner) > 0 { return true }
        for key in ["box_shadow", "background_image", "clip_path", "filter", "transform"] {
            if let s = v.style[key]?.string, s != "none", !s.isEmpty { return true }
        }
        return false
    }

    /// No edge is contained: a box keeps off the edge by not touching it (a margin, its root's padding).
    static func contained(_ v: NodeView) -> UIRectEdge { [] }
}
#endif
