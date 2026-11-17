// @exact/sign-in on Apple (LLP 1133.014): Apple requests, credential
// decoding and provider buttons. Passkeys owns the combined controller;
// Google and getCredentialState are not implemented here.
import AuthenticationServices
import ExactPackage
import Foundation
import Security
#if os(macOS)
import AppKit
#else
import UIKit
#endif

public final class SignInModule: NSObject, ExactPackage, ExactCredentialMember {
    let context: ExactPackageContext
    private var running: [ObjectIdentifier: AppleSignInCeremony] = [:]
    static let appleConfig = "https://appleid.apple.com"

    public init(context: ExactPackageContext) {
        self.context = context
        super.init()
        context.credentials.register("passkeys#get.identity", member: self)
    }

    public func credentialRequest(_ args: [String: Any]) throws -> ExactCredentialRequest {
        let request = try Self.request(args)
        let state = request.state
        return ExactCredentialRequest(requests: [request]) { credential in
            guard let credential = credential as? ASAuthorizationAppleIDCredential else { return nil }
            return Self.answer(credential, state: state)
        }
    }

    private var apple: Bool {
        #if os(macOS)
        guard let task = SecTaskCreateFromSelf(nil) else { return false }
        let values = SecTaskCopyValueForEntitlement(task, "com.apple.developer.applesignin" as CFString, nil) as? [String]
        return values?.contains("Default") == true
        #else
        // iOS has no public entitlement query. AuthenticationServices
        // checks the signed entitlement when it performs the request.
        return true
        #endif
    }

