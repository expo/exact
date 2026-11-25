#if os(iOS) || os(tvOS)
import UIKit

/// A plain container: the document, a canvas's overlay (LLP 1014). Hit-
/// testable at alpha 0 — a canvas's children painted through its surface
/// composite at alpha 0 and must still take a tap, which UIKit's default
/// hit-test refuses below 0.01 — and transparent to a hit on nothing, so
/// the touch reaches what holds it (the canvas, the viewport).
package final class PlainView: UIView {
    package override func didAddSubview(_ subview: UIView) { super.didAddSubview(subview); FocusSearch.joined(subview) }
    package override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        // The document takes the page's bands too, where its edge scrollers reach (LLP 1130 D3).
        let m = (superview as? Viewport)?.pageMargins ?? .zero
        // A one-axis clip's box fits its children's paint, not their hit
        // slop: past the node it limits only the clipped axis. Its canvas
        // overlay leaves the clip to the canvas's own hit test. A layout
        // transition uses its shown padding edge.
        let inside: Bool
        if let node = superview as? NodeView, node.clipBox === self, node.clipsOneAxis || node.surface != nil {
            inside = node.reachesChildren(at: convert(point, to: node))
        } else {
            inside = canvasOverlayOwner?.clipsOneAxis == true
                || bounds.inset(by: UIEdgeInsets(top: -m.top, left: -m.left, bottom: -m.bottom, right: -m.right)).contains(point)
        }
        guard !isHidden, isUserInteractionEnabled, inside else { return nil }
        return NodeView.hitChildren(in: self, at: point, with: event)
    }
    package override var accessibilityElements: [Any]? {
        get { super.accessibilityElements ?? NodeView.emptyBoxAccessibility(in: self) }
        set { super.accessibilityElements = newValue }
    }
}

extension UIEdgeInsets {
    /// Each side with `other`'s added.
    func adding(_ other: UIEdgeInsets) -> UIEdgeInsets {
        UIEdgeInsets(top: top + other.top, left: left + other.left, bottom: bottom + other.bottom, right: right + other.right)
    }
}

/// A scroll view whose indicators take its own safe area where UIKit's adjustment gives them none
/// (`SafeIndicatorTerm`): Exact's `ScrollView` and a grouped list's collection.
package protocol SafeIndicators: UIScrollView {
    /// D14's writer writes the indicator insets whole (`ScrollInsetsIOS.write`), the side bands among its terms, and
    /// the term is none of them.
    var writtenWhole: Bool { get set }
}

/// What a scroll view's own safe area adds to both indicators' insets where UIKit's adjustment gives them none: its
/// sides always, and its top and bottom too while that adjustment is off. UIKit's adjustment, on, adds the safe area's
/// top and bottom (under a large title it hangs over, a top of its own), never its sides, so a scroll view filling
/// the iPhone Duo's cover screen keeps its indicator under the button strip; with this term it lies beside it, clear
/// of the system's controls (HIG Layout). A page band turns the adjustment off while it bands the scroll view, so the
/// band and the safe area are each counted once (`NodeView.bandScroll`). The term moves as a difference, as the
/// padding edge's (`ScrollView.paddingInset`), the keyboard's and a page band's do, so each leaves the others' in
/// place; its scroll view syncs it as its safe area, its adjustment or `writtenWhole` changes.
package struct SafeIndicatorTerm {
    private var applied = UIEdgeInsets.zero

    package init() {}

    package mutating func sync(_ scroll: some SafeIndicators) {
        let safe = scroll.safeAreaInsets
        let term = scroll.writtenWhole ? .zero
            : scroll.automaticallyAdjustsScrollIndicatorInsets ? UIEdgeInsets(top: 0, left: safe.left, bottom: 0, right: safe.right) : safe
        guard term != applied else { return }
        let moved = UIEdgeInsets(top: term.top - applied.top, left: term.left - applied.left,
                                 bottom: term.bottom - applied.bottom, right: term.right - applied.right)
        applied = term
        scroll.verticalScrollIndicatorInsets = scroll.verticalScrollIndicatorInsets.adding(moved)
        scroll.horizontalScrollIndicatorInsets = scroll.horizontalScrollIndicatorInsets.adding(moved)
    }
}

