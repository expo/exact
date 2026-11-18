// @ref LLP 1069.006 D3, LLP 1047.001.000 E — the sign-in sheet,
// `ASWebAuthenticationSession`, for `openAuthSession`. AuthenticationServices
// is in its own module because linking it costs every iOS launch, before
// `main`, the loading of ~300 more images (~1 s in the simulator). An app's
// composition links this module only when its grants name `auth.session`
// (`auth_session`) and installs it before any session is made; the core
// (`Auth.swift`) keeps the tickets, the agent's hold and the outcomes, and
// asks this module only to open and cancel the sheet. tvOS has no sheet, so
// installing it there does nothing.
import ExactKit

public enum ExactAuthSession {
    public static func install() {
        #if !os(tvOS)
        AuthSessionLink.installed = AuthSessionHost()
        #endif
    }
}

#if !os(tvOS)
import AuthenticationServices

final class AuthSessionHost: AuthSessionCapability {
    @available(iOS 17.4, macOS 14.4, *)
    static func callback(_ parts: URLComponents) -> ASWebAuthenticationSession.Callback {
        if parts.scheme?.lowercased() == "https" {
            return .https(host: parts.host!, path: parts.path)
        }
        return .customScheme(parts.scheme!.lowercased())
    }

    func open(url: URL, callback: URLComponents, ephemeral: Bool, anchor: @escaping () -> AuthAnchor?,
              done: @escaping (Result<URL, AuthFailure>) -> Void) -> Result<AuthSheet, AuthFailure> {
        guard let scheme = callback.scheme?.lowercased() else {
            return .failure(AuthFailure(status: 502, message: "the session's URL or callback is unreadable"))
        }
        let sheet = Sheet(anchor)
        let completion: ASWebAuthenticationSession.CompletionHandler = { url, error in
            DispatchQueue.main.async {
                if let url {
                    done(.success(url))
                } else {
                    done(.failure(Self.failure(error)))
                }
            }
        }
        let session: ASWebAuthenticationSession
        if scheme == "https" {
            guard #available(iOS 17.4, macOS 14.4, *), callback.host != nil else {
                return .failure(AuthFailure(status: 501, message: "an https callback needs iOS 17.4 or macOS 14.4"))
            }
            session = ASWebAuthenticationSession(url: url, callback: Self.callback(callback), completionHandler: completion)
        } else if #available(iOS 17.4, macOS 14.4, *) {
            session = ASWebAuthenticationSession(url: url, callback: Self.callback(callback), completionHandler: completion)
        } else {
            session = ASWebAuthenticationSession(url: url, callbackURLScheme: scheme, completionHandler: completion)
        }
        session.prefersEphemeralWebBrowserSession = ephemeral
        // The session holds its provider weakly; the sheet holds both.
        session.presentationContextProvider = sheet
        sheet.session = session
        guard session.start() else {
            return .failure(AuthFailure(status: 502, message: "the session could not start (no window)"))
        }
        return .success(sheet)
    }

    static func failure(_ error: Error?) -> AuthFailure {
        // Apple also uses canceledLogin for an unassociated HTTPS callback, with a failure reason.
        let reason = (error as NSError?)?.localizedFailureReason
        if reason == nil, (error as? ASWebAuthenticationSessionError)?.code == .canceledLogin {
            return AuthFailure(status: 499, message: "cancelled")
        }
        return AuthFailure(status: 502, message: reason ?? error?.localizedDescription ?? "the session ended without a callback")
    }

    private final class Sheet: NSObject, AuthSheet, ASWebAuthenticationPresentationContextProviding {
        let anchor: () -> AuthAnchor?
        var session: ASWebAuthenticationSession?
        init(_ anchor: @escaping () -> AuthAnchor?) { self.anchor = anchor }
        func cancel() { session?.cancel() }
        func presentationAnchor(for _: ASWebAuthenticationSession) -> ASPresentationAnchor {
            anchor() ?? ASPresentationAnchor()
        }
    }
}
#endif
