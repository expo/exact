import XCTest
import ExactCore
@testable import ExactWidgetPublication

/// The record of a kind's placed widgets, from the extension's write to the app's answer (LLP 1126.005 D4).
final class WidgetPlacementsTests: XCTestCase {
    private var root: URL!
    private var clock = Date()
    private var scheduled: [(delay: TimeInterval, work: () -> Void)] = []
    private var reloads: [String] = []
    private let rows = ["shelf", "book"]
    private let piranesi = ["shelf": "reading", "book": "b1"], finished = ["shelf": "done"]

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("placements-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        WidgetPlacements.parameters = ["progress": rows, "cover": rows]
        WidgetPlacements.now = { [unowned self] in clock }
        WidgetPlacements.schedule = { [unowned self] delay, work in scheduled.append((delay, work)) }
        WidgetPlacements.reloaded = [:]
        WidgetPlacements.unsettled = [:]
        WidgetPlacements.activated = .distantPast
    }
    override func tearDown() {
        try? FileManager.default.removeItem(at: root)
        WidgetPlacements.parameters = [:]
        WidgetPlacements.now = Date.init
        WidgetPlacements.schedule = { delay, work in DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work) }
    }

    private func extensionRecords(_ listed: [[String: String]?]?, asked: [String: String]?, kind: String = "progress") {
        WidgetPlacementRecords.record(kind: kind, parameters: rows, listed: listed, asked: asked, root: root, appID: nil, now: clock)
    }
    private func ask(_ counts: [String: Int]?, kind: String = "progress") -> (ready: Bool, books: [String?]) {
        var answer: [String: Any] = [:]
        WidgetPlacements.answer(kind: kind, root: root, counts: { $0(counts) }, reload: { [unowned self] in reloads.append($0) }) { answer = $0 }
        let placements = answer["placements"] as? [[String: Any]] ?? []
        return (answer["ready"] as? Bool ?? false, placements.map { $0["book"] as? String })
    }

    func testTheAnswerIsThisBuildsRecordOfAsManyPlacementsAsWidgetKitCounts() {
        extensionRecords([piranesi, finished], asked: piranesi)
        let matched = ask(["progress": 2, "library": 1])
        XCTAssertTrue(matched.ready)
        XCTAssertEqual(matched.books, [nil, "b1"], "an unchosen row is null")
        XCTAssertEqual(reloads, [])

        WidgetPlacements.parameters["progress"] = ["shelf", "title"]
        XCTAssertFalse(ask(["progress": 2]).ready, "a record written for other parameters is another build's")
        XCTAssertEqual(reloads, ["progress"], "the extension is asked to record it again")
        WidgetPlacements.parameters["progress"] = rows
        XCTAssertTrue(ask(["progress": 2]).ready)

        XCTAssertFalse(ask(["progress": 1]).ready, "a removal runs no timeline: the record is stale")
        XCTAssertEqual(reloads, ["progress", "progress"])
        _ = ask(["progress": 1])
        XCTAssertEqual(reloads.count, 2, "one reload for one mismatch")
        _ = ask(["progress": 3])
        XCTAssertEqual(reloads.count, 3, "another count is another mismatch")
        clock += WidgetPlacements.interval
        _ = ask(["progress": 3])
        XCTAssertEqual(reloads.count, 4, "a mismatch that stays is asked for again at the next check")

        let none = ask(["library": 1])
        XCTAssertTrue(none.ready)
        XCTAssertEqual(none.books, [], "no placement of the kind needs no record")
        XCTAssertFalse(ask(["cover": 1], kind: "cover").ready, "placed but never recorded")

        extensionRecords([["shelf": "reading"], ["shelf": "reading"]], asked: ["shelf": "reading"])
        let equal = ask(["progress": 2])
        XCTAssertTrue(equal.ready, "two placements whose values resolve to the same record still count as two")
        XCTAssertEqual(equal.books, [nil], "one record for each different configuration")

        extensionRecords([piranesi, nil], asked: piranesi)
        XCTAssertFalse(ask(["progress": 2]).ready, "a placement the extension could not read is not left out")
        extensionRecords(nil, asked: piranesi)
        XCTAssertFalse(ask(["progress": 2]).ready, "WidgetKit did not answer the extension")
    }

    func testAnUnknownAnswerIsCheckedAgainUntilItResolves() {
        extensionRecords([piranesi, finished], asked: piranesi)
        XCTAssertFalse(ask(["progress": 1]).ready)
        XCTAssertEqual(reloads, ["progress"])
        XCTAssertEqual(scheduled.map(\.delay), [WidgetPlacements.interval], "a mismatch keeps a check pending")

        let asksAgain = expectation(forNotification: Notification.Name("exact.widgets.placementsChanged"), object: nil)
        scheduled.removeFirst().work()
        wait(for: [asksAgain], timeout: 5)
        clock += WidgetPlacements.interval
        let gone = ask(["progress": 0])
        XCTAssertTrue(gone.ready, "the last widget went: no timeline, activation or ping is needed")
        XCTAssertEqual(gone.books, [])

        scheduled = []
        XCTAssertFalse(ask(nil).ready, "unknown while WidgetKit does not answer the app")
        XCTAssertEqual(scheduled.count, 1, "and asked again")
        XCTAssertEqual(reloads, ["progress"], "the record is not at fault")

        scheduled = []
        clock += WidgetPlacements.settling + 1
        XCTAssertFalse(ask(nil).ready)
        XCTAssertEqual(scheduled.count, 0, "the checks end two minutes after the answer became unknown")
    }

    func testAnEditTheExtensionReadFromWidgetKitsOldListIsNotAnswered() {
        let dune = ["shelf": "reading", "book": "b2"]
        extensionRecords([piranesi], asked: piranesi)
        XCTAssertEqual(ask(["progress": 1]).books, ["b1"])

        extensionRecords([piranesi], asked: dune)
        let stale = ask(["progress": 1])
        XCTAssertFalse(stale.ready, "the timeline was for a configuration WidgetKit did not list yet")
        XCTAssertEqual(reloads, ["progress"], "so the extension is asked to read again")
        XCTAssertFalse(ask(["progress": 0]).ready, "no count answers while a placement waits to be listed")

        extensionRecords([piranesi, finished], asked: finished)
        XCTAssertFalse(ask(["progress": 2]).ready, "another widget's timeline does not settle it")

        extensionRecords([dune], asked: dune)
        let settled = ask(["progress": 1])
        XCTAssertTrue(settled.ready)
        XCTAssertEqual(settled.books, ["b2"])

        extensionRecords([dune], asked: piranesi)
        XCTAssertFalse(ask(["progress": 1]).ready)
        clock += WidgetPlacementRecords.lag
        XCTAssertEqual(ask(["progress": 1]).books, ["b2"], "a configuration WidgetKit never lists stops holding the answer")
    }

    func testActivationAsksAgainOnceHoweverOftenInstalled() {
        WidgetPlacements.install(root: root, appID: nil)
        WidgetPlacements.install(root: root, appID: nil)
        extensionRecords([piranesi, finished], asked: piranesi)
        _ = ask(["progress": 1])
        _ = ask(["progress": 1])
        XCTAssertEqual(reloads, ["progress"])
        var changes = 0
        let observer = NotificationCenter.default.addObserver(forName: Notification.Name("exact.widgets.placementsChanged"), object: nil, queue: nil) { _ in changes += 1 }
        defer { NotificationCenter.default.removeObserver(observer) }
        #if os(macOS)
        NotificationCenter.default.post(name: Notification.Name("NSApplicationDidBecomeActiveNotification"), object: nil)
        #else
        NotificationCenter.default.post(name: Notification.Name("UIApplicationDidBecomeActiveNotification"), object: nil)
        #endif
        XCTAssertEqual(changes, 1, "one activation asks the readers once")
        _ = ask(["progress": 1])
        XCTAssertEqual(reloads, ["progress", "progress"], "a mismatch still there after activation asks WidgetKit again")
        XCTAssertTrue(ask(["progress": 2]).ready)
    }
}
