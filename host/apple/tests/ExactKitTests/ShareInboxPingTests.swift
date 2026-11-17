import Foundation
import XCTest
@testable import ExactKit

// The share extension's ping (LLP 1133.024 D4) goes to one session that
// can claim the inbox: the newest that began to listen and is not
// destroyed, so a session ending leaves the ping with the one before it.
final class ShareInboxPingTests: XCTestCase {
    func testAPingGoesToTheNewestSessionNotDestroyed() {
        let ping = ShareInboxPing()
        // A name nothing posts: this listener outlives the test.
        let name = "test.\(UUID().uuidString).shares"
        let a = ExactApp.shared.makeSession(label: "share-a")
        defer { a.destroy() }
        var b: ExactSession? = ExactApp.shared.makeSession(label: "share-b")
        ping.listen(name, for: a)
        ping.listen(name, for: b!)
        XCTAssertTrue(ping.recipient === b)
        b?.destroy()
        XCTAssertTrue(ping.recipient === a, "a destroyed session's ping goes to the one still live")
        b = nil
        XCTAssertTrue(ping.recipient === a, "a released session's ping goes to the one still live")
        let c = ExactApp.shared.makeSession(label: "share-c")
        defer { c.destroy() }
        ping.listen(name, for: c)
        ping.listen(name, for: a)
        XCTAssertTrue(ping.recipient === a, "listening again makes a session the newest")
        a.destroy()
        XCTAssertTrue(ping.recipient === c)
        c.destroy()
        XCTAssertNil(ping.recipient)
    }
}
