// `scrollend` on UIKit (`ScrollEnd.swift`): the delegate reports the end of
// a drag that throws nothing, of its deceleration, and of an animation; a
// move none of them holds ends once still. A pooled view's owed end is
// dropped when it goes to another node (`NodePool`).
#if os(iOS) || os(tvOS)
import UIKit

extension NodeView {
    /// The end the reader was following moved during their interaction:
    /// settle there when it ends, as a browser's scroll anchoring does.
    package func scrollViewDidEndDragging(_ scrollView: UIScrollView, willDecelerate decelerate: Bool) {
        if !decelerate { presenter?.session?.frameHealth.endScroll(scrollView) }
        pagedDragEnded(scrollView)
        guard scrollView === self.scrollView else { return }
        // A follow of the end that animates ends with its animation.
        if !decelerate, !followEndIfOwed() { endScroll() }
    }
    package func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) {
        presenter?.session?.frameHealth.endScroll(scrollView)
        if scrollView === self.scrollView, !followEndIfOwed() { scrollMayHaveEnded(scrollView) }
    }
    package func scrollViewDidEndScrollingAnimation(_ scrollView: UIScrollView) {
        guard scrollView === self.scrollView, presenter?.elementScrolls.cutting !== scrollView else { return }
        followingEndAnimated = false; authoredScrollAnimating = false
        presenter?.elementScrolls.revealEnded(scrollView)
        // A smooth correction's driver ends its own motion (`OffsetDriver`);
        // a UIKit animation's end is not its end.
        if let c = presenter?.collections, c.drivesOffset(id) || c.startOwed.contains(id) { return }
        presenter?.collections.animationEnded(id, atTarget: endedAtTarget(scrollView))
        scrollMayHaveEnded(scrollView)
    }
    /// Whether it started an animation.
    @discardableResult private func followEndIfOwed() -> Bool {
        guard followsEndAfterInteraction, let sv = scrollView else { return false }
        followsEndAfterInteraction = false
        let maximum = max(-sv.adjustedContentInset.top, sv.contentSize.height + sv.adjustedContentInset.bottom - sv.bounds.height)
        anchoredScrollTop = maximum
        guard sv.contentOffset.y >= maximum - 80, abs(sv.contentOffset.y - maximum) > 0.5, !authorOwnsScroll(sv) else { return false }
        sv.setContentOffset(PixelGrid.offset(CGPoint(x: sv.contentOffset.x, y: maximum), in: sv), animated: true)
        return true
    }

    func trackIntersections(previous: Presenter? = nil, listening: Bool = true) {
        if let previous, intersectListening {
            previous.intersectHandlerCount -= 1
            intersectListening = false
        }
        let current = listening && presenter != nil && handlers.contains("intersect")
        if current != intersectListening {
            presenter?.intersectHandlerCount += current ? 1 : -1
            intersectListening = current
        }
    }

    /// The scroll view reported its offset (`scrollViewDidScroll`).
    func scrollEndMoved(_ sv: UIScrollView) {
        // A collapse (`display: none`) clamps the offset and sends no
        // `scroll`: nothing to end, and not a position to compare with.
        guard (handlers.contains("scrollend") || presenter?.collections.owns(id) == true || presenter?.session?.timerAdmissions.enabled == true), hasScrollLayoutBox else { return }
        let end = scrollEnd ?? ScrollEnd()
        scrollEnd = end
        guard end.moved(left: sv.contentOffset.x, top: sv.contentOffset.y, inset: scrollTopInset(sv), scale: sv.zoomScale) else { return }
        // A drag and its deceleration report their end; a touch that only
        // tracks reports none, so the quiet check asks until it lifts.
        if end.live || sv.isDragging || sv.isDecelerating || sv.isZooming || sv.isZoomBouncing { end.hold() } else { settleScrollEnd() }
    }

    /// Once still: ended, unless a touch, an agent drag, a smooth correction
    /// or an element reveal holds it now (asked again then; a touch that never drags
    /// reports no end of its own, a correction's first frame may not have
    /// come yet, and a reveal ends with its animation).
    private func settleScrollEnd() {
        scrollEnd?.settle { [weak self, token = incarnation] in
            guard let self, self.incarnation == token, let sv = self.scrollView else { return }
            if self.scrollEnd?.live == true || self.zoomHeld(sv) || self.authorOwnsScroll(sv) { self.settleScrollEnd() } else { self.endScroll() }
        }
    }

    /// A deceleration or an animation stopped: over, unless a finger caught
    /// it (UIKit stops a fling a finger lands on), which ends when it lifts
    /// or once still, or the app's own scroll took it over: a fling a
    /// `scrollIntoView` stops ends with that scroll, where it rests, not
    /// where the fling stopped (LLP 1070.000 §15).
    func scrollMayHaveEnded(_ sv: UIScrollView) {
        if scrollEnd?.live == true || sv.isTracking || sv.isDragging || sv.isZooming || sv.isZoomBouncing || authorOwnsScroll(sv) { settleScrollEnd() } else { endScroll() }
    }

    /// The app's scroll owns the port until it settles (LLP 1070.000 §15):
    /// a list's `scrollIntoView` under way, a smooth correction running or
    /// about to start, an element's reveal, or a smooth `scrollTop` write.
    /// The end-follow moves nothing meanwhile, and the scroll's `scrollend`
    /// waits for it.
    func authorOwnsScroll(_ sv: UIScrollView) -> Bool {
        guard let presenter else { return authoredScrollAnimating }
        return authoredScrollAnimating || presenter.elementScrolls.revealing(sv) != nil
            || presenter.collections.drivesOffset(id) || presenter.collections.startOwed.contains(id)
            || presenter.collections.snapshot(id)?.underWay == true
    }

    /// The scroll is over: its last `scroll` first, then `scrollend`, once.
    func endScroll() {
        guard presenter?.views[id] === self, let end = scrollEnd, !end.live, end.take(), hasScrollLayoutBox else { return }
        // The last `scroll` may run its handler at once, and a batch it
        // commits may give this view to another node.
        let node = id, token = incarnation
        presenter?.collections.rested(node)
        sendScrollEvent()
        guard id == node, incarnation == token, presenter?.views[node] === self else { return }
        if handlers.contains("scrollend") { presenter?.scrollend(node) }
        presenter?.session?.timerAdmissions.check()
    }
}
#endif
