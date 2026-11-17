// The app's remote-notification registration (LLP 1133.004 §9 P3). The
// system answers a registration on the app delegate. When `@exact/push`
// loads it asks (`exact.push.install`) for the delegate to be given the two
// methods that carry the answer, which are added only where the delegate
// does not answer them already, its own or inherited; an embedder whose
// delegate answers them forwards the answer with the two entries below.
// Either way it is kept here app-wide: the last device token, in lowercase
// hex, or the last failure. `@exact/push`
// reaches it through `context.push` (`exact.push`, an `NSMutableDictionary`
// whose `reply` this sets: `{op: read}` answers `{token?, error?,
// answers}`), and hears each answer as `exact.push.changed`, a token the
// same as the last included, since a registration waits for its answer.
//
// ExactKit never registers, and no class implements either delegate method
// until the package asks, so an app that does not import it has neither. Every entry may be called on any
// thread; the store is read and written on the main thread only.
import Foundation
#if os(iOS)
import ObjectiveC
import UIKit
private typealias PushApp = UIApplication
private typealias PushAppDelegate = UIApplicationDelegate
#elseif os(macOS)
import AppKit
import ObjectiveC
private typealias PushApp = NSApplication
private typealias PushAppDelegate = NSApplicationDelegate
#endif

enum PushRegistration {
    private static var token: String?
    private static var failure: String?
    /// How many answers the system gave this process.
    private static var answers = 0
    private static var listening = false

    private static func onMain(_ body: @escaping () -> Void) {
        if Thread.isMainThread { body() } else { DispatchQueue.main.async(execute: body) }
    }

    /// Before any package can ask: at launch, beside the notification delegate.
    static func listen() {
        guard !listening else { return }
        listening = true
        NotificationCenter.default.addObserver(forName: Notification.Name("exact.push"), object: nil, queue: nil) { note in
            guard let box = note.object as? NSMutableDictionary else { return }
            let run = { box["reply"] = read }
            if Thread.isMainThread { run() } else { DispatchQueue.main.sync(execute: run) }
        }
        #if os(iOS) || os(macOS)
        NotificationCenter.default.addObserver(forName: Notification.Name("exact.push.install"), object: nil, queue: nil) { _ in
            let install = { answerOnTheAppDelegate() }
            if Thread.isMainThread { install() } else { DispatchQueue.main.sync(execute: install) }
        }
        #endif
    }

    #if os(iOS) || os(macOS)
    // The same two selectors on UIKit's and AppKit's application delegates.
    static let selectors = [NSSelectorFromString("application:didRegisterForRemoteNotificationsWithDeviceToken:"),
                            NSSelectorFromString("application:didFailToRegisterForRemoteNotificationsWithError:")]
    private static var keptDelegate: PushAppDelegate?
    private static var installed = false

    /// The app's delegate given the two answering methods (App Store Connect
    /// warns of an app that answers a registration without `aps-environment`,
    /// so only an app that registers has them). UIKit and AppKit read which
    /// methods their delegate has when the delegate is set, so it is set
    /// again, and kept: neither holds a delegate it is told to forget.
    static func answerOnTheAppDelegate() {
        guard !installed, let delegate = PushApp.shared.delegate else { return }
        installed = true
        guard install(on: delegate) else { return }
        keptDelegate = delegate
        PushApp.shared.delegate = nil
        PushApp.shared.delegate = delegate
    }

    /// Adds to `object`'s class each answering method it does not answer
    /// already, its own or inherited: an embedder's keeps running. Whether
    /// any was added.
    @discardableResult
    static func install(on object: AnyObject) -> Bool {
        guard let type = object_getClass(object) else { return false }
        let registered: @convention(block) (AnyObject, AnyObject, Data) -> Void = { _, _, token in PushRegistration.registered(token) }
        let failed: @convention(block) (AnyObject, AnyObject, NSError) -> Void = { _, _, error in PushRegistration.failed(error.localizedDescription) }
        var added = false
        for (selector, block) in zip(selectors, [registered as Any, failed as Any]) where class_getInstanceMethod(type, selector) == nil {
            added = class_addMethod(type, selector, imp_implementationWithBlock(block), "v@:@@") || added
        }
        return added
    }
    #endif

    static var read: [String: Any] {
        var out: [String: Any] = ["answers": answers]
        if let token { out["token"] = token }
        if let failure { out["error"] = failure }
        return out
    }

    static func registered(_ deviceToken: Data) {
        let hex = deviceToken.map { String(format: "%02x", $0) }.joined()
        onMain {
            listen()
            token = hex
            failure = nil
            answer()
        }
    }

    static func failed(_ message: String) {
        onMain {
            listen()
            token = nil
            failure = message
            answer()
        }
    }

    private static func answer() {
        answers += 1
        NotificationCenter.default.post(name: Notification.Name("exact.push.changed"), object: nil, userInfo: read)
    }

    /// A test's fresh process.
    static func reset() { token = nil; failure = nil; answers = 0 }
}

extension ExactApp {
    /// The app delegate's `application(_:didRegisterForRemoteNotificationsWithDeviceToken:)`,
    /// forwarded: an embedder whose own delegate receives it calls this. Any thread.
    public func registeredForRemoteNotifications(deviceToken: Data) {
        PushRegistration.registered(deviceToken)
    }

    /// The app delegate's `application(_:didFailToRegisterForRemoteNotificationsWithError:)`,
    /// forwarded as above. Any thread.
    public func failedToRegisterForRemoteNotifications(error: Error) {
        PushRegistration.failed(error.localizedDescription)
    }
}
