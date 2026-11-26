// One content owner's update (LLP 1123 D2) as AppKit gives it at one pass of its container's layout: the container
// view's own `safeAreaInsets`, Exact's contributions to that view (`ContributionsMac`), the view's map into its
// window and its configuration's identity, in the form both Apple hosts encode (`OwnerUpdateWire`). AppKit has no
// software keyboard, reserved regions, segments or bar regions, so those stay unreported; the owner's ancestry,
// targets and margins are its own steps.
#if os(macOS)
import AppKit

enum OwnerUpdateMac {
    /// The update of the owner whose container is `view`. Exact's writes to the view are applied first, so the band
    /// and their effects describe the same layout (D11). Out of a window the view has no band, map or configuration.
    static func sample(_ view: NSView) -> OwnerUpdateWire {
        var update = OwnerUpdateWire()
        if let ledger = ContributionsMac.existing(on: view) {
            ledger.apply()
            update.contributions = ledger.entries.map {
                .init(node: $0.identity.node, operation: $0.identity.operation.rawValue, effect: edges($0.insets))
            }
        }
        guard let window = view.window else { return update }
        update.band = edges(view.safeAreaInsets)
        update.windowMap = map(of: view, in: window)
        update.configuration = configuration(of: window)
        return update
    }

    static func edges(_ i: NSEdgeInsets) -> OwnerUpdateWire.Edges { .init(top: i.top, right: i.right, bottom: i.bottom, left: i.left) }

    /// The view's map from its bounds' origin into its window's space, D6's `"window"` space on AppKit: the window's
    /// frame with its origin at the top left and y down, as UIKit's window is, through AppKit's own conversion.
    static func map(of view: NSView, in window: NSWindow) -> CGAffineTransform {
        let height = window.frame.height, origin = view.bounds.origin
        let point = { (p: CGPoint) -> CGPoint in
            let w = view.convert(p, to: nil)
            return CGPoint(x: w.x, y: height - w.y)
        }
        let at = point(origin), right = point(CGPoint(x: origin.x + 1, y: origin.y)), below = point(CGPoint(x: origin.x, y: origin.y + 1))
        return CGAffineTransform(a: right.x - at.x, b: right.y - at.y, c: below.x - at.x, d: below.y - at.y, tx: at.x, ty: at.y)
    }

    /// The identity of the window's configuration (D9, "Moving edges"): whether it is full screen, its toolbar shows,
    /// its title shows, its title bar is transparent and its content spans the title bar, and its size, which the
    /// largest band an `observed-max` reader settles against is kept for.
    static func configuration(of window: NSWindow) -> UInt64 {
        var hasher = Hasher()
        hasher.combine(window.styleMask.contains(.fullScreen))
        hasher.combine(window.styleMask.contains(.fullSizeContentView))
        hasher.combine(window.toolbar?.isVisible)
        hasher.combine(window.titleVisibility.rawValue)
        hasher.combine(window.titlebarAppearsTransparent)
        hasher.combine(window.frame.width)
        hasher.combine(window.frame.height)
        return UInt64(bitPattern: Int64(hasher.finalize()))
    }
}
#endif
