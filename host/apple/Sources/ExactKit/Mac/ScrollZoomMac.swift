// A zoomable scroll container on AppKit (@ref LLP 1057.001.000 §5):
// NSScrollView's magnification, so the trackpad's pinch, its bounce past the
// range and the smart magnify are AppKit's. Magnification scales the clip
// view's bounds, and AppKit hits and draws the document at the scale. The
// document is the child's border box alone, and D3's unscaled padding,
// margins and centring are the clip view's content insets, which are in the
// scroll view's points and so unscaled; they are given again as the
// magnification changes (the clip view holds them divided by the
// magnification, in its bounds' units). A `scroll` reports offsets in zoomed
// points: the clip's origin times the zoom, plus the leading inset.
#if os(macOS)
import AppKit

/// A zoomable scroller's zoom: the child that is its document's box, and
/// what the app last wrote.
final class ScrollZoomMac: NSObject {
    weak var owner: NodeView?
    weak var child: NodeView?
    /// The `scrollZoom` prop as last seen: a change is a write (D2).
    var written: Double?
    var pending: Double?
    /// The zooms reported since the last gesture began. A write of one is the
    /// app mirroring the gesture back as it ran, and moves nothing.
    var reported: [Double] = []
    var lastEvent: [Double] = []
    var magnifying = false

