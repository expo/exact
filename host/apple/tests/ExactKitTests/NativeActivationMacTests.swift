#if os(macOS)
import AppKit
import CryptoKit
import ExactCore
import XCTest
@testable import ExactKit

final class NativeActivationMacTests: XCTestCase {
    func testLoaderCompletionPresentsTheActivatedFrameAndIgnoresARetiredSession() throws {
        try complete(refuse: false)
    }

    func testLoaderFailureReachesTheSessionWithoutAnotherInput() throws {
        try complete(refuse: true)
    }

    private func complete(refuse: Bool) throws {
        let app = ExactApp.shared
        guard app.rustPolicy.mode == "native" else { throw XCTSkip("requires a native Rust cohort: run with caltrain-apple") }
        _ = NSApplication.shared
        let fixture = try NativeActivationFixture(refuse: refuse)
        defer { fixture.close() }
        let session = app.makeSession(label: "native-loader"), retired = app.makeSession(label: "retired-native-loader")
        defer { session.destroy(); retired.destroy() }
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 100, height: 100), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        for current in [session, retired] {
            let candidate = try XCTUnwrap(current.prepare(fixture.plan, resolver: app.resolver, module: fixture.module, size: CGSize(width: 100, height: 100)))
            let batch = try XCTUnwrap(current.commit(candidate))
            current.presentCommitted(batch, label: "native activation fixture")
        }
        let view = ExactView(session: session)
        window.contentView = view
        window.orderFrontRegardless()
        waitFor("the loading session's first draw and pending activation") { session.pendingActivation != nil }
        let entered = expectation(description: "dlopen entered the real library constructor")
        DispatchQueue.global().async {
            _ = try? fixture.entered.fileHandleForReading.read(upToCount: 1)
            entered.fulfill()
        }
        wait(for: [entered], timeout: 30)
        XCTAssertFalse(session.dataActivated)
        // The second runtime holds a waiter too, then retires before the load ends.
        retired.firstDrawn(generation: retired.generation, token: 0)
        waitFor("the second session's pending activation") { retired.pendingActivation != nil }
        retired.destroy()
        var applied = 0
        var activationError: String?
        session.onApplyForTests = { batch in
            if let error = batch.error { activationError = error }
            if batch.ops.contains(where: { $0.props["text"] == "99" }) { applied += 1 }
        }
        try fixture.release.fileHandleForWriting.write(contentsOf: Data([1]))
        // Only observe the real session. No agent operation, dataReady call or
        // clock tick retries activation while the loader and owner finish.
        waitFor("the loader's completion applied by the session") { session.dataActivated }
        XCTAssertEqual(applied, refuse ? 0 : 1)
        if refuse {
            XCTAssertTrue(activationError?.contains("Rust native export ABI differs") == true, activationError ?? "no refusal")
        } else {
            let value = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "value" })
            XCTAssertEqual(value.props["text"], "99")
            let result = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "result" })
            waitFor("the activated frame's blue layer") {
                guard let color = result.layer?.presentation()?.backgroundColor,
                      let rgb = NSColor(cgColor: color)?.usingColorSpace(.deviceRGB) else { return false }
                return rgb.blueComponent > 0.99 && rgb.redComponent < 0.01
            }
            XCTAssertNotNil(value.window, "the activated frame belongs to the presented session")
        }
        XCTAssertEqual(retired.state, .destroyed)
    }
}

private final class NativeActivationFixture {
    let entered = Pipe(), release = Pipe()
    let directory: URL
    let plan: Data
    let module: ExactModule

    init(refuse: Bool) throws {
        let compat = try XCTUnwrap(try JSONSerialization.jsonObject(with: Runtime.bakedCompat()) as? [String: Any])
        let inputs = try XCTUnwrap(compat["inputs"] as? [String: Any])
        let abi = try XCTUnwrap(inputs["rustAbi"] as? UInt32)
        let grants = try XCTUnwrap(inputs["rustGrants"] as? String)
        let app = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_TEST_APP_ID"])
        let compiler = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"])
        var root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        for _ in 0..<4 { root.deleteLastPathComponent() }
        directory = root.appendingPathComponent("target/native-activation-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try JSONSerialization.data(withJSONObject: ["app": ["id": app, "name": "Native activation"]]).write(to: directory.appendingPathComponent("app.json"))
        let source = try String(contentsOf: root.appendingPathComponent("logic/tests/fixtures/activation.contract"), encoding: .utf8)
        try source.write(to: directory.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        try Self.run(compiler, ["build", directory.appendingPathComponent("app.contract").path, "-o", directory.appendingPathComponent("app.plan").path])
        plan = try Data(contentsOf: directory.appendingPathComponent("app.plan"))
        func u32(_ value: UInt32) -> [UInt8] { withUnsafeBytes(of: value.littleEndian) { Array($0) } }
        func string(_ text: String) -> [UInt8] { u32(UInt32(text.utf8.count)) + Array(text.utf8) }
        let header = u32(abi)
        let metadata = header + [0] + string(app) + string(grants)
        let unit = header + [1, 0, 3]
        let number = withUnsafeBytes(of: Double(99).bitPattern.littleEndian) { Array($0) }
        let answer = header + [2, 0] + Array(repeating: UInt8(0), count: 16) + [0] + string("") + [0, 0] + number
        var c = try String(contentsOf: root.appendingPathComponent("logic/tests/fixtures/activation.c"), encoding: .utf8)
        let substitutions = ["@ABI@": String(abi), "@REFUSE@": refuse ? "1" : "0",
            "@ENTERED@": String(entered.fileHandleForWriting.fileDescriptor), "@RELEASE@": String(release.fileHandleForReading.fileDescriptor),
            "@METADATA@": metadata.map(String.init).joined(separator: ","), "@UNIT@": unit.map(String.init).joined(separator: ","),
            "@ANSWER@": answer.map(String.init).joined(separator: ",")]
        for (key, value) in substitutions { c = c.replacingOccurrences(of: key, with: value) }
        let cPath = directory.appendingPathComponent("activation.c"), library = directory.appendingPathComponent("activation.dylib")
        try c.write(to: cPath, atomically: true, encoding: .utf8)
        try Self.run("/usr/bin/cc", ["-shared", "-fPIC", cPath.path, "-o", library.path])
        let bytes = try Data(contentsOf: library)
        func card(_ file: String, _ data: Data) -> [String: Any] {
            ["file": file, "bytes": data.count, "sha256": SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()]
        }
        let receipt: [String: Any] = ["version": 1, "kind": "rust", "abi": abi, "appId": app, "grants": grants,
            "executor": "native", "target": ExactApp.shared.rustPolicy.target, "plan": card("app.plan", plan), "module": card("app.module.dylib", bytes)]
        module = ExactModule(receipt: try JSONSerialization.data(withJSONObject: receipt), bytecode: bytes)
    }

    func close() {
        try? release.fileHandleForWriting.close()
        try? FileManager.default.removeItem(at: directory)
    }

    private static func run(_ executable: String, _ arguments: [String]) throws {
        let process = Process(), errors = Pipe()
        process.executableURL = URL(fileURLWithPath: executable); process.arguments = arguments
        process.standardOutput = FileHandle.nullDevice; process.standardError = errors
        try process.run()
        let error = try errors.fileHandleForReading.readToEnd() ?? Data()
        process.waitUntilExit()
        XCTAssertEqual(process.terminationStatus, 0, String(decoding: error, as: UTF8.self))
        guard process.terminationStatus == 0 else { throw NativeActivationFailure() }
    }
}
private struct NativeActivationFailure: Error {}
#endif
