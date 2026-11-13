// A notification's actions (LLP 1133.004 stage 2, as amended 2026-11-12).
//
// - Categories. The app declares its actions as literals on its posts; the
//   plan keeps each list (`declared`, the runner's), and the host registers
//   one category per list once the session has booted, its identifier the
//   actions' names joined by commas: what a post's content names, and what
//   a push's `aps.category` names. The lists are also kept in the app's
//   defaults only to identify categories Exact owns when reconciling a
//   later plan. Responses wait for the current plan's declarations.
//   Categories an embedder registered itself are kept.
// - A chosen action is read only against those declarations: a category
//   that is not the app's, or an action it does not declare, raises
//   nothing. A push's payload contributes nothing but its identifier as the
//   tag; which action the person chose and what they typed are the system's
//   report (§9 P5 holds for the rest of the payload).
// - A foreground action is `notificationclick` with the action's name and
//   the reply, a person's activation, and brings the app forward. A
//   background action (`foreground: false`) is raised as a background
//   action: the session boots with no view if it has not (iOS, LLP 1133.017
//   D7), the runner runs the handler as an invocation that owns its work,
//   and the host keeps the app running, the delegate's completion handler
//   and a background task held, until the runner says the invocation ended
//   or the system's time runs out; then it expires the action and lets the
//   app be suspended at once.
import Foundation
import CryptoKit
import Security
#if canImport(UIKit)
import UIKit
#endif
#if !os(tvOS)
import UserNotifications

/// One declared action, as the plan's literal writes it.
struct DeclaredAction: Equatable {
    let action: String, title: String, foreground: Bool, destructive: Bool, authenticationRequired: Bool
    let placeholder: String?

    init?(_ json: [String: Any]) {
        guard let action = json["action"] as? String, !action.isEmpty else { return nil }
        self.action = action
        title = json["title"] as? String ?? action
        foreground = json["foreground"] as? Bool ?? true
        destructive = json["destructive"] as? Bool ?? false
        authenticationRequired = json["authenticationRequired"] as? Bool ?? false
        placeholder = json["placeholder"] as? String
    }

    /// The system returns this revision, not just the authored name. An old
    /// button cannot borrow a new definition's authentication or foreground flags.
    var identifier: String {
        let fields: [Any] = [action, title, foreground, destructive, authenticationRequired, placeholder as Any? ?? NSNull()]
        let data = try! JSONSerialization.data(withJSONObject: fields, options: [.sortedKeys])
        return action + "=" + SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    /// The platform's action: a text field's for one with a placeholder.
    var platform: UNNotificationAction {
        var options: UNNotificationActionOptions = []
        if foreground { options.insert(.foreground) }
        if destructive { options.insert(.destructive) }
        if authenticationRequired { options.insert(.authenticationRequired) }
        guard let placeholder else { return UNNotificationAction(identifier: identifier, title: title, options: options) }
        return UNTextInputNotificationAction(identifier: identifier, title: title, options: options,
                                             textInputButtonTitle: title, textInputPlaceholder: placeholder)
    }
}

/// The app's declared categories, by identifier.
enum DeclaredCategories {
    static let key = "exact.notifications.categories"

    /// A list's category: the identifier a post and a push name it by.
    static func identifier(_ actions: [DeclaredAction]) -> String { actions.map(\.action).joined(separator: ",") }

    /// The lists the runner's `declared` op answers, read from its JSON.
    static func lists(_ declared: String) -> [[DeclaredAction]] {
        guard let json = try? JSONSerialization.jsonObject(with: Data(declared.utf8)) as? [String: Any],
              let lists = (json["notifications.showNotification"] as? [String: Any])?["actions"] as? [[[String: Any]]] else { return [] }
        return lists.map { $0.compactMap(DeclaredAction.init) }.filter { !$0.isEmpty }
    }

    /// The last registered Exact definitions, kept for ownership reconciliation.
    static var kept: [String: [DeclaredAction]] {
        guard let data = UserDefaults.standard.data(forKey: key),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: [[String: Any]]] else { return [:] }
        return json.mapValues { $0.compactMap(DeclaredAction.init) }
    }

