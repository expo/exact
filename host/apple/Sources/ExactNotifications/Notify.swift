// The one notification delegate a process has (LLP 1133.004 D7), in
// ExactNotifications: an embedder links that product and calls
// `ExactApp.installNotificationClicks` before launch completes. It is set at launch,
// before the system can deliver a launching click, which a module's
// implementation could not be: a module is made when its session boots.
// A click on content a module posted (`@exact/notifications`) carries the
// host's activation entry (`userInfo["exact"]`): it opens the entry's
// `navigate` location, or raises the module's event on the session that
// posted it, held until that session can hear it. A remote notification's
// payload is its sender's: its click opens the payload's top-level
// `navigate`, or the Expo push service's `body.navigate`, as a local
// notification's opens, and nothing else of the payload is read
// (LLP 1133.004 §9 P5). A chosen action is the app's
// declared action, read against its declarations (NotifyActions.swift).
// A notification shows with the app in front
// too, as a browser shows a page's, and sounds only when its content has a
// sound (a silent post has none).
import Foundation
import ExactKit
#if !os(tvOS)
import UserNotifications
#endif

#if !os(tvOS)
/// Shows a notification while the app is in front, as a browser shows one
/// over its own page; the system would otherwise drop it silently.
final class NotificationPresenter: NSObject, UNUserNotificationCenterDelegate {
    static let shared = NotificationPresenter()
    static var available: Bool {
        // UserNotifications needs a Launch Services bundle: a sidecar
        // Info.plist gives a bare executable an identifier, and an XCTest
        // runner has one too, yet `current()` raises in both.
        #if os(macOS)
        Bundle.main.bundleURL.pathExtension == "app"
        #else
        ["app", "appex"].contains(Bundle.main.bundleURL.pathExtension)
        #endif
    }
    weak var app: ExactApp?
    var activate: ((ExactSession) -> Void)?
    /// Boots a session with no view for a background action that launched
    /// the app with none (iOS: `bootForWake`).
    var boot: ((ExactSession) -> Void)?
    /// A background boot must be able to read AfterFirstUnlock secrets (LLP 1133.017 D6).
    var firstUnlockAvailable: () -> Bool = {
        #if os(iOS)
        NotificationFirstUnlock.available()
        #else
        true
        #endif
    }
    /// The background actions held open, by session (NotifyActions.swift).
    var opened: [ObjectIdentifier: [OpenAction]] = [:]
    /// The centre's registered categories, read and replaced; a test stands
    /// in for them, since an XCTest process has no notification centre.
    var categories: (read: (@escaping (Set<UNNotificationCategory>) -> Void) -> Void, write: (Set<UNNotificationCategory>) -> Void) = (
        read: { reply in
            guard NotificationPresenter.available else { return reply([]) }
            UNUserNotificationCenter.current().getNotificationCategories { categories in
                DispatchQueue.main.async { reply(categories) }
            }
        },
        write: {
            guard NotificationPresenter.available else { return }
            UNUserNotificationCenter.current().setNotificationCategories($0)
        })
    /// Each session's declarations belong to one plan generation, never to defaults.
    var currentCategories: [ObjectIdentifier: (generation: Int, categories: [String: [DeclaredAction]], ready: Bool)] = [:]
    var registeredCategories: [String: [DeclaredAction]] = [:]
    var categoryUpdates: [() -> Void] = []
    var reconcilingCategories = false
    private var delivered = Set<String>()
    private var order: [String] = []

    func install(on app: ExactApp, activate: ((ExactSession) -> Void)?, boot: ((ExactSession) -> Void)?) {
        self.app = app
        self.activate = activate
        self.boot = boot
        #if os(iOS)
        _ = firstUnlockAvailable() // Keep the marker even in an app with no secrets.
        #endif
        guard Self.available else { return }
        let center = UNUserNotificationCenter.current()
        if center.delegate == nil || center.delegate === self { center.delegate = self }
    }

