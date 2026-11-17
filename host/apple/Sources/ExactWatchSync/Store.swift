import CExact
import CryptoKit
import ExactCore
import Foundation

/// No plaintext response enters Runtime's output buffer, Batch or the agent.
final class WatchStore {
    let app: String
    let policy: [String: [String]]
    let plan: Data
    let grants: String
    init(bundle: Bundle, watch: Bool) throws {
        guard let config = bundle.object(forInfoDictionaryKey: "ExactWatchSync") as? [String: Any],
              let app = config["app"] as? String, let grants = config["grants"] as? String,
              let policy = config["policy"] as? [String: [String]] else { throw WatchEnvelope.Failure.invalid }
        self.app = app; self.grants = grants; self.policy = policy
        plan = try Data(contentsOf: bundle.bundleURL.appendingPathComponent(watch ? "plans/watch.plan" : "watch-sync.plan"))
    }
    init(app: String, policy: [String: [String]], plan: Data, grants: String) {
        self.app = app; self.policy = policy; self.plan = plan; self.grants = grants
    }
    // Plaintext is confined to this synchronous CryptoKit callback. WatchStore's
    // request and WCSession receive only the envelope returned here.
    private static let seal: @convention(c) (UnsafePointer<UInt8>?, Int, (@convention(c) (UnsafePointer<UInt8>?, Int, Int) -> Void)?, Int) -> Void = { bytes, count, reply, context in
        guard let bytes, let reply else { return }
        do {
            let input = try JSONSerialization.jsonObject(with: Data(bytes: bytes, count: count)) as? [String: Any]
            guard let snapshot = input?["snapshot"] as? [String: Any],
                  let recipient = input?["recipient"] as? String,
                  let encoded = input?["publicKey"] as? String, let key = Data(base64Encoded: encoded) else { return }
            let envelope = try WatchEnvelope.seal(snapshot: snapshot, recipient: recipient, publicKey: key)
            let output = try WatchEnvelope.wireData(envelope)
            output.withUnsafeBytes { reply($0.bindMemory(to: UInt8.self).baseAddress, $0.count, context) }
        } catch { /* The host returns a value-free refusal. */ }
    }
    /// Import a retained snapshot. One this watch can never accept (another installation's
    /// key, an unknown format, a value its plan refuses) is settled, not retried.
    func receive(_ data: Data, recipient: String, key: Curve25519.KeyAgreement.PrivateKey) -> WatchReceive.Outcome {
        guard data.count <= WatchEnvelope.limit,
              let envelope = try? JSONDecoder().decode(WatchEnvelope.self, from: data),
              let snapshot = try? envelope.open(recipient: recipient, key: key) else { return .settled }
        do { return try request("apply", fields: ["snapshot": snapshot])["applied"] as? Bool == true ? .applied : .settled }
        catch WatchEnvelope.Failure.refused { return .settled }
        catch { return .failed }
    }
    /// Whether the store holds no unfinished import.
    func ready() -> Bool { (try? request("ready")) != nil }

    /// Send one snapshot through `hand`, WatchConnectivity's `updateApplicationContext`.
    /// The revision is spent only once the dictionary `hand` receives is known to fit,
    /// and is given back when `hand` throws: no watch has seen it.
    func submit(sender: String, recipient: String, publicKey: Data, hand: ([String: Any]) throws -> Void) -> WatchSend.Submitted {
        do {
            let envelope = try request("snapshot", fields: ["sender": sender, "recipient": recipient, "publicKey": publicKey.base64EncodedString()])
            guard let revision = envelope["revision"] as? NSNumber, let sender = envelope["sender"] as? String else { return .failed }
            let data = try JSONSerialization.data(withJSONObject: envelope, options: [.sortedKeys, .withoutEscapingSlashes])
            let context: [String: Any] = ["snapshot": data]
            guard try PropertyListSerialization.data(fromPropertyList: context, format: .binary, options: 0).count <= WatchEnvelope.limit else { return .refused(.oversized) }
            _ = try request("reserve", fields: ["sender": sender, "revision": revision])
            do { try hand(context) } catch {
                _ = try? request("release", fields: ["revision": revision])
                return .failed
            }
            return .accepted
        } catch WatchEnvelope.Failure.oversized { return .refused(.oversized) }
        catch { return .failed }
    }
    private final class Reply { var bytes = Data() }
    func request(_ operation: String, fields: [String: Any] = [:]) throws -> [String: Any] {
        var request = fields
        request["op"] = operation; request["app"] = app; request["grants"] = grants; request["policy"] = policy
        let input = try JSONSerialization.data(withJSONObject: request)
        let reply = Reply()
        Owner.shared.sync {
            plan.withUnsafeBytes { p in input.withUnsafeBytes { i in
                exact_watch_store(p.bindMemory(to: UInt8.self).baseAddress, p.count,
                                  i.bindMemory(to: UInt8.self).baseAddress, i.count, Self.seal, { bytes, count, context in
                    let reply = Unmanaged<Reply>.fromOpaque(UnsafeRawPointer(bitPattern: context)!).takeUnretainedValue()
                    reply.bytes = Data(bytes: bytes!, count: count)
                }, Int(bitPattern: Unmanaged.passUnretained(reply).toOpaque()))
            } }
        }
        guard let value = try JSONSerialization.jsonObject(with: reply.bytes) as? [String: Any] else { throw WatchEnvelope.Failure.unavailable }
        guard value["error"] != nil else { return value }
        if value["refused"] as? Bool == true { throw WatchEnvelope.Failure.refused }
        throw value["oversized"] as? Bool == true ? WatchEnvelope.Failure.oversized : WatchEnvelope.Failure.unavailable
    }
}
