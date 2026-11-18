// @ref LLP 1047.001 D10 — a row's swipe actions and a list's native row
// cells are a family the composition selects (the roster's `Collections`,
// which a standalone swipe row and a list of cells select too). The core
// reaches it only through what this file declares. The presenter always
// holds an object: without the family it holds `NoSwipeActions` and
// `NoRowCells`, which do nothing and own nothing, so a plan with no list pays
// one call per hook. The family's target, `ExactCollections`, installs the
// hosts. Grouped lists (`ExactGroupedLists`) read swipe configurations
// through the same protocol.
#if os(iOS) || os(tvOS)
import UIKit

/// What the core asks of a presenter's swipe actions.
package protocol SwipeActionsHosting: AnyObject {
    /// Whether an assistive technology is running, where actions are offered without a swipe.
    var assistive: Bool { get }
    /// Whether `id` is an action a swipe shows.
    func ownsAction(_ id: UInt32) -> Bool
    /// A finger came down on `owner`'s row.
    func touch(_ owner: NodeView)
    /// The plan restarted.
    func reset()
    /// Before a batch: every opened row as the batch will find it.
    func prepare(_ batch: Batch?)
    /// After a batch; `changed` and `reshaped` are the touched views and their ancestors.
    func sync(changed: Set<UInt32>?, reshaped: Set<UInt32>?)
    #if os(iOS)
    /// A painted swipe row's scroll began, moved, or will end at `target`.
    func paintedBegan(_ owner: NodeView)
    func paintedScrolled(_ owner: NodeView)
    func paintedWillEnd(_ owner: NodeView, _ scroll: UIScrollView, target: UnsafeMutablePointer<CGPoint>)
    /// A row is opening.
    func opening(_ id: UInt32)
    /// `scroll` scrolls: rows it moves close.
    func scrolling(_ scroll: UIScrollView)
    /// A grouped list row's swipe configuration.
    func configuration(row id: UInt32, leading: Bool) -> UISwipeActionsConfiguration?
    #endif
    #if EXACT_INSPECT
    /// The action `button` stands for, if it is one.
    func action(for button: UIButton) -> UInt32?
    #endif
    /// The button that shows action `id`.
    func actionView(_ id: UInt32) -> UIButton?
    /// Perform action `id` as a person would; nil when no row owns it.
    func perform(_ id: UInt32, before: () -> Bool) -> Bool?
    /// The inspection walk's accounting (LLP 1080.001 D3).
    func inspectionOwns(_ view: UIView) -> Bool
    func hides(_ node: NodeView) -> Bool
    func projects(_ view: UIView) -> Bool
}

/// What the core asks of a presenter's native row cells.
package protocol RowCellsHosting: AnyObject {
    /// A press on `node`; true when a row cell shows it.
    func press(_ node: NodeView, _ down: Bool) -> Bool
    /// A tap landed on `node`: its row's cell, selected before the press runs.
    func select(_ node: NodeView) -> NodeView?
    /// The press ran: `row` stays selected under a screen it pushed over `screen`.
    func ran(_ row: NodeView, on screen: UIViewController?)
    /// The screen `view` is on.
    func screen(of view: UIView) -> UIViewController?
    /// After a batch; `changed` is the touched views and their ancestors.
    func sync(changed: Set<UInt32>?)
    /// `route`'s screen is about to show again.
    func appearing(_ route: NodeView, alongside coordinator: UIViewControllerTransitionCoordinator?)
    /// The plan restarted.
    func reset()
    /// Whether `view` is a list's separators or a row's pressed ground (the inspection walk's accounting).
    func inspectionOwns(_ view: UIView) -> Bool
}

/// No swipe actions: nothing is owned, nothing happens.
final class NoSwipeActions: SwipeActionsHosting {
    var assistive: Bool { false }
    func ownsAction(_ id: UInt32) -> Bool { false }
    func touch(_ owner: NodeView) {}
    func reset() {}
    func prepare(_ batch: Batch?) {}
    func sync(changed: Set<UInt32>?, reshaped: Set<UInt32>?) {}
    #if os(iOS)
    func paintedBegan(_ owner: NodeView) {}
    func paintedScrolled(_ owner: NodeView) {}
    func paintedWillEnd(_ owner: NodeView, _ scroll: UIScrollView, target: UnsafeMutablePointer<CGPoint>) {}
    func opening(_ id: UInt32) {}
    func scrolling(_ scroll: UIScrollView) {}
    func configuration(row id: UInt32, leading: Bool) -> UISwipeActionsConfiguration? { nil }
    #endif
    #if EXACT_INSPECT
    func action(for button: UIButton) -> UInt32? { nil }
    #endif
    func actionView(_ id: UInt32) -> UIButton? { nil }
    func perform(_ id: UInt32, before: () -> Bool) -> Bool? { nil }
    func inspectionOwns(_ view: UIView) -> Bool { false }
    func hides(_ node: NodeView) -> Bool { false }
    func projects(_ view: UIView) -> Bool { false }
}

/// No row cells: no press is a cell's.
final class NoRowCells: RowCellsHosting {
    func press(_ node: NodeView, _ down: Bool) -> Bool { false }
    func select(_ node: NodeView) -> NodeView? { nil }
    func ran(_ row: NodeView, on screen: UIViewController?) {}
    func screen(of view: UIView) -> UIViewController? { nil }
    func sync(changed: Set<UInt32>?) {}
    func appearing(_ route: NodeView, alongside coordinator: UIViewControllerTransitionCoordinator?) {}
    func reset() {}
    func inspectionOwns(_ view: UIView) -> Bool { false }
}

/// What the family asks of the presenter's navigation.
extension Presenter {
    /// Whether `view` is on a route a pushed screen covers.
    package func isInactiveRoute(containing view: UIView) -> Bool { navigation.isInactiveRoute(containing: view) }
}

extension UIView {
    /// Whether this is a native button, whose touch a scroll may take.
    package var isNativeButtonView: Bool { self is NativeButtonIOS }
}

/// The slots are set before any session is made, and read on the main thread.
package enum SwipeActionsLink {
    /// A presenter's swipe actions.
    package static var make: ((Presenter) -> any SwipeActionsHosting)?
    /// A presenter's native row cells.
    package static var makeRowCells: ((Presenter) -> any RowCellsHosting)?
}
#endif
