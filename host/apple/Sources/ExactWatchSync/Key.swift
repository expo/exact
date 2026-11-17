import CryptoKit
import Foundation
import Security

/// The install marker is not a key. Losing the app container creates a new key,
/// even on systems where Keychain items survive an uninstall.
struct WatchKey {
    let installation: String
    let key: Curve25519.KeyAgreement.PrivateKey
    static func load(defaults: UserDefaults, service: String) throws -> Self {
        let marker = "exact.watch.installation"
        let previous = defaults.string(forKey: marker)
        let installation = previous ?? UUID().uuidString
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                   kSecAttrService as String: service + ".watch-sync",
                                   kSecAttrAccount as String: "recipient",
                                   kSecAttrSynchronizable as String: false]
        var result: CFTypeRef?
        if previous != nil {
            let status = SecItemCopyMatching(query.merging([kSecReturnData as String: true], uniquingKeysWith: { _, b in b }) as CFDictionary, &result)
            if status == errSecSuccess, let bytes = result as? Data {
                return Self(installation: installation, key: try Curve25519.KeyAgreement.PrivateKey(rawRepresentation: bytes))
            }
            guard status == errSecItemNotFound else { throw WatchEnvelope.Failure.keychain }
        }
        let key = Curve25519.KeyAgreement.PrivateKey()
        let deleted = SecItemDelete(query as CFDictionary)
        guard deleted == errSecSuccess || deleted == errSecItemNotFound else { throw WatchEnvelope.Failure.keychain }
        let attributes = query.merging([kSecValueData as String: key.rawRepresentation,
                                      kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly], uniquingKeysWith: { _, b in b })
        guard SecItemAdd(attributes as CFDictionary, nil) == errSecSuccess else { throw WatchEnvelope.Failure.keychain }
        // A missing key also starts a new ordering domain.
        let current = previous == nil ? installation : UUID().uuidString
        defaults.set(current, forKey: marker)
        return Self(installation: current, key: key)
    }
}
