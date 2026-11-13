#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

extension ExactSession {
    /// LLP 1057.003 D5: a scroller a scroll or view timeline reads (its view's
    /// `scrollTimeline` prop; the page's, a root view's `scrollTimeline` of
    /// `page`) presents the timelines' consumers from its own scroll callback,
    /// so UIKit or AppKit moves both in one frame. Inside a batch, once the
    /// batch has landed.
    func timelineScrolled(_ id: UInt32?, left: Double, top: Double) {
        let reads: Bool
        if let id {
            reads = presenter.views[id]?.props["scrollTimeline"] != nil
        } else {
            reads = presenter.root.subviews.contains { ($0 as? NodeView)?.props["scrollTimeline"] == "page" }
        }
        guard reads else { return }
        presenter.afterBatch { [weak self] in
            guard let self, state != .destroyed else { return }
            apply(runtime.timelineScrolled(id, left: left, top: top))
        }
    }
}
