import Foundation

/// The linked backend performs the platform check and reads the locked item.
package protocol PresenceCapability: AnyObject {
    func check(_ op: [String: Any], authenticator: (UnsafeMutableRawPointer, PresenceAuthenticateFn)?,
               current: @escaping () -> Bool, answer: @escaping ([String: Any]) -> Void,
               failed: @escaping (String, String) -> Void)
}

package enum PresenceLink {
    package static var installed: PresenceCapability?
}

/// The authenticator entry's type (NativeModule.swift's table, offset 232).
package typealias PresenceAuthenticateFn = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int, UnsafeMutableRawPointer?, UnsafeMutableRawPointer?,
                                                   (@convention(c) (UnsafeMutableRawPointer?, UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int) -> Void)?) -> Void

extension ExactSession {
    /// Route a check and its answer through the generation that requested it.
    /// `issued` is the generation whose batch asked: the call number is its
    /// runner's, so the answer goes nowhere once that generation has ended.
    func presenceOp(_ op: [String: Any], issued: GenerationStamp) {
        guard let call = (op["call"] as? NSNumber)?.uint64Value else { return }
        let answer = { (fields: [String: Any]) in
            var fields = fields
            fields["call"] = call
            issued.session?.runtime.presence(fields)
        }
        let failed = { (code: String, message: String) in
            guard let session = issued.session else { return }
            session.runtime.log("presence \(op["name"] as? String ?? ""): the platform's check failed \(code): \(message)")
            answer(["status": "failed", "code": code, "message": message])
        }
        guard let backend = PresenceLink.installed else {
            return failed("NotSupportedError", "this build links no presence backend")
        }
        backend.check(op, authenticator: natives.authenticator(), current: { issued.session != nil }, answer: answer, failed: failed)
    }
}
