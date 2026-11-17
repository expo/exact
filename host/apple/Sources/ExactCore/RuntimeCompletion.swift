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
    private var scrollSequence: UInt64 = 0
    private struct ScrollOrder {
        var applied: UInt64 = 0
        var pending = 0
    }
    private final class ScrollWrite {
        let door: RuntimeDoor
        let generation: Int, view: UInt32, sequence: UInt64
        var pending = true

        init(_ door: RuntimeDoor, generation: Int, view: UInt32, sequence: UInt64) {
            self.door = door; self.generation = generation; self.view = view; self.sequence = sequence
        }

        deinit { if pending { _ = door.finishScroll(self, applying: false) } }
    }
    private var scrolls: [UInt32: ScrollOrder] = [:]
    #if DEBUG
    package var pendingScrollViewsForTests: Int { lock.withLock { scrolls.count } }
    #endif

    package func lease() -> RuntimeLease {
        lock.withLock { RuntimeLease(session: self, generation: generation) }
    }

    package func accepts(_ lease: RuntimeLease) -> Bool {
        lock.withLock { alive && lease.session === self && lease.generation == generation }
    }

    /// Queued observations and mailbox facts share an order. A mailbox may
    /// run first, but an older notification must never restore its offset.
    package func scroll(_ lease: RuntimeLease, view: UInt32, _ body: @escaping () -> Void) -> () -> Void {
        let write: ScrollWrite? = lock.withLock {
            guard alive, lease.session === self, lease.generation == generation else { return nil }
            scrollSequence += 1
            scrolls[view, default: ScrollOrder()].pending += 1
            return ScrollWrite(self, generation: generation, view: view, sequence: scrollSequence)
        }
        guard let write else { return {} }
        return { if self.finishScroll(write, applying: true) { body() } }
    }

    private func finishScroll(_ write: ScrollWrite, applying: Bool) -> Bool {
        lock.withLock {
            guard write.pending else { return false }
            write.pending = false
            guard alive, write.generation == generation, var order = scrolls[write.view] else { return false }
            let apply = applying && write.sequence > order.applied
            if apply { order.applied = write.sequence }
            order.pending -= 1
            // The watermark is needed only while an older write can arrive.
            scrolls[write.view] = order.pending == 0 ? nil : order
            return apply
        }
    }

    /// Called in the owner transaction that replaces the runner, before
    /// another queued job can run. A refused replacement keeps its lease.
    package func replace() {
        lock.withLock { generation += 1; scrolls.removeAll(keepingCapacity: true) }
    }

    /// Admission stops before a destroy deferred by a served callback.
    package func close() {
        lock.withLock {
            if alive { alive = false; generation += 1 }
            scrolls.removeAll()
        }
    }
}

extension Runtime {
    package var completionLease: RuntimeLease { completions.lease() }

    package func accepts(_ lease: RuntimeLease) -> Bool { completions.accepts(lease) }

    /// Refused jobs still answer once per expected batch so the caller can
    /// retire its in-flight work. No lease lock is held across a main callback.
    package func post(count: Int = 1, caller: String = #function, prepare: () -> Void = {},
                      done: @escaping (RuntimeCompletion) -> Void, _ body: @escaping (Int) -> Batch) {
        let lease = completionLease
        let held = enter(caller)
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
