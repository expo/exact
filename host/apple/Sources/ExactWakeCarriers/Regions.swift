// The region carrier (LLP 1133.021 D14): one `CLMonitor` named
// `<bundle id>.regions` and the store of regions and their undelivered
// transitions. On iOS a relaunch for a region event must "recreate the
// monitor with the same identifier" before anything else, which is before a
// package's Swift loads with the app's module, so the carrier is the
// host's, made at install when the store holds a region, and a package
// reaches it through `context.regions`.
//
// The module side reaches it through `NotificationCenter` (`exact.regions`,
// an `NSMutableDictionary` whose `reply` the carrier sets): `{op: add,
// package, name, latitude, longitude, radius}`, `{op: remove, package,
// name}`, `{op: list, package}`, `{op: pending, package, name}`, `{op:
// delivered, package, name, event, timestamp}`.
//
// The store is the truth (D14): a crossing is written to it first, the
// region's `inside` set and its one undelivered transition replaced, then
// offered to the package that added it as a wake of kind `region`, its keys
// the regions' names. A transition stays until the package says that one
// was delivered, and is offered again whenever a module becomes ready, which
// a later launch's is after its first pixel. One written while its region is
// in a wake is offered when that wake ends.
import Foundation
import ExactKit
#if os(iOS) || os(macOS)
import CoreLocation
#endif
#if os(iOS)
import UIKit
#endif

public enum RegionCarrier {
    struct Region: Codable {
        var package: String, name: String, latitude: Double, longitude: Double, radius: Double
        var inside: Bool?
        /// The undelivered transition, the latest: `enter` or `leave`, and when.
        var pending: String?
        var at: Double?
    }

    static var regions: [String: Region] = [:]
    private static var installed = false
    /// Region keys in a wake now, not offered again until it ends.
    private static var offering: Set<String> = []
    /// Region keys in a wake whose transition changed meanwhile: offered again when it ends.
    private static var crossedWhileOffering: Set<String> = []

    /// Namespaced key and `CLMonitor` condition identifier for `(package, name)`.
    static func key(package: String, name: String) -> String { safe(package) + ":" + safe(name) }
    private static func safe(_ s: String) -> String {
        s.addingPercentEncoding(withAllowedCharacters: .alphanumerics.union(CharacterSet(charactersIn: "-_"))) ?? s
    }

    /// Under the app's support directory, not purgeable; on the Mac under its bundle id.
    static let file: URL = {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        #if os(macOS)
        let dir = support.appendingPathComponent(Bundle.main.bundleIdentifier ?? "app").appendingPathComponent("regions")
        #else
        let dir = support.appendingPathComponent("exact-regions")
        #endif
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent("store.json")
    }()

    private static func keep() {
        if let data = try? JSONEncoder().encode(Array(regions.values)) { try? data.write(to: file, options: .atomic) }
    }

    /// At launch: the store read back, and the monitor made again if it holds a region.
    static func install() {
        guard !installed else { return }
        installed = true
        if let data = try? Data(contentsOf: file), let kept = try? JSONDecoder().decode([Region].self, from: data) {
            regions = Dictionary(kept.map { (key(package: $0.package, name: $0.name), $0) }, uniquingKeysWith: { $1 })
        }
        NotificationCenter.default.addObserver(forName: Notification.Name("exact.regions"), object: nil, queue: nil) { note in
            guard let box = note.object as? NSMutableDictionary else { return }
            let run = { box["reply"] = handle(box as? [String: Any] ?? [:]) }
            if Thread.isMainThread { run() } else { DispatchQueue.main.sync(execute: run) }
        }
        #if os(iOS) || os(macOS)
        if !regions.isEmpty { Monitor.start() }
        #endif
    }

