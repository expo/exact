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

    /// One reserved region as the owner update carries it (LLP 1123 D16): its identity, its kind (0
    /// division, 1 occlusion), its frame including its margins, its margins and whether it is active.
    package struct Region: Equatable {
        package var id: UInt64
        package var kind: UInt8
        package var frame: CGRect
        package var margins: UIEdgeInsets
        package var active: Bool
    }

    /// The view's reserved regions of both kinds, active and inactive, each frame in the view's
    /// coordinates from its bounds' origin; `nil` before iOS 27.1, which has none.
    package static func all(of view: UIView) -> [Region]? {
        #if os(iOS)
        if #available(iOS 27.1, *) {
            let origin = view.bounds.origin
            let kinds: [(UIView.ReservedRegion.Kind, UInt8)] = [(.division, 0), (.occlusion, 1)]
            return kinds.flatMap { kind, raw in
                view.reservedRegions(kind: kind, options: .includeInactive).map {
                    Region(id: RegionIds.id(of: $0.id), kind: raw, frame: $0.frame.offsetBy(dx: -origin.x, dy: -origin.y),
                           margins: $0.margins, active: $0.isActive)
                }
            }
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

#if os(iOS)
/// The host's identities for UIKit's reserved regions: one per region identifier for the process's
/// life, so a region keeps its number across queries, which return new values for it.
@available(iOS 27.1, *)
private enum RegionIds {
    nonisolated(unsafe) private static var ids: [UIView.ReservedRegion.ID: UInt64] = [:]

    static func id(of region: UIView.ReservedRegion.ID) -> UInt64 {
        if let id = ids[region] { return id }
        let id = UInt64(ids.count + 1)
        ids[region] = id
        return id
    }
}
#endif
#endif
