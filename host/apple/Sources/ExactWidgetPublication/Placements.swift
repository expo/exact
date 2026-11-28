import Foundation
import ExactCore
import os

package enum WidgetPlacements {
    static let settling: TimeInterval = 120, interval: TimeInterval = 30
    private static let log = Logger(subsystem: "dev.exact.widgets", category: "placements")
    private static var installed = false
    static var activated = Date.distantPast
    static var reloaded: [String: (placed: Int, record: WidgetPlacementRecords.Record?, at: Date)] = [:]
    static var unsettled: [String: Date] = [:]
    private static var recheck = 0
    static var now: () -> Date = Date.init
    static var schedule: (TimeInterval, @escaping () -> Void) -> Void = { delay, work in
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
    }
    static var parameters = Bundle.main.object(forInfoDictionaryKey: "ExactWidgetParameters") as? [String: [String]] ?? [:]
    static var countsArtifact = Bundle.main.privateFrameworksURL?.appendingPathComponent("ExactWidgetReload.framework/ExactWidgetReload")

    static func install(root: URL? = WidgetContainer.root, appID: String? = WidgetContainer.appID) {
        guard !installed else { return }
        installed = true
        let center = NotificationCenter.default
        center.addObserver(forName: Notification.Name("exact.widgets.placed"), object: nil, queue: nil) { note in
            guard let body = note.object as? NSMutableDictionary, let kind = body["kind"] as? String,
                  let reply = body["reply"] as? ([String: Any]) -> Void else { return }
            body["taken"] = true
            DispatchQueue.main.async { answer(kind: kind, root: root, reply: reply) }
        }
        for name in ["UIApplicationDidBecomeActiveNotification", "NSApplicationDidBecomeActiveNotification"] {
            center.addObserver(forName: Notification.Name(name), object: nil, queue: .main) { _ in activated = now(); reloaded = [:]; changed() }
        }
        guard let appID else { return }
        CFNotificationCenterAddObserver(CFNotificationCenterGetDarwinNotifyCenter(), nil, { _, _, _, _, _ in
            DispatchQueue.main.async { WidgetPlacements.changed() }
        }, (appID + ".widget-placements") as CFString, nil, .deliverImmediately)
    }

    static func changed() {
        NotificationCenter.default.post(name: Notification.Name("exact.widgets.placementsChanged"), object: nil)
    }

    package static func answer(kind: String, root: URL?, counts: ((@escaping ([String: Int]?) -> Void) -> Void)? = nil, reload: @escaping (String) -> Void = reloadKind, reply: @escaping ([String: Any]) -> Void) {
        let record = WidgetPlacementRecords.read(root: root)[kind]
        let written = WidgetPlacementRecords.written(root: root) ?? .distantPast
        (counts ?? placedCounts) { counts in
            let time = now()
            let unknown = {
                let since = unsettled[kind] ?? time
                unsettled[kind] = since
                if time.timeIntervalSince(max(written, activated, since)) < settling { checkAgain() }
                reply(["ready": false, "placements": [Any]()])
            }
            guard let counts else {
                log.notice("placedWidgets \(kind, privacy: .public): WidgetKit gave the app no count")
                return unknown()
            }
            let placed = counts[kind] ?? 0
            guard case .placed(let placements) = WidgetPlacementRecords.answer(record, parameters: parameters[kind], placed: placed, now: time) else {
                log.notice("placedWidgets \(kind, privacy: .public): \(placed) placed, \(record?.count ?? 0) recorded")
                let last = reloaded[kind]
                if last?.placed != placed || last?.record != record || time.timeIntervalSince(last?.at ?? .distantPast) >= interval {
                    reloaded[kind] = (placed, record, time)
                    reload(kind)
                }
                return unknown()
            }
            reloaded[kind] = nil
            unsettled[kind] = nil
            if time.timeIntervalSince(max(written, activated)) < settling { checkAgain() }
            log.debug("placedWidgets \(kind, privacy: .public): ready, \(placed) placed, \(placements.count) records")
            reply(["ready": true, "placements": placements.map { $0.mapValues { $0.map { $0 as Any } ?? NSNull() } }])
        }
    }

    private static func checkAgain() {
        recheck += 1
        let mine = recheck
        schedule(interval) { if mine == recheck { changed() } }
    }

    private static func reloadKind(_ kind: String) {
        DispatchQueue.main.async { WidgetContainer.reloadAfterStoreChange(kinds: [kind]) }
    }

    private typealias Done = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int) -> Void
    private typealias Counts = @convention(c) (UnsafeMutableRawPointer?, Done) -> Void
    private static var countsCall: Counts?
    private final class Pending { let done: ([String: Int]?) -> Void; init(_ done: @escaping ([String: Int]?) -> Void) { self.done = done } }

    static func placedCounts(_ done: @escaping ([String: Int]?) -> Void) {
        if countsCall == nil, let path = countsArtifact?.path, let library = dlopen(path, RTLD_NOW | RTLD_LOCAL),
           let symbol = dlsym(library, "exact_widget_counts") {
            countsCall = unsafeBitCast(symbol, to: Counts.self)
        }
        guard let countsCall else { return done(nil) }
        countsCall(Unmanaged.passRetained(Pending(done)).toOpaque()) { context, bytes, length in
            guard let context else { return }
            let pending = Unmanaged<Pending>.fromOpaque(context).takeRetainedValue()
            let counts = bytes.flatMap { try? JSONSerialization.jsonObject(with: Data(bytes: $0, count: length)) as? [String: Int] }
            DispatchQueue.main.async { pending.done(counts) }
        }
    }
}
