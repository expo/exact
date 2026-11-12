// Appended to Notifications.swift by authorization.test.mjs. The system's callbacks
// are held, then released on a background queue and joined before inspecting calls.
import ObjectiveC

private final class Settings: UNNotificationSettings {
    static var authorization = UNAuthorizationStatus.notDetermined
    override var authorizationStatus: UNAuthorizationStatus { Self.authorization }
    override var alertSetting: UNNotificationSetting { .enabled }
    override var soundSetting: UNNotificationSetting { .enabled }
    override var badgeSetting: UNNotificationSetting { .enabled }
}

private final class Center: UNUserNotificationCenter {
    static var reads: [(UNNotificationSettings) -> Void] = []
    static var asks: [(UNAuthorizationOptions, (Bool, Error?) -> Void)] = []
    static var posts: [(UNNotificationRequest, ((Error?) -> Void)?)] = []

    override func getNotificationSettings(completionHandler: @escaping (UNNotificationSettings) -> Void) {
        Self.reads.append(completionHandler)
    }

    override func requestAuthorization(options: UNAuthorizationOptions, completionHandler: @escaping (Bool, Error?) -> Void) {
        Self.asks.append((options, completionHandler))
    }

    override func add(_ request: UNNotificationRequest, withCompletionHandler completionHandler: ((Error?) -> Void)? = nil) {
        Self.posts.append((request, completionHandler))
    }
}

