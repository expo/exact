#if !os(tvOS)
import XCTest
@testable import ExactKit

/// The content index's mirror and the door's answers with no session (LLP 1133.023 D4, D9, D7).
final class ContentIndexTests: XCTestCase {
    private var directory: URL!

    override func setUp() {
        super.setUp()
        directory = FileManager.default.temporaryDirectory.appendingPathComponent("content-index-\(UUID().uuidString)")
        ExactContentIndex.file = directory.appendingPathComponent("exact-content-index.json")
        ExactTools.install()
    }
    override func tearDown() {
        try? FileManager.default.removeItem(at: directory)
        super.tearDown()
    }

    private func book(_ id: String, _ title: String, _ author: String) -> [String: JSONValue] {
        JSONValue.fields(["id": id, "title": title, "description": author, "url": "/book/\(id)"])
    }

    /// What the generated entity query posts, answered by the host with no session booted.
    private func query(_ kind: String, ids: [String]? = nil, matching: String? = nil) -> [String] {
        let body = NSMutableDictionary(dictionary: ["kind": kind])
        if let ids { body["ids"] = ids }
        if let matching { body["matching"] = matching }
        NotificationCenter.default.post(name: Notification.Name("exact.contentIndex.query"), object: body)
        return (body["entries"] as? [[String: Any]] ?? []).compactMap { $0["id"] as? String }
    }

    func testTheEntityQueryAnswersFromTheMirrorWithNoSession() {
        XCTAssertNil(ExactContentIndex.set(kind: "book", [book("1", "Piranesi", "Susanna Clarke"), book("2", "Dune", "Frank Herbert")]))
        // Suggested: the app's order. By id: the ids asked, in their order.
        XCTAssertEqual(query("book"), ["1", "2"])
        XCTAssertEqual(query("book", ids: ["2", "9", "1"]), ["2", "1"])
        // Matching: title or description, case- and diacritic-insensitive.
        XCTAssertEqual(query("book", matching: "HÉR"), ["2"])
        XCTAssertEqual(query("book", matching: "piran"), ["1"])
        XCTAssertEqual(query("author"), [])
        // A fresh read of the file, as the next launch makes, answers the same.
        ExactContentIndex.file = ExactContentIndex.file
        XCTAssertEqual(query("book", matching: "dune"), ["2"])
    }

    func testSetReplacesOneKindAndKeepsTheOthers() {
        XCTAssertNil(ExactContentIndex.set(kind: "book", [book("1", "Piranesi", "Susanna Clarke")]))
        XCTAssertNil(ExactContentIndex.set(kind: "shelf", [book("s", "Reading", "")]))
        XCTAssertNil(ExactContentIndex.set(kind: "book", [book("2", "Dune", "Frank Herbert")]))
        XCTAssertEqual(query("book"), ["2"])
        XCTAssertEqual(query("shelf"), ["s"])
        XCTAssertEqual(ExactContentIndex.location(ofItem: "book:2"), "/book/2")
        XCTAssertNil(ExactContentIndex.location(ofItem: "book:1"), "a removed entry opens the app at its start")
        ExactContentIndex.delete(id: "2")
        XCTAssertEqual(query("book"), [])
    }

    func testRefusalsChangeNothing() {
        XCTAssertNil(ExactContentIndex.set(kind: "book", [book("1", "Piranesi", "Susanna Clarke")]))
        XCTAssertEqual(ExactContentIndex.set(kind: "book", [JSONValue.fields(["id": "x", "title": "Away", "url": "https://example.com"])]), "TypeError")
        let many = (0...ExactContentIndex.most).map { book("\($0)", "Book \($0)", "") }
        XCTAssertEqual(ExactContentIndex.set(kind: "book", many), "QuotaExceededError")
        let long = book("l", String(repeating: "a", count: ExactContentIndex.textBytes), "")
        XCTAssertEqual(ExactContentIndex.add(long), "QuotaExceededError")
        XCTAssertEqual(query("book"), ["1"])
    }

