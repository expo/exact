#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactCollections

final class CollectionFillIOSTests: XCTestCase {
    private func snapshot(_ revision: Int, measured: Bool) -> [String: Any] {
        let rows: [[String: Any]] = (0..<4).map {
            ["view": 2 + $0, "root": 2 + $0, "epoch": 1, "index": $0, "size": 72, "measured": measured]
        }
        return ["op": "collections", "items": [["view": 1, "revision": revision, "scrollSequence": 0,
            "count": 100, "totalExtent": 7200, "pending": false, "rows": rows, "correction": NSNull()]]]
    }

    /// Four 72-point rows from index `0` (or the indices given), each at its
    /// place in a 100-row list under 20 points of padding.
    private func fixture(_ indices: [Int] = Array(0..<4), presenter p: Presenter = Presenter()) throws -> (Presenter, UIWindow, UIScrollView) {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 300, height: 200))
        window.addSubview(p.viewport)
        p.viewport.frame = window.bounds
        window.makeKeyAndVisible()
        var ops: [[String: Any]] = [
            ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll", "padding_top": 20]],
            ["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0,
                "count": 100, "totalExtent": 7200, "pending": false,
                "rows": indices.enumerated().map { ["view": 2 + $0.offset, "root": 2 + $0.offset, "epoch": 1, "index": $0.element] },
                "correction": NSNull()]]],
        ]
        for (i, index) in indices.enumerated() {
            ops.append(["op": "create", "id": 2 + i, "kind": "view"])
            ops.append(["op": "frame", "id": 2 + i, "x": 10, "y": 20 + 72 * index, "w": 280, "h": 72])
        }
        ops += [["op": "children", "id": 1, "ids": indices.indices.map { 2 + $0 }], ["op": "roots", "ids": [1]],
                ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 200],
                ["op": "content", "id": 1, "w": 300, "h": 7220]]
        p.apply(wireBatch(ops))
        window.layoutIfNeeded()
        let scroll = try XCTUnwrap(p.views[1]?.scroll)
        return (p, window, scroll)
    }

    /// Advance offsets and slices at 60 Hz, draining owner work between frames.
    /// The bounds include the final fill and retirement after the reversal.
    func testScriptedFlingCountsFillsFlushesAndSlices() throws {
        var rests: [[Int]] = []
        for asynchronous in [false, true] {
            let previous = ExactSession.asyncFills
            ExactSession.asyncFills = asynchronous
            defer { ExactSession.asyncFills = previous }
            let session = ExactApp.shared.makeSession(label: "counted-fling")
            let p = session.presenter
            let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
            defer { session.destroy(); window.isHidden = true }
            window.addSubview(p.viewport)
            p.viewport.frame = window.bounds
            window.makeKeyAndVisible()
            XCTAssertNil(session.boot(plan: try fixturePlan("SCROLL_SNAP"), size: window.bounds.size).error)
            let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "strip" })
            let scroll = try XCTUnwrap(node.scrollView)
            p.scrollPump.settle()
            waitForMainQueue("initial list continuations")
            var time = CACurrentMediaTime()
            p.scrollPump.clock = { time }
            var reports = 0, landings = 0
            let feedback = p.listHost.onFeedback, fill = p.listHost.onFill
            p.listHost.onFeedback = { reports += 1; feedback?($0) }
            if let fill { p.listHost.onFill = { reports += 1; fill($0, $1) } }
            session.onLandForTests = { kind, _ in if case .fill = kind { landings += 1 } }
            var settledRows: [Int] = []
            let fills = p.scrollPump.fillPasses, flushes = p.listHost.flushes, slices = p.listHost.slices
            for frame in 0..<240 {
                time += 1.0 / 60
                let step = frame < 120 ? frame + 1 : 239 - frame
                scroll.contentOffset.x = CGFloat(step * 12)
                p.scrollPump.pump()
                session.drainFill()
                FrameClock.shared.drop(p.scrollPump)
                waitForMainQueue("frame's queued pump and collection continuation")
                XCTAssertTrue(p.listHost.covers(node.id), "frame \(frame) has every row it shows")
                if frame == 119 || frame == 239 {
                    time += 0.2
                    p.listHost.changed(node.id, user: true)
                    p.scrollPump.settle()
                    settledRows += p.listHost.entries[node.id]!.snapshot.rows.map(\.index)
                }
            }
            for _ in 0..<20 {
                time += 1.0 / 60
                p.scrollPump.pump()
                session.drainFill()
                FrameClock.shared.drop(p.scrollPump)
                waitForMainQueue("final fill and retirement")
                if p.listHost.fillPending.isEmpty { break }
            }
            XCTAssertTrue(p.listHost.fillPending.isEmpty)
            XCTAssertEqual(scroll.contentOffset.x, 0, accuracy: 0.001)
            let counts = [p.scrollPump.fillPasses - fills, p.listHost.flushes - flushes, p.listHost.slices - slices]
            XCTAssertEqual(settledRows, Array(7...13) + Array(0...3), "the two resting windows measured on base")
            rests.append(settledRows)
            if asynchronous {
                XCTAssertLessThanOrEqual(reports, 60)
                XCTAssertLessThanOrEqual(counts[1], 100)
                XCTAssertLessThanOrEqual(landings, 30)
                XCTAssertEqual(rests[0], rests[1], "the ordinary rest window is identical with synchronous fills")
            }
            XCTContext.runActivity(named: "T fling async=\(asynchronous) reports=\(reports) flushes=\(counts[1]) landings=\(landings) rest_rows=\(settledRows)") { _ in }
            // The synchronous route keeps its existing slice budget.
            let baseline = asynchronous ? [248, 305, 124] : [240, 280, 120]
            for (count, bound) in zip(counts, baseline) { XCTAssertLessThanOrEqual(count, bound + 2) }
            print("J fling async=\(asynchronous) frames=240 fills=\(counts[0]) flushes=\(counts[1]) slices=\(counts[2])")
        }
    }

    func testShopGridFortyFlingsWithReversalsAndRests() throws {
        var rests: [[[Int]]] = []
        for asynchronous in [false, true] {
            let previous = ExactSession.asyncFills
            ExactSession.asyncFills = asynchronous
            defer { ExactSession.asyncFills = previous }
            let session = ExactApp.shared.makeSession(label: "shop-grid-flings")
            let p = session.presenter
            let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 700))
            defer { session.destroy(); window.isHidden = true }
            window.addSubview(p.viewport); p.viewport.frame = window.bounds; window.makeKeyAndVisible()
            XCTAssertNil(session.boot(plan: try fixturePlan("SHOP_SCROLL"), size: window.bounds.size).error)
            let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "product-grid" })
            let scroll = try XCTUnwrap(node.scrollView)
            settleAnimated(session)
            waitForMainQueue("initial grid continuations")
            var time = CACurrentMediaTime(), landings = 0, retirements = 0
            let feedback = p.listHost.onFeedback
            p.listHost.onFeedback = { bytes in
                if bytes.withUnsafeBytes({ $0.loadUnaligned(fromByteOffset: 92, as: UInt32.self) }) & 4 != 0 { retirements += 1 }
                feedback?(bytes)
            }
            p.scrollPump.clock = { time }
            session.onLandForTests = { kind, _ in if case .fill = kind { landings += 1 } }
            let encoded = CollectionFacts.encodedReports, flushes = p.listHost.flushes
            let posts = Owner.shared.postedJobsForTests
            let entries = Owner.shared.sync { session.runtime.collectionFeedbackEntries }
            var settledRows: [[Int]] = []
            for fling in 0..<40 {
                let direction = (fling / 5) % 2 == 0 ? 1.0 : -1.0
                // A 3,000 pt/s release decays at UIKit's normal rate for one second.
                for frame in 0..<60 {
                    time += 1.0 / 60
                    let step = direction * 3000 * pow(0.998, Double(frame) * 1000 / 60) / 60
                    let maximum = scroll.contentSize.height - scroll.bounds.height
                    scroll.contentOffset.y = min(maximum, max(0, scroll.contentOffset.y + step))
                    p.scrollPump.pump()
                    Owner.shared.sync {}
                    FrameClock.shared.drop(p.scrollPump)
                    waitForMainQueue("frame's owner publication and pump")
                    waitFor("the frame's publication") { !session.hasPublished() }
                    XCTAssertTrue(p.listHost.covers(node.id), "fling \(fling), frame \(frame)")
                }
                time += 0.3
                node.scrollViewDidEndDecelerating(scroll)
                settleAnimated(session)
                waitForMainQueue("grid at rest")
                settledRows.append(p.listHost.entries[node.id]!.snapshot.rows.map(\.index))
            }
            let reports = CollectionFacts.encodedReports - encoded
            let owner = Owner.shared.sync { session.runtime.collectionFeedbackEntries } - entries
            let flushed = p.listHost.flushes - flushes
            let posted = Owner.shared.postedJobsForTests - posts
            XCTContext.runActivity(named: "T shop async=\(asynchronous) reports=\(reports) owner_entries=\(owner) owner_posts=\(posted) retirements=\(retirements) flushes=\(flushed) landings=\(landings)") { _ in }
            print("T shop async=\(asynchronous) flings=40 reports=\(reports) owner_entries=\(owner) owner_posts=\(posted) retirements=\(retirements) flushes=\(flushed) landings=\(landings) rests=\(settledRows)")
            rests.append(settledRows)
            let base = [[2, 3, 4, 5, 6, 7, 8], [6, 7, 8, 9, 10, 11, 12], [10, 11, 12, 13, 14, 15, 16],
                [15, 16, 17, 18, 19, 20], [19, 20, 21, 22, 23, 24, 25], [15, 16, 17, 18, 19, 20],
                [10, 11, 12, 13, 14, 15, 16], [6, 7, 8, 9, 10, 11, 12], [2, 3, 4, 5, 6, 7, 8], [0, 1, 2, 3]]
            XCTAssertEqual(settledRows, Array(repeating: base, count: 4).flatMap { $0 }, "rest windows recorded on T2")
            XCTAssertEqual(owner, reports, "each encoded report reaches the runner once")
            XCTAssertEqual(retirements, 0, "these flings stay inside the retirement band")
            XCTAssertEqual(landings > 0, asynchronous)
            XCTAssertLessThanOrEqual(landings, asynchronous ? 100 : 0)
            // Acknowledging row measurements needs no second report with the same viewport and pins.
            let budget = asynchronous ? 276 : 136
            XCTAssertLessThanOrEqual(reports, budget)
            XCTAssertLessThanOrEqual(flushed, budget)
            XCTAssertLessThanOrEqual(reports + posted, 600, "include every posted job, even jobs unrelated to travel")
            if asynchronous { XCTAssertEqual(rests[0], rests[1], "every resting window equals synchronous fills") }
        }
    }

    func testQuietFramesEncodeNothingAndPostNoOwnerJobs() throws {
        let (session, window, node, scroll) = try listFixture(plan: "SHOP_SCROLL", target: "product-grid")
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter
        waitForMainQueue("initial list continuations")
        p.listHost.fillSlice(node.id, limit: UInt32.max - 1)
        session.drainFill()
        settleAnimated(session)
        waitForMainQueue("measured grid coverage")
        var time = CACurrentMediaTime()
        p.scrollPump.clock = { time }
        node.scrollEnd = ScrollEnd(); node.scrollEnd?.live = true
        let entry = try XCTUnwrap(p.listHost.entries[node.id])
        let band = try XCTUnwrap(entry.snapshot.window)
        let previous = try XCTUnwrap(entry.lastFacts)
        XCTAssertFalse(entry.snapshot.pending)
        XCTAssertFalse(entry.owed || entry.gesture || entry.records.changed)
        XCTAssertTrue(entry.records.unacknowledged.isEmpty)
        XCTAssertNil(previous.focus); XCTAssertNil(previous.interaction)
        let encoded = CollectionFacts.encodedReports, flushes = p.listHost.flushes
        let entries = Owner.shared.sync { session.runtime.collectionFeedbackEntries }
        // Keep the owner occupied so only the mailbox can retain these moves.
        Owner.shared.sync {
            Owner.shared.callMain {
                let posts = Owner.shared.postedJobsForTests
                let sequence = entry.cursor.sequence
                for offset in 1...40 {
                    // Include a stale sample and two moves at the same time.
                    if offset % 3 == 0 { time += 0.2 }
                    else if offset % 3 == 1 { time += 1.0 / 60 }
                    let reads = p.listHost.geometryReads
                    scroll.contentOffset.y = CGFloat(offset)
                    XCTAssertEqual(p.listHost.geometryReads - reads, 1, "one geometry read per quiet callback")
                    let facts = p.listHost.geometry(node.id)!
                    let speed = p.scrollPump.velocity(node.id)
                    let ahead = speed * p.scrollPump.leadTime(node.id)
                    let top = max(0, facts.offset)
                    XCTAssertGreaterThanOrEqual(top + min(0, ahead), band.built[0])
                    XCTAssertLessThanOrEqual(top + facts.portMain + max(0, ahead), band.built[1])
                    XCTAssertEqual([facts.portMain, facts.portCross, facts.cross, facts.paddingStart, facts.paddingEnd],
                        [previous.portMain, previous.portCross, previous.cross, previous.paddingStart, previous.paddingEnd])
                    XCTAssertFalse(band.owesReveal(offset: top, port: facts.portMain, extent: entry.snapshot.extent))
                    XCTAssertFalse(band.owesEdge(offset: top, port: facts.portMain, velocity: speed))
                    XCTAssertFalse(band.owesRetirement(offset: top, port: facts.portMain, moving: true))
                    XCTAssertTrue(p.listHost.fillPending.isEmpty)
                }
                XCTAssertEqual(CollectionFacts.encodedReports, encoded)
                XCTAssertEqual(p.listHost.flushes, flushes)
                XCTAssertEqual(Owner.shared.postedJobsForTests, posts)
                XCTAssertEqual(entry.cursor.sequence, sequence + 40)
            }
        }
        Owner.shared.sync { XCTAssertEqual(session.runtime.collectionFeedbackEntries, entries) }
    }

    func testRetirementUsesAnExtentBandWithHysteresis() throws {
        let (p, window, _) = try measuredWindow()
        defer { p.reset(); window.isHidden = true }
        let band = try XCTUnwrap(p.listHost.entries[1]?.snapshot.window)
        // Moving rows retire three ports away and a rest trims to one.
        XCTAssertFalse(band.owesRetirement(offset: 651, port: 200, moving: true))
        XCTAssertTrue(band.owesRetirement(offset: 672, port: 200, moving: true))
        XCTAssertFalse(band.owesRetirement(offset: 251, port: 200, moving: false))
        XCTAssertTrue(band.owesRetirement(offset: 272, port: 200, moving: false))
    }

    func testEndBounceAndEndPaddingSendNoReports() throws {
        for atEnd in [false, true] {
            let (p, window, scroll) = try fixture(atEnd ? Array(96..<100) : Array(0..<4))
            defer { p.reset(); window.isHidden = true }
            p.listHost.onFeedback = { _ in }
            p.listHost.settle()
            var op = snapshot(2, measured: true)
            var items = try XCTUnwrap(op["items"] as? [[String: Any]])
            if atEnd {
                items[0]["rows"] = (0..<4).map {
                    ["view": 2 + $0, "root": 2 + $0, "epoch": 1, "index": 96 + $0, "size": 72, "measured": true]
                }
            }
            items[0]["window"] = ["built": atEnd ? [6912, 7200] : [0, 288],
                "retire": atEnd ? [6984, 7128] : [72, 216], "reveal": [0, 7200],
                "edges": [72, 7128], "margin": [0, 0], "mask": atEnd ? 2 : 0,
                "leadSeconds": 0.25, "maxLead": 400] as [String: Any]
            op["items"] = items
            p.apply(wireBatch([op, contentOp(1, 300, 200, padding: [20, 0, 40, 0]),
                ["op": "content", "id": 1, "w": 300, "h": 7260]]))
            scroll.contentOffset.y = atEnd ? 7020 : 0
            p.scrollPump.settle()
            XCTAssertTrue(p.listHost.fillPending.isEmpty, "the rest slice finishes before counting travel")
            var reports = 0
            p.listHost.onFeedback = { _ in reports += 1 }
            let flushes = p.listHost.flushes
            for step in 1...40 {
                scroll.contentOffset.y = CGFloat(atEnd ? 7020 + step * 2 : -step)
                XCTAssertTrue(p.listHost.covers(1))
            }
            XCTAssertEqual(reports, 0, "edge=\(atEnd ? "end" : "start")")
            XCTAssertEqual(p.listHost.flushes, flushes)
            XCTAssertTrue(p.listHost.fillPending.isEmpty)
            print("W bounce end=\(atEnd) callbacks=40 reports=\(reports)")
        }
    }

    func testALandingThatOnlyNeedsLeadRequestsABuildWithoutRetiring() throws {
        let (p, window, scroll) = try measuredWindow()
        defer { p.reset(); window.isHidden = true }
        var reports = 0, builds = 0, inFlight = false
        p.listHost.onFeedback = { _ in reports += 1 }
        p.listHost.onFill = { _, _ in builds += 1; inFlight = true }
        p.listHost.filling = { inFlight }
        p.listHost.fillSlice(1, limit: 3)
        scroll.contentOffset.y = 60
        p.listHost.motion = { _ in 3000 }
        inFlight = false
        p.listHost.landed(1, at: ProcessInfo.processInfo.systemUptime)
        p.listHost.fillSlice(1, limit: 3)
        XCTAssertEqual(reports, 0, "missing lead alone cannot owe a retire-only report")
        XCTAssertEqual(builds, 2)
    }

    private func measuredWindow(edge: Double = 7128, start: Double = 72, mask: Int = 0, presenter: Presenter = Presenter()) throws -> (Presenter, UIWindow, UIScrollView) {
        let (p, window, scroll) = try fixture(presenter: presenter)
        p.listHost.onFeedback = { _ in }
        p.listHost.settle()
        var op = snapshot(2, measured: true)
        var items = try XCTUnwrap(op["items"] as? [[String: Any]])
        items[0]["window"] = ["built": [0, 288], "retire": [72, 216], "reveal": [0, 7200], "edges": [start, edge],
            "margin": [0, 0], "mask": mask, "leadSeconds": 0.25, "maxLead": 400] as [String: Any]
        op["items"] = items
        p.apply(wireBatch([op]))
        p.listHost.settle()
        return (p, window, scroll)
    }

    func testQuietTravelKeepsPinChangesAndExplicitSlices() throws {
        let (p, window, scroll) = try measuredWindow()
        defer { p.reset(); window.isHidden = true }
        var reports: [Data] = []
        p.listHost.onFeedback = { reports.append($0) }
        let flushes = p.listHost.flushes
        for offset in 1...60 { scroll.contentOffset.y = CGFloat(offset) }
        XCTAssertTrue(reports.isEmpty)
        XCTAssertEqual(p.listHost.flushes, flushes, "inside the window no flush is owed")
        p.listHost.pointer(2)
        p.listHost.settle()
        XCTAssertEqual(p.listHost.entries[1]?.lastFacts?.interaction, 2)
        p.listHost.pointer(nil)
        p.listHost.settle()
        XCTAssertNil(p.listHost.entries[1]?.lastFacts?.interaction)
        let before = reports.count
        scroll.contentOffset.y = 61
        p.listHost.fillSlice(1, limit: 5)
        XCTAssertGreaterThan(reports.count, before, "an explicit slice cannot be discarded as travel")
    }

    func testTheEndBandOwesAFreshReportAtTheSameOffset() throws {
        let (p, window, scroll) = try measuredWindow(edge: 260, mask: 10)
        defer { p.reset(); window.isHidden = true }
        let facts = try XCTUnwrap(p.listHost.geometry(1))
        let boundary = 260 - facts.portMain
        p.listHost.motion = { _ in nil }
        var offsets: [Double] = []
        p.listHost.onFeedback = { bytes in
            offsets.append(bytes.withUnsafeBytes { Double(bitPattern: $0.loadUnaligned(fromByteOffset: 24, as: UInt64.self)) })
        }
        for offset in 0...Int(boundary) + 1 {
            scroll.contentOffset.y = CGFloat(offset) + CGFloat(facts.paddingStart)
        }
        XCTAssertEqual(offsets, [boundary + 1], "contact at the band's bound is silent; entering it reports immediately")
    }

    func testEnteringAnEdgeAndReversingDrainsAHeldPublicationBeforeReporting() throws {
        let session = ExactApp.shared.makeSession(label: "held-edge-reversal")
        let (p, window, scroll) = try measuredWindow(edge: 260, mask: 10, presenter: session.presenter)
        defer { session.destroy(); window.isHidden = true }
        let node = try XCTUnwrap(p.views[1])
        node.scrollEnd = ScrollEnd(); node.scrollEnd?.live = true
        p.listHost.motion = { _ in nil }
        let facts = try XCTUnwrap(p.listHost.geometry(1))
        let boundary = 260 - facts.portMain
        scroll.contentOffset.y = CGFloat(boundary + facts.paddingStart)
        var landed = 0, offsets: [Double] = []
        session.onLandForTests = { _, _ in landed += 1 }
        for _ in 0..<8 {
            session.noticesInFlight += 1
            session.publish(.notice, RuntimeCompletion(lease: session.runtime.completionLease, batch: wireBatch([])))
        }
        session.holdFillForTests()
        session.publish(.fill(1), RuntimeCompletion(lease: session.runtime.completionLease, batch: wireBatch([])))
        XCTAssertTrue(session.hasPublished())
        p.listHost.onFeedback = { bytes in
            XCTAssertFalse(session.hasPublished(), "all earlier publications precede the fresh edge report")
            XCTAssertEqual(landed, 9)
            let offset = bytes.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 24, as: Double.self) }
            offsets.append(offset)
            // Acknowledge entering the band so the reversal must rearm it.
            var op = self.snapshot(2 + offsets.count, measured: true)
            var items = op["items"] as! [[String: Any]]
            items[0]["window"] = ["built": [0, 288], "retire": [72, 216], "reveal": [0, 7200],
                "edges": [72, 260], "margin": [0, 0], "mask": offset > boundary ? 2 : 10,
                "leadSeconds": 0.25, "maxLead": 400] as [String: Any]
            op["items"] = items
            p.apply(wireBatch([op]))
        }
        scroll.contentOffset.y += 1
        XCTAssertEqual(offsets, [boundary + 1], "entering the edge reports before this callback returns")
        scroll.contentOffset.y -= 2
        XCTAssertEqual(offsets, [boundary + 1, boundary - 1], "the next callback rearms at its current offset")
        XCTAssertTrue(p.listHost.covers(1))
    }

    func testAccelerationAndReversalUseTheCurrentEdgeLead() throws {
        let (p, window, scroll) = try measuredWindow(edge: 350, start: 0, mask: 15)
        defer { p.reset(); window.isHidden = true }
        var time = CACurrentMediaTime()
        p.scrollPump.clock = { time }
        var offsets: [Double] = []
        p.listHost.onFeedback = { bytes in
            offsets.append(bytes.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 24, as: Double.self) })
        }
        scroll.contentOffset.y = 20
        for offset in [21, 22, 42] {
            time += 1.0 / 60
            scroll.contentOffset.y = CGFloat(offset)
        }
        let current = p.scrollPump.velocity(1)
        XCTAssertGreaterThan(current, 600)
        XCTAssertEqual(offsets, [42], "acceleration enters the expanded end band on this frame")
        // At the next frame the velocity reverses: its start band expands now.
        let before = offsets.count
        time += 1.0 / 60
        scroll.contentOffset.y = 32
        XCTAssertLessThan(p.scrollPump.velocity(1), 0)
        XCTAssertGreaterThan(offsets.count, before)
        XCTAssertEqual(offsets.last, 32, "reversal uses this frame's start lead")
    }

    func testReenteringAnEdgeUsesTheRunnersRearmedWindow() throws {
        let (session, window, node, scroll) = try listFixture(plan: "QUIET_EDGE")
        let p = session.presenter
        defer { session.destroy(); window.isHidden = true }
        node.scrollEnd = ScrollEnd(); node.scrollEnd?.live = true
        p.listHost.motion = { _ in nil }
        let feedback = p.listHost.onFeedback
        var offsets: [Double] = []
        p.listHost.onFeedback = { bytes in
            offsets.append(bytes.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 24, as: Double.self) })
            feedback?(bytes)
        }
        func reaches() throws -> String {
            try XCTUnwrap(p.views.values.first { $0.props["testId"] == "reaches" }).props["text"] ?? ""
        }
        let rows = try XCTUnwrap(p.listHost.entries[node.id]).snapshot.rows.map(\.view)
        let flushes = p.listHost.flushes
        for (offset, count) in [(201.0, "1"), (200.0, "1"), (201.0, "2")] {
            scroll.contentOffset.y = CGFloat(offset)
            XCTAssertEqual(try reaches(), count, "edge at \(offset)")
            XCTAssertTrue(p.listHost.covers(node.id))
        }
        XCTAssertEqual(offsets, [201, 200, 201], "exit publishes the mask that makes reentry report")
        XCTAssertEqual(p.listHost.flushes - flushes, 3)
        XCTAssertEqual(p.listHost.entries[node.id]?.snapshot.rows.map(\.view), rows)
        offsets.removeAll()
        for offset in 202...340 { scroll.contentOffset.y = CGFloat(offset) }
        XCTAssertEqual(try reaches(), "2", "reaching and bouncing past the list end does not repeat reachend")
        XCTAssertTrue(offsets.isEmpty, "the end edge remains armed only after leaving its band")
    }

    private func settleAnimated(_ session: ExactSession) {
        for _ in 0..<8 {
            session.presenter.scrollPump.settle()
            session.drainFill()
            if session.presenter.listHost.fillPending.isEmpty { return }
        }
        XCTFail("the animated list still owes a fill after settlement")
    }

    private func listFixture(plan: String = "QUIET_SCROLL", target: String = "feed") throws -> (ExactSession, UIWindow, NodeView, UIScrollView) {
        let session = ExactApp.shared.makeSession(label: "quiet-reveals")
        let p = session.presenter
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        window.addSubview(p.viewport); p.viewport.frame = window.bounds; window.makeKeyAndVisible()
        XCTAssertNil(session.boot(plan: try fixturePlan(plan), size: window.bounds.size).error)
        session.apply(session.runtime.startOnFrame(false, at: session.now()))
        settleAnimated(session)
        let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == target })
        return (session, window, node, try XCTUnwrap(node.scrollView))
    }

    func testQuietTravelReportsWhenAPrefetchedAnimationFirstShows() throws {
        let (session, window, _, scroll) = try listFixture()
        let p = session.presenter
        defer { session.destroy(); window.isHidden = true }
        let coin = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "coin-4" })
        func animation() throws -> CAAnimation {
            let key = try XCTUnwrap(coin.layer.animationKeys()?.first { $0.contains("appear") })
            return try XCTUnwrap(coin.layer.animation(forKey: key))
        }
        XCTAssertEqual(try animation().speed, 0, "prefetched animation is held")
        var reports = 0
        let feedback = p.listHost.onFeedback
        p.listHost.onFeedback = { reports += 1; feedback?($0) }
        session.holdFillForTests()
        session.publish(.fill(try XCTUnwrap(p.listHost.entries.keys.first)),
            RuntimeCompletion(lease: session.runtime.completionLease, batch: wireBatch([])))
        scroll.contentOffset.y = 150
        XCTAssertFalse(session.hasPublished(), "the first reveal drains the earlier held publication")
        XCTAssertGreaterThan(reports, 0, "first overlap reports synchronously, inside measured coverage")
        XCTAssertEqual(try animation().speed, 1, "the animation starts in its first visible frame")
    }

    func testSuccessiveAnchorCorrectionsSurviveQuietTravelAndAHeldPublication() throws {
        let (p, window, scroll) = try measuredWindow()
        defer { p.reset(); window.isHidden = true }
        var reports = 0
        p.listHost.onFeedback = { _ in reports += 1 }
        func correction(_ revision: Int, from: Double, to: Double) throws -> Batch {
            var op = snapshot(revision, measured: true)
            var items = try XCTUnwrap(op["items"] as? [[String: Any]])
            items[0]["correction"] = ["scrollSequence": p.listHost.entries[1]!.cursor.sequence,
                "from": from, "offset": to] as [String: Any]
            items[0]["window"] = ["built": [0, 288], "retire": [72, 216], "reveal": [0, 7200],
                "edges": [72, 7128], "margin": [0, 0], "mask": 0, "leadSeconds": 0.25, "maxLead": 400] as [String: Any]
            op["items"] = items
            return wireBatch([op])
        }
        // The runner regression checks that travel acknowledges the first
        // correction and that the next publication retains a relative origin.
        p.apply(try correction(3, from: 0, to: 10))
        p.listHost.settle()
        XCTAssertEqual(scroll.contentOffset.y, 10, accuracy: 0.01)
        let before = reports, flushes = p.listHost.flushes
        scroll.contentOffset.y = 15
        let held = try correction(4, from: 15, to: 25)
        scroll.contentOffset.y = 20
        XCTAssertEqual(reports, before)
        XCTAssertEqual(p.listHost.flushes, flushes, "both frames take UIKit's quiet path")
        XCTAssertFalse(p.listHost.fillPending.contains(1))
        p.apply(held)
        XCTAssertEqual(scroll.contentOffset.y, 30, accuracy: 0.01,
            "the held correction adds to travel since it was published")
        XCTAssertTrue(p.listHost.covers(1))
    }

    func testUncoveredTravelDrainsBeforeTheRetirementCheck() throws {
        let (p, window, scroll) = try measuredWindow()
        defer { p.reset(); window.isHidden = true }
        var filling = true, drained = false, reports = 0
        p.listHost.filling = { filling }
        p.listHost.drain = { filling = false; drained = true }
        p.listHost.onFeedback = { _ in
            XCTAssertTrue(drained)
            reports += 1
        }
        scroll.contentOffset.y = 2000
        XCTAssertTrue(drained, "a pending fill must land before rescuing uncovered rows")
        XCTAssertGreaterThan(reports, 0, "the uncovered port reports in the callback")
    }

    func testAHeldAbsoluteCorrectionCannotReplaceTheCurrentFramesTravel() throws {
        let session = ExactApp.shared.makeSession(label: "held-absolute-correction")
        let indices = Array(12..<18)
        let (p, window, scroll) = try fixture(indices, presenter: session.presenter)
        defer { session.destroy(); window.isHidden = true }
        p.listHost.onFeedback = { _ in }
        scroll.contentOffset.y = 1000
        p.listHost.settle()
        let sequence = try XCTUnwrap(p.listHost.entries[1]?.cursor.sequence)
        let rows = indices.enumerated().map { index, row in
            ["view": 2 + index, "root": 2 + index, "epoch": 1, "index": row, "size": 72, "measured": true] as [String: Any]
        }
        let batch = wireBatch([["op": "collections", "items": [["view": 1, "revision": 2,
            "scrollSequence": sequence, "count": 100, "totalExtent": 7200, "pending": false,
            "rows": rows, "correction": ["scrollSequence": sequence, "offset": 1100]]]]])
        session.noticesInFlight += 1
        session.publish(.notice, RuntimeCompletion(lease: session.runtime.completionLease, batch: batch))
        XCTAssertTrue(session.hasPublished())
        var landed = 0
        session.onLandForTests = { _, _ in
            landed += 1
            XCTAssertEqual(p.listHost.entries[1]?.cursor.sequence, sequence + 1,
                "the reader's move precedes the held correction")
        }
        scroll.contentOffset.y = 990
        XCTAssertEqual(landed, 0, "quiet travel leaves publication delivery outside the callback")
        session.drainFill()
        XCTAssertEqual(landed, 1)
        XCTAssertFalse(session.hasPublished())
        XCTAssertEqual(scroll.contentOffset.y, 990, accuracy: 0.01)
        XCTAssertEqual(p.listHost.entries[1]?.cursor.sequence, sequence + 1,
            "one frame advances the sequence exactly once")
        XCTAssertTrue(p.listHost.covers(1))
    }

    func testAReversalWhilePublicationIsHeldKeepsTheRowsUnderTheFinger() throws {
        let previous = ExactSession.asyncFills
        ExactSession.asyncFills = true
        defer { ExactSession.asyncFills = previous }
        let session = ExactApp.shared.makeSession(label: "held-reversal")
        let p = session.presenter
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        defer { session.destroy(); window.isHidden = true }
        window.addSubview(p.viewport); p.viewport.frame = window.bounds; window.makeKeyAndVisible()
        XCTAssertNil(session.boot(plan: try fixturePlan("SCROLL_SNAP"), size: window.bounds.size).error)
        p.scrollPump.settle()
        let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "strip" })
        let scroll = try XCTUnwrap(node.scrollView)
        let initial = try XCTUnwrap(p.listHost.entries[node.id]?.snapshot)
        let pin = try XCTUnwrap(initial.rows.first?.root)
        var held: RuntimeCompletion?, busy = false
        p.listHost.filling = { busy }
        p.listHost.onFill = { _, bytes in
            busy = true
            session.runtime.collectionFeedbackAsync(bytes, now: session.now()) { held = $0 }
        }
        let motion = p.listHost.motion
        p.listHost.motion = { _ in 1800 }
        scroll.contentOffset.x = 300
        p.listHost.fillSlice(node.id, limit: 20)
        Owner.shared.sync {}
        let completion = try XCTUnwrap(held)
        XCTAssertTrue(busy)
        XCTAssertTrue(p.views[pin] != nil, "unpublished construction cannot destroy the old row")
        p.listHost.pointer(pin)
        scroll.contentOffset.x = 0
        XCTAssertTrue(p.listHost.covers(node.id), "the reversal still shows the original rows")
        busy = false
        session.publish(.fill(node.id), completion)
        node.scrollViewDidScroll(scroll)
        waitFor("the held publication lands outside the callback") { !session.hasPublished() }
        XCTAssertFalse(session.hasPublished())
        XCTAssertTrue(p.listHost.covers(node.id))
        XCTAssertNotNil(p.views[pin], "the fresh interaction pin survives retirement")
        p.listHost.pointer(nil)
        p.listHost.motion = motion
        p.scrollPump.settle()
        XCTAssertEqual(p.listHost.entries[node.id]?.snapshot.rows.map(\.index), initial.rows.map(\.index))
    }

    func testAHeldFillRefreshesQuietTravelBeforeAnAnchorDeletion() throws {
        let (session, window, node, scroll) = try listFixture(plan: "SHOP_SCROLL", target: "product-grid")
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter
        let size = try XCTUnwrap(p.listHost.entries[node.id]?.snapshot.rows.first?.size)
        scroll.contentOffset.y = CGFloat(size * 10)
        settleAnimated(session)
        waitForMainQueue("the deep window settles")
        node.scrollEnd = ScrollEnd(); node.scrollEnd?.live = true
        let row = try XCTUnwrap(p.listHost.entries[node.id]?.snapshot.rows.first {
            guard let view = p.views[$0.view] else { return false }
            let frame = view.convert(view.bounds, to: scroll)
            return frame.minY <= scroll.contentOffset.y && frame.maxY > scroll.contentOffset.y
        })
        let wrapper = try XCTUnwrap(p.views[row.view])
        let button = try XCTUnwrap(p.views.values.first { $0.kind == "button" && $0.isDescendant(of: wrapper) })
        var held: RuntimeCompletion?, busy = false
        p.listHost.filling = { busy }
        p.listHost.onFill = { _, bytes in
            busy = true
            session.runtime.collectionFeedbackAsync(bytes, now: session.now()) { held = $0 }
        }
        p.listHost.motion = { _ in 3000 }
        p.listHost.fillSlice(node.id, limit: UInt32.max - 1)
        Owner.shared.sync {}
        let completion = try XCTUnwrap(held)
        let revision = try XCTUnwrap(p.listHost.entries[node.id]?.snapshot.revision)
        p.listHost.motion = { _ in 0 }
        let encoded = CollectionFacts.encodedReports, flushed = p.listHost.flushes
        scroll.contentOffset.y += CGFloat(size)
        XCTAssertEqual(CollectionFacts.encodedReports, encoded, "travel after owner completion is quiet")
        busy = false
        session.publish(.fill(node.id), completion)
        session.landFill()
        XCTAssertGreaterThan(try XCTUnwrap(p.listHost.entries[node.id]?.snapshot.revision), revision)
        XCTAssertEqual(CollectionFacts.encodedReports, encoded + 1, "new rows need one host measurement report")
        XCTAssertEqual(p.listHost.flushes, flushed + 1)
        XCTAssertNotNil(p.listHost.entries[node.id]?.snapshot.window, "validated measurements restore quiet travel")
        let before = scroll.contentOffset.y
        // Delete the former anchor before another offset callback can refresh the mailbox.
        let update = session.runtime.press(button.id, now: session.now())
        let changed = try XCTUnwrap(update.ops.flatMap { $0.collections ?? [] }.first { $0.view == node.id })
        XCTAssertEqual(changed.count, 119, "the data update deletes the former anchor")
        XCTAssertEqual(try XCTUnwrap(changed.correction?.from), Double(before), accuracy: 0.01)
        session.apply(update)
        XCTAssertEqual(scroll.contentOffset.y, before - CGFloat(size), accuracy: 0.01)
        XCTAssertTrue(p.listHost.covers(node.id))
    }

    func testCoveredRetirementWaitsForThePumpAndUsesFreshFacts() throws {
        let session = ExactApp.shared.makeSession(label: "deferred-retirement")
        let indices = Array(0..<14)
        let (p, window, scroll) = try fixture(indices, presenter: session.presenter)
        defer { session.destroy(); window.isHidden = true }
        p.listHost.onFeedback = { _ in }
        p.listHost.settle()
        let rows = indices.map { ["view": 2 + $0, "root": 2 + $0, "epoch": 1, "index": $0, "size": 72, "measured": true] as [String: Any] }
        p.apply(wireBatch([["op": "collections", "items": [["view": 1, "revision": 2,
            "scrollSequence": 0, "count": 100, "totalExtent": 7200, "pending": false, "rows": rows,
            "window": ["built": [0, 1008], "retire": [72, 936], "reveal": [0, 7200], "edges": [72, 7128],
                "margin": [0, 0], "mask": 0, "leadSeconds": 0.25, "maxLead": 400]]]]]))
        p.listHost.settle()
        p.listHost.motion = { _ in 0 }
        let node = try XCTUnwrap(p.views[1])
        node.scrollEnd = ScrollEnd(); node.scrollEnd?.live = true
        scroll.contentOffset.y += CGFloat(671 - (try XCTUnwrap(p.listHost.geometry(1))).offset)
        var reports: [Data] = [], drains = 0
        let drain = p.listHost.drain
        p.listHost.drain = { drains += 1; drain?() }
        p.listHost.onFeedback = { reports.append($0) }
        let encoded = CollectionFacts.encodedReports, flushed = p.listHost.flushes
        scroll.contentOffset.y += 1
        XCTAssertTrue(p.listHost.covers(1))
        XCTAssertTrue(reports.isEmpty)
        XCTAssertEqual(CollectionFacts.encodedReports, encoded)
        XCTAssertEqual(p.listHost.flushes, flushed)
        XCTAssertEqual(drains, 0, "retirement cannot drain publications inside the scroll callback")
        XCTAssertTrue(p.listHost.fillPending.contains(1))
        scroll.contentOffset.y += 2
        p.listHost.fillSlice(1, limit: 10)
        XCTAssertEqual(reports.count, 1)
        let report = try XCTUnwrap(reports.first)
        XCTAssertEqual(report.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 24, as: Double.self) }, 674)
        XCTAssertNotEqual(report.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 92, as: UInt32.self) } & 4, 0,
            "the pump retires synchronously from its fresh offset")
    }

    func testAHeldWindowedFillRechecksRetirementAfterAReversal() throws {
        let previous = ExactSession.asyncFills
        ExactSession.asyncFills = true
        defer { ExactSession.asyncFills = previous }
        for reversal in [0.0, 100.0] {
            let (session, window, node, scroll) = try listFixture(plan: "SHOP_SCROLL", target: "product-grid")
            defer { session.destroy(); window.isHidden = true }
            let p = session.presenter
            waitForMainQueue("initial grid continuations")
            var time = CACurrentMediaTime()
            p.scrollPump.clock = { time }
            node.scrollEnd = ScrollEnd(); node.scrollEnd?.live = true
            p.listHost.motion = { _ in 6000 }
            scroll.contentOffset.y = 600
            p.listHost.fillSlice(node.id, limit: UInt32.max - 1)
            session.drainFill()
            waitForMainQueue("the extended window lands")
            let entry = try XCTUnwrap(p.listHost.entries[node.id])
            let band = try XCTUnwrap(entry.snapshot.window)
            let port = try XCTUnwrap(p.listHost.geometry(node.id)).portMain
            let crossing = band.retire[0] + port + band.maxLead + 1
            XCTAssertLessThan(crossing + port, band.built[1], "the retire crossing stays covered")
            var held: RuntimeCompletion?, busy = false
            p.listHost.filling = { busy }
            p.listHost.onFill = { _, bytes in
                busy = true
                session.runtime.collectionFeedbackAsync(bytes, now: session.now()) { held = $0 }
            }
            scroll.contentOffset.y += 1
            p.listHost.fillSlice(node.id, limit: UInt32.max - 1)
            Owner.shared.sync {}
            let completion = try XCTUnwrap(held)
            var reports: [Data] = []
            let feedback = p.listHost.onFeedback
            p.listHost.onFeedback = { reports.append($0); feedback?($0) }
            p.listHost.motion = { _ in 0 }
            scroll.contentOffset.y = CGFloat(crossing)
            p.listHost.fillSlice(node.id, limit: UInt32.max - 1)
            XCTAssertTrue(reports.isEmpty, "the in-flight build holds retirement")
            if reversal > 0 { scroll.contentOffset.y -= CGFloat(reversal) }
            let offset = try XCTUnwrap(p.listHost.geometry(node.id)).offset
            XCTAssertEqual(band.owesRetirement(offset: offset, port: port, moving: true), reversal == 0)
            let retained = entry.snapshot.rows.filter { row in
                guard let view = p.views[row.view] else { return false }
                let frame = view.convert(view.bounds, to: scroll)
                return frame.maxY > offset - band.maxLead && frame.maxY <= offset - port
            }
            XCTAssertFalse(retained.isEmpty, "rows in the hysteresis band exercise the destruction cap")
            busy = false
            session.publish(.fill(node.id), completion)
            session.landFill()
            time += 0.1
            p.scrollPump.pump()
            waitForMainQueue("the fresh retirement decision")
            XCTAssertEqual(reports.count, reversal == 0 ? 1 : 0)
            if let report = reports.first {
                XCTAssertEqual(report.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 88, as: UInt32.self) }, 0,
                    "a windowed retirement cannot inherit the unlimited build limit")
                XCTAssertNotEqual(report.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 92, as: UInt32.self) } & 4, 0)
            }
            for row in retained { XCTAssertNotNil(p.views[row.view], "row \(row.index) remains inside the retire band") }
            XCTAssertTrue(p.listHost.covers(node.id))
            print("T held retirement reversal=\(reversal) reports=\(reports.count) retained=\(retained.count)")
        }
    }

    func testOffsetReportsReuseRowsAndAChangedHeightIsMeasuredOnce() throws {
        let (p, window, scroll) = try fixture()
        defer { p.reset(); window.isHidden = true }
        var reports: [Data] = []
        p.listHost.onFeedback = { reports.append($0) }
        p.listHost.settle()
        let records = try XCTUnwrap(p.listHost.entries[1]?.records)
        let measured = records.measuredRows, spans = records.spanReads
        let reportStart = reports.count
        for offset in 1...100 {
            scroll.contentOffset.y = CGFloat(offset % 60)
            p.listHost.fillSlice(1, limit: 5)
            XCTAssertTrue(p.listHost.covers(1))
            _ = p.listHost.rowsToCover(1, ahead: 100)
        }
        let offsetMeasurements = records.measuredRows - measured
        let offsetSpans = records.spanReads - spans
        let offsetReports = reports.count - reportStart
        XCTAssertEqual(offsetMeasurements, 0, "offset-only reports measure no rows")
        XCTAssertEqual(offsetSpans, 0, "coverage reuses the mounted spans")
        XCTAssertEqual(reports.count - reportStart, 100, "every offset still sends its report")
        let before = reports.count
        p.apply(wireBatch([
            ["op": "props", "id": 2, "set": ["text": "a taller row"]],
            ["op": "frame", "id": 2, "x": 10, "y": 20, "w": 280, "h": 90],
        ]))
        p.listHost.settle()
        XCTAssertEqual(records.measuredRows, measured + 1, "content's changed border box is measured once")
        XCTAssertGreaterThan(reports.count, before)
        XCTAssertEqual(records.rows[2]?.measurement?.size, 90)
        XCTAssertEqual(records.rows[3]?.measurement?.size, 72)
        print("M offset_reports=\(offsetReports) row_measurements=\(offsetMeasurements) span_reads=\(offsetSpans) changed_height_measurements=\(records.measuredRows - measured - offsetMeasurements)")
    }

    func testAReadBeforeTheBatchAppliesItsFramesCannotCacheTheOldHeight() throws {
        let (p, window, _) = try fixture()
        defer { p.reset(); window.isHidden = true }
        p.listHost.onFeedback = { _ in }
        p.listHost.settle()
        let records = try XCTUnwrap(p.listHost.entries[1]?.records)
        let before = records.measuredRows
        let batch = wireBatch([["op": "frame", "id": 2, "x": 10, "y": 20, "w": 280, "h": 90]])
        p.listHost.beginBatch(batch)
        _ = p.listHost.geometry(1)
        XCTAssertEqual(records.rows[2]?.measurement?.size, 72)
        p.applyGeometry(batch.ops[0])
        p.listHost.endBatch()
        p.listHost.settle()
        XCTAssertEqual(records.rows[2]?.measurement?.size, 90)
        XCTAssertEqual(p.listHost.entries[1]?.lastFacts?.measurements.first?.size, 90)
        XCTAssertEqual(records.measuredRows, before + 2, "only the changed row is read, before and after its frame")
    }

    func testAReadBeforePaddingAppliesCannotSettleTheOuterBatchPort() throws {
        for nested in [false, true] {
            let (p, window, scroll) = try fixture()
            defer { p.reset(); window.isHidden = true }
            p.listHost.onFeedback = { _ in }
            p.listHost.settle()
            let records = try XCTUnwrap(p.listHost.entries[1]?.records)
            let measured = records.measuredRows, spans = records.spanReads
            let batch = wireBatch([contentOp(1, 300, 200, padding: [20, 0, 40, 0])])
            p.listHost.beginBatch(batch)
            if nested {
                p.listHost.beginBatch(wireBatch([]))
                p.listHost.endBatch()
            } else {
                XCTAssertEqual(p.listHost.geometry(1)?.paddingEnd, 0)
            }
            p.applyGeometry(batch.ops[0])
            p.listHost.endBatch()
            p.listHost.settle()
            XCTAssertEqual(p.listHost.geometry(1)?.paddingEnd, 40, "nested=\(nested)")
            XCTAssertEqual(p.listHost.entries[1]?.lastFacts?.paddingEnd, 40, "the report uses the applied padding")
            for offset in 1...100 {
                scroll.contentOffset.y = CGFloat(offset % 60)
                XCTAssertEqual(p.listHost.geometry(1)?.paddingEnd, 40)
            }
            XCTAssertEqual(records.measuredRows, measured, "padding and offsets do not remeasure rows")
            XCTAssertEqual(records.spanReads, spans, "padding and offsets keep the row spans")
        }
    }

    func testAcknowledgedMeasurementsStayCachedAndEpochChangesInvalidateThem() throws {
        let (p, window, _) = try fixture()
        defer { p.reset(); window.isHidden = true }
        var reports: [Data] = []
        p.listHost.onFeedback = { reports.append($0) }
        p.listHost.settle()
        let records = try XCTUnwrap(p.listHost.entries[1]?.records)
        let measured = records.measuredRows
        p.apply(wireBatch([snapshot(2, measured: true)]))
        p.listHost.settle()
        p.listHost.userIntent(1, travel: true)
        p.listHost.settle()
        let acknowledged = try XCTUnwrap(reports.last)
        XCTAssertEqual(acknowledged.count, 100, "acknowledged rows leave the report")
        XCTAssertEqual(records.measuredRows, measured, "acknowledgement does not remeasure")
        var next = snapshot(3, measured: false)
        var items = try XCTUnwrap(next["items"] as? [[String: Any]])
        var rows = try XCTUnwrap(items[0]["rows"] as? [[String: Any]])
        rows[0]["epoch"] = 2
        items[0]["rows"] = rows; next["items"] = items
        p.apply(wireBatch([next]))
        p.listHost.settle()
        XCTAssertEqual(records.measuredRows, measured + 1)
        XCTAssertEqual(records.rows[2]?.measurement?.epoch, 2)
        XCTAssertEqual(records.unacknowledged.count, 4, "unacknowledged rows are retried")
        rows.append(["view": 99, "root": 99, "epoch": 1, "index": 4])
        items[0]["rows"] = rows; items[0]["revision"] = 4; next["items"] = items
        p.apply(wireBatch([next]))
        p.listHost.settle()
        XCTAssertNil(records.rows[99]?.measurement)
        p.apply(wireBatch([
            ["op": "create", "id": 99, "kind": "view"],
            ["op": "frame", "id": 99, "x": 10, "y": 308, "w": 280, "h": 72],
            ["op": "children", "id": 1, "ids": [2, 3, 4, 5, 99]],
        ]))
        p.listHost.settle()
        XCTAssertEqual(records.measuredRows, measured + 2, "a wrapper mounted later is measured")
        XCTAssertEqual(records.rows[99]?.measurement?.size, 72)
    }

    func testCachedCoverageMatchesTheSortedWalkAcrossGapsOverlapsAndReordering() throws {
        let (p, window, scroll) = try fixture([0, 1, 2, 3, 99])
        defer { p.reset(); window.isHidden = true }
        p.listHost.onFeedback = { _ in }
        p.apply(wireBatch([["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 300]]))
        let phases = [
            [(20, 72), (92, 72), (164, 72), (236, 72), (7148, 72)],
            [(20, 72), (92, 72), (80, 180), (236, 72), (7148, 72)],
            [(1200, 100), (1000, 100), (1100, 100), (1300, 100), (7148, 72)],
            [(1200, 100), (1000, 100), (1100, 100), (1300, 100), (900, 100)],
        ]
        let records = try XCTUnwrap(p.listHost.entries[1]?.records)
        var offsets = 0
        for (phase, frames) in phases.enumerated() {
            let indices = phase == 2 ? [10, 11, 12, 13, 99] : [0, 1, 2, 3, 99]
            var ops: [[String: Any]] = [["op": "collections", "items": [[
                "view": 1, "revision": phase + 2, "scrollSequence": 0, "count": 100, "totalExtent": 7200,
                "rows": indices.enumerated().map { ["view": 2 + $0.offset, "root": 2 + $0.offset, "epoch": 1, "index": $0.element] },
            ]]]]
            ops += frames.enumerated().map { i, frame in
                ["op": "frame", "id": 2 + i, "x": 10, "y": frame.0, "w": 280, "h": frame.1]
            }
            p.apply(wireBatch(ops))
            let snapshot = try XCTUnwrap(p.listHost.entries[1]?.snapshot)
            let spans = try snapshot.rows.map { row -> (Int, CGRect) in
                let view = try XCTUnwrap(p.views[row.view])
                return (row.index, view.convert(view.bounds, to: scroll))
            }
            let sorted = spans.sorted { $0.1.minY < $1.1.minY }
            _ = p.listHost.covers(1)
            let measured = records.measuredRows, reads = records.spanReads
            for offset in [1000] + Array(stride(from: 0, through: 7200, by: 37)) {
                scroll.contentOffset.y = CGFloat(offset)
                let port = scroll.bounds
                let head = try XCTUnwrap(sorted.first), tail = try XCTUnwrap(sorted.last)
                var reached = head.0 == 0 ? max(port.minY, head.1.minY) : port.minY
                let end = tail.0 == snapshot.count - 1 ? min(port.maxY, tail.1.maxY) : port.maxY
                var covered = reached >= end
                if !covered {
                    for (_, frame) in sorted {
                        if frame.maxY <= reached { continue }
                        if frame.minY > reached + 0.5 { break }
                        reached = frame.maxY
                        if reached >= end { covered = true; break }
                    }
                }
                XCTAssertEqual(p.listHost.covers(1), covered, "offset \(offset), phase \(phase)")
                if phase == 2, offset == 1000 {
                    XCTAssertTrue(covered, "reordered contiguous rows cover the whole port")
                }
                let mean = spans.reduce(CGFloat(0)) { $0 + $1.1.height } / CGFloat(spans.count)
                for ahead in [CGFloat(-200), CGFloat(200)] {
                    var edge = ahead > 0 ? port.minY : port.maxY
                    var last: Int?
                    for (index, frame) in ahead > 0 ? spans : Array(spans.reversed()) {
                        if ahead > 0, frame.minY <= edge + 0.5, frame.maxY >= edge { edge = frame.maxY; last = index }
                        if ahead < 0, frame.maxY >= edge - 0.5, frame.minY <= edge { edge = frame.minY; last = index }
                    }
                    let shortfall = ahead > 0 ? port.maxY + ahead - edge : edge - (port.minY + ahead)
                    let atEnd = ahead > 0 ? last == 99 : last == 0
                    let expected: UInt32 = atEnd || shortfall <= 0 ? 0 : UInt32(min(64, (shortfall / mean).rounded(.up)))
                    XCTAssertEqual(p.listHost.rowsToCover(1, ahead: ahead), expected)
                }
                offsets += 1
            }
            XCTAssertEqual(records.measuredRows, measured, "coverage never remeasures rows for an offset")
            XCTAssertEqual(records.spanReads, reads, "coverage never reads row views for an offset")
        }
        print("M sorted_coverage_offsets=\(offsets) row_measurements=0 span_reads=0")
    }

    func testAReversedFillRetiresFromFreshGeometryAndCurrentPins() throws {
        let (p, window, scroll) = try fixture()
        defer { p.reset(); window.isHidden = true }
        var reports: [Data] = [], fills: [Data] = []
        var inFlight = false
        p.listHost.onFeedback = { reports.append($0) }
        p.listHost.settle()
        p.listHost.onFill = { _, bytes in fills.append(bytes); inFlight = true }
        p.listHost.filling = { inFlight }
        p.listHost.fillSlice(1, limit: 3)
        XCTAssertEqual(fills.count, 1)
        let before = reports.count
        scroll.contentOffset.y = 70
        scroll.contentOffset.y = 30
        p.listHost.pointer(3)
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 10, "y": 20, "w": 280, "h": 90]]))
        XCTAssertEqual(reports.count, before, "all reports wait for the fill")
        let offset = try XCTUnwrap(p.listHost.geometry(1)).offset
        inFlight = false
        p.listHost.landed(1, at: ProcessInfo.processInfo.systemUptime)
        p.listHost.fillSlice(1, limit: 3)
        let report = try XCTUnwrap(reports.last)
        XCTAssertGreaterThan(reports.count, before)
        let word = { (at: Int) in report.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: at, as: UInt32.self) } }
        XCTAssertEqual(word(92) & 6, 4, "retirement stays synchronous and cannot build")
        XCTAssertEqual(word(76), 3, "the row under the current contact stays pinned")
        let sentOffset = report.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 24, as: Double.self) }
        XCTAssertEqual(sentOffset, offset)
        XCTAssertEqual(p.listHost.entries[1]?.lastFacts?.measurements.first?.size, 90)
    }

    func testCachedFixedCoverageKeepsGapsAndRefreshesWhenFixedSizesChange() throws {
        let indices = [0, 1, 2, 3, 99]
        let (p, window, scroll) = try fixture(indices)
        defer { p.reset(); window.isHidden = true }
        p.apply(wireBatch([contentOp(1, 300, 200, padding: [20, 0, 0, 0])]))
        let records = try XCTUnwrap(p.listHost.entries[1]?.records)
        let phases = [(180, 240, indices), (80, 40, indices), (600, 800, []), (900, 500, [])]
        for (revision, phase) in phases.enumerated() {
            let (lead, trail, mounted) = phase
            let extent = lead + 7200 + trail
            let rows = mounted.enumerated().map { offset, index in
                ["view": 2 + offset, "root": 2 + offset, "epoch": 1, "index": index]
            }
            var ops: [[String: Any]] = [["op": "collections", "items": [[
                "view": 1, "revision": revision + 2, "scrollSequence": 0, "count": 100,
                "totalExtent": extent, "rows": rows, "fixed": [
                    ["view": 10, "roots": [], "epoch": 1, "size": lead],
                    ["view": 11, "roots": [], "epoch": 1, "size": trail],
                ]]]]]
            for (offset, index) in mounted.enumerated() {
                ops.append(["op": "frame", "id": 2 + offset, "x": 10, "y": 20 + lead + 72 * index, "w": 280, "h": 72])
            }
            ops.append(["op": "content", "id": 1, "w": 300, "h": 20 + extent])
            p.apply(wireBatch(ops))
            _ = p.listHost.covers(1)
            let measured = records.measuredRows, reads = records.spanReads
            var spans = mounted.map { index in CGFloat(20 + lead + 72 * index)...CGFloat(20 + lead + 72 * (index + 1)) }
            spans += [CGFloat(20)...CGFloat(20 + lead), CGFloat(20 + extent - trail)...CGFloat(20 + extent)]
            spans.sort { $0.lowerBound < $1.lowerBound }
            for offset in stride(from: 0, through: 20 + extent, by: 37) {
                scroll.contentOffset.y = CGFloat(offset)
                var reached = max(scroll.bounds.minY, 20)
                let end = min(scroll.bounds.maxY, CGFloat(20 + extent))
                var covered = reached >= end
                if !covered {
                    for span in spans {
                        if span.upperBound <= reached { continue }
                        if span.lowerBound > reached + 0.5 { break }
                        reached = span.upperBound
                        if reached >= end { covered = true; break }
                    }
                }
                XCTAssertEqual(p.listHost.covers(1), covered, "phase \(revision), offset \(offset)")
            }
            XCTAssertEqual(records.measuredRows, measured, "fixed coverage never remeasures rows for an offset")
            XCTAssertEqual(records.spanReads, reads, "fixed coverage never reads row views for an offset")
        }
        scroll.contentOffset.y = 750
        XCTAssertFalse(p.listHost.covers(1))
        let padding = wireBatch([contentOp(1, 300, 200, padding: [80, 0, 0, 0])])
        p.listHost.beginBatch(padding)
        XCTAssertFalse(p.listHost.covers(1), "a read during application still sees the old padding")
        p.apply(padding)
        p.listHost.endBatch()
        XCTAssertTrue(p.listHost.covers(1), "padding moves fixed spans even with no mounted data rows")
    }

    // Baseline coverage: callback rationing predates this lane.
    func testCoveredScrollDefersItsReportAndAGapRescuesOnlyWhatShows() throws {
        let (p, window, scroll) = try fixture()
        defer { p.reset(); window.isHidden = true }
        var reports: [Data] = []
        p.listHost.onFeedback = { reports.append($0) }
        let start = CACurrentMediaTime()
        for i in 1...1000 { scroll.contentOffset.y = CGFloat(1 + i % 90) }
        let milliseconds = (CACurrentMediaTime() - start) * 1000
        XCTAssertTrue(p.listHost.covers(1))
        XCTAssertTrue(reports.isEmpty, "covered UIKit callbacks return before a row report")
        XCTAssertTrue(p.listHost.fillPending.contains(1))
        p.listHost.fillSlice(1, limit: 5)
        func limit(_ data: Data) -> UInt32 { data.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 88, as: UInt32.self) } }
        XCTAssertEqual(reports.count, 1)
        XCTAssertTrue(p.listHost.observingTurns)
        p.scrollPump.reset()
        XCTAssertTrue(p.scrollPump.observingTurns, "the next native callback reads this turn's elapsed time")
        XCTAssertTrue(p.listHost.observingTurns, "later native input needs a fresh report budget before continuations")
        XCTAssertEqual(limit(try XCTUnwrap(reports.last)), 5)
        scroll.contentOffset.y = 200
        XCTAssertFalse(p.listHost.covers(1))
        XCTAssertEqual(reports.count, 2, "a visible gap is rescued in its UIKit callback")
        p.listHost.reset()
        XCTAssertTrue(p.listHost.observingTurns)
        XCTAssertEqual(limit(try XCTUnwrap(reports.last)), 2, "visible rows plus two, rather than a whole window")
        print("D3 scroll callbacks=1000 covered_reports=0 ms=\(milliseconds) slice_limit=5 rescue_limit=2")
    }
    func testCoveredScrollPublishesItsDeferredReportThroughTheScheduledPump() throws {
        let (p, window, scroll) = try fixture()
        defer { p.reset(); window.isHidden = true }
        p.listHost.onFeedback = { _ in }
        p.listHost.flushIfNeeded()
        let ready = expectation(description: "initial collection reports drained")
        DispatchQueue.main.async { ready.fulfill() }
        wait(for: [ready], timeout: 5)
        let reported = expectation(description: "covered scroll's pump report")
        var inScrollCallback = true
        var reports: [Data] = []
        p.listHost.onFeedback = {
            XCTAssertFalse(inScrollCallback, "row reports run after UIKit's offset callback")
            reports.append($0)
            if reports.count == 1 { reported.fulfill() }
        }
        let started = CACurrentMediaTime()
        scroll.contentOffset.y = 40
        XCTAssertTrue(p.listHost.covers(1))
        XCTAssertTrue(p.listHost.fillPending.contains(1))
        XCTAssertTrue(reports.isEmpty)
        inScrollCallback = false
        wait(for: [reported], timeout: 5)
        XCTAssertEqual(reports.count, 1)
        XCTAssertFalse(p.listHost.fillPending.contains(1), "the scheduled pump services the queued fill")
        let limit = try XCTUnwrap(reports.first).withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 88, as: UInt32.self) }
        XCTAssertGreaterThan(limit, 0)
        XCTAssertLessThan(limit, UInt32.max - 1, "the pump rations its slice")
        print("D3 scheduled scroll reports=\(reports.count) limit=\(limit) elapsed_ms=\((CACurrentMediaTime() - started) * 1000)")
    }

    /// Steps of a port or more each frame (a scroll indicator dragged)
    /// show rows no frame before showed: past the first, a jump's, each
    /// rescue is a glimpse, its rows bounded.
    func testCrossingAPortEachFrameRescuesAGlimpse() throws {
        let (p, window, scroll) = try fixture()
        defer { p.reset(); window.isHidden = true }
        var reports: [Data] = []
        p.listHost.onFeedback = { reports.append($0) }
        var time = CACurrentMediaTime()
        p.scrollPump.clock = { time }
        func word(_ data: Data, _ at: Int) -> UInt32 { data.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: at, as: UInt32.self) } }
        scroll.contentOffset.y = 10
        reports.removeAll()
        time += 1.0 / 60
        scroll.contentOffset.y = 900
        XCTAssertEqual(word(try XCTUnwrap(reports.last), 92) & 16, 0, "a lone jump builds what shows")
        time += 1.0 / 60
        scroll.contentOffset.y = 1800
        let glimpse = try XCTUnwrap(reports.last)
        XCTAssertEqual(word(glimpse, 92) & 16, 16, "the next step across a port is a glimpse")
        XCTAssertEqual(word(glimpse, 88), 2, "two rows before a glimpse's cost is measured")
        XCTAssertTrue(p.listHost.glimpses.contains(1))
    }

    func testSeparatedPortCrossingsRescueTheWholeViewport() throws {
        let previous = ExactSession.asyncFills
        ExactSession.asyncFills = false
        defer { ExactSession.asyncFills = previous }
        let (session, window, node, scroll) = try listFixture()
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter
        var time = CACurrentMediaTime()
        p.scrollPump.clock = { time }
        node.scrollEnd = ScrollEnd(); node.scrollEnd?.live = true
        var reports: [Data] = []
        let feedback = p.listHost.onFeedback
        p.listHost.onFeedback = { reports.append($0); feedback?($0) }
        scroll.contentOffset.y = 10
        for offset in [1050, 2050] {
            time += 0.05
            reports.removeAll()
            scroll.contentOffset.y = CGFloat(offset)
            let report = try XCTUnwrap(reports.first)
            XCTAssertEqual(report.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 92, as: UInt32.self) } & 16, 0,
                "a step from three frames ago cannot turn this jump into a glimpse")
            XCTAssertFalse(p.listHost.glimpses.contains(node.id))
            XCTAssertTrue(p.listHost.covers(node.id), "all four partly visible rows are built in the callback")
        }
    }

    func testScrollReportsOnlyNewRowMeasurements() throws {
        let (p, window, scroll) = try fixture()
        defer { p.reset(); window.isHidden = true }
        var reports: [Data] = []
        p.listHost.onFeedback = { reports.append($0) }
        func count(_ data: Data) -> UInt32 {
            data.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 96, as: UInt32.self) }
        }
        p.listHost.flushIfNeeded()
        XCTAssertEqual(count(try XCTUnwrap(reports.last)), 4)
        p.apply(wireBatch([snapshot(2, measured: true)]))
        p.listHost.settle()
        reports.removeAll()
        let started = CACurrentMediaTime()
        for y in [30, 40, 50] {
            scroll.contentOffset.y = CGFloat(y)
            p.listHost.fillSlice(1, limit: 0)
        }
        let sizes = reports.map(count)
        print("QUEUE 675 scroll reports=\(sizes.count) measurements=\(sizes) bytes=\(reports.reduce(0) { $0 + $1.count }) ms=\((CACurrentMediaTime() - started) * 1000)")
        XCTAssertEqual(sizes, [0, 0, 0])
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 10, "y": 20, "w": 280, "h": 73]]))
        p.listHost.settle()
        XCTAssertEqual(count(try XCTUnwrap(reports.last)), 1, "a changed height is sent")
        p.apply(wireBatch([snapshot(3, measured: false)]))
        p.listHost.settle()
        // Invalidation withdraws the acknowledgement, even if a row's
        // frame and epoch have not changed.
        p.listHost.fillSlice(1, limit: 1)
        XCTAssertEqual(count(try XCTUnwrap(reports.last)), 4)
    }

    func testARejectedPinDoesNotAcknowledgeItsRowMeasurements() throws {
        let (p, window, _) = try fixture()
        defer { p.reset(); window.isHidden = true }
        p.apply(wireBatch([snapshot(1, measured: true)]))
        var storedHeight = 72.0, rejected = 0
        p.listHost.onFeedback = { data in
            data.withUnsafeBytes { bytes in
                let pin = UInt32(littleEndian: bytes.loadUnaligned(fromByteOffset: 76, as: UInt32.self))
                // The runner rejects the whole report for an unpublished pin.
                if pin == 99 { rejected += 1; return }
                let count = UInt32(littleEndian: bytes.loadUnaligned(fromByteOffset: 96, as: UInt32.self))
                for i in 0..<Int(count) {
                    let at = 100 + i * 20
                    let id = UInt32(littleEndian: bytes.loadUnaligned(fromByteOffset: at, as: UInt32.self))
                    if id == 2 {
                        storedHeight = Double(bitPattern: UInt64(littleEndian: bytes.loadUnaligned(fromByteOffset: at + 12, as: UInt64.self)))
                    }
                }
            }
        }
        p.listHost.flushIfNeeded()
        // Model a host view the runner does not own, inside a mounted row.
        p.apply(wireBatch([
            ["op": "create", "id": 99, "kind": "view", "handlers": ["press"]],
            ["op": "children", "id": 2, "ids": [99]],
            ["op": "frame", "id": 2, "x": 10, "y": 20, "w": 280, "h": 73],
        ]))
        p.listHost.pointer(99)
        p.listHost.settle()
        XCTAssertEqual(rejected, 1)
        XCTAssertEqual(storedHeight, 72)
        p.listHost.pointer(nil)
        p.listHost.settle()
        XCTAssertEqual(storedHeight, 73, "the valid report retries the rejected height at the same revision")
        XCTAssertEqual(p.listHost.entries[1]?.snapshot.revision, 1)
    }


    // @ref LLP 1010 §6.11 — a kept end row past a gap of unbuilt rows does
    // not end the travel: the rows the port will uncover are still owed.
    func testAKeptEndRowPastAGapStillLeadsTheSlice() throws {
        do {
            let (p, window, scroll) = try fixture([0, 1, 2, 3, 99])
            defer { p.reset(); window.isHidden = true }
            // The port reaches 260 of the run's 308; 200 points more need
            // ceil(152 / 72) = 3 rows, the kept last row notwithstanding.
            scroll.contentOffset.y = 60
            XCTAssertEqual(p.listHost.rowsToCover(1, ahead: 200), 3)
            XCTAssertEqual(p.listHost.rowsToCover(1, ahead: -200), 0, "the first item is built and joined")
        }
        do {
            let (p, window, scroll) = try fixture([0, 50, 51, 52, 53])
            defer { p.reset(); window.isHidden = true }
            // Rows 50–53 span 3620–3908; from 3680 back 200 points needs
            // ceil(140 / 72) = 2 rows, the kept first row notwithstanding.
            scroll.contentOffset.y = 3680
            XCTAssertEqual(p.listHost.rowsToCover(1, ahead: -200), 2)
        }
    }
}
#endif