/// A scroll container — the viewport over the document, and a node whose
/// effective `overflow` scrolls. The platform pans it (LLP 1002 D4: scroll
/// always wins; UIKit does not chain a pan out of a nested scroll view at
/// its edge). Which axes it scrolls comes from the node's rows; a tap's
/// wheel (the agent's) applies the web's chaining rule itself
/// (`AgentIOS.swift`).
package class ScrollView: UIScrollView, SafeIndicators {
    var scrollingNode: NodeView? { (superview as? NodeView) ?? (self as? Viewport)?.documentNode }
    /// Where it last came to rest along x and y, and on which snap area (`ScrollSnapIOS.swift`).
    var snapRested = [ScrollSnap.Rested(), ScrollSnap.Rested()]
    /// UIKit's paging is on because its snap is a pager's (`ScrollSnapIOS.swift`).
    package internal(set) var snapPages = false
    package override func layoutSubviews() {
        super.layoutSubviews()
        syncIndicators()
        keepPlatformViewsOver()
        holdPaddingClip()
    }
    /// Its node's padding edge, in the node's coordinates, while a border puts that edge inside the scroll view
    /// (`NodeView.syncScrollCorner`), `inset` from its sides by the border's widths. It masks the layer, whose
    /// coordinates scroll with the bounds, so each layout, which UIKit runs as the bounds move, puts it back over
    /// the node's box, in that layout's own transaction and with no animation of its own. The mask cuts everything
    /// in the layer, UIKit's scroll indicators too, which lie at the scroll view's edge: they keep inside it by the
    /// same widths (`paddingInset`), between the border and the padding edge, where
    /// [CSS has a scrollbar](https://drafts.csswg.org/css-overflow-3/#scrollbar-gutter-property).
    private var paddingClip: CAShapeLayer?
    func clip(toPaddingEdge edge: CGPath?, inset: UIEdgeInsets) {
        guard let edge else {
            if let clip = paddingClip, layer.mask === clip { layer.mask = nil }
            paddingClip = nil
            return paddingInset = .zero
        }
        let clip = paddingClip ?? CAShapeLayer()
        if paddingClip == nil {
            clip.anchorPoint = .zero
            clip.fillColor = CGColor(gray: 1, alpha: 1)
            // It is where the scroll is at once: a layer no view owns would ease to each new position.
            clip.actions = ["position": NSNull(), "path": NSNull()]
            paddingClip = clip
        }
        if clip.path != edge { clip.path = edge }
        if layer.mask !== clip { layer.mask = clip }
        holdPaddingClip()
        paddingInset = inset
    }
    private func holdPaddingClip() {
        guard let clip = paddingClip, layer.mask === clip else { return }
        let origin = CGPoint(x: bounds.minX - frame.minX, y: bounds.minY - frame.minY)
        if clip.position != origin { clip.position = origin }
    }
    /// What the padding edge's mask adds to both indicators' insets on each side: the border's widths while it
    /// masks, else none. It moves as a difference, as the keyboard's and a page band's terms do
    /// (`KeyboardAvoidanceIOS.swift`, `PageEdgesIOS.swift`), so each leaves the others' in place and the insets are
    /// as they were once the border goes. D14's writer, which writes them whole, adds it (`ScrollInsetsIOS.write`).
    var paddingInset = UIEdgeInsets.zero {
        didSet {
            guard paddingInset != oldValue else { return }
            let moved = UIEdgeInsets(top: paddingInset.top - oldValue.top, left: paddingInset.left - oldValue.left,
                                     bottom: paddingInset.bottom - oldValue.bottom, right: paddingInset.right - oldValue.right)
            verticalScrollIndicatorInsets = verticalScrollIndicatorInsets.adding(moved)
            horizontalScrollIndicatorInsets = horizontalScrollIndicatorInsets.adding(moved)
        }
    }
    private var safeIndicators = SafeIndicatorTerm()
    package var writtenWhole = false { didSet { safeIndicators.sync(self) } }
    package override var automaticallyAdjustsScrollIndicatorInsets: Bool { didSet { safeIndicators.sync(self) } }
    package override func safeAreaInsetsDidChange() {
        super.safeAreaInsetsDidChange()
        safeIndicators.sync(self)
    }
    /// UIKit exposes an indicator and its thumb to accessibility even when
    /// the axis has no range. Keep the requested visibility across resizes
    /// and content changes, including CSS `none` and native hatch writes.
    private var horizontalIndicator = true, verticalIndicator = true
    package override var showsHorizontalScrollIndicator: Bool {
        get { super.showsHorizontalScrollIndicator }
        set { horizontalIndicator = newValue; syncIndicators() }
    }
    package override var showsVerticalScrollIndicator: Bool {
        get { super.showsVerticalScrollIndicator }
        set { verticalIndicator = newValue; syncIndicators() }
    }
    private func syncIndicators() {
        let inset = adjustedContentInset
        let x = horizontalIndicator && scrollsX && isScrollEnabled && contentSize.width + inset.left + inset.right > bounds.width + 0.5
        let y = verticalIndicator && scrollsY && isScrollEnabled && contentSize.height + inset.top + inset.bottom > bounds.height + 0.5
        if super.showsHorizontalScrollIndicator != x { super.showsHorizontalScrollIndicator = x }
        if super.showsVerticalScrollIndicator != y { super.showsVerticalScrollIndicator = y }
    }
    package override var contentSize: CGSize { didSet { if contentSize != oldValue { setNeedsLayout() } } }
    package override var isScrollEnabled: Bool { didSet { if isScrollEnabled != oldValue { setNeedsLayout() } } }
    /// VoiceOver's scroll is the reader's, as a drag is, and supersedes a position Exact carries (`dropCarriedScroll`)
    package override func accessibilityScroll(_ direction: UIAccessibilityScrollDirection) -> Bool {
        scrollingNode?.dropCarriedScroll()
        return super.accessibilityScroll(direction)
    }
    /// The offset of a node's scroller or a route's viewport, which UIKit
    /// itself also sets (keeping it at the start as a safe area reaches the
    /// scroller and insets it): inside an animation UIKit has open it moves
    /// only if the last frame showed the node (`Presenter.moveOrPlace`). The
    /// session's viewport has no node and moves as UIKit sets it.
    package override var bounds: CGRect {
        get { super.bounds }
        set {
            guard Presenter.animationMayBeOpen, let node = scrollingNode, let presenter = node.presenter else { return super.bounds = newValue }
            presenter.moveOrPlace(node) { super.bounds = newValue }
        }
    }
    /// The insets the offset was last kept under (`insetsSettled`, or D14's writer through `kept(under:)`).
    private var keptInset = UIEdgeInsets.zero
    /// A list that opens at its end (`scroll-start: end`) or follows it, at
    /// its end, stays there as its insets change, as SwiftUI's
    /// `defaultScrollAnchor(.bottom)` keeps it; UIKit keeps the offset. A
    /// tab bar's inset reaches a grid after it opened at its newest row
    /// (r25's photos, at launch and as a viewer closes), and kept, the
    /// offset left that row under the bar.
    /// The kept insets move once Exact's own writes to a controller above settle (LLP 1123 D11), so
    /// the list compares the insets from before the writes with the settled ones, not a step's.
    package override func adjustedContentInsetDidChange() {
        super.adjustedContentInsetDidChange()
        setNeedsLayout()
        scrollingNode?.scrollInsetMoved(self)
        SafeAreaSteps.react(self) { [weak self] in self?.insetsSettled() }
    }

    private func insetsSettled() {
        let atEnd = restsAtEnd, was = keptInset
        keptInset = adjustedContentInset
        // the port is a `scroll` event's (`clientHeight`): one that changes without moving
        // the offset gets no `scrollViewDidScroll`, so its event is sent from here.
        (superview as? NodeView)?.queueScrollEvent()
        guard atEnd, !isTracking, !isDecelerating, was.bottom != keptInset.bottom else { return }
        contentOffset = PixelGrid.offset(CGPoint(x: contentOffset.x, y: end(under: keptInset)), in: self)
    }
    /// Whether it keeps its end as its insets change (`adjustedContentInsetDidChange`, and D14's writer, which moves
    /// the offset itself): a list that opens at its end or follows it, with content, not a horizontal collection, and
    /// not one whose author owns its scroll position.
    package var keepsEnd: Bool { keepsEnd(atRest: false) }
    /// `keepsEnd`, judged with `atRest` where UIKit reports the scroll view at rest and reports every animation it runs:
    /// then only the list's own scroll owns its position (`listOwnsScroll`), whatever records a reveal or a smooth write
    /// left behind.
    package func keepsEnd(atRest: Bool) -> Bool {
        guard let node = superview as? NodeView, node.props["scrollStart"] == "end" || node.props["scrollFollowEnd"] == "true" else { return false }
        let owned = atRest ? node.listOwnsScroll : node.authorOwnsScroll(self)
        return node.presenter?.collections.snapshot(node.id)?.horizontal != true && contentSize.height > 0 && !owned
    }
    /// The offset at which its content's end rests under `inset`.
    package func end(under inset: UIEdgeInsets) -> CGFloat { max(-inset.top, contentSize.height + inset.bottom - bounds.height) }
    /// Whether it keeps its end and rests there under the insets its offset was last kept under.
    package var restsAtEnd: Bool { restsAtEnd(atRest: false) }
    package func restsAtEnd(atRest: Bool) -> Bool { keepsEnd(atRest: atRest) && contentOffset.y >= end(under: keptInset) - 1 }
    /// Records that its offset is kept under `inset`, as D14's writer keeps it, so the reaction to
    /// the writer's insets keeps nothing again.
    package func kept(under inset: UIEdgeInsets) { keptInset = inset }
    /// Keeps `restsAtEnd` as it was across a change of its bounds' height by `growth` that moves no
    /// inset, as reach makes (`setReach`), until D14's writer keeps the offset under its insets.
    package func boundsGrew(by growth: CGFloat) { keptInset.bottom += growth }
    package override var accessibilityElements: [Any]? {
        get {
            super.accessibilityElements ?? scrollingNode?.presenter?.navigation.lifted?.accessibilityElements(of: self)
                ?? NodeView.emptyBoxAccessibility(in: self)
        }
        set { super.accessibilityElements = newValue }
    }
    package override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        // UIView's own checks, without its walk of the subviews: the walk
        // below visits each of them, and walking twice at every nested
        // scroller made a touch on a list walk its rows four times.
        guard !isHidden, isUserInteractionEnabled, alpha >= 0.01, self.point(inside: point, with: event) else { return nil }
        if let child = NodeView.hitChildren(in: self, at: point, with: event) { return child }
        // A paragraph's scroller with nothing to scroll is its node, as on the
        // web, where a click on it bubbles to the button around it: a
        // `UIScrollView` keeps a touch from the responder chain. CSS makes one
        // beside every `overflow-x: hidden` (`overflow-y` computes to `auto`),
        // so a truncating title is one. A scroller holding nodes (a page) keeps
        // its blank ground: its bounce, and the tap there that ends editing.
        if !hasRange, !subviews.contains(where: { $0 is NodeView }), let owner = superview as? NodeView { return owner }
        return self
    }

    /// Whether its content reaches past its port on an axis it scrolls, or a
    /// pull-to-refresh hangs above it.
    var hasRange: Bool {
        let inset = adjustedContentInset
        if scrollsX && contentSize.width + inset.left + inset.right > bounds.width + 0.5 { return true }
        if scrollsY && contentSize.height + inset.top + inset.bottom > bounds.height + 0.5 { return true }
        #if os(iOS)
        if refreshControl != nil { return true }
        #endif
        return false
    }

    package var scrollsX = true { didSet { if scrollsX != oldValue { setNeedsLayout() } } }
    package var scrollsY = true { didSet { if scrollsY != oldValue { setNeedsLayout() } } }
    #if os(tvOS)
    /// The offset the remote's last step scrolls to, and when it began
    /// (`RemoteTVOS.swift`): a step pressed during that animation goes on
    /// from there.
    var remoteStepTarget: (offset: CGPoint, at: CFTimeInterval)?
    #endif
    /// Let native body buttons (LLP 1069.011 D4) and tap-only invoker overlays
    /// yield to scrolling; UIKit normally keeps a UIControl's touch. Input-owning
    /// surfaces and other controls keep their existing rules.
    package override func touchesShouldCancel(in view: UIView) -> Bool {
        guard !CanvasInputs.owns(view) else { return false }
        if view is NativeButtonIOS { return true }
        if let button = view as? InvokerButton, !button.showsMenuAsPrimaryAction { return true }
        return super.touchesShouldCancel(in: view)
    }
    package override func gestureRecognizerShouldBegin(_ gesture: UIGestureRecognizer) -> Bool {
        if gesture === panGestureRecognizer {
            let velocity = panGestureRecognizer.velocity(in: self)
            let location = panGestureRecognizer.location(in: self)
            let translation = panGestureRecognizer.translation(in: self)
            if !admitsPan(velocity: velocity, translation: translation, start: CGPoint(x: location.x - translation.x, y: location.y - translation.y)) { return false }
        }
        #if os(iOS)
        if gesture === pinchGestureRecognizer, !admitsPinch(at: gesture.location(in: self)) { return false }
        #endif
        return super.gestureRecognizerShouldBegin(gesture)
    }
    /// The pan's own check, apart from UIKit's: `touch-action` intersected
    /// from the node hit at `start` through this container, then chaining.
    /// The direction is the velocity, or the movement that crossed the slop
    /// while UIKit has none yet, as a photo's yield reads it (LLP 1057.001
    /// rule 2), so the drag it steps aside from is taken here.
    package func admitsPan(velocity: CGPoint, translation: CGPoint, start: CGPoint, chain: Bool = true) -> Bool {
        let direction = velocity == .zero ? translation : velocity
        var view = hitTest(start, with: nil)
        if CanvasInputs.owns(view) { return false }
        // CSS intersects touch-action from the hit element through the
        // scroll container. It governs initial direction, not reversal.
        while let current = view {
            if let node = current as? NodeView, !node.allowsTouchPan(direction) { return false }
            if current === self { break }
            view = current.superview
        }
        if let owner = scrollingNode, !owner.allowsTouchPan(direction) { return false }
        return !chain || !handsOff(direction)
    }
    #if os(iOS)
    /// A zoomable scroller's pinch (LLP 1057.001.000 §3): rule 2, where
    /// `touch-action` from the node under the fingers through this container
    /// allows `pinch-zoom`, and never over a surface that owns its input.
    func admitsPinch(at point: CGPoint) -> Bool {
        var view = hitTest(point, with: nil)
        if CanvasInputs.owns(view) { return false }
        while let current = view {
            if let node = current as? NodeView, !node.allowsTouchPinch { return false }
            if current === self { break }
            view = current.superview
        }
        return scrollingNode?.allowsTouchPinch ?? true
    }
    #endif
    /// CSS's scroll chaining at a gesture's start (LLP 1070 G1, Q4 as ruled
    /// provisionally): under `overscroll-behavior: auto`, a drag that begins
    /// at this view's edge and pushes outward belongs to the enclosing
    /// scroller that can take it, as Chrome and Safari chain it, instead of
    /// UIKit's rubber band here. Only for a known direction, on an axis this
    /// view actually scrolls; `contain` keeps the band, `none` keeps the
    /// gesture with no band. Never mid-gesture: UIKit asks once, at begin.
    func handsOff(_ velocity: CGPoint) -> Bool {
        guard velocity != .zero, let owner = scrollingNode else { return false }
        let horizontal = abs(velocity.x) > abs(velocity.y)
        let behavior = owner.style[horizontal ? "overscroll_behavior_x" : "overscroll_behavior_y"]?.string ?? "auto"
        // One flag for both axes, written each gesture so a style that leaves
        // `none` gets its band back: off for this drag's `none`, or for an
        // axis this view scrolls under `none`.
        let still = { (axis: String) in owner.style["overscroll_behavior_\(axis)"]?.string == "none" }
        bounces = !(behavior == "none" || (scrollsX && still("x")) || (scrollsY && still("y")))
        #if os(iOS)
        if (owner.heightRecognizer as? SheetScrollGesture)?.takesEdge(in: self, direction: velocity) == true { return false }
        #endif
        return chains(velocity)
    }
    /// `handsOff`'s answer without its side effect: whether a drag in
    /// `velocity` begun now chains to an enclosing scroller (a descendant's
    /// recognizer asks it, LLP 1057.001 rule 2).
    package func chains(_ velocity: CGPoint) -> Bool {
        guard velocity != .zero, let owner = scrollingNode else { return false }
        let horizontal = abs(velocity.x) > abs(velocity.y)
        guard (owner.style[horizontal ? "overscroll_behavior_x" : "overscroll_behavior_y"]?.string ?? "auto") == "auto" else { return false }
        let i = adjustedContentInset
        let (at, low, high, scrolls) = horizontal
            ? (contentOffset.x, -i.left, contentSize.width + i.right - bounds.width, scrollsX)
            : (contentOffset.y, -i.top, contentSize.height + i.bottom - bounds.height, scrollsY)
        guard scrolls, high > low else { return false }
        // A finger moving toward +x pulls the content toward its start.
        let toward = horizontal ? velocity.x : velocity.y
        let atEdge = toward > 0 ? at <= low + 0.5 : at >= high - 0.5
        guard atEdge else { return false }
        // Only when an enclosing scroller can take that direction; with none,
        // the band here is all a finger can have.
        var up = superview
        while let current = up {
            if let outer = current as? UIScrollView, outer is ScrollView || outer is GroupedScroller {
                let o = outer.adjustedContentInset
                let (at, low, high, scrolls) = horizontal
                    ? (outer.contentOffset.x, -o.left, outer.contentSize.width + o.right - outer.bounds.width, (outer as? ScrollView)?.scrollsX ?? false)
                    : (outer.contentOffset.y, -o.top, outer.contentSize.height + o.bottom - outer.bounds.height, (outer as? ScrollView)?.scrollsY ?? (outer.superview as? NodeView)?.scrollsVertically ?? false)
                if scrolls && high > low && (toward > 0 ? at > low + 0.5 : at < high - 0.5) { return true }
                if scrolls, let node = (outer as? ScrollView)?.scrollingNode ?? (outer.superview as? NodeView),
                   (node.style[horizontal ? "overscroll_behavior_x" : "overscroll_behavior_y"]?.string ?? "auto") != "auto" { return false }
            }
            up = current.superview
        }
        return false
    }
    /// A touch that no node took — nothing focusable, nothing pressable —
    /// ends the editing and sends the keyboard away. UIKit does nothing
    /// here by itself; hand-built iOS apps add exactly this, a tap on the
    /// background that calls `endEditing`, and that is the reason it stays
    /// (LLP 1115.000 D9.4; LLP 1008 §9 gave a page's blur as the reason).
    package override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        // An inert button can still retain focus, for example between the
        // two taps of a double-tap recognizer. Its unhandled touch is not
        // blank ground. Check before forwarding to enclosing scroll views.
        var target = touches.first?.view
        while let view = target {
            if let node = view as? NodeView {
                if node.presenter?.contextRetainsFocus(node) == true { return }
                break
            }
            target = view.superview
        }
        super.touchesEnded(touches, with: event)
        // A blur is the session's (LLP 1035.001 D5): wherever it is mounted,
        // never the window's — found through the nearest node above a nested
        // scroller; the viewport itself has none above it and is its own.
        if let t = touches.first, bounds.contains(t.location(in: self)) {
            var above = superview
            while let current = above {
                if let node = current as? NodeView, let presenter = node.presenter { presenter.endEditing(); return }
                above = current.superview
            }
            endEditing(true)
        }
    }
}

