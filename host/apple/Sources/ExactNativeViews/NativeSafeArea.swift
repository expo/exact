// A native view's safe-area declaration as the host reads it (LLP 1123 D15): what the module's
// `ExactNativeSafeArea` says, crossing the module's table as Foundation and UIKit values, since the
// module and the host share no Swift type.
#if os(iOS) || os(tvOS)
import UIKit

package struct NativeSafeArea {
    /// Whether the host clears what is still owed in the view through its controller's additional
    /// safe-area insets.
    package var adjustsAdditionalSafeAreaInsets: Bool
    /// The scroll views the view lends, each with the `id` of the box whose policy it takes (nil for
    /// its own) and the layout that clears up to a given clearance and returns what it cleared.
    package var scrollViews: [(view: UIScrollView, anchor: String?, layout: ((UIEdgeInsets) -> UIEdgeInsets)?)]
    /// A SwiftUI host's `safeAreaRegions.rawValue`, nil where none is hosted.
    package var safeAreaRegions: UInt?
    /// The view's own undoing, after the host has given back what it wrote and borrowed.
    package var release: () -> Void

    /// The declaration `safe_area` returned; nil where it is not one, as from a module of another version.
    init?(_ d: NSDictionary) {
        typealias Layout = @convention(block) (UIEdgeInsets) -> UIEdgeInsets
        typealias Release = @convention(block) () -> Void
        guard let adjusts = d["adjusts"] as? Bool, let scrolls = d["scrollViews"] as? [NSDictionary] else { return nil }
        adjustsAdditionalSafeAreaInsets = adjusts
        scrollViews = scrolls.compactMap { s in
            guard let view = s["view"] as? UIScrollView else { return nil }
            let layout = s["layout"].map { unsafeBitCast($0 as AnyObject, to: Layout.self) }
            return (view, s["anchor"] as? String, layout.map { block in { block($0) } })
        }
        safeAreaRegions = (d["regions"] as? NSNumber)?.uintValue
        let release = d["release"].map { unsafeBitCast($0 as AnyObject, to: Release.self) }
        self.release = release.map { block in { block() } } ?? {}
    }
}

extension NativeViews {
    /// What the native view of node `id` declared of its safe area as it mounted (LLP 1123 D15).
    package func safeArea(of id: UInt32) -> NativeSafeArea? { entries[id]?.safeArea }
}
#endif
