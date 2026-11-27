import XCTest
@testable import ExactWidgetPublication

final class WidgetPlacementsTests: XCTestCase {
    private var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("placements-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let records: [String: Any] = ["progress": [["shelf": "reading", "book": "b1"], ["shelf": "done", "book": NSNull()]]]
        try JSONSerialization.data(withJSONObject: records).write(to: root.appendingPathComponent("widget-placements.json"))
    }
    override func tearDown() { try? FileManager.default.removeItem(at: root) }

    private func ask(_ counts: [String: Int]?, kind: String = "progress", reloads: inout [String]) -> [String: Any] {
        var answer: [String: Any] = [:]
        var reloaded: [String] = []
        WidgetPlacements.answer(kind: kind, root: root, counts: { $0(counts) }, reload: { reloaded.append($0) }) { answer = $0 }
        reloads += reloaded
        return answer
    }

    func testTheRecordAnswersOnlyWhenWidgetKitCountsAsManyPlacements() {
        var reloads: [String] = []
        let matched = ask(["progress": 2, "library": 1], reloads: &reloads)
        XCTAssertEqual(matched["ready"] as? Bool, true)
        XCTAssertEqual((matched["placements"] as? [[String: Any]])?.compactMap { $0["shelf"] as? String }, ["reading", "done"])
        XCTAssertTrue((matched["placements"] as? [[String: Any]])?[1]["book"] is NSNull, "an unchosen row is null")
        XCTAssertEqual(reloads, [])

        let removed = ask(["progress": 1], reloads: &reloads)
        XCTAssertEqual(removed["ready"] as? Bool, false, "a removal runs no timeline: the record is stale")
        XCTAssertEqual(reloads, ["progress"], "the kind's timelines are asked for again, so the extension records it")
        _ = ask(["progress": 1], reloads: &reloads)
        XCTAssertEqual(reloads, ["progress"], "one reload for one mismatch")
        _ = ask(["progress": 3], reloads: &reloads)
        XCTAssertEqual(reloads, ["progress", "progress"], "another count is another mismatch")

        XCTAssertEqual(ask(["progress": 2], reloads: &reloads)["ready"] as? Bool, true)
        let none = ask(["library": 1], reloads: &reloads)
        XCTAssertEqual(none["ready"] as? Bool, true)
        XCTAssertEqual((none["placements"] as? [Any])?.count, 0, "no placement of the kind needs no record")
        XCTAssertEqual(ask(nil, reloads: &reloads)["ready"] as? Bool, false, "unknown while WidgetKit does not answer")
        XCTAssertEqual(ask(["cover": 1], kind: "cover", reloads: &reloads)["ready"] as? Bool, false, "placed but never recorded")
    }

    func testActivationAllowsAnotherReloadForTheSameMismatch() {
        WidgetPlacements.install(root: root, appID: nil)
        var reloads: [String] = []
        _ = ask(["progress": 1], reloads: &reloads)
        _ = ask(["progress": 1], reloads: &reloads)
        XCTAssertEqual(reloads, ["progress"])
        #if os(macOS)
        NotificationCenter.default.post(name: Notification.Name("NSApplicationDidBecomeActiveNotification"), object: nil)
        #else
        NotificationCenter.default.post(name: Notification.Name("UIApplicationDidBecomeActiveNotification"), object: nil)
        #endif
        _ = ask(["progress": 1], reloads: &reloads)
        XCTAssertEqual(reloads, ["progress", "progress"], "a mismatch still there after activation asks WidgetKit again")
        XCTAssertEqual(ask(["progress": 2], reloads: &reloads)["ready"] as? Bool, true)
    }
}
