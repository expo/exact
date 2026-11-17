import XCTest
import AuthenticationServices
import ExactPackage
@testable import PasskeysImplementation
@testable import SignInImplementation
#if os(macOS)
import AppKit
#else
import UIKit
#endif

final class PasskeysMemberTests: XCTestCase {
    private func context(_ session: String = UUID().uuidString) -> ExactPackageContext {
        ExactPackageContext(agent: false, session: session, changed: { _ in }, log: { _ in }, now: { 0 })
    }

    private func window() -> ASPresentationAnchor {
        #if os(macOS)
        _ = NSApplication.shared
        return NSWindow(contentRect: .init(x: 0, y: 0, width: 100, height: 100), styleMask: [], backing: .buffered, defer: false)
        #else
        return UIWindow()
        #endif
    }

    private func options() -> [String: Any] {
        ["publicKey": ["rpId": "example.com", "challenge": "AQ"],
         "identity": ["mode": "passive", "providers": [["configURL": "https://appleid.apple.com", "clientId": "com.example.web",
                                                      "params": ["nonce": "raw-nonce"], "fields": ["email"]]]]]
    }

    func testAppleMemberIsSharedOnlyWithinItsSessionAndUnregisters() throws {
        let first = context(), same = context(first.session), other = context()
        let signIn = SignInModule(context: first)
        XCTAssertTrue(first.credentials === same.credentials)
        XCTAssertFalse(first.credentials === other.credentials)
        let member = try same.credentials.request("passkeys#get.identity", args: options())
        let request = try XCTUnwrap(member.requests.first as? ASAuthorizationAppleIDRequest)
        XCTAssertEqual(request.nonce, "raw-nonce")
        XCTAssertEqual(request.requestedScopes, [.email])
        XCTAssertNil(member.answer(Credential()))
        XCTAssertThrowsError(try other.credentials.request("passkeys#get.identity", args: options()))
        signIn.destroy {}
        XCTAssertThrowsError(try same.credentials.request("passkeys#get.identity", args: options()))
        let replacement = SignInModule(context: same)
        signIn.destroy {}
        XCTAssertNoThrow(try same.credentials.request("passkeys#get.identity", args: options()))
        withExtendedLifetime(replacement) {}
    }

    func testOneControllerContainsAppleAndPublicKeyRequests() throws {
        let context = context(), anchor = window()
        let signIn = SignInModule(context: context)
        let passkeys = PasskeysModule(context: context)
        let ceremony = try passkeys.prepareGet(options(), anchor: anchor) { _ in }
        XCTAssertEqual(ceremony.controller.authorizationRequests.count, 2)
        let platform = try XCTUnwrap(ceremony.controller.authorizationRequests.first as? ASAuthorizationPlatformPublicKeyCredentialAssertionRequest)
        XCTAssertEqual(platform.relyingPartyIdentifier, "example.com")
        XCTAssertEqual(platform.challenge, Data([1]))
        XCTAssertTrue(ceremony.controller.authorizationRequests.last is ASAuthorizationAppleIDRequest)
        XCTAssertTrue(ceremony.presentationAnchor(for: ceremony.controller) === anchor)
        var args = options(), identity = try XCTUnwrap(args["identity"] as? [String: Any])
        identity["mode"] = "active"; args["identity"] = identity
        XCTAssertThrowsError(try passkeys.prepareGet(args, anchor: anchor) { _ in })
        for mediation in ["silent", "conditional"] {
            var args = options(); args["mediation"] = mediation
            XCTAssertThrowsError(try passkeys.prepareGet(args, anchor: anchor) { _ in })
        }
        var key = options(); key["apple"] = ["securityKey": true]
        XCTAssertEqual(try passkeys.prepareGet(key, anchor: anchor) { _ in }.controller.authorizationRequests.count, 3)
        withExtendedLifetime(signIn) {}
    }

    func testMissingOrReleasedMemberIsRefusedByName() throws {
        let context = context(), passkeys = PasskeysModule(context: context)
        XCTAssertThrowsError(try passkeys.prepareGet(options(), anchor: window()) { _ in }) { error in
            let failure = error as? ExactCredentialFailure
            XCTAssertEqual(failure?.json["error"] as? String, "NotSupportedError")
            XCTAssertEqual(failure?.json["message"] as? String, "no credential member registered for passkeys#get.identity")
        }
        var member: Member? = Member()
        context.credentials.register("passkeys#get.identity", member: try XCTUnwrap(member))
        XCTAssertNoThrow(try passkeys.prepareGet(options(), anchor: window()) { _ in })
        member = nil
        XCTAssertThrowsError(try passkeys.prepareGet(options(), anchor: window()) { _ in })
    }

