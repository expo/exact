// `scrolldragstart` and `scrolldragend` on AppKit (Charlie, 2026-11-10;
// issues/20261021-scroll-drag-events-and-hit-slop.md). UIKit's
// `scrollViewWillBeginDragging` and `scrollViewWillEndDragging` have no
// AppKit counterpart; the closest is a trackpad scroll's finger phases
// (`NSEvent.phase`), which its momentum (`momentumPhase`) follows after the
// lift. A mouse wheel's events have no phase and send neither.
#if os(macOS)
import AppKit

/// The fingers' moves of one drag a scroller took, for the release velocity
/// AppKit does not report.
struct ScrollDragTrack {
    /// UIKit's window for a release velocity is about this long.
    static let window: TimeInterval = 0.1
    /// When each move came, and how far it moved the offset (points).
    private(set) var moves: [(at: TimeInterval, by: CGPoint)] = []
    /// Where the offset was when the drag began.
    var start = CGPoint.zero

    mutating func moved(at t: TimeInterval, by offset: CGPoint) {
        moves.append((t, offset))
        moves.removeAll { t - $0.at > Self.window }
    }

    /// The velocity at a lift at `t`, in points per millisecond, positive as
    /// the offset grows: the moves of the last 100 ms over their span, zero
    /// when the fingers rested that long before lifting.
    func velocity(at t: TimeInterval) -> CGPoint {
        let recent = moves.filter { t - $0.at <= Self.window }
        guard let first = recent.first, let last = recent.last, last.at > first.at else { return .zero }
        let travel = recent.dropFirst().reduce(CGPoint.zero) { CGPoint(x: $0.x + $1.by.x, y: $0.y + $1.by.y) }
        let ms = (last.at - first.at) * 1000
        return CGPoint(x: travel.x / ms, y: travel.y / ms)
    }
}

extension ChainingScrollView {
    /// A wheel event this scroller saw with `delta` (AppKit's scrolling
    /// deltas, which move the offset the other way). `taken`: it scrolls
    /// here. A drag begins with the first finger move it takes and ends at
    /// the lift.
    func noteDrag(_ event: NSEvent, delta: CGPoint, taken: Bool) {
        guard event.momentumPhase.isEmpty, !event.phase.isEmpty, let node = superview as? NodeView else { return }
        let lift = event.phase.contains(.ended) || event.phase.contains(.cancelled)
        if !lift, taken, delta != .zero {
            if dragTrack == nil {
                dragTrack = ScrollDragTrack()
                dragTrack?.start = contentView.bounds.origin
                if node.handlers.contains("scrolldragstart") { node.presenter?.scrollDrag(node.id, nil) }
            }
            dragTrack?.moved(at: event.timestamp, by: CGPoint(x: -delta.x, y: -delta.y))
        } else if lift, let track = dragTrack {
            dragTrack = nil
            let velocity = track.velocity(at: event.timestamp)
            if node.handlers.contains("scrolldragend") { node.presenter?.scrollDrag(node.id, (velocity, nil)) }
            // A snapping scroller eases to its rest; the momentum after is dropped (`ScrollSnapMac.swift`).
            _ = node.snapAtLift(velocity: velocity, start: track.start)
        }
    }
}
#endif
