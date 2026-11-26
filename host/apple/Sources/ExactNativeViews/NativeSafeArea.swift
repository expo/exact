// A native view's safe-area declaration as the host reads it (LLP 1123 D15): what the module's
// `ExactNativeSafeArea` says, crossing the module's table as Foundation and the toolkit's values, since
// the module and the host share no Swift type.
import ExactKit
#if os(iOS) || os(tvOS)
import UIKit
package typealias NativeScrollView = UIScrollView
package typealias NativeEdgeInsets = UIEdgeInsets
package typealias LentScroll = LentScrollIOS
#else
import AppKit
package typealias NativeScrollView = NSScrollView
package typealias NativeEdgeInsets = NSEdgeInsets
package typealias LentScroll = LentScrollMac
#endif

package struct NativeSafeArea {
    /// Whether the host clears what is still owed in the view through additional safe-area insets: its
    /// controller's on UIKit, its own on AppKit.
    package var adjustsAdditionalSafeAreaInsets: Bool
    /// The scroll views the view lends, each with the `id` of the box whose policy it takes (nil for
    /// its own) and the layout that clears up to a given clearance and returns what it cleared.
    package var scrollViews: [(view: NativeScrollView, anchor: String?, layout: ((NativeEdgeInsets) -> NativeEdgeInsets)?)]
    /// A SwiftUI host's `safeAreaRegions.rawValue`, nil where none is hosted.
    package var safeAreaRegions: UInt?
    /// The view's own undoing, after the host has given back what it wrote and borrowed.
    package var release: () -> Void

    /// The declaration `safe_area` returned; nil where it is not one, as from a module of another version.
    init?(_ d: NSDictionary) {
        typealias Layout = @convention(block) (NativeEdgeInsets) -> NativeEdgeInsets
        typealias Release = @convention(block) () -> Void
        guard let adjusts = d["adjusts"] as? Bool, let scrolls = d["scrollViews"] as? [NSDictionary] else { return nil }
        adjustsAdditionalSafeAreaInsets = adjusts
        scrollViews = scrolls.compactMap { s in
            guard let view = s["view"] as? NativeScrollView else { return nil }
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