    func testPickedCredentialReachesItsDecoderAndRepliesOnce() throws {
        let context = context(), passkeys = PasskeysModule(context: context), member = Member()
        context.credentials.register("passkeys#get.identity", member: member)
        var replies: [[String: Any]] = []
        let identity = try passkeys.prepareGet(options(), anchor: window()) { replies.append($0) }
        passkeys.hold(identity)
        let next = try passkeys.prepareGet(options(), anchor: window()) { replies.append($0) }
        let firstState = try XCTUnwrap((identity.controller.authorizationRequests.last as? ASAuthorizationAppleIDRequest)?.state)
        let nextState = try XCTUnwrap((next.controller.authorizationRequests.last as? ASAuthorizationAppleIDRequest)?.state)
        XCTAssertNotEqual(firstState, nextState)
        identity.finish(.success(MemberCredential()))
        XCTAssertEqual(replies.count, 1)
        XCTAssertEqual(replies[0]["type"] as? String, "identity")
        XCTAssertEqual((replies[0]["token"] as? [String: Any])?["authorization"] as? [String: String],
                       ["id_token": "id-token", "code": "code", "state": firstState])
        identity.finish(.success(Assertion()))
        XCTAssertEqual(replies.count, 1)

        let publicKey = try passkeys.prepareGet(options(), anchor: window()) { replies.append($0) }
        publicKey.finish(.success(Assertion()))
        XCTAssertEqual(replies.count, 2)
        XCTAssertEqual(replies[1]["type"] as? String, "public-key")
        XCTAssertEqual(replies[1]["id"] as? String, "AQI")
        XCTAssertEqual((replies[1]["response"] as? [String: String])?["userHandle"], "Aw")
        XCTAssertEqual(member.answers, 1)

        next.finish(.success(MemberCredential()))
        XCTAssertEqual((replies.last?["token"] as? [String: Any])?["authorization"] as? [String: String],
                       ["id_token": "id-token", "code": "code", "state": nextState])

        let unknown = try passkeys.prepareGet(options(), anchor: window()) { replies.append($0) }
        unknown.finish(.success(Credential()))
        XCTAssertEqual(replies.last?["error"] as? String, "UnknownError")
    }

    func testCancellationAndSupersessionLeaveOtherCallsAndSessionsLive() throws {
        let context = context(), passkeys = PasskeysModule(context: context), member = Member()
        context.credentials.register("passkeys#get.identity", member: member)
        var replies: [Int: [[String: Any]]] = [:]
        func pending(_ number: Int, combined: Bool = true) throws -> PasskeysModule.Ceremony {
            context.request = number
            var args = options()
            if !combined { args["identity"] = nil }
            let ceremony = try passkeys.prepareGet(args, anchor: window()) { replies[number, default: []].append($0) }
            passkeys.hold(ceremony)
            return ceremony
        }
        let first = try pending(1)
        passkeys.cancel(1)
        XCTAssertEqual(replies[1]?.first?["error"] as? String, "AbortError")
        first.finish(.success(MemberCredential()))
        XCTAssertEqual(replies[1]?.count, 1)
        let other = try pending(2, combined: false)
        let passive = try pending(3)
        let replacement = try pending(4, combined: false)
        XCTAssertEqual(replies[3]?.first?["error"] as? String, "AbortError")
        passive.authorizationController(controller: passive.controller, didCompleteWithError: NSError(domain: ASAuthorizationError.errorDomain, code: 1001))
        XCTAssertEqual(replies[3]?.count, 1)
        XCTAssertNil(replies[2]); XCTAssertNil(replies[4])

        let secondContext = self.context(), second = PasskeysModule(context: secondContext)
        secondContext.credentials.register("passkeys#get.identity", member: member)
        secondContext.request = 4
        var secondReplies: [[String: Any]] = []
        let independent = try second.prepareGet(options(), anchor: window()) { secondReplies.append($0) }
        second.hold(independent)
        passkeys.cancel(4)
        XCTAssertTrue(secondReplies.isEmpty)
        independent.finish(.success(MemberCredential()))
        XCTAssertEqual(secondReplies.first?["type"] as? String, "identity")
        other.authorizationController(controller: other.controller, didCompleteWithError: NSError(domain: ASAuthorizationError.errorDomain, code: 1001))
        XCTAssertEqual(replies[2]?.first?["error"] as? String, "NotAllowedError")
        replacement.abort()
        XCTAssertEqual(replies[4]?.count, 1)

        _ = try pending(5)
        passkeys.foreground(false)
        XCTAssertEqual(replies[5]?.first?["error"] as? String, "NotAllowedError")
        let superseded = try pending(6), last = try pending(7)
        XCTAssertEqual(replies[6]?.first?["error"] as? String, "AbortError")
        superseded.finish(.success(MemberCredential()))
        XCTAssertEqual(replies[6]?.count, 1)
        var destroyed = false
        passkeys.destroy { destroyed = true }
        XCTAssertTrue(destroyed)
        XCTAssertEqual(replies[7]?.first?["error"] as? String, "AbortError")
        last.finish(.success(MemberCredential()))
        XCTAssertEqual(replies[7]?.count, 1)
    }

