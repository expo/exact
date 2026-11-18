// A scroll container's document view, and what the presenter keeps of a
// list's batches and fill costs: the core's, which every scroller and the
// lists family use (LLP 1047.001 D10; the family is `ListLink`'s).
#if os(macOS)
import AppKit

/// A scroll container's document view. A collection's is also where
/// AppKit's responsive scrolling asks for overdraw (`prepareContent(in:)`):
/// it may carry only the rows the collection has built, so a concurrent
/// scroll pauses at their edge until the main thread builds more, rather
/// than carrying the background into view (LLP 1050.000 D5).
package final class FlippedView: NSView {
    package override var isFlipped: Bool { true }
    /// A finger came down on the trackpad over this document, while it
    /// accepts indirect touches (`CollectionHost.watchContact`).
    package var contactBegan: ((TimeInterval) -> Void)?
    package override func touchesBegan(with event: NSEvent) {
        if !event.touches(matching: .began, in: self).isEmpty { contactBegan?(event.timestamp) }
        super.touchesBegan(with: event)
    }
    package override func hitTest(_ point: NSPoint) -> NSView? { raisedHit(super.hitTest(point), point) }
    /// What the mounted rows cover, in this view's coordinates; nil: anything.
    package var preparedLimit: (() -> NSRect?)?
    /// AppKit's last request, kept so rows built later can widen the answer.
    private(set) var requestedPrepared: NSRect?
    package override func prepareContent(in rect: NSRect) {
        requestedPrepared = rect
        super.prepareContent(in: clampPrepared(rect))
    }
    /// `rect` within the rows' cover. What shows stays prepared, gap or not:
    /// the scroll callback's rescue builds it.
    func clampPrepared(_ rect: NSRect) -> NSRect {
        guard let limit = preparedLimit?() else { return rect }
        let clamped = rect.intersection(limit)
        return clamped.isNull || clamped.isEmpty ? visibleRect : clamped.union(visibleRect)
    }
    /// The rows changed: offer AppKit what they now cover of its request.
    package func refreshPrepared() {
        guard preparedLimit != nil else { return }
        let target = clampPrepared(requestedPrepared ?? preparedContentRect)
        if preparedContentRect != target { preparedContentRect = target }
    }
}

extension Presenter {
    /// A batch that says only where lists stand (a report's answer that
    /// moved no view: in a fling, the measurement a slice's rows sent, half
    /// its batches) and nothing of the page's. It changes no view, so none of
    /// a batch's passes over the views runs (`apply`): each list takes its
    /// snapshot and its corrections as in any batch, and its table the rows
    /// the snapshot places.
    static func onlySnapshots(_ batch: Batch) -> Bool {
        !batch.ops.isEmpty && batch.ops.allSatisfy { $0.op == .collections } && batch.error == nil && batch.wakelock == nil
            && batch.orientation == nil && batch.rootTabs == nil && !batch.layoutProvisional && !batch.controls
    }

    /// Charge the report (decode, layout and apply) to the
    /// rows it created. Cheap rows share those fixed costs in the next report.
    /// Grow at most twofold and retain recent expensive samples conservatively.
    struct ListFillCost {
        private(set) var secondsPerRow: TimeInterval?
        private var lastRows = 1
        mutating func record(seconds: TimeInterval, rows: Int) {
            guard rows > 0 else { return }
            let sample = max(0.000001, seconds / Double(rows))
            secondsPerRow = max(sample, (secondsPerRow ?? sample) * 0.75 + sample * 0.25)
            lastRows = rows
        }
        func rows(within remaining: TimeInterval) -> UInt32 {
            guard remaining > 0 else { return 0 }
            guard let secondsPerRow else { return 1 }
            let fitting = floor(remaining * 0.9 / secondsPerRow)
            return UInt32(max(0, min(Double(lastRows) * 2, fitting, Double(UInt32.max - 1))))
        }
    }
}
#endif
