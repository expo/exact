import Foundation

// @ref LLP 1172.005 E11 — a queued runtime job belongs to the session and
// generation that asked, including the interval before its batch lands.
package struct RuntimeLease {
    package let session: RuntimeDoor
    package let generation: Int
}

package struct RuntimeCompletion {
    package let lease: RuntimeLease
    package let batch: Batch
    package init(lease: RuntimeLease, batch: Batch) { self.lease = lease; self.batch = batch }
}

package final class RuntimeDoor {
    private let lock = NSLock()
    private var generation = 0
    private var alive = true
    package func lease() -> RuntimeLease {
        lock.withLock { RuntimeLease(session: self, generation: generation) }
    }

    package func accepts(_ lease: RuntimeLease) -> Bool {
        lock.withLock { alive && lease.session === self && lease.generation == generation }
    }

    /// Called in the owner transaction that replaces the runner, before
    /// another queued job can run. A refused replacement keeps its lease.
    package func replace() {
        lock.withLock { generation += 1 }
    }

    /// Admission stops before a destroy deferred by a served callback.
    package func close() {
        lock.withLock {
            if alive { alive = false; generation += 1 }
        }
    }
}

extension Runtime {
    package var completionLease: RuntimeLease { completions.lease() }

    package func accepts(_ lease: RuntimeLease) -> Bool { completions.accepts(lease) }

    /// Refused jobs still answer once per expected batch so the caller can
    /// retire its in-flight work. No lease lock is held across a main callback.
    /// `layout` as `enter`'s.
    package func post(count: Int = 1, caller: String = #function, layout: Bool = false, prepare: () -> Void = {},
                      done: @escaping (RuntimeCompletion) -> Void, _ body: @escaping (Int) -> Batch) {
        let lease = completionLease
        let held = enter(caller, layout: layout)
        if !held { prepare() }
        Owner.shared.post { [self] in
            for index in 0..<count {
                let batch = !held && accepts(lease) ? body(index) : Runtime.heldBatch
                done(RuntimeCompletion(lease: lease, batch: batch))
            }
        }
    }

    package func replaced(_ batch: Batch) -> Batch {
        if batch.error == nil && !batch.pending && !batch.busy && !batch.held { completions.replace() }
        return batch
    }
}