    func testNewCeremoniesSupersedePassiveRequestsAcrossPackages() throws {
        for combinedFirst in [true, false] {
            let context = context(), signIn = SignInModule(context: self.context(context.session))
            let passkeys = PasskeysModule(context: context)
            let otherContext = self.context(), other = PasskeysModule(context: otherContext), member = Member()
            otherContext.credentials.register("passkeys#get.identity", member: member)
            var replies: [[String: Any]] = [], otherReplies: [[String: Any]] = [], starts: [String] = []
            let independent = try other.prepareGet(options(), anchor: window()) { otherReplies.append($0) }
            other.hold(independent)
            context.request = 1
            signIn.context.request = 2
            var plain = options(); plain["identity"] = nil
            let combined = try passkeys.prepareGet(combinedFirst ? options() : plain, anchor: window()) { replies.append($0) }
            let standalone = AppleSignInCeremony(request: try SignInModule.request(options()), number: 2,
                                                passive: !combinedFirst, anchor: window())
            standalone.done = { replies.append($0) }
            if combinedFirst {
                passkeys.hold(combined) { starts.append("combined") }
                signIn.hold(standalone) { starts.append("standalone") }
            } else {
                signIn.hold(standalone) { starts.append("standalone") }
                passkeys.hold(combined) { starts.append("combined") }
            }
            XCTAssertEqual(starts.count, 2)
            XCTAssertEqual(replies.count, 1)
            XCTAssertEqual(replies.first?["error"] as? String, "AbortError")
            XCTAssertTrue(otherReplies.isEmpty)
            if combinedFirst {
                combined.finish(.success(Assertion()))
                passkeys.cancel(1)
                XCTAssertEqual(replies.count, 1)
                signIn.cancel(2)
            } else {
                standalone.finish(["type": "identity"])
                signIn.cancel(2)
                XCTAssertEqual(replies.count, 1)
                passkeys.cancel(1)
            }
            XCTAssertEqual(replies.count, 2)
            XCTAssertEqual(replies.last?["error"] as? String, "AbortError")
            independent.finish(.success(MemberCredential()))
            XCTAssertEqual(otherReplies.first?["type"] as? String, "identity")
        }
    }