#if !os(tvOS)
/// UIKit's pull-to-refresh control, below the scroller's `padding-top`
/// (@ref LLP 1010 §6.9, the one rule a padded list takes that CSS has no
/// word for: React Native's `progressViewOffset`, placed as its
/// `RCTRefreshControl` places it). The padding is in the content, so a header
/// laid over it would hide the spinner; here the control's top sits that far
/// down the scroller's port, behind the content, and the pull uncovers it
/// between the header and the first row, where it stays while the app
/// refreshes. UIKit lays the control out as the pull goes; after each
/// layout its frame moves to that place, which is a no-op once it is there.
/// With no padding, UIKit's own place.
final class PaddedRefreshControl: UIRefreshControl {
    override func layoutSubviews() {
        super.layoutSubviews()
        // On a node's own scroll, whose padding is in its content. A grouped
        // list's collection view holds the padding as an inset: UIKit's place.
        guard superview is ScrollView, let owner = superview?.superview as? NodeView else { return }
        let top = owner.padding("top")
        guard top != 0 else { return }
        let gap = convert(CGPoint(x: 0, y: top), from: owner).y
        if gap != 0 { frame = frame.offsetBy(dx: 0, dy: gap) }
    }
}
#endif

extension Presenter {
    func finishScrollPositions(restoring: Bool) {
        let ids = restoring ? scrollers.union(pendingScrolls) : pendingScrolls
        if restoring {
            // What the batch's `scrollIntoView`s will move, and the scrollers it writes `scrollTop` to.
            let names = Set(session?.pendingElementScrolls ?? [])
            let authored = (names.isEmpty ? [] : views.values.filter { $0.props["id"].map(names.contains) == true })
                + ids.compactMap { views[$0] }.filter { $0.pendingScrollTop != nil || $0.pendingScrollLeft != nil }
            for id in ids {
                guard let node = views[id] else { continue }
                // Navigation can resize a native port after its row projection.
                // Settle that layout before restoring or authoring an offset.
                if node.groupedOwner { node.scrollView?.layoutIfNeeded() }
                if !collections.owns(node.id) { node.restoreScrollPosition(authored: authored.contains { $0.isDescendant(of: node) }) }
            }
        }
        for id in ids { views[id]?.settleZoom() }
        for id in materialNodes {
            guard let node = views[id] else { continue }
            if let material = node.materialView { node.sendSubviewToBack(node.glassSlot ?? material) }
        }
        // Every capture is restored before any authored offset lands: a
        // list's scroll outside its mounted rows reports at once, and the
        // batch its feedback applies captures, restores and clears every
        // scroller's capture, which a follow-end scroller still waiting here
        // would read as having been at its end.
        writePendingScrolls(ids, laidOut: restoring)
    }

