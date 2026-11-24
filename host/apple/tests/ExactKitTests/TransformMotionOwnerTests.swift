import XCTest
#if os(macOS)
import AppKit
#endif
@testable import ExactKit
@testable import ExactDrag

/// LLP 1072 T1/T2: every runtime lives in the owner thread's registry, so a
/// call made from main must be an owner job. The paired transform packet
/// was not: from main it found no runtime at all, and every transform drag
/// (a photo viewer's pan and pinch) was refused at its first geometry
/// report. Whatever the runtime answers a packet now, it is that runtime's
/// answer, not the registry's refusal.
final class TransformMotionOwnerTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactDrag.install() } // LLP 1047.001 D4
    func testPinchModelKeepsUniformScalesAndRefusesAPerAxisTarget() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let p = Presenter()
        p.apply(wireBatch([["op": "create", "id": 1, "kind": "view"], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 100, "h": 100],
            ["op": "present", "id": 1, "property": "scale", "x": 1.5, "y": 1.5]]))
        let node = try XCTUnwrap(p.views[1])
        XCTAssertEqual(node.transformDragModel()?.scale, 1.5)
        p.apply(wireBatch([["op": "present", "id": 1, "property": "scale", "x": 1.5, "y": 0.5]]))
        XCTAssertNil(node.transformDragModel())
    }

    func testAPacketReachesItsRuntimeFromMain() throws {
        let runtime = Runtime()
        defer { runtime.destroy() }
        let packet = TransformDragPacket(op: 10, runtime: 1, handleKey: 1, targetKey: 2, clipKey: 3, sequence: 1,
                                         values: [100, 100, 100, 100, 0, 0], now: 0)
        let reply = try XCTUnwrap(runtime.transformMotion(packet), "a reply")
        XCTAssertFalse(reply.batch.error?.contains("no such runtime") ?? false, reply.batch.error ?? "")
    }
}
