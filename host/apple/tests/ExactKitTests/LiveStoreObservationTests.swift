#if os(macOS)
import XCTest
@testable import ExactKit

private func checkLiveFiles(scratch: String?) throws {
    // The storage observation, without the GPU surfaces the bundle installs
    // (`TestsInstall.swift`): with them a live `state` gave this read no reply.
    let surfaces = SurfacesLink.installed
    SurfacesLink.installed = nil
    defer { SurfacesLink.installed = surfaces }
    let session = ExactApp.shared.makeSession()
    defer { session.destroy() }
    XCTAssertNil(session.boot(size: CGSize(width: 390, height: 844)).error)
    XCTAssertEqual(ExactEnv.environment["EXACT_AGENT"], "live")
    XCTAssertEqual(ExactEnv.environment["EXACT_STORE"], "real")
    let pipe = Pipe(), saved = Agent.out
    Agent.out = pipe.fileHandleForWriting
    defer { Agent.out = saved }
    session.agentInstance.handle(op: "state", ["op": "state"], line: "{\"op\":\"state\"}")
    try pipe.fileHandleForWriting.close()
    let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: pipe.fileHandleForReading.readDataToEndOfFile()) as? [String: Any])
    let files = try XCTUnwrap(state["storage"] as? [String: Any])
    XCTAssertEqual(files["available"] as? Bool, scratch != nil)
    XCTAssertEqual(files["store"] as? String, scratch)
    let trace = session.traceSnapshot()
    let observation = try XCTUnwrap(trace["observation"] as? [String: Any])
    XCTAssertEqual(observation["store"] as? String, scratch ?? "none")
    XCTAssertTrue((observation["substitutions"] as? [String] ?? []).contains("storage"))
    XCTAssertEqual(observation["performanceClaim"] as? Bool, false)
}

final class LiveStoreObservationTests: XCTestCase {
    func testRealSecretsDoNotClaimRealAppFiles() throws { try checkLiveFiles(scratch: nil) }

    /// `--live` keeps the wall clock: every clock request but an input's landing is refused.
    func testLiveRefusesTheClockAndTakesALanding() throws {
        let session = ExactApp.shared.makeSession()
        defer { session.destroy() }
        XCTAssertNil(session.boot(size: CGSize(width: 390, height: 844)).error)
        for request: [String: Any] in [["op": "clock", "to": 1000.0], ["op": "clock", "take": true], ["op": "clock", "settle": true], ["op": "clock", "data": true]] {
            XCTAssertEqual(session.agentInstance.clock(request)["code"] as? String, "live-clock", "\(request)")
        }
        XCTAssertNil(session.clock)
        let landed = session.agentInstance.clock(["op": "clock", "land": true])
        XCTAssertNil(landed["error"])
        XCTAssertEqual(landed["wall"] as? Bool, true)
        XCTAssertNil(session.clock)
    }
}
final class LiveScratchStoreObservationTests: XCTestCase {
    func testRealSecretsKeepAppFilesInTheNamedScratchTree() throws { try checkLiveFiles(scratch: "agent2-live") }
}
#endif