    static func keep(_ categories: [String: [DeclaredAction]]) {
        let json = categories.mapValues { $0.map { a -> [String: Any] in
            var o: [String: Any] = ["action": a.action, "title": a.title, "foreground": a.foreground,
                                    "destructive": a.destructive, "authenticationRequired": a.authenticationRequired]
            if let p = a.placeholder { o["placeholder"] = p }
            return o
        } }
        if let data = try? JSONSerialization.data(withJSONObject: json) { UserDefaults.standard.set(data, forKey: key) }
    }
}

/// LLP 1133.017 D6's marker tests first unlock, unlike protected-data availability,
/// which becomes false again each time the person locks the device.
enum NotificationFirstUnlock {
    static func available(
        read: (CFDictionary) -> OSStatus = { SecItemCopyMatching($0, nil) },
        add: (CFDictionary) -> OSStatus = { SecItemAdd($0, nil) }
    ) -> Bool {
        let item: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: (Bundle.main.bundleIdentifier ?? "exact") + ".exact.first-unlock",
            kSecAttrAccount: "marker",
            kSecUseDataProtectionKeychain: true,
        ]
        var query = item
        query[kSecReturnData] = true
        query[kSecMatchLimit] = kSecMatchLimitOne
        query[kSecUseAuthenticationUI] = kSecUseAuthenticationUIFail
        let status = read(query as CFDictionary)
        if status == errSecSuccess { return true }
        guard status == errSecItemNotFound else { return false }
        var marker = item
        marker[kSecAttrAccessible] = kSecAttrAccessibleAfterFirstUnlock
        marker[kSecValueData] = Data([1])
        let added = add(marker as CFDictionary)
        guard added == errSecSuccess || added == errSecDuplicateItem else { return false }
        return read(query as CFDictionary) == errSecSuccess
    }
}

/// A background action the host holds open: the delegate's completion
/// handler and the background task, until the runner says it ended.
final class OpenAction {
    let id: String
    var cold: Bool
    weak var session: ExactSession?
    private var done: (() -> Void)?
    var waiting: (() -> Void)?
    var raised = false
    var generation: Int?
    #if canImport(UIKit)
    var task = UIBackgroundTaskIdentifier.invalid
    static var beginTask: (@escaping () -> Void) -> UIBackgroundTaskIdentifier = {
        UIApplication.shared.beginBackgroundTask(withName: "exact.notification-action", expirationHandler: $0)
    }
    #endif
    init(id: String, cold: Bool, done: @escaping () -> Void) { self.id = id; self.cold = cold; self.done = done }

    /// Once: the system may suspend the app.
    func finish() {
        let d = done
        done = nil
        waiting = nil
        #if canImport(UIKit)
        if task != .invalid { UIApplication.shared.endBackgroundTask(task); task = .invalid }
        #endif
        d?()
    }
    var finished: Bool { done == nil }
}

extension NotificationPresenter {
    /// Register the union of live sessions' current declarations. Conflicting
    /// definitions of a category are neither registered nor authorized.
    func declare(_ lists: [[DeclaredAction]], for session: ExactSession, ready: @escaping () -> Void = {}) {
        var mine: [String: [DeclaredAction]] = [:]
        for list in lists { mine[DeclaredCategories.identifier(list)] = list }
        currentCategories[ObjectIdentifier(session)] = (session.generation, mine, false)
        updateCategories(ready)
    }

    private func declarations(for session: ExactSession, registered: Bool = false) -> [String: [DeclaredAction]]? {
        guard session.state != .destroyed, let current = currentCategories[ObjectIdentifier(session)],
              current.generation == session.generation, !registered || current.ready else { return nil }
        return current.categories
    }

    private var combinedCategories: [String: [DeclaredAction]] {
        var combined: [String: [DeclaredAction]] = [:]
        var conflicts = Set<String>()
        for session in app?.sessions ?? [] {
            for (id, actions) in declarations(for: session) ?? [:] {
                if let prior = combined[id], prior != actions { conflicts.insert(id) }
                combined[id] = actions
            }
        }
        for id in conflicts { combined[id] = nil }
        return combined
    }

    private func updateCategories(_ ready: @escaping () -> Void = {}) {
        categoryUpdates.append(ready)
        reconcileCategories()
    }

