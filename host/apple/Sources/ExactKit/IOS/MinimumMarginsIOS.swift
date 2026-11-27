// D8's minimum margins on an owner's controller (LLP 1123, `-exact-respects-system-minimum-layout-margins`).
#if os(iOS) || os(tvOS)
import UIKit

extension UIViewController {
    /// The owner's root `node`'s `-exact-respects-system-minimum-layout-margins`, as UIKit's
    /// `viewRespectsSystemMinimumLayoutMargins`: `false` lets the controller's view keep margins below the system's
    /// minimum, which the margins the host reports of it then follow.
    package func followMinimumMargins(of node: NodeView) {
        let respects = node.style["layout_margins_minimum"]?.number != 1
        if viewRespectsSystemMinimumLayoutMargins != respects { viewRespectsSystemMinimumLayoutMargins = respects }
    }
}
#endif