    func testModalRequestsWaitAcrossPackagesAndCancelledRequestsNeverStart() throws {
        let context = context(), passkeys = PasskeysModule(context: context)
        let signIn = SignInModule(context: self.context(context.session))
        var replies: [Int: [[String: Any]]] = [:], starts: [Int] = []
        var plain = options(); plain["identity"] = nil
        func passkey(_ number: Int) throws -> PasskeysModule.Ceremony {
            context.request = number
            let ceremony = try passkeys.prepareGet(plain, anchor: window()) { replies[number, default: []].append($0) }
            passkeys.hold(ceremony) { starts.append(number) }
            return ceremony
        }
        func identity(_ number: Int) throws -> AppleSignInCeremony {
            let ceremony = AppleSignInCeremony(request: try SignInModule.request(options()), number: number,
                                              passive: false, anchor: window())
            ceremony.done = { replies[number, default: []].append($0) }
            signIn.hold(ceremony) { starts.append(number) }
            return ceremony
        }
        let first = try passkey(1), second = try identity(2), cancelled = try passkey(3)
        let fourth = try identity(4), fifth = try passkey(5)
        XCTAssertEqual(starts, [1])
        XCTAssertTrue(replies.isEmpty)
        passkeys.cancel(3)
        XCTAssertEqual(replies[3]?.first?["error"] as? String, "AbortError")
        XCTAssertEqual(starts, [1])
        cancelled.finish(.success(Assertion()))
        first.finish(.success(Assertion()))
        XCTAssertEqual(starts, [1, 2])
        first.abort()
        XCTAssertEqual(starts, [1, 2])
        signIn.cancel(2)
        XCTAssertEqual(starts, [1, 2, 4])
        second.finish(["type": "identity"])
        XCTAssertEqual(replies[2]?.count, 1)
        fourth.finish(["type": "identity"])
        XCTAssertEqual(starts, [1, 2, 4, 5])
        fifth.finish(.success(Assertion()))
        XCTAssertEqual(replies.count, 5)
        XCTAssertTrue(replies.values.allSatisfy { $0.count == 1 })
    }

    func testBackgroundingCancelsActiveAndQueuedSheetsWithNotAllowedError() throws {
        let context = context(), passkeys = PasskeysModule(context: context), signIn = SignInModule(context: context)
        var replies: [[String: Any]] = [], starts: [String] = []
        let active = AppleSignInCeremony(request: try SignInModule.request(options()), number: 1, passive: false, anchor: window())
        active.done = { replies.append($0) }
        signIn.hold(active) { starts.append("identity") }
        let queued = try passkeys.prepareGet(options(), anchor: window()) { replies.append($0) }
        passkeys.hold(queued) { starts.append("combined") }
        XCTAssertEqual(starts, ["identity"])
        signIn.foreground(false)
        passkeys.foreground(false)
        XCTAssertEqual(replies.count, 2)
        XCTAssertTrue(replies.allSatisfy { $0["error"] as? String == "NotAllowedError" })
        XCTAssertEqual(starts, ["identity"])
        active.finish(["type": "identity"])
        queued.finish(.success(Assertion()))
        XCTAssertEqual(replies.count, 2)
        passkeys.foreground(true)
        signIn.foreground(true)
        XCTAssertEqual(starts, ["identity"])
    }

    func testSessionDestructionDoesNotStartWaitingSheetsOrEndAnotherSession() throws {
        let context = context(), passkeys = PasskeysModule(context: context), signIn = SignInModule(context: context)
        let otherContext = self.context(), other = PasskeysModule(context: otherContext)
        var replies: [[String: Any]] = [], otherReplies: [[String: Any]] = [], starts = 0
        var plain = options(); plain["identity"] = nil
        let first = try passkeys.prepareGet(plain, anchor: window()) { replies.append($0) }
        passkeys.hold(first) { starts += 1 }
        let queued = AppleSignInCeremony(request: try SignInModule.request(options()), number: 2, passive: false, anchor: window())
        queued.done = { replies.append($0) }
        signIn.hold(queued) { starts += 1 }
        let independent = try other.prepareGet(plain, anchor: window()) { otherReplies.append($0) }
        other.hold(independent)
        ExactCredentialMembers.cancelSession(context.session)
        passkeys.destroy {}
        signIn.destroy {}
        XCTAssertEqual(starts, 1)
        XCTAssertEqual(replies.count, 2)
        XCTAssertTrue(replies.allSatisfy { $0["error"] as? String == "AbortError" })
        XCTAssertTrue(otherReplies.isEmpty)
        first.finish(.success(Assertion()))
        queued.finish(["type": "identity"])
        XCTAssertEqual(replies.count, 2)
        independent.finish(.success(Assertion()))
        XCTAssertEqual(otherReplies.first?["type"] as? String, "public-key")

        let last = self.context(), module = PasskeysModule(context: last)
        for _ in 0..<3 {
            let pending = try module.prepareGet(plain, anchor: window()) { replies.append($0) }
            module.hold(pending) { starts += 1 }
        }
        module.destroy {}
        XCTAssertEqual(starts, 2)
        XCTAssertEqual(replies.count, 5)
        XCTAssertTrue(replies.suffix(3).allSatisfy { $0["error"] as? String == "AbortError" })
    }

