#if os(iOS) || os(tvOS)
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// A recognizer that keeps the timestamps of the touches it receives: the
/// input's own time, which a hold's samples are taken at (LLP 1057.001 §3).
/// `UIEvent.timestamp` is on `CACurrentMediaTime()`'s timebase.
protocol TimedGesture: UIGestureRecognizer {
    /// The newest touch event's time, and the newest that moved a touch.
    var timestamps: (event: TimeInterval, moved: TimeInterval)? { get set }
}
extension TimedGesture {
    /// When what the recognizer reports in `state` happened. A lift or a
    /// cancel is its own event; a location is the newest move's, which UIKit
    /// may report later (a pan recognized only as the finger lifts).
    func time(_ state: UIGestureRecognizer.State) -> TimeInterval {
        guard let timestamps else { return CACurrentMediaTime() }
        return [.ended, .cancelled, .failed].contains(state) ? timestamps.event : timestamps.moved
    }
    func received(_ event: UIEvent, moved: Bool) {
        timestamps = (event.timestamp, moved ? event.timestamp : timestamps?.moved ?? event.timestamp)
    }
}

final class TimedPan: UIPanGestureRecognizer, TimedGesture {
    var timestamps: (event: TimeInterval, moved: TimeInterval)?
    /// Where (window points) and when its first touch came down.
    private(set) var down: (point: CGPoint, time: TimeInterval)?
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        received(event, moved: true)
        if down == nil, let touch = touches.first { down = (touch.location(in: nil), event.timestamp) }
        super.touchesBegan(touches, with: event)
    }
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) { received(event, moved: true); super.touchesMoved(touches, with: event) }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) { received(event, moved: false); super.touchesEnded(touches, with: event) }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { received(event, moved: false); super.touchesCancelled(touches, with: event) }
    override func reset() { timestamps = nil; down = nil; super.reset() }
}

#if !os(tvOS)
final class TimedPinch: UIPinchGestureRecognizer, TimedGesture {
    var timestamps: (event: TimeInterval, moved: TimeInterval)?
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) { received(event, moved: true); super.touchesBegan(touches, with: event) }
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) { received(event, moved: true); super.touchesMoved(touches, with: event) }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) { received(event, moved: false); super.touchesEnded(touches, with: event) }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { received(event, moved: false); super.touchesCancelled(touches, with: event) }
    override func reset() { timestamps = nil; super.reset() }
}
#endif
#endif
