#if os(macOS)
import XCTest
@testable import ExactKit

final class DocumentsMacTests: XCTestCase {
    func testACompoundDeclaredSuffixMatchesTheWholeFilename() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("compound-suffix-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let archive = dir.appendingPathComponent("archive.TAR.GZ")
        try Data().write(to: archive)
        let metadata: [String: Any] = ["CFBundleDocumentTypes": [["CFBundleTypeExtensions": ["tar.gz"]]]]
        XCTAssertTrue(ExactDocuments.accepts(archive, metadata: metadata))
    }
}
#endif
