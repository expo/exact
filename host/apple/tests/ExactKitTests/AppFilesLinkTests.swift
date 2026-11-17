import XCTest
import ExactPackage
import ExactSnapshot
@testable import ExactKit

/// An `app:/` file is read as the data module's `readFile` reads it: a
/// symbolic link the app's own code planted in its directories, to a file or
/// a directory outside them, gives an `image`, a video, picture in picture,
/// an iframe's document and an extension's `file` nothing.
final class AppFilesLinkTests: XCTestCase {
    private var base: URL!

    override func setUpWithError() throws {
        base = FileManager.default.temporaryDirectory.appendingPathComponent("exact-app-links-\(UUID().uuidString)")
        let data = base.appendingPathComponent("data")
        try FileManager.default.createDirectory(at: data.appendingPathComponent("photos"), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: base.appendingPathComponent("private"), withIntermediateDirectories: true)
        try PNG.write(to: base.appendingPathComponent("private/secret.png"))
        try PNG.write(to: data.appendingPathComponent("photos/a.png"))
        try FileManager.default.createSymbolicLink(at: data.appendingPathComponent("x.png"), withDestinationURL: base.appendingPathComponent("private/secret.png"))
        try FileManager.default.createSymbolicLink(at: data.appendingPathComponent("linked"), withDestinationURL: base.appendingPathComponent("private"))
        AppFiles.learn(["roots": ["data": data.path]])
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: base)
    }

    func testAFileOpensAndALinkOpensNothing() throws {
        XCTAssertEqual(try XCTUnwrap(AppFiles.open("app:/data/photos/a.png")).readToEnd(), PNG.bytes)
        XCTAssertEqual(AppFiles.file("app:/data/photos/a.png")?.lastPathComponent, "a.png")
        for path in ["app:/data/x.png", "app:/data/linked/secret.png", "app:/data/photos", "app:/data/../private/secret.png", "app:/data/absent.png"] {
            XCTAssertNil(AppFiles.open(path), path)
            XCTAssertNil(AppFiles.file(path), path)
        }
        // Where a host writes is the joined place, a link or not.
        XCTAssertNotNil(AppFiles.url("app:/data/x.png"))
    }

    func testAnImageThroughALinkFailsToOpen() throws {
        let resolver = AssetResolver(root: base)
        let input = try RasterInput.open("app:/data/photos/a.png", resolver: resolver)
        XCTAssertEqual(try input.bytes(), PNG.bytes)
        XCTAssertEqual(try input.metadata().naturalSize, CGSize(width: 1, height: 1))
        XCTAssertThrowsError(try RasterInput.open("app:/data/x.png", resolver: resolver))
        XCTAssertThrowsError(try RasterInput.open("app:/data/linked/secret.png", resolver: resolver))
    }

    /// A file swapped for a link after its source opened is not read through it.
    func testAnImageSwappedForALinkReadsNothing() throws {
        let input = try RasterInput.open("app:/data/photos/a.png", resolver: AssetResolver(root: base))
        let file = base.appendingPathComponent("data/photos/a.png")
        try FileManager.default.removeItem(at: file)
        try FileManager.default.createSymbolicLink(at: file, withDestinationURL: base.appendingPathComponent("private/secret.png"))
        XCTAssertThrowsError(try input.bytes())
        XCTAssertThrowsError(try input.metadata())
    }

    /// The walk an extension's `file` takes (`ExactExtensions.file`
    /// compiles the same source into the app's module).
    func testTheWalkRefusesALinkAtAnyPart() throws {
        let data = base.appendingPathComponent("data")
        XCTAssertNotNil(AppFileWalk.open(data, ["photos", "a.png"]))
        XCTAssertNil(AppFileWalk.open(data, ["x.png"]))
        XCTAssertNil(AppFileWalk.open(data, ["linked", "secret.png"]))
        XCTAssertNil(AppFileWalk.open(data, []))
        XCTAssertNil(AppFileWalk.open(data, ["..", "private", "secret.png"]))
        XCTAssertNil(AppFileWalk.open(data.appendingPathComponent("linked"), ["secret.png"]), "a root that is a link")
    }

    func testAPackageCanCreateAFileWithoutFollowingLinks() throws {
        let data = base.appendingPathComponent("data")
        for names in [["purchases.json"], ["new", "nested", "review.json"], ["photos", "a.png"]] {
            XCTAssertEqual(AppFileWalk.destination(data, names), names.reduce(data) { $0.appendingPathComponent($1) })
        }
        try FileManager.default.createSymbolicLink(at: data.appendingPathComponent("dangling"), withDestinationURL: base.appendingPathComponent("absent"))
        for names in [["x.png"], ["linked", "new.json"], ["dangling"], ["dangling", "new.json"], [], ["..", "new.json"], ["new", "..", "new.json"], ["photos", ""], ["photos", "a.png", "child"]] {
            XCTAssertNil(AppFileWalk.destination(data, names), names.joined(separator: "/"))
        }
        XCTAssertNil(AppFileWalk.destination(data.appendingPathComponent("linked"), ["new.json"]))
        XCTAssertFalse(FileManager.default.fileExists(atPath: data.appendingPathComponent("purchases.json").path))
    }

    func testAFileDestinationRefusesAPopulatedDirectory() throws {
        let data = base.appendingPathComponent("data")
        let album = data.appendingPathComponent("album.jpg")
        try FileManager.default.createDirectory(at: album, withIntermediateDirectories: true)
        let photo = album.appendingPathComponent("a.png")
        try PNG.write(to: photo)
        XCTAssertNil(AppFileWalk.destination(data, ["album.jpg"]))
        XCTAssertEqual(try Data(contentsOf: photo), PNG.bytes)
        XCTAssertNil(AppFileWalk.destination(data, ["photos"]))
    }

    func testAPackageCanCreateOrReplaceADirectoryWithoutFollowingLinks() throws {
        let data = base.appendingPathComponent("data")
        for names in [["transcripts"], ["new", "nested", "transcripts"], ["photos"]] {
            XCTAssertEqual(AppFileWalk.directory(data, names), names.reduce(data) { $0.appendingPathComponent($1) })
        }
        for names in [["x.png"], ["linked"], ["linked", "transcripts"], [], ["..", "transcripts"], ["photos", ""]] {
            XCTAssertNil(AppFileWalk.directory(data, names), names.joined(separator: "/"))
        }
        try PNG.write(to: data.appendingPathComponent("transcript"))
        XCTAssertNil(AppFileWalk.directory(data, ["transcript"]))
        XCTAssertEqual(try Data(contentsOf: data.appendingPathComponent("transcript")), PNG.bytes)
    }

    func testAQueuedDirectoryReplacementCannotBeRedirectedThroughALink() throws {
        let files = FileManager.default, data = base.appendingPathComponent("data")
        let admitted = data.appendingPathComponent("takes/take-2.transcript")
        try files.createDirectory(at: admitted, withIntermediateDirectories: true)
        try Data("partial".utf8).write(to: admitted.appendingPathComponent("00000.jsonl"))
        let outside = base.appendingPathComponent("outside")
        let outsideTranscript = outside.appendingPathComponent("take-2.transcript")
        try files.createDirectory(at: outsideTranscript, withIntermediateDirectories: true)
        let sentinel = outsideTranscript.appendingPathComponent("sentinel")
        try Data("keep".utf8).write(to: sentinel)

        let names = ["takes", "take-2.transcript"]
        XCTAssertNotNil(AppFileWalk.directory(data, names), "admitted before the queued cleanup")
        let writer = DispatchQueue(label: "exact.app-files-link-test")
        let held = DispatchSemaphore(value: 0), done = expectation(description: "writer")
        writer.async { held.wait() }
        writer.async {
            if let directory = AppFileWalk.replaceDirectory(data, names) {
                let fd = openat(directory.fileDescriptor, "00000.jsonl", O_WRONLY | O_CREAT | O_NOFOLLOW | O_CLOEXEC, 0o600)
                if fd >= 0 {
                    let file = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
                    try? file.write(contentsOf: Data("new".utf8))
                    try? file.close()
                }
            }
            done.fulfill()
        }
        let original = data.appendingPathComponent("original-takes")
        try files.moveItem(at: data.appendingPathComponent("takes"), to: original)
        try files.createSymbolicLink(at: data.appendingPathComponent("takes"), withDestinationURL: outside)
        held.signal()
        wait(for: [done], timeout: 2)

        XCTAssertEqual(try Data(contentsOf: sentinel), Data("keep".utf8))
        XCTAssertFalse(files.fileExists(atPath: outsideTranscript.appendingPathComponent("00000.jsonl").path))
    }

    func testAPackageDirectoryRefusesAFIFOWithoutBlockingItsWriter() throws {
        let directory = base.appendingPathComponent("fifo-transcript")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        let fifo = directory.appendingPathComponent("00000.jsonl")
        XCTAssertEqual(mkfifo(fifo.path, 0o600), 0)
        let directoryFD = open(directory.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        XCTAssertGreaterThanOrEqual(directoryFD, 0)
        let handle = FileHandle(fileDescriptor: directoryFD, closeOnDealloc: true)
        let context = ExactPackageContext(agent: true, session: UUID().uuidString,
                                          changed: { _ in }, log: { _ in }, now: { 0 },
                                          replaceDirectory: { _ in handle })
        let output = try XCTUnwrap(context.replaceDirectory("app:/data/transcript"))
        let writer = DispatchQueue(label: "exact.app-files-fifo-test")
        let entered = DispatchSemaphore(value: 0), done = DispatchSemaphore(value: 0)
        var written: Bool?
        writer.async {
            entered.signal()
            written = output.append(Data("line\n".utf8), to: "00000.jsonl")
            done.signal()
        }
        XCTAssertEqual(entered.wait(timeout: .now() + 2), .success, "the transcript writer starts")
        let prompt = done.wait(timeout: .now() + 2)
        if prompt != .success {
            let reader = open(fifo.path, O_RDONLY | O_NONBLOCK | O_CLOEXEC)
            if reader >= 0 { close(reader) }
            XCTAssertEqual(done.wait(timeout: .now() + 2), .success, "the cleanup reader releases the blocked open")
        }
        XCTAssertEqual(prompt, .success, "a special file cannot hold the serial transcript writer")
        XCTAssertEqual(written, false)
    }

    #if canImport(WidgetKit)
    func testASnapshotReadsAnAppImageWithoutFollowingAnyLink() throws {
        let files = FileManager.default, id = "com.example.snapshot-links"
        let roots = SnapshotRasters.appRoots(group: base, appID: id)
        let data = try XCTUnwrap(roots["data"])
        try files.createDirectory(at: data.appendingPathComponent("photos"), withIntermediateDirectories: true)
        try PNG.write(to: data.appendingPathComponent("photos/a.png"))
        for (link, target) in [("leaf-in", data.appendingPathComponent("photos/a.png")),
                               ("leaf-out", base.appendingPathComponent("private/secret.png")),
                               ("ancestor-in", data.appendingPathComponent("photos")),
                               ("ancestor-out", base.appendingPathComponent("private"))] {
            try files.createSymbolicLink(at: data.appendingPathComponent(link), withDestinationURL: target)
        }
        let rasters = SnapshotRasters()
        func decode(_ suffix: String) -> SnapshotRasters.Decoded? {
            rasters.decode("app:/data/" + suffix, roots: [], appGroup: base, appID: id,
                           box: CGSize(width: 1, height: 1), fit: "contain", scale: 1)
        }
        XCTAssertEqual(decode("photos/a.png")?.natural, CGSize(width: 1, height: 1))
        for suffix in ["leaf-in", "leaf-out", "ancestor-in/a.png", "ancestor-out/secret.png"] {
            XCTAssertNil(decode(suffix), suffix)
        }
        try files.moveItem(at: data.appendingPathComponent("photos"), to: data.appendingPathComponent("old-photos"))
        try files.createSymbolicLink(at: data.appendingPathComponent("photos"), withDestinationURL: data.appendingPathComponent("old-photos"))
        XCTAssertNil(decode("photos/a.png"), "a cached image does not admit a replacement link")
    }
    #endif

    private enum PNG {
        /// A 1 × 1 opaque PNG.
        static let bytes = Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==")!
        static func write(to url: URL) throws { try bytes.write(to: url) }
    }
}
