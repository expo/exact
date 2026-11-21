#if os(iOS)
import UIKit

/// The height of the list column's navigation bar with its title inline, while the split it is in is expanded,
/// else 0: the least height of the page's range bar, so that once both columns are scrolled and the page's own bar
/// has hidden, the range bar and the list's bar end on one line (the open Duo, iOS 27.1). The split is Exact's
/// (LLP 1127), found from the gallery's list, which the Contract hatches as `sidebar-list` on iOS; nothing here
/// sets the split's delegate or columns.
///
/// A check is scheduled, for the main queue's next turn, when the list or the range bar changes size, enters or
/// leaves a window or changes size class, when the window changes, and when UIKit says the split collapses or
/// expands. A check measures the bar afresh while it is in a window, beside the page or over it, with the list
/// route's item on top; one made during a transition of the split or the list's column, collapsed or expanded, also
/// registers one more check for the transition's end. The height holds while the split
/// stays expanded, whether the list shows or not, so hiding the list, showing it or dragging it in moves nothing in
/// the page's header. A collapse or a new split forgets it: a list that has not shown since launch, or since the
/// split last expanded, is measured when it first shows, which raises the range bar then.
final class SidebarBarHeight {
    /// What the `sidebarBarHeight` answer watches: said whenever `height` changes
    static let topic = "gallery-sidebar-bar"

    private(set) var height: CGFloat = 0
    /// Called on the main thread when `height` changes
    var changed: () -> Void = {}

    /// The list's view, whose controller's split this follows
    private weak var list: UIView?
    private weak var split: UISplitViewController?
    /// The last height measured on `split` while it was expanded
    private var inline: CGFloat = 0
    private var scheduled = false
    /// The transition whose end a check is registered for
    private weak var awaited: (any UIViewControllerTransitionCoordinator)?
    private var expansions: NSObjectProtocol?

    init() {
        // UIKit posts this as a split collapses or expands, which need not lay the list out again
        expansions = NotificationCenter.default.addObserver(forName: UIViewController.showDetailTargetDidChangeNotification,
                                                            object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.scheduleUpdate() }
        }
    }

    deinit { expansions.map(NotificationCenter.default.removeObserver) }

    /// Follows the split of the controller that `list`, the gallery's list, is in.
    func follow(list view: UIView) {
        list = view
        scheduleUpdate()
    }

    /// Forgets `view`, a list that has left: the height is 0 until another list is followed.
    func forget(list view: UIView) {
        guard list === view else { return }
        list = nil
        scheduleUpdate()
    }

    /// Checks once the current layout pass has finished, on the main queue's next turn; calls made before then share
    /// that one check, and a call made while it runs schedules another.
    func scheduleUpdate() {
        guard !scheduled else { return }
        scheduled = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            scheduled = false
            update()
        }
    }

    private func update() {
        let found = list.flatMap(Self.controller(of:))?.splitViewController
        if found !== split {
            split = found
            inline = 0
        }
        var now: CGFloat = 0
        if let split, split.view.window != nil {
            // Check once more when a transition in flight ends, in case it leaves the bar another height, or expands
            // a split that UIKit still reports collapsed until it has
            if let transition = split.transitionCoordinator ?? split.viewController(for: .primary)?.transitionCoordinator,
               transition !== awaited,
               transition.animate(alongsideTransition: nil, completion: { [weak self] _ in self?.scheduleUpdate() }) {
                awaited = transition
            }
            if split.isCollapsed {
                // Measure afresh once it expands, which can change the bar without changing its width
                inline = 0
            } else {
                if case let (navigation, item)? = listBar(split), navigation.navigationBar.window != nil {
                    // Measure only in a window, where the bar has the height it shows (laid out apart, it reports
                    // UIKit's bare 44 points), once the column has laid it out at its current width
                    navigation.view.layoutIfNeeded()
                    let bar = navigation.navigationBar
                    if bar.window != nil, bar.bounds.width > 0 {
                        let measured = inlineHeight(of: bar, item: item)
                        if measured.isFinite, measured > 0 { inline = measured }
                    }
                }
                now = inline
            }
        }
        guard now != height else { return }
        height = now
        changed()
    }

    /// The navigation controller over the split's primary column and the list route's item, while that item is the
    /// top of the controller's bar.
    private func listBar(_ split: UISplitViewController) -> (UINavigationController, UINavigationItem)? {
        let primary = split.viewController(for: .primary)
        guard let navigation = (primary as? UINavigationController) ?? primary?.navigationController,
              let list = navigation.viewControllers.first, navigation.navigationBar.topItem === list.navigationItem else { return nil }
        return (navigation, list.navigationItem)
    }

    /// What the bar says it fits while the list's item asks for no large title, the item's mode set back at once
    /// without animation; with the large title showing, the bar reports the large title's height instead.
    private func inlineHeight(of bar: UINavigationBar, item: UINavigationItem) -> CGFloat {
        let mode = item.largeTitleDisplayMode
        var height: CGFloat = 0
        UIView.performWithoutAnimation {
            item.largeTitleDisplayMode = .never
            height = bar.sizeThatFits(CGSize(width: bar.bounds.width, height: 0)).height
            item.largeTitleDisplayMode = mode
        }
        return height
    }

    /// The controller whose view holds `view`, found up the responder chain.
    private static func controller(of view: UIView) -> UIViewController? {
        var next = view.next
        while let responder = next {
            if let controller = responder as? UIViewController { return controller }
            next = responder.next
        }
        return nil
    }
}

/// An empty view across a hatched view that says when its size or size classes change or it enters or leaves a
/// window, and not when it is laid out at the same size or its safe area changes, as a column's scrolling does. Across the list, it hears
/// the list show, beside the page or over it, and the list's column change width; across the page's range bar, it
/// hears the split beside the page collapse or expand.
final class SidebarBarWatch: UIView {
    var onChange: () -> Void = {}
    private var size: CGSize?

    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        registerForTraitChanges([UITraitVerticalSizeClass.self, UITraitHorizontalSizeClass.self]) { (watch: SidebarBarWatch, _) in
            watch.onChange()
        }
    }

    required init?(coder: NSCoder) { nil }

    override func layoutSubviews() {
        super.layoutSubviews()
        guard bounds.size != size else { return }
        size = bounds.size
        onChange()
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        onChange()
    }
}
#endif
