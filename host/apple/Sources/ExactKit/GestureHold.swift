// Values and native lifetime only. The platform retains recognition/arbitration.
import CExact
import Foundation

/// The thresholds exact2 defines itself, read from `exact_motion::gesture`
/// (LLP 1057.001 §3): the swipe knee, its resistance, the edge a swipe yields.
package enum Gesture {
    static let knee = exact_gesture_constant(0)
    static let resistance = exact_gesture_constant(1)
    static let edge = exact_gesture_constant(2)
    package static let slop = exact_gesture_constant(3)
    /// Seconds (`exact_motion::gesture::LIFT_STILL`).
    package static let liftStill = exact_gesture_constant(4)
    /// Whether a lift `dt` seconds after a hold's newest sample is a sample
    /// of its own: it `moved` the input, or it came after `liftStill`.
    package static func liftSamples(moved: Bool, dt: Double) -> Bool {
        moved || dt > liftStill
    }
}

/// An input's own timestamps carried onto the session clock from where its
/// hold began: the times a hold's samples are taken at, which its release
/// velocity reads (LLP 1057.001 §3). The session clock is what moves the
/// presentation; a driver may hold it still while a finger moves, or run it
/// ahead, and the velocity is still the finger's.
package struct InputTimeline {
    let input: Double
    let clock: Double
    /// `input` is an event's timestamp in seconds (`UIEvent`'s or `NSEvent`'s),
    /// `clock` the session's milliseconds at that event.
    package init(input: Double, clock: Double) { self.input = input; self.clock = clock }
    package func at(_ time: Double) -> Double { clock + (time - input) * 1000 }
}

package struct NativeHold {
    package let token: UInt64
    package let x: Double
    let y: Double
    init?(_ op: [String: Any]) {
        guard let raw = op["token"] as? String, let token = UInt64(raw),
              let x = op["x"] as? Double, let y = op["y"] as? Double,
              x.isFinite, y.isFinite else { return nil }
        self.token = token; self.x = x; self.y = y
    }
}

/// Invert resistance at the caught presentation, then apply finger displacement.
/// Zero displacement is exactly the captured value, even beyond the knee.
enum SwipeRecognition {
    /// A sideways pan begins the swipe toward a direction the node hears
    /// (`swiperight`, `swipeleft`, or both), or back from a hold caught on
    /// that side. Directions are the screen's, in a right-to-left layout too.
    static func accepts(x: Double, y: Double, presentedX: Double, right: Bool = true, left: Bool = false) -> Bool {
        abs(x) > abs(y) && ((right && (x > 0 || presentedX > 0.5)) || (left && (x < 0 || presentedX < -0.5)))
    }
    /// Which of the two the node hears: `swiperight`, `swipeleft`.
    static func directions(_ handlers: Set<String>) -> (right: Bool, left: Bool) {
        (handlers.contains("swiperight"), handlers.contains("swipeleft"))
    }
    static func hears(_ handlers: Set<String>) -> Bool {
        handlers.contains("swiperight") || handlers.contains("swipeleft")
    }
    /// Whether a release presented at `value` runs an action: at or past the
    /// knee on a side the node hears.
    static func armed(_ value: Double, right: Bool, left: Bool) -> Bool {
        (right && value >= Gesture.knee) || (left && value <= -Gesture.knee)
    }
    /// An indicator's progress: toward the side the node hears, either way
    /// when it hears both.
    static func progress(_ value: Double, right: Bool, left: Bool) -> Double {
        let toward = right && left ? abs(value) : left ? -value : value
        return min(1, max(0, toward / Gesture.knee))
    }
}

struct SwipeDisplacement {
    let base: Double
    private var origin: Double {
        abs(base) <= Gesture.knee ? base : (base < 0 ? -1 : 1) * (Gesture.knee + (abs(base) - Gesture.knee) / Gesture.resistance)
    }
    func value(_ displacement: Double) -> Double {
        if displacement == 0 { return base }
        let position = origin + displacement
        return abs(position) <= Gesture.knee ? position : (position < 0 ? -1 : 1) * (Gesture.knee + (abs(position) - Gesture.knee) * Gesture.resistance)
    }
    func velocity(displacement: Double, fingerVelocity: Double) -> Double {
        fingerVelocity * (abs(origin + displacement) > Gesture.knee ? Gesture.resistance : 1)
    }
}

