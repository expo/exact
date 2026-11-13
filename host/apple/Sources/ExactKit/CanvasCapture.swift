// @ref LLP 1056 D9 (as amended for expo/exact#188) — a canvas capture's
// presenter half: the runtime names the canvas (`mode: "canvas"` surface
// work), the presenter encodes the bitmap it keeps, and the runtime writes
// the PNG to the app file and answers the source (`fulfillSurface` kind 8).
import Foundation
import ExactCore

extension ExactSession {
    /// Replay finishes before the snapshot on the presenter's serial queue.
    func drainCanvasCaptures(_ batch: Batch) {
        let work = pendingSurfaceWork
        pendingSurfaceWork = []
        for (op, owner) in work {
            guard op["mode"] as? String == "canvas" else { pendingSurfaceWork.append((op, owner)); continue }
            let ticket = (op["ticket"] as? NSNumber)?.uint64Value ?? 0
            guard generation == owner, runtime.requestActive(ticket) else { continue }
            captureCanvas(op)
        }
    }

    package func completeSurface(_ ticket: UInt64, generation owner: Int, kind: UInt32, body: Data = Data()) {
        guard state != .destroyed, generation == owner, runtime.requestActive(ticket) else { return }
        apply(runtime.fulfillSurface(ticket, kind: kind, body: body, now: now()))
    }

    /// One capture op; a refusal the runtime already made goes back on the
    /// next turn, outside the batch being applied, with nothing to write.
    func captureCanvas(_ op: [String: Any]) {
        let owner = generation, num = { (key: String) in (op[key] as? NSNumber)?.uint64Value ?? 0 }
        let ticket = num("ticket")
        guard op["refused"] == nil else {
            DispatchQueue.main.async { [weak self] in self?.completeSurface(ticket, generation: owner, kind: 8) }
            return
        }
        presenter.canvas2d.capture(UInt32(num("id")), lifetime: num("lifetime"), generation: UInt32(num("generation")), sequence: num("sequence"),
                                   width: Int(num("w")), height: Int(num("h")),
                                   snapshot: { [weak self] copy in
            DispatchQueue.main.sync {
                guard let self, self.state != .destroyed, self.generation == owner else { return false }
                return self.runtime.snapshotCanvas(ticket, copy: copy)
            }
        }) { [weak self] png in
            self?.completeSurface(ticket, generation: owner, kind: 8, body: png ?? Data())
        }
    }
}
