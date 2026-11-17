// A zoomable scroll container on UIKit (@ref LLP 1057.001.000 §5): the
// scroll view's own zoom, so the pinch, the bounce past the range and the
// deceleration are UIKit's. UIKit scales the view `viewForZooming` returns
// and sizes the content to it. That view is a proxy here, empty and the
// child's laid-out size: the child's transform is the host's
// (`applyTransform`), and a view two writers transform shows whichever wrote
// last. The child is drawn from the zoom through `hostPlacement`; the extent
// and the centring are D3's, with padding and margins unscaled, so the
// content size is set here after UIKit's (`fitScroll`).
#if os(iOS) || os(tvOS)
import UIKit

/// A zoomable scroller's zoom: the proxy UIKit scales and the child drawn
/// from it.
final class ScrollZoom: NSObject {
    let proxy = UIView()
    weak var child: NodeView?
    /// The zooms reported since the last gesture began. A write of one is the
    /// app mirroring the gesture back as it ran, and moves nothing (D2).
    var reported: [Double] = []
    #if os(iOS)
    var doubleTap: UITapGestureRecognizer?
    /// A press waiting to learn whether its tap is a double tap's first (§3 rule 5).
    var heldPress: (() -> Void)?
    /// The double tap recognized this turn: the second tap's own press is dropped.
    var doubled = false
    #endif
    override init() {
        super.init()
        proxy.isUserInteractionEnabled = false
    }
}

extension NodeView {
    /// `touch-action` allows a pinch to zoom here (`auto`, `manipulation` or `pinch-zoom`).
    var allowsTouchPinch: Bool {
        let action = style["touch_action"]?.string ?? "auto"
        return action == "auto" || action == "manipulation" || action.split(separator: " ").contains("pinch-zoom")
    }
    /// D3 in `sv`'s port (`ScrollZoom.swift`).
    func zoomPlace(_ z: CGFloat, child: NodeView, in sv: UIScrollView) -> (shift: CGPoint, used: CGSize) {
        zoomPlace(z, child: child, frame: child.laidOutFrame, port: scrollPort(sv).size)
    }

    /// How far the content reaches under the zoom, which `fitScroll` sizes the scroller to.
    var zoomExtent: CGSize? {
        guard let child = extras?.zoom?.child, let sv = scroll else { return nil }
        return zoomPlace(sv.zoomScale, child: child, in: sv).used
    }

    /// The scroller's one element child, or nil.
    private var zoomChild: NodeView? {
        guard let sv = scroll else { return nil }
        var found: NodeView?
        for case let node as NodeView in sv.subviews {
            if found != nil { return nil }
            found = node
        }
        return found
    }

    /// Whether a gesture or its motion holds the zoom: a write waits for rest (D2).
    func zoomHeld(_ sv: UIScrollView) -> Bool {
        sv.isZooming || sv.isZoomBouncing || sv.isTracking || sv.isDragging || sv.isDecelerating
    }

    /// After a batch (`restoreScrollPositions`), before its offsets: the
    /// range, the child, and a `scrollZoom` written, about the port's centre.
    func settleZoom() {
        guard let range = zoomRange else {
            if extras?.zoom != nil { dropZoom() }
            return
        }
        guard let sv = scroll, sv.scrollsX, sv.scrollsY, let child = zoomChild, hasScrollLayoutBox else { return dropZoom() }
        let zoom: ScrollZoom
        if let made = extras?.zoom { zoom = made } else {
            zoom = ScrollZoom()
            more.zoom = zoom
            sv.insertSubview(zoom.proxy, at: 0)
            #if os(iOS)
            installZoomDoubleTap(zoom, in: sv)
            #endif
        }
        // A replaced child keeps the zoom.
        if zoom.child !== child {
            if let old = zoom.child { old.zoomFactor = 1; old.zoomShift = .zero; old.applyTransform() }
            zoom.child = child
        }
        let before = (z: sv.zoomScale, offset: sv.contentOffset)
        if sv.minimumZoomScale != range.lowerBound { sv.minimumZoomScale = range.lowerBound }
        if sv.maximumZoomScale != range.upperBound { sv.maximumZoomScale = range.upperBound }
        var target = min(max(before.z, range.lowerBound), range.upperBound), animated = false
        let held = zoomHeld(sv)
        if !held, let asked = extras?.pendingZoom {
            extras?.pendingZoom = nil
            if !zoom.reported.contains(asked) {
                zoom.reported.removeAll()
                target = min(max(CGFloat(asked), range.lowerBound), range.upperBound)
                animated = style["scroll_behavior"]?.string == "smooth" && !ExactEnv.agentFreezes && window != nil
            }
        }
        guard !held, target != before.z || target != sv.zoomScale else { return placeZoom() }
        let port = scrollPort(sv)
        moveZoom(sv, child: child, from: before, to: target, focal: CGPoint(x: port.midX - sv.bounds.minX, y: port.midY - sv.bounds.minY), animated: animated)
    }

