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
        guard published.append((kind, completion)) else { return }
        DispatchQueue.main.async { [weak self] in self?.landPublished() }
    }
    func landPublished() {
        if landing { published.yieldDrain(); return }
        landFill()
        if published.finishDrain() { DispatchQueue.main.async { [weak self] in self?.landPublished() } }
    }

    /// A published batch waits to land: any, or one that `which` picks.
    func hasPublished(_ which: (Batch) -> Bool = { _ in true }) -> Bool {
        published.contains(which)
    }
}