private extension NotificationsModule {
    static func testAuthorization() -> Int {
        let center = class_createInstance(Center.self, 0) as! Center
        let current = class_getClassMethod(UNUserNotificationCenter.self, NSSelectorFromString("currentNotificationCenter"))!
        let replacement = imp_implementationWithBlock({ (_: AnyObject) in center } as @convention(block) (AnyObject) -> UNUserNotificationCenter)
        let original = method_setImplementation(current, replacement)
        defer {
            method_setImplementation(current, original)
            imp_removeBlock(replacement)
        }
        var failures = 0
        func check(_ ok: Bool, _ message: String) {
            if !ok { failures += 1; print("FAIL: \(message)") }
        }
        func background(_ body: @escaping () -> Void) {
            let done = DispatchSemaphore(value: 0)
            DispatchQueue(label: "notification-answer").async { body(); done.signal() }
            check(done.wait(timeout: .now() + 5) == .success, "background callback completed")
            var drained = false
            DispatchQueue.main.async { drained = true }
            let deadline = Date().addingTimeInterval(5)
            while !drained, Date() < deadline { _ = RunLoop.main.run(mode: .default, before: deadline) }
            check(drained, "main queue completed")
        }
        func read(_ authorization: UNAuthorizationStatus) {
            check(Center.reads.count == 1, "one pending settings read, got \(Center.reads.count)")
            guard !Center.reads.isEmpty else { return }
            Settings.authorization = authorization
            let settings = class_createInstance(Settings.self, 0) as! Settings
            let answer = Center.reads.removeFirst()
            background { answer(settings) }
        }
        func run(_ name: String, _ authorization: UNAuthorizationStatus, previous: String,
                 grant: Bool = true, permissionError: Error? = nil, postError: Error? = nil, scheduled: Bool = false) {
            let before = failures
            Center.reads = []; Center.asks = []; Center.posts = []
            var changes: [String] = [], logs: [String] = [], replies: [[String: Any]] = []
            let context = ExactPackageContext(agent: false, session: name, changed: {
                check(Thread.isMainThread, "\(name): status change on main thread")
                changes.append($0)
            }, log: {
                check(Thread.isMainThread, "\(name): log on main thread")
                logs.append($0)
            }, now: { 0 }, activation: { event, args, navigate in
                ["event": event, "args": args, "navigate": navigate ?? ""]
            })
            let module = NotificationsModule(context: context)
            module.said = previous
            var args: [String: Any] = ["title": "Finished", "body": "Ready", "tag": "thread", "silent": true, "data": "payload", "navigate": "/thread"]
            if scheduled { args["showTrigger"] = (Date().timeIntervalSince1970 + 120) * 1000 }
            module.show(args) {
                check(Thread.isMainThread, "\(name): reply on main thread")
                replies.append($0)
            }
            check(Center.asks.isEmpty && Center.posts.isEmpty && replies.isEmpty, "\(name): read authorization before asking or posting")
            guard Center.reads.count == 1 else {
                check(false, "\(name): posting must read authorization first")
                return
            }
            read(authorization)
            let asks = authorization == .notDetermined
            check(Center.asks.count == (asks ? 1 : 0), "\(name): only notDetermined asks")
            if asks {
                check(Center.posts.isEmpty && replies.isEmpty && changes.isEmpty, "\(name): wait for the permission answer")
                guard let (options, answer) = Center.asks.first else { return }
                check(options == [.alert, .sound, .badge], "\(name): request the posting options")
                background { answer(grant, permissionError) }
            }
            let allowed = asks ? grant : authorization != .denied
            check(Center.posts.count == (allowed ? 1 : 0), "\(name): post only while granted")
            check(changes.isEmpty, "\(name): wait for fresh settings before announcing")
            let finalAuthorization = asks ? (permissionError != nil ? .notDetermined : grant ? .authorized : .denied) : authorization
            read(finalAuthorization)
            let state = finalAuthorization == .notDetermined ? "prompt" : finalAuthorization == .denied ? "denied" : "granted"
            check(changes == (previous == state ? [] : ["notifications.notificationStatus"]), "\(name): announce a changed permission once")
            if allowed, let (request, answer) = Center.posts.first {
                check(replies.isEmpty, "\(name): wait for the posting answer")
                check(request.identifier == "thread" && request.content.title == "Finished" && request.content.body == "Ready"
                      && request.content.sound == nil, "\(name): preserve the post's content")
                check(request.content.userInfo["event"] as? String == "notificationclick"
                      && request.content.userInfo["navigate"] as? String == "/thread", "\(name): preserve activation")
                check((request.trigger != nil) == scheduled, "\(name): preserve the schedule")
                check(answer != nil, "\(name): the post has a completion handler")
                background { answer?(postError) }
                let expectedLog = scheduled ? "showNotification: scheduled" : "showNotification: shown"
                check(logs == (postError == nil ? [expectedLog] : []), "\(name): log only an accepted post")
            } else {
                check(logs.isEmpty, "\(name): no log for a denied post")
            }
            check(replies.count == 1, "\(name): reply once")
            let expectedError: String? = allowed ? (postError == nil ? nil : "UnknownError") : "NotAllowedError"
            check(replies.first?["error"] as? String == expectedError, "\(name): propagate the result")
            if let postError {
                check(replies.first?["message"] as? String == postError.localizedDescription, "\(name): preserve the posting error")
            } else if allowed {
                check(replies.first?.isEmpty == true, "\(name): successful post answers nothing")
            }
            check(Center.reads.isEmpty, "\(name): every settings read answered")
            print("\(failures == before ? "PASS" : "FAIL") \(name)")
        }
        run("authorized", .authorized, previous: "granted")
        run("provisional", .provisional, previous: "granted")
        // The macOS SDK does not name iOS's ephemeral authorization (4).
        run("ephemeral", UNAuthorizationStatus(rawValue: 4)!, previous: "granted")
        run("denied", .denied, previous: "granted")
        run("undetermined then granted", .notDetermined, previous: "prompt", scheduled: true)
        run("undetermined then denied", .notDetermined, previous: "prompt", grant: false)
        let error = NSError(domain: "notifications-test", code: 1, userInfo: [NSLocalizedDescriptionKey: "posting failed"])
        run("permission request failed", .notDetermined, previous: "prompt", grant: false, permissionError: error)
        run("posting failed under provisional", .provisional, previous: "denied", postError: error)
        return failures
    }
}

@main private struct AuthorizationTests {
    static func main() {
        let failures = NotificationsModule.testAuthorization()
        print("notification authorization: \(failures) failures")
        exit(failures == 0 ? 0 : 1)
    }
}
