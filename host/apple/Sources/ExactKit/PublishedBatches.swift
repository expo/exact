import Foundation

/// Owner publications in order. A synchronous drain can run before the queued
/// one; that queued drain still owns scheduling until it has checked for work.
final class PublishedBatches {
    typealias Item = (kind: ExactSession.Published, completion: RuntimeCompletion)
    private let lock = NSLock()
    private var items: [Item?] = []
    private var cursor = 0
    private var count = 0
    private var scheduled = false
    #if DEBUG
    private var drains = 0
    var scheduledDrains: Int { lock.withLock { drains } }
    #endif

    func append(_ item: Item) -> Bool {
        lock.withLock {
            if count == items.count {
                var grown = [Item?](repeating: nil, count: max(1, items.count * 2))
                for offset in 0..<count { grown[offset] = items[(cursor + offset) % items.count] }
                items = grown
                cursor = 0
            }
            items[(cursor + count) % items.count] = item
            count += 1
            guard !scheduled else { return false }
            scheduled = true
            #if DEBUG
            drains += 1
            #endif
            return true
        }
    }

    func contains(_ which: (Batch) -> Bool) -> Bool {
        lock.withLock {
            for offset in 0..<count where which(items[(cursor + offset) % items.count]!.completion.batch) { return true }
            return false
        }
    }

    func peek() -> Item? { lock.withLock { count > 0 ? items[cursor] : nil } }
    /// The first publication's place in the runtime's order (`Batch.sequence`).
    var earliest: UInt64? { peek()?.completion.batch.sequence }

    func next() -> Item? {
        lock.withLock {
            guard count > 0 else { return nil }
            let item = items[cursor]!
            items[cursor] = nil
            cursor = (cursor + 1) % items.count
            count -= 1
            return item
        }
    }

    /// A queued callback ran inside a synchronous landing's run-loop service.
    /// That landing will consume the work; no queued callback remains.
    func yieldDrain() { lock.withLock { scheduled = false } }

    /// A landing left publications made after its batch: true when no turn
    /// is scheduled to land them, and the caller schedules one.
    func claimDrain() -> Bool {
        lock.withLock {
            guard count > 0, !scheduled else { return false }
            scheduled = true
            #if DEBUG
            drains += 1
            #endif
            return true
        }
    }

    /// A publication can arrive after the drain found the queue empty.
    /// Keep its scheduled turn until that publication has landed too.
    func finishDrain() -> Bool {
        lock.withLock {
            if count > 0 {
                #if DEBUG
                drains += 1
                #endif
                return true
            }
            scheduled = false
            return false
        }
    }
}

extension ExactSession {
    /// On the owner: queue a batch for main, in the owner's order.
    func publish(_ kind: Published, _ completion: RuntimeCompletion) {
        var completion = completion
        if completion.batch.sequence == 0 { // a batch the host made, numbered as it is published
            var batch = completion.batch
            batch.sequence = runtime.sequence.next()
            completion = RuntimeCompletion(lease: completion.lease, batch: batch)
        }
        guard published.append((kind, completion)) else { return }
        DispatchQueue.main.async { [weak self] in self?.landPublished() }
    }
    /// Wait for the slice in flight and land it: what shows, the agent and
    /// every read that must see it (T5, T7, T9).
    /// The outer drain reaches a fixed point with nothing left to land (T13).
    /// A callback inside a landing leaves queued batches to that landing.
    public func drainFill() {
        while true {
            while stage != nil { finishStage() }
            guard !landing, ownerInFlight || hasPublished() || !pending.isEmpty else { return }
            Owner.shared.sync {}
            landFill()
            if stage == nil, !hasPublished(), pending.isEmpty { return }
        }
    }

    func landPublished() {
        // Stopped at a staged batch: its reveal lands the rest (`finishStage`).
        if landing || landFill() { published.yieldDrain(); return }
        if published.finishDrain() { DispatchQueue.main.async { [weak self] in self?.landPublished() } }
    }

    /// A published batch waits to land: any, or one that `which` picks.
    func hasPublished(_ which: (Batch) -> Bool = { _ in true }) -> Bool {
        published.contains(which)
    }
}