    static func capabilities(apple: Bool) -> [String: Any] {
        ["available": true, "providers": [
            "apple": ["active": apple, "passive": apple, "withPublicKey": apple],
            "google": ["active": false, "passive": false, "withPublicKey": false],
        ]]
    }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        switch name {
        case "getIdentityCapabilities": reply(Self.capabilities(apple: apple))
        case "preventSilentAccess": reply([:])
        case "identity": signIn(args, reply)
        default: reply(["error": "NotSupportedError", "message": "\(name) is not built on Apple yet (LLP 1133.014 §11 stage 2)"])
        }
    }

    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] {
        name == "getIdentityCapabilitiesSync" ? Self.capabilities(apple: apple) : ["error": "NotSupportedError"]
    }

    private func signIn(_ args: [String: Any], _ reply: @escaping ([String: Any]) -> Void) {
        guard args["publicKey"] == nil else { return reply(["error": "NotSupportedError", "message": "passkeys owns combined requests"]) }
        guard context.origin == "app", context.foreground else { return reply(["error": "NotAllowedError", "message": "person-required"]) }
        guard apple else { return reply(["error": "NotSupportedError", "message": "this build has no Sign in with Apple entitlement"]) }
        let request: ASAuthorizationAppleIDRequest
        do { request = try Self.request(args) }
        catch let failure as ExactCredentialFailure { return reply(failure.json) }
        catch { return reply(["error": "TypeError"]) }
        let passive = (args["identity"] as? [String: Any])?["mode"] as? String != "active"
        present(request, passive: passive, reply)
    }

    func present(_ request: ASAuthorizationAppleIDRequest, passive: Bool, _ reply: @escaping ([String: Any]) -> Void) {
        guard let anchor = context.presentation?.window else { return reply(["error": "NotAllowedError", "message": "the session has no presentation window"]) }
        let current = AppleSignInCeremony(request: request, number: context.request, passive: passive, anchor: anchor)
        current.done = reply
        hold(current) { [weak current] in current?.start() }
    }

    func hold(_ current: AppleSignInCeremony, start: @escaping () -> Void = {}) {
        let key = ObjectIdentifier(current)
        running[key] = current
        let reply = current.done
        let credentials = context.credentials
        current.done = { [weak self, weak current] answer in
            self?.running[key] = nil
            reply?(answer)
            if let current { credentials.finish(current) }
        }
        credentials.enqueue(current, replaceable: current.passive, modal: true,
                            start: start, cancel: { [weak current] in current?.abort($0) })
    }

    public func cancel(_ request: Int) {
        Array(running.values).filter { $0.number == request }.forEach { $0.abort() }
    }

    public func foreground(_ shown: Bool) {
        if !shown { context.credentials.cancelSheets() }
    }

    public func destroy(_ done: @escaping () -> Void) {
        context.credentials.unregister("passkeys#get.identity", member: self)
        context.credentials.cancel(Array(running.values))
        done()
    }

    /// The raw nonce is passed unchanged, as in the web authorization URL.
    /// Apple returns the supplied nonce in the ID token; hashing only this
    /// host's request would give the server two verification rules.
    static func request(_ args: [String: Any]) throws -> ASAuthorizationAppleIDRequest {
        let options = args["identity"] as? [String: Any] ?? [:]
        let providers = options["providers"] as? [[String: Any]] ?? []
        let mode = options["mode"] as? String ?? "passive"
        let mediation = args["mediation"] as? String ?? "optional"
        guard (args["publicKey"] == nil || mode == "passive"), ["active", "passive"].contains(mode),
              ["optional", "required"].contains(mediation),
              let provider = providers.first(where: { $0["configURL"] as? String == appleConfig }),
              mode != "active" || providers.count == 1 else {
            throw ExactCredentialFailure(["error": "NotSupportedError", "message": "Apple identity must be passive beside a passkey, with optional or required mediation"])
        }
        let request = ASAuthorizationAppleIDProvider().createRequest()
        request.nonce = (provider["params"] as? [String: Any])?["nonce"] as? String
        request.state = UUID().uuidString
        let fields = provider["fields"] as? [String] ?? ["name", "email", "picture"]
        request.requestedScopes = [ASAuthorization.Scope.fullName, .email].filter { fields.contains($0 == .fullName ? "name" : "email") }
        return request
    }

    static func answer(_ credential: ASAuthorizationAppleIDCredential, state: String?) -> [String: Any] {
        guard credential.state == state else {
            return ["error": "IdentityCredentialError", "message": "Apple returned an unexpected state"]
        }
        return answer(token: credential.identityToken, code: credential.authorizationCode, state: credential.state,
                      user: credential.user, name: credential.fullName, email: credential.email,
                      status: credential.realUserStatus, scopes: credential.authorizedScopes)
    }

    static func answer(token: Data?, code: Data?, state: String?, user: String, name: PersonNameComponents?,
                       email: String?, status: ASUserDetectionStatus, scopes: [ASAuthorization.Scope]) -> [String: Any] {
        guard let token, let tokenText = String(data: token, encoding: .utf8), !tokenText.isEmpty,
              let code, let codeText = String(data: code, encoding: .utf8), !codeText.isEmpty else {
            return ["error": "IdentityCredentialError", "message": "Apple returned no identity token or authorization code"]
        }
        var authorization = ["id_token": tokenText, "code": codeText]
        if let state { authorization["state"] = state }
        var body: [String: Any] = ["authorization": authorization]
        var person: [String: Any] = [:]
        if let name, name.givenName != nil || name.familyName != nil {
            person["name"] = ["firstName": name.givenName ?? "", "lastName": name.familyName ?? ""]
        }
        if let email { person["email"] = email }
        if !person.isEmpty { body["user"] = person }
        return ["type": "identity", "id": "", "configURL": appleConfig, "isAutoSelected": false,
                "token": body, "handle": UUID().uuidString,
                "apple": ["user": user, "realUserStatus": status == .likelyReal ? "likelyReal" : status == .unknown ? "unknown" : "unsupported",
                          "authorizedScopes": scopes.map { $0 == .fullName ? "name" : $0 == .email ? "email" : $0.rawValue }]]
    }

    static func failure(_ error: Error) -> [String: Any] {
        let ns = error as NSError
        let code = ns.domain == ASAuthorizationError.errorDomain ? ns.code : -1
        let words = [1000: "unknown", 1001: "canceled", 1002: "invalidResponse", 1003: "notHandled", 1004: "failed", 1005: "notInteractive"]
        let name = code == ASAuthorizationError.canceled.rawValue ? "NetworkError"
            : code == ASAuthorizationError.notInteractive.rawValue ? "NotAllowedError" : "IdentityCredentialError"
        return ["error": name, "message": ns.localizedDescription,
                "apple": ["code": words[code] ?? "\(ns.domain) \(ns.code)"]]
    }

    public func view(_ tag: String, id: String, props: [String: String], events: ExactPackageEvents) throws -> ExactPackageView {
        guard tag == "exact-sign-in-button" else { throw ExactPackageRefusal("no view \(tag)") }
        let button = SignInButton(events: events)
        try button.setProps(props)
        events.load()
        return button
    }
}

/// One request, anchored to the calling session, with exactly one reply.
final class AppleSignInCeremony: NSObject, ASAuthorizationControllerDelegate, ASAuthorizationControllerPresentationContextProviding {
    let controller: ASAuthorizationController
    let number: Int?
    let passive: Bool
    let anchor: ASPresentationAnchor
    let state: String?
    var done: (([String: Any]) -> Void)?

    init(request: ASAuthorizationAppleIDRequest, number: Int?, passive: Bool, anchor: ASPresentationAnchor) {
        controller = ASAuthorizationController(authorizationRequests: [request])
        self.number = number; self.passive = passive; self.anchor = anchor; state = request.state
        super.init()
        controller.delegate = self
        controller.presentationContextProvider = self
    }

    func start() {
        if passive { controller.performRequests(options: .preferImmediatelyAvailableCredentials) }
        else { controller.performRequests() }
    }

    func finish(_ answer: [String: Any]) {
        let reply = done
        done = nil
        reply?(answer)
    }

    func abort(_ answer: [String: Any] = ["error": "AbortError"]) {
        let reply = done
        done = nil
        controller.cancel()
        reply?(answer)
    }

    func authorizationController(controller: ASAuthorizationController, didCompleteWithAuthorization authorization: ASAuthorization) {
        guard let credential = authorization.credential as? ASAuthorizationAppleIDCredential else {
            return finish(["error": "IdentityCredentialError", "message": "Apple returned an unexpected credential"])
        }
        finish(SignInModule.answer(credential, state: state))
    }

