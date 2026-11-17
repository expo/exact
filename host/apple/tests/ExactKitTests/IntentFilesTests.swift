import AppIntents
import XCTest
import UniformTypeIdentifiers
@testable import ExactKit

final class IntentFilesTests: XCTestCase {
    func testImportWaitsForStorageAndUsesTheSessionLanding() throws {
        ExactTools.install()
        let session = ExactApp.shared.makeSession(label: "intent-files")
        defer { session.destroy() }
        XCTAssertNil(session.boot(size: CGSize(width: 390, height: 844)).error)
        XCTAssertFalse(session.dataActivated)
        let originalBoot = ExactTools.boot
        defer { ExactTools.boot = originalBoot }
        var bootedForImport = false
        ExactTools.boot = {
            bootedForImport = true
            session.firstDrawn(generation: session.generation, token: 0)
        }
        let done = expectation(description: "imported after activation")
        let bytes = Data([137, 80, 78, 71])
        Task { @MainActor in
            do {
                let path = try await IntentFiles.importFile(IntentFile(data: bytes, filename: "image.png", type: .png), accept: ["public.png"])
                XCTAssertTrue(path.hasPrefix("app:/tmp/picked/"))
                // The session owns the app storage root; pickedPath exposes that same root to the importer.
                let landing = try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"pickedPath","name":"probe.png"}"#).utf8)) as? [String: Any]
                let next = try XCTUnwrap(landing?["file"] as? String)
                let target = URL(fileURLWithPath: next).deletingLastPathComponent().appendingPathComponent((path as NSString).lastPathComponent)
                XCTAssertEqual(try Data(contentsOf: target), bytes)
            } catch { XCTFail("\(error)") }
            done.fulfill()
        }
        wait(for: [done], timeout: 10)
        XCTAssertTrue(bootedForImport)
        XCTAssertTrue(session.dataActivated)
    }

    func testFilesUseThePickerLandingAndKeepTheirBytes() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("intent-files-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let source = directory.appendingPathComponent("original.txt"), bytes = Data("A file from Shortcuts.".utf8)
        try bytes.write(to: source)
        let files = [IntentFile(data: bytes, filename: "data.txt", type: .plainText), IntentFile(fileURL: source, type: .plainText)]
        for file in files {
            let path = try IntentFiles.copy(file, accept: ["public.plain-text"]) { name in
                ("app:/tmp/picked/\(name)", directory.appendingPathComponent("picked/\(name)"))
            }
            XCTAssertEqual(path, "app:/tmp/picked/\(file.filename)")
            XCTAssertEqual(try Data(contentsOf: directory.appendingPathComponent("picked/\(file.filename)")), bytes)
        }
        XCTAssertEqual(try Data(contentsOf: source), bytes)
    }

    func testWrongTypesDirectoriesAndOversizedFilesAreRefused() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("intent-files-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let large = directory.appendingPathComponent("large.txt")
        XCTAssertTrue(FileManager.default.createFile(atPath: large.path, contents: nil))
        let handle = try FileHandle(forWritingTo: large)
        try handle.truncate(atOffset: UInt64(IntentFiles.maximumBytes + 1))
        try handle.close()
        let target = directory.appendingPathComponent("picked/file.txt")
        let files = [IntentFile(data: Data(), filename: "wrong.png", type: .png), IntentFile(fileURL: directory, type: .plainText), IntentFile(fileURL: large, type: .plainText)]
        for file in files {
            XCTAssertThrowsError(try IntentFiles.copy(file, accept: ["public.plain-text"]) { _ in
                ("app:/tmp/picked/file.txt", target)
            })
            XCTAssertFalse(FileManager.default.fileExists(atPath: target.path))
        }
    }
}
