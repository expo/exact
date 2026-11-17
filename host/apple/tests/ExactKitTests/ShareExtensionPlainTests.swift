// LLP 1133.024 D4 (amended 2026-11-15): the share extension's inbox as an app
// that maps no `url` compiles it, without `EXACT_SHARE_PAGE`.
import ExactSharePlain
import Foundation
import UniformTypeIdentifiers
import XCTest

final class ShareExtensionPlainTests: XCTestCase {
    private var scratch: URL!

    override func setUpWithError() throws {
        scratch = FileManager.default.temporaryDirectory.appendingPathComponent("share-plain-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws { try? FileManager.default.removeItem(at: scratch) }

    /// An app that maps only `text`, which is where the claim puts a link, with a member for any file.
    private let info = ShareTargetInfo(group: "group.com.exact.test", appId: "com.exact.test", mapped: ["text"], members: [(name: "files", types: ["public.data"])])

    /// An attachment of `type` whose load fails.
    private func failing(_ type: UTType) -> NSItemProvider {
        let failing = NSItemProvider()
        failing.registerDataRepresentation(forTypeIdentifier: type.identifier, visibility: .all) { loaded in
            loaded(nil, NSError(domain: NSCocoaErrorDomain, code: NSFileReadUnknownError))
            return nil
        }
        return failing
    }

    /// The record a writer leaves for `attachments` and the sheet's `text`, in a container of its own.
    private func taken(_ attachments: [NSItemProvider], text: String? = nil, in name: String) throws -> [String: Any] {
        let item = NSExtensionItem()
        item.attachments = attachments
        let container = scratch.appendingPathComponent(name, isDirectory: true)
        let writer = try XCTUnwrap(ShareInboxWriter(info: info, container: container))
        let done = expectation(description: "taken")
        writer.take([item]) { done.fulfill() }
        wait(for: [done], timeout: 10)
        try writer.finish(title: nil, text: text)
        let data = try Data(contentsOf: writer.directory.appendingPathComponent("share.json"))
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    /// This build reads no page details: Safari's answer is a property list like another, stored as a file.
    func testSafarisAnswerIsAFileLikeAnother() throws {
        let answer = NSItemProvider(item: [NSExtensionJavaScriptPreprocessingResultsKey: ["url": "https://x.test/trip"]] as NSDictionary, typeIdentifier: UTType.propertyList.identifier)
        let record = try taken([answer], in: "answer")
        XCTAssertNil(record["url"])
        XCTAssertEqual((record["files"] as? [[String: String]])?.map { $0["member"] }, ["files"])
    }

    /// A link that does not load is left out of a share that holds something else. A share it was the whole of is
    /// not written, and neither is one whose only file does not load.
    func testAShareLeftWithNothingIsNotWritten() throws {
        let link = NSItemProvider(object: URL(string: "https://x.test/a")! as NSURL)
        XCTAssertThrowsError(try taken([failing(.url)], in: "link")) { XCTAssertEqual(($0 as NSError).code, 3) }
        XCTAssertThrowsError(try taken([failing(.png)], in: "file")) { XCTAssertEqual(($0 as NSError).code, 6) }
        var record = try taken([failing(.url)], text: "Read later", in: "link-text")
        XCTAssertEqual(record["text"] as? String, "Read later")
        XCTAssertNil(record["url"])
        record = try taken([failing(.url), link, failing(.png)], in: "link-link")
        XCTAssertEqual(record["url"] as? String, "https://x.test/a")
        XCTAssertEqual(record["dropped"] as? [String: Int], ["files": 1])
    }
}
