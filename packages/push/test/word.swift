// The branch `subscribePush()` takes for each notification authorization
// (`PushModule.step`, which it switches on), and the state
// `pushPermissionState()` reads for it (`PushModule.word`), compiled with
// Push.swift by drive.mjs and run on this Mac: the simulator cannot be put
// into provisional authorization. Only `notDetermined` asks; every
// authorization read as granted registers without asking, so an embedder's
// provisional one never prompts. Then the installation ID the Apple
// subscription carries (`PushModule.installation`), over a Keychain kept in
// memory: unread before the first unlock, created once, then the same.
import ExactPackage
import Foundation
import Security
import UserNotifications

@main struct Word {
    static func main() {
        var failures = 0
        // `.ephemeral` (4), which the macOS SDK does not name.
        let want: [(UNAuthorizationStatus, String, PushModule.Step, String)] = [
            (.notDetermined, "notDetermined", .ask, "prompt"), (.denied, "denied", .refuse, "denied"),
            (.authorized, "authorized", .register, "granted"), (.provisional, "provisional", .register, "granted"),
            (UNAuthorizationStatus(rawValue: 4)!, "ephemeral", .register, "granted"),
        ]
        for (status, name, step, word) in want {
            let (gotStep, gotWord) = (PushModule.step(status), PushModule.word(status))
            let ok = gotStep == step && gotWord == word
            if !ok { failures += 1 }
            print("\(ok ? "PASS" : "FAIL") \(name): subscribePush() takes \(step.rawValue), pushPermissionState() reads \(word)\(ok ? "" : ": got \(gotStep.rawValue), \(gotWord)")")
        }
        // The installation ID (`PushModule.installation`) over a Keychain kept in memory.
        var kept: [String: Data] = [:]
        var readable = errSecSuccess
        var adds = 0
        func key(_ query: CFDictionary) -> String {
            let q = query as NSDictionary
            return "\(q[kSecAttrService] ?? "")/\(q[kSecAttrAccount] ?? "")"
        }
        func dataProtection(_ query: CFDictionary) -> Bool { (query as NSDictionary)[kSecUseDataProtectionKeychain] as? Bool == true }
        let copy: (CFDictionary, UnsafeMutablePointer<CFTypeRef?>?) -> OSStatus = { query, found in
            guard dataProtection(query) else { return errSecMissingEntitlement }
            guard readable == errSecSuccess else { return readable }
            guard let data = kept[key(query)] else { return errSecItemNotFound }
            found?.pointee = data as CFData
            return errSecSuccess
        }
        let add: (CFDictionary) -> OSStatus = { item in
            adds += 1
            let q = item as NSDictionary
            guard dataProtection(item), (q[kSecAttrAccessible] as? String) == (kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly as String) else { return errSecParam }
            guard kept[key(item)] == nil else { return errSecDuplicateItem }
            kept[key(item)] = q[kSecValueData] as? Data
            return errSecSuccess
        }
        func expect(_ ok: Bool, _ name: String, _ detail: @autoclosure () -> String) {
            if !ok { failures += 1 }
            print("\(ok ? "PASS" : "FAIL") \(name)\(ok ? "" : ": \(detail())")")
        }
        readable = errSecInteractionNotAllowed
        let locked = PushModule.installation(copy: copy, add: add)
        expect(locked == nil && adds == 0, "before the first unlock the installation ID is unread and nothing is written", "\(String(describing: locked)), \(adds) adds")
        readable = errSecSuccess
        let first = PushModule.installation(copy: copy, add: add)
        let uuid = first.flatMap(UUID.init(uuidString:)) != nil && first == first?.lowercased()
        expect(uuid && adds == 1, "the first read keeps a new lowercase UUID on this device only", "\(String(describing: first)), \(adds) adds")
        let again = PushModule.installation(copy: copy, add: add)
        expect(again == first && adds == 1, "a later read, a relaunch's or a rotation's, answers the same ID", "\(String(describing: again)) after \(String(describing: first))")
        // Another process kept one between this read and this add: that one is answered.
        kept = [:]
        var raced = false
        let racing: (CFDictionary, UnsafeMutablePointer<CFTypeRef?>?) -> OSStatus = { query, found in
            if !raced { raced = true; kept[key(query)] = Data("0d9c8b7a-6f5e-4d3c-8b2a-1f0e9d8c7b6a".utf8); return errSecItemNotFound }
            return copy(query, found)
        }
        let other = PushModule.installation(copy: racing, add: add)
        expect(other == "0d9c8b7a-6f5e-4d3c-8b2a-1f0e9d8c7b6a", "an ID another process kept first is the one answered", String(describing: other))
        exit(failures == 0 ? 0 : 1)
    }
}
