import Foundation
import UserNotifications

final class ControlledDownload: NotificationServiceDownload {
    private let completion: (URL?, Error?) -> Void
    private(set) var cancelled = false

    init(_ completion: @escaping (URL?, Error?) -> Void) {
        self.completion = completion
    }

    func resume() {}
    func cancel() { cancelled = true }
    func release(_ url: URL?, error: Error? = nil) { completion(url, error) }
}

final class Downloads {
    var tasks: [ControlledDownload] = []
}

@main struct Reference {
    static func main() throws {
        var failures = 0
        func check(_ name: String, _ body: () -> Bool) {
            let passed = body()
            if !passed { failures += 1 }
            print("\(passed ? "PASS" : "FAIL") notification service \(name)")
        }

        let suite = "exact.notification-service.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let image = directory.appendingPathComponent("pixel.png")
        try Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=")!.write(to: image)

        func makeService(_ downloads: Downloads) -> NotificationService {
            NotificationService(defaults: { defaults }) { _, completion in
                let task = ControlledDownload(completion)
                downloads.tasks.append(task)
                return task
            }
        }
        func request(key: String? = "key", media: Bool = true) -> UNNotificationRequest {
            if let key { defaults.set(key, forKey: "notification-key") }
            else { defaults.removeObject(forKey: "notification-key") }
            let clear = Array("Decrypted".utf8), secret = Array((key ?? "x").utf8)
            let content = UNMutableNotificationContent()
            content.title = "Original"
            content.userInfo = [
                "encrypted-title": Data(clear.enumerated().map { $0.element ^ secret[$0.offset % secret.count] }).base64EncodedString(),
                "media-url": media ? "https://example.com/pixel.png" : nil,
            ].compactMapValues { $0 }
            return UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
        }

        check("success changes the title, attaches media, and completes once") {
            let downloads = Downloads(), service = makeService(downloads)
            var answers: [UNNotificationContent] = []
            service.didReceive(request()) { answers.append($0) }
            guard downloads.tasks.count == 1 else { return false }
            downloads.tasks[0].release(image)
            downloads.tasks[0].release(image)
            return answers.count == 1 && answers[0].title == "[Exact] Decrypted" && answers[0].attachments.count == 1
        }
        check("attachment failure returns the changed alert once") {
            let downloads = Downloads(), service = makeService(downloads)
            var answers: [UNNotificationContent] = []
            service.didReceive(request()) { answers.append($0) }
            downloads.tasks[0].release(directory.appendingPathComponent("missing.png"))
            return answers.count == 1 && answers[0].title == "[Exact] Decrypted" && answers[0].attachments.isEmpty
        }
        check("an unavailable key preserves the visible title") {
            let service = makeService(Downloads())
            var answers: [UNNotificationContent] = []
            service.didReceive(request(key: nil, media: false)) { answers.append($0) }
            return answers.count == 1 && answers[0].title == "[Exact] Original"
        }
        check("expiry cancels, returns the best attempt once, and ignores a late callback") {
            let downloads = Downloads(), service = makeService(downloads)
            var answers: [UNNotificationContent] = []
            service.didReceive(request()) { answers.append($0) }
            service.serviceExtensionTimeWillExpire()
            guard answers.count == 1, downloads.tasks[0].cancelled,
                  answers[0].title == "[Exact] Decrypted", answers[0].attachments.isEmpty else { return false }
            downloads.tasks[0].release(image)
            return answers.count == 1 && answers[0].attachments.isEmpty
        }
        exit(failures == 0 ? 0 : 1)
    }
}
