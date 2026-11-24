// Journals Observe's navigation metrics (cold_ttr, warm_ttr, tti) for each router change.
// Its cause is `input` when a person's input made the commit, whatever control
// took it (`ExactSession.applyInput` marks the batch), and `program` when the
// program did, even just after a tap: a timer, a reply, or work that fell due
// inside the input's dispatch (LLP 1132 D1, "Cause"). An input navigation
// starts when its input happened (`inputHappened`), a program one at the
// commit. It is cold if its entry was not shown before in the session.
// Render time is the first vsync after the committing run-loop turn ends.
// TTI is when outstanding work clears, as the owner read it without main waiting,
// never before render. A newer change cancels it.
// Only a cold navigation has a TTI: a screen already shown (back, a tab seen
// before) was interactive before, as Observe's markInteractive on mount is
// never called again for it. A warm one ends at its warm_ttr.
// A back swipe, which UIKit makes itself (LLP 1035.001.000), reaches the
// router once its transition has ended, but the screen it shows was
// drawn from the transition's first frame: it starts when UIKit began it, and
// render is that frame, the first vsync after the turn in which UIKit began to
// show it (`willShow` comes before it).
import Foundation
import QuartzCore
#if os(macOS)
import AppKit
#endif

package final class NavigationMarks: NSObject {
    package static let shared = NavigationMarks()
    /// The timestamp of the touch or press event the window is dispatching
    /// (`ExactWindow`), in seconds on the `CACurrentMediaTime` clock.
    package var dispatching: Double?
    private var pending: Pending?
    /// An evaluation is queued: one per turn, as each reads the ledger on the owner.
    private var evaluateScheduled = false
    private(set) var link: CADisplayLink?
    private var deadline: DispatchWorkItem?
    /// Recent measurements for `state.observe`, newest last.
    private(set) var recent: [String] = []

    private func note(_ line: String) {
        recent.append(line)
        if recent.count > 16 { recent.removeFirst() }
    }

    private final class Pending {
        weak var session: ExactSession?
        let start: Double
        let committed: Double
        let fields: [String: Any]
        let cold: Bool
        var turnEnded: Double?
        var presented: Double?
        init(session: ExactSession, start: Double, committed: Double, fields: [String: Any], cold: Bool) {
            self.session = session
            self.start = start
            self.committed = committed
            self.fields = fields
            self.cold = cold
        }
    }

    /// Who made a queued act that presses as a tap does (`Presenter.hatchAct`):
    /// a native control in the person's input, which happened `at`, or a
    /// script's `click()`, the program's.
    package enum Origin { case person(at: Double), program }
    private var acting: Origin?

    /// Run a queued act as `origin` made it.
    package func acting(_ origin: Origin, _ work: () -> Void) {
        let outer = acting
        acting = origin
        defer { acting = outer }
        work()
    }

    /// The person's input, as an act queued now carries it.
    package func personNow() -> Origin {
        let now = CACurrentMediaTime()
        return .person(at: inputHappened(taken: now) ?? now)
    }

    /// When the input the host took at `taken` happened, `nil` for a script's
    /// act. On iOS it is the event the window is dispatching, as a native
    /// control (a tab bar item, a bar button) acts inside that dispatch, else
    /// `taken` (an action UIKit runs later, VoiceOver's activation). On the
    /// Mac it is AppKit's current event, the last the app took from its
    /// queue, whether a window, a menu or a key equivalent had it, if it came
    /// within the second before.
    func inputHappened(taken: Double) -> Double? {
        switch acting {
        case .person(let at)?: return at
        case .program?: return nil
        case nil: break
        }
        #if os(macOS)
        let personal: Set<NSEvent.EventType> = [.leftMouseDown, .leftMouseUp, .leftMouseDragged, .rightMouseDown, .rightMouseUp,
                                                .rightMouseDragged, .otherMouseDown, .otherMouseUp, .otherMouseDragged, .keyDown,
                                                .keyUp, .scrollWheel, .swipe]
        guard let event = NSApp.currentEvent.flatMap({ personal.contains($0.type) ? $0.timestamp : nil }),
              event <= taken, taken - event <= 1.0 else { return taken }
        return event
        #else
        return min(dispatching ?? taken, taken)
        #endif
    }

    /// When UIKit began its last back swipe: the event it began in, else the
    /// turn.
    private var platformBegan: Double?
    /// That transition's first frame, in seconds on the `CACurrentMediaTime` clock.
    private var platformShowing: Double?
    private var platformTurnEnded: Double?
    private var platformLink: CADisplayLink?

    /// UIKit begins showing a screen itself (`willShow` outside Exact's
    /// projection): its first frame is the next vsync after this turn, as
    /// Core Animation commits the transition at the turn's end.
    package func platformBeganShowing() {
        let now = CACurrentMediaTime()
        platformBegan = inputHappened(taken: now) ?? now
        platformShowing = nil
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            platformTurnEnded = CACurrentMediaTime()
            if platformLink == nil { platformLink = mainDisplayLink(self, #selector(platformTick(_:))) }
        }
    }

    /// That transition was cancelled (a swipe let go): it showed nothing.
    package func platformCancelled() {
        platformBegan = nil
        platformShowing = nil
        platformTurnEnded = nil
        platformLink?.invalidate()
        platformLink = nil
    }

    @objc private func platformTick(_ link: CADisplayLink) {
        defer { platformLink?.invalidate(); platformLink = nil }
        guard let ended = platformTurnEnded, let g = Vsync(link).next(after: ended) else { return }
        platformShowing = g
        platformTurnEnded = nil
    }

    /// `input` is when the person's input whose batch carries the change
    /// happened (`Batch.input`); the change is the program's without one,
    /// or when work that fell due made it (`due`: an overdue timer runs in
    /// an input's dispatch before the input's own action).
    func routerChanged(_ session: ExactSession, payload: [String: Any], input: Double? = nil) {
        let input = payload["due"] as? Bool == true ? nil : input
        guard let top = (payload["top"] as? NSNumber)?.uint64Value else { return }
        let now = CACurrentMediaTime()
        var fields: [String: Any] = ["route": payload["pattern"] as? String ?? "", "url": payload["url"] as? String ?? "",
                                     "routeParams": payload["params"] as? [String: Any] ?? [:]]
        // The first route is the launch route, which startup metrics cover.
        if session.seenNavigationEntries == nil {
            session.seenNavigationEntries = [top]
            fields["entry"] = Int(top)
            ExactJournal.shared.record("navigation.launch", fields)
            note("launch \(fields["route"] ?? "")")
            return
        }
        let cold = session.seenNavigationEntries?.insert(top).inserted == true
        for removed in payload["removed"] as? [NSNumber] ?? [] { session.seenNavigationEntries?.remove(removed.uint64Value) }
        let start = input ?? now
        fields["exact.nav.cause"] = input == nil ? "program" : "input"
        stop()
        if pending != nil { ExactJournal.shared.record("navigation.superseded", ["route": fields["route"] ?? ""]) }
        note("change \(fields["route"] ?? "") \(cold ? "cold" : "warm") \(fields["exact.nav.cause"] ?? "")")
        // A back swipe's change: its screen showed when UIKit began to show
        // it, not now, after the transition.
        defer { platformBegan = nil }
        if let shown = platformShowing, now - shown <= 2.0 {
            platformShowing = nil
            fields["exact.nav.platform"] = true
            let p = Pending(session: session, start: min(platformBegan ?? shown, shown), committed: now, fields: fields, cold: cold)
            pending = p
            p.presented = shown
            var f = fields
            f["name"] = cold ? "cold_ttr" : "warm_ttr"
            f["value"] = shown - p.start
            ExactJournal.shared.record("navigation", f)
            note("\(f["name"] ?? "") \(f["route"] ?? "") \(String(format: "%.1f", (shown - p.start) * 1000)) ms (UIKit's)")
            guard cold else {
                pending = nil
                deadline?.cancel()
                return
            }
            deadline?.cancel()
            let item = DispatchWorkItem { [weak self, weak p] in
                guard let self, let p, pending === p else { return }
                pending = nil
                stop()
            }
            deadline = item
            DispatchQueue.main.asyncAfter(deadline: .now() + ExactLaunch.ttiTimeout, execute: item)
            // Nothing outstanding now: nothing was while UIKit animated
            // either (the screen was there), so it was interactive as shown.
            evaluate(at: shown)
            return
        }
        platformShowing = nil
        let p = Pending(session: session, start: start, committed: now, fields: fields, cold: cold)
        pending = p
        // Core Animation commits at the end of this turn, so the next turn is after it.
        DispatchQueue.main.async { [weak self, weak p] in
            guard let self, let p, pending === p else { return }
            p.turnEnded = CACurrentMediaTime()
            if link == nil { link = mainDisplayLink(self, #selector(tick(_:))) }
        }
        deadline?.cancel()
        let item = DispatchWorkItem { [weak self, weak p] in
            guard let self, let p, pending === p else { return }
            pending = nil
            stop()
        }
        deadline = item
        DispatchQueue.main.asyncAfter(deadline: .now() + ExactLaunch.ttiTimeout, execute: item)
    }

    /// Work ended (`ExactSession.workEnded`): re-checks outstanding work once
    /// the change is shown.
    func workEnded(_ session: ExactSession) {
        guard let p = pending, p.session === session, p.presented != nil, !evaluateScheduled else { return }
        evaluateScheduled = true
        DispatchQueue.main.async { [weak self] in
            self?.evaluateScheduled = false
            self?.evaluate()
        }
    }

    private func stop() {
        link?.invalidate()
        link = nil
    }

    @objc private func tick(_ link: CADisplayLink) {
        guard let p = pending, let ended = p.turnEnded, p.presented == nil, let g = Vsync(link).next(after: ended) else { return }
        // Only the first frame needs a link; work that ends later re-checks TTI
        stop()
        p.presented = g
        var f = p.fields
        f["name"] = p.cold ? "cold_ttr" : "warm_ttr"
        f["value"] = g - p.start
        ExactJournal.shared.record("navigation", f)
        note("\(f["name"] ?? "") \(f["route"] ?? "") \(String(format: "%.1f", (g - p.start) * 1000)) ms")
        guard p.cold else {
            pending = nil
            deadline?.cancel()
            stop()
            return
        }
        evaluate()
    }

    /// An evaluation's read is on its way, and another was asked meanwhile.
    private var reading = false, readAgain = false

    /// Records TTI once nothing is outstanding after the change was shown:
    /// at `clear` if no work ended meanwhile, else the ledger's time, never
    /// before the change showed. Main does not wait for the read
    /// (`readLedger`), and one is on its way at a time.
    private func evaluate(at clear: Double? = nil) {
        guard let p = pending, p.presented != nil, let session = p.session else { return }
        guard !reading else { readAgain = true; return }
        reading = true
        ExactLaunch.readLedger(session) { [weak self, weak p] ledger in
            guard let self else { return }
            reading = false
            defer { if readAgain { readAgain = false; evaluate() } }
            guard let p, pending === p, let presented = p.presented,
                  let ledger, ledger.items.isEmpty, !ledger.poisoned else { return }
            pending = nil
            readAgain = false
            deadline?.cancel()
            stop()
            var f = p.fields
            f["name"] = "tti"
            // Work that landed meanwhile was outstanding when UIKit showed the screen.
            f["value"] = max(ledger.quiet ? clear ?? ledger.at : ledger.at, presented) - p.start
            if !ledger.failed.isEmpty { f["exact.tti.failed"] = ledger.failed.count }
            ExactJournal.shared.record("navigation", f)
            note("tti \(f["route"] ?? "") \(String(format: "%.1f", (f["value"] as? Double ?? 0) * 1000)) ms")
        }
    }
}
