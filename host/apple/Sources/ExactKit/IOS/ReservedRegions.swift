// UIKit 27.1's fold (LLP 1078 D5): a view's division regions, the fold band and its margins, active
// while the hinge is partially open (`UIView.reservedRegions(kind:options:)`), and
// `UIHingeInteraction`, an update when the hinge moves, which changes a region's `isActive` without
// changing any bounds. tvOS, whose host compiles against its 27.0 SDK, has no fold.
#if os(iOS) || os(tvOS)
import UIKit

package enum ReservedRegions {
    /// Whether this UIKit has reserved regions at all (iOS 27.1 and later).
    static var available: Bool {
        #if os(iOS)
        if #available(iOS 27.1, *) { return true }
        #endif
        return false
    }

    /// The view's division regions, active and inactive, in its own coordinates: each frame
    /// includes the margins. `nil` before iOS 27.1.
    package static func divisions(of view: UIView) -> [(frame: CGRect, active: Bool)]? {
        #if os(iOS)
        if #available(iOS 27.1, *) {
            return view.reservedRegions(kind: .division, options: .includeInactive).map { ($0.frame, $0.isActive) }
        }
        #endif
        return nil
    }

    /// Add a `UIHingeInteraction` whose updates report the hinge angle in radians and its status
    /// (0 unknown, 1 closed, 2 partially open, 3 fully open). Both are nil without a hinge. Returns false
    /// before iOS 27.1.
    @discardableResult
    static func observeHinge(on view: UIView, changed: @escaping (Int?, CGFloat?) -> Void) -> Bool {
        #if os(iOS)
        if #available(iOS 27.1, *) {
            view.addInteraction(UIHingeInteraction { _, update in changed(update.hinge.map { $0.status.rawValue }, update.hinge?.angle) })
            return true
        }
        #endif
        return false
    }
}
#endif
