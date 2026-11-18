#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactCollections

final class CollectionOpeningIOSTests: XCTestCase {
    func testScrollendCanRemoveItsRestingListWithoutRetainingIt() throws {
        let p = Presenter()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 300, height: 200))
        window.addSubview(p.viewport); p.viewport.frame = window.bounds; window.makeKeyAndVisible()
        defer { p.reset(); window.isHidden = true }
        var ended: [UInt32] = []
        p.onScrollend = { id in
            ended.append(id)
            p.apply(wireBatch([
                ["op": "collections", "items": []],
                ["op": "destroy", "id": id],
            ]))
        }
        for id in UInt32(1)...40 {
            p.apply(wireBatch([
                ["op": "create", "id": id, "kind": "list", "style": ["overflow_y": "scroll"]],
                ["op": "collections", "items": [["view": id, "revision": 1, "scrollSequence": 0,
                    "count": 100, "totalExtent": 7200, "pending": false, "rows": []]]],
                ["op": "roots", "ids": [id]],
                ["op": "frame", "id": id, "x": 0, "y": 0, "w": 300, "h": 200],
                ["op": "content", "id": id, "w": 300, "h": 7200],
            ]))
            let node = try XCTUnwrap(p.views[id])
            node.handlers.insert("scrollend")
            node.scrollEnd = ScrollEnd()
            XCTAssertTrue(node.scrollEnd!.moved(left: 0, top: 20, scale: 0))
            p.listHost.entries[id]?.travelled = true
            node.endScroll()
            XCTAssertNil(p.listHost.entries[id])
            XCTAssertTrue(p.listHost.fillPending.isEmpty)
            XCTAssertTrue(p.listHost.resting.isEmpty, "destroyed lists leave no rest work")
        }
        XCTAssertEqual(ended, Array(UInt32(1)...40), "each list still receives its scrollend once")
        print("T removed_resting_lists=40 retained_resting_lists=\(p.listHost.resting.count)")
    }

    func testTheShopWindowFinishesOpeningAndRestingOnDisplayFrames() throws {
        let previous = ExactSession.asyncFills
        defer { ExactSession.asyncFills = previous }
        for asynchronous in [false, true] {
            ExactSession.asyncFills = asynchronous
            try shopWindowOnDisplayFrames(asynchronous: asynchronous)
        }
    }

    private func shopWindowOnDisplayFrames(asynchronous: Bool) throws {
        let session = ExactApp.shared.makeSession(label: "shop-window-frames")
        let p = session.presenter
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 700))
        defer { session.destroy(); window.isHidden = true }
        window.addSubview(p.viewport); p.viewport.frame = window.bounds; window.makeKeyAndVisible()
        XCTAssertNil(session.boot(plan: try fixturePlan("SHOP_SCROLL"), size: window.bounds.size).error)
        let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "product-grid" })
        let scroll = try XCTUnwrap(node.scrollView)
        func waitForWindow() {
            waitFor("the ordinary window, through the run loop alone", facts: {
                "rows=\(p.listHost.entries[node.id]!.snapshot.rows.map(\.index)) pending=\(p.listHost.entries[node.id]!.snapshot.pending)"
            }) {
                p.listHost.entries[node.id]?.snapshot.pending == false
                    && p.listHost.fillPending.isEmpty && !session.ownerInFlight && !session.hasPublished()
            }
        }
        waitForWindow()
        XCTAssertEqual(p.listHost.entries[node.id]?.snapshot.rows.map(\.index), Array(0...3),
            "base opens four two-card rows, including the two below the fold")
        let openingViews = p.views.count
        let flushes = p.listHost.flushes
        var landings = 0
        session.onLandForTests = { kind, _ in if case .fill = kind { landings += 1 } }
        var reports: [Data] = [], retired: [Int] = []
        let feedback = p.listHost.onFeedback, fill = p.listHost.onFill
        p.listHost.onFeedback = { bytes in
            reports.append(bytes)
            let before = Set(p.listHost.entries[node.id]!.snapshot.rows.map(\.view))
            feedback?(bytes)
            let after = Set(p.listHost.entries[node.id]!.snapshot.rows.map(\.view))
            let limit = bytes.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 88, as: UInt32.self) }
            retired.append(before.subtracting(after).count)
            XCTAssertLessThanOrEqual(retired.last!, max(Int(limit) * 2, 4))
        }
        if let fill { p.listHost.onFill = { reports.append($1); fill($0, $1) } }
        var callbackDurations: [Double] = [], restWindows: [[Int]] = []
        for direction in [1.0, -1.0] {
            let driver = NSObject()
            var frame = 0
            node.scrollEnd = ScrollEnd(); node.scrollEnd?.live = true
            FrameClock.shared.want(driver, .offsetDriver, rate: FrameClock.full(on: window.screen)) { _ in
                let step = direction * 3000 * pow(0.998, Double(frame) * 1000 / 60) / 60
                scroll.contentOffset.y = min(scroll.contentSize.height - scroll.bounds.height, max(0, scroll.contentOffset.y + step))
                XCTAssertTrue(p.listHost.covers(node.id), "every fling frame has its visible rows")
                frame += 1
                if frame == 60 { FrameClock.shared.drop(driver) }
            }
            waitFor("sixty display frames of travel") { frame == 60 }
            FrameClock.shared.drop(driver)
            var events: [String] = []
            node.handlers.formUnion(["scroll", "scrollend"])
            p.onScroll = { _, _ in events.append("scroll") }
            p.onScrollend = { _ in events.append("scrollend") }
            node.scrollEnd?.live = false
            session.noticesInFlight += 1
            session.publish(.notice, RuntimeCompletion(lease: session.runtime.completionLease, batch: wireBatch([])))
            let encoded = CollectionFacts.encodedReports, applied = session.appliedBatches
            let began = CACurrentMediaTime()
            node.scrollViewDidEndDecelerating(scroll)
            callbackDurations.append((CACurrentMediaTime() - began) * 1000)
            XCTAssertEqual(CollectionFacts.encodedReports, encoded, "the end delegate only requests a bounded slice")
            XCTAssertEqual(session.appliedBatches, applied, "no publication lands in the end delegate")
            XCTAssertTrue(session.hasPublished(), "the end delegate leaves queued publications to the main queue")
            XCTAssertEqual(events.suffix(2), ["scroll", "scrollend"])
            node.handlers.subtract(["scroll", "scrollend"])
            waitForWindow()
            restWindows.append(p.listHost.entries[node.id]!.snapshot.rows.map(\.index))
        }
        XCTAssertEqual(restWindows, [Array(2...8), Array(0...3)], "base's forward and reversed rest windows")
        XCTAssertEqual(p.views.count, openingViews, "the reversed rest has base's opening view count")
        XCTAssertTrue(retired.contains { $0 > 0 })
        XCTAssertTrue(reports.allSatisfy { bytes in
            bytes.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 88, as: UInt32.self) } < UInt32.max - 1
        }, "live pump slices never request an unlimited build or retirement")
        let measured = "T rest async=\(asynchronous) display_frames=120 end_callback_ms=\(callbackDurations) retire_rows=\(retired)"
            + " reports=\(reports.count) flushes=\(p.listHost.flushes - flushes) landings=\(landings)"
        XCTContext.runActivity(named: measured) { _ in }
        print(measured)
    }

    func testOpeningReportsOnlyTheVisibleWindowBeforeLaterFillSlices() throws {
        let p = Presenter()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        defer { p.reset(); window.isHidden = true }
        var wires: [Data] = []
        p.listHost.onFeedback = { wires.append($0) }
        p.apply(wireBatch([
            ["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0, "count": 100,
                "totalExtent": 5000, "pending": false, "rows": [["view": 10, "root": 10, "epoch": 1, "index": 0]], "correction": NSNull()]]],
            ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 10, "kind": "view"],
            ["op": "children", "id": 1, "ids": [10]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 300.0],
            ["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 300.0, "h": 50.0],
            ["op": "content", "id": 1, "w": 300.0, "h": 5000.0],
        ]))
        p.listHost.flushIfNeeded()
        func limit(_ wire: Data) -> UInt32 {
            wire.subdata(in: 88..<92).withUnsafeBytes { $0.loadUnaligned(as: UInt32.self) }
        }
        XCTAssertEqual(limit(try XCTUnwrap(wires.first)), 0, "opening rescues what shows without building the offscreen lead")
        wires.removeAll()
        p.listHost.changed(1)
        p.listHost.flushIfNeeded()
        XCTAssertTrue(wires.isEmpty, "unchanged geometry with no pending lead needs no report")
        p.listHost.fillSlice(1, limit: 3)
        XCTAssertEqual(limit(try XCTUnwrap(wires.first)), 3, "a later slice can build its bounded lead")
    }
}
#endif
