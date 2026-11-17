// LLP 1133.024 D4 (amended 2026-11-15), D9: the share extension's inbox,
// written through a stub extension context, as the app's claim reads it.
import Foundation
import UniformTypeIdentifiers
import XCTest
@testable import ExactShare
#if canImport(AppKit) && !targetEnvironment(macCatalyst)
import AppKit
#endif
#if canImport(UIKit)
import UIKit
#endif

/// An extension context a test hands a controller: the shared items, and how the request ended.
private final class StubContext: NSExtensionContext {
    var items: [Any] = []
    var completed = false
    var cancelled: Error?
    override var inputItems: [Any] { items }
    override func completeRequest(returningItems: [Any]?, completionHandler: ((Bool) -> Void)? = nil) { completed = true }
    override func cancelRequest(withError error: Error) { cancelled = error }
}

final class ShareExtensionTests: XCTestCase {
    private var scratch: URL!

    override func setUpWithError() throws {
        scratch = FileManager.default.temporaryDirectory.appendingPathComponent("share-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: scratch, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws { try? FileManager.default.removeItem(at: scratch) }

    private let info = ShareTargetInfo(group: "group.com.exact.test", appId: "com.exact.test", mapped: ["title", "text"],
                                       members: [(name: "files", types: ["public.plain-text", "public.image"])])

    /// A note, a link, a file no member accepts and the sheet's own text: the note is stored under a counter's
    /// name, the link is `url`, the odd file is counted, and the text is not an attachment.
    private func items() throws -> [NSExtensionItem] {
        let note = scratch.appendingPathComponent("packing.txt"), odd = scratch.appendingPathComponent("data.bin")
        try "water, map".write(to: note, atomically: true, encoding: .utf8)
        try Data([0, 1, 2, 3]).write(to: odd)
        let item = NSExtensionItem()
        item.attributedTitle = NSAttributedString(string: "Trip")
        item.attributedContentText = NSAttributedString(string: "Bring water")
        item.attachments = [NSItemProvider(contentsOf: note)!, NSItemProvider(object: URL(string: "https://x.test/a")! as NSURL),
                            NSItemProvider(contentsOf: odd)!, NSItemProvider(object: "Bring water" as NSString)]
        return [item]
    }

    private func record(in container: URL) throws -> (URL, [String: Any]) {
        let shares = try FileManager.default.contentsOfDirectory(at: container.appendingPathComponent("shares"), includingPropertiesForKeys: nil)
        XCTAssertEqual(shares.count, 1)
        let share = try XCTUnwrap(shares.first)
        let data = try Data(contentsOf: share.appendingPathComponent("share.json"))
        return (share, try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any]))
    }

