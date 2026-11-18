import Foundation
import XCTest
@testable import ExactKit
@testable import ExactCollections
#if canImport(AppKit)
import AppKit
#endif

final class CollectionTests: XCTestCase {
    private func decodeSnapshot(_ value: [String: Any]) -> CollectionSnapshot? {
        wireBatch([["op": "collections", "items": [value]]]).ops.first?.collections?.first
    }

    func testCollectionSnapshotsAreTypedInEitherOperationFieldOrder() throws {
        let items = #"[{"view":2,"axis":"x","parent":1,"revision":"18446744073709551614","scrollSequence":3,"totalExtent":4000,"count":100,"pending":true,"seeking":true,"waiting":false,"request":"9","lastRequest":"10","rows":[{"view":4,"root":5,"epoch":"18446744073709551613","index":7,"size":40,"measured":true}],"correction":{"scrollSequence":3,"offset":90,"from":-20,"smooth":true}}]"#
        for op in [#"{"op":"collections","items":\#(items)}"#, #"{"items":\#(items),"op":"collections"}"#] {
            let batch = Batch.decode(Data(#"{"ops":[\#(op)]}"#.utf8))
            XCTAssertNil(batch.error)
            let decoded = try XCTUnwrap(batch.ops.first)
            XCTAssertTrue(decoded.payload.isEmpty, "no snapshot dictionaries cross to presentation")
            let snapshot = try XCTUnwrap(decoded.collections?.first)
            XCTAssertEqual(snapshot.view, 2)
            XCTAssertTrue(snapshot.horizontal)
            XCTAssertEqual(snapshot.parent, 1)
            XCTAssertEqual(snapshot.revision, UInt64.max - 1)
            XCTAssertEqual(snapshot.rows.first?.epoch, UInt64.max - 2)
            XCTAssertEqual(snapshot.rows.first?.index, 7)
            XCTAssertEqual(snapshot.rows.first?.size, 40)
            XCTAssertEqual(snapshot.rows.first?.measured, true)
            XCTAssertTrue(snapshot.pending && snapshot.seeking && !snapshot.waiting)
            XCTAssertEqual(snapshot.request, 9)
            XCTAssertEqual(snapshot.lastRequest, 10)
            XCTAssertEqual(snapshot.correction?.from, -20)
            XCTAssertEqual(snapshot.correction?.smooth, true)
        }
    }

    func testTypedSnapshotsPreserveFixedContentAndRefuseMalformedFixedSlots() throws {
        let fixed: [[String: Any]] = [
            ["view": 5, "roots": [6, 7], "epoch": 1, "size": 20],
            ["view": 8, "roots": [], "epoch": 1, "size": 0],
        ]
        var value: [String: Any] = ["view": 1, "revision": 1, "scrollSequence": 0, "totalExtent": 200,
                                    "rows": [["view": 2, "root": 3, "epoch": 1]], "fixed": fixed]
        let snapshot = try XCTUnwrap(decodeSnapshot(value))
        XCTAssertEqual(snapshot.fixed.map(\.view), [5, 8])
        XCTAssertEqual(snapshot.fixed.map(\.roots), [[6, 7], []])
        XCTAssertEqual(snapshot.fixed.map(\.size), [20, 0])
        for bad in [Array(fixed.prefix(1)), [fixed[0], fixed[0]],
                    [["view": 2, "roots": [], "epoch": 1, "size": 0], fixed[1]],
                    [["view": 5, "roots": [0], "epoch": 1, "size": 20], fixed[1]],
                    [["view": 5, "roots": [], "epoch": 2, "size": 20], fixed[1]]] {
            value["fixed"] = bad
            value["correction"] = NSNull()
            XCTAssertNil(decodeSnapshot(value))
        }
        value["fixed"] = fixed
        value["axis"] = "x"
        XCTAssertNil(decodeSnapshot(value), "fixed slots are vertical")
    }

    func testSnapQueryReadsUnmountedRowsWithoutChangingTheWindow() throws {
        #if canImport(AppKit)
        _ = NSApplication.shared
        #endif
        let session = ExactApp.shared.makeSession(label: "snap-query")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("SCROLL_SNAP"), size: CGSize(width: 300, height: 100)).error)
        let p = session.presenter
        let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "strip" })
        let before = try XCTUnwrap(p.listHost.entries[node.id]?.snapshot)
        XCTAssertEqual(session.runtime.collectionSnap(node.id, proposed: 30020, port: 300, maximum: 149700), 30000)
        XCTAssertNil(session.runtime.collectionSnap(node.id, proposed: 30020, port: 300, maximum: 149700, distance: 19))
        XCTAssertEqual(session.runtime.collectionSnap(node.id, proposed: 30020, port: 300, maximum: 149700, distance: 20), 30000)
        XCTAssertNil(session.runtime.collectionSnap(node.id, proposed: 30000, port: 300, maximum: 149700, distance: 0))
        XCTAssertNil(session.runtime.collectionSnap(UInt32.max, proposed: 30020, port: 300, maximum: 149700))
        XCTAssertEqual(p.listHost.entries[node.id]?.snapshot.rows.map(\.view), before.rows.map(\.view))
        XCTAssertEqual(p.listHost.entries[node.id]?.snapshot.revision, before.revision)
    }

    /// An unmounted `always` row between a release and its rest holds a snap
    /// (CSS Scroll Snap 1 §6.2): the runner names the first one either way.
    func testSnapStopQueryNamesTheFirstUnmountedAlwaysRowOnTheWay() throws {
        #if canImport(AppKit)
        _ = NSApplication.shared
        #endif
        let session = ExactApp.shared.makeSession(label: "snap-stop-query")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("SCROLL_SNAP"), size: CGSize(width: 300, height: 100)).error)
        let node = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "stops" })
        let stop = { (release: Double, rest: Double) in
            session.runtime.collectionSnapStop(node.id, release: release, rest: rest, port: 300, minimum: 0, maximum: 149700)
        }
        XCTAssertEqual(stop(14000, 30000), 15000)
        XCTAssertEqual(stop(16000, 3000), 15000, "going back too")
        XCTAssertNil(stop(15500, 30000), "none past the release")
        XCTAssertNil(stop(14000, 14200), "none short of it")
        #if os(macOS)
        // On AppKit the rule takes it: a lift released at 14,000 projected to 30,000 rests on the `always` row.
        let rest = node.snappedOrigin(CGPoint(x: 30000, y: 0), release: CGPoint(x: 14000, y: 0))
        XCTAssertEqual(rest?.x ?? 0, 15000, accuracy: 0.01)
        #endif
    }

    func testWireIsVersionFourLittleEndianAndPreservesLargeEpochs() {
        let facts = CollectionFacts(offset: 12, portMain: 480, portCross: 320, cross: 300,
            measurements: [.init(view: 23, epoch: 0xfedcba9876543210, size: 72)], focus: 31, interaction: nil,
            paddingStart: 92, paddingEnd: 83)
        let bytes = [UInt8](facts.encode(view: 17, revision: 9, sequence: 11))
        let double = { (n: Double) in [UInt8](withUnsafeBytes(of: n.bitPattern.littleEndian, Array.init)) }
        XCTAssertEqual(bytes.count, 120)
        XCTAssertEqual(Array(bytes.prefix(8)), [4, 0, 0, 0, 17, 0, 0, 0])
        XCTAssertEqual(Array(bytes[32..<40]), double(480), "the main axis first")
        XCTAssertEqual(Array(bytes[8..<16]), [9, 0, 0, 0, 0, 0, 0, 0])
        XCTAssertEqual(Array(bytes[56..<72]), double(92) + double(83), "the padding laid out (LLP 1010 §6.9)")
        XCTAssertEqual(Array(bytes[72..<80]), [31, 0, 0, 0, 0, 0, 0, 0])
        XCTAssertEqual(Array(bytes[88..<100]), [255, 255, 255, 255, 0, 0, 0, 0, 1, 0, 0, 0], "no limit, no moving ancestor, one row")
        XCTAssertEqual(Array(bytes[104..<112]), [0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe])
        let filled = [UInt8](facts.encode(view: 17, revision: 9, sequence: 11, velocity: -2, limit: 3, ancestorMoving: true))
        XCTAssertEqual(Array(filled[80..<96]), [0, 0, 0, 0, 0, 0, 0, 0xc0, 3, 0, 0, 0, 1, 0, 0, 0])
    }

    func testCorrectionIsOncePerRevisionAndCannotReplaceNewUserIntent() {
        var cursor = CollectionCursor()
        XCTAssertTrue(cursor.takeCorrection(revision: 2, sequence: 0))
        XCTAssertFalse(cursor.takeCorrection(revision: 2, sequence: 0))
        cursor.advance()
        XCTAssertFalse(cursor.takeCorrection(revision: 3, sequence: 0))
        XCTAssertTrue(cursor.takeCorrection(revision: 3, sequence: 1))
        XCTAssertFalse(cursor.takeCorrection(revision: 2, sequence: 1))
    }

    func testFeedbackDedupIncludesEpochGeometryAndBothIndependentPins() {
        let a = CollectionFacts(offset: 0, portMain: 200, portCross: 100, cross: 80,
            measurements: [.init(view: 4, epoch: 7, size: 30)], focus: 8, interaction: 9)
        var b = a
        XCTAssertEqual(a, b)
        b.measurements[0].epoch = 8
        XCTAssertNotEqual(a, b)
        b = a; b.cross = 79
        XCTAssertNotEqual(a, b)
        b = a; b.focus = nil
        XCTAssertNotEqual(a, b)
        b = a; b.interaction = nil
        XCTAssertNotEqual(a, b)
        // A padding change alone, which keeps a border-box list's size, is
        // reported too (LLP 1010 §6.9)
        b = a; b.paddingEnd = 109
        XCTAssertNotEqual(a, b)
    }

    func testMalformedSnapshotsDoNotBecomeGeometry() {
        let row: [String: Any] = ["view": 3, "root": 4, "index": 0, "start": 0, "size": 40, "epoch": "18446744073709551614", "measured": false]
        let value: [String: Any] = ["view": 2, "revision": 5, "scrollSequence": 0, "count": 100,
            "totalExtent": 4000, "rows": [row], "correction": NSNull()]
        XCTAssertEqual(decodeSnapshot(value)?.rows.first?.epoch, UInt64.max - 1)
        var broken = value; broken["totalExtent"] = -1
        XCTAssertNil(decodeSnapshot(broken))
        broken = value; broken["rows"] = [row, row]
        XCTAssertNil(decodeSnapshot(broken))
        // A `scrollIntoView` into the padding before the first row is a
        // negative correction (LLP 1010 §6.9); an unrepresentable one is not.
        var padded = value; padded["correction"] = ["scrollSequence": 1, "offset": -92]
        XCTAssertEqual(decodeSnapshot(padded)?.correction?.offset, -92)
        // An anchor's correction taken from a port in that padding keeps its
        // `from`, so it stays relative (Grok's scroll-padding review r2).
        padded["correction"] = ["scrollSequence": 1, "offset": 500, "from": -92]
        XCTAssertEqual(decodeSnapshot(padded)?.correction?.from, -92)
        padded["correction"] = ["scrollSequence": 1, "offset": -Double.greatestFiniteMagnitude]
        XCTAssertNil(decodeSnapshot(padded))
    }


    func testDecimalJSONCountersRetainAllSixtyFourBits() throws {
        let bytes = Data(#"{"view":2,"revision":"18446744073709551614","scrollSequence":"18446744073709551613","totalExtent":4000,"rows":[],"correction":{"scrollSequence":"18446744073709551613","offset":90}}"#.utf8)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: bytes) as? [String: Any])
        let snapshot = try XCTUnwrap(decodeSnapshot(object))
        XCTAssertEqual(snapshot.revision, UInt64.max - 1)
        XCTAssertEqual(snapshot.sequence, UInt64.max - 2)
        XCTAssertEqual(snapshot.correction?.sequence, UInt64.max - 2)
    }

    func testTurnBudgetRejectsRecursionAndYieldsAfterTwoPasses() {
        var budget = CollectionTurnBudget()
        XCTAssertTrue(budget.begin())
        XCTAssertFalse(budget.begin())
        budget.end()
        XCTAssertTrue(budget.begin())
        budget.end()
        XCTAssertFalse(budget.begin())
        budget.nextTurn()
        XCTAssertTrue(budget.begin())
    }

    func testEachWakeRenewsPinReportsBeforeContinuationsRun() throws {
        #if canImport(AppKit)
        _ = NSApplication.shared
        #endif
        let p = Presenter()
        defer { p.reset() }
        p.viewport.frame = CGRect(x: 0, y: 0, width: 300, height: 200)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0,
                "totalExtent": 1000, "rows": [["view": 2, "root": 2, "epoch": 1]]]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 200],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 300, "h": 100],
            ["op": "content", "id": 1, "w": 300, "h": 1000],
        ]))
        var reports: [Data] = []
        p.listHost.onFeedback = { reports.append($0) }
        p.listHost.settle()
        #if os(iOS) || os(tvOS)
        p.scrollPump.reset()
        #endif
        var wakes = 0, armed = false
        let done = expectation(description: "pin reports on two consecutive wakes")
        let activities = CFRunLoopActivity.beforeWaiting.rawValue | CFRunLoopActivity.afterWaiting.rawValue
        let observer = try XCTUnwrap(CFRunLoopObserverCreateWithHandler(nil, activities, true, 1) { _, activity in
            guard wakes < 2 else { return }
            if activity == .beforeWaiting {
                guard !armed else { return }
                // Spend both passes after this turn's queued continuations.
                p.listHost.pointer(nil)
                p.listHost.fillSlice(1, limit: 0)
                let before = reports.count
                p.listHost.pointer(2)
                p.listHost.pointer(nil)
                XCTAssertEqual(reports.count - before, 2)
                armed = true
            } else if armed {
                // Order 1 follows the budget observer, before native input
                // and the main queue can run on this wake.
                let before = reports.count
                let refused = p.listHost.budget.refusedSpent
                p.listHost.pointer(2)
                XCTAssertEqual(reports.count - before, 1, "the input reports its pin synchronously")
                XCTAssertEqual(p.listHost.budget.refusedSpent, refused)
                XCTAssertEqual(p.listHost.entries[1]?.lastFacts?.interaction, 2)
                armed = false
                wakes += 1
                if wakes == 2 { done.fulfill() }
            }
        })
        CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
        defer { CFRunLoopRemoveObserver(CFRunLoopGetMain(), observer, .commonModes) }
        wait(for: [done], timeout: 5)
        XCTAssertEqual(wakes, 2)
        XCTAssertTrue(p.listHost.observingTurns)
        p.listHost.reset()
        XCTAssertTrue(p.listHost.observingTurns)
    }

    /// A rescue is never refused for the turn's passes, only while a pass
    /// runs (a report's batch reentering); each refusal is counted by why.
    func testARescueIsNotRefusedForTheTurnsPasses() {
        var budget = CollectionTurnBudget()
        XCTAssertTrue(budget.begin()); budget.end()
        XCTAssertTrue(budget.begin()); budget.end()
        XCTAssertFalse(budget.begin())
        XCTAssertEqual(budget.refusedSpent, 1)
        XCTAssertTrue(budget.begin(rescue: true))
        XCTAssertFalse(budget.begin(rescue: true), "not inside a running pass")
        XCTAssertEqual(budget.refusedBusy, 1)
        budget.end()
        XCTAssertEqual(budget.refusedSpent, 1)
    }
}
