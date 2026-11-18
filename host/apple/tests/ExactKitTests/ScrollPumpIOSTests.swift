#if os(iOS)
import UIKit
import XCTest
import ExactSurfaces
@testable import ExactKit
@testable import ExactImages
@testable import ExactCollections

/// A scroll asks the pump for work only when work is owed: a paragraph
/// without all its pixels, or a list's rows. A screen whose text is all
/// painted scrolls with no pass queued and no display link.
final class ScrollPumpIOSTests: XCTestCase {
    private var window: UIWindow!

    /// A 400-point square window showing `p`'s viewport.
    private func mount(_ p: Presenter) {
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
    }

    /// Paragraphs below the port and within a port of it, 18 points apart,
    /// which workers prepare: owed while none has its pixels.
    private func belowThePort(_ ids: [Int]) -> [[String: Any]] {
        ids.flatMap { id in [
            ["op": "create", "id": id, "kind": "text", "props": ["text": "Paragraph \(id) below the port"], "style": ["font_size": 16.0]],
            ["op": "frame", "id": id, "x": 0.0, "y": 420.0 + Double(id - ids[0]) * 18, "w": 300.0, "h": 18.0],
        ] as [[String: Any]] }
    }

    private func fixture(_ label: String) -> ExactSession {
        let session = ExactApp.shared.makeSession(label: label)
        let p = session.presenter
        mount(p)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "text",
             "props": ["text": "Sign in with your account"], "style": ["font_size": 16.0]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 100.0, "w": 300.0, "h": 40.0],
        ]))
        window.layoutIfNeeded(); p.viewport.layer.displayIfNeeded()
        p.views[2]?.layer.displayIfNeeded()
        drain(p)
        p.scrollPump.reset()
        return session
    }
    private func drain(_ p: Presenter) {
        RegionTextExecutor.queue.waitUntilAllOperationsAreFinished()
        let end = Date().addingTimeInterval(2)
        while p.textRasters.inFlight > 0 && Date() < end { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01)) }
    }

    private func addLeadText(_ p: Presenter, ids: [Int]) {
        p.apply(wireBatch(belowThePort(ids) + [
            ["op": "children", "id": 1, "ids": [2] + ids],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0],
        ]))
        window.layoutIfNeeded()
        for id in ids { p.views[UInt32(id)]?.layer.displayIfNeeded() }
    }

    func testUntracedScrollFramesCreateNoSignpostIntervals() throws {
        guard !Presenter.signposts.isEnabled else { throw XCTSkip("Instruments enabled scroll tracing") }
        let session = fixture("pump-no-signpost-state")
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter, node = try XCTUnwrap(p.views[1])
        node.applyStyle(["overflow_y": .string("scroll")])
        let scroll = try XCTUnwrap(node.scrollView)
        let intervals = p.scrollSignpostIntervals
        for _ in 0..<120 { node.scrollViewDidScroll(scroll) }
        XCTAssertEqual(p.scrollSignpostIntervals - intervals, 0)
        print("J untraced scroll_frames=120 signpost_intervals=\(p.scrollSignpostIntervals - intervals)")
    }

    func testAPaintedScrollBuildsNoTextSurfacesAndWalksNoTextIndex() throws {
        let session = fixture("pump-no-text-work")
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter
        XCTAssertTrue(try XCTUnwrap(p.views[2]).textRasterSettled)
        p.apply(wireBatch(belowThePort([3])))
        p.apply(wireBatch([
            ["op": "create", "id": 4, "kind": "text", "props": ["text": ""]],
            ["op": "children", "id": 1, "ids": [2, 4]],
        ]))
        XCTAssertNil(p.views[3]?.window, "a pooled or detached paragraph stays in the text table")
        p.scrollPump.reset()
        XCTAssertTrue(p.owedText.isEmpty)
        let surfaces = p.textSurfaceBuilds, visits = p.textIndex.visits
        for _ in 0..<120 { p.scrollPump.scrolled(nil) }
        XCTAssertEqual(p.textSurfaceBuilds - surfaces, 0)
        XCTAssertEqual(p.textIndex.visits - visits, 0)
        XCTAssertFalse(p.scrollPump.asksForFrames)
        XCTAssertTrue(p.scrollPump.observingTurns)
        print("J quiet frames=120 text_surfaces=\(p.textSurfaceBuilds - surfaces) text_index_walks=\(p.textIndex.visits - visits)")
    }

    func testAnUnpaintedParagraphEnteringTheWindowPaintsInThatScrollFrame() throws {
        let session = fixture("pump-attached-text")
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter
        p.apply(wireBatch(belowThePort([3])))
        let node = try XCTUnwrap(p.views[3])
        node.frame.origin.y = 160
        XCTAssertFalse(node.textRasterReady)
        XCTAssertFalse(p.owedText.contains(3))
        p.root.addSubview(node)
        XCTAssertTrue(p.owedText.contains(3))
        XCTAssertFalse(node.textRasterReady)
        p.scrollPump.scrolled(nil)
        XCTAssertTrue(node.textRasterReady, "no run-loop turn between attachment, scroll and the pixels")
        XCTAssertTrue(node.textRasterFrame.contains(p.textScrollportRect(node)))
        node.removeFromSuperview()
        node.dropTextRaster()
        XCTAssertFalse(p.owedText.contains(3))
        p.root.addSubview(node)
        XCTAssertTrue(p.owedText.contains(3))
        p.scrollPump.scrolled(nil)
        XCTAssertTrue(node.textRasterReady)
        p.scrollPump.settle()
        XCTAssertTrue(p.scrollPump.observingTurns)
    }

    func testFallbackTextSkipsTheRasterPassAndReturnsWhenItCanRasterAgain() throws {
        let session = fixture("pump-fallback-text")
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter, node = try XCTUnwrap(p.views[2])
        var style = node.style
        style["background_clip"] = .string("text")
        style["background_color"] = .array([.number(255), .number(0), .number(0), .number(255)])
        node.applyStyle(style)
        node.layer.displayIfNeeded()
        XCTAssertFalse(node.canRasterText)
        XCTAssertNil(node.textRaster)
        XCTAssertFalse(node.textRasterSettled)
        XCTAssertNotNil(node.layer.contents, "UIKit's backing store holds the fallback glyphs")
        let bitmap = try XCTUnwrap(Bitmap.blank(width: Int(node.bounds.width), height: Int(node.bounds.height)))
        let bytes = try XCTUnwrap(bitmap.bytes)
        func pixels() -> [UInt8] {
            bitmap.context.clear(node.bounds)
            node.layer.render(in: bitmap.context)
            return Array(UnsafeBufferPointer(start: bytes.assumingMemoryBound(to: UInt8.self), count: bitmap.bytesPerRow * bitmap.height))
        }
        let before = pixels()
        XCTAssertGreaterThan(stride(from: 3, to: before.count, by: 4).filter { before[$0] > 16 }.count, 50)
        p.scrollPump.reset()
        let surfaces = p.textSurfaceBuilds, visits = p.textIndex.visits
        for _ in 0..<120 { p.scrollPump.scrolled(nil) }
        XCTAssertEqual(p.textSurfaceBuilds - surfaces, 0)
        XCTAssertEqual(p.textIndex.visits - visits, 0)
        XCTAssertFalse(p.scrollPump.asksForFrames)
        XCTAssertEqual(pixels(), before, "the scroll keeps the fallback glyphs")
        print("J fallback frames=120 text_surfaces=\(p.textSurfaceBuilds - surfaces) text_index_walks=\(p.textIndex.visits - visits)")
        node.applyStyle(["font_size": .number(16)])
        XCTAssertTrue(node.canRasterText)
        XCTAssertTrue(p.owedText.contains(node.id), "eligibility changes without a raster-ready transition")
        p.scrollPump.scrolled(nil)
        XCTAssertTrue(node.textRasterReady, "the first eligible scroll frame paints the paragraph")
    }

    func testFillTurnsReuseOneBlockAndAResetWithdrawsTheOldTurn() {
        let session = fixture("pump-stored-block")
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter, pump = p.scrollPump
        let made = pump.afterScrollBlocks
        for _ in 0..<10 {
            pump.requestFill()
            waitForMainQueue("the scheduled pump")
            pump.settle()
            XCTAssertFalse(pump.asksForFrames, "the stored block executes on every turn")
        }
        XCTAssertEqual(pump.afterScrollBlocks - made, 1)
        pump.requestFill()
        pump.reset()
        pump.requestFill()
        waitForMainQueue("the replacement pump")
        pump.settle()
        XCTAssertEqual(pump.afterScrollBlocks - made, 2)
        XCTAssertFalse(pump.asksForFrames)
        XCTAssertTrue(pump.observingTurns)
        print("J fill_turns=10 scheduled_blocks=1")
    }

    func testRasterMembershipFollowsFlowColumnsFailureCaptureAndCanvasAncestry() throws {
        ExactSurfaces.install()
        let session = fixture("pump-raster-eligibility")
        defer { session.destroy(); window.isHidden = true }
        let p = session.presenter, node = try XCTUnwrap(p.views[2])
        node.applyFlow([["kind": "Circle", "cx": 0, "cy": 1000, "r": 10]])
        XCTAssertFalse(node.usesTextRaster)
        XCTAssertFalse(p.owedText.contains(2))
        XCTAssertFalse(p.paintVisibleText())
        node.applyFlow([])
        XCTAssertTrue(p.owedText.contains(2))
        node.applyColumns(["fragments": [[0, 0, 300, 40, 0, 1, 0, 0]]])
        XCTAssertFalse(node.usesTextRaster)
        XCTAssertFalse(p.owedText.contains(2))
        XCTAssertFalse(p.paintVisibleText())
        node.applyColumns([:])
        XCTAssertTrue(p.owedText.contains(2))
        p.paintVisibleText()
        let key = try XCTUnwrap(node.textRasterKey)
        node.dropTextRaster()
        node.textRasterKey = key
        node.textRasterFailed = true
        XCTAssertFalse(p.owedText.contains(2), "allocation fallback draws through UIKit")
        XCTAssertFalse(p.paintVisibleText())
        node.textRasterFailed = false
        XCTAssertTrue(p.owedText.contains(2))
        Capture.capturing = true
        node.setNeedsDisplay(); node.layer.displayIfNeeded()
        Capture.capturing = false
        XCTAssertTrue(p.owedText.contains(2), "a temporary capture cannot withdraw next frame's raster work")
        p.scrollPump.scrolled(nil)
        XCTAssertTrue(node.textRasterReady)
        let canvas = NodeView(id: 20, kind: "canvas", presenter: p)
        let parent = NodeView(id: 21, kind: "view", presenter: p)
        parent.frame = p.viewport.bounds
        p.root.addSubview(canvas)
        p.root.addSubview(parent)
        parent.addSubview(node)
        node.dropTextRaster()
        XCTAssertTrue(p.owedText.contains(2))
        try XCTUnwrap(canvas.overlay).addSubview(parent)
        XCTAssertFalse(p.owedText.contains(2), "moving an ancestor changes how its text paints")
        XCTAssertFalse(p.paintVisibleText())
        p.root.addSubview(parent)
        XCTAssertTrue(p.owedText.contains(2))
        p.scrollPump.scrolled(nil)
        XCTAssertTrue(node.textRasterReady)
    }

    func testScrollerMembershipFollowsReparentingAndANewScrollContainer() throws {
        let p = Presenter()
        mount(p)
        defer { p.reset(); window.isHidden = true }
        let gradient: [String: Any] = ["linear": 180, "stops": [0, 255, 0, 0, 255, 1, 0, 0, 255, 255]]
        p.apply(wireBatch([
            ["op": "create", "id": 10, "kind": "view", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 11, "kind": "view", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 12, "kind": "view"],
            ["op": "create", "id": 13, "kind": "view",
             "style": ["background_image": gradient, "background_attachment": "fixed"]],
            ["op": "children", "id": 12, "ids": [13]],
            ["op": "children", "id": 10, "ids": [12]],
            ["op": "roots", "ids": [10, 11]],
            ["op": "frame", "id": 10, "x": 0, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 11, "x": 200, "y": 0, "w": 200, "h": 200],
            ["op": "frame", "id": 12, "x": 0, "y": 0, "w": 100, "h": 100],
            ["op": "frame", "id": 13, "x": 0, "y": 0, "w": 100, "h": 50],
        ]))
        let a = try XCTUnwrap(p.views[10]?.scroll), b = try XCTUnwrap(p.views[11]?.scroll)
        let parent = try XCTUnwrap(p.views[12]), member = try XCTUnwrap(p.views[13])
        member.layer.displayIfNeeded()
        XCTAssertNotNil(member.boxGradient)
        XCTAssertTrue(p.gradientsMove(in: a))
        XCTAssertFalse(p.gradientsMove(in: b))
        b.addSubview(parent)
        XCTAssertFalse(p.gradientsMove(in: a))
        XCTAssertTrue(p.gradientsMove(in: b))
        parent.applyStyle(["overflow_y": .string("scroll")])
        let inner = try XCTUnwrap(parent.scroll)
        XCTAssertTrue(p.gradientsMove(in: inner), "a container made after its children inherits their membership")
        p.noteFixedGradient(member, fixed: false)
        XCTAssertFalse(p.gradientsMove(in: inner))
        p.noteFixedGradient(member, fixed: true)
        XCTAssertTrue(p.gradientsMove(in: inner))
        member.removeFromSuperview()
        XCTAssertFalse(p.gradientsMove(in: inner))
    }

    func testFreedSlotsRefillBetweenFrameSlicesAndKeepScrollingFrames() throws {
        let session = fixture("pump-completions")
        defer { session.destroy() }
        let p = session.presenter, ids = Array(10..<30)
        RegionTextExecutor.queue.isSuspended = true
        defer { RegionTextExecutor.queue.isSuspended = false }
        addLeadText(p, ids: ids)
        p.refreshVisibleText()
        XCTAssertFalse(p.textRasters.hasRoom)
        let slots = p.textRasters.inFlight
        RegionTextExecutor.queue.isSuspended = false
        RegionTextExecutor.queue.waitUntilAllOperationsAreFinished()
        RegionTextExecutor.queue.isSuspended = true

        // Finished mailboxes still occupy the slots. Run a frame slice, then
        // consume two mailboxes in this turn, before their queued notifications.
        p.scrollPump.reset()
        p.scrollPump.requestText()
        FrameClock.shared.fire(try XCTUnwrap(FrameClock.shared.link))
        XCTAssertFalse(FrameClock.shared.wants(p.scrollPump))
        for offset in 0..<2 {
            let node = try XCTUnwrap(p.views[UInt32(ids[offset])])
            XCTAssertTrue(p.textRasters.ensure(node, urgent: true))
            XCTAssertTrue(node.textRasterSettled)
            XCTAssertEqual(p.textRasters.inFlight, slots, "the freed slot starts another job in the completion's turn")
            XCTAssertNotNil(p.views[UInt32(ids[slots + offset])]?.textRasterKey)
            XCTAssertFalse(FrameClock.shared.wants(p.scrollPump))
        }

        p.scrollPump.scrolled(nil)
        XCTAssertTrue(FrameClock.shared.wants(p.scrollPump))
        XCTAssertTrue(p.textRasters.ensure(try XCTUnwrap(p.views[UInt32(ids[2])]), urgent: true))
        XCTAssertEqual(p.textRasters.inFlight, slots)
        XCTAssertTrue(FrameClock.shared.wants(p.scrollPump), "a completion leaves the scrolling request active")
        RegionTextExecutor.queue.isSuspended = false
        waitFor("the scroll and its text settle") {
            ids.allSatisfy { p.views[UInt32($0)]?.textRasterSettled == true } && !p.scrollPump.asksForFrames
        }
    }

    func testACompletionDuringATextWalkWaitsForItsClipGeometry() throws {
        let session = fixture("pump-text-walk")
        defer { session.destroy() }
        let p = session.presenter
        addLeadText(p, ids: [3])
        drain(p)
        p.scrollPump.reset()
        let node = try XCTUnwrap(p.views[3])
        node.dropTextRaster()
        let clips = TextClips(p.viewport, soon: 0, reach: 0)
        p.textClips = clips
        p.textRasters.slotFreed?()
        p.textRasters.slotFreed?()
        XCTAssertTrue(p.textClips === clips)
        XCTAssertNil(node.textRasterKey, "a completion cannot nest another text walk")
        XCTAssertFalse(FrameClock.shared.wants(p.scrollPump))
        p.textClips = nil
        waitForMainQueue("the deferred text pass")
        XCTAssertTrue(node.textRasterSettled)
    }

    func testACompletionDuringABatchLeavesTextForTheBatchsPass() throws {
        let session = fixture("pump-batch")
        defer { session.destroy() }
        let p = session.presenter
        addLeadText(p, ids: [3])
        drain(p)
        p.scrollPump.reset()
        let node = try XCTUnwrap(p.views[3])
        node.dropTextRaster()
        defer { p.onViewportFit = nil }
        var completed = false
        p.onViewportFit = {
            XCTAssertTrue(p.applying)
            p.textRasters.slotFreed?()
            XCTAssertNil(node.textRasterKey)
            XCTAssertFalse(FrameClock.shared.wants(p.scrollPump))
            completed = true
        }
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["viewportFit": "cover"]]]))
        XCTAssertTrue(completed)
        waitFor("the batch's text pass") { node.textRasterSettled }
    }

    func testAScrollOverPaintedTextAsksForNothing() throws {
        let session = fixture("pump-settled")
        defer { session.destroy() }
        let p = session.presenter
        XCTAssertTrue(try XCTUnwrap(p.views[2]).textRasterSettled)
        for _ in 0..<10 { p.scrollPump.scrolled(nil) }
        XCTAssertFalse(p.scrollPump.asksForFrames, "no pass queued and no display link")
    }

    func testQuietFastTravelLimitsDecodesUntilItSlowsOrEnds() throws {
        let p = Presenter()
        mount(p)
        defer { p.reset(); window.isHidden = true }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll"]],
            ["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0,
                "count": 1000, "totalExtent": 100_000, "pending": false, "rows": []]]],
            ["op": "roots", "ids": [1]], ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 400],
            ["op": "content", "id": 1, "w": 400, "h": 100_000],
        ]))
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scrollView)
        p.scrollPump.reset()
        node.scrollEnd = ScrollEnd(); node.scrollEnd?.live = true
        var time = CACurrentMediaTime()
        p.scrollPump.clock = { time }
        p.scrollPump.sampleScroll(node)
        time += 1.0 / 60
        scroll.contentOffset.y = 360
        XCTAssertGreaterThan(p.scrollPump.velocity(1), ScrollPump.fastTravel)
        XCTAssertFalse(p.scrollPump.asksForFrames)
        XCTAssertTrue(RasterWorkers.shared.single, "quiet frames still keep the second decoder idle")
        p.scrollPump.pump()
        XCTAssertFalse(p.scrollPump.asksForFrames)
        XCTAssertTrue(RasterWorkers.shared.single, "giving up the display link does not end travel")
        let fills = p.scrollPump.fillPasses, flushes = p.listHost.flushes
        time += 0.16
        XCTAssertEqual(p.scrollPump.velocity(1), 0)
        waitFor("the fast sample expires while the finger stays still") { !RasterWorkers.shared.single }
        XCTAssertFalse(p.scrollPump.asksForFrames)
        XCTAssertEqual(p.scrollPump.fillPasses, fills, "expiry needs no pump")
        XCTAssertEqual(p.listHost.flushes, flushes, "expiry sends no report")
        for _ in 0..<6 {
            time += 1.0 / 60
            scroll.contentOffset.y += 360
        }
        XCTAssertTrue(RasterWorkers.shared.single, "fresh travel limits decodes again")
        time += 1.0 / 60
        scroll.contentOffset.y += 1
        XCTAssertLessThan(p.scrollPump.velocity(1), ScrollPump.fastTravel)
        XCTAssertFalse(RasterWorkers.shared.single)
        for _ in 0..<3 {
            time += 1.0 / 60
            scroll.contentOffset.y += 360
        }
        XCTAssertGreaterThan(p.scrollPump.velocity(1), ScrollPump.fastTravel)
        XCTAssertTrue(RasterWorkers.shared.single)
        p.listHost.rested(1)
        XCTAssertFalse(RasterWorkers.shared.single, "rest clears the latch without another pump")
        time += 1.0 / 60
        scroll.contentOffset.y += 360
        time += 1.0 / 60
        scroll.contentOffset.y += 360
        XCTAssertTrue(RasterWorkers.shared.single)
        p.scrollPump.reset()
        XCTAssertFalse(RasterWorkers.shared.single)
    }

    func testAScrollOverTextWithoutItsPixelsAsksForAPass() throws {
        let session = fixture("pump-owed")
        defer { session.destroy() }
        let p = session.presenter
        p.views[2]?.dropTextRaster()
        p.scrollPump.scrolled(nil)
        XCTAssertTrue(p.scrollPump.asksForFrames, "the pass that gives it its pixels")
    }

    /// Owed work takes frames from the app's one link, asked once for the
    /// scroll: not a link made and torn down per frame.
    func testOwedWorkTakesTheAppsLinkOnce() throws {
        let session = fixture("pump-clock")
        defer { session.destroy() }
        let p = session.presenter
        RegionTextExecutor.queue.isSuspended = true
        defer { RegionTextExecutor.queue.isSuspended = false }
        // Owed at every scroll while its worker cannot run, where a
        // paragraph painted before would take its kept pixels at the first
        p.apply(wireBatch(belowThePort([3]) + [
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0]]))
        window.layoutIfNeeded(); p.views[3]?.layer.displayIfNeeded()
        var made: Set<Int> = []
        for _ in 0..<20 {
            p.scrollPump.scrolled(nil)
            XCTAssertTrue(FrameClock.shared.wants(p.scrollPump))
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.005))
            made.insert(FrameClock.shared.linksMade)
        }
        XCTAssertEqual(made.count, 1, "the app's one link")
    }

    /// At rest, text that waits only for a worker's slot asks for no
    /// frames: each slot that frees asks for the next pass, a dropped
    /// job's too.
    func testTextWaitingForAWorkersSlotAsksForNoFrames() throws {
        let session = ExactApp.shared.makeSession(label: "pump-slots")
        defer { session.destroy() }
        let p = session.presenter
        mount(p)
        // More paragraphs than there are slots
        let ids = Array(10..<30)
        RegionTextExecutor.queue.isSuspended = true
        defer { RegionTextExecutor.queue.isSuspended = false }
        p.apply(wireBatch([["op": "create", "id": 1, "kind": "view"]] + belowThePort(ids) + [
            ["op": "children", "id": 1, "ids": ids], ["op": "roots", "ids": [1]], ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0]]))
        window.layoutIfNeeded(); p.viewport.layer.displayIfNeeded()
        for id in ids { p.views[UInt32(id)]?.layer.displayIfNeeded() }
        p.scrollPump.reset()
        p.scrollPump.requestText()
        XCTAssertTrue(p.scrollPump.asksForFrames)
        waitFor("the pump gives up its frames") { !p.scrollPump.asksForFrames }
        XCTAssertFalse(p.textRasters.hasRoom, "every worker's slot is taken")
        // New words for every paragraph: the next pass drops the jobs in the
        // slots unstarted, and only their freed slots can ask for the passes
        // that give the paragraphs workers again
        p.apply(wireBatch(ids.map { ["op": "props", "id": $0, "set": ["text": "New words for paragraph \($0)"]] }))
        waitFor("the pump gives up its frames again") { !p.scrollPump.asksForFrames }
        XCTAssertFalse(p.textRasters.hasRoom, "every slot is still taken")
        let settled = { ids.allSatisfy { p.views[UInt32($0)]?.textRasterSettled == true } }
        XCTAssertFalse(settled())
        let slotFreed = p.textRasters.slotFreed
        defer { p.textRasters.slotFreed = slotFreed }
        var completions = 0, completionRequestedFrames = false
        p.textRasters.slotFreed = {
            slotFreed?()
            completions += 1
            completionRequestedFrames = completionRequestedFrames || FrameClock.shared.wants(p.scrollPump)
        }
        RegionTextExecutor.queue.isSuspended = false
        // Stuck: no worker in flight and no pass to come, with paragraphs still owed
        let stuck = { p.textRasters.inFlight == 0 && !p.scrollPump.asksForFrames && !settled() }
        waitFor("each freed slot asks for the pass that gives the next paragraph a worker", giveUp: stuck) {
            settled() && !p.scrollPump.asksForFrames
        }
        XCTAssertGreaterThan(completions, 0)
        XCTAssertFalse(completionRequestedFrames, "a worker completion starts the next text job without waking the display link")
    }
}
#endif
