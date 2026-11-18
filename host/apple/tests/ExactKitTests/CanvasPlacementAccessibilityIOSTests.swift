#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactSurfaces

/// Where the fake surface puts each child: a shift that grows each frame,
/// and the indices it hides (`gpu_placement` 2).
private var shift: Float = 0
private var hiddenIndices: Set<UInt32> = []

/// A canvas whose surface places its children (LLP 1014 D5) tells
/// assistive technology only when the children it exposes appear, disappear
/// or reorder: a moving child's frame is read on demand, and a layout change
/// makes VoiceOver read the screen again. UIKit, so a simulator runs it:
///   bun host/apple/build.mjs --test --ios --only CanvasPlacementAccessibilityIOSTests
final class CanvasPlacementAccessibilityIOSTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() } // LLP 1047.001 D4
    override func setUp() { shift = 0; hiddenIndices = [] }

    func testMovingChildrenPostNothingAndAChangedListPostsOnce() throws {
        let s = ExactApp.shared.makeSession(label: "canvas-placement-accessibility")
        defer { s.destroy() }
        let m = GpuModule(create: { _, _, _, _, _ in 7 }, bind: { _, _, _ in 0 },
            render: { _, _, _, _, _ in 0 }, dirty: { _ in 0 }, destroy: { _ in },
            texture: { _, _, _, _, _ in 0 }, textureMetal: nil, sync: nil,
            childrenMode: { _ in 3 }, readback: { _, _, _, _, _, _, _ in 0 },
            child: { _, _, _, _, _, _, _, _, _, _, _, _ in 0 }, childrenCount: { _, _ in 0 },
            placement: { _, index, out, _ in
                if hiddenIndices.contains(index) { return 2 }
                let h: [Float] = [1, 0, shift, 0, 1, 0, 0, 0, 1, 0]
                for (i, v) in h.enumerated() { out?[i] = v }
                return 1
            },
            shader: nil, validateShader: nil, clearShaders: nil, errorLen: { 0 }, errorPtr: { nil },
            wantsInput: nil, input: nil, messages: nil, published: nil, agent: nil, outPtr: nil)
        let host = s.surfaceHost
        host.modules[""] = m; host.attempted = [""]
        m.canvases.add(host)
        let p = s.presenter
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        window.addSubview(p.viewport)
        let canvas = NodeView(id: 100, kind: "canvas", presenter: p)
        canvas.frame = CGRect(x: 0, y: 0, width: 300, height: 300)
        p.root.addSubview(canvas); p.views[100] = canvas
        let overlay = try XCTUnwrap(canvas.overlay)
        try XCTUnwrap(canvas.canvasMetal).frame = canvas.bounds
        overlay.frame = canvas.bounds
        let cards = (0..<3).map { i -> NodeView in
            let card = NodeView(id: UInt32(101 + i), kind: "view", presenter: p)
            card.frame = CGRect(x: 0, y: CGFloat(i) * 80, width: 200, height: 60)
            card.isAccessibilityElement = true
            overlay.addSubview(card); p.views[card.id] = card
            return card
        }
        let e = CanvasesHost.Entry(view: canvas, name: "deck", values: Data("[]".utf8)); e.id = 7; e.module = m; e.each = true
        host.entries[100] = e

        var posts = 0
        let post = CanvasesHost.postLayoutChanged
        CanvasesHost.postLayoutChanged = { posts += 1 }
        defer { CanvasesHost.postLayoutChanged = post }
        func frame() -> [NodeView] {
            _ = host.readback(view: canvas)
            return (overlay.accessibilityElements ?? []).compactMap { $0 as? NodeView }
        }

        XCTAssertEqual(frame(), cards, "the first frame lists the placed children")
        XCTAssertEqual(posts, 1)
        for _ in 0..<12 {
            shift += 7
            _ = frame()
        }
        XCTAssertEqual(cards[0].placement?[2], 84, "the children moved")
        // The list is assigned where the post is (UIKit hands back a copy
        // of it, and its setter is not key-value observable).
        XCTAssertEqual(posts, 1, "children that only move post no layout change, nor assign the list")

        hiddenIndices = [1]
        XCTAssertEqual(frame(), [cards[0], cards[2]], "a hidden placement leaves the list")
        XCTAssertEqual(posts, 2, "and posts once")
        _ = frame()
        XCTAssertEqual(posts, 2)

        hiddenIndices = []
        XCTAssertEqual(frame(), cards, "it comes back")
        XCTAssertEqual(posts, 3, "and posts once")

        overlay.exchangeSubview(at: 0, withSubviewAt: 2)
        XCTAssertEqual(frame(), [cards[2], cards[1], cards[0]], "the list follows the children's order")
        XCTAssertEqual(posts, 4, "a reorder posts once")
        _ = frame()
        XCTAssertEqual(posts, 4)
    }
}
#endif