    /// Write the positions Exact carried across a change of its own outside a batch (`retainScrollPosition`) to
    /// scrollers laid out. No batch's `restoreScrollPositions` need follow when the covers change nothing, and one
    /// that came later would write a position over the reader's scroll made meanwhile by any means, VoiceOver's or
    /// the keyboard's among them.
    package func writeCarriedScrolls() {
        let carried = pendingScrolls.filter { views[$0].map { $0.extras?.pendingScrollTransfer == true && $0.hasScrollLayoutBox } ?? false }
        guard !carried.isEmpty else { return }
        for id in carried where views[id]?.groupedOwner == true { views[id]?.scrollView?.layoutIfNeeded() }
        writePendingScrolls(carried, laidOut: true)
    }

    /// Write `ids`' pending positions, laying a native list out first unless the caller has (`laidOut`). A position
    /// Exact carries waits while covers are still to be answered (`coversAnswerOwed`), checked before each write,
    /// since a layout or an earlier write can report them; it lands in the layout their answer brings.
    private func writePendingScrolls(_ ids: Set<UInt32>, laidOut: Bool) {
        for id in ids {
            guard let node = views[id] else { continue }
            if !laidOut, node.groupedOwner { node.scrollView?.layoutIfNeeded() }
            guard node.extras?.pendingScrollTransfer != true || !navigation.coversAnswerOwed else { continue }
            if node.pendingScrollTop != nil || node.pendingScrollLeft != nil { collections.userIntent(node.id) }
            node.applyPendingScroll()
        }
        pendingScrolls = pendingScrolls.filter { views[$0]?.pendingScrollTop != nil || views[$0]?.pendingScrollLeft != nil }
    }
}

