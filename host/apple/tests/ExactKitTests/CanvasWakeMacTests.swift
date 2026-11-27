#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactSurfaces

private final class WakeVisibleWindow: NSWindow {
    override var occlusionState: NSWindow.OcclusionState { .visible }
    override var isVisible: Bool { true }
}

private var wakeDirty: Set<UInt32> = []
private var wakeRenders: [UInt32] = []
private var wakeReadbacks: [UInt32] = []
private var wakeTextures: [UInt32] = []
private var wakeThroughIds: Set<UInt32> = []

/// `SurfaceWaker::wake` (`gpu_on_wake`, LLP 1009 D12) on Apple hosts:
/// wakes a resting visible live canvas, stays latched without wall-clock spin
/// under an agent frozen clock, and marks enclosing `through` ancestors dirty.
final class CanvasWakeMacTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() }

    private func makeModule(throughIds: Set<UInt32> = []) -> GpuModule {
        wakeDirty = []
        wakeRenders = []
        wakeReadbacks = []
        wakeTextures = []
        wakeThroughIds = throughIds
        let m = GpuModule(
            create: { _, _, _, _, _ in 1 },
            bind: { _, _, _ in 0 },
            render: { id, _, _, _, _ in
                wakeDirty.remove(id)
                wakeRenders.append(id)
                return 0
            },
            dirty: { id in wakeDirty.contains(id) ? 1 : 0 },
            destroy: { _ in },
            texture: { id, _, _, _, _ in
                wakeTextures.append(id)
                return 0
            },
            textureMetal: nil,
            sync: nil,
            childrenMode: { id in wakeThroughIds.contains(id) ? 1 : 0 },
            readback: { id, _, _, _, _, _, _ in
                wakeDirty.remove(id)
                wakeReadbacks.append(id)
                return 0
            },
            child: { _, _, _, _, _, _, _, _, _, _, _, _ in 0 },
            childrenCount: { _, _ in 0 },
            placement: { _, _, _, _ in 0 },
            shader: nil,
            validateShader: nil,
            clearShaders: nil,
            errorLen: { 0 },
            errorPtr: { nil },
            wantsInput: nil,
            input: nil,
            messages: nil,
            published: nil,
            agent: nil,
            outPtr: { nil }
        )
        m.drew = { _ in 1 }
        return m
    }

    func testSurfaceWokenStartsLiveFramesAndRespectsFrozenClock() throws {
        _ = NSApplication.shared
        let m = makeModule()
        let s = ExactApp.shared.makeSession(label: "canvas-wake")
        defer { s.destroy() }
        s.surfaceHost.modules[""] = m
        s.surfaceHost.attempted = [""]
        let w = WakeVisibleWindow(
            contentRect: NSRect(x: 10, y: 10, width: 100, height: 100),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        w.contentView = ExactView(session: s)
        let canvas = NodeView(id: 100, kind: "canvas", presenter: s.presenter)
        canvas.frame = NSRect(x: 0, y: 0, width: 100, height: 100)
        s.presenter.root.addSubview(canvas)
        s.presenter.views[100] = canvas
        let e = CanvasesHost.Entry(view: canvas, name: "world", values: Data("[]".utf8))
        e.id = 2
        e.module = m
        s.surfaceHost.entries[100] = e

        // Resting canvas: surfaceWoken does not request frames when nothing is dirty.
        s.clock = nil
        s.surfaceHost.surfaceWoken()
        XCTAssertFalse(s.surfaceHost.wantsFrames)

        // Under a frozen agent clock, surfaceWoken must not trigger an async wall-clock render.
        s.clock = 1000
        wakeDirty.insert(2)
        s.surfaceHost.surfaceWoken()
        RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.05))
        XCTAssertTrue(wakeRenders.isEmpty, "frozen clock must not render on wall-clock wake")
        XCTAssertTrue(wakeDirty.contains(2), "wake remains latched until agent clock settles")

        // Settling on the agent clock renders the latched wake once.
        s.canvases.settle(now: 1000)
        XCTAssertEqual(wakeRenders, [2])
        XCTAssertFalse(wakeDirty.contains(2))

        // On a live session (clock == nil), surfaceWoken starts the display link via requestCanvas.
        s.clock = nil
        s.frames.run(false)
        XCTAssertNil(s.frames.link)
        wakeDirty.insert(2)
        XCTAssertTrue(s.surfaceHost.wantsFrames)
        s.surfaceHost.surfaceWoken()
        XCTAssertNotNil(s.frames.link, "surfaceWoken must schedule the live display link")
        XCTAssertFalse(s.surfaceHost.tick(now: 1016))
        XCTAssertEqual(wakeRenders, [2, 2])
        XCTAssertFalse(s.surfaceHost.wantsFrames)
        withExtendedLifetime(w) {}
    }

    func testNestedCanvasWakeMarksThroughAncestorNeedsCaptureInSameTick() throws {
        _ = NSApplication.shared
        let m = makeModule(throughIds: [10, 12])
        let s = ExactApp.shared.makeSession(label: "canvas-wake-nested")
        defer { s.destroy() }
        s.surfaceHost.modules[""] = m
        s.surfaceHost.attempted = [""]
        let w = WakeVisibleWindow(
            contentRect: NSRect(x: 10, y: 10, width: 100, height: 100),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        w.contentView = ExactView(session: s)
        let outer = NodeView(id: 200, kind: "canvas", presenter: s.presenter)
        outer.frame = NSRect(x: 0, y: 0, width: 100, height: 100)
        s.presenter.root.addSubview(outer)
        s.presenter.views[200] = outer
        let outerEntry = CanvasesHost.Entry(view: outer, name: "outer", values: Data("[]".utf8))
        outerEntry.id = 10
        outerEntry.module = m
        outerEntry.through = true
        s.surfaceHost.entries[200] = outerEntry

        let middle = NodeView(id: 202, kind: "canvas", presenter: s.presenter)
        middle.frame = NSRect(x: 0, y: 0, width: 80, height: 80)
        try XCTUnwrap(outer.overlay).addSubview(middle)
        s.presenter.views[202] = middle
        let middleEntry = CanvasesHost.Entry(view: middle, name: "middle", values: Data("[]".utf8))
        middleEntry.id = 12
        middleEntry.module = m
        middleEntry.through = true
        s.surfaceHost.entries[202] = middleEntry

        let inner = NodeView(id: 201, kind: "canvas", presenter: s.presenter)
        inner.frame = NSRect(x: 0, y: 0, width: 50, height: 50)
        try XCTUnwrap(middle.overlay).addSubview(inner)
        s.presenter.views[201] = inner
        let innerEntry = CanvasesHost.Entry(view: inner, name: "inner", values: Data("[]".utf8))
        innerEntry.id = 11
        innerEntry.module = m
        s.surfaceHost.entries[201] = innerEntry
        w.displayIfNeeded()

        outer.needsCapture = false
        middle.needsCapture = false
        wakeDirty.insert(11)
        s.clock = 500
        s.surfaceHost.surfaceWoken()
        XCTAssertTrue(middle.needsCapture, "surfaceWoken must mark enclosing through parent for capture")

        // Even if needsCapture starts false before tick, tick captures deepest-first and propagates to outer.
        outer.needsCapture = false
        middle.needsCapture = false
        wakeDirty.insert(11)
        wakeTextures = []
        wakeReadbacks = []
        _ = s.surfaceHost.tick(now: 500)
        XCTAssertFalse(middle.needsCapture)
        XCTAssertFalse(outer.needsCapture)
        XCTAssertEqual(wakeTextures, [12, 10], "captureIfNeeded uploads child through-canvas before its enclosing ancestor")
        XCTAssertEqual(wakeReadbacks, [11, 12], "leaf readback feeds middle capture, middle readback feeds outer capture")
        withExtendedLifetime(w) {}
    }
}
#endif