    // Category state, confined to main:
    // - A live session declares (generation, lists). Only its current generation
    //   participates; it becomes ready after the centre's write, before data starts.
    // - `kept` owns exactly the last written Exact definitions, across launches.
    //   A registered definition not matching that ownership belongs to the embedder:
    //   preserve it and refuse an Exact claim of its identifier. Conflicting live
    //   declarations are refused too. Removal forgets ownership, permitting reuse.
    // - A response needs its destination's ready declaration and the registered
    //   union to agree after data activation, including the definition revision in
    //   the system's action ID.
    //   Saved ownership never authorizes a response. A changed definition refuses old
    //   buttons even when the category ID is unchanged or the process has restarted.
    // - Replacement supersedes actions already assigned to the old runner, but keeps
    //   responses awaiting declarations. Teardown removes that session's declarations
    //   and releases its responses; another session's equal declaration keeps ownership.
    private func reconcileCategories() {
        guard !reconcilingCategories, !categoryUpdates.isEmpty else { return }
        reconcilingCategories = true
        let centre = categories
        centre.read { [self] registered in
            let updates = categoryUpdates
            categoryUpdates = []
            let owned = DeclaredCategories.kept
            let others = registered.filter { category in
                guard let prior = owned[category.identifier] else { return true }
                return !category.isEqual(Self.category(category.identifier, prior))
            }
            let foreign = Set(others.map(\.identifier))
            let ours = combinedCategories.filter { !foreign.contains($0.key) }
            centre.write(others.union(ours.map { Self.category($0.key, $0.value) }))
            DeclaredCategories.keep(ours)
            registeredCategories = ours
            for session in app?.sessions ?? [] where declarations(for: session) != nil {
                currentCategories[ObjectIdentifier(session)]?.ready = true
            }
            reconcilingCategories = false
            updates.forEach { $0() }
            for session in app?.sessions ?? [] { deliverWaiting(session) }
            reconcileCategories()
        }
    }

    func deliverWaiting(_ session: ExactSession) {
        guard declarations(for: session, registered: true) != nil,
              !session.booted || session.dataActivated else { return }
        for open in opened[ObjectIdentifier(session)] ?? [] {
            let waiting = open.waiting
            open.waiting = nil
            waiting?()
        }
    }

    private static func category(_ id: String, _ actions: [DeclaredAction]) -> UNNotificationCategory {
        UNNotificationCategory(identifier: id, actions: actions.map(\.platform), intentIdentifiers: [], options: [])
    }

    /// The declared action the person chose on a notification of category
    /// `category`; nil for a category or an action the app did not declare.
    func declared(_ action: String, in category: String, for session: ExactSession) -> DeclaredAction? {
        guard let own = declarations(for: session, registered: true)?[category],
              combinedCategories[category] == own, registeredCategories[category] == own else { return nil }
        return own.first { $0.action == action }
    }