    private static func handle(_ body: [String: Any]) -> Any? {
        let package = body["package"] as? String ?? "", name = body["name"] as? String ?? ""
        let k = key(package: package, name: name)
        switch body["op"] as? String {
        case "add":
            #if os(iOS) || os(macOS)
            let r = Region(package: package, name: name, latitude: body["latitude"] as? Double ?? 0, longitude: body["longitude"] as? Double ?? 0,
                           radius: body["radius"] as? Double ?? 100)
            regions[k] = r
            keep()
            WakeStore.log("regions: added \(name)")
            Monitor.add(k)
            return nil
            #else
            return "NotSupportedError"
            #endif
        case "remove":
            guard regions.removeValue(forKey: k) != nil else { return nil }
            keep()
            WakeStore.log("regions: removed \(name)")
            #if os(iOS) || os(macOS)
            Monitor.remove(k)
            #endif
            return nil
        case "list":
            return regions.values.filter { $0.package == package }.sorted { $0.name < $1.name }.map { r -> [String: Any] in
                var out: [String: Any] = ["name": r.name, "latitude": r.latitude, "longitude": r.longitude, "radius": r.radius]
                if let inside = r.inside { out["inside"] = inside }
                return out
            }
        case "pending":
            guard let r = regions[k], let event = r.pending else { return nil }
            return ["event": event, "timestamp": r.at ?? 0]
        case "delivered":
            // A later crossing may have replaced the transition raised.
            guard let r = regions[k], r.pending == body["event"] as? String,
                  r.at == (body["timestamp"] as? NSNumber)?.doubleValue else { return nil }
            regions[k]?.pending = nil
            regions[k]?.at = nil
            keep()
            return nil
        default:
            return nil
        }
    }

    /// The platform reported region `identifier` `inside` or not at `at` (ms):
    /// written to the store, then offered as a wake.
    static func crossed(_ identifier: String, inside: Bool, at: Double) {
        guard var r = regions[identifier], r.inside != inside else { return }
        r.inside = inside
        r.pending = inside ? "enter" : "leave"
        r.at = at
        regions[identifier] = r
        keep()
        WakeStore.log("regions: \(r.name) \(inside ? "entered" : "left")")
        if offering.contains(identifier) { crossedWhileOffering.insert(identifier) }
        offer([identifier])
    }

    /// A module is ready: the transitions kept undelivered are offered.
    static func ready() {
        offer(regions.filter { $0.value.pending != nil }.map(\.key))
    }

    private static func offer(_ keys: [String]) {
        let fresh = keys.filter { !offering.contains($0) }
        guard !fresh.isEmpty else { return }
        offering.formUnion(fresh)
        #if os(iOS)
        // Its time bounds the wake (D14 step 1).
        var task = UIBackgroundTaskIdentifier.invalid
        task = UIApplication.shared.beginBackgroundTask(withName: "exact.regions") {
            UIApplication.shared.endBackgroundTask(task)
            task = .invalid
        }
        #endif
        WakeStore.region(fresh.compactMap { k in regions[k].map { ($0.package, $0.name) } }) { _ in
            offering.subtract(fresh)
            let again = crossedWhileOffering.intersection(fresh)
            crossedWhileOffering.subtract(again)
            offer(again.filter { regions[$0]?.pending != nil }.sorted())
            #if os(iOS)
            if task != .invalid { UIApplication.shared.endBackgroundTask(task) }
            #endif
        }
    }

    #if os(iOS) || os(macOS)
    /// The monitor and, from iOS 18, the service session that keeps Always
    /// in force while the app runs in the background. Mutations run serially on
    /// `@MainActor` against the live `RegionCarrier.regions` map so out-of-order
    /// `add`/`remove` calls always converge to the latest store state.
    enum Monitor {
        struct Driver {
            var open: @MainActor () async -> Void
            var identifiers: @MainActor () async -> Set<String>
            var add: @MainActor (String, Region) async -> Void
            var remove: @MainActor (String) async -> Void
        }

