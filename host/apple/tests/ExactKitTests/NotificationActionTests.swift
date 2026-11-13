#if !os(tvOS)
import Foundation
import Security
import ObjectiveC
import CExact
import ExactCore
import XCTest
import UserNotifications
#if os(iOS)
import UIKit
#endif
@testable import ExactKit

#if os(iOS)
private extension UIApplication {
    @objc var notificationActionBackgroundState: UIApplication.State { .background }
    @objc var notificationActionProtectedDataUnavailable: Bool { false }
}
#endif

private final class ActionRequest: UNNotificationRequest {
    static var triggerValue: UNNotificationTrigger?
    override var trigger: UNNotificationTrigger? { Self.triggerValue }
}

private final class ActionNotification: UNNotification {
    static var requestValue: UNNotificationRequest!
    override var request: UNNotificationRequest { Self.requestValue }
    static var dateValue = Date()
    override var date: Date { Self.dateValue }
}

private final class ActionResponse: UNTextInputNotificationResponse {
    static var notificationValue: UNNotification!
    static var actionValue = ""
    static var textValue = ""
    override var notification: UNNotification { Self.notificationValue }
    override var actionIdentifier: String { Self.actionValue }
    override var userText: String { Self.textValue }
}

/// A notification's declared actions (LLP 1133.004 stage 2): the plan's
/// lists registered as categories; a chosen action read against them,
/// through the delegate with the system's response stood in; a foreground
/// action raised as the click with its name and reply, a background one held
/// open until the runner says its invocation ended or the system's time ran
/// out.
final class NotificationActionTests: XCTestCase {
    private var sessions: [ExactSession] = []
    private var kept: Data?
    private var restorePresenter: (() -> Void)?

    private let declared = """
    {"notifications.showNotification":{"actions":[[\
    {"action":"approve","title":"Approve","foreground":false,"destructive":false,"authenticationRequired":true,"placeholder":null},\
    {"action":"note","title":"Add a note","foreground":true,"destructive":false,"authenticationRequired":false,"placeholder":"Note"}]]}}
    """

    /// The centre's categories as the host registers them.
    private var registered: Set<UNNotificationCategory> = []

