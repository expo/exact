#if os(iOS)
import UIKit

/// The height of the list column's navigation bar with its title inline, while the split it is in is expanded,
/// else 0: the least height of the page's range bar, so that once both columns are scrolled and the page's own bar
/// has hidden, the range bar and the list's bar end on one line (the open Duo, iOS 27.1). The split is Exact's
/// (LLP 1127), found from a route's controller; nothing here sets its delegate or columns.
///
/// It holds whether or not the list shows, so hiding the list, showing it or dragging it in moves nothing in the
/// page's header. It is measured only while the list's bar is in a window, when the module is told of a route or of
/// a window change, since nothing tells a module that the list showed: a list hidden since launch is measured as
/// the first section is chosen from it, and a size change while the list is hidden takes effect at the next route.
final class SidebarBarHeight {
    /// What the `sidebarBarHeight` answer watches: said whenever `height` changes
    static let topic = "gallery-sidebar-bar"

    private(set) var height: CGFloat = 0
    private weak var split: UISplitViewController?
    /// The inline bar height, and the bar width and vertical size class it was measured at
    private var inline: CGFloat = 0
    private var measuredAt: [CGFloat]?
    private var expansions: NSObjectProtocol?

    /// Called on the main thread when `height` changes, but for a change `update` returns to its caller
    var changed: () -> Void = {}

    init() {
        // UIKit posts this as a split collapses or expands, which need not lay any route out again
        expansions = NotificationCenter.default.addObserver(forName: UIViewController.showDetailTargetDidChangeNotification,
                                                            object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { if self?.update() == true { self?.changed() } }
        }
    }

    deinit { expansions.map(NotificationCenter.default.removeObserver) }

    /// Follows the split `controller` is in, if any.
    func follow(_ controller: UIViewController) {
        guard let found = controller.splitViewController, found !== split else { return }
        split = found
        measuredAt = nil
    }

    /// Measures again where the list's bar has a new width or vertical size class; true when `height` changed.
    @discardableResult
    func update() -> Bool {
        guard let split else { return false }
        if !split.isCollapsed, let bar = listBar(split) {
            // Measure only in a window, where the bar has the height it shows (laid out apart, it reports UIKit's
            // bare 44 points), and again only at a new width or vertical size class, since measuring sets the
            // shown item's mode twice. The text size does not change the inline bar's height (the Duo, iOS 27.1).
            let at = [bar.bounds.width, CGFloat(split.traitCollection.verticalSizeClass.rawValue)]
            if bar.window != nil, bar.bounds.width > 0, at != measuredAt, let measured = inlineHeight(of: bar) {
                measuredAt = at
                inline = measured
            }
        }
        let now = split.isCollapsed ? 0 : inline
        guard now != height else { return false }
        height = now
        return true
    }

    /// The navigation bar over the split's primary column.
    private func listBar(_ split: UISplitViewController) -> UINavigationBar? {
        let primary = split.viewController(for: .primary)
        return ((primary as? UINavigationController) ?? primary?.navigationController)?.navigationBar
    }

    /// What the bar says it fits while its top item asks for no large title, the item's mode set back at once
    /// without animation; with the large title showing, the bar reports the large title's height instead.
    private func inlineHeight(of bar: UINavigationBar) -> CGFloat? {
        guard let item = bar.topItem else { return nil }
        let mode = item.largeTitleDisplayMode
        var height: CGFloat = 0
        UIView.performWithoutAnimation {
            item.largeTitleDisplayMode = .never
            height = bar.sizeThatFits(CGSize(width: bar.bounds.width, height: 0)).height
            item.largeTitleDisplayMode = mode
        }
        return height
    }
}
#endif
