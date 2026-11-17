import XCTest
@testable import ExactCore

/// A read that blocks (a file under a privacy-gated folder, a slow volume, a
/// provider waiting on I/O) holds up only the asset it reads: every other
/// lookup on the resolver keeps answering, so the main thread never waits
/// behind it (#457).
final class AssetResolverLockTests: XCTestCase {
    private final class Box: @unchecked Sendable { var data: Data?; var url: URL? }

    private func resolver(blocking slow: String, entered: DispatchSemaphore, release: DispatchSemaphore) -> AssetResolver {
        AssetResolver(root: FileManager.default.temporaryDirectory, names: ["assets/slow.bin", "assets/fast.bin"]) { name in
            if name == slow { entered.signal(); release.wait() }
            return Data(name.utf8)
        }
    }

    /// While `start` is held inside the provider's read, a lookup of another
    /// asset and the refusal both answer.
    private func assertOthersAnswer(while start: @escaping @Sendable (AssetResolver) -> Void, file: StaticString = #filePath, line: UInt = #line) {
        let entered = DispatchSemaphore(value: 0), release = DispatchSemaphore(value: 0), held = DispatchSemaphore(value: 0)
        let resolver = resolver(blocking: "assets/slow.bin", entered: entered, release: release)
        DispatchQueue.global().async { start(resolver); held.signal() }
        XCTAssertEqual(entered.wait(timeout: .now() + 5), .success, "the read never started", file: file, line: line)
        let box = Box(), answered = DispatchSemaphore(value: 0)
        DispatchQueue.global().async { box.data = resolver.bytes("assets/fast.bin"); _ = resolver.refusal; answered.signal() }
        let result = answered.wait(timeout: .now() + 2)
        release.signal() // let the held read finish either way, so a failure does not wedge the run
        XCTAssertEqual(held.wait(timeout: .now() + 5), .success, file: file, line: line)
        XCTAssertEqual(result, .success, "a blocked read held the resolver's lock", file: file, line: line)
        if result == .success { XCTAssertEqual(box.data, Data("assets/fast.bin".utf8), file: file, line: line) }
    }

    func testABlockedReadHoldsUpNoOtherLookup() {
        assertOthersAnswer(while: { _ = $0.bytes("assets/slow.bin") })
    }

    func testABlockedMaterializationHoldsUpNoOtherLookup() {
        assertOthersAnswer(while: { _ = $0.url("assets/slow.bin") })
    }
}
