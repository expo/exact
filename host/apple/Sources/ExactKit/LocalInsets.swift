// @ref LLP 1123 D9 — each content owner's local insets, which
// `var(--exact-safe-area-inset-*)` reads: what its container's chrome (a
// navigation bar, a tab bar, the status bar over a whole-view stack, a
// sheet's edges) takes of its box, as UIKit's `safeAreaInsets` and AppKit's
// have it. A host reports them after the batch being applied, as it reports
// covers, and only what changed.
import CoreGraphics

/// What a host wants the kernel to hold of local insets, by the owner's root
/// view (nil: the session root), and what it last told it.
struct LocalInsetsReport {
    /// Top, right, bottom, left, in points.
    var wanted: [UInt32?: [CGFloat]] = [:]
    private var delivered: [UInt32?: [CGFloat]] = [:]

    /// What changed since the last delivery, owners whose view is gone left
    /// out (their node took its insets with it); marked delivered.
    mutating func take(live: (UInt32) -> Bool) -> [(UInt32?, [CGFloat]?)] {
        var changes: [(UInt32?, [CGFloat]?)] = []
        for (owner, insets) in wanted where delivered[owner] != insets && owner.map(live) != false {
            changes.append((owner, insets))
        }
        for owner in delivered.keys where wanted[owner] == nil && owner.map(live) != false {
            changes.append((owner, nil))
        }
        delivered = wanted.filter { $0.key.map(live) != false }
        return changes
    }
}

extension ExactSession {
    /// Tell the kernel `changes` to local insets, between batches.
    func localInsets(_ changes: [(UInt32?, [CGFloat]?)]) {
        guard !changes.isEmpty else { return }
        whenIdle(bound: true) { [unowned self] in applyUnlessEmpty(runtime.localInsets(changes)) }
    }
}
