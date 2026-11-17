import Foundation
import UserNotifications

protocol NotificationServiceDownload: AnyObject {
    func resume()
    func cancel()
}

extension URLSessionDownloadTask: NotificationServiceDownload {}

final class FinishOnce<Value> {
    private let lock = NSLock()
    private var handler: ((Value) -> Void)?

    init(_ handler: @escaping (Value) -> Void) {
        self.handler = handler
    }

    func call(_ value: Value) {
        lock.lock()
        let next = handler
        handler = nil
        lock.unlock()
        next?(value)
    }
}

final class NotificationService: UNNotificationServiceExtension {
    typealias Download = (URL, @escaping (URL?, Error?) -> Void) -> NotificationServiceDownload

    private let lock = NSLock()
    private let defaults: () -> UserDefaults?
    private let download: Download
    private var task: NotificationServiceDownload?
    private var finish: FinishOnce<UNNotificationContent>?
    private var bestAttempt: UNMutableNotificationContent?

    override init() {
        defaults = { UserDefaults(suiteName: "group.__APP_ID__") }
        download = { url, completion in
            URLSession.shared.downloadTask(with: url) { temporary, _, error in completion(temporary, error) }
        }
        super.init()
    }

    init(defaults: @escaping () -> UserDefaults?, download: @escaping Download) {
        self.defaults = defaults
        self.download = download
        super.init()
    }

    override func didReceive(
        _ request: UNNotificationRequest,
        withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        let content = request.content.mutableCopy() as! UNMutableNotificationContent
        let shared = defaults()
        content.title = "[Exact] " + (decryptedTitle(request.content.userInfo, defaults: shared) ?? content.title)
        shared?.set(Date().timeIntervalSince1970, forKey: "notification-service-marker")
        lock.lock()
        bestAttempt = content
        finish = FinishOnce(contentHandler)
        task = nil
        lock.unlock()

        guard let text = request.content.userInfo["media-url"] as? String, let url = URL(string: text) else {
            complete()
            return
        }
        let created = download(url) { [weak self] temporary, error in
            self?.downloadFinished(temporary, error: error, extension: url.pathExtension)
        }
        lock.lock()
        let active = finish != nil
        if active { task = created }
        lock.unlock()
        if active { created.resume() } else { created.cancel() }
    }

    override func serviceExtensionTimeWillExpire() {
        lock.lock()
        let task = self.task, content = bestAttempt, finish = self.finish
        self.task = nil
        bestAttempt = nil
        self.finish = nil
        lock.unlock()
        task?.cancel()
        if let content { finish?.call(content) }
    }

    private func downloadFinished(_ temporary: URL?, error: Error?, extension pathExtension: String) {
        lock.lock()
        let active = finish != nil
        lock.unlock()
        guard active else { return }
        var attachment: UNNotificationAttachment?
        if error == nil, let temporary {
            let local = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
                .appendingPathExtension(pathExtension.isEmpty ? "bin" : pathExtension)
            do {
                try FileManager.default.moveItem(at: temporary, to: local)
                attachment = try UNNotificationAttachment(identifier: "media", url: local)
            } catch { /* The changed alert is the documented fallback. */ }
        }
        complete(attachment)
    }

    private func complete(_ attachment: UNNotificationAttachment? = nil) {
        lock.lock()
        if let attachment { bestAttempt?.attachments = [attachment] }
        let content = bestAttempt, finish = self.finish
        task = nil
        bestAttempt = nil
        self.finish = nil
        lock.unlock()
        if let content { finish?.call(content) }
    }
}

func decryptedTitle(_ payload: [AnyHashable: Any], defaults: UserDefaults?) -> String? {
    guard let encoded = payload["encrypted-title"] as? String,
          let bytes = Data(base64Encoded: encoded),
          let key = defaults?.string(forKey: "notification-key"), !key.isEmpty else { return nil }
    let secret = Array(key.utf8)
    return String(bytes: bytes.enumerated().map { $0.element ^ secret[$0.offset % secret.count] }, encoding: .utf8)
}