    /// To zoom `z`, keeping the child's point under `focal` (in the scroller's
    /// visible bounds, from their top left) where it was at `before`.
    func moveZoom(_ sv: UIScrollView, child: NodeView, from before: (z: CGFloat, offset: CGPoint), to z: CGFloat, focal: CGPoint, animated: Bool) {
        let lead = child.laidOutFrame.origin, was = zoomPlace(before.z, child: child, in: sv).shift
        let u = CGPoint(x: (before.offset.x + focal.x - lead.x - was.x) / before.z, y: (before.offset.y + focal.y - lead.y - was.y) / before.z)
        let apply = { [weak self] in
            guard let self else { return }
            sv.zoomScale = z
            self.placeZoom()
            let shift = self.zoomPlace(z, child: child, in: sv).shift, i = sv.adjustedContentInset, s = self.presenter?.gridScale ?? 1
            let x = lead.x + shift.x + u.x * z - focal.x, y = lead.y + shift.y + u.y * z - focal.y
            sv.contentOffset = CGPoint(x: PixelGrid.clamp(x, -i.left, max(-i.left, sv.contentSize.width + i.right - sv.bounds.width), scale: s),
                                       y: PixelGrid.clamp(y, -i.top, max(-i.top, sv.contentSize.height + i.bottom - sv.bounds.height), scale: s))
        }
        guard animated else { return apply() }
        authoredScrollAnimating = true
        UIView.animate(withDuration: 0.3, delay: 0, options: [.curveEaseInOut, .beginFromCurrentState], animations: apply) { [weak self, token = incarnation] _ in
            guard let self, self.incarnation == token else { return }
            self.authoredScrollAnimating = false
            self.scrollMayHaveEnded(sv)
        }
    }

    /// The child drawn at the zoom, the extent that gives, and the proxy
    /// over where the child shows, which UIKit's next step zooms from.
    func placeZoom() {
        guard let zoom = extras?.zoom, let child = zoom.child, let sv = scroll else { return }
        let z = sv.zoomScale, shift = zoomPlace(z, child: child, in: sv).shift
        if child.zoomFactor != z || child.zoomShift != shift {
            child.zoomFactor = z; child.zoomShift = shift
            child.applyTransform()
        }
        fitScroll()
        let f = child.laidOutFrame
        if zoom.proxy.bounds.size != f.size { zoom.proxy.bounds.size = f.size }
        let centre = CGPoint(x: f.minX + shift.x + f.width * z / 2, y: f.minY + shift.y + f.height * z / 2)
        if zoom.proxy.center != centre { zoom.proxy.center = centre }
    }