    #if os(iOS)
    func testStandaloneIdentitySupersedesConditionalAutoFill() throws {
        let context = context(), passkeys = PasskeysModule(context: context), signIn = SignInModule(context: context)
        var args = options(); args["identity"] = nil; args["mediation"] = "conditional"
        var replies: [[String: Any]] = []
        let conditional = try passkeys.prepareGet(args, anchor: window()) { replies.append($0) }
        passkeys.hold(conditional)
        signIn.foreground(false)
        XCTAssertTrue(replies.isEmpty)
        signIn.foreground(true)
        let active = AppleSignInCeremony(request: try SignInModule.request(options()), number: 2, passive: false, anchor: window())
        active.done = { replies.append($0) }
        signIn.hold(active)
        XCTAssertEqual(replies.count, 1)
        XCTAssertEqual(replies.first?["error"] as? String, "AbortError")
        conditional.finish(.success(Assertion()))
        XCTAssertEqual(replies.count, 1)
        signIn.destroy {}
        XCTAssertEqual(replies.count, 2)
    }

    func testConditionalGetSurvivesBackgroundUntilItFinishesOrIsReplaced() throws {
        let context = context(), passkeys = PasskeysModule(context: context), member = Member()
        context.credentials.register("passkeys#get.identity", member: member)
        var args = options()
        args["identity"] = nil
        args["mediation"] = "conditional"
        var replies: [[String: Any]] = []
        let conditional = try passkeys.prepareGet(args, anchor: window()) { replies.append($0) }
        passkeys.hold(conditional)
        passkeys.foreground(false)
        XCTAssertTrue(replies.isEmpty)
        passkeys.foreground(true)
        conditional.finish(.success(Assertion()))
        XCTAssertEqual(replies.count, 1)
        XCTAssertEqual(replies.first?["type"] as? String, "public-key")

        let pending = try passkeys.prepareGet(args, anchor: window()) { replies.append($0) }
        passkeys.hold(pending)
        passkeys.foreground(false)
        XCTAssertEqual(replies.count, 1)
        passkeys.foreground(true)
        let combined = try passkeys.prepareGet(options(), anchor: window()) { replies.append($0) }
        passkeys.hold(combined)
        XCTAssertEqual(replies.count, 2)
        XCTAssertEqual(replies.last?["error"] as? String, "AbortError")
        pending.finish(.success(Assertion()))
        XCTAssertEqual(replies.count, 2)
        passkeys.foreground(false)
        XCTAssertEqual(replies.count, 3)
        XCTAssertEqual(replies.last?["error"] as? String, "NotAllowedError")
        let last = try passkeys.prepareGet(args, anchor: window()) { replies.append($0) }
        passkeys.hold(last)
        passkeys.foreground(false)
        XCTAssertEqual(replies.count, 3)
        passkeys.destroy {}
        XCTAssertEqual(replies.count, 4)
        XCTAssertEqual(replies.last?["error"] as? String, "AbortError")
        last.finish(.success(Assertion()))
        XCTAssertEqual(replies.count, 4)
    }
    #endif

    @objc(ExactMemberTestCredential) private class Credential: NSObject, ASAuthorizationCredential {
        static var supportsSecureCoding: Bool { true }
        override init() { super.init() }
        required init?(coder: NSCoder) { super.init() }
        func encode(with coder: NSCoder) {}
        func copy(with zone: NSZone? = nil) -> Any { self }
    }

    private final class MemberCredential: Credential {}

    private final class Assertion: Credential, ASAuthorizationPublicKeyCredentialAssertion {
        let credentialID = Data([1, 2]), userID = Data([3]), rawClientDataJSON = Data([4])
        let rawAuthenticatorData = Data([5]), signature = Data([6])
    }

    private final class Member: ExactCredentialMember {
        var answers = 0
        func credentialRequest(_ args: [String: Any]) throws -> ExactCredentialRequest {
            let request = try SignInModule.request(args)
            let state = request.state
            return ExactCredentialRequest(requests: [request]) { [self] credential in
                guard credential is MemberCredential else { return nil }
                answers += 1
                return SignInModule.answer(token: Data("id-token".utf8), code: Data("code".utf8), state: state,
                                           user: "apple-user", name: nil, email: nil, status: .likelyReal, scopes: [.email])
            }
        }
    }
}