struct SwipeIndicator {
    let base: Double
    let progressAtCatch: Double
    func value(_ progress: Double) -> Double {
        if progress == progressAtCatch { return base }
        if progress < progressAtCatch { return base * progress / progressAtCatch }
        return base + (1 - base) * (progress - progressAtCatch) / (1 - progressAtCatch)
    }
}

/// Input eligibility is distinct from token ownership: an inert/hidden row
/// still owns its token until cancellation releases the presentation and pin.
package enum SwipeInput {
    package static func allows(_ view: NodeView) -> Bool {
        guard view.window != nil, !view.disabled, !view.inert, !view.isHidden else { return false }
        #if os(iOS) || os(tvOS)
        guard view.isUserInteractionEnabled else { return false }
        #endif
        var ancestor = view.superview
        while let parent = ancestor {
            if parent.isHidden || (parent as? NodeView)?.disabled == true { return false }
            #if os(iOS) || os(tvOS)
            if !parent.isUserInteractionEnabled { return false }
            // Through a presented route's logical parents (LLP 1123 D2)
            ancestor = view.presenter?.eventParent(of: parent) ?? parent.superview
            #else
            ancestor = parent.superview
            #endif
        }
        return true
    }
}

/// One recognized swipe, at most one translate and two authored indicator holds.
/// Every callback checks both the session incarnation and native view identity.
final class SwipeHold {
    weak var session: ExactSession?
    weak var view: NodeView?
    let generation: Int
    let primary: NativeHold
    var indicators: [(NativeHold, Double, UInt32)] = []
    private(set) var ended = false
    private var finishing = false
    private var displacement = 0.0
    /// The input's times, where the engine measures the release velocity (a
    /// mouse); nil where the platform does (UIKit's pan).
    private let timeline: InputTimeline?
    private var time: Double?
    private var pin: UInt64?
    private var deliveryQueued = false
    private var pendingMove: (Double, Double?)?
    private var pendingEnd: (Double, Double?, Bool, Double?)?

    // A recognizer can cancel synchronously while UIKit reparents or disables
    // a view. One queued delivery per gesture crosses the *session* boundary,
    // including its final motion flag, then rechecks incarnation/view/token.
    private func scheduleDelivery() {
        guard !deliveryQueued else { return }
        deliveryQueued = true
        DispatchQueue.main.async { [self] in
            deliveryQueued = false
            if session?.isApplyingPresentation == true { scheduleDelivery(); return }
            let end = pendingEnd, sample = pendingMove
            pendingEnd = nil; pendingMove = nil
            if let sample, end?.2 != true { _ = move(sample.0, time: sample.1) }
            if let end { finish(displacement: end.0, fingerVelocity: end.1, cancel: end.2, time: end.3) }
        }
    }
    var mapping: SwipeDisplacement { SwipeDisplacement(base: primary.x) }

    /// `time`: the input's timestamp in seconds, where the engine measures the
    /// release velocity.
    init?(_ view: NodeView, time: Double? = nil) {
        guard let session = view.presenter?.session, !session.isApplyingPresentation,
              SwipeInput.allows(view), SwipeRecognition.hears(view.handlers) else { return nil }
        self.session = session; self.view = view; generation = session.generation
        let now = session.now()
        timeline = time.map { InputTimeline(input: $0, clock: now) }
        self.time = time
        let (hold, batch) = session.runtime.holdBegin(view.id, property: 0, now: now)
        guard let hold else { session.apply(batch); return nil }
        primary = hold
        session.apply(batch)
        guard live else { return nil }
        // Only the first explicitly authored direct child is the companion.
        if let indicator = NodeView.committed(view.subviews).first(where: { $0.props["swipeIndicator"] == "true" }) {
            for property: UInt32 in [3, 1] {
                let (token, batch) = session.runtime.holdBegin(indicator.id, property: property, now: session.now())
                if let token { indicators.append((token, token.x, property)) }
                session.apply(batch)
            }
        }
        pin = session.presenter.collections.holdPointer(view.id)
        session.trackInputHold(self)
        guard inputEligible else { cancel(); return nil }
    }
    var incarnationLive: Bool {
        guard let session, let view else { return false }
        return session.generation == generation && !session.runtime.destroyed && session.presenter.views[view.id] === view
    }
    var live: Bool { !ended && incarnationLive && session?.runtime.hasHold(primary.token) == true }