    func authorizationController(controller: ASAuthorizationController, didCompleteWithError error: Error) {
        finish(SignInModule.failure(error))
    }

    func presentationAnchor(for controller: ASAuthorizationController) -> ASPresentationAnchor { anchor }
}

/// The provider's button: Apple's system button, or Google's standard one.
final class SignInButton: NSObject, ExactPackageView {
    let view: ExactPackagePlatformView
    private let events: ExactPackageEvents
    private var shown: (provider: String, context: String)?

    init(events: ExactPackageEvents) {
        self.events = events
        #if os(macOS)
        view = NSView()
        #else
        view = UIView()
        #endif
    }

    func setProps(_ props: [String: String]) throws {
        let provider = props["provider"] ?? "apple", context = props["context"] ?? "signin"
        guard provider == "apple" || provider == "google" else { throw ExactPackageRefusal("provider \(provider)") }
        if shown?.provider == provider, shown?.context == context { return }
        shown = (provider, context)
        for sub in view.subviews { sub.removeFromSuperview() }
        let made = provider == "apple" ? appleButton(context, radius: props["cornerRadius"].flatMap(Double.init)) : googleButton(context)
        made.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(made)
        NSLayoutConstraint.activate([
            made.leadingAnchor.constraint(equalTo: view.leadingAnchor), made.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            made.topAnchor.constraint(equalTo: view.topAnchor), made.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
    }

    @objc func pressed() { events.press() }

    private func appleButton(_ context: String, radius: Double?) -> ExactPackagePlatformView {
        let type: ASAuthorizationAppleIDButton.ButtonType = context == "signup" ? .signUp : context == "continue" ? .continue : .signIn
        #if os(macOS)
        let dark = NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        let button = ASAuthorizationAppleIDButton(authorizationButtonType: type, authorizationButtonStyle: dark ? .white : .black)
        button.target = self
        button.action = #selector(pressed)
        #else
        let dark = view.traitCollection.userInterfaceStyle == .dark
        let button = ASAuthorizationAppleIDButton(authorizationButtonType: type, authorizationButtonStyle: dark ? .white : .black)
        button.addTarget(self, action: #selector(pressed), for: .touchUpInside)
        #endif
        if let radius { button.cornerRadius = radius }
        return button
    }

    private func googleButton(_ context: String) -> ExactPackagePlatformView {
        let words = context == "signup" ? "Sign up with Google" : context == "continue" ? "Continue with Google" : "Sign in with Google"
        #if os(macOS)
        let button = NSButton(title: words, image: Self.mark(), target: self, action: #selector(pressed))
        button.imagePosition = .imageLeading
        button.bezelStyle = .rounded
        button.setAccessibilityLabel(words)
        #else
        var config = UIButton.Configuration.bordered()
        config.title = words
        config.image = Self.mark()
        config.imagePadding = 10
        config.baseForegroundColor = UIColor(red: 0.12, green: 0.12, blue: 0.12, alpha: 1)
        config.baseBackgroundColor = .white
        config.background.strokeColor = UIColor(red: 0.455, green: 0.467, blue: 0.459, alpha: 1)
        config.background.strokeWidth = 1
        config.cornerStyle = .small
        let button = UIButton(configuration: config)
        button.addTarget(self, action: #selector(pressed), for: .touchUpInside)
        button.accessibilityLabel = words
        #endif
        return button
    }

    /// Google's "G": four arcs in its colours and the bar, 18 points square.
    private static func mark() -> PlatformImage {
        let size = CGSize(width: 18, height: 18)
        let draw: (CGContext) -> Void = { g in
            let centre = CGPoint(x: 9, y: 9), r: CGFloat = 6.75, width: CGFloat = 3.4
            g.setLineWidth(width)
            let arcs: [(CGFloat, CGFloat, CGColor)] = [
                (-40, 45, CGColor(red: 0.259, green: 0.522, blue: 0.957, alpha: 1)),
                (45, 135, CGColor(red: 0.204, green: 0.659, blue: 0.325, alpha: 1)),
                (135, 200, CGColor(red: 0.984, green: 0.737, blue: 0.020, alpha: 1)),
                (200, 320, CGColor(red: 0.918, green: 0.263, blue: 0.208, alpha: 1)),
            ]
            for (from, to, colour) in arcs {
                g.setStrokeColor(colour)
                g.beginPath()
                g.addArc(center: centre, radius: r, startAngle: from * .pi / 180, endAngle: to * .pi / 180, clockwise: false)
                g.strokePath()
            }
            g.setFillColor(arcs[0].2)
            g.fill(CGRect(x: 9, y: 9 - width / 2, width: r + width / 2, height: width))
        }
        #if os(macOS)
        return NSImage(size: size, flipped: true) { _ in
            if let g = NSGraphicsContext.current?.cgContext { draw(g) }
            return true
        }
        #else
        return UIGraphicsImageRenderer(size: size).image { draw($0.cgContext) }
        #endif
    }
}

#if os(macOS)
typealias PlatformImage = NSImage
#else
typealias PlatformImage = UIImage
#endif
