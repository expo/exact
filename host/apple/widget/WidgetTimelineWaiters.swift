// Coalesce data refreshes while retaining each family's layout proposal.
#if canImport(WidgetKit)
import ExactCore
import ExactSnapshot
import Foundation
import WidgetKit

public final class WidgetTimelineWaiters<Entry: TimelineEntry>: @unchecked Sendable {
    private let lock = NSLock()
    private var waiting: [WidgetTimelineCache: [(CGSize, (Timeline<Entry>) -> Void)]] = [:]
    private let stamp: (Entry, CGSize) -> Entry
    public init(stamp: @escaping (Entry, CGSize) -> Entry) { self.stamp = stamp }
    public func begin(_ cache: WidgetTimelineCache, size: CGSize, completion: @escaping (Timeline<Entry>) -> Void) -> Bool {
        lock.lock(); defer { lock.unlock() }
        let first = waiting[cache] == nil
        waiting[cache, default: []].append((size, completion))
        return first
    }
    /// `revision` is the store revision `timeline` was computed under, which can be newer than the
    /// one `cache` was begun with when a store write restarted the refresh.
    public func finish(_ cache: WidgetTimelineCache, _ timeline: Timeline<Entry>, revision: WidgetTimelineCache, fallback: Timeline<Entry>) {
        lock.lock()
        let callbacks = waiting.removeValue(forKey: cache) ?? []
        lock.unlock()
        let answer = revision.isCurrent ? timeline : fallback
        for (size, callback) in callbacks {
            callback(Timeline(entries: answer.entries.map { stamp($0, size) }, policy: answer.policy))
        }
    }
}
#endif
