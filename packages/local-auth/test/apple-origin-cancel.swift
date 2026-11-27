import ExactPackage
import Foundation
import LocalAuthentication

@main
private struct OriginCancelTest {
    static func main() {
        let context = ExactPackageContext(
            agent: false, session: "local-auth-test",
            changed: { _ in }, log: { _ in }, now: { 0 }
        )
        let module = LocalAuthModule(context: context)

        // 1. Reverse interleave: tool-origin authenticate followed immediately by app-origin localAuthStatus.
        context.origin = "tool"
        context.request = 10
        var toolAnswer: [String: Any]?
        module.later("authenticate", ["reason": "Unlock"], preferred: nil) { toolAnswer = $0 }
        context.origin = "app"
        context.request = nil
        var statusAnswer: [String: Any]?
        module.later("localAuthStatus", [:], preferred: nil) { statusAnswer = $0 }
        guard toolAnswer?["error"] as? String == "PERMISSION_DENIED",
              toolAnswer?["message"] as? String == "person-required" else {
            fail("tool-origin authenticate was not refused: \(String(describing: toolAnswer))")
        }
        guard statusAnswer?["available"] != nil else {
            fail("localAuthStatus did not answer: \(String(describing: statusAnswer))")
        }

        // 2. Forward interleave + cancel(request): app-origin authenticate followed immediately by tool-origin status.
        var capturedContext: LAContext?
        var capturedReuse: TimeInterval = -1
        LocalAuthModule.hatch = { ctx in
            capturedContext = ctx
            capturedReuse = ctx.touchIDAuthenticationAllowableReuseDuration
        }
        defer { LocalAuthModule.hatch = nil }

        context.origin = "app"
        context.request = 11
        var appAnswer: [String: Any]?
        module.later("authenticate", [
            "reason": "Unlock",
            "biometricOnly": true,
            "apple": ["touchIDAuthenticationAllowableReuseDuration": 10.0],
        ], preferred: nil) { appAnswer = $0 }
        // Immediately overwrite origin as an interleaved tool read would.
        context.origin = "tool"
        context.request = nil
        module.later("localAuthStatus", [:], preferred: nil) { _ in }

        guard let ctx = capturedContext else {
            fail("app-origin authenticate did not capture context synchronously before origin changed")
        }
        guard capturedReuse == 10.0 else {
            fail("expected reuse duration 10.0 for app origin, got \(capturedReuse)")
        }

        // Cancel with a non-matching request id must not invalidate ctx.
        module.cancel(99)
        var errBefore: NSError?
        _ = ctx.canEvaluatePolicy(.deviceOwnerAuthentication, error: &errBefore)
        guard errBefore.flatMap({ LAError.Code(rawValue: $0.code) }) != .invalidContext else {
            fail("unrelated cancel(99) invalidated the context")
        }

        // Cancel with the matching request id must invalidate ctx.
        module.cancel(11)
        var errAfter: NSError?
        _ = ctx.canEvaluatePolicy(.deviceOwnerAuthentication, error: &errAfter)
        guard errAfter.flatMap({ LAError.Code(rawValue: $0.code) }) == .invalidContext else {
            fail("matching cancel(11) did not invalidate the context: \(String(describing: errAfter))")
        }

        let deadline = Date(timeIntervalSinceNow: 2)
        while appAnswer == nil, Date() < deadline {
            RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01))
        }
        guard let appAnswer else { fail("app-origin authenticate did not settle") }
        guard appAnswer["error"] as? String != "PERMISSION_DENIED" else {
            fail("app-origin authenticate failed with PERMISSION_DENIED after interleaved tool call: \(appAnswer)")
        }
        print("local-auth origin+cancel ok")
    }

    private static func fail(_ message: String) -> Never {
        FileHandle.standardError.write(Data("\(message)\n".utf8))
        exit(1)
    }
}
