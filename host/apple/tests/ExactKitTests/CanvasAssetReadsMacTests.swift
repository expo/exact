#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactSurfaces

private var requested: [String] = []
private var delivered: [String] = []
/// Kept open for the test's life: a canvas outside a window is not live.
private var windows: [NSWindow] = []
private let out = UnsafeMutablePointer<UInt8>.allocate(capacity: 4096)

/// A canvas's asset files are read off the main thread under the live clock
/// and delivered on a later turn; under the agent's clock, in the call that
/// asked (issue #129).
final class CanvasAssetReadsMacTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() } // LLP 1047.001 D4

    /// A session with one live canvas whose module asks for `requested` and records `delivered`.
    private func canvas(label: String) -> (ExactSession, CanvasesHost.Entry) {
        let s = ExactApp.shared.makeSession(label: label)
        let m = GpuModule(create: { _, _, _, _, _ in 1 }, bind: { _, _, _ in 0 },
            render: { _, _, _, _, _ in 0 }, dirty: { _ in 0 }, destroy: { _ in },
            texture: { _, _, _, _, _ in 0 }, textureMetal: nil, sync: nil,
            childrenMode: { _ in 0 }, readback: { _, _, _, _, _, _, _ in 0 },
            child: { _, _, _, _, _, _, _, _, _, _, _, _ in 0 }, childrenCount: { _, _ in 0 }, placement: { _, _, _, _ in 0 },
            shader: nil, validateShader: nil, clearShaders: nil, errorLen: { 0 }, errorPtr: { nil },
            wantsInput: { _ in 0 }, input: nil, messages: nil, published: nil, agent: { _, _, _ in 0 },
            outPtr: { UnsafePointer(out) })
        m.assets = { _ in
            let json = try! JSONSerialization.data(withJSONObject: ["requests": requested, "retired": [String]()])
            requested = []
            json.copyBytes(to: out, count: json.count)
            return UInt32(json.count)
        }
        m.asset = { _, name, n, bytes, count in
            let name = String(decoding: UnsafeBufferPointer(start: name, count: n), as: UTF8.self)
            let body = String(decoding: UnsafeBufferPointer(start: bytes, count: count), as: UTF8.self)
            delivered.append(name == body ? name : "\(name) read as \(body)")
            return true
        }
        s.surfaceHost.modules[""] = m; s.surfaceHost.attempted = [""]
        let p = s.presenter
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 200, height: 200), styleMask: [.borderless], backing: .buffered, defer: false)
        window.contentView = p.viewport
        windows.append(window)
        let canvas = NodeView(id: 100, kind: "canvas", presenter: p)
        p.root.addSubview(canvas); p.views[100] = canvas
        let e = CanvasesHost.Entry(view: canvas, name: "world", values: []); e.id = 1; e.module = m
        s.surfaceHost.entries[100] = e
        return (s, e)
    }

    func testALiveCanvasDeliversItsAssetsOnALaterTurnAndTheAgentsClockAtOnce() throws {
        _ = NSApplication.shared
        delivered = []
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("asset-reads-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root.appendingPathComponent("assets"), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        for name in ["a.tex", "b.tex"] { try Data(name.utf8).write(to: root.appendingPathComponent("assets/" + name)) }
        let previous = ExactApp.shared.assetRoot
        ExactApp.shared.assetRoot = root
        defer { ExactApp.shared.assetRoot = previous }
        let (s, e) = canvas(label: "asset-reads")
        defer { s.destroy() }
        requested = ["a.tex"]
        s.surfaceHost.messages(e)
        XCTAssertEqual(delivered, [], "the file is still being read")
        waitFor("the read to land") { !delivered.isEmpty }
        XCTAssertEqual(delivered, ["a.tex"])

        s.clock = 0
        requested = ["b.tex"]
        s.surfaceHost.messages(e)
        XCTAssertEqual(delivered, ["a.tex", "b.tex"], "the agent's clock is answered in the call")
    }

    func testTheAgentTakingTheClockMidReadDeliversTheReadOnce() throws {
        _ = NSApplication.shared
        delivered = []
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("asset-reads-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root.appendingPathComponent("assets"), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("a.tex".utf8).write(to: root.appendingPathComponent("assets/a.tex"))
        let previous = ExactApp.shared.assetRoot
        ExactApp.shared.assetRoot = root
        defer { ExactApp.shared.assetRoot = previous }
        let (s, e) = canvas(label: "asset-takeover")
        defer { s.destroy() }
        requested = ["a.tex"]
        s.surfaceHost.messages(e)
        XCTAssertEqual(delivered, [], "the file is still being read")
        // The agent takes the clock before the read lands: the next service
        // answers it at once, and the late result is not delivered again.
        s.clock = 0
        s.surfaceHost.messages(e)
        XCTAssertEqual(delivered, ["a.tex"])
        // Once the read has finished and the main queue has run its result,
        // that late result is not delivered.
        s.surfaceHost.assetReads.sync {}
        waitForMainQueue()
        s.surfaceHost.messages(e)
        XCTAssertEqual(delivered, ["a.tex"])
    }
}
#endif
