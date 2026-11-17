#if os(iOS) || os(tvOS)
import UIKit

/// The window's first responder, found the way UIKit offers: an action sent
/// to nil reaches it.
enum FirstResponder {
    private static weak var found: UIResponder?
    static var current: UIResponder? {
        found = nil
        UIApplication.shared.sendAction(#selector(UIResponder.exactReportFirstResponder), to: nil, from: nil, for: nil)
        return found
    }
    fileprivate static func report(_ responder: UIResponder) { found = responder }

    /// Whether `responder` may be the input UIKit makes first responder while a
    /// menu shows, for type-to-select (LLP 1021). Exact tells it by its shape,
    /// not its class (LLP 1035.003): it takes key input and is no text input,
    /// view or view controller (measured on iOS 27.1). An app's own key input
    /// of that shape, focused by a menu's action, is taken for it.
    static func standsInForAMenu(_ responder: UIResponder) -> Bool {
        responder is UIKeyInput && !(responder is UITextInput || responder is UIView || responder is UIViewController)
    }
}

extension UIResponder {
    @objc fileprivate func exactReportFirstResponder() { FirstResponder.report(self) }
}
#endif
