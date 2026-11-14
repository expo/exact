#if os(iOS)
import UniformTypeIdentifiers
import XCTest
@testable import ExactMediaPicker

/// The photo picker's chosen items staged for the core (LLP 1069.002 D4): a
/// cancel has no items and no failure; an item that does not stage is a
/// failure the core refuses, never a cancel or a shorter list.
final class MediaPickerStageIOSTests: XCTestCase {
    private func stage(_ providers: [NSItemProvider]) -> (files: [(url: URL, name: String)], failure: String?) {
        var result: ([(url: URL, name: String)], String?)?
        MediaPicker.stage(providers) { result = ($0, $1) }
        let deadline = Date(timeIntervalSinceNow: 10)
        while result == nil && Date() < deadline { RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.02)) }
        guard let result else { XCTFail("staging never finished"); return ([], nil) }
        for f in result.0 { try? FileManager.default.removeItem(at: f.url) }
        return result
    }

    /// A provider of a JPEG whose load hands over `url`, or fails with `error`.
    private func photo(_ name: String, url: URL?, error: Error? = nil) -> NSItemProvider {
        let provider = NSItemProvider()
        provider.suggestedName = name
        provider.registerFileRepresentation(forTypeIdentifier: UTType.jpeg.identifier, fileOptions: [], visibility: .all) { load in
            load(url, false, error)
            return nil
        }
        return provider
    }

    func testAFailedStageIsAFailureNotACancel() throws {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("media-picker-stage-\(UUID().uuidString).jpg")
        try Data("jpeg".utf8).write(to: file)
        defer { try? FileManager.default.removeItem(at: file) }

        let cancelled = stage([])
        XCTAssertTrue(cancelled.files.isEmpty)
        XCTAssertNil(cancelled.failure)

        let picked = stage([photo("beach", url: file)])
        XCTAssertEqual(picked.files.count, 1)
        XCTAssertTrue(picked.files.first?.name.hasPrefix("beach.") == true, "\(picked.files)")
        XCTAssertNil(picked.failure)

        let unloaded = stage([photo("lost", url: nil, error: CocoaError(.fileReadNoSuchFile))])
        XCTAssertTrue(unloaded.files.isEmpty)
        XCTAssertNotNil(unloaded.failure, "a load that fails is not a cancel")

        let text = NSItemProvider(item: "words" as NSString, typeIdentifier: UTType.plainText.identifier)
        text.suggestedName = "note"
        let mixed = stage([photo("beach", url: file), text])
        XCTAssertEqual(mixed.files.count, 1, "the file that staged is handed back for the caller to remove")
        XCTAssertEqual(mixed.failure, "note is neither an image nor a movie")
    }
}
#endif
