import Foundation
import XCTest
@testable import ExactKit

/// The host events a package takes (LLP 1133.001.000 D2): a delegate method
/// added only for a taken event, and a mailbox addressed to sessions.
final class HostEventsTests: XCTestCase {
    private let event = "cloudkit-share"
    private var made: [ExactSession] = []

    override func setUp() {
        super.setUp()
        HostEvents.reset()
        XCTAssertTrue(ExactApp.shared.sessions.allSatisfy { $0.state == .destroyed }, "no other test's session is live")
    }

    override func tearDown() {
        made.forEach { $0.destroy() }
        made = []
        HostEvents.reset()
        super.tearDown()
    }

    private func session(_ label: String) -> ExactSession {
        let s = ExactApp.shared.makeSession(label: label)
        made.append(s)
        return s
    }

    /// What a package instance labelled `label` drains through its context.
    private func drain(_ label: String) -> [NSObject] {
        let box = NSMutableDictionary(dictionary: ["event": event, "session": label])
        NotificationCenter.default.post(name: Notification.Name("exact.hostEvents"), object: box)
        return (box["reply"] as? [String: Any])?["items"] as? [NSObject] ?? []
    }

    private func deliver(_ payload: NSObject, to label: String? = nil, event name: String? = nil) {
        var body: [String: Any] = ["event": name ?? event, "payload": payload]
        if let label { body["session"] = label }
        NotificationCenter.default.post(name: Notification.Name("exact.hostEvents.deliver"), object: body as NSDictionary)
    }

    #if os(iOS) || os(macOS)
    /// A delegate answers the event's selector only when the build takes it,
    /// and the method hands the system's object to the mailbox.
    func testTheSelectorIsAnsweredOnlyWhenTheEventIsTaken() throws {
        _ = session("main")
        #if os(iOS)
        let selector = NSSelectorFromString("windowScene:userDidAcceptCloudKitShareWithMetadata:")
        let install = { (taken: [HostEvents.Event], type: AnyClass) in HostEvents.install(taken, applicationDelegate: nil, sceneDelegate: type) }
        #else
        let selector = NSSelectorFromString("application:userDidAcceptCloudKitShareWithMetadata:")
        let install = { (taken: [HostEvents.Event], type: AnyClass) in HostEvents.install(taken, applicationDelegate: type, sceneDelegate: nil) }
        #endif
        let taken = HostEvents.taken(["ExactHostEvents": [event]])
        XCTAssertEqual(taken.map(\.name), [event])
        XCTAssertTrue(HostEvents.taken([:]).isEmpty)
        XCTAssertTrue(HostEvents.taken(["ExactHostEvents": ["not-listed"]]).isEmpty, "nothing for a name without selectors")
        install(HostEvents.taken([:]), NotTaken.self)
        XCTAssertFalse(NotTaken().responds(to: selector))
        install(taken, Taken.self)
        let delegate = Taken()
        XCTAssertTrue(delegate.responds(to: selector))
        let metadata = NSObject()
        _ = delegate.perform(selector, with: NSObject(), with: metadata)
        XCTAssertEqual(drain("main").map(ObjectIdentifier.init), [ObjectIdentifier(metadata)])
    }
    #endif

    #if os(iOS)
    /// A share that launched the app is read from the connection options by
    /// key, only for a taken event.
    func testAColdLaunchIsReadFromTheConnectionOptions() {
        _ = session("main")
        let options = Options()
        HostEvents.connected(options, [], session: nil)
        XCTAssertTrue(drain("main").isEmpty)
        HostEvents.connected(options, HostEvents.taken(["ExactHostEvents": [event]]), session: nil)
        XCTAssertEqual(drain("main").map(ObjectIdentifier.init), [ObjectIdentifier(options.cloudKitShareMetadata)])
    }
    #endif

    /// A delivery before any instance asks is held and drained once.
    func testADeliveryBeforeTheModuleLoadsIsDrainedOnceAfter() {
        _ = session("main")
        var heard: [[AnyHashable: Any]] = []
        let observer = NotificationCenter.default.addObserver(forName: Notification.Name("exact.hostEvents.arrived"), object: nil, queue: nil) { heard.append($0.userInfo ?? [:]) }
        defer { NotificationCenter.default.removeObserver(observer) }
        let metadata = NSObject()
        DispatchQueue.global().sync { ExactApp.shared.hostEvent(event, metadata) }
        let deadline = Date().addingTimeInterval(5)
        while heard.isEmpty, Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.02)) }
        XCTAssertEqual(heard.first?["event"] as? String, event)
        XCTAssertEqual(heard.first?["session"] as? String, "main")
        XCTAssertEqual(drain("main").map(ObjectIdentifier.init), [ObjectIdentifier(metadata)])
        XCTAssertTrue(drain("main").isEmpty)
        ExactApp.shared.hostEvent("not-listed", NSObject())
        XCTAssertTrue(drain("main").isEmpty, "a name not on the list is ignored")
    }

    /// A payload for session B is not A's, and becomes A's when B ends.
    func testAPayloadForAnEndedSessionGoesToTheFirstLiveOne() {
        let a = session("a"), b = session("b")
        let metadata = NSObject()
        ExactApp.shared.hostEvent(event, metadata, session: b)
        XCTAssertTrue(drain("a").isEmpty)
        var heard: [[AnyHashable: Any]] = []
        let observer = NotificationCenter.default.addObserver(forName: Notification.Name("exact.hostEvents.arrived"), object: nil, queue: nil) { heard.append($0.userInfo ?? [:]) }
        defer { NotificationCenter.default.removeObserver(observer) }
        b.destroy()
        XCTAssertEqual(heard.first?["session"] as? String, a.label)
        XCTAssertEqual(drain("a").map(ObjectIdentifier.init), [ObjectIdentifier(metadata)])
        XCTAssertTrue(drain("b").isEmpty)
    }

    /// At most `bound` are held per event: the oldest goes first.
    func testTheSeventeenthDropsTheOldest() {
        _ = session("main")
        let all = (0...HostEvents.bound).map { _ in NSObject() }
        all.forEach { deliver($0, to: "main") }
        XCTAssertEqual(drain("main").map(ObjectIdentifier.init), all.dropFirst().map(ObjectIdentifier.init))
    }

    /// The same object, or an equal one, still waiting is held once; after a
    /// drain it is a new delivery.
    func testTheSameObjectTwiceIsHeldOnce() {
        _ = session("main")
        let metadata = NSObject()
        deliver(metadata, to: "main")
        deliver(metadata, to: "main")
        deliver(NSString(string: "share"), to: "main")
        deliver(NSString(string: "share"), to: "main")
        XCTAssertEqual(drain("main").count, 2)
        deliver(metadata, to: "main")
        XCTAssertEqual(drain("main").count, 1)
        deliver(NSObject(), to: "main", event: "not-listed")
        XCTAssertTrue(drain("main").isEmpty)
    }
}

private final class Taken: NSObject {}
private final class NotTaken: NSObject {}
#if os(iOS)
private final class Options: NSObject {
    @objc let cloudKitShareMetadata = NSObject()
}
#endif
