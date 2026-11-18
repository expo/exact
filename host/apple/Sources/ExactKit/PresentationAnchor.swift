// The view a system sheet is presented from: the share sheet, the
// authentication session's window.
#if canImport(UIKit)
import UIKit
package typealias PresentationAnchorView = UIView
#else
import AppKit
package typealias PresentationAnchorView = NSView
#endif

extension Presenter {
    /// Where a sheet the page asks for is presented from (LLP 1069.003 D3):
    /// the pressed node's view; a menu row's popover invoker when the
    /// row itself is not on screen (a menu item); else the window's content,
    /// centred (a timer, an answer, a menu-bar command).
    package func presentationAnchor(_ source: UInt32?) -> (PresentationAnchorView?, CGRect) {
        func shown(_ v: PresentationAnchorView) -> Bool {
            var at: PresentationAnchorView? = v
            while let a = at { if a.isHidden { return false }; at = a.superview }
            return v.window != nil
        }
        if let id = source, let v = views[id] {
            if shown(v) { return (v, v.bounds) }
            var at: PresentationAnchorView? = v.superview
            while let a = at, (a as? NodeView)?.props["popover"] == nil { at = a.superview }
            if let popover = at as? NodeView,
               let invoker = (carrying("popovertarget") + carrying("commandfor")).first(where: { invocation(of: $0)?.target === popover && shown($0) }) {
                return (invoker, invoker.bounds)
            }
        }
        #if canImport(UIKit)
        let content = ViewPresentation.presenter(for: root)?.viewIfLoaded
        #else
        let content = views.values.first(where: { $0.window != nil })?.window?.contentView
        #endif
        guard let content else { return (nil, .zero) }
        return (content, CGRect(x: content.bounds.midX, y: content.bounds.midY, width: 0, height: 0))
    }
}
