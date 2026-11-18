#if os(iOS)
import Network
import UIKit
import XCTest

/// The XCTest touch runner (`host/apple/touches.swift`): taps and drags from
/// the HID system, which reach the app through UIKit's own hit-testing and
/// gesture recognizers. `build.mjs --test --ios` starts it for a test that
/// throws `missing` for a touch (`xctest.mjs`, `hostedTests`).
final class TouchRunner {
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

    /// `point` in `window`'s scene as a request names it: in the scene and
    /// in the screen's fixed space, on its display.
    static func aim(_ point: CGPoint, in window: UIWindow) throws -> [String: Any] {
        let scene = try XCTUnwrap(window.windowScene)
        let fixed = window.screen.fixedCoordinateSpace.convert(point, from: scene.coordinateSpace)
        return ["point": [point.x, point.y], "screen": [fixed.x, fixed.y],
                "display": [window.screen.bounds.width, window.screen.bounds.height]]
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
                else if ended { failure = NSError(domain: "TouchRunner", code: 1, userInfo: [NSLocalizedDescriptionKey: "XCTest touch runner disconnected"]) }
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
#endif
