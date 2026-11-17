// The retained WCSession dictionary contains plaintext state and only sealed secrets.
import CryptoKit
import Foundation
import CoreFoundation

struct WatchEnvelope: Codable {
    static let version = 1
    /// The largest application context this transport submits, in bytes, measured as the
    /// binary property list of the dictionary handed to WatchConnectivity.
    static let limit = 60_000
    let format: Int
    let recipient: String
    let sender: String
    let revision: UInt64
    let state: [String: String?]
    let names: [String]
    let encapsulated: Data
    let ciphertext: Data

    private struct Header: Codable {
        let format: Int
        let recipient: String
        let sender: String
        let revision: UInt64
        let state: [String: String?]
        let names: [String]
    }
    static func wireData<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(value)
    }
    private var header: Data {
        get throws { try Self.wireData(Header(format: format, recipient: recipient, sender: sender, revision: revision, state: state, names: names)) }
    }
    static func seal(snapshot: [String: Any], recipient: String, publicKey: Data) throws -> Self {
        guard let sender = snapshot["sender"] as? String,
              let number = snapshot["revision"] as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID(),
              let revision = UInt64(number.stringValue), revision > 0,
              let values = snapshot["values"] as? [String: Any] else { throw Failure.invalid }
        var state: [String: String?] = [:], secrets: [String: String?] = [:]
        for (name, value) in values {
            guard value is String || value is NSNull else { throw Failure.invalid }
            if name.hasPrefix("exact.state.") { state[name] = .some(value as? String) }
            else { secrets[name] = .some(value as? String) }
        }
        let names = secrets.keys.sorted()
        let bare = Self(format: version, recipient: recipient, sender: sender, revision: revision, state: state, names: names, encapsulated: Data(), ciphertext: Data())
        // One fresh HPKE context per complete snapshot: delivery may skip revisions.
        let key = try Curve25519.KeyAgreement.PublicKey(rawRepresentation: publicKey)
        var hpke = try HPKE.Sender(recipientKey: key, ciphersuite: .Curve25519_SHA256_ChachaPoly, info: Data("Exact watch sync v1".utf8))
        let sealed = try hpke.seal(wireData(secrets), authenticating: bare.header)
        return Self(format: version, recipient: recipient, sender: sender, revision: revision, state: state, names: names, encapsulated: hpke.encapsulatedKey, ciphertext: sealed)
    }
    func open(recipient: String, key: Curve25519.KeyAgreement.PrivateKey) throws -> [String: Any] {
        guard format == Self.version, self.recipient == recipient, revision > 0,
              names == names.sorted(), Set(names).count == names.count,
              state.keys.allSatisfy({ $0.hasPrefix("exact.state.") }) else { throw Failure.invalid }
        var hpke = try HPKE.Recipient(privateKey: key, ciphersuite: .Curve25519_SHA256_ChachaPoly, info: Data("Exact watch sync v1".utf8), encapsulatedKey: encapsulated)
        let clear = try hpke.open(ciphertext, authenticating: header)
        let secrets = try JSONDecoder().decode([String: String?].self, from: clear)
        guard secrets.keys.sorted() == names, names.allSatisfy({ !$0.hasPrefix("exact.") }) else { throw Failure.invalid }
        var values: [String: Any] = [:]
        for (name, value) in state.merging(secrets, uniquingKeysWith: { a, _ in a }) { values[name] = value as Any? ?? NSNull() }
        return ["sender": sender, "revision": revision, "values": values]
    }
    enum Failure: Error {
        case invalid, keychain
        /// The store refuses the request for good.
        case refused
        /// The sealed snapshot exceeds `limit`, the most WatchConnectivity is handed.
        case oversized
        /// The store could not finish now.
        case unavailable
    }
}
