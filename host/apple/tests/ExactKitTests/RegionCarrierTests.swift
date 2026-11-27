import XCTest
@testable import ExactKit
@testable import ExactWakeCarriers

/// A region's crossings are each heard (LLP 1133.021 D14): one written while
/// the region's last is in a wake survives that one's delivery and is
/// offered when the wake ends.
final class RegionCarrierTests: XCTestCase {
    private var observer: NSObjectProtocol?

    override func setUp() { super.setUp(); WakeStore.install() }
    override func tearDown() { observer.map(NotificationCenter.default.removeObserver); observer = nil; super.tearDown() }

    /// The carrier's answer to a package's request, as `context.regions` asks it.
    private func ask(_ body: [String: Any]) -> Any? {
        let box = NSMutableDictionary(dictionary: body)
        NotificationCenter.default.post(name: Notification.Name("exact.regions"), object: box)
        return box["reply"]
    }

    func testACrossingDuringAWakeIsOfferedWhenTheWakeEnds() {
        let package = "region-carrier-\(UUID().uuidString.prefix(6))", name = "\(package)-home"
        let key = RegionCarrier.key(package: package, name: name)
        RegionCarrier.regions[key] = .init(package: package, name: name, latitude: 0, longitude: 0, radius: 100, inside: false)
        defer { _ = ask(["op": "remove", "package": package, "name": name]) }
        let pending = { self.ask(["op": "pending", "package": package, "name": name]) as? [String: Any] }
        // Each wake the package takes: the transition it read, and its done.
        var offered: [[String: Any]] = [], held: [() -> Void] = []
        observer = NotificationCenter.default.addObserver(forName: Notification.Name("exact.wakes.due"), object: nil, queue: nil) { note in
            guard let body = note.object as? NSMutableDictionary, body["package"] as? String == package,
                  let token = body["token"] as? String else { return }
            body["taken"] = true
            offered.append(pending() ?? [:])
            held.append {
                let done: [String: Any] = ["token": token, "package": package, "success": true]
                NotificationCenter.default.post(name: Notification.Name("exact.wakes.done"), object: done as NSDictionary)
            }
        }
        RegionCarrier.crossed(key, inside: true, at: 1)
        XCTAssertEqual(offered.map { $0["event"] as? String }, ["enter"])
        RegionCarrier.crossed(key, inside: false, at: 2)
        XCTAssertEqual(offered.count, 1, "a region in a wake is not offered again until it ends")

        _ = ask(["op": "delivered", "package": package, "name": name, "event": "enter", "timestamp": 1.0])
        XCTAssertEqual(pending()?["event"] as? String, "leave", "the enter's delivery keeps the leave written after it")
        held.removeFirst()()
        XCTAssertEqual(offered.map { $0["event"] as? String }, ["enter", "leave"], "the leave is offered when the enter's wake ends")
        XCTAssertEqual(offered.last?["timestamp"] as? Double, 2)

        _ = ask(["op": "delivered", "package": package, "name": name, "event": "leave", "timestamp": 2.0])
        XCTAssertNil(pending())
        held.forEach { $0() }
        XCTAssertEqual(offered.count, 2)
    }

