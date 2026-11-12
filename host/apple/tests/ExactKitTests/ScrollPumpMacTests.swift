#if os(macOS)
import AppKit
import IOSurface
import XCTest
@testable import ExactKit

final class ScrollPumpMacTests: XCTestCase {
    private var session: ExactSession!
    private var window: NSWindow!
    private var gates: [BlockOperation] = []

    private func apply(_ p: Presenter, _ ops: [[String: Any]]) {
        p.apply(batchFixture(ops: ops, timers: false, motion: false, clock: nil, error: nil))
    }

    private func fixture(_ ids: [Int]) -> Presenter {
        _ = NSApplication.shared
        session = ExactApp.shared.makeSession(label: "pump-slots-mac")
        let p = session.presenter
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 500, height: 400),
                          styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        RegionTextExecutor.queue.isSuspended = true
        let paragraphs: [[String: Any]] = ids.flatMap { id in [
            ["op": "create", "id": id, "kind": "text", "props": ["text": "Paragraph \(id) below the port"]],
            ["op": "frame", "id": id, "x": 0.0, "y": 500.0 + Double(id - ids[0]) * 60, "w": 400.0, "h": 42.0],
        ] }
        apply(p, [["op": "create", "id": 1, "kind": "view"]] + paragraphs + [
            ["op": "children", "id": 1, "ids": ids], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 500.0, "h": 2000.0],
        ])
        return p
    }

    override func tearDown() {
        session?.presenter.reset()
        for gate in gates where !gate.isFinished { gate.start() }
        gates.removeAll()
        RegionTextExecutor.queue.isSuspended = false
        RegionTextExecutor.queue.waitUntilAllOperationsAreFinished()
        waitForMainQueue()
        window?.close()
        session?.destroy()
        window = nil
        session = nil
        super.tearDown()
    }

    func testTextWaitingForWorkersStopsItsDisplayLink() throws {
        let ids = Array(2..<12)
        let p = fixture(ids)
        XCTAssertTrue(p.asksForFrames)
        p.pump()
        p.pump()
        XCTAssertEqual(ids.filter { p.views[UInt32($0)]?.textRasterKey != nil }.count, 2, "both worker slots are occupied")
        XCTAssertFalse(p.asksForFrames, "no frame can admit text while every worker is busy")
    }

    private enum Result { case offscreen, visible, stale, removed }

    private func checkCompletion(_ result: Result) throws {
        let p = fixture([2, 3, 4])
        let first = try XCTUnwrap(p.views[2]), second = try XCTUnwrap(p.views[3])
        XCTAssertTrue(p.textRasters.ensure(first, urgent: false))
        let job = try XCTUnwrap(RegionTextExecutor.queue.operations.first)
        XCTAssertTrue(p.textRasters.ensure(second, urgent: false))
        for operation in RegionTextExecutor.queue.operations {
            let gate = BlockOperation {}
            operation.addDependency(gate)
            gates.append(gate)
        }
        let release = try XCTUnwrap(job.dependencies.first as? BlockOperation)
        p.pump()
        XCTAssertFalse(p.asksForFrames)
        switch result {
        case .offscreen: break
        case .visible: first.frame.origin.y = 80
        case .stale:
            apply(p, [["op": "props", "id": 2, "set": ["text": "New words supersede the worker's pixels"]]])
            p.pump()
        case .removed:
            apply(p, [["op": "children", "id": 1, "ids": [3, 4]], ["op": "destroy", "id": 2]])
            p.pump()
        }
        var resumed = false
        let resume = p.textRasters.slotFreed
        defer { p.textRasters.slotFreed = resume }
        p.textRasters.slotFreed = {
            resume?()
            resumed = p.asksForFrames
        }
        RegionTextExecutor.queue.isSuspended = false
        release.start()
        waitFor("a freed slot resumes text even while the other worker is held") { resumed }
        RegionTextExecutor.queue.isSuspended = true
        XCTAssertFalse(second.textRasterReady, "the other worker has not completed")
        if result == .stale || result == .removed {
            XCTAssertNil(first.textRaster, "discarded pixels cannot publish")
        } else {
            XCTAssertTrue(first.textRasterReady)
            XCTAssertEqual(first.textRasterPending, result == .offscreen)
        }
        p.pump()
        let next = try XCTUnwrap(result == .stale ? p.views[2] : p.views[4])
        XCTAssertNotNil(next.textRasterKey, "the next owed paragraph takes the freed slot")
        if result == .offscreen {
            XCTAssertFalse(first.textRasterPending, "the same pass publishes ready offscreen pixels")
            XCTAssertNotNil(first.layer?.contents)
        }
    }

    func testOffscreenCompletionResumesText() throws { try checkCompletion(.offscreen) }
    func testVisibleCompletionResumesText() throws { try checkCompletion(.visible) }
    func testStaleCompletionResumesText() throws { try checkCompletion(.stale) }
    func testRemovedCompletionResumesText() throws { try checkCompletion(.removed) }

    func testSupersededVisibleReplacementResumesOtherText() throws {
        let p = fixture([2, 3, 4])
        for id: UInt32 in [2, 3] {
            let node = try XCTUnwrap(p.views[id])
            XCTAssertTrue(p.textRasters.ensure(node, urgent: true))
            node.frame.origin.y = CGFloat(id) * 60
        }
        apply(p, [2, 3].map { ["op": "props", "id": $0, "set": ["text": "Replacement for paragraph \($0)"]] })
        p.pump()
        XCTAssertEqual(RegionTextExecutor.queue.operationCount, 2)
        p.pump()
        XCTAssertFalse(p.asksForFrames, "a replacement waits for its workers without frames")
        apply(p, [["op": "children", "id": 1, "ids": [3, 4]], ["op": "destroy", "id": 2]])
        p.pump()
        XCTAssertFalse(p.asksForFrames)
        var resumed = false
        let resume = p.textRasters.slotFreed
        defer { p.textRasters.slotFreed = resume }
        p.textRasters.slotFreed = { resume?(); resumed = p.asksForFrames }
        RegionTextExecutor.queue.isSuspended = false
        waitFor("replacement completion resumes text after its first paragraph leaves") { resumed }
        RegionTextExecutor.queue.isSuspended = true
        p.pump()
        XCTAssertNotNil(p.views[4]?.textRasterKey)
        XCTAssertTrue(p.views[3]?.textRasterReady == true)
    }

    func testReadyPixelsPublishWhileWorkersAreFull() throws {
        let ids = Array(2..<12)
        let p = fixture(ids)
        p.pump()
        XCTAssertFalse(p.asksForFrames)
        let pixels = try XCTUnwrap(IOSurface(properties: [.width: 400, .height: 42, .bytesPerElement: 4]))
        for id in ids.dropFirst(2) {
            let node = try XCTUnwrap(p.views[UInt32(id)])
            let key = TextRasterKey(spec: node.paragraphSpec(), size: node.bounds.size,
                                    box: node.paragraphBox(), scale: node.pixelScale ?? 2)
            node.textRasterKey = key
            node.showTextRaster(pixels, for: key, deferOffscreen: true)
        }
        p.pump()
        XCTAssertFalse(p.asksForFrames)
        for id in ids.dropFirst(2) {
            let node = try XCTUnwrap(p.views[UInt32(id)])
            XCTAssertFalse(node.textRasterPending)
            XCTAssertTrue(node.layer?.contents as? IOSurface === pixels)
        }
    }

    func testBackingStoreBandsKeepTheirFramesWhileWorkersAreFull() throws {
        let p = fixture([2, 3, 4])
        p.pump()
        let ids = [10, 11, 12]
        let paragraphs: [[String: Any]] = ids.flatMap { id in [
            ["op": "create", "id": id, "kind": "text", "props": ["text": "Tall paragraph \(id)"]],
            ["op": "frame", "id": id, "x": 0.0, "y": 450.0, "w": 400.0, "h": 1200.0],
        ] }
        apply(p, paragraphs + [["op": "children", "id": 1, "ids": [2, 3, 4] + ids]])
        p.pump()
        XCTAssertTrue(p.asksForFrames, "backing-store bands still have frame work")
        p.pump()
        XCTAssertFalse(p.asksForFrames)
        for id in ids {
            let node = try XCTUnwrap(p.views[UInt32(id)])
            XCTAssertFalse(node.rastersText)
            XCTAssertFalse(p.textVisibleRect(node).isEmpty)
        }
    }

    func testCollectionWorkKeepsItsFramesWhileWorkersAreFull() throws {
        let p = fixture([2, 3, 4])
        p.pump()
        p.collections.filling = { true }
        apply(p, [
            ["op": "create", "id": 10, "kind": "list", "style": ["overflow_y": "scroll"]],
            ["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 400.0, "h": 200.0],
            ["op": "children", "id": 1, "ids": [2, 3, 4, 10]],
            ["op": "collections", "items": [["view": 10, "revision": 1, "scrollSequence": 0,
                                            "count": 100, "totalExtent": 3000, "rows": [], "pending": true]]],
        ])
        p.pump()
        XCTAssertEqual(p.collections.fillPending, [10])
        XCTAssertTrue(p.asksForFrames)
        p.collections.filling = { false }
        p.pump()
        XCTAssertTrue(p.collections.fillPending.isEmpty)
        XCTAssertFalse(p.asksForFrames)
    }
}
#endif
