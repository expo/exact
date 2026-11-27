// One content owner's update (LLP 1123 D2) as AppKit gives it at one pass of its container's layout: the container
// view's own `safeAreaInsets`, Exact's contributions to that view (`ContributionsMac`), its containment ancestry and
// relational targets (D9), its margins (D8), its chrome's auxiliary areas (D6), the view's map into its window and its
// configuration's identity, in the form both Apple hosts encode (`OwnerUpdateWire`). AppKit has no software keyboard,
// reserved regions, segments or bar regions, so those stay unreported, nor a tab controller's content guide, a system
// minimum, a readable width, a vertical bar or a tab sidebar.
#if os(macOS)
import AppKit

enum OwnerUpdateMac {
    /// The kernel's `Kind::bit`s and `Relation::ALL` indices that AppKit has.
    enum Kind { static let tab: UInt8 = 1 << 3, split: UInt8 = 1 << 4, column: UInt8 = 1 << 5 }
    enum Relation { static let presenter: UInt8 = 0, session: UInt8 = 1, root: UInt8 = 2, window: UInt8 = 3, windowContent: UInt8 = 5, screen: UInt8 = 6 }

    /// The update of the owner whose container is `view`. Exact's writes to the view are applied first, so the band
    /// and their effects describe the same layout (D11). Out of a window the view has no band, margins, map or
    /// configuration.
    static func sample(_ view: NSView, session: NSView? = nil) -> OwnerUpdateWire {
        var update = OwnerUpdateWire()
        if let ledger = ContributionsMac.existing(on: view) {
            ledger.apply()
            update.contributions = ledger.entries.map {
                .init(node: $0.identity.node, operation: $0.identity.operation.rawValue, effect: edges($0.insets))
            }
        }
        update.chain = chain(of: view)
        guard let window = view.window else { return update }
        let related: [(UInt8, OwnerUpdateWire.Target?)] = [
            (Relation.presenter, window.sheetParent.map { target($0.contentView) }),
            (Relation.session, session.map { target($0) }),
            (Relation.root, target(window.contentViewController?.view ?? window.contentView)),
            (Relation.window, target(window.contentView)),
            (Relation.windowContent, content(of: window)),
            (Relation.screen, window.screen.map { screen($0, in: window) }),
        ]
        update.relations = related.compactMap { r, t in t.map { (relation: r, target: $0) } }
        update.band = edges(view.safeAreaInsets)
        update.margins = margins(of: view, less: update.contributions)
        update.chrome = chrome(of: view, in: window)
        update.windowMap = map(of: view, in: window)
        update.configuration = configuration(of: window)
        return update
    }

    static func edges(_ i: NSEdgeInsets) -> OwnerUpdateWire.Edges { .init(top: i.top, right: i.right, bottom: i.bottom, left: i.left) }

    /// The container's margins (D8) as AppKit gives them, less Exact's contributions as the band is (D11), which AppKit
    /// adds to them as to the safe area, a negative one included. AppKit sets no view's own margins, so a plain view's
    /// are the container's less its safe area. Each is taken from AppKit's signed distances and is at least 0 after.
    static func margins(of view: NSView, less contributions: [OwnerUpdateWire.Contribution]) -> OwnerUpdateWire.Margins {
        let m = LayoutRegionsMac.margins(of: view, signed: true), safe = view.safeAreaInsets
        let added = contributions.reduce(OwnerUpdateWire.Edges.zero) {
            .init(top: $0.top + $1.effect.top, right: $0.right + $1.effect.right, bottom: $0.bottom + $1.effect.bottom, left: $0.left + $1.effect.left)
        }
        let at0 = { (top: CGFloat, right: CGFloat, bottom: CGFloat, left: CGFloat) in
            OwnerUpdateWire.Edges(top: max(0, top), right: max(0, right), bottom: max(0, bottom), left: max(0, left))
        }
        return .init(root: at0(m[0] - added.top, m[1] - added.right, m[2] - added.bottom, m[3] - added.left),
                     plain: at0(m[0] - safe.top, m[1] - safe.right, m[2] - safe.bottom, m[3] - safe.left),
                     minimum: nil, readableWidth: nil)
    }