        nonisolated(unsafe) static var monitor: CLMonitor?
        nonisolated(unsafe) static var session: AnyObject?
        nonisolated(unsafe) static var driver: Driver = liveDriver
        nonisolated(unsafe) private static var opened = false
        nonisolated(unsafe) private static var dirty: Set<String> = []
        nonisolated(unsafe) private static var replaced: Set<String> = []
        nonisolated(unsafe) private static var reconcileID: UInt64 = 0
        nonisolated(unsafe) static var reconciling: Task<Void, Never>?

        private static var name: String { (Bundle.main.bundleIdentifier ?? "app") + ".regions" }

        static let liveDriver = Driver(
            open: {
                #if os(iOS)
                if #available(iOS 18, *) { session = CLServiceSession(authorization: .always) }
                #endif
                let m = await CLMonitor(name)
                monitor = m
                Task { @MainActor in
                    do {
                        for try await event in await m.events {
                            let at = event.date.timeIntervalSince1970 * 1000
                            switch event.state {
                            case .satisfied: RegionCarrier.crossed(event.identifier, inside: true, at: at)
                            case .unsatisfied: RegionCarrier.crossed(event.identifier, inside: false, at: at)
                            default: break
                            }
                        }
                    } catch {
                        WakeStore.log("regions: the monitor's events ended: \(error)")
                    }
                }
            },
            identifiers: {
                guard let m = monitor else { return [] }
                return Set(await m.identifiers)
            },
            add: { identifier, r in
                guard let m = monitor else { return }
                await m.add(condition(r), identifier: identifier)
            },
            remove: { identifier in
                guard let m = monitor else { return }
                await m.remove(identifier)
            }
        )

        /// Made once; its events iterated for the life of the process. The
        /// store's regions the monitor lacks are added, and only those, since
        /// adding a condition again starts its state from `unknown`.
        static func start() {
            schedule()
        }

        static func condition(_ r: Region) -> CLMonitor.CircularGeographicCondition {
            CLMonitor.CircularGeographicCondition(center: CLLocationCoordinate2D(latitude: r.latitude, longitude: r.longitude), radius: r.radius)
        }

        static func add(_ key: String) {
            dirty.insert(key)
            replaced.insert(key)
            schedule()
        }

        static func remove(_ key: String) {
            guard opened || reconciling != nil || !dirty.isEmpty else { return }
            dirty.insert(key)
            replaced.remove(key)
            schedule()
        }

        static func resetForTests() {
            reconcileID &+= 1
            reconciling?.cancel()
            reconciling = nil
            opened = false
            dirty.removeAll()
            replaced.removeAll()
        }

        private static func schedule() {
            guard reconciling == nil else { return }
            reconcileID &+= 1
            let id = reconcileID
            reconciling = Task { @MainActor in
                defer { if reconcileID == id { reconciling = nil } }
                guard !Task.isCancelled else { return }
                await drain()
            }
        }

        @MainActor
        private static func drain() async {
            if !opened {
                guard !Task.isCancelled else { return }
                opened = true
                await driver.open()
                guard !Task.isCancelled else { return }
                let have = await driver.identifiers()
                guard !Task.isCancelled else { return }
                // Re-read live `regions` after initialization; remove stale or legacy conditions
                // and add only missing ones so existing conditions keep their monitored state.
                for id in have where RegionCarrier.regions[id] == nil { dirty.insert(id) }
                for id in RegionCarrier.regions.keys where !have.contains(id) { dirty.insert(id) }
                WakeStore.log("regions: monitoring \(RegionCarrier.regions.count)")
            }
            while !Task.isCancelled, let key = dirty.sorted().first {
                dirty.remove(key)
                let forceReplace = replaced.remove(key) != nil
                if let r = RegionCarrier.regions[key] {
                    if forceReplace { await driver.remove(key) }
                    if Task.isCancelled || dirty.contains(key) || RegionCarrier.regions[key] == nil { continue }
                    await driver.add(key, r)
                } else {
                    await driver.remove(key)
                }
            }
        }
    }
    #endif
}