    #if os(iOS) || os(macOS)
    func testPackagesDoNotOverwriteOrRemoveEachOthersRegions() {
        let savedDriver = RegionCarrier.Monitor.driver
        var monitored: [String: RegionCarrier.Region] = [:]
        RegionCarrier.Monitor.resetForTests()
        RegionCarrier.Monitor.driver = .init(
            open: {},
            identifiers: { Set(monitored.keys) },
            add: { id, r in monitored[id] = r },
            remove: { id in monitored.removeValue(forKey: id) }
        )
        let pkgA = "pkg-a-\(UUID().uuidString.prefix(6))", pkgB = "pkg-b-\(UUID().uuidString.prefix(6))"
        let name = "shared-home"
        defer {
            _ = ask(["op": "remove", "package": pkgA, "name": name])
            _ = ask(["op": "remove", "package": pkgB, "name": name])
            waitFor("cleanup settles") { RegionCarrier.Monitor.reconciling == nil }
            RegionCarrier.Monitor.resetForTests()
            RegionCarrier.Monitor.driver = savedDriver
        }
        _ = ask(["op": "add", "package": pkgA, "name": name, "latitude": 1.0, "longitude": 2.0, "radius": 100.0])
        _ = ask(["op": "add", "package": pkgB, "name": name, "latitude": 3.0, "longitude": 4.0, "radius": 200.0])
        let listA = ask(["op": "list", "package": pkgA]) as? [[String: Any]] ?? []
        let listB = ask(["op": "list", "package": pkgB]) as? [[String: Any]] ?? []
        XCTAssertEqual(listA.first?["latitude"] as? Double, 1.0)
        XCTAssertEqual(listB.first?["latitude"] as? Double, 3.0)
        _ = ask(["op": "remove", "package": pkgA, "name": name])
        XCTAssertTrue((ask(["op": "list", "package": pkgA]) as? [[String: Any]] ?? []).isEmpty)
        let afterB = ask(["op": "list", "package": pkgB]) as? [[String: Any]] ?? []
        XCTAssertEqual(afterB.count, 1)
        XCTAssertEqual(afterB.first?["latitude"] as? Double, 3.0)
        waitFor("monitor settles with only pkgB's region") { RegionCarrier.Monitor.reconciling == nil }
        XCTAssertNil(monitored[RegionCarrier.key(package: pkgA, name: name)])
        XCTAssertEqual(monitored[RegionCarrier.key(package: pkgB, name: name)]?.latitude, 3.0)
    }

    func testRapidAddAndRemoveConvergesWithoutLeakingRemovedGeofence() {
        let savedDriver = RegionCarrier.Monitor.driver
        var monitored: [String: RegionCarrier.Region] = [:]
        var pauseNextRemove = true
        var resumeRemove: CheckedContinuation<Void, Never>?
        RegionCarrier.Monitor.resetForTests()
        RegionCarrier.Monitor.driver = .init(
            open: {},
            identifiers: { Set(monitored.keys) },
            add: { id, r in monitored[id] = r },
            remove: { id in
                monitored.removeValue(forKey: id)
                if pauseNextRemove {
                    pauseNextRemove = false
                    await withCheckedContinuation { resumeRemove = $0 }
                }
            }
        )
        let pkg = "race-\(UUID().uuidString.prefix(6))", name = "home"
        let key = RegionCarrier.key(package: pkg, name: name)
        defer {
            pauseNextRemove = false
            resumeRemove?.resume()
            _ = ask(["op": "remove", "package": pkg, "name": name])
            waitFor("cleanup settles") { RegionCarrier.Monitor.reconciling == nil }
            RegionCarrier.Monitor.resetForTests()
            RegionCarrier.Monitor.driver = savedDriver
        }

        // Add suspends inside remove; remove arrives while suspended.
        _ = ask(["op": "add", "package": pkg, "name": name, "latitude": 10.0, "longitude": 20.0, "radius": 100.0])
        waitFor("add reaches suspended remove") { resumeRemove != nil }
        _ = ask(["op": "remove", "package": pkg, "name": name])
        let cont = resumeRemove; resumeRemove = nil; cont?.resume()
        waitFor("reconciler finishes after remove") { RegionCarrier.Monitor.reconciling == nil }
        XCTAssertNil(monitored[key], "removed geofence must not be re-added by the suspended add")

        // Rapid update replaces coordinates cleanly.
        pauseNextRemove = true
        _ = ask(["op": "add", "package": pkg, "name": name, "latitude": 11.0, "longitude": 21.0, "radius": 100.0])
        waitFor("first add suspends") { resumeRemove != nil }
        _ = ask(["op": "add", "package": pkg, "name": name, "latitude": 33.0, "longitude": 44.0, "radius": 250.0])
        let cont2 = resumeRemove; resumeRemove = nil; cont2?.resume()
        waitFor("reconciler finishes after update") { RegionCarrier.Monitor.reconciling == nil }
        XCTAssertEqual(monitored[key]?.latitude, 33.0)
        XCTAssertEqual(monitored[key]?.radius, 250.0)
    }
    #endif
}