    var inputEligible: Bool {
        guard incarnationLive, let view else { return false }
        return SwipeInput.allows(view) && SwipeRecognition.hears(view.handlers)
    }
    /// The directions the view hears now; none once it is gone.
    var directions: (right: Bool, left: Bool) { view.map { SwipeRecognition.directions($0.handlers) } ?? (false, false) }
    /// Whether a release now, presented at `displacement`'s value, runs an action.
    func armed(_ displacement: Double) -> Bool {
        let (right, left) = directions
        return SwipeRecognition.armed(mapping.value(displacement), right: right, left: left)
    }
    func cancelIfInputIneligible() {
        if !ended && !inputEligible { cancel() }
    }

    @discardableResult func move(_ displacement: Double, time: Double? = nil) -> Bool {
        guard displacement.isFinite, !ended, pendingEnd == nil, let session else { return false }
        if session.isApplyingPresentation || deliveryQueued {
            pendingMove = (displacement, time); scheduleDelivery(); return true
        }
        guard live else { return false }
        guard inputEligible else {
            if !finishing { cancel() }
            return false
        }
        self.displacement = displacement
        if let time { self.time = time }
        let value = mapping.value(displacement)
        let batch = session.runtime.holdUpdate(primary.token, x: value, y: primary.y, now: session.now(), at: at)
        session.apply(batch)
        guard batch.error == nil, live else { return false }
        let (right, left) = directions
        let progress = SwipeRecognition.progress(value, right: right, left: left)
        let caught = SwipeRecognition.progress(primary.x, right: right, left: left)
        for (hold, base, property) in indicators {
            let value = SwipeIndicator(base: base, progressAtCatch: caught).value(progress)
            session.apply(session.runtime.holdUpdate(hold.token, x: value, y: property == 1 ? value : 0, now: session.now()))
        }
        return live
    }

    /// The session time of the newest sample, on the input's timeline.
    private var at: Double? { time.flatMap { t in timeline?.at(t) } }

    /// `fingerVelocity` nil: the platform measured none (a mouse), so the
    /// engine's own estimate over the presented values releases it.
    func finish(displacement: Double, fingerVelocity: Double?, cancel: Bool, time: Double? = nil) {
        guard !ended, !finishing, pendingEnd == nil else { return }
        if session?.isApplyingPresentation == true || deliveryQueued {
            pendingEnd = (displacement, fingerVelocity, cancel, time)
            scheduleDelivery(); return
        }
        finishing = true
        // Apply the last pointer sample before checking eligibility for action.
        let lifted = Gesture.liftSamples(moved: displacement != self.displacement,
                                         dt: time.flatMap { t in self.time.map { t - $0 } } ?? .infinity)
        let updated = lifted ? move(displacement, time: time) : live && inputEligible
        guard let session else { ended = true; return }
        if updated && !cancel && fingerVelocity?.isFinite != false && armed(displacement),
           live, inputEligible, let view {
            let id = view.id, now = session.now()
            session.applyInput(session.runtime.swipe(id, mapping.value(displacement) > 0 ? .right : .left, now: now), at: id)
        }
        ended = true
        session.retireInputHold(self)
        // An action may remove this view or reboot the runtime. Never end in a successor.
        if session.generation == generation && !session.runtime.destroyed {
            let refused = cancel || !updated || !inputEligible
            if let fingerVelocity {
                let velocity = mapping.velocity(displacement: displacement, fingerVelocity: fingerVelocity)
                session.apply(session.runtime.holdEnd(primary.token, cancel: refused || !velocity.isFinite, vx: velocity.isFinite ? velocity : 0, now: session.now()))
            } else {
                session.apply(session.runtime.holdEnd(primary.token, cancel: refused, measured: true, now: session.now(), at: at))
            }
            for (hold, _, _) in indicators {
                session.apply(session.runtime.holdEnd(hold.token, cancel: true, now: session.now()))
            }
            session.presenter.collections.releaseInteractionLater(ifCurrent: pin)
        }
    }
    func cancel() { finish(displacement: pendingMove?.0 ?? displacement, fingerVelocity: 0, cancel: true) }
}

/// Recognition may have displacement but zero instantaneous velocity (notably
/// a coalesced iOS-on-Mac drag). Direction still belongs to the actual input.
/// A height drag and a list row's reorder both read it.
package enum HeightDragDirection {
    package static func accepts(velocityX: Double, velocityY: Double, translationX: Double, translationY: Double) -> Bool {
        guard velocityX.isFinite, velocityY.isFinite, translationX.isFinite, translationY.isFinite else { return false }
        if velocityX == 0 && velocityY == 0 { return abs(translationY) > abs(translationX) }
        return abs(velocityY) > abs(velocityX)
    }
}
