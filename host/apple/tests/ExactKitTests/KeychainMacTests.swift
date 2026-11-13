#if os(macOS)
import Security
import XCTest

@_silgen_name("ibex2_darwin_keychain_get")
private func keychainGet(_ service: UnsafePointer<CChar>, _ name: UnsafePointer<CChar>,
                         _ value: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>,
                         _ length: UnsafeMutablePointer<Int>, _ error: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>,
                         _ keychain: SecKeychain) -> Int32

@_silgen_name("ibex2_darwin_keychain_set")
private func keychainSet(_ service: UnsafePointer<CChar>, _ name: UnsafePointer<CChar>,
                         _ value: UnsafePointer<UInt8>?, _ length: Int,
                         _ error: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>, _ keychain: SecKeychain) -> Int32

@_silgen_name("ibex2_darwin_keychain_forget")
private func keychainForget(_ service: UnsafePointer<CChar>, _ name: UnsafePointer<CChar>,
                            _ error: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>, _ keychain: SecKeychain) -> Int32

final class KeychainMacTests: XCTestCase {
    private func check(_ status: OSStatus, file: StaticString = #filePath, line: UInt = #line) throws {
        XCTAssertEqual(status, errSecSuccess, "\(SecCopyErrorMessageString(status, nil) as String? ?? "OSStatus \(status)")",
                       file: file, line: line)
        if status != errSecSuccess { throw NSError(domain: NSOSStatusErrorDomain, code: Int(status)) }
    }

    func testTheShimUsesOnlyItsTemporaryKeychain() throws {
        var originalDefault: SecKeychain?
        let defaultStatus = SecKeychainCopyDefault(&originalDefault)
        var search: CFArray?
        try check(SecKeychainCopySearchList(&search))
        let originalSearch = try XCTUnwrap(search as? [SecKeychain])
        var interactive = DarwinBoolean(false)
        try check(SecKeychainGetUserInteractionAllowed(&interactive))
        try check(SecKeychainSetUserInteractionAllowed(false))
        defer { XCTAssertEqual(SecKeychainSetUserInteractionAllowed(interactive.boolValue), errSecSuccess) }
        func assertUserKeychainsUnchanged() {
            var currentDefault: SecKeychain?
            XCTAssertEqual(SecKeychainCopyDefault(&currentDefault), defaultStatus)
            XCTAssertEqual(currentDefault, originalDefault)
            var currentSearch: CFArray?
            XCTAssertEqual(SecKeychainCopySearchList(&currentSearch), errSecSuccess)
            XCTAssertEqual(currentSearch as? [SecKeychain], originalSearch)
        }
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("exact-keychain-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        var keychains: [SecKeychain] = []
        defer {
            for keychain in keychains { XCTAssertEqual(SecKeychainDelete(keychain), errSecSuccess) }
            XCTAssertNoThrow(try FileManager.default.removeItem(at: directory))
            assertUserKeychainsUnchanged()
        }
        let password = UUID().uuidString
        for name in ["primary", "other"] {
            var created: SecKeychain?
            let path = directory.appendingPathComponent("\(name).keychain").path
            let status = password.withCString { SecKeychainCreate(path, UInt32(password.utf8.count), $0, false, nil, &created) }
            if let created { keychains.append(created) }
            try check(status)
            let keychain = try XCTUnwrap(created)
            try check(password.withCString { SecKeychainUnlock(keychain, UInt32(password.utf8.count), $0, true) })
        }
        assertUserKeychainsUnchanged()
        let primary = keychains[0], other = keychains[1]
        let service = "dev.exact.keychain-test.\(UUID().uuidString)", name = "session"
        func query(_ keychain: SecKeychain) -> [String: Any] {
            [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
             kSecAttrAccount as String: name, kSecMatchSearchList as String: [keychain]]
        }
        func add(_ keychain: SecKeychain, _ value: Data) throws {
            var item = query(keychain)
            item.removeValue(forKey: kSecMatchSearchList as String)
            item[kSecUseKeychain as String] = keychain
            item[kSecValueData as String] = value
            try check(SecItemAdd(item as CFDictionary, nil))
        }
        func independentRead(_ keychain: SecKeychain) throws -> Data? {
            var read = query(keychain)
            read[kSecReturnData as String] = true
            read[kSecMatchLimit as String] = kSecMatchLimitOne
            var result: CFTypeRef?
            let status = SecItemCopyMatching(read as CFDictionary, &result)
            if status == errSecItemNotFound { return nil }
            try check(status)
            return try XCTUnwrap(result as? Data)
        }
        func get() throws -> Data? {
            var value: UnsafeMutablePointer<UInt8>?, error: UnsafeMutablePointer<CChar>?
            var length = 0
            let status = keychainGet(service, name, &value, &length, &error, primary)
            defer { free(value); free(error) }
            XCTAssertNotEqual(status, -1, error.map { String(cString: $0) } ?? "get failed")
            if status == -1 { throw NSError(domain: "KeychainShim", code: Int(status)) }
            if status == 1 {
                XCTAssertNil(value)
                XCTAssertEqual(length, 0)
                XCTAssertNil(error)
                return nil
            }
            XCTAssertEqual(status, 0)
            XCTAssertNil(error)
            return Data(bytes: try XCTUnwrap(value), count: length)
        }
        func set(_ data: Data) throws {
            var error: UnsafeMutablePointer<CChar>?
            let status = data.withUnsafeBytes {
                keychainSet(service, name, $0.bindMemory(to: UInt8.self).baseAddress, data.count, &error, primary)
            }
            defer { free(error) }
            XCTAssertEqual(status, 0, error.map { String(cString: $0) } ?? "set failed")
            if status != 0 { throw NSError(domain: "KeychainShim", code: Int(status)) }
            XCTAssertNil(error)
        }
        func forget() throws {
            var error: UnsafeMutablePointer<CChar>?
            let status = keychainForget(service, name, &error, primary)
            defer { free(error) }
            XCTAssertEqual(status, 0, error.map { String(cString: $0) } ?? "forget failed")
            if status != 0 { throw NSError(domain: "KeychainShim", code: Int(status)) }
            XCTAssertNil(error)
        }

        let seed = Data("independently stored".utf8), sentinel = Data("other keychain".utf8)
        try add(primary, seed)
        try add(other, sentinel)
        // Stop before any shim write if it cannot find the explicitly scoped seed.
        let stored = try XCTUnwrap(get())
        XCTAssertEqual(stored, seed)
        try forget()
        XCTAssertNil(try independentRead(primary))
        XCTAssertNil(try get())

        let first = Data("one".utf8), replacement = Data("\u{0}two 😀".utf8)
        for value in [first, replacement] {
            try set(value)
            XCTAssertEqual(try get(), value)
            XCTAssertEqual(try independentRead(primary), value)
            XCTAssertEqual(try independentRead(other), sentinel)
        }
        try forget()
        try forget()
        XCTAssertNil(try get())
        XCTAssertNil(try independentRead(primary))
        XCTAssertEqual(try independentRead(other), sentinel)
        assertUserKeychainsUnchanged()
    }
}
#endif