extension NodeView {
    /// A capability supplies the only physical scroller for this node;
    /// its delegate still owns cells and forwards the shared callbacks.
    package func mountNativeScrollView(_ view: UIScrollView?, contentInsets: UIEdgeInsets = .zero) {
        if extras?.nativeScrollView !== view, let previous = extras?.nativeScrollView {
            retainScrollPosition(from: previous)
        }
        if view != nil || extras?.nativeScrollView != nil {
            more.nativeScrollContentInsets = view == nil ? .zero : contentInsets
        }
        guard extras?.nativeScrollView !== view else { return }
        if pageBand != .zero { setPageBand(.zero) } // the scroll view leaving gives its band back (LLP 1130 D3)
        if reach != .zero || reachGiven { more.reach = .zero; more.reachGiven = false } // and its reach, which its successor waits for anew (LLP 1123 D13)
        more.nativeScrollView = view
        more.nativeScrollDelegate = view?.delegate
        if scrollView == nil { presenter?.scrollers.remove(id) }
        else { presenter?.scrollers.insert(id) }
        scrollWritten = nil
        view?.contentInsetAdjustmentBehavior = .never
        view?.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        syncScroll()
        updateRefresh()
        applyAffordances()
        presenter?.navigation.scrollBackendChanged(self)
        presenter?.keyboardToolbars.backendChanged()
    }

    /// The reader's move supersedes a position Exact carries (`retainScrollPosition`), as an authored write does.
    package func dropCarriedScroll() {
        if extras?.pendingScrollTransfer == true { pendingScrollTop = nil; pendingScrollLeft = nil }
    }