    #if os(macOS)
    /// The Dock menu: the app's list after the manifest's (none in a test bundle), each opening its location (D3, D17).
    func testTheDockMenuHasTheAppsList() {
        let kept = UserDefaults.standard.data(forKey: "exact.shortcuts")
        defer { UserDefaults.standard.set(kept, forKey: "exact.shortcuts") }
        ExactShortcuts.set([["name": "Continue Piranesi", "url": "/book/seed-2"], ["name": "Away", "url": "https://example.com"]])
        let menu = ExactShortcuts.dockMenu()
        XCTAssertEqual(menu?.items.map(\.title), ["Continue Piranesi"], "an entry that is not an app path is left out")
        XCTAssertEqual(menu?.items.first?.representedObject as? String, "/book/seed-2")
        ExactShortcuts.set([])
        XCTAssertNil(ExactShortcuts.dockMenu())
    }
    #endif

    func testACallWithNoSessionIsRefusedAtTheDoor() {
        let refused = expectation(description: "the door answers")
        let body = NSMutableDictionary(dictionary: ["tool": "logPages", "args": ["page": 1], "door": "apple", "digest": ""])
        body["reply"] = { (answer: [String: Any]) in
            XCTAssertEqual(answer["outcome"] as? String, "refused")
            XCTAssertEqual(answer["certainty"] as? String, "none")
            refused.fulfill()
        }
        NotificationCenter.default.post(name: Notification.Name("exact.tools.call"), object: body)
        XCTAssertEqual(body["taken"] as? Bool, true)
        wait(for: [refused], timeout: 5)
    }

    /// A task the system cancelled before its intent posted the call posts
    /// the cancellation first (`withTaskCancellationHandler`), from any
    /// thread: the call never reaches a session and is refused `cancelled`.
    func testACallItsCallerCancelledFirstIsRefusedCancelled() {
        let refused = expectation(description: "the door answers")
        let body = NSMutableDictionary(dictionary: ["tool": "logPages", "args": ["page": 1], "door": "apple", "digest": ""])
        body["reply"] = { (answer: [String: Any]) in
            XCTAssertEqual(answer["outcome"] as? String, "refused")
            XCTAssertEqual(answer["certainty"] as? String, "none")
            XCTAssertEqual(answer["reason"] as? String, "cancelled")
            refused.fulfill()
        }
        let posted = expectation(description: "the cancellation is posted")
        DispatchQueue.global().async {
            NotificationCenter.default.post(name: Notification.Name("exact.tools.cancel"), object: body)
            posted.fulfill()
        }
        wait(for: [posted], timeout: 5)
        NotificationCenter.default.post(name: Notification.Name("exact.tools.call"), object: body)
        wait(for: [refused], timeout: 5)
    }

    /// Call B waits in the runner behind call A when its task is cancelled,
    /// and A ends before B's next poll. The cancellation reaches the runner
    /// as the notification is handled, so B is dropped, not dispatched as A
    /// ends. The runner here is a stand-in that keeps B waiting until A ends
    /// and then dispatches it unless it was told to cancel it first.
    func testACallCancelledWhileItWaitsInTheRunnerIsDroppedBeforeTheCallAheadEnds() {
        var aRunning = true, bCancelled = false, bDispatched = false, ops: [String] = []
        let agent = { (text: String) -> String in
            let request = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any] ?? [:]
            let op = request["op"] as? String ?? ""
            ops.append(op)
            if op == "cancel" {
                if aRunning { bCancelled = true }
                return #"{"call":2,"state":"ended"}"#
            }
            if aRunning { return #"{"call":2,"state":"waiting"}"# }
            if bCancelled { return #"{"outcome":"refused","certainty":"none","reason":"cancelled","message":"Its caller cancelled it before it ran."}"# }
            bDispatched = true
            return #"{"outcome":"ok","certainty":"applied","reason":""}"#
        }
        let cancellation = ExactTools.Cancellation()
        let body = NSMutableDictionary(dictionary: ["cancellation": cancellation])
        let answered = expectation(description: "B is answered")
        ExactTools.follow(2, agent: agent, cancellation: cancellation) { answer in
            XCTAssertEqual(answer["reason"] as? String, "cancelled")
            answered.fulfill()
        }
        XCTAssertEqual(ops, ["called"])
        let handled = expectation(description: "the cancellation is handled on the main thread")
        DispatchQueue.global().async {
            NotificationCenter.default.post(name: Notification.Name("exact.tools.cancel"), object: body)
            DispatchQueue.main.async { handled.fulfill() }
        }
        wait(for: [handled], timeout: 5)
        XCTAssertEqual(ops.last, "cancel", "told as the notification is handled, not at the next poll")
        aRunning = false
        wait(for: [answered], timeout: 5)
        XCTAssertFalse(bDispatched)
    }
}
#endif