    /// Not zoomable any more: the child as layout placed it.
    func dropZoom() {
        guard let zoom = extras?.zoom else { return }
        if let sv = scroll {
            if sv.zoomScale != 1 { sv.zoomScale = 1 }
            sv.minimumZoomScale = 1; sv.maximumZoomScale = 1
            #if os(iOS)
            if let tap = zoom.doubleTap { sv.removeGestureRecognizer(tap) }
            #endif
        }
        extras?.zoom = nil
        extras?.pendingZoom = nil
        zoom.proxy.removeFromSuperview()
        if let child = zoom.child, child.zoomFactor != 1 || child.zoomShift != .zero {
            child.zoomFactor = 1; child.zoomShift = .zero
            child.applyTransform()
        }
        fitScroll()
    }

    /// The agent's `tap … pinch <scale>` where UIKit synthesizes no pinch:
    /// the zoom times `scale` within the range, about `focal` (a window
    /// point), as the pinch would leave it at rest.
    func recognizedZoomPinch(_ scale: Double, focal: CGPoint) -> [String: Any] {
        guard scale.isFinite, scale > 0 else { return ["error": "pinch: expected a positive finite scale"] }
        guard let sv = scroll, let range = zoomRange, let child = extras?.zoom?.child else { return ["error": "pinch: #\(id) does not zoom"] }
        let target = min(max(sv.zoomScale * CGFloat(scale), range.lowerBound), range.upperBound), at = sv.convert(focal, from: nil)
        extras?.zoom?.reported.removeAll()
        moveZoom(sv, child: child, from: (sv.zoomScale, sv.contentOffset), to: target, focal: CGPoint(x: at.x - sv.bounds.minX, y: at.y - sv.bounds.minY), animated: false)
        scrollMayHaveEnded(sv)
        return ["tapped": Int(id), "pinch": scale, "delivery": "recognized", "scrollZoom": Double(sv.zoomScale)]
    }

    package func viewForZooming(in scrollView: UIScrollView) -> UIView? {
        scrollView === scroll ? extras?.zoom?.proxy : nil
    }
    package func scrollViewWillBeginZooming(_ scrollView: UIScrollView, with view: UIView?) {
        extras?.zoom?.reported.removeAll()
    }
    package func scrollViewDidZoom(_ scrollView: UIScrollView) {
        guard scrollView === scroll, extras?.zoom != nil else { return }
        placeZoom()
        presenter?.transformGeometry.changed()
        repaintThrough()
        scrollEndMoved(scrollView)
        if presenter?.applying == false && !dispatchingScrollEvent && scrollView.isZooming { sendScrollEvent() } else { queueScrollEvent() }
    }
    package func scrollViewDidEndZooming(_ scrollView: UIScrollView, with view: UIView?, atScale scale: CGFloat) {
        guard scrollView === scroll else { return }
        // A write the gesture held, now at rest; UIKit's bounce has ended.
        DispatchQueue.main.async { [weak self, token = incarnation] in
            guard let self, self.incarnation == token, let sv = self.scroll, !self.zoomHeld(sv) else { return }
            if self.extras?.pendingZoom != nil { self.settleZoom() }
            self.scrollMayHaveEnded(sv)
        }
    }
}

#if os(iOS)
/// UIKit's double tap for zoom (D5): above the minimum to the minimum, at it
/// to twice it (within the range), about the tap. It begins only where the
/// path's `touch-action` is `auto` (`manipulation` turns it off, as in a
/// browser), outside a control and an input-owning surface, and where no
/// authored `dblclick` takes it.
extension ScrollZoom: UIGestureRecognizerDelegate {
    /// About as long as UIKit's double tap waits for its second tap.
    static let doubleTapWait = 0.35
    func gestureRecognizer(_ g: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        doubleTapAdmits(touch.view)
    }
    func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }
    /// Whether a double tap may begin on `view`.
    func doubleTapAdmits(_ view: UIView?) -> Bool {
        guard !CanvasInputs.owns(view), let scroller = proxy.superview else { return false }
        var at = view
        while let current = at {
            if let node = current as? NodeView,
               node.handlers.contains("dblclick") || node.isZoomControl || (node.style["touch_action"]?.string ?? "auto") != "auto" { return false }
            if current === scroller { break }
            at = current.superview
        }
        guard at != nil, let owner = scroller.superview as? NodeView else { return false }
        return (owner.style["touch_action"]?.string ?? "auto") == "auto" && !owner.handlers.contains("dblclick")
    }
    @objc func doubleTapped(_ g: UITapGestureRecognizer) {
        guard g.state == .ended, let scroller = proxy.superview, let owner = scroller.superview as? NodeView else { return }
        heldPress = nil
        doubled = true
        DispatchQueue.main.async { [weak self] in self?.doubled = false }
        owner.zoomDoubleTapped(at: g.location(in: scroller))
    }
}

