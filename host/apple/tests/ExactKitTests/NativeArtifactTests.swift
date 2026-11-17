import XCTest
@testable import ExactKit

/// An app whose module artifact is there but the loader refuses answers
/// `unavailable` to every native call, so a drive's ready line carries
/// `artifactFailure()` and the drive refuses it, quoting `dlopen`'s reason.
/// A skewed artifact is LLP 1024 D8.3's failure family: the app runs.
final class NativeArtifactTests: XCTestCase {
    func testAnArtifactTheLoaderRefusesSaysWhy() throws {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID().uuidString).dylib")
        try Data("not a Mach-O file".utf8).write(to: file)
        defer { try? FileManager.default.removeItem(at: file); NativeViews.uninstallTable() }
        let natives = NativeViews()
        XCTAssertNil(natives.artifactFailure(), "an app with no artifact has nothing to refuse")
        natives.installArtifact(file.path)
        let failure = try XCTUnwrap(natives.artifactFailure())
        XCTAssertTrue(failure.contains("dlopen \(file.path)"), failure)

        // A table of another ABI major, as D8.3's smoke builds one.
        let skewed: [UInt32] = [UInt32(NativeViews.tableMajor) + 1, 512]
        skewed.withUnsafeBytes { NativeViews.install(table: $0.baseAddress!) }
        XCTAssertNil(natives.artifactFailure(), "a skewed artifact leaves the app running")
    }
}
