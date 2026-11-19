// Whether Core Animation is moving a layer on the wall clock: the host waits for UIKit's motion of a platform view's
// box to end (`NativeCrop`), and the agent waits for it before a picture (`AgentMotionIOS.swift`).
#if os(iOS) || os(tvOS)
import QuartzCore

package enum LayerMotion {
    /// Whether `a`, on `layer` under `key`, is still running on the wall
    /// clock: finite (a caret's blink and a spinner repeat for ever), not
    /// held at speed 0 (Contract motion the agent's clock seeks), and not
    /// past its end, though kept on the layer to hold its last frame
    /// (`isRemovedOnCompletion` false). Its begin time is the one Core
    /// Animation recorded when it was attached; one still at 0 (not yet
    /// committed, or kept with none) runs for its span from when it was
    /// first seen, the nearest to its attachment a reader can know. Callers
    /// look at every animation in one pass, so ones attached together are
    /// first seen together.
    package static func inFlight(_ a: CAAnimation, on layer: CALayer, key: String, now: CFTimeInterval) -> Bool {
        guard a.speed != 0, a.repeatCount < 1e6, a.repeatDuration < 1e6, a.duration < 60 else { return false }
        let span = max(a.duration * Double(max(1, a.repeatCount)) * (a.autoreverses ? 2 : 1), a.repeatDuration) / Double(abs(a.speed))
        guard a.beginTime == 0 else { return layer.convertTime(now, from: nil) < a.beginTime + span }
        let seen = "\(ObjectIdentifier(layer).hashValue)/\(key)/\(ObjectIdentifier(a).hashValue)"
        let first = unbegun[seen]?.first ?? now
        unbegun[seen] = (first, now)
        return now - first < span
    }
    /// When each animation with no begin time was first and last seen (`inFlight`).
    nonisolated(unsafe) private static var unbegun: [String: (first: CFTimeInterval, last: CFTimeInterval)] = [:]
    /// Forgets, before a pass, an animation not seen for a minute (the
    /// longest `inFlight` counts): it is gone, and a reused identity is a
    /// new one. Never during a pass, so every one in it is timed alike.
    package static func pruneUnbegun(now: CFTimeInterval) { unbegun = unbegun.filter { now - $0.value.last < 60 } }
}
#endif
