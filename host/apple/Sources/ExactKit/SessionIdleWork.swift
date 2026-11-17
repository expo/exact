// The work a session runs when idle (`whenIdle`), in the order asked, across a slice's landing (LLP 1072 §3).
import Foundation

extension ExactSession {
    /// View reports expire after reset; executor work survives a plan commit.
    package func whenIdle(bound: Bool = false, _ work: @escaping () -> Void, cancelled: (() -> Void)? = nil) {
        idle((bound ? viewsEpoch : nil, work, cancelled, coversCause))
    }
    private func idle(_ item: IdleWork) {
        if state == .destroyed || (item.views != nil && item.views != viewsEpoch) { item.cancelled?() }
        else if fillInFlight || fillLanding || draining || !afterFill.isEmpty { afterFill.append(item) }
        else { item.work() }
    }

    /// Run the work that waited, and what it asks meanwhile, in order, until none is left or a slice is in flight. A drain
    /// runs a thousand at most, so work that keeps asking for more cannot hold main without end; the rest runs next turn,
    /// unsettled for reads and the frame until then. A drain asked inside one (a slice landed by work it runs) leaves the
    /// queue to it, which runs the rest after that work. `destroy` cancels what is left.
    func runWaitingWork() {
        guard !draining else { return }
        draining = true; defer { draining = false }
        var ran = 0
        while !fillInFlight, !fillLanding, !afterFill.isEmpty {
            guard ran < 1_000 else { log("The session's waiting work did not empty in 1,000 runs; the rest runs on the main queue's next turn"); return DispatchQueue.main.async { [weak self] in self?.runWaitingWork() } }
            ran += 1
            let item = afterFill.removeFirst()
            if state == .destroyed || (item.views != nil && item.views != viewsEpoch) { item.cancelled?() } else { inCoversChain(item.cause, item.work) }
        }
    }
}
