#if os(macOS)
import XCTest
@testable import ExactKit

private func checkLiveFiles(scratch: String?) throws {
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
}
final class LiveScratchStoreObservationTests: XCTestCase {
    func testRealSecretsKeepAppFilesInTheNamedScratchTree() throws { try checkLiveFiles(scratch: "agent2-live") }
}
#endif
