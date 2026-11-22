// A bar item's drawn face: a badge (`BadgeFace`, LLP 1075.003 §9.6) is the
// item's image in its own colours, baked again when the bar's tint dims. Any
// other face UIKit's item cannot draw is hosted, drawn as written (LLP
// 1141.004 D1), so no other face is painted here.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// UIKit keeps a bar item's original bitmap unchanged when its tint dims. This view hears the
/// bar's effective tint changes so only faces containing tint ink are baked again.
final class BarFaceTintView: UIView {
    var changed: (() -> Void)?
    override func tintColorDidChange() { super.tintColorDidChange(); changed?() }
}

extension NavigationHost {
    func retintBarFaces(in nav: UINavigationController) {
        let dimmed = nav.navigationBar.tintAdjustmentMode == .dimmed
        for route in nav.viewControllers.compactMap({ $0 as? RouteController }) {
            for made in route.barItemsMade.values {
                if let face = made.item.badge, face.shares.contains(where: { $0 != nil }) {
                    made.bar.image = face.image(dimmed: dimmed)
                }
            }
        }
    }
    /// Whether two item lists are the same items, in order.
    static func same(_ a: [UIBarButtonItem]?, _ b: [UIBarButtonItem]?) -> Bool {
        (a ?? []).elementsEqual(b ?? []) { $0 === $1 }
    }

    /// The item a header's control is drawn as in a bar that shows, if it is one.
    func shownBarItem(_ id: UInt32) -> UIBarButtonItem? {
        controllers.values.lazy.filter { $0.viewIfLoaded?.window != nil }.compactMap { $0.barItemsMade[id]?.bar }.first
    }
}
#endif
