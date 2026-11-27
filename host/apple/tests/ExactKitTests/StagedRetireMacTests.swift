#if os(macOS)
import AppKit
import XCTest
@testable import ExactCore
@testable import ExactKit
@testable import ExactAnimations
@testable import ExactNativeViews
@testable import ExactSurfaces

/// Staged replacement retirement and reveal-slot mounting on macOS (LLP 1072 T13).
final class StagedRetireMacTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactAnimations.install(); ExactSurfaces.install() } // LLP 1047.001 D4
    private var window: NSWindow?
    private var minimum = 0
    override func setUp() { super.setUp(); minimum = StagePlan.minimumOps; StagePlan.minimumOps = 10 }
    override func tearDown() { StagePlan.minimumOps = minimum; window?.close(); window = nil }

    private func show(_ viewport: NSView) {
        _ = NSApplication.shared
        viewport.frame = NSRect(x: 0, y: 0, width: 400, height: 600)
        let window = NSWindow(contentRect: viewport.frame, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = viewport
        self.window?.close()
        self.window = window
    }

    private let tree: [[String: Any]] = [9001, 9002, 9004, 9005].map { ["op": "create", "id": $0, "kind": "view", "style": [String: Any]()] } + [
        ["op": "create", "id": 9003, "kind": "text", "style": [String: Any](), "props": ["text": "old"]],
        ["op": "children", "id": 9001, "ids": [9002, 9004, 9005]], ["op": "children", "id": 9002, "ids": [9003]], ["op": "roots", "ids": [9001]],
        ["op": "frame", "id": 9001, "x": 0.0, "y": 0.0, "w": 400.0, "h": 600.0], ["op": "frame", "id": 9002, "x": 0.0, "y": 0.0, "w": 400.0, "h": 100.0],
        ["op": "frame", "id": 9003, "x": 0.0, "y": 0.0, "w": 400.0, "h": 20.0], ["op": "frame", "id": 9004, "x": 0.0, "y": 500.0, "w": 10.0, "h": 10.0],
        ["op": "frame", "id": 9005, "x": 20.0, "y": 500.0, "w": 10.0, "h": 10.0],
    ]

    func testStagedReplacementBulkRetiresPlainDescendantsAndMountsTopInRevealSlot() throws {
        func bigBlock(_ k: Int, replacing old: Int?, rows: Int = 400) -> Batch {
            let base: [(box: UInt32, rows: UInt32)] = [(9100, 10000), (20000, 21000)]
            var ops: [[String: Any]] = old.map { o in
                [["op": "destroy", "id": base[o].box]] + (0..<(3 * rows)).map { ["op": "destroy", "id": base[o].rows + UInt32($0)] }
            } ?? [["op": "destroy", "id": 9003], ["op": "destroy", "id": 9002]]
            let (box, first) = base[k]
            ops.append(["op": "create", "id": box, "kind": "view", "style": [String: Any]()])
            var kids: [UInt32] = []
            for i in 0..<rows {
                let row = first + UInt32(3 * i)
                kids.append(row)
                ops += [
                    ["op": "create", "id": row, "kind": "view", "style": ["background_color": "#ffffff", "border_width_bottom": 1.0, "border_color_bottom": "#e5e5e5"],
                     "props": i == 5 ? ["title": "row tip"] : [:]],
                    ["op": "create", "id": row + 1, "kind": "text", "style": ["font_size": 14.0], "props": ["text": "Row \(k)-\(i) primary title"]],
                    ["op": "create", "id": row + 2, "kind": "text", "style": ["font_size": 12.0], "props": ["text": "Subtitle details for item \(i)"]],
                    ["op": "children", "id": row, "ids": [row + 1, row + 2]],
                    ["op": "frame", "id": row, "x": 0.0, "y": Double(i) * 44, "w": 900.0, "h": 44.0],
                    ["op": "frame", "id": row + 1, "x": 16.0, "y": 4.0, "w": 400.0, "h": 18.0],
                    ["op": "frame", "id": row + 2, "x": 16.0, "y": 22.0, "w": 400.0, "h": 16.0],
                ]
            }
            ops += [
                ["op": "children", "id": box, "ids": kids],
                ["op": "children", "id": 9001, "ids": [box, 9004, 9005]],
                ["op": "frame", "id": box, "x": 0.0, "y": 0.0, "w": 900.0, "h": Double(rows) * 44],
                ["op": "frame", "id": 9001, "x": 0.0, "y": 0.0, "w": 900.0, "h": Double(rows) * 44],
            ]
            return wireBatch(ops)
        }
        let p = Presenter()
        show(p.viewport)
        p.viewport.frame = NSRect(x: 0, y: 0, width: 900, height: 700)
        window?.setContentSize(p.viewport.frame.size)
        window?.orderFront(nil)
        p.apply(wireBatch(tree))
        p.apply(bigBlock(0, replacing: nil))
        CATransaction.flush()

        for round in 0..<3 {
            let next = (round + 1) % 2, old = round % 2
            let oldBase: UInt32 = old == 0 ? 10000 : 21000
            let nextBox: UInt32 = next == 0 ? 9100 : 20000
            let tipView = try XCTUnwrap(p.views[oldBase + 15])
            XCTAssertEqual(tipView.toolTip, "row tip")
            p.pointerHeld = oldBase + 3
            p.pointerPath = [oldBase + 3, 9001]

            let plan = try XCTUnwrap(p.stagePlan(bigBlock(next, replacing: old)))
            while !plan.preparedAll {
                p.prepare(plan, until: CACurrentMediaTime() + 0.004)
                CATransaction.flush()
            }
            let parent = try XCTUnwrap(p.views[9001])
            let newTop = try XCTUnwrap(plan.made[nextBox])
            let sibling = try XCTUnwrap(p.views[9004])
            let topIdx = try XCTUnwrap(parent.subviews.firstIndex(where: { $0 === newTop }))
            let sibIdx = try XCTUnwrap(parent.subviews.firstIndex(where: { $0 === sibling }))
            XCTAssertLessThan(topIdx, sibIdx, "empty top mounts before surviving sibling 9004 during prepare")

            let reveal = plan.reveal()
            XCTAssertEqual(plan.retiredPlain.count, 1200)
            XCTAssertLessThan(reveal.ops.count, 10, "plain retired descendants do not ride through reveal.ops")

            p.revealing = plan
            p.apply(reveal)
            p.revealing = nil
            CATransaction.flush()

            XCTAssertNil(p.views[oldBase], "bulk-retired descendants leave views at reveal")
            XCTAssertNil(p.views[oldBase + 15])
            XCTAssertNil(tipView.toolTip, "bulk retirement clears tooltips")
            XCTAssertNil(p.pointerHeld, "bulk retirement clears held pointer")
            XCTAssertEqual(p.pointerPath, [9001])
            XCTAssertEqual(p.root.frame.height, 400 * 44, accuracy: 1)

            p.teardown.finish()
            CATransaction.flush()
            XCTAssertNil(tipView.superview, "teardown detaches and forgets bulk-retired descendants")
            XCTAssertNil(tipView.presenter, "teardown calls forget() on bulk-retired descendants")
        }
    }

    func testBulkRetireKeepsHatchedAndControlChildrenInRevealOps() throws {
        let p = Presenter()
        show(p.viewport)
        p.apply(wireBatch(tree))
        // Build block 9100 containing a hatched row, a native control with a text child, and plain rows.
        let initial: [[String: Any]] = [
            ["op": "destroy", "id": 9003], ["op": "destroy", "id": 9002],
            ["op": "create", "id": 9100, "kind": "view", "style": [String: Any]()],
            ["op": "create", "id": 9101, "kind": "view", "style": [String: Any](), "props": ["hatch": "map"]],
            ["op": "create", "id": 9102, "kind": "text", "style": [String: Any](), "props": ["text": "under hatch"]],
            ["op": "create", "id": 9103, "kind": "control", "style": [String: Any](), "props": ["type": "button"]],
            ["op": "create", "id": 9104, "kind": "text", "style": [String: Any](), "props": ["text": "inside button"]],
            ["op": "create", "id": 9105, "kind": "view", "style": [String: Any]()],
            ["op": "children", "id": 9101, "ids": [9102]],
            ["op": "children", "id": 9103, "ids": [9104]],
            ["op": "children", "id": 9100, "ids": [9101, 9103, 9105]],
            ["op": "children", "id": 9001, "ids": [9100, 9004, 9005]],
        ]
        p.apply(wireBatch(initial))
        var ops: [[String: Any]] = (9100...9105).reversed().map { ["op": "destroy", "id": $0] }
        ops.append(["op": "create", "id": 9200, "kind": "view", "style": [String: Any]()])
        for i in 0..<20 {
            ops.append(["op": "create", "id": 9300 + i, "kind": "view", "style": [String: Any]()])
        }
        ops += [
            ["op": "children", "id": 9200, "ids": (0..<20).map { 9300 + $0 }],
            ["op": "children", "id": 9001, "ids": [9200, 9004, 9005]],
        ]
        let plan = try XCTUnwrap(p.stagePlan(wireBatch(ops)))
        while !plan.preparedAll { p.prepare(plan, until: .infinity) }
        XCTAssertTrue(plan.retiredPlainSet.contains(9105), "plain descendant under plain top bulk-retires")
        XCTAssertFalse(plan.retiredPlainSet.contains(9101), "hatched node stays in reveal.ops")
        XCTAssertFalse(plan.retiredPlainSet.contains(9102), "descendant of hatched node stays in reveal.ops")
        XCTAssertFalse(plan.retiredPlainSet.contains(9103), "native control stays in reveal.ops")
        XCTAssertFalse(plan.retiredPlainSet.contains(9104), "text child of native control stays in reveal.ops")
        XCTAssertFalse(plan.retiredPlainSet.contains(9100), "outermost retiring top stays in reveal.ops")
        p.revealing = plan
        p.apply(plan.reveal())
        p.revealing = nil
        p.teardown.finish()
        for id: UInt32 in 9100...9105 { XCTAssertNil(p.views[id]) }
        XCTAssertNotNil(p.views[9200])
    }

    func testBulkRetireExcludesSameBatchMutatedClipAndNavigationNodesAndIsolatesHeldState() throws {
        let p = Presenter()
        show(p.viewport)
        p.apply(wireBatch(tree))
        let initial: [[String: Any]] = [
            ["op": "destroy", "id": 9003], ["op": "destroy", "id": 9002],
            ["op": "create", "id": 9100, "kind": "view", "style": [String: Any]()],
            ["op": "create", "id": 9101, "kind": "view", "style": ["clip_path": "inset(2px)"]],
            ["op": "create", "id": 9102, "kind": "view", "style": [String: Any]()],
            ["op": "create", "id": 9103, "kind": "view", "style": [String: Any]()],
            ["op": "create", "id": 9104, "kind": "view", "style": [String: Any]()],
            ["op": "create", "id": 9105, "kind": "view", "style": [String: Any]()],
            ["op": "children", "id": 9100, "ids": [9101, 9102, 9103, 9104, 9105]],
            ["op": "children", "id": 9001, "ids": [9100, 9004, 9005]],
            ["op": "frame", "id": 9100, "x": 0.0, "y": 0.0, "w": 200.0, "h": 100.0],
            ["op": "frame", "id": 9101, "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0],
        ]
        p.apply(wireBatch(initial))
        p.boxAnimations.setClip(9101, CAShapeLayer(), clock: nil)
        XCTAssertTrue(p.boxAnimations.tracks(9101))
        // Session B holds an active state on a node with the same numeric ID (9104).
        let other = Presenter()
        let otherWindow = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 200, height: 200), styleMask: [.titled], backing: .buffered, defer: false)
        otherWindow.isReleasedWhenClosed = false
        otherWindow.contentView = other.viewport
        defer { otherWindow.close() }
        other.apply(wireBatch([
            ["op": "create", "id": 9104, "kind": "view", "style": [String: Any](), "handlers": ["press"]],
            ["op": "roots", "ids": [9104]],
        ]))
        let otherView = try XCTUnwrap(other.views[9104])
        MainActor.assumeIsolated {
            let r = HeldState.hold("active", on: 9104, in: other, dropped: nil)
            XCTAssertEqual(r.held, "active")
        }
        defer { MainActor.assumeIsolated { HeldState.letGo(nil) } }

        // In the staged batch: 9102 is restyled before destroy, 9103 is reparented into surviving 9004 before destroy,
        // 9105 is referenced only by an adapter payload (`height-drag.target`) before destroy,
        // 9101 carries a static clip registered with BoxAnimations, and 9104 is plain.
        var ops: [[String: Any]] = [
            ["op": "style", "id": 9102, "style": ["opacity": 0.5]],
            ["op": "children", "id": 9004, "ids": [9103]],
            ["op": "height-drag", "id": 9005, "target": 9105],
            ["op": "destroy", "id": 9105],
            ["op": "destroy", "id": 9104],
            ["op": "destroy", "id": 9103],
            ["op": "destroy", "id": 9102],
            ["op": "destroy", "id": 9101],
            ["op": "destroy", "id": 9100],
            ["op": "create", "id": 9200, "kind": "view", "style": [String: Any]()],
        ]
        for i in 0..<20 { ops.append(["op": "create", "id": 9300 + i, "kind": "view", "style": [String: Any]()]) }
        ops += [
            ["op": "children", "id": 9200, "ids": (0..<20).map { 9300 + $0 }],
            ["op": "children", "id": 9001, "ids": [9200, 9004, 9005]],
        ]
        let plan = try XCTUnwrap(p.stagePlan(wireBatch(ops)))
        while !plan.preparedAll { p.prepare(plan, until: .infinity) }
        XCTAssertTrue(plan.retiredPlainSet.contains(9104), "untouched plain descendant bulk-retires")
        XCTAssertFalse(plan.retiredPlainSet.contains(9101), "static clip tracked by BoxAnimations stays in reveal.ops")
        XCTAssertFalse(plan.retiredPlainSet.contains(9102), "same-batch styled descendant stays in reveal.ops")
        XCTAssertFalse(plan.retiredPlainSet.contains(9103), "same-batch reparented descendant stays in reveal.ops")
        XCTAssertFalse(plan.retiredPlainSet.contains(9105), "adapter-payload referenced descendant stays in reveal.ops")

        let moved = try XCTUnwrap(p.views[9103])
        p.revealing = plan
        p.apply(plan.reveal())
        p.revealing = nil
        p.teardown.finish()
        XCTAssertNil(moved.superview, "reparented destroyed child is detached, not left painted in surviving sibling")
        XCTAssertFalse(p.boxAnimations.tracks(9101), "BoxAnimations forgets static clip state")
        MainActor.assumeIsolated {
            XCTAssertTrue(HeldState.view === otherView, "bulk retirement in session A does not drop session B's hold on the same numeric id")
        }
        XCTAssertTrue(other.views[9104] === otherView)
    }

    func testDocumentOverflowMatchesConvertUnderOppositeFlipAndBoundsScaling() throws {
        let p = Presenter()
        // Non-flipped document holding a flipped NodeView: coordinate conversion flips Y.
        let unflipped = NSView(frame: NSRect(x: 0, y: 0, width: 200, height: 240))
        let node = NodeView(id: 1, kind: "view", presenter: p)
        node.frame = NSRect(x: 10, y: 20, width: 100, height: 100)
        unflipped.addSubview(node)
        XCTAssertEqual(node.documentOverflow(in: unflipped), node.convert(node.bounds, to: unflipped))

        // Bounds-scaled parent inside flipped document: child bounds scale by 2x into document space.
        let doc = FlippedView(frame: NSRect(x: 0, y: 0, width: 400, height: 200))
        let scaled = NodeView(id: 2, kind: "view", presenter: p)
        scaled.frame = NSRect(x: 0, y: 0, width: 400, height: 200)
        scaled.bounds = NSRect(x: 0, y: 0, width: 200, height: 100)
        let child = NodeView(id: 3, kind: "view", presenter: p)
        child.frame = NSRect(x: 80, y: 0, width: 100, height: 20)
        scaled.addSubview(child)
        doc.addSubview(scaled)
        XCTAssertEqual(child.documentOverflow(in: doc), child.convert(child.bounds, to: doc))

        // Combined bounds scaling and opposite flipping: flipped scaled parent inside unflipped document.
        let unflippedScaledDoc = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 200))
        let scaled2 = NodeView(id: 4, kind: "view", presenter: p)
        scaled2.frame = NSRect(x: 0, y: 0, width: 400, height: 200)
        scaled2.bounds = NSRect(x: 0, y: 0, width: 200, height: 100)
        let child2 = NodeView(id: 5, kind: "view", presenter: p)
        child2.frame = NSRect(x: 80, y: 0, width: 100, height: 20)
        scaled2.addSubview(child2)
        unflippedScaledDoc.addSubview(scaled2)
        XCTAssertEqual(child2.documentOverflow(in: unflippedScaledDoc), child2.convert(child2.bounds, to: unflippedScaledDoc))
    }
}
#endif
