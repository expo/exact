// A node's scroll view on macOS: the scroll events it sends, the offsets the
// author writes, and its document's size.
#if os(macOS)
import AppKit

extension NodeView {
    @objc func clipScrolled() {
        // A collection's knob keeps the offset the reader saw (CollectionMac.swift).
        if let sv = scroll, presenter?.collections.admitsScroll(sv) == false { return }
        if let o = scroll?.contentView.bounds.origin { presenter?.onScrolled?(id, Double(o.x), Double(o.y)) }
        if let sv = scroll { presenter?.session?.frameHealth.trackScrolled(sv) }
        presenter?.stickies.scrolled(id)
        presenter?.collectionScrolled(id)
        presenter?.transformGeometry.changed(); presenter?.tipsScrolled()
        presenter?.videoVisibility?.changed()
        // The list window and the text bands follow the scroll; they are not
        // part of it (`Presenter.scrolled`).
        presenter?.scrolled()
        repaintThrough(); queueScrollEvent(); scrollEndMoved()
    }

    private func queueScrollEvent() {
        guard handlers.contains("scroll"), !scrollEventQueued else { return }
        scrollEventQueued = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.scrollEventQueued = false
            self.sendScrollEvent()
        }
    }
    func sendScrollEvent() {
        if let sv = scroll, let zoom = sv.zoom { return sendZoomScrollEvent(sv, zoom) }
        guard handlers.contains("scroll"), let sv = scroll, case let point = sv.contentView.bounds.origin, point != lastScrollEvent,
              presenter?.views[id] === self, hasScrollLayoutBox else { return }
        lastScrollEvent = point
        // CSS's extents (`ScrollEvent`): the port, and the port plus the range the clip
        // view clamps to, so at the end `scrollHeight - scrollTop - clientHeight` is 0 (chat F4).
        let port = sv.contentView.bounds.size, document = sv.documentView?.frame.size ?? port
        let range = CGSize(width: max(0, document.width - port.width), height: max(0, document.height - port.height))
        presenter?.scroll(id, [point.x, point.y, port.width + range.width, port.height + range.height,
                               port.width, port.height].map(Double.init))
    }

    var hasScrollLayoutBox: Bool {
        var ancestor: NSView? = self
        while let current = ancestor {
            if let node = current as? NodeView, node.style["display"]?.string == "none" { return false }
            ancestor = current.superview
        }
        return true
    }

    func applyPendingScroll(instant: Bool = false) {
        defer { pendingScrollTop = nil; pendingScrollLeft = nil }
        guard let sv = scroll, let doc = sv.documentView else { return }
        guard hasScrollLayoutBox else {
            if hiddenScroll == nil { hiddenScroll = beforeLayoutScroll ?? .zero }
            return
        }
        if let saved = hiddenScroll {
            hiddenScroll = nil
            let target = NSPoint(
                x: min(max(saved.x, 0), max(0, doc.bounds.width - sv.contentView.bounds.width)),
                y: min(max(saved.y, 0), max(0, doc.bounds.height - sv.contentView.bounds.height)))
            if sv.contentView.bounds.origin != target {
                sv.contentView.scroll(to: target)
                sv.reflectScrolledClipView(sv.contentView)
            }
        }
        guard pendingScrollTop != nil || pendingScrollLeft != nil else { return }
        cancelElementScroll()
        let y = pendingScrollTop.map { CGFloat($0) == sv.contentView.bounds.minY ? sv.contentView.bounds.minY : min(max(CGFloat($0), 0), max(0, doc.bounds.height - sv.contentView.bounds.height)) } ?? sv.contentView.bounds.minY
        let x = pendingScrollLeft.map { CGFloat($0) == sv.contentView.bounds.minX ? sv.contentView.bounds.minX : min(max(CGFloat($0), 0), max(0, doc.bounds.width - sv.contentView.bounds.width)) } ?? sv.contentView.bounds.minX
        let target = NSPoint(x: x, y: y)
        guard sv.contentView.bounds.origin != target else { return }
        // `scroll-behavior: smooth` (CSS) animates a prop write, never a
        // reader's own scroll. Under the agent's frozen clock it lands at once,
        // so `layout` reads the target.
        // It holds the scroller's `scrollend`, and owns its offset, until it lands, as a reveal does.
        if !instant && style["scroll_behavior"]?.string == "smooth" && !ExactEnv.agentFreezes {
            let token = UUID(), end = scrollEnd ?? ScrollEnd()
            scrollEnd = end; end.elementAnimation = token; end.hold()
            NSAnimationContext.runAnimationGroup({ _ in sv.contentView.animator().setBoundsOrigin(target) }, completionHandler: { [weak self] in
                sv.reflectScrolledClipView(sv.contentView)
                guard end.elementAnimation == token else { return }
                end.elementAnimation = nil; self?.endScroll()
            })
        } else {
            sv.contentView.scroll(to: target)
            sv.reflectScrolledClipView(sv.contentView)
        }
    }

    package func fitScroll() {
        guard let scroll, let document = scroll.documentView else { return }
        let r = reach, port = scroll.contentView.bounds.size, origin = scroll.kernelDocumentOrigin, o = grid?.origin ?? .zero, s = presenter?.gridScale ?? 1
        let visible = CGSize(width: port.width - r.left - r.right, height: port.height - r.top - r.bottom) // the laid-out port, no reach's (LLP 1123 D13)
        var size = CGSize(width: max(PixelGrid.length(content.width, from: o, vertical: false, scale: s) - origin.x, visible.width), height: max(PixelGrid.length(content.height, from: o, vertical: true, scale: s) - origin.y, visible.height)) // ending where its last edge is drawn (`PixelGridMac.swift`)
        if let height = presenter?.tables.feeds[id]?.sectionDocumentHeight { size.height = max(size.height, height) }
        if case let size = scroll.zoom?.child?.grid?.layout.size ?? size, document.frame.size != size { document.setFrameSize(size) }
    }
}
#endif