    override func setUp() {
        super.setUp()
        let presenter = NotificationPresenter.shared
        let categories = presenter.categories, current = presenter.currentCategories, priorRegistered = presenter.registeredCategories
        let firstUnlock = presenter.firstUnlockAvailable
        restorePresenter = {
            presenter.categories = categories; presenter.currentCategories = current
            presenter.registeredCategories = priorRegistered; presenter.firstUnlockAvailable = firstUnlock
        }
        // Bare XCTest has no app Keychain identity. Unlock tests supply the marker's statuses.
        presenter.firstUnlockAvailable = { true }
        presenter.currentCategories = [:]
        presenter.registeredCategories = [:]
        // An XCTest process has no notification centre (`current()` traps): the categories are held here.
        NotificationPresenter.shared.categories = (read: { [unowned self] in $0(registered) }, write: { [unowned self] in registered = $0 })
        kept = UserDefaults.standard.data(forKey: DeclaredCategories.key)
        DeclaredCategories.keep(["approve,note": DeclaredCategories.lists(declared)[0]])
    }

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        NotificationPresenter.shared.app = nil
        NotificationPresenter.shared.activate = nil
        NotificationPresenter.shared.boot = nil
        restorePresenter?()
        UserDefaults.standard.set(kept, forKey: DeclaredCategories.key)
        super.tearDown()
    }

    /// The system's response to a chosen action; a push trigger has no
    /// public initializer.
    private func response(_ info: [AnyHashable: Any], category: String, action: String, text: String = "",
                          push: Bool = false, definition: DeclaredAction? = nil) -> UNNotificationResponse {
        let content = UNMutableNotificationContent()
        content.userInfo = info
        content.categoryIdentifier = category
        let request = UNNotificationRequest(identifier: "req-\(UUID().uuidString)", content: content, trigger: nil)
        ActionRequest.triggerValue = push ? class_createInstance(UNPushNotificationTrigger.self, 0) as? UNPushNotificationTrigger : nil
        object_setClass(request, ActionRequest.self)
        ActionNotification.dateValue = Date()
        ActionNotification.requestValue = request
        ActionResponse.notificationValue = class_createInstance(ActionNotification.self, 0) as? ActionNotification
        ActionResponse.actionValue = (definition ?? DeclaredCategories.lists(declared).flatMap { $0 }.first { $0.action == action })?.platform.identifier ?? action
        ActionResponse.textValue = text
        return class_createInstance(ActionResponse.self, 0) as! ActionResponse
    }

    /// The response as the delegate receives it; whether its completion
    /// handler was called.
    @discardableResult
    private func choose(_ response: UNNotificationResponse) -> () -> Bool {
        let center = class_createInstance(UNUserNotificationCenter.self, 0) as! UNUserNotificationCenter
        var completed = false
        NotificationPresenter.shared.userNotificationCenter(center, didReceive: response) { completed = true }
        return { completed }
    }

    private func session(label: String = "actions") -> (ExactSession, () -> Int) {
        let session = ExactApp.shared.makeSession(label: "\(label)-\(UUID().uuidString)")
        sessions.append(session)
        NotificationPresenter.shared.app = ExactApp.shared
        NotificationPresenter.shared.declare(DeclaredCategories.lists(declared), for: session)
        var activations = 0
        NotificationPresenter.shared.activate = { if $0 === session { activations += 1 } }
        return (session, { activations })
    }

    /// A local post's activation entry, as the package writes it.
    private func entry(_ session: ExactSession) -> [AnyHashable: Any] {
        ["exact": ["session": session.label, "owner": session.notificationOwnerID, "process": ExactApp.shared.notificationProcessID,
                   "extension": "notifications", "event": "notificationclick", "chosen": 1,
                   "args": #"["p1",{"tag":"p1","data":"/p/1","title":"Coffee","body":null}]"#]]
    }

    private func held(_ session: ExactSession) -> [[String: Any]] {
        session.heldModuleEvents.compactMap { (try? JSONSerialization.jsonObject(with: Data($0.utf8))) as? [String: Any] }
    }

    #if os(macOS)
    func testTheNotificationInstallerBindsAnActivationClosure() {
        let (session, _) = self.session()
        var activated = false
        ExactApp.shared.installNotificationClicks(boot: nil, activate: { target in activated = target === session })
        NotificationPresenter.shared.activate?(session)
        XCTAssertTrue(activated)
        XCTAssertNil(NotificationPresenter.shared.boot)
        activated = false
        choose(response(entry(session), category: "", action: UNNotificationDefaultActionIdentifier))
        XCTAssertTrue(activated, "the notification body activates the Mac adapter")
        activated = false
        choose(response(entry(session), category: "approve,note", action: "note", text: "Reply"))
        XCTAssertTrue(activated, "a foreground action activates the Mac adapter")
        activated = false
        NotificationPresenter.shared.currentCategories = [:]
        choose(response(entry(session), category: "approve,note", action: "approve"))
        XCTAssertFalse(activated, "waiting for declarations never activates the app")
    }
    #endif

    func testAReplacementWaitsForItsOwnDeclarations() throws {
        let (session, _) = self.session()
        try boot(session)
        waitForActivation(session)
        let candidate = try XCTUnwrap(session.prepare(try fixturePlan("NOTIFICATION_NO_ACTIONS"), resolver: ExactApp.shared.resolver, size: CGSize(width: 390, height: 844)))
        let batch = try XCTUnwrap(session.commit(candidate))
        session.presentCommitted(batch, label: "notification declarations")
        let completed = choose(response(entry(session), category: "approve,note", action: "approve"))
        XCTAssertFalse(completed(), "authorization waits for the replacement's declarations")
        XCTAssertFalse(acted(session).contains("p1 approve"), acted(session))
        session.raiseModuleEvent(extension: "notifications", event: "notificationclick",
                                 args: #"["plain",{"tag":"plain","data":null,"title":"","body":null,"action":null,"reply":null}]"#, activation: true)
        XCTAssertFalse(acted(session).contains("plain"), "a previous data generation cannot receive a module event")
        session.drawReceipt().drawn()
        waitForActivation(session)
        XCTAssertTrue(completed())
        XCTAssertFalse(acted(session).contains("p1 approve"), acted(session))
        XCTAssertTrue(acted(session).contains("plain"), acted(session))
    }

    func testARegisteredResponseWaitsForDataAndRevalidatesAfterReplacement() throws {
        let (session, activations) = self.session()
        XCTAssertNil(session.boot(plan: try fixturePlan("NOTIFICATION_ACTIONS"), size: CGSize(width: 390, height: 844)).error)
        NotificationPresenter.shared.declare(DeclaredCategories.lists(declared), for: session)
        let completed = choose(response(entry(session), category: "approve,note", action: "note", text: "Old"))
        XCTAssertFalse(completed(), "registration alone does not deliver before data activation")
        XCTAssertEqual(activations(), 0)
        let candidate = try XCTUnwrap(session.prepare(try fixturePlan("NOTIFICATION_NO_ACTIONS"), resolver: ExactApp.shared.resolver, size: CGSize(width: 390, height: 844)))
        let batch = try XCTUnwrap(session.commit(candidate))
        session.presentCommitted(batch, label: "replace before action delivery")
        session.drawReceipt().drawn()
        waitForActivation(session)
        XCTAssertTrue(completed())
        XCTAssertEqual(activations(), 0)
        XCTAssertFalse(acted(session).contains("p1 note Old"), "a response must be checked against the generation that hears it")
    }

    func testAnotherSessionCannotRemoveThePostingSessionsCategories() throws {
        let (a, _) = self.session(label: "a")
        let (b, _) = self.session(label: "b")
        try boot(a)
        waitForActivation(a)
        XCTAssertNil(b.boot(plan: try fixturePlan("NOTIFICATION_NO_ACTIONS"), size: CGSize(width: 390, height: 844)).error)
        b.drawReceipt().drawn()
        waitForActivation(b)
        XCTAssertEqual(Set(registered.map(\.identifier)), ["approve,note"])
        let completed = choose(response(entry(a), category: "approve,note", action: "approve"))
        waitFor("the posting session's action ends") { completed() }
        XCTAssertTrue(acted(a).contains("p1 approve"), acted(a))
        choose(response(entry(b), category: "approve,note", action: "approve"))
        XCTAssertFalse(acted(b).contains("p1 approve"), acted(b))
        a.destroy()
        XCTAssertTrue(registered.isEmpty, "ending a session removes only its categories")
    }

    func testAPushActionFindsTheFirstSessionThatDeclaresIt() throws {
        let (a, _) = self.session(label: "without-actions")
        let (b, _) = self.session(label: "declaring")
        XCTAssertNil(a.boot(plan: try fixturePlan("NOTIFICATION_NO_ACTIONS"), size: CGSize(width: 390, height: 844)).error)
        a.drawReceipt().drawn()
        waitForActivation(a)
        try boot(b)
        waitForActivation(b)
        let push = response(["aps": ["category": "approve,note"]], category: "approve,note", action: "approve", push: true)
        let completed = choose(push)
        waitFor("the push action ends") { completed() }
        XCTAssertFalse(acted(a).contains("approve"), acted(a))
        XCTAssertTrue(acted(b).contains(push.notification.request.identifier + " approve"), acted(b))
        XCTAssertNil(NotificationPresenter.shared.opened[ObjectIdentifier(b)])
    }

    func testAWaitingPushMovesItsHoldToTheDeclaringSession() throws {
        let (a, _) = self.session(label: "first")
        let (b, _) = self.session(label: "declaring")
        XCTAssertNil(a.boot(plan: try fixturePlan("NOTIFICATION_NO_ACTIONS"), size: CGSize(width: 390, height: 844)).error)
        XCTAssertNil(b.boot(plan: try fixturePlan("NOTIFICATION_ACTIONS"), size: CGSize(width: 390, height: 844)).error)
        NotificationPresenter.shared.currentCategories = [:]
        let push = response([:], category: "approve,note", action: "approve", push: true)
        var completed = false
        NotificationPresenter.shared.chose(push, to: a) { completed = true }
        XCTAssertNotNil(NotificationPresenter.shared.opened[ObjectIdentifier(a)])
        a.drawReceipt().drawn()
        waitForActivation(a)
        XCTAssertFalse(completed)
        XCTAssertNil(NotificationPresenter.shared.opened[ObjectIdentifier(a)])
        XCTAssertNotNil(NotificationPresenter.shared.opened[ObjectIdentifier(b)])
        b.drawReceipt().drawn()
        waitForActivation(b)
        XCTAssertTrue(completed)
        XCTAssertTrue(acted(b).contains(push.notification.request.identifier + " approve"), acted(b))
        XCTAssertNil(NotificationPresenter.shared.opened[ObjectIdentifier(b)])
    }

    func testAnInvisibleContinuationReleasesTheBackgroundAction() throws {
        let (session, _) = self.session()
        try boot(session)
        waitForActivation(session)
        var reply: UnsafeMutableRawPointer?
        let later: ExactAppLaterFn = { ctx, _, _, pending in
            ctx?.assumingMemoryBound(to: UnsafeMutableRawPointer?.self).pointee = pending
        }
        withUnsafeMutablePointer(to: &reply) { pointer in
            Owner.shared.sync { exact_set_app_module(session.runtime.rt, later, nil, pointer) }
            session.runtime.bindExtensions(["quick-actions"])
            var completed = false
            NotificationPresenter.shared.background(session, extension: "notifications", event: "notificationclick",
                args: #"["quiet",{"tag":"quiet","data":null,"title":"","body":null,"action":"approve","reply":null}]"#) { completed = true }
            waitFor("the action request") { pointer.pointee != nil }
            XCTAssertFalse(completed)
            if let pending = pointer.pointee {
                Data(#"{"state":"unsupported"}"#.utf8).withUnsafeBytes {
                    exact_app_reply(pending, 200, $0.bindMemory(to: UInt8.self).baseAddress, $0.count)
                }
            }
            waitFor("the continuation ends") { session.agent("{\"op\":\"actions\"}").contains("completed") }
            session.drainFill()
            XCTAssertTrue(completed, "an ending with no presentation change still releases the delegate")
            XCTAssertNil(NotificationPresenter.shared.opened[ObjectIdentifier(session)])
        }
    }

    func testConflictingCategoriesAreRefusedUntilTheConflictEnds() throws {
        let (a, _) = self.session(label: "a")
        let (b, _) = self.session(label: "b")
        let presenter = NotificationPresenter.shared
        var conflicting = DeclaredCategories.lists(declared)[0]
        conflicting[0] = try XCTUnwrap(DeclaredAction(["action": "approve", "title": "Other approval", "foreground": true]))
        presenter.declare([conflicting], for: b)
        XCTAssertTrue(registered.isEmpty)
        XCTAssertNil(presenter.declared("approve", in: "approve,note", for: a))
        XCTAssertNil(presenter.declared("approve", in: "approve,note", for: b))
        b.destroy()
        XCTAssertEqual(Set(registered.map(\.identifier)), ["approve,note"])
        XCTAssertNotNil(presenter.declared("approve", in: "approve,note", for: a))
    }

    func testThePlansListsAreRegisteredAsCategories() throws {
        let (session, _) = self.session()
        let lists = DeclaredCategories.lists(declared)
        XCTAssertEqual(lists.count, 1)
        XCTAssertEqual(DeclaredCategories.identifier(lists[0]), "approve,note")
        let approve = lists[0][0].platform, note = lists[0][1].platform
        XCTAssertEqual(approve.identifier, lists[0][0].identifier)
        XCTAssertEqual(approve.options, [.authenticationRequired], "a background action brings nothing forward")
        XCTAssertFalse(approve is UNTextInputNotificationAction)
        XCTAssertEqual(note.options, [.foreground])
        XCTAssertEqual((note as? UNTextInputNotificationAction)?.textInputPlaceholder, "Note")
        let destructive = try XCTUnwrap(DeclaredAction(["action": "delete", "title": "Delete", "foreground": false, "destructive": true]))
        XCTAssertEqual(destructive.platform.options, [.destructive])
        XCTAssertTrue(DeclaredCategories.lists("{}").isEmpty)
        // An embedder's own category stays beside the app's.
        registered = [UNNotificationCategory(identifier: "embedder", actions: [], intentIdentifiers: [], options: [])]
        NotificationPresenter.shared.declare(lists, for: session)
        let ids = Set(registered.map(\.identifier))
        let approveNote = try XCTUnwrap(registered.first { $0.identifier == "approve,note" })
        XCTAssertEqual(approveNote.actions.map(\.identifier), lists[0].map(\.identifier))
        XCTAssertEqual(ids, ["approve,note", "embedder"])
    }

    func testRemovedCategoriesAreReconciledIncludingAnEmptyPlan() throws {
        let (session, _) = self.session()
        let lists = DeclaredCategories.lists(declared)
        let old = UNNotificationCategory(identifier: "approve,note", actions: lists[0].map(\.platform), intentIdentifiers: [], options: [])
        let embedder = UNNotificationCategory(identifier: "embedder", actions: [], intentIdentifiers: [], options: [])
        registered = [old, embedder]
        NotificationPresenter.shared.declare([[lists[0][0]]], for: session)
        XCTAssertEqual(Set(registered.map(\.identifier)), ["approve", "embedder"])
        XCTAssertNil(NotificationPresenter.shared.declared("approve", in: "approve,note", for: session))
        NotificationPresenter.shared.declare([], for: session)
        XCTAssertEqual(Set(registered.map(\.identifier)), ["embedder"])
        XCTAssertNil(NotificationPresenter.shared.declared("approve", in: "approve", for: session))
    }

    func testCategoryOwnershipTransitionsPreserveEmbedderDefinitions() throws {
        let (a, _) = session(label: "owner")
        let presenter = NotificationPresenter.shared
        let lists = DeclaredCategories.lists(declared)
        presenter.declare([], for: a)
        XCTAssertTrue(DeclaredCategories.kept.isEmpty, "removal relinquishes ownership")
        let embedder = UNNotificationCategory(identifier: "approve,note", actions: [], intentIdentifiers: [], options: [])
        registered = [embedder]
        presenter.declare([], for: a)
        XCTAssertEqual(registered, [embedder], "a released name belongs to its new owner")
        DeclaredCategories.keep([:])
        registered = [embedder]
        presenter.declare(lists, for: a)
        XCTAssertEqual(registered, [embedder], "a new claim cannot replace an embedder")
        XCTAssertNil(presenter.declared("approve", in: "approve,note", for: a))
        a.destroy()
        XCTAssertEqual(registered, [embedder], "teardown cannot remove the refused claim")

        registered = []
        let (b, _) = session(label: "new-owner")
        XCTAssertNotNil(presenter.declared("approve", in: "approve,note", for: b))
        let (c, _) = session(label: "shared-owner")
        b.destroy()
        XCTAssertNotNil(presenter.declared("approve", in: "approve,note", for: c))
        c.destroy()
        XCTAssertTrue(registered.isEmpty)
        XCTAssertTrue(DeclaredCategories.kept.isEmpty)
    }

    func testRegistrationPrecedesDataActivationAndWaitingResponses() throws {
        let (session, _) = self.session()
        let presenter = NotificationPresenter.shared
        presenter.currentCategories = [:]
        var read: ((Set<UNNotificationCategory>) -> Void)?
        presenter.categories.read = { read = $0 }
        let completed = choose(response(entry(session), category: "approve,note", action: "note"))
        XCTAssertFalse(completed())
        try boot(session)
        waitFor("the category read") { read != nil }
        XCTAssertNil(session.preparedGeneration, "initial data code cannot post before registration")
        XCTAssertFalse(session.dataActivated)
        XCTAssertFalse(completed(), "an action that can post also waits for registration")
        let finish = try XCTUnwrap(read)
        presenter.categories.read = { [unowned self] in $0(registered) }
        finish(registered)
        waitForActivation(session)
        XCTAssertTrue(session.dataActivated)
        XCTAssertTrue(completed())
        XCTAssertEqual(Set(registered.map(\.identifier)), ["approve,note"])
    }

    func testReplacementSettlesRaisedActionsButKeepsUnauthorizedResponses() throws {
        let (session, _) = self.session()
        try boot(session)
        waitForActivation(session)
        var reply: UnsafeMutableRawPointer?
        let later: ExactAppLaterFn = { ctx, _, _, pending in
            ctx?.assumingMemoryBound(to: UnsafeMutableRawPointer?.self).pointee = pending
        }
        try withUnsafeMutablePointer(to: &reply) { pointer in
            Owner.shared.sync { exact_set_app_module(session.runtime.rt, later, nil, pointer) }
            session.runtime.bindExtensions(["quick-actions"])
            var completed = 0
            NotificationPresenter.shared.background(session, extension: "notifications", event: "notificationclick",
                args: #"["wait",{"tag":"wait","data":null,"title":"","body":null,"action":"approve","reply":null}]"#) { completed += 1 }
            waitFor("the action's request") { pointer.pointee != nil }
            XCTAssertEqual(completed, 0)
            let candidate = try XCTUnwrap(session.prepare(try fixturePlan("NOTIFICATION_ACTIONS"), resolver: ExactApp.shared.resolver, size: CGSize(width: 390, height: 844)))
            let batch = try XCTUnwrap(session.commit(candidate))
            session.presentCommitted(batch, label: "notification action replacement")
            XCTAssertEqual(completed, 1, "the replaced runner cannot report the action's end")
            let waiting = choose(response(entry(session), category: "approve,note", action: "note"))
            XCTAssertFalse(waiting(), "a response awaiting the new declarations is separate")
            session.drawReceipt().drawn()
            waitForActivation(session)
            XCTAssertTrue(waiting())
            XCTAssertEqual(completed, 1)
            if let pending = pointer.pointee {
                Data("{}".utf8).withUnsafeBytes {
                    exact_app_reply(pending, 200, $0.bindMemory(to: UInt8.self).baseAddress, $0.count)
                }
            }
        }
    }

    func testCategoryReconciliationSerializesReads() {
        let (session, _) = self.session()
        var reads: [(Set<UNNotificationCategory>) -> Void] = []
        let read = NotificationPresenter.shared.categories.read
        defer { NotificationPresenter.shared.categories.read = read }
        NotificationPresenter.shared.categories.read = { reads.append($0) }
        let lists = DeclaredCategories.lists(declared)
        NotificationPresenter.shared.declare([[lists[0][0]]], for: session)
        NotificationPresenter.shared.declare(lists, for: session)
        XCTAssertEqual(reads.count, 1, "a second read must wait for the first write")
        guard !reads.isEmpty else { return XCTFail("no category read") }
        let first = reads.removeFirst()
        first([])
        if !reads.isEmpty { reads.removeFirst()(registered) }
        XCTAssertEqual(Set(registered.map(\.identifier)), ["approve,note"])
    }

    func testAForegroundActionIsTheClickWithItsNameAndReply() throws {
        let (session, activations) = self.session()
        let completed = choose(response(entry(session), category: "approve,note", action: "note", text: "On my way"))
        XCTAssertTrue(completed(), "a foreground action lets the app go at once")
        XCTAssertEqual(activations(), 1)
        let event = try XCTUnwrap(held(session).first)
        XCTAssertNil(event["background"])
        XCTAssertEqual(event["activation"] as? Bool, true)
        let args = try XCTUnwrap(event["args"] as? [Any])
        let fields = try XCTUnwrap(args.last as? [String: Any])
        XCTAssertEqual(fields["action"] as? String, "note")
        XCTAssertEqual(fields["reply"] as? String, "On my way")
        XCTAssertEqual(fields["data"] as? String, "/p/1", "a local post's own data")
        XCTAssertNil(session.launchLocation, "an action never opens a location")
    }

    /// A push's action carries its identifier as the tag, the action and the
    /// reply, and nothing else of the payload: not a forged entry, not its
    /// keys, not its `navigate`.
    func testAPushsActionCarriesOnlyItsIdentifierTheActionAndTheReply() throws {
        let (session, _) = self.session()
        var info = entry(session)
        info["aps"] = ["alert": ["title": "Coffee", "body": "$4.50"], "category": "approve,note"]
        info["proposalId"] = "p9"
        info["navigate"] = "/forged"
        let chosen = response(info, category: "approve,note", action: "note", text: "Later", push: true)
        choose(chosen)
        let event = try XCTUnwrap(held(session).first)
        XCTAssertEqual(event["extension"] as? String, "notifications")
        XCTAssertEqual(event["event"] as? String, "notificationclick")
        let args = try XCTUnwrap(event["args"] as? [Any])
        let id = chosen.notification.request.identifier
        XCTAssertEqual(args.first as? String, id)
        let fields = try XCTUnwrap(args.last as? [String: Any])
        XCTAssertEqual(fields["tag"] as? String, id)
        XCTAssertTrue(fields["data"] is NSNull, "\(fields)")
        XCTAssertEqual(fields["title"] as? String, "")
        XCTAssertEqual(fields["action"] as? String, "note")
        XCTAssertEqual(fields["reply"] as? String, "Later")
        XCTAssertNil(session.launchLocation)
    }

    func testAnActionTheAppDidNotDeclareRaisesNothing() {
        let (session, activations) = self.session()
        for (category, action) in [("approve,note", "delete"), ("embedder", "approve"), ("", "approve")] {
            let completed = choose(response(entry(session), category: category, action: action))
            XCTAssertTrue(completed(), "\(category) \(action)")
        }
        XCTAssertTrue(session.heldModuleEvents.isEmpty)
        XCTAssertEqual(activations(), 0)
    }

    func testTheFirstUnlockMarkerIsCreatedAndReadWithAfterFirstUnlockProtection() {
        for added in [errSecSuccess, errSecDuplicateItem, errSecInteractionNotAllowed] {
            var reads = 0
            let available = NotificationFirstUnlock.available(read: { query in
                let q = query as NSDictionary
                XCTAssertEqual(q[kSecClass] as? String, kSecClassGenericPassword as String)
                XCTAssertEqual(q[kSecReturnData] as? Bool, true)
                XCTAssertEqual(q[kSecUseAuthenticationUI] as? String, kSecUseAuthenticationUIFail as String)
                reads += 1
                return reads == 1 ? errSecItemNotFound : errSecSuccess
            }, add: { item in
                let q = item as NSDictionary
                XCTAssertEqual(q[kSecAttrAccessible] as? String, kSecAttrAccessibleAfterFirstUnlock as String)
                XCTAssertEqual(q[kSecValueData] as? Data, Data([1]))
                return added
            })
            XCTAssertEqual(available, added != errSecInteractionNotAllowed)
            XCTAssertEqual(reads, available ? 2 : 1)
        }
    }

    func testABeforeFirstUnlockResponseCompletesWithoutBootOrDelivery() {
        let presenter = NotificationPresenter.shared
        let original = presenter.firstUnlockAvailable
        defer { presenter.firstUnlockAvailable = original }
        presenter.firstUnlockAvailable = {
            NotificationFirstUnlock.available(read: { _ in errSecInteractionNotAllowed }, add: { _ in
                XCTFail("an inaccessible marker is not a missing marker")
                return errSecInteractionNotAllowed
            })
        }
        let (session, activations) = self.session()
        let action = DeclaredAction(["action": "approve", "title": "Approve", "foreground": false,
                                     "authenticationRequired": false])!
        var boots = 0
        presenter.boot = { _ in boots += 1 }
        for hasDeclarations in [false, true] {
            presenter.currentCategories = [:]
            if hasDeclarations { presenter.declare([[action]], for: session) }
            let completed = choose(response(entry(session), category: "approve", action: "approve", definition: action))
            XCTAssertTrue(completed(), "a response before first unlock must not wait for data")
            XCTAssertEqual(boots, 0)
            XCTAssertFalse(session.booted)
            XCTAssertFalse(session.dataActivated)
            XCTAssertTrue(session.heldModuleEvents.isEmpty)
            XCTAssertNil(presenter.opened[ObjectIdentifier(session)])
            XCTAssertEqual(activations(), 0)
        }
    }

    func testAColdResponseWaitsForCurrentDeclarations() {
        let presenter = NotificationPresenter.shared
        let (session, activations) = self.session()
        presenter.currentCategories = [:]
        var boots = 0
        presenter.boot = { _ in boots += 1 }
        // Keep the category ID unchanged while the update newly requires an unlock.
        var old = DeclaredCategories.lists(declared)[0]
        old[0] = DeclaredAction(["action": "approve", "title": "Approve", "foreground": false])!
        DeclaredCategories.keep(["approve,note": old])
        registered = [UNNotificationCategory(identifier: "approve,note", actions: old.map(\.platform), intentIdentifiers: [], options: [])]
        let completed = choose(response(entry(session), category: "approve,note", action: "approve", definition: old[0]))
        XCTAssertFalse(completed())
        XCTAssertEqual(boots, 1)
        XCTAssertTrue(session.heldModuleEvents.isEmpty)
        presenter.declare(DeclaredCategories.lists(declared), for: session)
        XCTAssertTrue(completed())
        XCTAssertEqual(activations(), 0)
        XCTAssertTrue(session.heldModuleEvents.isEmpty)
        // The old system action stays refused after reconciliation and another launch.
        presenter.currentCategories = [:]
        presenter.registeredCategories = [:]
        let relaunched = choose(response(entry(session), category: "approve,note", action: "approve", definition: old[0]))
        presenter.declare(DeclaredCategories.lists(declared), for: session)
        XCTAssertTrue(relaunched())
        XCTAssertTrue(session.heldModuleEvents.isEmpty)
        choose(response(entry(session), category: "approve,note", action: "note", text: "Current"))
        XCTAssertEqual(activations(), 1, "a current system action still works")
    }

    #if os(iOS)
    func testALockedAfterFirstUnlockResponseBootsAndDelivers() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "bootForWake needs the host's UIApplication") }
        let state = try XCTUnwrap(class_getInstanceMethod(UIApplication.self, #selector(getter: UIApplication.applicationState)))
        let background = try XCTUnwrap(class_getInstanceMethod(UIApplication.self, #selector(getter: UIApplication.notificationActionBackgroundState)))
        let protected = try XCTUnwrap(class_getInstanceMethod(UIApplication.self, #selector(getter: UIApplication.isProtectedDataAvailable)))
        let locked = try XCTUnwrap(class_getInstanceMethod(UIApplication.self, #selector(getter: UIApplication.notificationActionProtectedDataUnavailable)))
        method_exchangeImplementations(state, background)
        method_exchangeImplementations(protected, locked)
        let presenter = NotificationPresenter.shared
        let original = presenter.firstUnlockAvailable
        defer {
            presenter.firstUnlockAvailable = original
            method_exchangeImplementations(state, background)
            method_exchangeImplementations(protected, locked)
        }
        XCTAssertFalse(UIApplication.shared.isProtectedDataAvailable)
        var reads = 0
        presenter.firstUnlockAvailable = {
            NotificationFirstUnlock.available(read: { _ in reads += 1; return errSecSuccess }, add: { _ in
                XCTFail("the marker already exists")
                return errSecDuplicateItem
            })
        }
        let (session, activations) = self.session()
        presenter.currentCategories = [:]
        let plan = try fixturePlan("NOTIFICATION_ACTIONS")
        var boots = 0
        presenter.boot = { target in
            boots += 1
            XCTAssertNil(target.boot(plan: plan, size: CGSize(width: 390, height: 844)).error)
            target.bootForWake()
        }
        let completed = choose(response(entry(session), category: "approve,note", action: "approve"))
        spin { completed() }
        XCTAssertGreaterThan(reads, 0)
        XCTAssertEqual(boots, 1)
        XCTAssertTrue(session.dataActivated)
        XCTAssertTrue(acted(session).contains("p1 approve"), acted(session))
        XCTAssertNil(session.view)
        XCTAssertEqual(activations(), 0)
        XCTAssertNil(presenter.opened[ObjectIdentifier(session)])
    }

    func testABackgroundActionActivatesABootedUndrawnSession() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "bootForWake needs the host's UIApplication") }
        let state = try XCTUnwrap(class_getInstanceMethod(UIApplication.self, #selector(getter: UIApplication.applicationState)))
        let background = try XCTUnwrap(class_getInstanceMethod(UIApplication.self, #selector(getter: UIApplication.notificationActionBackgroundState)))
        method_exchangeImplementations(state, background)
        defer { method_exchangeImplementations(state, background) }
        XCTAssertEqual(UIApplication.shared.applicationState, .background)
        let (session, activations) = self.session()
        XCTAssertNil(session.boot(plan: try fixturePlan("NOTIFICATION_ACTIONS"), size: CGSize(width: 390, height: 844)).error)
        XCTAssertTrue(session.booted)
        XCTAssertNil(session.firstDrawMs)
        XCTAssertFalse(session.dataActivated)
        NotificationPresenter.shared.boot = { $0.bootForWake() }

        let completed = choose(response(entry(session), category: "approve,note", action: "approve"))
        spin { completed() }
        XCTAssertTrue(completed())
        XCTAssertTrue(session.dataActivated)
        XCTAssertNil(session.view, "background activation needs no drawn view")
        XCTAssertEqual(activations(), 0)
        XCTAssertTrue(acted(session).contains("p1 approve"), acted(session))
        let facts = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.agent("{\"op\":\"state\"}").utf8)) as? [String: Any])
        XCTAssertEqual((facts["wake"] as? [String: Any])?["cold"] as? Bool, false)
        XCTAssertNil(NotificationPresenter.shared.opened[ObjectIdentifier(session)])
    }

    func testABackgroundActionActivatesAReplacementWithoutAnotherDraw() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "bootForWake needs the host's UIApplication") }
        let state = try XCTUnwrap(class_getInstanceMethod(UIApplication.self, #selector(getter: UIApplication.applicationState)))
        let background = try XCTUnwrap(class_getInstanceMethod(UIApplication.self, #selector(getter: UIApplication.notificationActionBackgroundState)))
        method_exchangeImplementations(state, background)
        defer { method_exchangeImplementations(state, background) }
        let (session, activations) = self.session()
        try boot(session)
        waitForActivation(session)
        let firstDraw = session.firstDrawMs
        let candidate = try XCTUnwrap(session.prepare(try fixturePlan("NOTIFICATION_ACTIONS"), resolver: ExactApp.shared.resolver, size: CGSize(width: 390, height: 844)))
        session.presentCommitted(try XCTUnwrap(session.commit(candidate)), label: "background replacement")
        XCTAssertFalse(session.dataActivated)
        NotificationPresenter.shared.boot = { $0.bootForWake() }
        let completed = choose(response(entry(session), category: "approve,note", action: "approve"))
        waitForMainQueue()
        waitForActivation(session)
        XCTAssertTrue(session.dataActivated, "the current generation activates without another draw")
        XCTAssertTrue(completed())
        XCTAssertEqual(session.firstDrawMs, firstDraw)
        XCTAssertEqual(activations(), 0)
        XCTAssertTrue(acted(session).contains("p1 approve"), acted(session))
    }

    func testInlineExpiryDoesNotQueueOrBootTheAction() throws {
        let original = OpenAction.beginTask
        defer { OpenAction.beginTask = original }
        var began = false
        OpenAction.beginTask = { expire in
            began = true
            expire()
            return .invalid
        }
        let (session, _) = self.session()
        var boots = 0
        NotificationPresenter.shared.boot = { _ in boots += 1 }
        let completed = choose(response(entry(session), category: "approve,note", action: "approve"))
        XCTAssertTrue(completed())
        XCTAssertTrue(began)
        XCTAssertEqual(boots, 0)
        XCTAssertTrue(session.heldModuleEvents.isEmpty)
    }

    func testExpiryAfterBootBeforeDataActivationDoesNotRun() throws {
        let (session, _) = self.session()
        XCTAssertNil(session.boot(plan: try fixturePlan("NOTIFICATION_ACTIONS"), size: CGSize(width: 390, height: 844)).error)
        let completed = choose(response(entry(session), category: "approve,note", action: "approve"))
        let open = try XCTUnwrap(NotificationPresenter.shared.opened[ObjectIdentifier(session)]?.first)
        NotificationPresenter.shared.expire(open, in: session)
        XCTAssertTrue(completed())
        XCTAssertTrue(session.heldModuleEvents.isEmpty)
        session.drawReceipt().drawn()
        waitForActivation(session)
        XCTAssertFalse(acted(session).contains("p1 approve"), acted(session))
    }

    #endif

    /// Boot `session` on the fixture and draw its first pixel, which
    /// activates its data.
    private func boot(_ session: ExactSession) throws {
        XCTAssertNil(session.boot(plan: try fixturePlan("NOTIFICATION_ACTIONS"), size: CGSize(width: 390, height: 844)).error)
        session.drawReceipt().drawn()
    }

    /// The tree, which shows what the root's handler heard.
    private func acted(_ session: ExactSession) -> String { session.agent("{\"op\":\"tree\"}") }

    private func spin(until done: () -> Bool, seconds: TimeInterval = 10) {
        waitFor("the notification action completes", cap: seconds, done)
    }

    /// The background action: raised as one (`background`), heard by the
    /// root's handler with the action's name, and the delegate's completion
    /// called only once the runner says the invocation ended.
    func testABackgroundActionHoldsTheAppUntilTheRunnerSaysItEnded() throws {
        let (session, activations) = self.session()
        try boot(session)
        spin { session.dataActivated }
        XCTAssertTrue(session.dataActivated)
        let completed = choose(response(entry(session), category: "approve,note", action: "approve"))
        spin { completed() }
        XCTAssertTrue(completed(), "the runner said it ended: \(session.agent("{\"op\":\"actions\"}"))")
        XCTAssertEqual(activations(), 0, "a background action brings nothing forward")
        XCTAssertTrue(acted(session).contains("p1 approve"), acted(session))
        let journal = session.agent("{\"op\":\"logs\"}")
        XCTAssertTrue(journal.contains("action notificationclick p1: completed"), journal)
    }

    func testAnActionWithoutAHandlerReleasesTheApp() throws {
        let (session, _) = self.session()
        try boot(session)
        waitForActivation(session)
        var completions = 0
        NotificationPresenter.shared.background(session, extension: "missing", event: "notificationclick", args: "[]") { completions += 1 }
        XCTAssertEqual(completions, 1)
        XCTAssertNil(NotificationPresenter.shared.opened[ObjectIdentifier(session)])
        XCTAssertTrue(session.agent("{\"op\":\"actions\"}").contains("unheard"))
    }

    /// A background action that finds no booted session (the system launched
    /// the app for it): the host boots one with no view, the action is
    /// delivered once its data is up, and the app is let go after.
    func testABackgroundActionBootsAColdSession() throws {
        let (session, _) = self.session()
        NotificationPresenter.shared.currentCategories = [:]
        var booted = 0
        NotificationPresenter.shared.boot = { target in
            XCTAssertTrue(target === session)
            booted += 1
            try? self.boot(target)
        }
        let completed = choose(response(entry(session), category: "approve,note", action: "approve"))
        XCTAssertEqual(booted, 1)
        XCTAssertFalse(completed(), "held until the session has heard it and its work ended")
        spin { completed() }
        XCTAssertTrue(completed())
        XCTAssertTrue(acted(session).contains("p1 approve"), acted(session))
        let state = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.agent("{\"op\":\"state\"}").utf8)) as? [String: Any])
        XCTAssertEqual((state["wake"] as? [String: Any])?["cold"] as? Bool, true)
        let journal = session.agent("{\"op\":\"logs\"}")
        XCTAssertTrue(journal.contains("action notificationclick p1: raised"), journal)
    }

    func testAnEmbedderBootsOnlyThePostingSession() throws {
        let (a, _) = self.session(label: "a")
        let (b, _) = self.session(label: "b")
        NotificationPresenter.shared.currentCategories = [:]
        var booted: [String] = []
        NotificationPresenter.shared.boot = { target in
            booted.append(target.label)
            try? self.boot(target)
        }
        let completed = choose(response(entry(b), category: "approve,note", action: "approve"))
        spin { completed() }
        XCTAssertTrue(completed())
        XCTAssertEqual(booted, [b.label])
        XCTAssertFalse(a.booted)
        XCTAssertTrue(acted(b).contains("p1 approve"), acted(b))
    }

    /// The system's time ran out before the action ended (here, its session
    /// never booted): the action expires and the app is let go.
    func testExpiryLetsTheAppGo() throws {
        let (session, _) = self.session()
        let completed = choose(response(entry(session), category: "approve,note", action: "approve"))
        XCTAssertFalse(completed())
        let copied = session.heldModuleEvents
        let open = try XCTUnwrap(NotificationPresenter.shared.opened[ObjectIdentifier(session)]?.first)
        NotificationPresenter.shared.expire(open, in: session)
        XCTAssertTrue(completed(), "let go inside the expiration handler")
        XCTAssertNil(NotificationPresenter.shared.opened[ObjectIdentifier(session)])
        XCTAssertTrue(session.heldModuleEvents.isEmpty, "expired actions must leave the preboot queue")
        try boot(session)
        waitForActivation(session)
        XCTAssertFalse(acted(session).contains("p1 approve"), acted(session))
        session.heldModuleEvents = copied
        session.deliverModuleEvents()
        XCTAssertFalse(acted(session).contains("p1 approve"), "a copy taken before expiry must also be refused")
    }
}
#endif