    /// A chosen action (anything but the default and the dismissal):
    /// raised as the declarations say, `done` called when the app may be
    /// suspended. A local post carries its activation entry; a push's
    /// identifier is its tag.
    func chose(_ response: UNNotificationResponse, to chosen: ExactSession?, held: OpenAction? = nil, done: @escaping () -> Void) {
        let notification = response.notification
        let request = notification.request
        let push = request.trigger is UNPushNotificationTrigger
        let session: ExactSession?
        if push {
            let candidates = ([chosen].compactMap { $0 } + (app?.sessions ?? [])).filter { $0.state != .destroyed }
            let category = request.content.categoryIdentifier
            let action = registeredCategories[category]?.first { $0.identifier == response.actionIdentifier }
            session = candidates.first { target in
                action.map { declared($0.action, in: category, for: target) == $0 } ?? false
            } ?? candidates.first { declarations(for: $0, registered: true) == nil } ?? candidates.first
        }
        else { session = (request.content.userInfo["exact"] as? [AnyHashable: Any]).flatMap(postingSession) }
        guard let session, session.state != .destroyed else { return done() }
        guard session.dataActivated || firstUnlockAvailable() else {
            session.log("notification action refused: AfterFirstUnlock Keychain marker is inaccessible")
            return done()
        }
        if let held, held.session !== session {
            if let prior = held.session {
                let key = ObjectIdentifier(prior)
                opened[key]?.removeAll { $0 === held }
                if opened[key]?.isEmpty == true { opened[key] = nil }
            }
            held.session = session
            held.cold = !session.booted
            opened[ObjectIdentifier(session), default: []].append(held)
        }
        if declarations(for: session, registered: true) == nil || (session.booted && !session.dataActivated) {
            let open = held ?? hold(session, done: done)
            guard !open.finished else { return }
            open.waiting = { [weak self, weak chosen, weak open] in
                guard let self, let open, !open.finished else { return }
                chose(response, to: chosen, held: open) {
                    guard let target = open.session else { return open.finish() }
                    self.settle(target, ended: [open.id])
                }
            }
            boot?(session)
            return
        }
        guard let action = registeredCategories[request.content.categoryIdentifier]?.first(where: { $0.identifier == response.actionIdentifier }),
              declared(action.action, in: request.content.categoryIdentifier, for: session) == action,
              firstDelivery(request.identifier + "#" + response.actionIdentifier, notification.date) else {
            FileHandle.standardError.write(Data("notification action refused: \(response.actionIdentifier) is not declared in \(request.content.categoryIdentifier)\n".utf8))
            return done()
        }
        let reply = action.placeholder == nil ? nil : (response as? UNTextInputNotificationResponse)?.userText ?? ""
        let raised: (session: ExactSession, extension: String, event: String, args: String)?
        if push { raised = pushAction(id: request.identifier, action: action.action, reply: reply, to: session) }
        else { raised = localAction(info: request.content.userInfo, action: action.action, reply: reply) }
        guard let raised else { return done() }
        guard !action.foreground else {
            raised.session.raiseModuleEvent(extension: raised.extension, event: raised.event, args: raised.args, activation: true)
            activate?(raised.session)
            return done()
        }
        background(raised.session, extension: raised.extension, event: raised.event, args: raised.args, held: held, done: done)
    }

    /// A push's action: `notificationclick` of `@exact/notifications` with
    /// the request's identifier as the tag, the action and the reply, and
    /// nothing else of the payload.
    private func pushAction(id: String, action: String, reply: String?, to chosen: ExactSession?)
        -> (session: ExactSession, extension: String, event: String, args: String)? {
        guard let session = chosen ?? app?.sessions.first(where: { $0.state != .destroyed }), session.state != .destroyed else { return nil }
        let event: [String: Any] = ["tag": id, "data": NSNull(), "title": "", "body": NSNull(), "action": action, "reply": reply ?? NSNull()]
        guard let data = try? JSONSerialization.data(withJSONObject: [id, event]) else { return nil }
        return (session, "notifications", "notificationclick", String(decoding: data, as: UTF8.self))
    }

    /// A local post's action: its activation entry's event, on the session
    /// that posted it, with the action and the reply written into the
    /// argument the entry names (`chosen`). Never its `navigate`.
    private func localAction(info: [AnyHashable: Any], action: String, reply: String?)
        -> (session: ExactSession, extension: String, event: String, args: String)? {
        guard let entry = info["exact"] as? [AnyHashable: Any], let name = entry["extension"] as? String,
              let event = entry["event"] as? String, let session = postingSession(entry) else { return nil }
        let args = Self.choosing(action, reply: reply, in: entry["args"] as? String ?? "[]", at: entry["chosen"] as? Int)
        return (session, name, event, args)
    }

    /// `args` with `action` and `reply` written into the argument at
    /// `index`, an object; unchanged when the entry names none.
    static func choosing(_ action: String, reply: String?, in args: String, at index: Int?) -> String {
        guard let index, var list = (try? JSONSerialization.jsonObject(with: Data(args.utf8))) as? [Any],
              list.indices.contains(index), var event = list[index] as? [String: Any] else { return args }
        event["action"] = action
        event["reply"] = reply ?? NSNull()
        list[index] = event
        return (try? JSONSerialization.data(withJSONObject: list)).map { String(decoding: $0, as: UTF8.self) } ?? args
    }

    /// Hold the response while the session boots or its action runs.
    private func hold(_ session: ExactSession, done: @escaping () -> Void) -> OpenAction {
        let open = OpenAction(id: UUID().uuidString, cold: !session.booted, done: done)
        open.session = session
        opened[ObjectIdentifier(session), default: []].append(open)
        #if os(iOS)
        open.task = OpenAction.beginTask { [weak self, weak open] in
            guard let open else { return }
            guard let session = open.session else { return open.finish() }
            self?.expire(open, in: session)
        }
        // UIKit may expire inline, before it returns the task identifier.
        if open.finished { open.finish() }
        #endif
        return open
    }

