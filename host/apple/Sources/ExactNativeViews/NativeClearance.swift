// A native view's clearance and lent scroll views (LLP 1123 D15): what is still owed in the view after
// reach, written to its own controller's `additionalSafeAreaInsets` as Exact's tagged contribution
// beside the app's, where its declaration consents, and the scroll views it lends, which D14's writer
// writes; both given back before the controller moves, the module resets the view or the view goes.
#if os(iOS) || os(tvOS)
import ExactKit
import UIKit

extension NativeViews {
    /// Clear `owed` in node `id`'s native view, in the view's own coordinates, through its
    /// controller, where its declaration consents and it has one; whether it did. Activation
    /// supplies `owed` after reach (stage 5); tests call it directly.
    @discardableResult
    package func clear(_ id: UInt32, owed: UIEdgeInsets) -> Bool {
        guard let entry = entries[id], entry.safeArea?.adjustsAdditionalSafeAreaInsets == true, let controller = entry.screen else { return false }
        ContributionsIOS.of(controller).write(Self.clearance(of: id), owed)
        return true
    }

    /// Give back what the host wrote and borrowed for `entry`'s view, its clearance and its lent
    /// scroll views' properties, then run its declaration's release.
    func releaseSafeArea(_ entry: NativeEntry) {
        guard let declared = entry.safeArea else { return }
        if let controller = entry.screen, let ledger = ContributionsIOS.existing(on: controller) {
            ledger.write(Self.clearance(of: entry.id), .zero)
        }
        entry.lent.forEach { $0.release() }
        declared.release()
    }

    /// The scroll views node `id`'s native view lends, which activation writes through D14's writer
    /// (stage 5); tests write them directly.
    package func lent(of id: UInt32) -> [LentScrollIOS] { entries[id]?.lent ?? [] }

    private static func clearance(of id: UInt32) -> ContributionsIOS.Identity { .init(node: id, operation: .nativeViewInsets) }
}
#endif
