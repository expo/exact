import XCTest
import AuthenticationServices
import ExactPackage
@testable import SignInImplementation
#if os(macOS)
import AppKit
#else
import UIKit
#endif

final class SignInTests: XCTestCase {
    private func options(fields: [String]? = nil) -> [String: Any] {
        var provider: [String: Any] = ["configURL": "https://appleid.apple.com", "clientId": "com.example.web", "params": ["nonce": "raw-nonce-123"]]
        if let fields { provider["fields"] = fields }
        return ["identity": ["mode": "active", "providers": [provider]]]
    }

    func testCapabilitiesOfferOnlyBuiltRequests() throws {
        let context = ExactPackageContext(agent: false, session: "sign-in-caps", changed: { _ in }, log: { _ in }, now: { 0 })
        let module = SignInModule(context: context)
        let sync = try module.call("getIdentityCapabilitiesSync", [:])
        var async: [String: Any] = [:]
        module.later("getIdentityCapabilities", [:], preferred: nil) { async = $0 }
        let admitted = SignInModule.capabilities(apple: true)
        let providers = try XCTUnwrap(admitted["providers"] as? [String: [String: Bool]])
        XCTAssertEqual(providers["apple"]?["active"], true)
        XCTAssertEqual(providers["apple"]?["passive"], true)
        for answer in [sync, async, admitted] {
            XCTAssertEqual(answer["available"] as? Bool, true)
            let providers = try XCTUnwrap(answer["providers"] as? [String: [String: Bool]])
            XCTAssertEqual(providers["google"], ["active": false, "passive": false, "withPublicKey": false])
            XCTAssertEqual(providers["apple"]?["withPublicKey"], false)
            XCTAssertEqual(providers["apple"]?["active"], providers["apple"]?["passive"])
        }
    }

    func testRequestUsesRawNonceAndRequestedScopes() throws {
        let request = try SignInModule.request(options())
        XCTAssertEqual(request.nonce, "raw-nonce-123")
        XCTAssertEqual(request.requestedScopes, [.fullName, .email])
        XCTAssertFalse(try XCTUnwrap(request.state).isEmpty)
        XCTAssertNotEqual(request.state, try SignInModule.request(options()).state)
        XCTAssertEqual(try SignInModule.request(options(fields: ["email", "picture", "email"])).requestedScopes, [.email])
        XCTAssertEqual(try SignInModule.request(options(fields: [])).requestedScopes, [])
        for mediation in ["silent", "conditional"] {
            var args = options(); args["mediation"] = mediation
            XCTAssertThrowsError(try SignInModule.request(args))
        }
        var args = options(); args["publicKey"] = ["rpId": "example.com"]
        XCTAssertThrowsError(try SignInModule.request(args))
        XCTAssertThrowsError(try SignInModule.request(["identity": ["providers": [["configURL": "https://example.com"]]]]))
    }

    func testAnswerHasTheDeclaredFieldsAndFirstAuthorizationData() throws {
        var name = PersonNameComponents(); name.givenName = "Test"; name.familyName = "Person"
        let answer = SignInModule.answer(token: Data("id-token".utf8), code: Data("code".utf8), state: "state",
                                         user: "apple-user", name: name, email: "test@example.com", status: .likelyReal, scopes: [.fullName, .email])
        XCTAssertEqual(answer["type"] as? String, "identity")
        XCTAssertEqual(answer["id"] as? String, "")
        XCTAssertEqual(answer["configURL"] as? String, "https://appleid.apple.com")
        XCTAssertEqual(answer["isAutoSelected"] as? Bool, false)
        XCTAssertNotNil(UUID(uuidString: try XCTUnwrap(answer["handle"] as? String)))
        let token = try XCTUnwrap(answer["token"] as? [String: Any])
        XCTAssertEqual(token["authorization"] as? [String: String], ["id_token": "id-token", "code": "code", "state": "state"])
        let user = try XCTUnwrap(token["user"] as? [String: Any])
        XCTAssertEqual(user["name"] as? [String: String], ["firstName": "Test", "lastName": "Person"])
        XCTAssertEqual(user["email"] as? String, "test@example.com")
        let apple = try XCTUnwrap(answer["apple"] as? [String: Any])
        XCTAssertEqual(apple["user"] as? String, "apple-user")
        XCTAssertEqual(apple["realUserStatus"] as? String, "likelyReal")
        XCTAssertEqual(apple["authorizedScopes"] as? [String], ["name", "email"])
        let again = SignInModule.answer(token: Data("token".utf8), code: Data("code".utf8), state: nil,
                                        user: "apple-user", name: nil, email: nil, status: .unknown, scopes: [])
        XCTAssertNil((again["token"] as? [String: Any])?["user"])
        XCTAssertEqual(SignInModule.answer(token: nil, code: nil, state: nil, user: "user", name: nil,
                                          email: nil, status: .unsupported, scopes: [])["error"] as? String, "IdentityCredentialError")
    }

    func testCancellationMatchesTheWebAndOtherFailuresKeepTheirCode() {
        for (code, expected) in [(ASAuthorizationError.canceled, "NetworkError"), (.notInteractive, "NotAllowedError"), (.invalidResponse, "IdentityCredentialError"), (.failed, "IdentityCredentialError")] {
            let answer = SignInModule.failure(NSError(domain: ASAuthorizationError.errorDomain, code: code.rawValue))
            XCTAssertEqual(answer["error"] as? String, expected)
            XCTAssertNotNil((answer["apple"] as? [String: String])?["code"])
        }
    }

    func testAbortRepliesOnceAndUsesTheSuppliedWindow() throws {
        #if os(macOS)
        _ = NSApplication.shared
        let window = NSWindow(contentRect: .init(x: 0, y: 0, width: 100, height: 100), styleMask: [], backing: .buffered, defer: false)
        #else
        let window = UIWindow()
        #endif
        let ceremony = AppleSignInCeremony(request: try SignInModule.request(options()), number: 42, passive: true, anchor: window)
        XCTAssertTrue(ceremony.presentationAnchor(for: ceremony.controller) === window)
        XCTAssertEqual(ceremony.number, 42)
        XCTAssertTrue(ceremony.passive)
        var replies: [[String: Any]] = []
        ceremony.done = { replies.append($0) }
        ceremony.abort()
        ceremony.authorizationController(controller: ceremony.controller, didCompleteWithError: NSError(domain: ASAuthorizationError.errorDomain, code: ASAuthorizationError.canceled.rawValue))
        XCTAssertEqual(replies.count, 1)
        XCTAssertEqual(replies.first?["error"] as? String, "AbortError")
    }

    func testModuleRefusesWithoutPresentationOrForATool() throws {
        let context = ExactPackageContext(agent: false, session: "sign-in-test", changed: { _ in }, log: { _ in }, now: { 0 })
        let module = SignInModule(context: context)
        var answer: [String: Any] = [:]
        // Enter after entitlement admission: the test process has no signed Apple capability.
        module.present(try SignInModule.request(options()), passive: false) { answer = $0 }
        XCTAssertEqual(answer["error"] as? String, "NotAllowedError")
        XCTAssertEqual(answer["message"] as? String, "the session has no presentation window")
        context.origin = "tool"
        module.later("identity", options(), preferred: nil) { answer = $0 }
        XCTAssertEqual(answer["error"] as? String, "NotAllowedError")
        XCTAssertEqual(answer["message"] as? String, "person-required")
    }
}
