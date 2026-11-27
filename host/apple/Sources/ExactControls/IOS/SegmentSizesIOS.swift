// @ref LLP 1069.011.001 D8 — a segmented control's `-exact-control-size`.
// UIKit's `UISegmentedControl` has no size of its own. SwiftUI's segmented
// picker takes `controlSize` and draws UIKit's control in a frame of that
// size's height. The host draws UIKit's control as tall as SwiftUI's picker
// of the same size. SwiftUI stays unlinked, as it would load at every launch,
// so the heights are SwiftUI's as measured.
#if os(iOS) || os(tvOS)
import UIKit

enum SegmentSizesIOS {
    /// How much taller than its own height UIKit's control is drawn at `size`, an
    /// `-exact-control-size` value: 0 where the row is unset or names a size no
    /// taller than the regular one, and on tvOS, whose picker takes no size.
    static func extra(_ size: String?) -> CGFloat {
        #if os(iOS)
        // SwiftUI's segmented picker, iOS 27.1: 31 points tall from mini to regular
        // and 48 at large and extra large, at every text size from XS to AX5
        size == "large" ? 48 - 31 : 0
        #else
        0
        #endif
    }
}
#endif