    /// The owner's chrome (D6): the unobscured areas of its window's screen beside the camera housing, top left and top
    /// right (`NSScreen.auxiliaryTopLeftArea`, `auxiliaryTopRightArea`), in the owner view's space, origin at its top
    /// left and y down, an empty rectangle where the screen has none there; none of UIKit's quantities. Off a screen it
    /// has none.
    static func chrome(of view: NSView, in window: NSWindow) -> OwnerUpdateWire.Chrome? {
        guard let screen = window.screen else { return nil }
        let place = { (area: NSRect?) -> CGRect in
            guard let area else { return .zero }
            let local = view.convert(window.convertFromScreen(area), from: nil)
            return view.isFlipped ? local : CGRect(x: local.minX, y: view.bounds.height - local.maxY, width: local.width, height: local.height)
        }
        return .init(verticalBarEdge: nil, sidebarAvailable: nil, auxiliaryTopLeft: place(screen.auxiliaryTopLeftArea),
                     auxiliaryTopRight: place(screen.auxiliaryTopRightArea))
    }

    /// The owner's containment ancestry (D9, "The native chain"): its container view first, then the controllers
    /// above it, as AppKit's `parent` gives them, each with the kinds it is. A split's item is a column.
    static func chain(of view: NSView) -> [(target: OwnerUpdateWire.Target, kinds: UInt8)] {
        let kinds = { (c: NSViewController) -> UInt8 in
            (c is NSTabViewController ? Kind.tab : 0) | (c is NSSplitViewController ? Kind.split : 0) | (c.parent is NSSplitViewController ? Kind.column : 0)
        }
        let own = controller(of: view)
        var chain = [(target(view), own.flatMap { $0.view === view ? kinds($0) : nil } ?? 0)]
        var next = own.flatMap { $0.view === view ? $0.parent : $0 }
        while let c = next {
            chain.append((target(c.viewIfLoaded), kinds(c)))
            next = c.parent
        }
        return chain
    }

    /// The controller whose view holds `view`, the nearest on its responder chain.
    private static func controller(of view: NSView) -> NSViewController? {
        var next: NSResponder? = view
        while let r = next, !(r is NSWindow) {
            if let c = r as? NSViewController { return c }
            next = r.nextResponder
        }
        return nil
    }

    /// `view` as a target (D9): its insets in its own coordinates and its frame in its window's space, none while it
    /// is out of a window.
    static func target(_ view: NSView?) -> OwnerUpdateWire.Target {
        guard let view else { return .init(id: 0, insets: nil, frame: nil) }
        guard let window = view.window else { return .init(id: TargetIds.id(of: view), insets: nil, frame: nil) }
        return .init(id: TargetIds.id(of: view), insets: edges(view.safeAreaInsets), frame: rect(view.convert(view.bounds, to: nil), in: window))
    }

    /// The window's content layout rectangle as a target (D9's `window-content`): the window's frame, inset to the part
    /// no title bar or toolbar covers.
    static func content(of window: NSWindow) -> OwnerUpdateWire.Target {
        let frame = CGRect(origin: .zero, size: window.frame.size), inner = window.contentLayoutRect
        return .init(id: TargetIds.id(of: window), insets: .init(top: frame.height - inner.maxY, right: frame.width - inner.maxX, bottom: inner.minY, left: inner.minX),
                     frame: frame)
    }

    /// The window's screen as a target (D9's `screen`): its safe area, beside the camera housing, and its frame in the
    /// window's space.
    static func screen(_ screen: NSScreen, in window: NSWindow) -> OwnerUpdateWire.Target {
        let s = screen.frame, w = window.frame
        return .init(id: TargetIds.id(of: screen), insets: edges(screen.safeAreaInsets),
                     frame: CGRect(x: s.minX - w.minX, y: w.maxY - s.maxY, width: s.width, height: s.height))
    }

    /// A rectangle in the window's base coordinates in its space: its origin at the top left, y down.
    static func rect(_ r: CGRect, in window: NSWindow) -> CGRect {
        CGRect(x: r.minX, y: window.frame.height - r.maxY, width: r.width, height: r.height)
    }

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
