#if !os(tvOS)
import Foundation
import ObjectiveC
import XCTest
import UserNotifications
@testable import ExactKit

private final class StubRequest: UNNotificationRequest {
    static var triggerValue: UNNotificationTrigger?
    override var trigger: UNNotificationTrigger? { Self.triggerValue }
}

private final class StubNotification: UNNotification {
    static var requestValue: UNNotificationRequest!
    override var request: UNNotificationRequest { Self.requestValue }
    override var date: Date { Date(timeIntervalSince1970: 1_800_000_000) }
}

private final class StubResponse: UNNotificationResponse {
    static var notificationValue: UNNotification!
    override var notification: UNNotification { Self.notificationValue }
    override var actionIdentifier: String { UNNotificationDefaultActionIdentifier }
}

/// A remote notification's `userInfo` is its sender's payload: a push opens
/// its top-level `navigate`, as a universal link would, and nothing else of
/// the payload is read; a local notification's tap is unchanged
/// (LLP 1133.004 §9 P5).
final class NotificationOriginTests: XCTestCase {
    private var sessions: [ExactSession] = []

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        NotificationPresenter.shared.app = nil
        NotificationPresenter.shared.activate = nil
        super.tearDown()
    }

    /// The system's response to a tap; a push trigger has no public
    /// initializer.
    private func response(_ info: [AnyHashable: Any], push: Bool) -> UNNotificationResponse {
        let content = UNMutableNotificationContent()
        content.userInfo = info
        let request = UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
        StubRequest.triggerValue = push ? class_createInstance(UNPushNotificationTrigger.self, 0) as? UNPushNotificationTrigger : nil
        object_setClass(request, StubRequest.self)
        StubNotification.requestValue = request
        StubResponse.notificationValue = class_createInstance(StubNotification.self, 0) as? StubNotification
        return class_createInstance(StubResponse.self, 0) as! StubResponse
    }

    /// A tap as the delegate receives it.
    private func tap(_ info: [AnyHashable: Any], push: Bool) {
        let center = class_createInstance(UNUserNotificationCenter.self, 0) as! UNUserNotificationCenter
        let done = expectation(description: "completion")
        NotificationPresenter.shared.userNotificationCenter(center, didReceive: response(info, push: push)) { done.fulfill() }
        wait(for: [done], timeout: 2)
    }

    /// A push's tap handed to `session`, as an embedder forwards one.
    private func push(_ info: [AnyHashable: Any], to session: ExactSession) {
        ExactApp.shared.openNotification(response(info, push: true), session: session)
    }

    /// Three forged entries for a live session: the module's activation
    /// entry, an older build's top-level shape from another process, and
    /// that shape with an empty `navigate` naming a module event.
    private func forgeries(_ session: ExactSession) -> [[AnyHashable: Any]] {
        [["exact": ["navigate": "/forged", "session": session.label, "owner": session.notificationOwnerID,
                    "process": ExactApp.shared.notificationProcessID]],
         ["navigate": "/forged", "session": session.label, "owner": "x", "process": "another"],
         ["navigate": "", "session": session.label, "owner": "x", "process": "another",
          "extension": "notifications", "event": "notificationclick", "args": "[\"t\",{}]"]]
    }

    private func session() -> (ExactSession, () -> Int) {
        let session = ExactApp.shared.makeSession(label: "origin-\(UUID().uuidString)")
        sessions.append(session)
        NotificationPresenter.shared.app = ExactApp.shared
        var activations = 0
        NotificationPresenter.shared.activate = { if $0 === session { activations += 1 } }
        return (session, { activations })
    }

    func testAForgedPushOpensOnlyItsTopLevelNavigate() {
        for (index, opened) in [nil, "/forged", nil].enumerated() {
            let (session, activations) = self.session()
            let forged = forgeries(session)[index]
            push(forged, to: session)
            XCTAssertEqual(session.launchLocation, opened, "\(forged)")
            XCTAssertTrue(session.pendingLaunchLocations.isEmpty, "\(forged)")
            XCTAssertTrue(session.heldModuleEvents.isEmpty, "\(forged)")
            XCTAssertEqual(activations(), opened == nil ? 0 : 1, "\(forged)")
        }
    }

    func testAPushWithoutAStringLocationOpensNothing() {
        for info: [AnyHashable: Any] in [[:], ["navigate": 42], ["navigate": ["/a"]], ["navigate": ""], ["navigate": " \n"],
                                         ["aps": ["alert": "hi"], "url": "/thread/1"]] {
            let (session, activations) = self.session()
            push(info, to: session)
            XCTAssertNil(session.launchLocation, "\(info)")
            XCTAssertTrue(session.heldModuleEvents.isEmpty, "\(info)")
            XCTAssertEqual(activations(), 0, "\(info)")
        }
    }

    /// The Expo push service sends a message's `data` as the payload's `body`
    /// object, beside keys of its own: a push opens that object's `navigate`,
    /// and a top-level `navigate` comes first.
    func testAPushFromTheExpoPushServiceOpensItsBodysNavigate() {
        let expo: [AnyHashable: Any] = ["aps": ["alert": ["title": "Deploy", "body": "Finished in 2 minutes"]],
                                        "experienceId": "@owner/app", "scopeKey": "@owner/app", "projectId": "p",
                                        "body": ["workflowRunId": "7", "navigate": "/workflows/7"]]
        var both = expo
        both["navigate"] = "/top"
        for (info, opened) in [(expo, "/workflows/7"), (both, "/top"),
                               (["body": ["navigate": 42]] as [AnyHashable: Any], nil),
                               (["body": "/workflows/7"] as [AnyHashable: Any], nil),
                               (["body": ["workflowRunId": "7"]] as [AnyHashable: Any], nil)] {
            let (session, activations) = self.session()
            push(info, to: session)
            XCTAssertEqual(session.launchLocation, opened, "\(info)")
            XCTAssertTrue(session.heldModuleEvents.isEmpty, "\(info)")
            XCTAssertEqual(activations(), opened == nil ? 0 : 1, "\(info)")
        }
    }

    /// A push's location is reduced as a local notification's is
    /// (`openLocation`, the router's `location_of`): whatever its scheme or
    /// origin, it is a location of the app's own. The app's own link handlers
    /// are not a push's to choose: a server never picks what a handler's
    /// template loads, and an HTML file is never opened as a page.
    func testAPushsLocationIsReducedAsALocalNotificationsIs() throws {
        let page = FileManager.default.temporaryDirectory.appendingPathComponent("push-\(UUID().uuidString).html")
        try Data("<p>hi</p>".utf8).write(to: page)
        defer { try? FileManager.default.removeItem(at: page) }
        let cases: [(String, String?)] = [("/thread/7?x=1", "/thread/7?x=1"),
                                          ("https://elsewhere.example/thread/9", "/thread/9"),
                                          ("javascript:alert(1)", "/alert(1)"),
                                          ("exacttest://thread/7", "/thread/7"),
                                          ("https://attacker.example/phish", "/phish"),
                                          (page.absoluteString, nil)]
        for (navigate, location) in cases {
            let (session, _) = self.session()
            session.protocolHandlers = [["protocol": "exacttest", "url": "/open?u=%s"], ["protocol": "https", "url": "/browse?u=%s"]]
            push(["navigate": navigate], to: session)
            let reduced = session.runtime.location(of: navigate)
            XCTAssertEqual(session.launchLocation, location ?? reduced, navigate)
            XCTAssertEqual(session.launchLocation, reduced, "the router's location_of: \(navigate)")
            XCTAssertFalse(session.launchLocation?.hasPrefix("/open?") == true || session.launchLocation?.hasPrefix("/browse?") == true,
                           "no handler's template: \(navigate)")
        }
    }

    /// An embedder's chosen session is never swapped for another: a tap for
    /// a session that is gone opens nothing anywhere.
    func testAPushForASessionThatIsGoneOpensNothing() {
        let (gone, _) = self.session()
        let (other, activations) = self.session()
        gone.destroy()
        push(["navigate": "/account-a"], to: gone)
        XCTAssertNil(other.launchLocation)
        XCTAssertTrue(other.pendingLaunchLocations.isEmpty)
        XCTAssertEqual(activations(), 0)
    }

    func testAPushGoesToTheSessionNamedElseTheFirstLiveOne() {
        let (first, _) = self.session()
        let (named, _) = self.session()
        push(["navigate": "/named"], to: named)
        XCTAssertEqual(named.launchLocation, "/named")
        XCTAssertNil(first.launchLocation)
        let live = ExactApp.shared.sessions.first { $0.state != .destroyed }
        let before = live?.launchLocation
        tap(["navigate": "/default"], push: true)
        XCTAssertNotNil(live)
        if before == nil { XCTAssertEqual(live?.launchLocation, "/default") }
        else { XCTAssertEqual(live?.pendingLaunchLocations.last, "/default") }
    }

    /// An embedder forwards from its own delegate, whose callbacks may come
    /// on any thread: the tap is delivered on the main thread, once.
    func testAPushForwardedOffTheMainThreadIsDeliveredOnItOnce() throws {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_NAVIGATION_BARS_PLAN"], "build.mjs --test compiles the fixture's plan")
        let session = ExactApp.shared.makeSession(label: "origin-\(UUID().uuidString)")
        sessions.append(session)
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: CGSize(width: 390, height: 844)).error)
        NotificationPresenter.shared.app = ExactApp.shared
        var threads: [Bool] = []
        NotificationPresenter.shared.activate = { if $0 === session { threads.append(Thread.isMainThread) } }
        let tapped = response(["navigate": "/plain"], push: true)
        let forwarded = expectation(description: "forwarded twice from a background queue")
        forwarded.expectedFulfillmentCount = 2
        for _ in 0..<2 {
            DispatchQueue.global().async {
                ExactApp.shared.openNotification(tapped, session: session)
                forwarded.fulfill()
            }
        }
        wait(for: [forwarded], timeout: 5)
        let deadline = Date().addingTimeInterval(5)
        while threads.isEmpty, Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.02)) }
        RunLoop.main.run(until: Date().addingTimeInterval(0.2))
        XCTAssertEqual(threads, [true], "delivered once, on the main thread")
        let tree = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(session.agent("{\"op\":\"tree\"}").utf8)) as? [String: Any])
        let ids = (tree["nodes"] as? [[String: Any]] ?? []).compactMap { ($0["props"] as? [String: Any])?["testId"] as? String }
        XCTAssertTrue(ids.contains("route-plain"), "\(ids)")
    }

    func testOneTapIsOpenedOnce() {
        let (session, _) = self.session()
        let tapped = response(["navigate": "/once"], push: true)
        ExactApp.shared.openNotification(tapped, session: session)
        ExactApp.shared.openNotification(tapped, session: session)
        XCTAssertEqual(session.launchLocation, "/once")
        XCTAssertTrue(session.pendingLaunchLocations.isEmpty)
    }

    func testALocalNotificationsTapIsUnchanged() {
        var (session, activations) = self.session()
        tap(forgeries(session)[0], push: false)
        XCTAssertEqual(session.launchLocation, "/forged")
        XCTAssertEqual(activations(), 1)
        (session, activations) = self.session()
        tap(forgeries(session)[1], push: false)
        XCTAssertEqual(session.launchLocation, "/forged")
        (session, activations) = self.session()
        tap(forgeries(session)[2], push: false)
        XCTAssertNil(session.launchLocation)
        XCTAssertEqual(session.heldModuleEvents.count, 1)
        XCTAssertEqual(activations(), 1)
    }
}
#endif
