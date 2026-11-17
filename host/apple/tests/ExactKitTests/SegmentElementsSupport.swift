#if os(iOS)
import UIKit

extension NSObject {
    /// An accessibility element's identifier: a view's or an element's (`UISegmentedControl.identify`).
    var readIdentifier: String? { (self as? UIView)?.accessibilityIdentifier ?? (self as? UIAccessibilityElement)?.accessibilityIdentifier }
}
#endif
