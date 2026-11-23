#if os(macOS)
import AppKit
import ExactCore

extension ExactSession {
    var nativeTextThenWaiting: Bool { nativeText.owed || nativeText.posted != nil || nativeText.stalled }

    func applyNativeTextInput(_ id: UInt32, _ value: String, _ selection: FieldSelection, _ edit: FieldInput) {
        (presenter.views[id]?.textArea as? TextArea)?.ordinaryInputSelection = (value, selection)
        let kind: UInt32 = postsTimerAdvances && clock == nil && !ExactEnv.agentFreezes ? 65 : 54
        nativeText.advances = 0
        let payload = edit.payload(value, selection)
        if kind == 65, canPostNativeText, pending.isEmpty, !landing, !insideApply,
           !fillInFlight, !fillLanding, !tickInFlight, !canvasInFlight, noticesInFlight == 0 {
            followOffset()
            nativeText.posted = .input
            runtime.postNativeTextInput(id, payload, now: now(), input: CACurrentMediaTime()) { [weak self] batch in
                self?.publish(.nativeTextInput, batch)
            }
        } else {
            applyInput(runtime.fieldEvent(id, kind, payload, now: now()), at: id)
        }
    }

    private var canPostNativeText: Bool {
        guard postsTimerAdvances, state == .ready, !frames.tasks, stage == nil, !runtime.held,
              !Owner.shared.isServingMain, nativeText.posted == nil else { return false }
        let layer = presenter.openTopLayer
        return layer.frames.isEmpty && layer.modal == nil && layer.backdrop == nil
    }

    func advanceNativeText() {
        guard !nativeText.stalled else { return }
        // A change can yield for presentation and then rearm itself.
        guard nativeText.advances < 128 else { stallNativeText(); return }
        nativeText.advances += 1
        if !nativeText.settling, canPostNativeText {
            nativeText.posted = .continuation
            advanceInFlight = true
            runtime.advanceNativeTextAsync { [weak self] batch in self?.publish(.advance, batch) }
        } else {
            let wasAdvancing = advancingDue
            advancingDue = true
            defer { advancingDue = wasAdvancing }
            let batch = runtime.landThen(nativeText: true)
            if batch.held || batch.busy { stallNativeText(); return }
            applyUnlessEmpty(batch)
        }
    }

    func nativeTextAdvancePublished(_ completion: RuntimeCompletion) {
        guard nativeText.posted == .continuation else { return }
        nativeText.posted = nil
        if runtime.accepts(completion.lease), completion.batch.held || completion.batch.busy {
            stallNativeText()
        }
    }

    private func stallNativeText() {
        if !nativeText.stalled { log("The native text input did not settle.") }
        nativeText.stalled = true
    }

    // A user action sees the previous edit and its continuation. A later
    // action can try another bounded turn after a refusal.
    func settleNativeText(retry: Bool = true, drainEarlierWork: Bool = false) -> Bool {
        guard nativeTextThenWaiting || drainEarlierWork && (ownerInFlight || stage != nil || hasPublished()) else { return true }
        guard !nativeText.settling, !landing, !insideApply, !Owner.shared.isServingMain, state != .destroyed else { return false }
        if nativeText.stalled {
            guard retry, nativeText.owed, !runtime.held else { return false }
            nativeText.stalled = false
            nativeText.advances = 0
        }
        nativeText.settling = true
        defer { nativeText.settling = false; deliverAfterNativeText() }
        repeat {
            drainFill()
            if nativeText.posted == nil, nativeText.owed, state != .destroyed {
                advanceNativeText()
                drainFill()
            }
        } while nativeTextThenWaiting && !nativeText.stalled && state != .destroyed
        return !nativeTextThenWaiting
    }

    func deliverAfterNativeText() {
        guard !insideApply, !landing, !nativeText.settling, !nativeTextThenWaiting, !nativeText.after.isEmpty else { return }
        let work = nativeText.after
        nativeText.after.removeAll()
        for deliver in work { deliver() }
    }
}

extension Presenter {
    func settledInputTarget(_ id: UInt32) -> NodeView? {
        guard let target = textHost(id) else { return nil }
        if let session, session.nativeTextThenWaiting {
            guard session.settleNativeText(), textHost(id) === target else { return nil }
        }
        return target
    }
}

enum NativeTextJob: Equatable { case input, continuation }

struct NativeTextContinuation {
    var owed = false
    var posted: NativeTextJob?
    var stalled = false
    var settling = false
    var advances = 0
    var after: [() -> Void] = []
}
#endif