    func testWriterStoresAcceptedFilesUnderItsOwnNames() throws {
        let writer = try XCTUnwrap(ShareInboxWriter(info: info, container: scratch))
        let done = expectation(description: "taken")
        writer.take(try items()) { done.fulfill() }
        wait(for: [done], timeout: 10)
        try writer.finish(title: "Trip", text: "Bring water")
        let (dir, record) = try record(in: scratch)
        XCTAssertEqual(record["v"] as? Int, 1)
        XCTAssertEqual(record["title"] as? String, "Trip")
        XCTAssertEqual(record["text"] as? String, "Bring water")
        XCTAssertEqual(record["url"] as? String, "https://x.test/a")
        let files = try XCTUnwrap(record["files"] as? [[String: String]])
        XCTAssertEqual(files.count, 1)
        XCTAssertEqual(files.first?["file"], "1.txt")
        XCTAssertEqual(files.first?["name"], "packing.txt")
        XCTAssertEqual(files.first?["type"], "text/plain")
        XCTAssertEqual(files.first?["member"], "files")
        XCTAssertEqual(record["dropped"] as? [String: Int], ["files": 1])
        XCTAssertEqual(try String(contentsOf: dir.appendingPathComponent("1.txt"), encoding: .utf8), "water, map")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.appendingPathComponent("share.json.partial").path))
    }

    /// A file over the cap is counted against the member that accepts it and not stored, and a link that is no web
    /// link is not the share's `url`. The share's text is saved without them; with nothing else there is no share.
    func testAFileOverTheCapIsDroppedAndALinkThatIsNoWebLinkIsLeftOut() throws {
        let film = try oversized("film.png")
        let item = NSExtensionItem()
        item.attachments = [NSItemProvider(contentsOf: film)!, link("mailto:a@x.test")]
        XCTAssertThrowsError(try taken([item], title: "Film", in: "cap")) { XCTAssertEqual(($0 as NSError).code, 6) }
        let (dir, record) = try taken([item], title: nil, text: "Tonight", in: "cap-text")
        XCTAssertEqual(record["text"] as? String, "Tonight")
        XCTAssertEqual(record["files"] as? [[String: String]], [])
        XCTAssertEqual(record["dropped"] as? [String: Int], ["files": 1])
        XCTAssertNil(record["url"])
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: dir.path), ["share.json"])
    }

    /// Safari's answer from Exact's page script, as Safari attaches it. Alone, as Safari on iOS 27 shares a page,
    /// or beside a link that is the page's own location, the answer's `url` (the page's canonical one) is the
    /// share's `url` and its title the share's `title`. A sender may spell that location as a browser does not:
    /// the scheme and host in capitals, the default port. The answer is no attachment; a sender's property list
    /// beside it is one, counted as dropped here, where no member accepts it.
    func testPageDetailsAreTheSharesUrlAndTitle() throws {
        for (n, shared) in ["https://x.test/trip?utm_source=feed#top", "HTTPS://X.test:443/trip?utm_source=feed#top", nil].enumerated() {
            let item = NSExtensionItem()
            item.attachments = [page(["url": "https://x.test/trip", "location": "https://x.test/trip?utm_source=feed#top", "title": "Trip notes"])] + (shared.map { [link($0)] } ?? [])
            var (_, record) = try taken([item], title: "x.test", in: "page-\(n)")
            XCTAssertEqual(record["url"] as? String, "https://x.test/trip", "shared \(shared ?? "no link")")
            XCTAssertEqual(record["title"] as? String, "Trip notes")
            XCTAssertEqual(record["dropped"] as? [String: Int], [:])
            item.attachments?.append(NSItemProvider(item: ["a": 1] as NSDictionary, typeIdentifier: UTType.propertyList.identifier))
            (_, record) = try taken([item], title: "x.test", in: "page-list-\(n)")
            XCTAssertEqual(record["url"] as? String, "https://x.test/trip")
            XCTAssertEqual(record["files"] as? [[String: String]], [])
            XCTAssertEqual(record["dropped"] as? [String: Int], ["files": 1])
        }
    }

    /// The answer describes the open page, and a link shared from that page is another location. When a link
    /// beside the answer is not the answer's `location` (its `url`, when it gives no other), the link and the
    /// item's title stand. A link to another place in the page (another fragment) is such a link: the person chose
    /// that place. So is the location with a last slash, which may be another resource. The answer is still
    /// Safari's, so it is not an attachment.
    func testALinkThatIsNotThePageKeepsItsUrlAndTitle() throws {
        let roundup: [String: Any] = ["url": "https://news.test/roundup", "location": "https://news.test/roundup?ref=feed", "title": "Roundup"]
        let article: [String: Any] = ["url": "https://news.test/article", "location": "https://news.test/article#intro", "title": "Article"]
        let shares = [(roundup, "https://news.test/story/5"), (roundup.filter { $0.key != "location" }, "https://news.test/story/5"),
                      (article, "https://news.test/article#results"), (article, "https://news.test/article"),
                      (["url": "https://x.test/canonical", "location": "https://x.test/article", "title": "Article"], "https://x.test/article/")]
        for (n, (answer, shared)) in shares.enumerated() {
            let item = NSExtensionItem()
            item.attachments = [page(answer), link(shared)]
            let (_, record) = try taken([item], title: "Story", in: "link-\(n)")
            XCTAssertEqual(record["url"] as? String, shared, "share \(n)")
            XCTAssertEqual(record["title"] as? String, "Story", "share \(n)")
            XCTAssertEqual(record["files"] as? [[String: String]], [])
            XCTAssertEqual(record["dropped"] as? [String: Int], [:])
        }
    }

    /// A link beside the answer that is no web link is not the page either: the share has no `url`, as it has
    /// none without the answer, and keeps the item's title. With no text either there is no share.
    func testALinkThatIsNoWebLinkIsNotReplacedByThePage() throws {
        let item = NSExtensionItem()
        item.attachments = [page(["url": "https://news.test/story", "location": "https://news.test/story", "title": "Story"]), link("mailto:a@x.test")]
        XCTAssertThrowsError(try taken([item], title: "Write to us", in: "mailto-alone")) { XCTAssertEqual(($0 as NSError).code, 6) }
        let (_, record) = try taken([item], title: "Write to us", text: "Hello", in: "mailto")
        XCTAssertNil(record["url"])
        XCTAssertEqual(record["title"] as? String, "Write to us")
    }

    /// A link that does not load may have been the share itself, so the page does not stand in for it. What else
    /// arrived is saved without a `url`, under the item's title: the share's text, or a picture. With nothing else
    /// the share fails. Beside a link that loads, the share is that link's.
    func testALinkThatDoesNotLoadIsLeftOut() throws {
        let photo = scratch.appendingPathComponent("photo.png")
        try Data([0x89, 0x50, 0x4E, 0x47]).write(to: photo)
        let item = NSExtensionItem()
        item.attachments = [page(["url": "https://news.test/story", "location": "https://news.test/story?ref=feed", "title": "Story"]), failingLink()]
        XCTAssertThrowsError(try taken([item], title: "Other", info: linked, in: "unloaded")) { XCTAssertEqual(($0 as NSError).code, 3) }
        var (_, record) = try taken([item], title: "Other", text: "Read later", in: "unloaded-text")
        XCTAssertNil(record["url"])
        XCTAssertEqual(record["text"] as? String, "Read later")
        XCTAssertEqual(record["title"] as? String, "Other")
        item.attachments?.append(NSItemProvider(contentsOf: photo)!)
        (_, record) = try taken([item], title: "Other", info: linked, in: "unloaded-photo")
        XCTAssertNil(record["url"])
        XCTAssertEqual((record["files"] as? [[String: String]])?.map { $0["name"] }, ["photo.png"])
        XCTAssertEqual(record["title"] as? String, "Other")
        item.attachments?.append(link("https://news.test/other"))
        (_, record) = try taken([item], title: "Other", info: linked, in: "unloaded-beside")
        XCTAssertEqual(record["url"] as? String, "https://news.test/other")
        XCTAssertEqual(record["title"] as? String, "Other")
    }

    /// An empty file one byte over the cap.
    private func oversized(_ name: String) throws -> URL {
        let file = scratch.appendingPathComponent(name)
        XCTAssertTrue(FileManager.default.createFile(atPath: file.path, contents: nil))
        let handle = try FileHandle(forWritingTo: file)
        try handle.truncate(atOffset: UInt64(ShareInboxWriter.cap) + 1)
        try handle.close()
        return file
    }

    /// A link attachment whose load fails.
    private func failingLink() -> NSItemProvider {
        let failing = NSItemProvider()
        failing.registerDataRepresentation(forTypeIdentifier: UTType.url.identifier, visibility: .all) { loaded in
            loaded(nil, NSError(domain: NSCocoaErrorDomain, code: NSFileReadUnknownError))
            return nil
        }
        return failing
    }

    /// The compose sheet's Post cancels the request with the error when the share cannot be written, and leaves
    /// nothing in the inbox: a request it completed would tell the sharing app the share was taken.
    func testComposeSheetCancelsWhenTheShareCannotBeWritten() throws {
        let item = NSExtensionItem()
        item.attachments = [failingLink()]
        let context = StubContext()
        context.items = [item]
        let sheet = ExactShareViewController()
        sheet.info = linked
        sheet.container = scratch
        sheet.stubContext = context
        _ = sheet.view
        #if canImport(UIKit)
        sheet.didSelectPost()
        #else
        sheet.post(nil)
        #endif
        waitFor("the request's end") { context.cancelled != nil || context.completed }
        XCTAssertEqual((context.cancelled as NSError?)?.code, 3)
        XCTAssertFalse(context.completed)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: scratch.appendingPathComponent("shares").path), [])
    }

    /// A page's address may carry a name and a password. Neither reaches the inbox, and the location is compared
    /// without them. A link keeps none either, whether it stands beside an answer or alone.
    func testNoUrlKeepsItsUserinfo() throws {
        for (n, shared) in [nil, "https://x.test/trip?ref=feed"].enumerated() {
            let item = NSExtensionItem()
            item.attachments = [page(["url": "https://alice:s3cret@x.test/trip", "location": "https://alice:s3cret@x.test/trip?ref=feed", "title": "Trip notes"])] + (shared.map { [link($0)] } ?? [])
            let (_, record) = try taken([item], title: "x.test", in: "userinfo-\(n)")
            XCTAssertEqual(record["url"] as? String, "https://x.test/trip", "shared \(shared ?? "no link")")
            XCTAssertEqual(record["title"] as? String, "Trip notes")
        }
        for (n, answer) in [nil, ["url": "https://x.test/article", "title": "Article"]].enumerated() {
            let item = NSExtensionItem()
            item.attachments = (answer.map { [page($0)] } ?? []) + [link("https://alice:s3cret@x.test/article?utm=1#results")]
            let (_, record) = try taken([item], title: "x.test", in: "userinfo-link-\(n)")
            XCTAssertEqual(record["url"] as? String, "https://x.test/article?utm=1#results", "answer \(n)")
            XCTAssertEqual(record["title"] as? String, "x.test")
        }
    }

    /// The share's `url` is the first web link attached, whichever loads first.
    func testTheSharesUrlIsTheFirstWebLinkAttached() throws {
        let late = NSItemProvider()
        late.registerDataRepresentation(forTypeIdentifier: UTType.url.identifier, visibility: .all) { loaded in
            DispatchQueue.global().asyncAfter(deadline: .now() + 0.3) { loaded(Data("https://x.test/first".utf8), nil) }
            return nil
        }
        let item = NSExtensionItem()
        item.attachments = [link("mailto:a@x.test"), late, link("https://x.test/second")]
        XCTAssertEqual(try taken([item], title: nil, in: "order").1["url"] as? String, "https://x.test/first")
    }

    /// The page writes its own answer. One whose `url` is no web link is left out whole, its title too, and what
    /// Safari shares stands; it is still Safari's answer and not a file, while a sender's property list beside it
    /// is stored. A title that is no text is left out alone.
    func testPageDetailsThatAreNoWebLinkAreLeftOut() throws {
        let here = "https://x.test/trip"
        let item = NSExtensionItem()
        item.attachments = [link(here), page(["url": "javascript:alert(1)", "location": here, "title": "Pwned"]),
                            NSItemProvider(item: ["a": 1] as NSDictionary, typeIdentifier: UTType.propertyList.identifier)]
        var (_, record) = try taken([item], title: "x.test", info: linked, in: "no-link")
        XCTAssertEqual(record["url"] as? String, here)
        XCTAssertEqual(record["title"] as? String, "x.test")
        XCTAssertEqual((record["files"] as? [[String: String]])?.map { $0["member"] }, ["bin"])
        XCTAssertEqual(record["dropped"] as? [String: Int], [:])
        item.attachments = [link("\(here)?ref=feed"), page(["url": here, "location": "\(here)?ref=feed", "title": 7])]
        (_, record) = try taken([item], title: "x.test", info: linked, in: "no-title")
        XCTAssertEqual(record["url"] as? String, here)
        XCTAssertEqual(record["title"] as? String, "x.test")
    }

    /// `merge`, row by row: what the links attached and Safari's answer come to, for each title and each set of
    /// mapped members. The page script's half of the table, what it answers for a page, is
    /// host/apple/extensions.test.mjs.
    func testWhatLinksAndThePagesAnswerComeTo() throws {
        let here = "https://x.test/trip?utm=1#top", canon = "https://x.test/trip", root = "https://x.test/"
        // The script's answers. `beside`: a canonical link that differs, with the location beside it. `alone`: one
        // URL, the location, for a page that names no canonical link, the location itself, one that would leave
        // https or one too long; an answer of the same shape gives a canonical link whose location is too long
        // to send. An http `url` beside an https location is the script's to refuse, not the extension's.
        let beside: [String: Any] = ["url": canon, "location": here, "title": "Page"], alone: [String: Any] = ["url": here, "title": "Page"]
        func answer(_ url: Any?, _ location: Any?, _ title: Any? = "Page") -> [String: Any] {
            ["url": url, "location": location, "title": title].compactMapValues { $0 }
        }
        enum Title { case page, item }
        // The links attached (nil for one that did not load), the answer, the `url`, and whose title.
        let rows: [(links: [String?], answer: [String: Any]?, url: String?, title: Title)] = [
            // No answer: the first web link attached, without its userinfo.
            ([], nil, nil, .item), ([canon], nil, canon, .item), (["https://alice:s3cret@x.test/trip"], nil, canon, .item),
            (["mailto:a@x.test"], nil, nil, .item), ([nil], nil, nil, .item), ([nil, "mailto:a@x.test", here, canon], nil, here, .item),
            // The answer alone, or beside links that are each the page's location, however spelled.
            ([], beside, canon, .page), ([here], beside, canon, .page), ([here, here], beside, canon, .page),
            (["HTTPS://X.TEST:443/trip?utm=1#top"], beside, canon, .page), (["https://alice:s3cret@x.test/trip?utm=1#top"], beside, canon, .page),
            ([], alone, here, .page), ([here], alone, here, .page), (["HTTPS://x.test/trip?utm=1#top"], alone, here, .page),
            ([], answer(root, nil), root, .page), (["https://x.test"], answer(root, nil), root, .page),
            (["https://x.test:443/"], answer(canon, "https://x.test"), canon, .page),
            ([], answer("http://x.test/trip", here), "http://x.test/trip", .page),
            // A link that is not the location is the share: the canonical link itself, a last slash, a fragment,
            // a query, the path's case, the scheme, the port or the host apart.
            ([canon], beside, canon, .item), ([canon], alone, canon, .item),
            (["https://x.test/trip/?utm=1#top"], beside, "https://x.test/trip/?utm=1#top", .item), (["https://x.test/trip/?utm=1#top"], alone, "https://x.test/trip/?utm=1#top", .item),
            (["https://x.test/trip/"], answer(canon, nil), "https://x.test/trip/", .item), ([canon], answer("https://x.test/trip/", nil), canon, .item),
            (["https://x.test/trip?utm=1"], beside, "https://x.test/trip?utm=1", .item), (["https://x.test/trip?utm=1#"], beside, "https://x.test/trip?utm=1#", .item),
            (["https://x.test/trip?utm=1#other"], beside, "https://x.test/trip?utm=1#other", .item), ([here], answer(canon, "https://x.test/trip?utm=1"), here, .item),
            (["https://x.test/trip#top"], beside, "https://x.test/trip#top", .item), (["https://x.test/trip?#top"], beside, "https://x.test/trip?#top", .item),
            (["https://x.test/?"], answer(root, nil), "https://x.test/?", .item),
            (["https://x.test/trip?utm=2#top"], beside, "https://x.test/trip?utm=2#top", .item), (["https://x.test/trip?UTM=1#top"], beside, "https://x.test/trip?UTM=1#top", .item),
            (["https://x.test/Trip?utm=1#top"], beside, "https://x.test/Trip?utm=1#top", .item), (["http://x.test/trip?utm=1#top"], beside, "http://x.test/trip?utm=1#top", .item),
            (["https://x.test:8443/trip?utm=1#top"], beside, "https://x.test:8443/trip?utm=1#top", .item), (["https://www.x.test/trip?utm=1#top"], beside, "https://www.x.test/trip?utm=1#top", .item),
            (["https://alice:s3cret@x.test/trip?utm=1#other"], beside, "https://x.test/trip?utm=1#other", .item),
            // One link that is not the location is enough, and the `url` is the first web link attached.
            ([here, "https://x.test/other"], beside, here, .item), (["https://x.test/other", here], beside, "https://x.test/other", .item),
            (["mailto:a@x.test"], beside, nil, .item), ([here, "mailto:a@x.test"], beside, here, .item), (["mailto:a@x.test"], alone, nil, .item),
            // A link that did not load may have been the share: the page does not stand in for it.
            ([nil], beside, nil, .item), ([nil], alone, nil, .item), ([here, nil], beside, here, .item), ([nil, here], beside, here, .item),
            // The page writes its own answer. Its URLs lose their userinfo.
            ([], answer("https://alice:s3cret@x.test/trip", "https://bob:pw@x.test/trip?utm=1#top"), canon, .page),
            ([here], answer("https://alice:s3cret@x.test/trip", "https://bob:pw@x.test/trip?utm=1#top"), canon, .page),
            // A `url` that is no web link leaves the answer out whole.
            ([], answer("javascript:alert(1)", here), nil, .item), ([here], answer("javascript:alert(1)", here), here, .item),
            ([here], answer(7, here), here, .item), ([], answer(nil, here), nil, .item),
            // A `location` that is no web link is one no link can be.
            ([], answer(canon, "javascript:alert(1)"), canon, .page), ([canon], answer(canon, "javascript:alert(1)"), canon, .item),
            ([here], answer(canon, 7), here, .item),
            // A title that is no text, or empty, leaves the item's.
            ([here], answer(canon, here, 7), canon, .item), ([here], answer(canon, here, ""), canon, .item), ([], answer(canon, here, nil), canon, .item),
        ]
        // A page's title and what is kept of it: one over `titleCap` is cut, on a whole character.
        let cap = ShareInboxWriter.titleCap
        let titles = [("Page", "Page"), (String(repeating: "x", count: cap), String(repeating: "x", count: cap)),
                      (String(repeating: "x", count: 2 << 20), String(repeating: "x", count: cap)),
                      (String(repeating: "x", count: cap - 1) + "\u{1F600}", String(repeating: "x", count: cap - 1))]
        for (n, row) in rows.enumerated() {
            for (title, kept) in titles {
                for mapped: Set<String> in [["title", "url"], ["url"], ["title", "text"], []] {
                    var answer = row.answer
                    if answer?["title"] as? String == "Page" { answer?["title"] = title }
                    let got = ShareInboxWriter.merge(links: row.links.map { $0.map { URL(string: $0)! } }, answer: answer, title: "Item", mapped: mapped)
                    XCTAssertEqual(got.url, row.url, "row \(n), mapped \(mapped.sorted())")
                    // Compared as a truth: a megabyte of title is not printed when a row fails.
                    XCTAssertTrue(got.title == (mapped.contains("title") ? (row.title == .page ? kept : "Item") : nil), "row \(n), a title of \(title.utf16.count), mapped \(mapped.sorted())")
                }
            }
        }
        // An item with no title of its own has none unless the page gives one.
        for title in [nil, ""] {
            XCTAssertNil(ShareInboxWriter.merge(links: [], answer: nil, title: title, mapped: ["title"]).title)
            XCTAssertEqual(ShareInboxWriter.merge(links: [], answer: beside, title: title, mapped: ["title"]).title, "Page")
        }
    }

    /// A property list attachment that counts each load as an object, and answers such a load with `object`.
    private final class Counted: NSItemProvider {
        var object: NSDictionary?, decoded = 0
        override func loadItem(forTypeIdentifier typeIdentifier: String, options: [AnyHashable: Any]? = nil, completionHandler: NSItemProvider.CompletionHandler? = nil) {
            decoded += 1
            if let object { completionHandler?(object, nil) } else { super.loadItem(forTypeIdentifier: typeIdentifier, options: options, completionHandler: completionHandler) }
        }
    }

    /// Safari's answer as a nameless property list whose file is a keyed archive, as an item provider writes a
    /// dictionary's, or a `plain` property list; padded to `bytes` when given.
    private func list(_ answer: [String: Any], bytes: Int? = nil, plain: Bool = false) throws -> Counted {
        var pad = 0, data = Data()
        repeat {
            pad += (bytes ?? data.count) - data.count
            let list = [NSExtensionJavaScriptPreprocessingResultsKey: answer, "pad": String(repeating: "x", count: pad)] as NSDictionary
            data = plain ? try PropertyListSerialization.data(fromPropertyList: list, format: .binary, options: 0)
                : try NSKeyedArchiver.archivedData(withRootObject: list, requiringSecureCoding: true)
        } while bytes != nil && data.count != bytes
        let provider = Counted()
        provider.registerDataRepresentation(forTypeIdentifier: UTType.propertyList.identifier, visibility: .all) { [data] loaded in
            loaded(data, nil)
            return nil
        }
        return provider
    }

    /// A property list is read only when its file is small enough to be Safari's answer, which holds two URLs
    /// and a title. Safari's answer, up to `pageCap` bytes, is read and is never a file. One byte larger, even
    /// shaped as an answer, is copied whole to the member that accepts it. Neither is ever loaded as an object:
    /// the extension's memory limit is far below the app's.
    func testAPropertyListIsReadOnlyUpToThePageCap() throws {
        let answer: [String: Any] = ["url": "https://x.test/canonical", "location": "https://x.test/trip", "title": "Trip notes"]
        for (n, size) in [nil, ShareInboxWriter.pageCap, ShareInboxWriter.pageCap + 1].enumerated() {
            let list = try list(answer, bytes: size)
            let item = NSExtensionItem()
            item.attachments = [list, link("https://x.test/trip")]
            let (dir, record) = try taken([item], title: "x.test", info: linked, in: "sized-\(n)")
            let files = try XCTUnwrap(record["files"] as? [[String: String]])
            if n < 2 {
                XCTAssertEqual(record["url"] as? String, "https://x.test/canonical", "size \(n)")
                XCTAssertEqual(record["title"] as? String, "Trip notes", "size \(n)")
                XCTAssertEqual(files, [], "size \(n)")
            } else {
                XCTAssertEqual(record["url"] as? String, "https://x.test/trip")
                XCTAssertEqual(record["title"] as? String, "x.test")
                XCTAssertEqual(files.map { $0["member"] }, ["bin"])
                let stored = try dir.appendingPathComponent(try XCTUnwrap(files.first?["file"])).resourceValues(forKeys: [.fileSizeKey]).fileSize
                XCTAssertEqual(stored, ShareInboxWriter.pageCap + 1)
            }
            XCTAssertEqual(list.decoded, 0, "size \(n)")
            XCTAssertEqual(record["dropped"] as? [String: Int], [:], "size \(n)")
        }
    }

    /// A sender may answer two requests with two representations: a small file, and an object of any size. The
    /// answer is what the file that was read holds, as a keyed archive or a plain property list, and the provider
    /// is never asked for an object.
    func testPageDetailsAreReadFromTheFileThatWasMeasured() throws {
        for (n, plain) in [false, true].enumerated() {
            let list = try list(["url": "https://x.test/file", "title": "File"], plain: plain)
            list.object = [NSExtensionJavaScriptPreprocessingResultsKey: ["url": "https://x.test/object", "title": String(repeating: "x", count: 2 << 20)]]
            let item = NSExtensionItem()
            item.attachments = [list]
            let (_, record) = try taken([item], title: nil, info: linked, in: "two-\(n)")
            XCTAssertEqual(record["url"] as? String, "https://x.test/file", "list \(n)")
            XCTAssertEqual(record["title"] as? String, "File", "list \(n)")
            XCTAssertEqual(record["files"] as? [[String: String]], [], "list \(n)")
            XCTAssertEqual(list.decoded, 0, "list \(n)")
        }
    }

    /// The largest answer the page script gives is one the extension reads: a title of the script's `limit` units
    /// outside ASCII, a canonical link of `limit` and a location of `most` (SharePage.js) come to under `pageCap`.
    func testTheLargestAnswerThePageScriptGivesIsRead() throws {
        let limit = ShareInboxWriter.titleCap, most = (1 << 20) - 4 * limit
        let canonical = "https://x.test/" + String(repeating: "c", count: limit - 15), location = "https://x.test/?" + String(repeating: "a", count: most - 16)
        let item = NSExtensionItem()
        item.attachments = [page(["url": canonical, "location": location, "title": String(repeating: "é", count: limit)]), link(location)]
        let (_, record) = try taken([item], title: nil, info: linked, in: "largest")
        XCTAssertEqual((record["url"] as? String)?.count, limit)
        XCTAssertEqual(record["url"] as? String, canonical)
        XCTAssertEqual((record["title"] as? String)?.utf16.count, limit)
    }

    /// Safari shares a page as the answer alone. One too large to read, here for its title, is the whole share
    /// for an app that keeps no file of it: nothing is written, and the confirmation shows no checkmark and
    /// cancels the request. Text the person shared beside such a list is a share without it: the text is saved,
    /// and the list is counted as dropped.
    func testAPageTooLargeToReadFailsOnlyAShareWithNothingElse() throws {
        let only = ShareTargetInfo(group: "group.com.exact.test", appId: "com.exact.test", mapped: ["title", "text", "url"], members: [(name: "images", types: ["public.image"])])
        let item = NSExtensionItem()
        item.attachments = [page(["url": "https://x.test/trip", "location": "https://x.test/trip", "title": String(repeating: "x", count: 2 << 20)])]
        XCTAssertThrowsError(try taken([item], title: "Trip", info: only, in: "oversized")) { XCTAssertEqual(($0 as NSError).code, 4) }
        let (_, record) = try taken([item], title: nil, text: "Save this", info: only, in: "oversized-text")
        XCTAssertEqual(record["text"] as? String, "Save this")
        XCTAssertNil(record["url"])
        XCTAssertEqual(record["files"] as? [[String: String]], [])
        XCTAssertEqual(record["dropped"] as? [String: Int], ["images": 1])
        let context = StubContext()
        context.items = [item]
        let confirmation = ExactShareConfirmationViewController()
        confirmation.info = only
        confirmation.container = scratch
        confirmation.stubContext = context
        _ = confirmation.view
        appear(confirmation)
        waitFor("the checkmark or a cancel") { confirmation.confirmed || context.cancelled != nil }
        XCTAssertFalse(confirmation.confirmed)
        XCTAssertFalse(context.completed)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: scratch.appendingPathComponent("shares").path), [])
    }

    /// Only Safari's answer is read as one. Any other property list is an attachment like another: the member that
    /// accepts it stores it, and is the one charged when it cannot be copied. Here, beside Safari's answer, a binary
    /// property list given as a nameless file, a dictionary given as an object, and a file whose load fails.
    func testAPropertyListThatIsNotSafarisAnswerGoesToTheMemberThatAcceptsIt() throws {
        let plist = try PropertyListSerialization.data(fromPropertyList: ["a": 1], format: .binary, options: 0)
        let source = scratch.appendingPathComponent("settings")
        try plist.write(to: source)
        func file(_ loaded: URL?) -> NSItemProvider {
            let provider = NSItemProvider()
            provider.registerFileRepresentation(forTypeIdentifier: "com.apple.binary-property-list", fileOptions: [], visibility: .all) { done in
                done(loaded, false, loaded == nil ? NSError(domain: NSCocoaErrorDomain, code: NSFileReadUnknownError) : nil)
                return nil
            }
            return provider
        }
        let item = NSExtensionItem()
        item.attachments = [file(source), NSItemProvider(item: ["a": 1] as NSDictionary, typeIdentifier: UTType.propertyList.identifier), file(nil),
                            page(["url": "https://x.test/trip", "title": "Trip notes"])]
        let (dir, record) = try taken([item], title: nil, info: linked, in: "lists")
        let files = try XCTUnwrap(record["files"] as? [[String: String]])
        XCTAssertEqual(files.map { $0["member"] }, ["bin", "bin"])
        XCTAssertEqual(try Data(contentsOf: dir.appendingPathComponent(try XCTUnwrap(files.first?["file"]))), plist)
        XCTAssertEqual(record["dropped"] as? [String: Int], ["bin": 1])
        XCTAssertEqual(record["url"] as? String, "https://x.test/trip")
        XCTAssertEqual(record["title"] as? String, "Trip notes")
    }

    /// An app that maps `url` and `title`, with a member for pictures and one for any other file.
    private let linked = ShareTargetInfo(group: "group.com.exact.test", appId: "com.exact.test", mapped: ["title", "url"],
                                         members: [(name: "images", types: ["public.image"]), (name: "bin", types: ["public.data"])])

    private func page(_ answer: [String: Any]) -> NSItemProvider {
        NSItemProvider(item: [NSExtensionJavaScriptPreprocessingResultsKey: answer] as NSDictionary, typeIdentifier: UTType.propertyList.identifier)
    }

    private func link(_ url: String) -> NSItemProvider { NSItemProvider(object: URL(string: url)! as NSURL) }

    /// The share's directory and record a writer leaves for `items`, with the item's `title` and the sheet's `text`,
    /// in a container of its own.
    private func taken(_ items: [NSExtensionItem], title: String?, text: String? = nil, info: ShareTargetInfo? = nil, in name: String) throws -> (URL, [String: Any]) {
        let container = scratch.appendingPathComponent(name, isDirectory: true)
        let writer = try XCTUnwrap(ShareInboxWriter(info: info ?? self.info, container: container))
        let done = expectation(description: "taken")
        writer.take(items) { done.fulfill() }
        wait(for: [done], timeout: 10)
        try writer.finish(title: title, text: text)
        return try record(in: container)
    }

    func testDiscardLeavesNothing() throws {
        let writer = try XCTUnwrap(ShareInboxWriter(info: info, container: scratch))
        writer.discard()
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: scratch.appendingPathComponent("shares").path), [])
    }

    /// `presentation: "confirmation"`: appearing writes the inbox with the item's own text, with no tap, shows the
    /// checkmark, and then completes the request.
    func testConfirmationSavesOnAppearAndCompletes() throws {
        let context = StubContext()
        context.items = try items()
        let confirmation = ExactShareConfirmationViewController()
        confirmation.info = info
        confirmation.container = scratch
        confirmation.stubContext = context
        _ = confirmation.view
        XCTAssertFalse(confirmation.confirmed)
        appear(confirmation)
        waitFor("the checkmark") { confirmation.confirmed }
        // The share is whole when the checkmark shows, and the request is still open under it.
        XCTAssertFalse(context.completed)
        let (dir, record) = try record(in: scratch)
        XCTAssertEqual(record["title"] as? String, "Trip")
        XCTAssertEqual(record["text"] as? String, "Bring water")
        XCTAssertEqual(record["url"] as? String, "https://x.test/a")
        XCTAssertEqual((record["files"] as? [[String: String]])?.map { $0["file"] }, ["1.txt"])
        XCTAssertEqual(record["dropped"] as? [String: Int], ["files": 1])
        XCTAssertEqual(try String(contentsOf: dir.appendingPathComponent("1.txt"), encoding: .utf8), "water, map")
        waitFor("the request's completion") { context.completed }
        XCTAssertNil(context.cancelled)
    }

    /// A sender may give its text only as an attachment (a string's item provider, no `attributedContentText`).
    /// The confirmation waits for that text, however late it loads, before the checkmark, and the record holds it.
    func testConfirmationTakesTextOnlyAProviderCarries() throws {
        let late = NSItemProvider()
        late.registerDataRepresentation(forTypeIdentifier: UTType.utf8PlainText.identifier, visibility: .all) { loaded in
            DispatchQueue.global().asyncAfter(deadline: .now() + 0.3) { loaded(Data("Save this".utf8), nil) }
            return nil
        }
        for (n, provider) in [NSItemProvider(object: "Save this" as NSString), late].enumerated() {
            let container = scratch.appendingPathComponent("text-\(n)", isDirectory: true)
            let item = NSExtensionItem()
            item.attachments = [provider]
            let context = StubContext()
            context.items = [item]
            let confirmation = ExactShareConfirmationViewController()
            confirmation.info = info
            confirmation.container = container
            confirmation.stubContext = context
            _ = confirmation.view
            appear(confirmation)
            waitFor("the checkmark") { confirmation.confirmed }
            let (_, record) = try record(in: container)
            XCTAssertEqual(record["text"] as? String, "Save this", "provider \(n)")
            XCTAssertEqual(record["files"] as? [[String: String]], [])
            XCTAssertEqual(record["dropped"] as? [String: Int], [:])
        }
    }

    #if canImport(UIKit)
    /// Once the share is in the inbox the controller posts one notification, a layout change whose argument is the
    /// app's name, a label on the sheet: that moves VoiceOver to it, and UIKit reads a label by its text. The
    /// checkmark is decorative: nothing else on the sheet is an accessibility element. (A test process has no
    /// accessibility runtime, so the label does not report itself as one here.)
    func testConfirmationMovesVoiceOverToTheAppName() throws {
        let context = StubContext()
        context.items = try items()
        let confirmation = ExactShareConfirmationViewController()
        confirmation.info = info
        confirmation.container = scratch
        confirmation.stubContext = context
        confirmation.name = "Fieldnotes"
        var posted: [(notification: UIAccessibility.Notification, argument: Any?)] = []
        confirmation.postAccessibility = { posted.append(($0, $1)) }
        _ = confirmation.view
        appear(confirmation)
        waitFor("the checkmark") { confirmation.confirmed }
        XCTAssertEqual(posted.map(\.notification), [.layoutChanged])
        let announced = try XCTUnwrap(posted.first?.argument as? UILabel)
        XCTAssertEqual(announced.text, "Fieldnotes")
        XCTAssertTrue(announced.isDescendant(of: confirmation.view))
        func elements(_ view: UIView) -> [UIView] {
            view.isHidden ? [] : (view.isAccessibilityElement ? [view] : []) + view.subviews.flatMap(elements)
        }
        XCTAssertEqual(elements(confirmation.view).filter { $0 !== announced }, [])
    }
    #else
    /// Once the share is in the inbox the controller posts one notification: an announcement of the app's name, for
    /// the application, the element AppKit's header gives an announcement (NSAccessibilityConstants.h).
    func testConfirmationAnnouncesTheAppNameOnTheApplication() throws {
        let context = StubContext()
        context.items = try items()
        let confirmation = ExactShareConfirmationViewController()
        confirmation.info = info
        confirmation.container = scratch
        confirmation.stubContext = context
        confirmation.name = "Fieldnotes"
        var posted: [(element: Any, notification: NSAccessibility.Notification, userInfo: [NSAccessibility.NotificationUserInfoKey: Any])] = []
        confirmation.postAccessibility = { posted.append(($0, $1, $2)) }
        _ = confirmation.view
        appear(confirmation)
        waitFor("the checkmark") { confirmation.confirmed }
        XCTAssertEqual(posted.map(\.notification), [.announcementRequested])
        XCTAssertTrue(posted.first?.element as AnyObject === NSApplication.shared)
        XCTAssertEqual(posted.first?.userInfo[.announcement] as? String, "Fieldnotes")
        XCTAssertEqual(posted.first?.userInfo[.priority] as? Int, NSAccessibilityPriorityLevel.high.rawValue)
        // The checkmark is decorative, and AppKit reads its symbol as "selected": the name is the only element
        // VoiceOver finds. A control's cell is its element, so the cells are what is asked.
        func elements(_ node: Any) -> [NSAccessibilityProtocol] {
            NSAccessibility.unignoredChildren(from: (node as? NSAccessibilityProtocol)?.accessibilityChildren() ?? [])
                .compactMap { $0 as? NSAccessibilityProtocol }.flatMap { [$0] + elements($0) }
        }
        let found = elements(confirmation.view)
        XCTAssertEqual(found.map { $0.accessibilityRole() }, [.staticText])
        XCTAssertEqual(found.first?.accessibilityValue() as? String, "Fieldnotes")
    }
    #endif

    /// An inbox that cannot be written shows no checkmark and cancels the request with the reason. Only the
    /// checkmark schedules a completion, so none is pending once the request is cancelled.
    func testConfirmationCancelsWhenTheInboxCannotBeWritten() throws {
        let blocked = scratch.appendingPathComponent("blocked")
        try Data().write(to: blocked)
        let context = StubContext()
        context.items = try items()
        let confirmation = ExactShareConfirmationViewController()
        confirmation.info = info
        confirmation.container = blocked
        confirmation.stubContext = context
        _ = confirmation.view
        appear(confirmation)
        waitFor("the request's cancel") { context.cancelled != nil }
        XCTAssertFalse(confirmation.confirmed)
        XCTAssertFalse(context.completed)
    }

    /// A text attachment that fails to load is not a share with no text: nothing is left in the inbox, there is no
    /// checkmark, and the request is cancelled.
    func testConfirmationCancelsWhenTheSharesTextCannotBeLoaded() throws {
        let item = NSExtensionItem()
        item.attachments = [failingText()]
        let context = StubContext()
        context.items = [item]
        let confirmation = ExactShareConfirmationViewController()
        confirmation.info = info
        confirmation.container = scratch
        confirmation.stubContext = context
        _ = confirmation.view
        appear(confirmation)
        waitFor("the request's cancel") { context.cancelled != nil }
        XCTAssertFalse(confirmation.confirmed)
        XCTAssertFalse(context.completed)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: scratch.appendingPathComponent("shares").path), [])
    }

    /// A share of which nothing can be saved shows no checkmark: the request is cancelled with the reason and
    /// nothing is left in the inbox. A title alone is no share. Here a picture that does not load, one over the
    /// cap, a file no member accepts, and, for an app that maps only `text`, where the claim puts a link, a link
    /// that does not load. What arrives beside any of them is saved without it.
    func testConfirmationCancelsWhenNothingOfTheShareIsSaved() throws {
        let photo = scratch.appendingPathComponent("photo.png"), odd = scratch.appendingPathComponent("data.bin")
        try Data([0x89, 0x50, 0x4E, 0x47]).write(to: photo)
        try Data([0, 1, 2, 3]).write(to: odd)
        let unloaded = NSItemProvider()
        unloaded.registerDataRepresentation(forTypeIdentifier: UTType.png.identifier, visibility: .all) { loaded in
            loaded(nil, NSError(domain: NSCocoaErrorDomain, code: NSFileReadUnknownError))
            return nil
        }
        let pictures = ShareTargetInfo(group: "group.com.exact.test", appId: "com.exact.test", mapped: ["title"], members: [(name: "images", types: ["public.image"])])
        let words = ShareTargetInfo(group: "group.com.exact.test", appId: "com.exact.test", mapped: ["text"], members: [])
        let lost: [(what: String, attachment: NSItemProvider, info: ShareTargetInfo, code: Int)] = [
            ("a picture that does not load", unloaded, pictures, 6), ("a picture over the cap", NSItemProvider(contentsOf: try oversized("film.png"))!, pictures, 6),
            ("a file no member accepts", NSItemProvider(contentsOf: odd)!, pictures, 6), ("a link that does not load", failingLink(), words, 3),
        ]
        for (n, share) in lost.enumerated() {
            for beside in [false, true] {
                let container = scratch.appendingPathComponent("lost-\(n)-\(beside)", isDirectory: true)
                let item = NSExtensionItem()
                item.attributedTitle = NSAttributedString(string: "Trip")
                item.attachments = [share.attachment] + (beside ? [share.code == 3 ? link("https://x.test/a") : NSItemProvider(contentsOf: photo)!] : [])
                let context = StubContext()
                context.items = [item]
                let confirmation = ExactShareConfirmationViewController()
                confirmation.info = share.info
                confirmation.container = container
                confirmation.stubContext = context
                _ = confirmation.view
                appear(confirmation)
                waitFor("the checkmark or a cancel") { confirmation.confirmed || context.cancelled != nil }
                XCTAssertEqual(confirmation.confirmed, beside, share.what)
                if beside {
                    let (_, record) = try record(in: container)
                    XCTAssertEqual(record["url"] as? String, share.code == 3 ? "https://x.test/a" : nil, share.what)
                    XCTAssertEqual((record["files"] as? [[String: String]])?.map { $0["name"] }, share.code == 3 ? [] : ["photo.png"], share.what)
                    XCTAssertEqual(record["dropped"] as? [String: Int], share.code == 3 ? [:] : ["images": 1], share.what)
                } else {
                    XCTAssertEqual((context.cancelled as NSError?)?.code, share.code, share.what)
                    XCTAssertFalse(context.completed, share.what)
                    XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: container.appendingPathComponent("shares").path), [], share.what)
                }
            }
        }
    }

    /// Text only an attachment carries is loaded whole, so its size is read first: up to `textCap` bytes it is the
    /// share's text, and one byte more fails the share, with nothing in the inbox and no checkmark.
    func testConfirmationCancelsWhenTheSharesTextIsTooLarge() throws {
        for (n, size) in [ShareInboxWriter.textCap, ShareInboxWriter.textCap + 1].enumerated() {
            let container = scratch.appendingPathComponent("long-\(n)", isDirectory: true)
            let long = NSItemProvider()
            long.registerDataRepresentation(forTypeIdentifier: UTType.utf8PlainText.identifier, visibility: .all) { loaded in
                loaded(Data(repeating: 0x61, count: size), nil)
                return nil
            }
            let item = NSExtensionItem()
            item.attachments = [long]
            let context = StubContext()
            context.items = [item]
            let confirmation = ExactShareConfirmationViewController()
            confirmation.info = info
            confirmation.container = container
            confirmation.stubContext = context
            _ = confirmation.view
            appear(confirmation)
            waitFor("the checkmark or a cancel") { confirmation.confirmed || context.cancelled != nil }
            XCTAssertEqual(confirmation.confirmed, n == 0, "size \(n)")
            if n == 0 {
                XCTAssertEqual((try record(in: container).1["text"] as? String)?.utf8.count, size)
            } else {
                XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: container.appendingPathComponent("shares").path), [])
            }
        }
    }

    /// A sender may answer two requests with two representations: a small file, and a string of any size. The
    /// share's text is what the file that was read holds, and the provider is never asked for the string, which
    /// here is over the cap.
    func testConfirmationReadsItsTextFromTheFileThatWasMeasured() throws {
        let two = NSItemProvider()
        two.registerDataRepresentation(forTypeIdentifier: UTType.plainText.identifier, visibility: .all) { loaded in
            loaded(Data("Save this".utf8), nil)
            return nil
        }
        two.registerObject(String(repeating: "b", count: ShareInboxWriter.textCap + 1) as NSString, visibility: .all)
        let item = NSExtensionItem()
        item.attachments = [two]
        let context = StubContext()
        context.items = [item]
        let confirmation = ExactShareConfirmationViewController()
        confirmation.info = info
        confirmation.container = scratch
        confirmation.stubContext = context
        _ = confirmation.view
        appear(confirmation)
        waitFor("the checkmark or a cancel") { confirmation.confirmed || context.cancelled != nil }
        XCTAssertNil(context.cancelled)
        XCTAssertEqual(try record(in: scratch).1["text"] as? String, "Save this")
    }

    /// Text the app does not map is not loaded, so a text attachment that fails to load cannot cost the share its
    /// picture.
    func testConfirmationKeepsFilesWhenTextTheAppDoesNotMapFails() throws {
        let photo = scratch.appendingPathComponent("photo.png")
        try Data([0x89, 0x50, 0x4E, 0x47]).write(to: photo)
        let item = NSExtensionItem()
        item.attachments = [NSItemProvider(contentsOf: photo)!, failingText()]
        let context = StubContext()
        context.items = [item]
        let confirmation = ExactShareConfirmationViewController()
        confirmation.info = ShareTargetInfo(group: "group.com.exact.test", appId: "com.exact.test", mapped: ["title"], members: [(name: "images", types: ["public.image"])])
        confirmation.container = scratch
        confirmation.stubContext = context
        _ = confirmation.view
        appear(confirmation)
        waitFor("the checkmark or a cancel") { confirmation.confirmed || context.cancelled != nil }
        XCTAssertNil(context.cancelled)
        let (_, record) = try record(in: scratch)
        XCTAssertEqual((record["files"] as? [[String: String]])?.map { $0["name"] }, ["photo.png"])
        XCTAssertNil(record["text"])
        XCTAssertEqual(record["dropped"] as? [String: Int], [:])
    }

    /// A plain-text attachment whose load fails.
    private func failingText() -> NSItemProvider {
        let failing = NSItemProvider()
        failing.registerDataRepresentation(forTypeIdentifier: UTType.utf8PlainText.identifier, visibility: .all) { loaded in
            loaded(nil, NSError(domain: NSCocoaErrorDomain, code: NSFileReadUnknownError))
            return nil
        }
        return failing
    }

    /// The person may dismiss the sheet while a file is still copying. Nothing is kept, so the app is told of no
    /// share, and the request ends cancelled: the copy that lands afterwards writes no record and shows no checkmark.
    func testConfirmationDismissedDuringTheCopyKeepsNothing() throws {
        let photo = scratch.appendingPathComponent("photo.png")
        try Data([0x89, 0x50, 0x4E, 0x47]).write(to: photo)
        // The copy's completion, held until the test releases it.
        final class Held { let lock = NSLock(); var done: ((URL?, Bool, Error?) -> Void)? }
        let held = Held()
        let slow = NSItemProvider()
        slow.registerFileRepresentation(forTypeIdentifier: UTType.png.identifier, fileOptions: [], visibility: .all) { done in
            held.lock.withLock { held.done = done }
            return nil
        }
        let item = NSExtensionItem()
        item.attributedContentText = NSAttributedString(string: "Bring water")
        item.attachments = [slow]
        let context = StubContext()
        context.items = [item]
        let confirmation = ExactShareConfirmationViewController()
        confirmation.info = info
        confirmation.container = scratch
        confirmation.stubContext = context
        _ = confirmation.view
        appear(confirmation)
        waitFor("the copy's start") { held.lock.withLock { held.done != nil } }
        disappear(confirmation)
        XCTAssertEqual((context.cancelled as NSError?)?.code, NSUserCancelledError)
        held.lock.withLock { held.done }?(photo, false, nil)
        waitFor("the save's end") { confirmation.saveEnded }
        XCTAssertFalse(confirmation.confirmed)
        XCTAssertFalse(context.completed)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: scratch.appendingPathComponent("shares").path), [])
    }

    /// Dismissed under the checkmark, the share is already in the inbox: it stays, and the request completes at
    /// once and not as cancelled.
    func testConfirmationDismissedUnderTheCheckmarkKeepsTheShare() throws {
        let context = StubContext()
        context.items = try items()
        let confirmation = ExactShareConfirmationViewController()
        confirmation.info = info
        confirmation.container = scratch
        confirmation.stubContext = context
        _ = confirmation.view
        appear(confirmation)
        waitFor("the checkmark") { confirmation.confirmed }
        XCTAssertFalse(context.completed)
        disappear(confirmation)
        XCTAssertTrue(context.completed)
        XCTAssertNil(context.cancelled)
        XCTAssertEqual(try record(in: scratch).1["text"] as? String, "Bring water")
    }

    #if canImport(UIKit)
    /// In a window, a share that cannot be saved presents an alert. The request stays open under it, so the person
    /// can read the reason, and its button cancels the request with the failure. (A test process has no scene, so
    /// no presentation runs to its end here: what UIKit tells the controller once the alert is up is not seen.)
    func testConfirmationsAlertKeepsTheRequestOpenUntilItsButton() throws {
        let item = NSExtensionItem()
        item.attachments = [failingText()]
        let context = StubContext()
        context.items = [item]
        let confirmation = ExactShareConfirmationViewController()
        confirmation.info = info
        confirmation.container = scratch
        confirmation.stubContext = context
        let scene = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        window.rootViewController = confirmation
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        appear(confirmation)
        waitFor("the alert") { confirmation.presentedViewController is UIAlertController || context.cancelled != nil }
        let alert = try XCTUnwrap(confirmation.presentedViewController as? UIAlertController)
        XCTAssertNil(context.cancelled)
        XCTAssertFalse(context.completed)
        // UIKit keeps an action's handler to itself; a tap on the button runs it.
        typealias Handler = @convention(block) (UIAlertAction) -> Void
        let button = try XCTUnwrap(alert.actions.first)
        XCTAssertEqual(alert.actions.count, 1)
        unsafeBitCast(try XCTUnwrap(button.value(forKey: "handler")) as AnyObject, to: Handler.self)(button)
        XCTAssertNotNil(context.cancelled)
        XCTAssertFalse(context.completed)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: scratch.appendingPathComponent("shares").path), [])
    }

    private func appear(_ controller: UIViewController) { controller.viewDidAppear(false) }
    private func disappear(_ controller: UIViewController) { controller.viewDidDisappear(false) }
    #else
    private func appear(_ controller: NSViewController) { controller.viewDidAppear() }
    private func disappear(_ controller: NSViewController) { controller.viewDidDisappear() }
    #endif

    #if canImport(AppKit) && !targetEnvironment(macCatalyst)
    /// The Mac's sheet, through a stub context: Post writes the inbox with the sheet's edited text and completes.
    func testMacSheetPostsThroughTheContext() throws {
        let context = StubContext()
        context.items = try items()
        let sheet = ExactShareViewController()
        sheet.info = info
        sheet.container = scratch
        sheet.stubContext = context
        _ = sheet.view
        sheet.viewDidLoad()
        XCTAssertEqual(sheet.text, "Bring water")
        sheet.text = "Bring water and a hat"
        sheet.post(nil)
        waitFor("the request's completion") { context.completed }
        let (_, record) = try record(in: scratch)
        XCTAssertEqual(record["text"] as? String, "Bring water and a hat")
        XCTAssertEqual((record["files"] as? [[String: String]])?.count, 1)
    }

    /// The Mac's sheet ends the request as cancelled, with nothing in the inbox, on Cancel and when a Post finds no
    /// container.
    func testMacSheetCancelsOnCancelAndWithoutAContainer() throws {
        for (n, withContainer) in [true, false].enumerated() {
            let container = scratch.appendingPathComponent("sheet-\(n)", isDirectory: true)
            let context = StubContext()
            context.items = try items()
            let sheet = ExactShareViewController()
            sheet.info = withContainer ? info : nil
            sheet.container = container
            sheet.stubContext = context
            _ = sheet.view
            if withContainer { sheet.cancel(nil) } else { sheet.post(nil) }
            XCTAssertEqual((context.cancelled as NSError?)?.code, withContainer ? NSUserCancelledError : 1)
            XCTAssertFalse(context.completed)
            XCTAssertFalse(FileManager.default.fileExists(atPath: container.path))
        }
    }
    #endif
}
