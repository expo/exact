import Foundation
import ExactCore
import os

package enum WidgetPlacements {
    private static var reloaded: [String: Int] = [:]
    static let settling: TimeInterval = 120, interval: TimeInterval = 30
    private static let log = Logger(subsystem: "dev.exact.widgets", category: "placements")
    private static var recheck: DispatchWorkItem?
    private static var activated = Date.distantPast
    static var countsArtifact = Bundle.main.privateFrameworksURL?.appendingPathComponent("ExactWidgetReload.framework/ExactWidgetReload")

    static func install(root: URL? = WidgetContainer.root, appID: String? = WidgetContainer.appID) {
        let center = NotificationCenter.default
        center.addObserver(forName: Notification.Name("exact.widgets.placed"), object: nil, queue: nil) { note in
            guard let body = note.object as? NSMutableDictionary, let kind = body["kind"] as? String,
                  let reply = body["reply"] as? ([String: Any]) -> Void else { return }
            body["taken"] = true
            DispatchQueue.main.async { answer(kind: kind, root: root, reply: reply) }
        }
        for name in ["UIApplicationDidBecomeActiveNotification", "NSApplicationDidBecomeActiveNotification"] {
            center.addObserver(forName: Notification.Name(name), object: nil, queue: .main) { _ in activated = Date(); reloaded = [:]; changed() }
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
        let file = root?.appendingPathComponent("widget-placements.json")
        let recorded = file.flatMap { try? Data(contentsOf: $0) }
            .flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: [[String: Any]]] }?[kind]
        let written = file.flatMap { try? FileManager.default.attributesOfItem(atPath: $0.path)[.modificationDate] as? Date }
        (counts ?? placedCounts) { counts in
            guard let counts else { return reply(["ready": false, "placements": [Any]()]) }
            let placed = counts[kind] ?? 0
            if placed == 0 { reloaded[kind] = nil; return reply(["ready": true, "placements": [Any]()]) }
            guard let recorded, recorded.count == placed else {
                log.notice("placedWidgets \(kind, privacy: .public): \(placed) placed, \(recorded?.count ?? 0) recorded")
                if reloaded[kind] != placed { reloaded[kind] = placed; reload(kind) }
                return reply(["ready": false, "placements": [Any]()])
            }
            reloaded[kind] = nil
            if Date().timeIntervalSince(max(written ?? .distantPast, activated)) < settling { checkAgain(after: interval) }
            reply(["ready": true, "placements": recorded])
        }
    }

    private static func checkAgain(after delay: TimeInterval) {
        recheck?.cancel()
        let work = DispatchWorkItem { recheck = nil; changed() }
        recheck = work
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
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
