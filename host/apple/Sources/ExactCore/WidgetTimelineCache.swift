// @ref LLP 1126.000 WP5, LLP 1126 D8 — account data expires on a store change.
import CommonCrypto
import Foundation

/// A refresh captures the app's store counter before reading any account data.
/// The same identity scopes in-flight sharing and the extension's persisted answer:
/// a generation, a kind, a placement's configuration and the store revision.
public struct WidgetTimelineCache: Hashable {
    /// How many configurations' answers a kind keeps in a generation. A person can edit a
    /// widget any number of times, so a write past this removes the answers written longest ago.
    package static let kept = 8
    private let configured: Bool
    private let file: URL
    private let counterFile: URL
    /// Nil when the counter file is unreadable: the refresh still answers from
    /// the live store, but nothing is kept or reused under an unknown revision.
    private let counter: UInt64?
    private enum Failure: Error { case storeChanged }
    private struct Saved: Codable {
        let counter: UInt64
        let bytes: Data
    }

    /// `configuration` is a configured placement's chosen values, the bytes its refresh receives.
    public init(root: URL, kind: String, generation: String, configuration: Data? = nil) throws {
        configured = configuration != nil
        let name: String
        if let configuration {
            var hash = [UInt8](repeating: 0, count: Int(CC_SHA256_DIGEST_LENGTH))
            _ = configuration.withUnsafeBytes { CC_SHA256($0.baseAddress, CC_LONG(configuration.count), &hash) }
            name = hash.map { String(format: "%02x", $0) }.joined()
        } else {
            name = "default"
        }
        file = root.appendingPathComponent("Library/Caches/widget-timelines/\(generation)/\(kind)/\(name).json")
        counterFile = root.appendingPathComponent("widget-store-generation")
        counter = try? Self.readCounter(counterFile)
    }

    private static func readCounter(_ file: URL) throws -> UInt64 {
        let text: String
        do { text = try String(contentsOf: file, encoding: .utf8) }
        catch CocoaError.fileReadNoSuchFile { return 0 }
        guard let counter = UInt64(text) else { throw CocoaError(.fileReadCorruptFile) }
        return counter
    }

    /// Recheck after refresh and before delivery: a write can race either path.
    /// An unreadable counter cannot authorize account content: it stays current
    /// only while it stays unreadable, and the app's next write makes it a number.
    public var isCurrent: Bool { (try? Self.readCounter(counterFile)) == counter }

    /// Only validated timeline bytes belong here. A failure may reuse an answer
    /// from this store revision, never one written before an account switch.
    public func resolve(_ refresh: () throws -> Data) throws -> Data {
        guard let counter else {
            guard isCurrent else { throw Failure.storeChanged }
            let bytes = try refresh()
            guard isCurrent else { throw Failure.storeChanged }
            return bytes
        }
        let bytes: Data
        do {
            guard isCurrent else { throw Failure.storeChanged }
            bytes = try refresh()
            guard isCurrent else { throw Failure.storeChanged }
        } catch {
            // This cache is separate from the app's store and update pointer.
            // A fresh process must prove the saved counter matches as well.
            let saved = try? JSONDecoder().decode(Saved.self, from: Data(contentsOf: file))
            if let saved, saved.counter < counter { try? FileManager.default.removeItem(at: file) }
            guard isCurrent, let saved, saved.counter == counter, isCurrent else { throw error }
            return saved.bytes
        }
        // Keeping the answer is best effort: a full or purged cache must not
        // turn a good refresh into a failure.
        if let saved = try? JSONEncoder().encode(Saved(counter: counter, bytes: bytes)) {
            try? FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try? saved.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
            if configured { removeOldest() }
        }
        guard isCurrent else { throw Failure.storeChanged }
        return bytes
    }

    /// Keep this answer and the `kept - 1` other configurations' written most recently.
    private func removeOldest() {
        let date = URLResourceKey.contentModificationDateKey
        guard let files = try? FileManager.default.contentsOfDirectory(at: file.deletingLastPathComponent(), includingPropertiesForKeys: [date]),
              files.count > Self.kept else { return }
        let others = files.filter { $0.lastPathComponent != file.lastPathComponent }
            .map { ($0, (try? $0.resourceValues(forKeys: [date]))?.contentModificationDate ?? .distantPast) }
            .sorted { $0.1 < $1.1 }
        for (old, _) in others.prefix(files.count - Self.kept) { try? FileManager.default.removeItem(at: old) }
    }
}
