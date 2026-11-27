// LLP 1123 D9: native targets' identities, shared by both Apple hosts.
import Foundation
import ObjectiveC

/// The host's identities for native targets: one per view or layout guide for its life, never
/// reused, so a new one at a freed one's address is a new target (D9).
package enum TargetIds {
    nonisolated(unsafe) private static var next: UInt64 = 1
    nonisolated(unsafe) private static var key: UInt8 = 0

    package static func id(of object: NSObject) -> UInt64 {
        if let id = objc_getAssociatedObject(object, &key) as? NSNumber { return id.uint64Value }
        let id = next
        next += 1
        objc_setAssociatedObject(object, &key, NSNumber(value: id), .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        return id
    }
}
