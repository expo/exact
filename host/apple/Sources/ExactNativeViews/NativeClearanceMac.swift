// A native view's clearance and lent scroll views on AppKit (LLP 1123 D15): what is still owed in the view after reach,
// written to the view's own `additionalSafeAreaInsets` as Exact's tagged contribution beside the app's
// (`ContributionsMac`), where its declaration consents, and the scroll views it lends, which D14's writer claims; both
// given back before the module destroys the view, then its declaration's release runs.
#if os(macOS)
import AppKit
import ExactKit

extension NativeViews {
    /// Read the view's safe-area declaration as it mounts, and put the view in the node's box.
    func mountView(_ entry: NativeEntry, table: NativeTable) {
        guard let owner = entry.owner, let view = entry.view else { return }
        entry.safeArea = entry.handle.flatMap { table.safeArea?($0) }.flatMap {
            NativeSafeArea(Unmanaged<NSDictionary>.fromOpaque($0).takeRetainedValue())
        }
        entry.lent = entry.safeArea?.scrollViews.map { LentScrollMac(view: $0.view, layout: $0.layout) } ?? []
        owner.addSubview(view)
    }

    /// Clear `owed` in node `id`'s native view, in the view's own coordinates, through its own additional safe-area
    /// insets, where its declaration consents; whether it did. Activation supplies `owed` after reach (stage 5); tests
    /// call it directly.
    @discardableResult
    package func clear(_ id: UInt32, owed: NSEdgeInsets) -> Bool {
        guard let entry = entries[id], entry.safeArea?.adjustsAdditionalSafeAreaInsets == true, let view = entry.view else { return false }
        ContributionsMac.of(view).write(Self.clearance(of: id), owed)
        return true
    }

    /// Give back what the host wrote and borrowed for `entry`'s view, its clearance and its lent scroll views'
    /// properties, then run its declaration's release.
    func releaseSafeArea(_ entry: NativeEntry) {
        guard let declared = entry.safeArea else { return }
        if let view = entry.view, let ledger = ContributionsMac.existing(on: view) {
            ledger.write(Self.clearance(of: entry.id), NSEdgeInsetsZero)
        }
        entry.lent.forEach { $0.release() }
        declared.release()
    }

    /// The scroll views node `id`'s native view lends, which activation writes through D14's writer (stage 5); tests
    /// write them directly.
    package func lent(of id: UInt32) -> [LentScrollMac] { entries[id]?.lent ?? [] }

    private static func clearance(of id: UInt32) -> ContributionsMac.Identity { .init(node: id, operation: .nativeViewInsets) }
}
#endif
