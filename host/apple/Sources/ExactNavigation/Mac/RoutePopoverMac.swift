// @ref LLP 1123 D7 — a route presented as a popover is an `NSPopover`. A route over the route beneath it with
// `navigationPresentation="popover"` is, in a hand-built Mac app, AppKit's popover: a window of its own beside the
// element its `navigationPopoverAnchor` names, with AppKit's arrow, material and shadow. The route's subtree moves into
// the popover's content, laid out in a block that is a maximum on each axis, where it is a content owner (D2), and the
// popover takes the size that comes back.
//
// **Its choices.** `navigationFullSizeContent="true"` runs its content under the arrow, the arrow's area its safe
// area's (`hasFullSizeContent`). `navigationPopoverArrows` names the side of the anchor it lies on, opposite the first
// direction its arrow may point (an arrow `up` lies below), and below it where unsaid; AppKit moves it to another side
// where that one has no room. Margins, overlap of the anchor and adaptation are UIKit's and are not read here.
//
// **What dismisses it** is the route's `closedby`, as a sheet's: unwritten or `closerequest`, AppKit's transient
// popover, which closes on Escape, on a click elsewhere and when another window becomes key; `none`, only the app. Its
// close dismisses the route as a sheet's Escape does, through the stack's way back (BackMac.swift `dismiss`), so the app
// decides, and the popover leaves when the route is no longer shown. A popover route with no anchor in the page's window, one AppKit does not show, and one under another
// presentation is the window sheet (RouteSheetMac.swift).
#if os(macOS)
import AppKit
import ExactKit

/// One route popover as AppKit shows it: the `NSPopover`, the owner's container that holds the route's subtree, and
/// the route's box in the page, for its return.
final class RoutePopover: NSObject, NSPopoverDelegate {
    let popover = NSPopover()
    let content = BlockView(frame: .zero)
    weak var route: NodeView?, base: NodeView?, presenter: Presenter?
    let home: NSRect
    /// The host closed it: its close takes the stack no further back.
    var closing = false
    private var finished = false

    init(base: NodeView, route: NodeView, presenter: Presenter) {
        self.base = base; self.route = route; self.presenter = presenter; home = route.frame
        super.init()
        let controller = NSViewController()
        content.frame = NSRect(origin: .zero, size: route.frame.size)
        controller.view = content
        popover.contentViewController = controller
        popover.contentSize = route.frame.size
        popover.behavior = base.props["closedby"] == "none" ? .applicationDefined : .transient
        // Its content under the arrow too, the arrow's area its safe area's (LLP 1123 D7)
        popover.hasFullSizeContent = base.props["navigationFullSizeContent"] == "true"
        popover.animates = !ExactEnv.agentFreezes
        popover.delegate = self
    }

    /// The content's size, as layout returned it.
    func fit(_ size: CGSize) {
        guard size.width > 0, size.height > 0, popover.contentSize != size else { return }
        popover.contentSize = size
    }

    /// AppKit has taken the popover away: on its own close, the route is dismissed as a sheet's Escape dismisses it, its
    /// way back pressed where the route still is, and the subtree goes home.
    func popoverDidClose(_ notification: Notification) {
        if !closing, let base { presenter?.navigationHost.dismiss(base) }
        finish()
    }

    /// The subtree goes back under its authored parent, where the kernel lays it out again. Once.
    func finish() {
        guard !finished else { return }
        finished = true
        guard let presenter, let route, presenter.views[route.id] === route else { return }
        route.frame = home
        presenter.blocks.release(route)
    }

    /// The element `base`'s `navigationPopoverAnchor` names, in the page's window and outside the popover: of several
    /// with that `id`, the first made.
    static func anchor(of base: NodeView, in presenter: Presenter) -> NodeView? {
        guard let name = base.props["navigationPopoverAnchor"], !name.isEmpty, let window = presenter.viewport.window else { return nil }
        return presenter.views.values.filter { $0.props["id"] == name && $0.window === window && !$0.isDescendant(of: base) && !$0.isHiddenOrHasHiddenAncestor }
            .min { $0.id < $1.id }
    }

    /// The side of the anchor, in its flipped coordinates, that `navigationPopoverArrows`' first direction puts the
    /// popover on: below for `up` or unsaid, above for `down`, right of it for `left`, left of it for `right`.
    static func edge(_ words: String?) -> NSRectEdge {
        switch words?.split(separator: " ").first {
        case "down": .minY
        case "left": .maxX
        case "right": .minX
        default: .maxY
        }
    }

    /// What AppKit leaves around a popover's content, a side (`MenuHost.popoverMargin`).
    static let margin: CGFloat = 13
}

extension NavigationHost {
    /// Of the presentations `sheetRoutes` found, the top one when its base is a popover route whose anchor is in the
    /// page's window: an `NSPopover`, not a sheet.
    func popoverRoute(of sheeted: [(base: NodeView, route: NodeView, header: SheetHeader?)]) -> (base: NodeView, route: NodeView)? {
        guard let top = sheeted.last, top.base.props["navigationPresentation"] == "popover",
              RoutePopover.anchor(of: top.base, in: presenter) != nil else { return nil }
        return (top.base, top.route)
    }

    /// After a batch, never inside one: the popover route navigation shows is in its `NSPopover`, and one it no longer
    /// shows, or whose anchor has gone, leaves.
    func syncPopover() {
        if let current = routePopover, current.route !== poppedRoute?.route || current.base !== poppedRoute?.base
            || !current.popover.isShown || current.base.map({ RoutePopover.anchor(of: $0, in: presenter) == nil }) ?? true {
            current.closing = true
            routePopover = nil
            if current.popover.isShown { current.popover.close() } else { current.finish() }
        }
        guard routePopover == nil, let wanted = poppedRoute, let anchor = RoutePopover.anchor(of: wanted.base, in: presenter),
              let window = presenter.viewport.window else { return }
        let shown = RoutePopover(base: wanted.base, route: wanted.route, presenter: presenter)
        let screen = (window.screen ?? NSScreen.main)?.visibleFrame.size ?? window.frame.size
        let maximum = CGSize(width: max(1, screen.width - 2 * RoutePopover.margin), height: max(1, screen.height - 2 * RoutePopover.margin))
        let block = HostBlock(width: .atMost(maximum.width), height: .atMost(maximum.height))
        presenter.blocks.show(wanted.route, in: shown.content, block: block) { [weak shown] in shown?.fit($0) }
        wanted.route.isHidden = false
        shown.popover.show(relativeTo: anchor.bounds, of: anchor, preferredEdge: RoutePopover.edge(wanted.base.props["navigationPopoverArrows"]))
        guard shown.popover.isShown else {
            // AppKit showed none (its anchor is scrolled out of sight): the route is the window sheet
            shown.closing = true
            shown.finish()
            poppedRoute = nil
            return resync()
        }
        routePopover = shown
        shown.popover.contentViewController?.view.window?.autorecalculatesKeyViewLoop = false
        presenter.syncKeyViewLoop()
    }
}
#endif
