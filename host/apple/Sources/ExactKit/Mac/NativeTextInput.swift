#if os(macOS)
import AppKit
import ExactCore

extension ExactSession {
    var nativeTextThenWaiting: Bool { nativeText.owed || nativeText.posted || nativeText.stalled }

    func applyNativeTextInput(_ id: UInt32, _ value: String, _ selection: FieldSelection, _ edit: FieldInput) {
        (presenter.views[id]?.textArea as? TextArea)?.ordinaryInputSelection = (value, selection)
        let kind: UInt32 = postsTimerAdvances && clock == nil && !ExactEnv.agentFreezes ? 65 : 54
        nativeText.advances = 0
        applyInput(runtime.fieldEvent(id, kind, edit.payload(value, selection), now: now()), at: id)
    }

    func advanceNativeText() {
        guard !nativeText.stalled else { return }
        // A change can yield for presentation and then rearm itself.
        guard nativeText.advances < 128 else { stallNativeText(); return }
        nativeText.advances += 1
        let layer = presenter.openTopLayer
        if !nativeText.settling, postsTimerAdvances, !frames.tasks, stage == nil, !runtime.held,
           layer.frames.isEmpty, layer.modal == nil, layer.backdrop == nil {
            nativeText.posted = true
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
        guard nativeText.posted else { return }
        nativeText.posted = false
        if runtime.accepts(completion.lease), completion.batch.held || completion.batch.busy { stallNativeText() }
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
            if !nativeText.posted, nativeText.owed, state != .destroyed {
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

struct NativeTextContinuation {
    var owed = false
    var posted = false
    var stalled = false
    var settling = false
    var advances = 0
    var after: [() -> Void] = []
}
#endif
