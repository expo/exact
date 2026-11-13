import Foundation
import XCTest
#if os(iOS)
import UIKit
private typealias TestApp = UIApplication
#elseif os(macOS)
import AppKit
private typealias TestApp = NSApplication
#endif
@testable import ExactKit

/// The system's answers to a registration, forwarded to ExactKit from any
/// thread, are kept app-wide and announced on the main thread
/// (LLP 1133.004 §9 P3).
final class PushRegistrationTests: XCTestCase {
    override func setUp() {
        super.setUp()
        PushRegistration.reset()
        PushRegistration.listen()
    }

    /// What `@exact/push` reads through its context.
    private func read() -> [String: Any] {
        let box = NSMutableDictionary(dictionary: ["op": "read"])
        NotificationCenter.default.post(name: Notification.Name("exact.push"), object: box)
        return box["reply"] as? [String: Any] ?? [:]
    }

    func testAnswersForwardedOffTheMainThreadAreKeptAndAnnouncedOnIt() {
        var heard: [(main: Bool, info: [AnyHashable: Any])] = []
        let observer = NotificationCenter.default.addObserver(forName: Notification.Name("exact.push.changed"), object: nil, queue: nil) {
            heard.append((Thread.isMainThread, $0.userInfo ?? [:]))
        }
        defer { NotificationCenter.default.removeObserver(observer) }
        let token = Data([0xde, 0xad, 0x01, 0x0f])
        let forwarded = expectation(description: "forwarded from a background queue")
        DispatchQueue.global().async {
            ExactApp.shared.registeredForRemoteNotifications(deviceToken: token)
            forwarded.fulfill()
        }
        wait(for: [forwarded], timeout: 5)
        let deadline = Date().addingTimeInterval(5)
        while heard.isEmpty, Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.02)) }
        XCTAssertEqual(heard.count, 1)
        XCTAssertEqual(heard.first?.main, true, "announced on the main thread")
        XCTAssertEqual(heard.first?.info["token"] as? String, "dead010f", "lowercase hex")
        XCTAssertEqual(read()["token"] as? String, "dead010f")
        XCTAssertEqual(read()["answers"] as? Int, 1)

        // The same token again is an answer still: a registration waits for one.
        ExactApp.shared.registeredForRemoteNotifications(deviceToken: token)
        XCTAssertEqual(heard.count, 2)
        XCTAssertEqual(read()["answers"] as? Int, 2)

        struct Refused: LocalizedError { var errorDescription: String? { "no valid aps-environment entitlement" } }
        ExactApp.shared.failedToRegisterForRemoteNotifications(error: Refused())
        XCTAssertEqual(heard.count, 3)
        XCTAssertEqual(read()["error"] as? String, "no valid aps-environment entitlement")
        XCTAssertNil(read()["token"], "a failed registration has no current token")

        ExactApp.shared.registeredForRemoteNotifications(deviceToken: Data([0x0a]))
        XCTAssertEqual(read()["token"] as? String, "0a")
        XCTAssertNil(read()["error"], "a token clears the failure")
    }

    #if os(iOS) || os(macOS)
    /// An app delegate with no answering methods is given them, and they hand
    /// the system's answer to the host.
    func testADelegateWithoutTheMethodsIsGivenThemAndTheyAnswerTheHost() throws {
        let delegate = BareDelegate()
        XCTAssertTrue(PushRegistration.install(on: delegate))
        _ = delegate.perform(PushRegistration.selectors[0], with: TestApp.shared, with: Data([0xab, 0x12]))
        XCTAssertEqual(read()["token"] as? String, "ab12")
        _ = delegate.perform(PushRegistration.selectors[1], with: TestApp.shared, with: NSError(domain: "t", code: 1, userInfo: [NSLocalizedDescriptionKey: "offline"]))
        XCTAssertEqual(read()["error"] as? String, "offline")
        XCTAssertNil(read()["token"])
    }

    /// An embedder's delegate that inherits the methods keeps them: its own
    /// handling runs, and the host learns only what the embedder forwards.
    func testAnInheritedImplementationIsKept() throws {
        let delegate = EmbedderDelegate()
        XCTAssertFalse(PushRegistration.install(on: delegate), "nothing added")
        _ = delegate.perform(PushRegistration.selectors[0], with: TestApp.shared, with: Data([0xcd]))
        XCTAssertEqual(delegate.heard, ["cd"], "the embedder's own method ran")
        XCTAssertNil(read()["token"], "the host learns only what the embedder forwards")
    }
    #endif

    func testNothingAnsweredReadsEmpty() {
        XCTAssertEqual(read()["answers"] as? Int, 0)
        XCTAssertNil(read()["token"])
    }
}

#if os(iOS) || os(macOS)
private final class BareDelegate: NSObject {}

/// An embedder's delegate whose registration callbacks are its superclass's.
private class EmbedderBase: NSObject {
    var heard: [String] = []
    @objc func application(_ application: TestApp, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        heard.append(deviceToken.map { String(format: "%02x", $0) }.joined())
    }
    @objc func application(_ application: TestApp, didFailToRegisterForRemoteNotificationsWithError error: Error) {}
}
private final class EmbedderDelegate: EmbedderBase {}
#endif
