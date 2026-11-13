#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactSurfaces

private let recordBuffer = UnsafeMutablePointer<UInt8>.allocate(capacity: 64)
private var records: [String] = []
private var taken = 0

private final class VisibleWindow: NSWindow {
    override var occlusionState: NSWindow.OcclusionState { .visible }
}

/// A canvas's record whose `presented` changed reaches the app when the
/// module says a frame went to the compositor (`gpu_on_presented`), not at
/// the canvas's next service, which a canvas at rest never has (LLP 1009 D9).
final class CanvasPresentedMacTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() } // LLP 1047.001 D4

    func testThePresentedReportTakesEachLivePublishersRecord() {
        _ = NSApplication.shared
        records = []; taken = 0
        let m = GpuModule(create: { _, _, _, _, _ in 1 }, bind: { _, _, _ in 0 },
            render: { _, _, _, _, _ in 0 }, dirty: { _ in 0 }, destroy: { _ in },
            texture: { _, _, _, _, _ in 0 }, textureMetal: nil, sync: nil,
            childrenMode: { _ in 0 }, readback: { _, _, _, _, _, _, _ in 0 },
            child: { _, _, _, _, _, _, _, _, _, _, _, _ in 0 },
            childrenCount: { _, _ in 0 }, placement: { _, _, _, _ in 0 },
            shader: nil, validateShader: nil, clearShaders: nil, errorLen: { 0 },
            errorPtr: { nil }, wantsInput: nil, input: nil, messages: nil,
            published: { _ in
                taken += 1
                guard !records.isEmpty else { return UInt32.max }
                let bytes = Array(records.removeFirst().utf8)
                bytes.withUnsafeBufferPointer { recordBuffer.update(from: $0.baseAddress!, count: $0.count) }
                return UInt32(bytes.count)
            }, agent: nil, outPtr: { UnsafePointer(recordBuffer) })
        let s = ExactApp.shared.makeSession(label: "canvas-presented")
        defer { s.destroy() }
        s.surfaceHost.modules[""] = m; s.surfaceHost.attempted = [""]
        let w = VisibleWindow(contentRect: NSRect(x: 10, y: 10, width: 100, height: 100),
                              styleMask: [.borderless], backing: .buffered, defer: false)
        w.contentView = ExactView(session: s)
        let canvas = NodeView(id: 100, kind: "canvas", presenter: s.presenter)
        s.presenter.root.addSubview(canvas); s.presenter.views[100] = canvas
        let e = CanvasesHost.Entry(view: canvas, name: "world", values: []); e.id = 1; e.module = m
        s.surfaceHost.entries[100] = e

        if s.surfaceHost.publishers["world"] == nil { s.surfaceHost.claimPublisher(e) }
        records = [#"{"presented":true}"#]
        let before = taken
        s.surfaceHost.publishPresented()
        XCTAssertGreaterThan(taken, before, "the live publisher's record is taken at the report")
        XCTAssertTrue(records.isEmpty, "and handed on")
        withExtendedLifetime(w) {}
    }
}
#endif
