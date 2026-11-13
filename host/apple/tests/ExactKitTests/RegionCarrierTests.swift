import XCTest
@testable import ExactKit

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
        RegionCarrier.regions[name] = .init(package: package, name: name, latitude: 0, longitude: 0, radius: 100, inside: false)
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
        RegionCarrier.crossed(name, inside: true, at: 1)
        XCTAssertEqual(offered.map { $0["event"] as? String }, ["enter"])
        RegionCarrier.crossed(name, inside: false, at: 2)
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
}