    init(owner: NodeView, scroll: ChainingScrollView) {
        self.owner = owner
        super.init()
        let center = NotificationCenter.default
        center.addObserver(self, selector: #selector(boundsChanged), name: NSView.boundsDidChangeNotification, object: scroll.contentView)
        center.addObserver(self, selector: #selector(magnifyStarted), name: NSScrollView.willStartLiveMagnifyNotification, object: scroll)
        center.addObserver(self, selector: #selector(magnifyEnded), name: NSScrollView.didEndLiveMagnifyNotification, object: scroll)
    }
    func detach() { NotificationCenter.default.removeObserver(self) }

    /// The magnification moved the clip's bounds: the insets follow the zoom.
    @objc func boundsChanged() { owner?.placeZoom() }
    @objc func magnifyStarted() {
        magnifying = true
        reported.removeAll()
        guard let owner, owner.handlers.contains("scrollend") else { return }
        let end = owner.scrollEnd ?? ScrollEnd()
        owner.scrollEnd = end
        end.live = true
        end.hold()
    }
    @objc func magnifyEnded() {
        magnifying = false
        owner?.scrollEnd?.live = false
        // A write the gesture held, now at rest, then the end.
        DispatchQueue.main.async { [weak self] in
            guard let owner = self?.owner else { return }
            if self?.pending != nil { owner.settleZoom() }
            owner.endScroll()
        }
    }
}

extension NodeView {
    /// The scroller's one element child, or nil.
    private func zoomChild(_ sv: ChainingScrollView) -> NodeView? {
        let children = sv.documentView?.subviews.compactMap { $0 as? NodeView } ?? []
        return children.count == 1 ? children[0] : nil
    }

    /// After a batch, before its offsets (`Presenter.applyBatch`): the range,
    /// the child, and a `scrollZoom` written, about the port's centre.
    func settleZoom() {
        guard let sv = scroll else { return }
        guard let range = zoomRange, sv.scrollsX, sv.scrollsY, let child = zoomChild(sv), hasScrollLayoutBox else {
            if sv.zoom != nil { dropZoom(sv) }
            return
        }
        let zoom = sv.zoom ?? ScrollZoomMac(owner: self, scroll: sv)
        if sv.zoom == nil {
            sv.zoom = zoom
            sv.allowsMagnification = true
        }
        // A replaced child keeps the zoom; each is placed again, in or out of the document's box.
        if zoom.child !== child {
            let old = zoom.child
            zoom.child = child
            for v in [old, child] { if let v, let layout = v.grid?.layout { presenter?.applyFrame(v, layout) } }
        }
        let written = props["scrollZoom"].flatMap(Double.init).flatMap { $0.isFinite && $0 > 0 ? $0 : nil }
        if written != zoom.written {
            zoom.written = written
            if let written { zoom.pending = written }
        }
        let before = sv.magnification
        if sv.minMagnification != range.lowerBound { sv.minMagnification = range.lowerBound }
        if sv.maxMagnification != range.upperBound { sv.maxMagnification = range.upperBound }
        var target = min(max(before, range.lowerBound), range.upperBound), animated = false
        let held = zoom.magnifying || scrollEnd?.live == true
        if !held, let asked = zoom.pending {
            zoom.pending = nil
            if !zoom.reported.contains(asked) {
                zoom.reported.removeAll()
                target = min(max(CGFloat(asked), range.lowerBound), range.upperBound)
                animated = style["scroll_behavior"]?.string == "smooth" && !ExactEnv.agentFreezes && window != nil
            }
        }
        placeZoom()
        if !held, target != sv.magnification {
            let port = sv.contentView.frame.size
            moveZoom(sv, to: target, focal: CGPoint(x: port.width / 2, y: port.height / 2), animated: animated)
        }
        // The batch's offsets, in zoomed points, land in the zoomed extent.
        if pendingScrollTop != nil || pendingScrollLeft != nil {
            let i = sv.contentInsets, z = sv.magnification, o = sv.contentView.bounds.origin
            let x = pendingScrollLeft.map { (CGFloat($0) - i.left) / z } ?? o.x, y = pendingScrollTop.map { (CGFloat($0) - i.top) / z } ?? o.y
            pendingScrollTop = nil; pendingScrollLeft = nil
            scrollZoomed(sv, to: CGPoint(x: x, y: y))
        }
    }

    /// To zoom `z`, keeping the document's point under `focal` (in the clip
    /// view's frame, from its top left) where it is.
    func moveZoom(_ sv: ChainingScrollView, to z: CGFloat, focal: CGPoint, animated: Bool) {
        let was = sv.magnification, o = sv.contentView.bounds.origin
        let at = CGPoint(x: o.x + focal.x / was, y: o.y + focal.y / was)
        guard animated else {
            sv.magnification = z
            placeZoom()
            return scrollZoomed(sv, to: CGPoint(x: at.x - focal.x / z, y: at.y - focal.y / z))
        }
        NSAnimationContext.runAnimationGroup({ c in
            c.duration = 0.3
            sv.animator().setMagnification(z, centeredAt: at)
        }, completionHandler: { [weak self] in
            self?.placeZoom()
            self?.endScroll()
        })
    }

    /// The clip at `origin` in the document's units, within the range the
    /// insets give at the zoom.
    func scrollZoomed(_ sv: ChainingScrollView, to origin: CGPoint) {
        let clip = sv.contentView, i = sv.contentInsets, z = sv.magnification
        let document = sv.documentView?.frame.size ?? .zero, port = clip.frame.size
        let x = min(max(origin.x, -i.left / z), max(-i.left / z, document.width + (i.right - port.width) / z))
        let y = min(max(origin.y, -i.top / z), max(-i.top / z, document.height + (i.bottom - port.height) / z))
        guard clip.bounds.origin != CGPoint(x: x, y: y) else { return }
        clip.scroll(to: CGPoint(x: x, y: y))
        sv.reflectScrolledClipView(clip)
    }

    /// The insets D3 gives at the current magnification: the leading margin
    /// and padding and half the slack before the scaled child, the trailing
    /// ones and the other half after it, in unscaled points.
    func placeZoom() {
        guard let sv = scroll, let zoom = sv.zoom, let child = zoom.child, let f = child.grid?.layout else { return }
        let origin = sv.kernelDocumentOrigin, port = sv.contentView.frame.size
        let lead = CGRect(x: f.minX - origin.x, y: f.minY - origin.y, width: f.width, height: f.height)
        let place = zoomPlace(sv.magnification, child: child, frame: lead, port: port), trail = zoomTrail(child)
        let insets = NSEdgeInsets(top: lead.minY + place.shift.y, left: lead.minX + place.shift.x,
                                  bottom: trail.height + place.shift.y, right: trail.width + place.shift.x)
        let now = sv.contentInsets
        if now.top != insets.top || now.left != insets.left || now.bottom != insets.bottom || now.right != insets.right { sv.contentInsets = insets }
        fitScroll()
    }

    /// Not zoomable any more: the document is the content again.
    func dropZoom(_ sv: ChainingScrollView) {
        guard let zoom = sv.zoom else { return }
        zoom.detach()
        sv.zoom = nil
        sv.magnification = 1
        sv.allowsMagnification = false
        sv.contentInsets = NSEdgeInsetsZero
        if let child = zoom.child, let layout = child.grid?.layout { presenter?.applyFrame(child, layout) }
        fitScroll()
    }

    /// A zoomable scroller's `scroll` (LLP 1057.001.000 D2): offsets and
    /// extents in zoomed points, and the zoom.
    func sendZoomScrollEvent(_ sv: ChainingScrollView, _ zoom: ScrollZoomMac) {
        guard handlers.contains("scroll"), presenter?.views[id] === self, hasScrollLayoutBox else { return }
        let clip = sv.contentView, i = sv.contentInsets, z = sv.magnification, o = clip.bounds.origin
        let document = sv.documentView?.frame.size ?? .zero, port = clip.frame.size
        let extent = CGSize(width: max(port.width, i.left + document.width * z + i.right), height: max(port.height, i.top + document.height * z + i.bottom))
        let event = [o.x * z + i.left, o.y * z + i.top, extent.width, extent.height, port.width, port.height, z].map(Double.init)
        guard event != zoom.lastEvent else { return }
        zoom.lastEvent = event
        zoom.reported.append(event[6])
        presenter?.scroll(id, event)
    }
}

extension ChainingScrollView {
    /// A trackpad pinch: AppKit's live magnification for a person. Under the
    /// agent, whose clock AppKit's animation does not follow, each event
    /// applies at once about the pointer, clamped to the range, as a wheel
    /// does there (`appliesAtOnce`); its end ends the scroll.
    package override func magnify(with event: NSEvent) {
        guard let zoom, let owner = zoom.owner, Self.appliesAtOnce else { return super.magnify(with: event) }
        if event.phase.contains(.began) { zoom.reported.removeAll() }
        let p = contentView.convert(windowPoint(event), from: nil), o = contentView.bounds.origin
        let focal = CGPoint(x: (p.x - o.x) * magnification, y: (p.y - o.y) * magnification)
        let target = min(max(magnification * (1 + event.magnification), minMagnification), maxMagnification)
        if target != magnification { owner.moveZoom(self, to: target, focal: focal, animated: false) }
        if event.phase.contains(.ended) || event.phase.contains(.cancelled) { owner.endScroll() }
    }
    /// The event's point in the window: one made apart from any window (the
    /// agent's) carries a screen point.
    private func windowPoint(_ event: NSEvent) -> CGPoint {
        event.window == nil ? window?.convertPoint(fromScreen: event.locationInWindow) ?? event.locationInWindow : event.locationInWindow
    }
    /// A two-finger double tap on a zoomable scroller (D5): above the minimum
    /// to the minimum, at it to twice it within the range, about the pointer.
    package override func smartMagnify(with event: NSEvent) {
        guard let zoom, let owner = zoom.owner else { return super.smartMagnify(with: event) }
        let low = minMagnification, target = magnification > low ? low : min(maxMagnification, 2 * low)
        guard target != magnification else { return }
        zoom.reported.removeAll()
        let p = contentView.convert(windowPoint(event), from: nil), o = contentView.bounds.origin
        let focal = CGPoint(x: (p.x - o.x) * magnification, y: (p.y - o.y) * magnification)
        owner.moveZoom(self, to: target, focal: focal, animated: !ExactEnv.agentFreezes)
    }
}
#endif
