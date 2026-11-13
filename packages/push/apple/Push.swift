// @exact/push on iOS, iPadOS and macOS (LLP 1133.004 §9 P3, P4): the APNs
// device token as a live value, for the app to send to its own server; Exact
// sends and relays nothing. A Mac takes push's entitlement only under a
// provisioning profile (LLP 1133.010 D7): a build signed without one reads
// unavailable, as a host without push does.
//
// - `subscribePush()` is the one call that may prompt, and only while
//   `notDetermined`: notification permission is asked then (with the options
//   a post asks for), refused `NotAllowedError` while denied, and every
//   authorization `pushPermissionState()` reads as granted (authorized,
//   provisional, ephemeral) registers without asking; then the token the
//   process already holds, or the system's answer to a registration: the
//   token, or `AbortError` with the system's message.
// - `pushSubscription()` never prompts. While permission is granted it reads
//   the token, registering once a launch, as Apple asks of every launch
//   ("Registering your app with APNs"); otherwise it reads null and registers
//   nothing. A change of permission (Settings, then back to the app) is
//   announced to its readers, who read again.
// - The system answers on the app delegate. When the package loads it asks
//   the host to give the delegate the two answering methods, which the host
//   adds where the delegate does not answer them already (an embedder's own
//   forwards to the host itself); so an app that does not import it has
//   neither. Either way the answer reaches the host, which keeps it
//   (`context.push`) and posts each one.
import ExactPackage
import Foundation
#if os(iOS)
import UIKit
private typealias App = UIApplication
#elseif os(macOS)
import AppKit
import Security
private typealias App = NSApplication
#endif
#if canImport(UserNotifications)
import UserNotifications
#endif

public final class PushModule: NSObject, ExactPackage {
    private let context: ExactPackageContext
    /// The token last announced to `pushSubscription()`'s readers.
    private var said: String?
    /// The permission state last read, to announce a change once.
    private var saidState: String?
    /// `subscribePush()` calls waiting for the system's answer, each with its own bound.
    private var waiting: [UUID: (reply: ([String: Any]) -> Void, bound: DispatchWorkItem)] = [:]
    private var observers: [NSObjectProtocol] = []
    /// One registration a launch, for the process: the system's answer is the app's.
    private static var registered = false

    public init(context: ExactPackageContext) {
        self.context = context
        super.init()
        // The app delegate is given the methods that carry the system's answer.
        NotificationCenter.default.post(name: Notification.Name("exact.push.install"), object: nil)
    }