extension NodeView {
    /// A control fires its press at once, and a double tap never begins on one.
    var isZoomControl: Bool {
        isButton || isSurfaceControl || kind == "control" || defaultLink != nil || NodeView.controlRoles.contains(props["accessibilityRole"] ?? "")
    }
    func installZoomDoubleTap(_ zoom: ScrollZoom, in sv: ScrollView) {
        let tap = UITapGestureRecognizer(target: zoom, action: #selector(ScrollZoom.doubleTapped(_:)))
        tap.numberOfTapsRequired = 2; tap.delaysTouchesEnded = false; tap.cancelsTouchesInView = false
        tap.delegate = zoom
        sv.addGestureRecognizer(tap)
        zoom.doubleTap = tap
    }
    /// The agent's `dblclick touch` where UIKit synthesizes no double tap:
    /// the zoomable scroller whose double tap a finger at `window` would
    /// begin zooms as it would, or nil.
    static func zoomDoubleTap(_ hit: UIView?, at window: CGPoint) -> NodeView? {
        var at = hit
        while let current = at, !(current is ScrollView && (current.superview as? NodeView)?.extras?.zoom != nil) { at = current.superview }
        guard let sv = at as? ScrollView, let owner = sv.superview as? NodeView, let zoom = owner.extras?.zoom, zoom.doubleTapAdmits(hit) else { return nil }
        owner.zoomDoubleTapped(at: sv.convert(window, from: nil))
        return owner
    }
    func zoomDoubleTapped(at point: CGPoint) {
        guard let sv = scroll, let range = zoomRange, let child = extras?.zoom?.child, !zoomHeld(sv) else { return }
        let target = sv.zoomScale > range.lowerBound ? range.lowerBound : min(range.upperBound, 2 * range.lowerBound)
        guard target != sv.zoomScale else { return }
        extras?.zoom?.reported.removeAll()
        moveZoom(sv, child: child, from: (sv.zoomScale, sv.contentOffset), to: target,
             focal: CGPoint(x: point.x - sv.bounds.minX, y: point.y - sv.bounds.minY), animated: !ExactEnv.agentFreezes && window != nil)
    }
    /// A press on content that is not a control, where a double tap could
    /// begin, waits until the second tap cannot come (§3 rule 5, as UIKit's
    /// `require(toFail:)` holds a single tap): `press` runs then, or never
    /// when the double tap takes the touch.
    func pressAfterDoubleTap(from touched: UIView, _ press: @escaping () -> Void) {
        var at = superview
        while let current = at, !(current is ScrollView && (current.superview as? NodeView)?.extras?.zoom != nil) { at = current.superview }
        guard let zoom = (at?.superview as? NodeView)?.extras?.zoom, zoom.doubleTap?.isEnabled == true, !isZoomControl,
              zoom.doubleTapAdmits(touched) else { return press() }
        if zoom.doubled { return }
        zoom.heldPress?()
        zoom.heldPress = press
        DispatchQueue.main.asyncAfter(deadline: .now() + ScrollZoom.doubleTapWait) { [weak zoom] in
            guard let zoom, let held = zoom.heldPress else { return }
            zoom.heldPress = nil
            held()
        }
    }
}
#else
extension NodeView {
    /// tvOS has no double tap to zoom: a press runs at once.
    func pressAfterDoubleTap(from touched: UIView, _ press: @escaping () -> Void) { press() }
}
#endif
#endif