    /// Runs `change`, an arranged pane's switch of `sv`'s inset adjustment behavior, keeping what D14 keeps across an
    /// inset change: UIKit keeps CSS `scrollTop` across a change of the adjusted inset but the offset across one of the
    /// behavior (LLP 1123, "Inset adoption"). A list that keeps its end and rests there keeps the end, which D14 puts
    /// first (an Exact scroller's; a grouped list's collection keeps `scrollTop`), a scroller in motion keeps its own
    /// position, with whatever a windowed list holds of it, and a switch that moves no inset is corrected not at all. Outside
    /// a batch the write is clamped to the new range, as D14's writer clamps; inside one it is clamped when the batch
    /// ends, so the batch's restore applies a row shift to the whole move first, and the batch's captures of the offset
    /// (`followedScroll`, `beforeLayoutScroll`) and a windowed list's (`ListHosting.portMoved`) move with the write, as a
    /// page band's move them.
    package func keepingScrollTop(of sv: UIScrollView, _ change: () -> Void) {
        let css = scrollTopInset(sv), was = sv.adjustedContentInset, before = sv.contentOffset.y
        // In motion as UIKit reports it (an arrangement's iOS reports its animations), or under a windowed list's own
        // smooth correction, which UIKit does not drive
        let moving = sv.scrollsItself || presenter?.collections.animating.contains(id) == true
        // Judged at rest, as it is wherever the end is kept: arrangements run where UIKit reports its animations
        let atEnd = (sv as? ScrollView)?.restsAtEnd(atRest: true) == true, mark = presenter?.collections.portMark(id)
        change()
        let i = sv.adjustedContentInset
        guard !moving, i != was || scrollTopInset(sv) != css else { return }
        // At the end it moves as far as the inset moved the end, so a batch's row shift still applies after it
        let end = { (i: UIEdgeInsets) in max(-i.top, sv.contentSize.height + i.bottom - sv.bounds.height) }
        let y = atEnd ? before + end(i) - end(was) : before + css - scrollTopInset(sv)
        let batch = presenter?.applying == true
        sv.contentOffset = PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: y), in: sv, clamped: !batch)
        // Clamped once the batch's restores are done, unless one wrote the offset since (a restore that moves it clamps)
        // or the scroller is then in motion, the batch's smooth `scrollTop` among them
        if batch, let presenter {
            let written = sv.contentOffset
            presenter.afterBatch { [weak self, weak sv] in
                guard let self, let sv, sv.contentOffset == written, !sv.scrollsItself,
                      self.presenter?.collections.animating.contains(id) != true else { return }
                let clamped = PixelGrid.offset(written, in: sv)
                if clamped != written { sv.contentOffset = clamped }
            }
        }
        let moved = sv.contentOffset.y - before
        if var followed = followedScroll { followed.top += moved; followed.port -= moved; followedScroll = followed }
        if var saved = beforeLayoutScroll { saved.offset.y += moved; saved.topInset -= moved; beforeLayoutScroll = saved }
        if let mark { presenter?.collections.portMoved(id, since: mark) }
    }

    /// `sv`'s adjusted inset moved: while `sv` is this node's scroll view and the node an arranged pane joined to UIKit's
    /// adjustment, the arrangement takes a pass, which publishes its owners' values and the route's covers anew.
    package func scrollInsetMoved(_ sv: UIScrollView) {
        #if os(iOS)
        if #available(iOS 27.1, *), scrollView === sv { presenter?.arrangements.scrollInsetMoved(id) }
        #endif
    }

    /// Whether a scroll newer than a position Exact carries is under way on `sv`: one the scroll view makes itself
    /// (`scrollsItself`), an element reveal, or a list's smooth correction
    package func scrollUnderWay(_ sv: UIScrollView) -> Bool {
        sv.scrollsItself || presenter?.elementScrolls.revealing(sv) != nil || presenter?.collections.animating.contains(id) == true
    }

    /// A replacement keeps the logical offset; an authored write takes priority. A position saved while the scroller had
    /// no port (`hiddenScroll`) is already the reader's, and `applyPendingScroll` restores it into whichever scroll view
    /// the node has then: nothing is carried over it.
    package func retainScrollPosition(from view: UIScrollView) {
        guard hiddenScroll == nil else { return }
        let authored = extras?.pendingScrollTransfer != true && (pendingScrollTop != nil || pendingScrollLeft != nil)
        if pendingScrollTop == nil { pendingScrollTop = Double(view.contentOffset.y + scrollTopInset(view)) }
        if pendingScrollLeft == nil { pendingScrollLeft = Double(view.contentOffset.x + scrollLeftInset(view)) }
        more.pendingScrollTransfer = !authored
        presenter?.pendingScrolls.insert(id)
    }

    /// UIKit owns a native list's row geometry, including rows offscreen.
    private func scrollRect(_ node: NodeView, in sv: UIScrollView) -> CGRect? {
        if groupedOwner { return presenter?.groupedLists?.projectedRect(for: node, in: sv) }
        return node.isDescendant(of: sv) && !node.isHidden ? node.convert(node.bounds, to: sv) : nil
    }

    func captureScrollPosition(focused: NodeView?) {
        // UIKit may have pending row geometry and its own offset adjustment.
        // Snapshot both from the same layout before preserving the reader.
        if groupedOwner { scrollView?.layoutIfNeeded() }
        beforeLayoutScroll = scrollView.map { ($0.contentOffset, scrollTopInset($0), scrollLeftInset($0), extras?.nativeScrollContentInsets.top ?? 0) }
        followedScroll = nil
        readingAnchors.removeAll(keepingCapacity: true)
        scrollAnchor = nil
        guard props["scrollFollowEnd"] == "true", let sv = scrollView else {
            activeReadingAnchor = nil; anchoredScrollTop = nil; retainedScrollTop = nil
            if let sv = scrollView {
                if groupedOwner {
                    // UIKit quantizes offsets independently of fractional insets.
                    // A rounding difference at the start is not a reading anchor.
                    if sv.contentOffset.y + sv.adjustedContentInset.top > 0.5 { readingAnchors = presenter?.groupedLists?.scrollAnchors(for: id) ?? [] }
                } else { captureScrollAnchor(sv, focused: focused) }
            }
            return
        }
        let maximum = max(-sv.adjustedContentInset.top, sv.contentSize.height + sv.adjustedContentInset.bottom - sv.bounds.height)
        // A retained route can gain height when another route hides the keyboard.
        // That clamp is not the reader choosing the end. Keep its intended offset
        // until the returning viewport can fit it, or the reader scrolls again.
        // (the anchor a value this host set, the offset UIKit's read-back of it: PixelGrid.same)
        if !(anchoredScrollTop.map { PixelGrid.same($0, sv.contentOffset.y, in: sv) } ?? false) { retainedScrollTop = nil }
        // An animated follow of the end (below) is still the end, mid-flight.
        let capturedTop = retainedScrollTop ?? sv.contentOffset.y
        followedScroll = (capturedTop,
                          (retainedScrollTop == nil && sv.contentOffset.y >= maximum - 1) || followingEndAnimated,
                          abs(capturedTop + sv.adjustedContentInset.top) <= 0.5,
                          scrollPortInsets(sv).top)
        guard let followedScroll, !followedScroll.end, followedScroll.top + sv.adjustedContentInset.top > 0.5 else { return }
        // A scroll by the reader invalidates the prior choice. Anchoring's
        // own adjustment does not: keep the same surviving row across batches.
        if anchoredScrollTop.map({ PixelGrid.same($0, sv.contentOffset.y, in: sv) }) == true, let node = activeReadingAnchor,
           presenter?.views[node.id] === node, scrollRect(node, in: sv) != nil, !node.isHidden {
            let rect = scrollRect(node, in: sv) ?? .zero
            if rect.width > 0, rect.height > 0, rect.intersects(sv.bounds) { readingAnchors.append((node, rect.minY)) }
        }
        // Prefer the first fully visible box; descend into a partially visible
        // one. This keeps a message stable when rows above it change height.
        // Coordinates are in the scroll view's content space, not the window.
        func anchors(in view: UIView) {
            for case let node as NodeView in view.subviews where !node.isHidden {
                let rect = node.convert(node.bounds, to: sv)
                guard rect.width > 0, rect.height > 0, rect.intersects(sv.bounds) else { continue }
                if !sv.bounds.contains(rect) { anchors(in: node.container) }
                readingAnchors.append((node, rect.minY))
            }
        }
        // Keep later visible candidates too: a deleted anchor cannot hold the
        // reader's position, but the next surviving message still can.
        if groupedOwner { readingAnchors = presenter?.groupedLists?.scrollAnchors(for: id) ?? [] }
        else { anchors(in: sv) }
    }
    /// `authored`: the batch asks for a scroll of it (a `scrollIntoView` of
    /// something in it, which comes after the batch), which wins over the end-follow.
    func restoreScrollPosition(authored: Bool = false) {
        defer { followedScroll = nil; readingAnchors.removeAll(keepingCapacity: true); scrollAnchor = nil }
        guard props["scrollFollowEnd"] == "true", let sv = scrollView else {
            if let sv = scrollView {
                if groupedOwner, let anchor = readingAnchors.first(where: { presenter?.views[$0.node.id] === $0.node && scrollRect($0.node, in: sv) != nil }),
                   let rect = scrollRect(anchor.node, in: sv) {
                    let shift = rect.minY - anchor.y
                    let i = sv.adjustedContentInset
                    // Native layout may already have moved the offset. Restore the captured reading position, counting the
                    // row shift once, and at rest keep CSS `scrollTop` across a change of the port's start inset, as UIKit
                    // keeps a grouped collection's row in its port when a large title turns inline (LLP 1123 D14, "Inset
                    // adoption"); the list's own padding is content, across which the row stays put
                    let port = { (inset: CGFloat, room: CGFloat) in inset - room }
                    let resting = !sv.scrollsItself && !authoredScrollAnimating
                    let moved = resting ? beforeLayoutScroll.map { port($0.topInset, $0.room) - port(scrollTopInset(sv), extras?.nativeScrollContentInsets.top ?? 0) } ?? 0 : 0
                    let top = (beforeLayoutScroll?.offset.y ?? sv.contentOffset.y) + moved + shift
                    let y = sv.isTracking || sv.isDecelerating ? top : min(max(top, -i.top), max(-i.top, sv.contentSize.height + i.bottom - sv.bounds.height))
                    // An element reveal heads for where its target now is instead (`IntoView.swift`).
                    if presenter?.elementScrolls.revealing(sv) != nil { presenter?.elementScrolls.retarget(sv) }
                    else if y != sv.contentOffset.y { sv.setContentOffset(PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: y), in: sv, clamped: !(sv.isTracking || sv.isDecelerating)), animated: false) }
                } else if !groupedOwner { restoreScrollAnchor(sv) }
            }
            return
        }
        let minimum = -sv.adjustedContentInset.top
        let maximum = max(minimum, sv.contentSize.height + sv.adjustedContentInset.bottom - sv.bounds.height)
        // The app's scroll owns the offset until it settles: the end is not
        // followed meanwhile, `scrollFollowEnd` turning true included (no
        // capture: the port stays where it is), and once it settles only
        // where it rests at the end (LLP 1070.000 §15).
        let owned = authored || authorOwnsScroll(sv)
        let port = scrollPortInsets(sv).top
        let prior = followedScroll ?? (owned ? (top: sv.contentOffset.y, end: false, start: false, port: port) : (top: maximum, end: true, start: false, port: port))
        let follows = prior.end && !owned
        // At rest at the start, UIKit's new minimum follows an inset change.
        // A captured physical offset would replay the obsolete inset instead.
        let restingStart = groupedOwner && prior.start && !(sv.isTracking || sv.isDragging || sv.isDecelerating)
        var top = restingStart ? minimum : prior.top
        activeReadingAnchor = nil
        if !prior.end, let anchor = readingAnchors.first(where: {
            presenter?.views[$0.node.id] === $0.node && scrollRect($0.node, in: sv) != nil
        }) {
            activeReadingAnchor = anchor.node
            top += (scrollRect(anchor.node, in: sv)?.minY ?? anchor.y) - anchor.y
        }
        // At rest mid-list, CSS `scrollTop` is kept across a change of the port's start inset, as UIKit keeps it (LLP 1123
        // D14): not at the end it was at, which D14 puts first, nor while the scroller moves, by the reader or by an
        // animation UIKit or a smooth write runs, which keep the offset as they find it
        let moving = sv.isTracking || sv.isDragging || sv.isDecelerating
        let kept = restingStart || prior.end || sv.scrollsItself || authoredScrollAnimating ? top : top + prior.port - port
        let y = Self.followedTop(current: sv.contentOffset.y, minimum: minimum, maximum: maximum, end: follows, top: kept,
                                 moving: moving)
        let inactive = window == nil || presenter?.navigation.isInactiveRoute(containing: self, layout: true) == true
        retainedScrollTop = !prior.end && kept > maximum && (inactive || retainedScrollTop != nil) ? kept : nil
        // While the reader's finger is down or the fling is running, an
        // absolute write would cut the pan, the deceleration or the rubber
        // band (a batch every 250 ms yanked a bottom overscroll back to the
        // end, mid-drag). Follow the end once the interaction is over, and
        // keep a surviving row in place by moving the offset by its shift
        // only, without clamping, as UIKit's own contentOffsetAdjustment does.
        if sv.isTracking || sv.isDecelerating {
            if follows { followsEndAfterInteraction = true }
            else if top != prior.top {
                if groupedOwner {
                    if sv.contentOffset.y != top { sv.contentOffset = PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: top), in: sv, clamped: false) }
                }
                else { sv.contentOffset = PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: sv.contentOffset.y + top - prior.top), in: sv, clamped: false) }
            }
            anchoredScrollTop = sv.contentOffset.y
            return
        }
        if presenter?.elementScrolls.revealing(sv) != nil {
            // An element reveal heads for where its target now is instead (`IntoView.swift`).
            presenter?.elementScrolls.retarget(sv)
        } else if authoredScrollAnimating {
            // A smooth `scrollTop` write runs on to its target, as UIKit's own animation does.
        } else if sv.contentOffset.y != y {
            // `scroll-behavior: smooth`: an end that moved down (a message
            // appended) is followed with UIKit's scroll animation, as a
            // smooth `scrollTop` write is; a shrink or a jump up lands at once.
            let animate = follows && y > sv.contentOffset.y && style["scroll_behavior"]?.string == "smooth" && !ExactEnv.agentFreezes && window != nil
            followingEndAnimated = animate
            sv.setContentOffset(PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: y), in: sv), animated: animate)
        }
        // UIKit quantizes the assigned offset. Compare its actual stored value
        // next time so that rounding cannot masquerade as a reader's scroll.
        anchoredScrollTop = sv.contentOffset.y
    }


    /// `instant`: a page that arrived (LLP 1153 D5.1), which `scroll-behavior: smooth` does not animate.
    func applyPendingScroll(instant: Bool = false) {
        // Hold a saved position, and a position written meanwhile, until the port has a size. A pane UIKit shows again,
        // or one UIKit hid when its arrangement ends, stays covered whole, laid out as `display: none`, until the covers
        // that follow are delivered, and a write before then would clamp to the top.
        if hiddenScroll != nil, let sv = scrollView, sv.bounds.isEmpty, hasScrollLayoutBox { return }
        defer { pendingScrollTop = nil; pendingScrollLeft = nil }
        guard let sv = scrollView else { return }
        // A position Exact carries (`retainScrollPosition`) gives way to a scroll under way, before anything here stops it
        if extras?.pendingScrollTransfer == true, scrollUnderWay(sv) { return }
        // An authored position takes over from a smooth correction.
        if (pendingScrollTop != nil || pendingScrollLeft != nil), presenter?.collections.animating.contains(id) == true {
            presenter?.collections.stopAnimation(id)
        }
        // It replaces an element reveal, as on the Mac (LLP 1070.000 §14).
        if pendingScrollTop != nil || pendingScrollLeft != nil { presenter?.elementScrolls.cutReveal(sv) }
        if pendingScrollTop != nil { retainedScrollTop = nil }
        guard hasScrollLayoutBox else {
            if hiddenScroll == nil {
                hiddenScroll = beforeLayoutScroll.map { CGPoint(x: $0.offset.x + $0.leftInset, y: $0.offset.y + $0.topInset) } ?? .zero
            }
            return
        }
        if let saved = hiddenScroll {
            hiddenScroll = nil
            writeScrollPosition(in: sv, left: Double(saved.x), top: Double(saved.y), animated: false, titleSlack: false)
            // Keep the restored position over one carried from another scroll view (a replaced backend, the title's
            // new scroller), which is that view's, not the reader's
            if extras?.pendingScrollTransfer == true { return }
        }
        guard pendingScrollTop != nil || pendingScrollLeft != nil else { return }
        // `scroll-behavior: smooth` (CSS) animates a prop write, never a
        // reader's own scroll. Under the agent's frozen clock it lands at once
        // (as a modal presents, LLP 1035.003 D5), so `layout` reads the target.
        let smooth = !instant && extras?.pendingScrollTransfer != true && style["scroll_behavior"]?.string == "smooth" && !ExactEnv.agentFreezes
        writeScrollPosition(in: sv, left: pendingScrollLeft, top: pendingScrollTop, animated: smooth, titleSlack: true)
    }

    /// Convert a logical write only after native layout. UIKit can
    /// invalidate its offset when that write collapses the navigation title;
    /// settle that one inset transition and apply the same intent once more.
    private func writeScrollPosition(in sv: UIScrollView, left: Double?, top: Double?, animated: Bool, titleSlack: Bool) {
        func target() -> CGPoint {
            let i = sv.adjustedContentInset, inset = scrollTopInset(sv)
            // CSS ends where a large title rests collapsed.
            let slack = titleSlack && scrollOrigin > 0 ? max(0, i.top - scrollCollapsed) : 0
            let y = top.map { CGFloat($0) - inset == sv.contentOffset.y ? sv.contentOffset.y : min(max(CGFloat($0) - inset, -i.top), max(-i.top, sv.contentSize.height + i.bottom - sv.bounds.height - slack)) } ?? sv.contentOffset.y
            let leftInset = scrollLeftInset(sv)
            let x = left.map { CGFloat($0) - leftInset == sv.contentOffset.x ? sv.contentOffset.x : min(max(CGFloat($0) - leftInset, -i.left), max(-i.left, sv.contentSize.width + i.right - sv.bounds.width)) } ?? sv.contentOffset.x
            // On the pixel grid inside the range, so the content it scrolls
            // stays on it and UIKit does not clamp it off (`PixelGridIOS.swift`).
            let s = presenter?.gridScale ?? 1
            return CGPoint(x: PixelGrid.clamp(x, -i.left, max(-i.left, sv.contentSize.width + i.right - sv.bounds.width), scale: s),
                           y: PixelGrid.clamp(y, -i.top, max(-i.top, sv.contentSize.height + i.bottom - sv.bounds.height - slack), scale: s))
        }
        let initial = target()
        guard sv.contentOffset != initial else { return }
        let inset = sv.adjustedContentInset.top
        authoredScrollAnimating = animated
        sv.setContentOffset(initial, animated: animated)
        if !animated, sv.adjustedContentInset.top != inset {
            sv.layoutIfNeeded()
            let settled = target()
            if sv.contentOffset != settled { sv.setContentOffset(settled, animated: false) }
        }
    }
    /// A `refresh` handler on a scroll container is UIKit's pull-to-refresh:
    /// the control fires the event; the app's `refreshing` going false ends it.
    /// The control is on the scroller that shows (LLP 1115.000 D11.3): a
    /// grouped list's collection view, which a finger pulls, never the
    /// hidden scroll beneath it. The list asks again when its collection
    /// view comes and goes.
    package func updateRefresh() {
        // tvOS has no refresh control.
        #if !os(tvOS)
        guard let sv = scrollView else { return }
        if handlers.contains("refresh") {
            if sv.refreshControl == nil {
                let control = PaddedRefreshControl()
                control.addTarget(self, action: #selector(pulledToRefresh), for: .valueChanged)
                sv.refreshControl = control
                sv.panGestureRecognizer.addTarget(self, action: #selector(refreshPullChanged(_:)))
            }
            // One pull, one refresh: a control ended while the finger still
            // holds the list past its threshold fires again in the same pull
            // (an app that refreshes in a frame or two fired seven times in
            // one). It ends when the finger lifts (`refreshPullChanged`).
            if props["refreshing"] != "true", let control = sv.refreshControl, control.isRefreshing, !sv.isTracking {
                control.endRefreshing()
            }
        } else if sv.refreshControl != nil {
            sv.refreshControl = nil
        }
        #endif
    }
    #if !os(tvOS)
    @objc func pulledToRefresh() {
        presenter?.refresh(id)
        // An app that starts nothing leaves `refreshing` false: end promptly.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) { [weak self, token = incarnation] in if self?.incarnation == token { self?.updateRefresh() } }
    }
    @objc func refreshPullChanged(_ pan: UIPanGestureRecognizer) {
        guard pan.state == .ended || pan.state == .cancelled || pan.state == .failed,
              props["refreshing"] != "true", let control = scrollView?.refreshControl, control.isRefreshing else { return }
        control.endRefreshing()
    }
    #endif
}

extension UIScrollView {
    /// Whether the scroll view is moving itself: the reader's drag and its deceleration, or a scroll it animates (a
    /// status-bar tap's, a hardware keyboard's, an animated `setContentOffset` or `scrollRectToVisible`, iOS 17.4's
    /// [`isScrollAnimating`](https://developer.apple.com/documentation/uikit/uiscrollview/isscrollanimating)). Such a
    /// scroll is newer than a position Exact carries (`retainScrollPosition`), which gives way to it.
    var scrollsItself: Bool {
        if isTracking || isDragging || isDecelerating { return true }
        if #available(iOS 17.4, tvOS 17.4, *) { return isScrollAnimating }
        return false
    }
}
#endif
