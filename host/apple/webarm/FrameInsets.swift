// How much of a frame's guest lies under its owner's bars and band, and the range that moves
// through (@ref LLP 1123 D15), as WebKit takes them on iOS.
import WebKit

#if os(iOS)
import UIKit

extension WebArm {
    /// The obscured insets and their viewport range, where the frame's options say them: a word said
    /// and then dropped gives WebKit back its default.
    func applyInsets(to web: WKWebView, said: (String) -> Bool) {
        // How much of the guest lies under its owner's bars and band, top left bottom right (LLP 1123
        // D15): WebKit lays the page's fixed and sticky content and its visible rect out clear of it, as
        // in a web view pinned under a navigation bar
        if #available(iOS 26.0, *), said("obscured") {
            let o = (options["obscured"] as? [NSNumber])?.map(\.doubleValue) ?? [0, 0, 0, 0]
            let insets = o.count == 4 ? UIEdgeInsets(top: o[0], left: o[1], bottom: o[2], right: o[3]) : .zero
            if web.obscuredContentInsets != insets { web.obscuredContentInsets = insets }
        }
        // The range those insets move through, least then most: WebKit's large viewport clears the
        // least and its small viewport the most, as under bars that collapse (`lvh`, `svh`)
        if said("viewportRange") {
            let r = (options["viewportRange"] as? [NSNumber])?.map(\.doubleValue) ?? []
            let edges = { (i: Int) in r.count == 8 ? UIEdgeInsets(top: r[i], left: r[i + 1], bottom: r[i + 2], right: r[i + 3]) : .zero }
            let (least, most) = (edges(0), edges(4))
            if web.minimumViewportInset != least || web.maximumViewportInset != most {
                web.setMinimumViewportInset(least, maximumViewportInset: most)
            }
        }
    }
}
#endif
