#if os(iOS)
import Network
import UIKit
import XCTest
@testable import ExactKit
import ExactDrag

final class TouchActionIOSTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactDrag.install() }
    private var window: UIWindow?
    override func tearDown() { window?.isHidden = true; window = nil; super.tearDown() }

    func testScrollAndVirtualizedListReadTouchActionAtRecognition() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "touch-action needs a window") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "build.mjs --test --ios starts the XCTest touch runner")
        }
        try checkPolicy(in: scene)
    }

    func testAuthoredPanReadsThePointerdownBatchAtRecognition() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "touch-action needs a window") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "build.mjs --test --ios starts the XCTest touch runner")
        }
        try checkPolicy(in: scene, authored: "pan")
    }

    func testTransformPanReadsThePointerdownBatchAtRecognition() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "touch-action needs a window") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "build.mjs --test --ios starts the XCTest touch runner")
        }
        try checkPolicy(in: scene, authored: "transform")
    }

    private func checkPolicy(in scene: UIWindowScene, authored: String? = nil) throws {
        let touches = try Touches(self)
        defer { touches.connection.cancel() }
        let controller = UIViewController(), window = UIWindow(windowScene: scene)
        window.frame = scene.screen.bounds
        window.rootViewController = controller
        window.makeKeyAndVisible()
        self.window = window
        for list in [false, true] {
            let p = Presenter()
            defer { p.onPointer = nil; p.onPan = nil; p.collections.reset(); p.viewport.removeFromSuperview() }
            p.viewport.frame = controller.view.bounds
            controller.view.addSubview(p.viewport)
            var ops: [[String: Any]] = []
            if list {
                ops.append(["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0,
                    "count": 50, "totalExtent": 2000, "rows": [["view": 2, "root": 2, "epoch": 1]], "correction": NSNull()]]])
            }
            ops += [
                ["op": "create", "id": 1, "kind": list ? "list" : "view", "handlers": authored == "transform" ? [] : ["pointerdown", "pointerup"] + (authored == "pan" ? ["pan"] : []),
                 "style": ["overflow_x": "scroll", "overflow_y": "scroll"]],
                ["op": "create", "id": 2, "kind": "view", "handlers": authored == "transform" ? ["pointerdown", "pointerup"] : []],
                ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
                ["op": "frame", "id": 1, "x": 20.0, "y": 100.0, "w": 300.0, "h": 300.0],
                ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 2000.0, "h": 2000.0],
                ["op": "content", "id": 1, "w": 2000.0, "h": 2000.0]
            ]
            if authored == "transform" {
                ops.append(["op": "transform-drag", "id": 2, "runtime": "1", "handleKey": "2",
                    "target": 2, "targetKey": "2", "clip": 1, "clipKey": "1"])
            }
            p.apply(wireBatch(ops))
            let policyID: UInt32 = authored == "transform" ? 2 : 1
            let node = try XCTUnwrap(p.views[policyID]), scroll = try XCTUnwrap(p.views[1]?.scroll)
            XCTAssertEqual(p.collections.entries[1] != nil, list)
            let recognized = Recognitions()
            if authored == "transform" {
                // No session or motion runtime: observe the production recognizer's admission only.
                let pan = try XCTUnwrap(node.transformRecognizer)
                pan.addTarget(recognized, action: #selector(Recognitions.pan(_:)))
            }
            p.navigation.finishBootOwner()
            window.layoutIfNeeded()
            let display = CADisplayLink(target: recognized, selector: #selector(Recognitions.frame))
            display.add(to: .main, forMode: .common)
            waitFor("the fixture is presented") { recognized.frames >= 2 }
            display.invalidate()
            for (initial, changed) in [("none", nil), ("pan-x", nil), ("pan-y", nil), ("pan-y", "none"), ("none", "pan-y")] as [(String, String?)] {
                for horizontal in [false, true] {
                    func write(_ action: String) {
                        var style: [String: Any] = ["touch_action": action]
                        if policyID == 1 { style.merge(["overflow_x": "scroll", "overflow_y": "scroll"]) { _, new in new } }
                        p.apply(wireBatch([["op": "style", "id": Int(policyID), "style": style]]))
                    }
                    write(initial)
                    scroll.setContentOffset(CGPoint(x: 300, y: 300), animated: false)
                    var downs = 0, pans = 0
                    recognized.begins = 0
                    p.onPan = { _, _, _ in pans += 1 }
                    p.onPointer = { _, kind, _ in
                        if kind == .down {
                            downs += 1
                            if let changed { write(changed) }
                        }
                    }
                    let start = scroll.convert(CGPoint(x: scroll.bounds.midX, y: scroll.bounds.midY), to: window)
                    let end = CGPoint(x: start.x - (horizontal ? 80 : 0), y: start.y - (horizontal ? 0 : 80))
                    let fixed = window.screen.fixedCoordinateSpace
                    let a = fixed.convert(start, from: scene.coordinateSpace), b = fixed.convert(end, from: scene.coordinateSpace)
                    let reply = try touches.ask(["op": "drag", "point": [start.x, start.y], "to": [end.x, end.y],
                        "screen": [a.x, a.y], "screenTo": [b.x, b.y], "display": [window.screen.bounds.width, window.screen.bounds.height],
                        "press": 0.1, "hold": 0.1, "velocity": 200.0])
                    XCTAssertEqual(reply["done"] as? Bool, true)
                    waitFor("the drag ends") { !scroll.isDragging && !scroll.isDecelerating }
                    XCTAssertEqual(downs, 1)
                    let delta = horizontal ? scroll.contentOffset.x - 300 : scroll.contentOffset.y - 300
                    let allowed = (changed ?? initial) == (horizontal ? "pan-x" : "pan-y")
                    let label = "\(authored ?? "native") \(list ? "list" : "scroll") \(initial) → \(changed ?? initial), \(horizontal ? "x" : "y")"
                    print("TOUCH-ACTION \(label): \(delta)")
                    if allowed { XCTAssertGreaterThan(delta, 30, label) }
                    else { XCTAssertEqual(delta, 0, accuracy: 0.5, label) }
                    if authored == "pan" { XCTAssertEqual(pans > 0, !allowed, label) }
                    if authored == "transform" { XCTAssertEqual(recognized.begins, allowed ? 0 : 1, label) }
                }
            }
        }
    }

    private final class Recognitions: NSObject {
        var begins = 0, frames = 0
        @objc func frame() { frames += 1 }
        @objc func pan(_ gesture: UIPanGestureRecognizer) {
            if gesture.state == .began { begins += 1 }
        }
    }

    private final class Touches {
        let connection: NWConnection
        let test: XCTestCase
        init(_ test: XCTestCase) throws {
            self.test = test
            let env = ProcessInfo.processInfo.environment
            let port = try XCTUnwrap(env["EXACT_TOUCH_PORT"].flatMap(UInt16.init).flatMap(NWEndpoint.Port.init(rawValue:)))
            connection = NWConnection(host: "127.0.0.1", port: port, using: .tcp)
            connection.start(queue: .main)
            _ = try ask(["token": try XCTUnwrap(env["EXACT_TOUCH_TOKEN"]), "app": try XCTUnwrap(Bundle.main.bundleIdentifier)])
        }
        func ask(_ request: [String: Any]) throws -> [String: Any] {
            var reply: [String: Any]?, failure: Error?, bytes = Data()
            var data = try JSONSerialization.data(withJSONObject: request)
            data.append(10)
            connection.send(content: data, completion: .contentProcessed { if let error = $0 { failure = error } })
            func receive() {
                connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { data, _, ended, error in
                    if let data { bytes.append(data) }
                    if let end = bytes.firstIndex(of: 10) {
                        do { reply = try JSONSerialization.jsonObject(with: bytes.prefix(upTo: end)) as? [String: Any] }
                        catch { failure = error }
                    } else if let error { failure = error }
                    else if ended { failure = NSError(domain: "TouchAction", code: 1, userInfo: [NSLocalizedDescriptionKey: "XCTest touch runner disconnected"]) }
                    else { receive() }
                }
            }
            receive()
            test.waitFor("XCTest answers \(request["op"] ?? "hello")", cap: 150) { reply != nil || failure != nil }
            if let failure { throw failure }
            let result = try XCTUnwrap(reply)
            XCTAssertNil(result["error"], "\(result)")
            return result
        }
    }
}
#endif