    /// `done` once the app may be suspended: at once, or when a background
    /// action has ended.
    func deliver(_ response: UNNotificationResponse, to chosen: ExactSession? = nil, done: @escaping () -> Void = {}) {
        switch response.actionIdentifier {
        case UNNotificationDefaultActionIdentifier: break
        case UNNotificationDismissActionIdentifier: return done()
        default: return chose(response, to: chosen, done: done)
        }
        defer { done() }
        let notification = response.notification
        let request = notification.request
        // A remote notification's `userInfo` could name any session and
        // module event: only a post of this app's own is read for an
        // activation entry. The trigger is the system's, never the payload's.
        if request.trigger is UNPushNotificationTrigger {
            return deliverPush(id: request.identifier, date: notification.date, info: request.content.userInfo, to: chosen)
        }
        deliver(id: request.identifier, date: notification.date, info: request.content.userInfo)
    }

    /// A push's tap: its `navigate`, a string at the top level or, in a push
    /// the Expo push service sent, in the `body` object it sends a message's
    /// `data` as; the top level first. It opens as a local notification's does
    /// (`openLocation`, the router's `location_of`), so it reaches only the
    /// app's own routes; the app's link handlers are not the
    /// sender's to pick. It opens in `chosen`, or the first live session when
    /// none was chosen; a chosen session that is gone gets nothing, and no
    /// other session stands in for it. Nothing else is read, no event is
    /// raised, and without a location nothing is opened.
    /// The message's `data` in a push the Expo push service sent: the `body`
    /// object beside Expo's own `experienceId` or `scopeKey`. Another sender's
    /// `body` is not read (LLP 1133.004 §9 P5).
    private func expoData(_ info: [AnyHashable: Any]) -> [AnyHashable: Any]? {
        guard info["experienceId"] is String || info["scopeKey"] is String else { return nil }
        return info["body"] as? [AnyHashable: Any]
    }
    func deliverPush(id: String, date: Date, info: [AnyHashable: Any], to chosen: ExactSession?) {
        guard let text = (info["navigate"] ?? expoData(info)?["navigate"]) as? String,
              !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, firstDelivery(id, date) else { return }
        let session = chosen ?? app?.sessions.first { $0.state != .destroyed }
        guard let session, session.state != .destroyed else {
            let line = chosen == nil ? "no session is live" : "its session \(chosen!.label) is gone"
            FileHandle.standardError.write(Data("notification click refused: \(line)\n".utf8))
            return
        }
        session.openLocation(text)
        activate?(session)
    }

    /// Whether this is the first delivery of a notification: the delegate
    /// and a scene's launch options may both hand over one tap.
    func firstDelivery(_ id: String, _ date: Date) -> Bool {
        let key = "\(id):\(date.timeIntervalSince1970)"
        guard delivered.insert(key).inserted else { return false }
        order.append(key)
        if order.count > 128 { delivered.remove(order.removeFirst()) }
        return true
    }

