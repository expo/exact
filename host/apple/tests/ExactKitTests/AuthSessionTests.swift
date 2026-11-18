import AuthenticationServices
import Network
import XCTest
@testable import ExactAuthSession
@testable import ExactKit
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

#if !os(tvOS)
final class AuthSessionTests: XCTestCase {
    func testHTTPSCallbackMatchesTheDeclaredHostAndPathWithConvexParameters() throws {
        guard #available(iOS 17.4, macOS 14.4, *) else { throw XCTSkip("HTTPS callbacks need iOS 17.4 or macOS 14.4") }
        let parts = try XCTUnwrap(URLComponents(string: "https://auth.example.test/return"))
        let callback = AuthSessionHost.callback(parts)
        for query in ["convexAuthCode=one&state=mine", "convexAuthError=denied&state=mine"] {
            XCTAssertTrue(callback.matchesURL(try XCTUnwrap(URL(string: "https://auth.example.test/return?\(query)"))))
        }
        // The runner checks scheme, exact path and state after Apple's broader navigation matcher returns.
        for url in ["https://other.example.test/return", "https://auth.example.test/elsewhere"] {
            XCTAssertFalse(callback.matchesURL(try XCTUnwrap(URL(string: url))), url)
        }
    }

    func testAnAssociationFailureKeepsItsReasonAndIsNotUserCancellation() {
        let reason = "Application with identifier fixture is not associated with domain auth.exact.invalid."
        let error = NSError(domain: ASWebAuthenticationSessionErrorDomain, code: 1,
                            userInfo: [NSLocalizedFailureReasonErrorKey: reason])
        let failure = AuthSessionHost.failure(error)
        XCTAssertEqual(failure.status, 502)
        XCTAssertEqual(failure.message, reason)
        let cancelled = AuthSessionHost.failure(NSError(domain: ASWebAuthenticationSessionErrorDomain, code: 1))
        XCTAssertEqual(cancelled.status, 499)
        XCTAssertEqual(cancelled.message, "cancelled")
        XCTAssertEqual(AuthSessionHost.failure(nil).status, 502)
    }

    func testARealSessionReturnsConvexParametersThroughAnHTTPSCallback() throws {
        guard #available(iOS 17.4, macOS 14.4, *) else { throw XCTSkip("HTTPS callbacks need iOS 17.4 or macOS 14.4") }
        #if canImport(UIKit)
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the auth session needs a scene in which to present its browser")
        }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        #else
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300),
                              styleMask: [.titled], backing: .buffered, defer: false)
        window.makeKeyAndOrderFront(nil)
        defer { window.orderOut(nil) }
        #endif
        let returned = try XCTUnwrap(URL(string: "https://auth.exact.invalid/return?convexAuthCode=fixture-code&state=fixture-state"))
        let callback = try XCTUnwrap(URLComponents(string: "https://auth.exact.invalid/return"))
        var redirects = 0
        let server = try loopbackListener { connection in
            connection.start(queue: .main)
            connection.receive(minimumIncompleteLength: 1, maximumLength: 16384) { data, _, _, error in
                guard data != nil, error == nil else { connection.cancel(); return }
                redirects += 1
                let response = "HTTP/1.1 302 Found\r\nLocation: \(returned.absoluteString)\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                connection.send(content: Data(response.utf8), completion: .contentProcessed { _ in connection.cancel() })
            }
        }
        defer { server.cancel() }
        let start = try XCTUnwrap(URL(string: "http://127.0.0.1:\(try XCTUnwrap(server.port).rawValue)/authorize"))
        let completed = expectation(description: "the HTTPS return or Apple's association refusal")
        var result: Result<URL, AuthFailure>?
        let opened = AuthSessionHost().open(url: start, callback: callback, ephemeral: true, anchor: { window }) {
            result = $0
            completed.fulfill()
        }
        let sheet: AuthSheet
        switch opened {
        case .success(let value): sheet = value
        case .failure(let failure): XCTFail("HTTPS session failed to start: \(failure.status) \(failure.message)"); return
        }
        defer { sheet.cancel() }
        wait(for: [completed], timeout: 30)
        switch try XCTUnwrap(result) {
        case .success(let url):
            XCTAssertGreaterThan(redirects, 0)
            XCTAssertEqual(url, returned)
            let query = try XCTUnwrap(URLComponents(url: url, resolvingAgainstBaseURL: false)).queryItems
            XCTAssertEqual(query?.first(where: { $0.name == "convexAuthCode" })?.value, "fixture-code")
            XCTAssertEqual(query?.first(where: { $0.name == "state" })?.value, "fixture-state")
        case .failure(let failure):
            XCTAssertEqual(failure.status, 502)
            guard failure.message.contains("not associated with domain") else {
                XCTFail("HTTPS return failed after \(redirects) redirects: \(failure.message)"); return
            }
            // This test runner has no signed webcredentials claim for the fixture domain.
            throw XCTSkip("HTTPS return blocked after \(redirects) redirects: \(failure.message)")
        }
    }
}
#endif