    deinit {
        observers.forEach(NotificationCenter.default.removeObserver)
        waiting.values.forEach { $0.bound.cancel() }
    }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        DispatchQueue.main.async { [self] in
            #if os(iOS) || os(macOS)
            guard Bundle.main.bundleIdentifier != nil, Self.entitled else { return unavailable(name, reply) }
            listen()
            switch name {
            case "pushPermissionState": state { reply($0) }
            case "pushSubscription":
                state { [self] status in
                    guard status["state"] as? String == "granted" else {
                        said = nil
                        return reply(["exact:answer": NSNull()])
                    }
                    register()
                    said = context.push.current["token"] as? String
                    reply(["exact:answer": subscription(said)])
                }
            case "subscribePush": subscribe(reply)
            default: reply(["error": "NotSupportedError"])
            }
            #else
            unavailable(name, reply)
            #endif
        }
    }

    private func unavailable(_ name: String, _ reply: ([String: Any]) -> Void) {
        switch name {
        case "pushPermissionState": reply(["available": false, "state": "unknown"])
        case "pushSubscription": reply(["exact:answer": NSNull()])
        default: reply(["error": "NotSupportedError"])
        }
    }

    /// The value app code reads: the credential under the platform's key.
    private func subscription(_ token: String?) -> Any {
        token.map { ["apple": ["deviceToken": $0]] as Any } ?? NSNull()
    }

    #if os(iOS) || os(macOS)
    /// Whether the build signs push's entitlement: always on iOS, whose
    /// bake writes it; on a Mac only under a profile that allows it.
    private static let entitled: Bool = {
        #if os(macOS)
        guard let task = SecTaskCreateFromSelf(nil) else { return false }
        return SecTaskCopyValueForEntitlement(task, "com.apple.developer.aps-environment" as CFString, nil) != nil
        #else
        return true
        #endif
    }()

    private func listen() {
        guard observers.isEmpty else { return }
        observers.append(NotificationCenter.default.addObserver(forName: ExactPushRegistration.changed, object: nil, queue: .main) { [weak self] note in
            self?.answered(note.userInfo ?? [:])
        })
        // The person may change notification permission in Settings: the
        // subscription's readers read again, and a grant registers.
        observers.append(NotificationCenter.default.addObserver(forName: App.didBecomeActiveNotification, object: nil, queue: .main) { [weak self] _ in
            self?.state { _ in }
        })
    }

    /// Once a launch (Apple: register in every launch, never cache the token).
    private func register() {
        guard !Self.registered else { return }
        Self.registered = true
        App.shared.registerForRemoteNotifications()
    }

    /// The authorization read first: only `prompt` asks, so an authorization
    /// held already (an embedder's provisional one included) never prompts, and
    /// a call the host forwarded because the state read granted cannot either.
    private func subscribe(_ reply: @escaping ([String: Any]) -> Void) {
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { [self] settings in
            switch Self.step(settings.authorizationStatus) {
            case .ask:
                center.requestAuthorization(options: [.alert, .sound, .badge]) { granted, _ in
                    DispatchQueue.main.async { [self] in
                        state { _ in }
                        granted ? subscribed(reply) : reply(["error": "NotAllowedError"])
                    }
                }
            case .refuse: DispatchQueue.main.async { [self] in state { _ in }; reply(["error": "NotAllowedError"]) }
            case .register: DispatchQueue.main.async { [self] in state { _ in }; subscribed(reply) }
            }
        }
    }

    /// Permission granted: the token the process holds, or the system's answer to a registration.
    private func subscribed(_ reply: @escaping ([String: Any]) -> Void) {
        // The process holds the system's answer already (another session's, or an embedder's forwarded one).
        if let token = context.push.current["token"] as? String {
            Self.registered = true
            return reply(["apple": ["deviceToken": token]])
        }
        // Apple promises no answer within any time; a call that had none
        // in 60 s fails alone, and a later answer still reaches the source.
        let id = UUID()
        let bound = DispatchWorkItem { [weak self] in
            guard let call = self?.waiting.removeValue(forKey: id) else { return }
            call.reply(["error": "AbortError", "message": "the system gave no answer to the registration in 60 s"])
        }
        waiting[id] = (reply, bound)
        DispatchQueue.main.asyncAfter(deadline: .now() + 60, execute: bound)
        Self.registered = false
        register()
    }

    /// The system's answer, kept by the host: each waiting subscribe settles,
    /// and a new token is announced to the source's readers.
    private func answered(_ answer: [AnyHashable: Any]) {
        let token = answer["token"] as? String
        let calls = waiting.values
        waiting = [:]
        for call in calls {
            call.bound.cancel()
            if let token { call.reply(["apple": ["deviceToken": token]]) }
            else { call.reply(["error": "AbortError", "message": answer["error"] as? String ?? "registration failed"]) }
        }
        if token != said, saidState == "granted" {
            said = token
            context.changed("push.pushSubscription")
        }
    }

    /// The permission fact: notifications' authorization, which a visible
    /// push shows under. A change is announced to both sources' readers.
    private func state(_ answer: @escaping ([String: Any]) -> Void) {
        UNUserNotificationCenter.current().getNotificationSettings { settings in
            let state = Self.word(settings.authorizationStatus)
            DispatchQueue.main.async { [self] in
                if saidState != nil, saidState != state {
                    context.changed("push.pushPermissionState")
                    context.changed("push.pushSubscription")
                }
                saidState = state
                answer(["available": true, "state": state])
            }
        }
    }
    #endif

    #if canImport(UserNotifications)
    /// What `subscribePush()` does for an authorization, the branch it takes.
    enum Step: String { case ask, refuse, register }

    /// Only `notDetermined` asks; every authorization held already
    /// (authorized, provisional, ephemeral) registers without asking.
    static func step(_ status: UNAuthorizationStatus) -> Step {
        switch status {
        case .notDetermined: .ask
        case .denied: .refuse
        default: .register
        }
    }

    /// The permission state an authorization reads as: the step's, so the
    /// state `pushPermissionState()` reads and what the call does agree.
    static func word(_ status: UNAuthorizationStatus) -> String {
        switch step(status) {
        case .ask: "prompt"
        case .refuse: "denied"
        case .register: "granted"
        }
    }
    #endif
}