    /// A module's activation entry (LLP 1133.004 D6, D7): the session that
    /// posted it, and a location to open or an event to raise.
    func deliver(id: String, date: Date, info: [AnyHashable: Any]) {
        // A notification an older build scheduled carries its session fields
        // and `navigate` at the top level of `userInfo`.
        guard let entry = localEntry(info),
              entry["session"] is String, entry["owner"] is String else { return }
        let location = (entry["navigate"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        guard firstDelivery(id, date), let session = postingSession(entry) else { return }
        if let location { session.openLocation(location) }
        else if let name = entry["extension"] as? String, let event = entry["event"] as? String {
            session.raiseModuleEvent(extension: name, event: event, args: entry["args"] as? String ?? "[]", activation: true)
        }
        activate?(session)
    }

    /// The session that posted the entry: by its owner identity in this
    /// process; by its label, else the first live one, for a post of a
    /// previous process. Nil, journaled, when it is gone.
    func postingSession(_ entry: [AnyHashable: Any]) -> ExactSession? {
        guard let label = entry["session"] as? String, let owner = entry["owner"] as? String else { return nil }
        let sessions = app?.sessions.filter { $0.state != .destroyed } ?? []
        let sameProcess = (entry["process"] as? String).map { $0 == app?.notificationProcessID } ?? true
        let session: ExactSession?
        if sameProcess { session = sessions.first { $0.notificationOwnerID == owner } }
        else { session = sessions.first { $0.label == label } ?? sessions.first }
        if session == nil {
            let line = "notification click refused: session \(label) is gone"
            sessions.forEach { $0.log(line) }
            if sessions.isEmpty { FileHandle.standardError.write(Data("\(line)\n".utf8)) }
        }
        return session
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse,
                                withCompletionHandler completionHandler: @escaping () -> Void) {
        receive(response, then: completionHandler)
    }

    /// Every tap, the delegate's and an embedder's forwarded one, is delivered
    /// here, on the main thread: a session's navigation reaches its views, and
    /// the record of what was delivered is read and written on one thread.
    func receive(_ response: UNNotificationResponse, to session: ExactSession? = nil, then done: (() -> Void)? = nil) {
        let handle = { self.deliver(response, to: session) { done?() } }
        if Thread.isMainThread { handle() } else { DispatchQueue.main.async(execute: handle) }
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification,
                                withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {
        let request = notification.request
        let options: UNNotificationPresentationOptions = request.content.sound == nil ? [.banner, .list] : [.banner, .list, .sound]
        present(request, options: options, complete: completionHandler)
    }

    private func localEntry(_ info: [AnyHashable: Any]) -> [AnyHashable: Any]? {
        info["exact"] as? [AnyHashable: Any] ?? (info["navigate"] != nil ? info : nil)
    }

    func present(_ request: UNNotificationRequest, options: UNNotificationPresentationOptions,
                 complete: @escaping (UNNotificationPresentationOptions) -> Void) {
        // Only the system sets the trigger. A push can copy every field of a local post's userInfo.
        guard !(request.trigger is UNPushNotificationTrigger),
              !options.intersection([.banner, .list, .alert]).isEmpty,
              let entry = localEntry(request.content.userInfo),
              entry["session"] is String, entry["owner"] is String else { return complete(options) }
        let present = {
            let visible = !PageFacts.hidden
            complete(options)
            if visible { self.shown(request, entry: entry) }
        }
        if Thread.isMainThread { present() } else { DispatchQueue.main.async(execute: present) }
    }

    private func shown(_ request: UNNotificationRequest, entry: [AnyHashable: Any]) {
        let content = request.content
        guard let session = postingSession(entry) else { return }
        let fields: [String: Any]
        if let name = entry["extension"] as? String {
            guard name == "notifications", entry["event"] as? String == "notificationclick",
                  let text = entry["args"] as? String,
                  let args = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [Any],
                  args.count == 2, let original = args[1] as? [String: Any], let tag = args[0] as? String else { return }
            fields = ["tag": tag, "title": original["title"] ?? content.title,
                      "body": original["body"] ?? NSNull(), "data": original["data"] ?? NSNull()]
        } else {
            // The original navigation-only post kept its title and body in the request.
            guard content.userInfo["exact"] == nil, entry["navigate"] is String else { return }
            fields = ["tag": entry["tag"] as? String ?? "", "title": content.title,
                      "body": content.body.isEmpty ? NSNull() as Any : content.body, "data": entry["data"] ?? NSNull()]
        }
        guard let json = try? JSONSerialization.data(withJSONObject: [fields["tag"]!, fields]) else { return }
        session.raiseModuleEvent(extension: "notifications", event: "notificationshow",
                                 args: String(decoding: json, as: UTF8.self), activation: false)
    }
}

extension ExactApp {
    /// Install at launch, before the system can deliver a launching tap.
    /// `boot` receives the selected session and boots it with no view for a
    /// background launch (LLP 1133.017 D7; iOS's `bootForWake`).
    public func installNotificationClicks(boot: ((ExactSession) -> Void)? = nil, activate: ((ExactSession) -> Void)? = nil) {
        NotificationPresenter.shared.install(on: self, activate: activate, boot: boot)
        PushRegistration.listen()
    }

    /// An embedder that owns the delegate forwards its final options and the system's completion handler together.
    public func presentNotification(_ notification: UNNotification, options: UNNotificationPresentationOptions,
                                    completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {
        NotificationPresenter.shared.present(notification.request, options: options, complete: completionHandler)
    }

    /// A scene's launch response may also reach the delegate; one request is
    /// delivered once. A push's tap opens in `session`, else the first live
    /// one: an embedder that keeps the delegate itself forwards its taps here
    /// and names the session (LLP 1031 D4, D5).
    /// It may be called on any thread; the tap is delivered on the main one.
    public func openNotification(_ response: UNNotificationResponse, session: ExactSession? = nil) {
        NotificationPresenter.shared.receive(response, to: session)
    }
}
#endif
