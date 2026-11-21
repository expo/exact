#if canImport(WidgetKit)
import XCTest
import WidgetKit
import ExactCore
import ExactSnapshot
import ExactWidgetTimeline

final class WidgetTimelineWaitersTests: XCTestCase {
    struct Entry: TimelineEntry {
        let date: Date
        var size: CGSize
    }
    func testInlineAndRectangularCoalesceWithoutSharingTheirSize() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let cache = try WidgetTimelineCache(root: root, kind: "trains", generation: "one")
        let waiters = WidgetTimelineWaiters<Entry> { entry, size in
            var entry = entry; entry.size = size; return entry
        }
        let inline = CGSize(width: 13.5, height: 13.5), rectangular = CGSize(width: 184, height: 69.5)
        for sizes in [[inline, rectangular], [rectangular, inline]] {
            var answers: [CGSize] = []
            for (index, size) in sizes.enumerated() {
                XCTAssertEqual(waiters.begin(cache, size: size) { answers.append($0.entries[0].size) }, index == 0)
            }
            let answer = Timeline(entries: [Entry(date: .now, size: sizes[0])], policy: .never)
            waiters.finish(cache, answer, revision: cache, fallback: answer)
            XCTAssertEqual(answers, sizes)
        }
    }
    func testCompletionsBegunBeforeAStoreWriteTakeTheAnswerOnlyWhileItsRevisionIsCurrent() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        var counter = 0
        func write() throws {
            counter += 1
            try Data("\(counter)".utf8).write(to: root.appendingPathComponent("widget-store-generation"), options: .atomic)
        }
        func cache() throws -> WidgetTimelineCache { try WidgetTimelineCache(root: root, kind: "trains", generation: "one") }
        let waiters = WidgetTimelineWaiters<Entry> { entry, _ in entry }
        let size = CGSize(width: 155, height: 155), answered = Date(timeIntervalSince1970: 1), fellBack = Date(timeIntervalSince1970: 2)
        let timeline = Timeline(entries: [Entry(date: answered, size: size)], policy: .never)
        let fallback = Timeline(entries: [Entry(date: fellBack, size: size)], policy: .never)
        for anotherWrite in [false, true] {
            let begun = try cache()
            var answers: [Date] = []
            XCTAssertTrue(waiters.begin(begun, size: size) { answers.append($0.entries[0].date) })
            try write()
            let revision = try cache()
            if anotherWrite { try write() }
            waiters.finish(begun, timeline, revision: revision, fallback: fallback)
            XCTAssertEqual(answers, [anotherWrite ? fellBack : answered], "the revision the answer was computed under decides")
        }
    }
}
#endif