    func background(_ session: ExactSession, extension name: String, event: String, args: String,
                    held: OpenAction? = nil, done: @escaping () -> Void) {
        let open = held ?? hold(session, done: done)
        guard !open.finished else { return }
        #if os(iOS)
        let hidden = UIApplication.shared.applicationState == .background
        #else
        let hidden = false
        #endif
        open.generation = session.booted ? session.generation : nil
        session.raiseModuleEvent(extension: name, event: event, args: args, activation: true,
                                 background: (id: open.id, cold: open.cold, hidden: hidden))
        if !session.booted { boot?(session) }
    }

    /// The system's time is up: the runner expires the action, failing its
    /// requests in flight, and the app is let go at once, inside the
    /// expiration handler, as UIKit requires.
    func expire(_ open: OpenAction, in session: ExactSession) {
        guard !open.finished else { return }
        session.heldModuleEvents.removeAll { json in
            let event = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [String: Any]
            return event?["background"] as? String == open.id
        }
        if open.raised && open.generation == session.generation { session.expireBackgroundAction(open.id) }
        settle(session, ended: [open.id])
    }

    func replaced(_ session: ExactSession) {
        for open in opened[ObjectIdentifier(session)] ?? [] {
            if let generation = open.generation, generation != session.generation { expire(open, in: session) }
        }
    }

    /// Drain endings when work lands, including batches that skip presentation.
    func applied(_ session: ExactSession) {
        guard let open = opened[ObjectIdentifier(session)], !open.isEmpty else { return }
        let reply = session.agent("{\"op\":\"actions\",\"drain\":true}")
        let ended = ((try? JSONSerialization.jsonObject(with: Data(reply.utf8))) as? [String: Any])?["ended"] as? [[String: Any]] ?? []
        settle(session, ended: Set(ended.compactMap { $0["id"] as? String }))
    }

    /// Let go of `ended`, or of every action when the session ends.
    func settle(_ session: ExactSession, ended: Set<String>, all: Bool = false) {
        let key = ObjectIdentifier(session)
        for open in opened[key] ?? [] where all || ended.contains(open.id) { open.finish() }
        opened[key] = (opened[key] ?? []).filter { !$0.finished }
        if opened[key]?.isEmpty == true { opened[key] = nil }
        if all, currentCategories.removeValue(forKey: key) != nil {
            updateCategories()
        }
    }
}

extension ExactSession {
    /// Registration precedes data code and delivery of launching actions.
    func declareNotificationCategories(ready: @escaping () -> Void) {
        NotificationPresenter.shared.declare(DeclaredCategories.lists(agent("{\"op\":\"declared\"}")), for: self, ready: ready)
    }

    func replaceBackgroundActions() { NotificationPresenter.shared.replaced(self) }
    func deliverNotificationResponses() { NotificationPresenter.shared.deliverWaiting(self) }

    /// The platform's time for background action `id` ran out.
    func expireBackgroundAction(_ id: String) {
        guard booted, let data = try? JSONSerialization.data(withJSONObject: ["expire": id]) else { return }
        apply(runtime.moduleEvent(String(decoding: data, as: UTF8.self)))
    }

    /// An event copied out of the preboot queue may have expired meanwhile.
    func beginBackgroundAction(_ id: String) -> Bool {
        guard let open = NotificationPresenter.shared.opened[ObjectIdentifier(self)]?.first(where: { $0.id == id }), !open.finished else { return false }
        open.raised = true
        open.generation = generation
        return true
    }

    /// After work lands: a background action it ended lets the app go.
    func backgroundActionsApplied() { NotificationPresenter.shared.applied(self) }

    /// The session ends: every background action it held lets the app go.
    func endBackgroundActions() { NotificationPresenter.shared.settle(self, ended: [], all: true) }
}
#else
extension ExactSession {
    func declareNotificationCategories(ready: @escaping () -> Void) { ready() }
    func replaceBackgroundActions() {}
    func deliverNotificationResponses() {}
    func beginBackgroundAction(_ id: String) -> Bool { false }
    func backgroundActionsApplied() {}
    func endBackgroundActions() {}
}
#endif
