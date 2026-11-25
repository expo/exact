// @ref LLP 1021 D3 — a bar item's popover on iOS: a menu (or a
// confirmation) is the item's pull-down; any other popover opens as itself,
// lifted and anchored on the item.
#if os(iOS) || os(tvOS)
import UIKit

extension MenuHost {
    /// Whether a bar item opening `target` is its pull-down (LLP 1021 D3):
    /// a menu-shaped popover, or a confirmation (which its rows present).
    package func pullsDown(_ target: NodeView) -> Bool { target.menuShaped || isConfirmation(target) }
    #if os(iOS)
    /// Whether `source` opens `pop` as UIKit's menu when the agent does not
    /// paint it (`sync`): a native button in the page its `role="menu"`
    /// popover, and any other invoker, a bar item included, its menu-shaped
    /// one, as its pull-down.
    func platformMenu(_ pop: NodeView, from source: NodeView) -> Bool {
        guard let presenter, !isConfirmation(pop), presenter.invocation(of: source)?.target === pop else { return false }
        let inBar: Bool
        if case .success? = presenter.navigation.barItem(source) { inBar = true } else { inBar = false }
        return source.isNativeButton && !inBar ? pop.props["accessibilityRole"] == "menu" : pop.menuShaped
    }
    #endif
    /// UIKit's bar item says nothing of its menu's life (`UIBarButtonItem`
    /// has a `menu` and no delegate), so a bar pull-down's `toggle` follows
    /// what public API shows (LLP 1021, "A bar item's pull-down";
    /// expo/exact#1462). It opens as UIKit reads its rows. It closes at a
    /// pick, at a touch on the app's views (`lightDismiss`), at
    /// `hidePopover` or another auto popover's showing (`agentShow`), when
    /// its button stops opening it from the bar (`endLeftBarMenu`), and when
    /// the next pull-down opens, which is where a close by Escape or
    /// VoiceOver's escape gesture is reported: nothing public reports those.
    /// A close seen here is reported as UIKit begins to dismiss the menu.
    ///
    /// `afterClose`: a close was reported in this turn, which the open
    /// follows on the next, so the two are not coalesced into one event.
    func barMenuOpened(_ pop: NodeView, from invoker: NodeView, in bar: @escaping () -> UIView?, afterClose: Bool) {
        let menu = BarMenu(popover: pop, invoker: invoker, bar: bar)
        barMenu = menu
        guard afterClose else { presenter?.popoverEvents.changed(pop, open: true); return }
        DispatchQueue.main.async { [weak self, weak menu] in
            guard let self, let menu, self.barMenu === menu, let pop = menu.popover else { return }
            self.presenter?.popoverEvents.changed(pop, open: true)
        }
    }
    /// `dismissing`: the app closed it (`hidePopover`, another popover
    /// shown, its button gone), so UIKit's menu is dismissed here, through
    /// the interactions of the views its item may be in (`dismissMenus`).
    /// False when no pull-down was open.
    @discardableResult func barMenuClosed(dismissing: Bool = false) -> Bool {
        guard let menu = barMenu else { return false }
        barMenu = nil
        if recordsGiven { presentedMenu = [:] }
        if dismissing, let bar = menu.bar() { Self.dismissMenus(of: bar) }
        if let pop = menu.popover, live(pop), pop.incarnation == menu.incarnation { presenter?.popoverEvents.changed(pop, open: false) }
        return true
    }
    /// After a batch: a pull-down whose button the bar no longer shows as
    /// an item with a menu, or that no longer opens it, is closed.
    func endLeftBarMenu() {
        guard let menu = barMenu else { return }
        guard let presenter, let invoker = menu.invoker, live(invoker), eligible(invoker, inBar: true),
              presenter.navigation.shownBarItem(invoker.id)?.menu != nil, let pop = menu.popover,
              let invocation = presenter.invocation(of: invoker), invocation.opens, invocation.target === pop else { barMenuClosed(dismissing: true); return }
    }
    /// UIKit can draw a bar's items beside it, in the view of its controller
    /// or of the tab bar controller that holds it, as on a closed iPhone Duo
    /// (iOS 27.1): any view there that does not hold the page.
    /// `UIContextMenuInteraction.dismissMenu()` dismisses only a menu its
    /// interaction presents.
    private static func dismissMenus(of bar: UIView) {
        let controller = (bar as? UINavigationBar)?.delegate as? UINavigationController, page = controller?.topViewController?.viewIfLoaded
        let beside = [controller, controller?.tabBarController].compactMap { $0?.viewIfLoaded }.flatMap(\.subviews)
        for view in [bar] + beside where page?.isDescendant(of: view) != true { dismissMenus(in: view) }
    }
    private static func dismissMenus(in view: UIView) {
        for case let interaction as UIContextMenuInteraction in view.interactions { interaction.dismissMenu() }
        for sub in view.subviews { dismissMenus(in: sub) }
    }
    /// A bar item's popover that is no menu, opened (or closed, a second
    /// tap) from the item as a native button in the page opens one: lifted,
    /// anchored on the item (`lift`). False when the item is ineligible or
    /// names nothing it opens now.
    package func openFromBar(_ node: NodeView) -> Bool {
        guard let presenter, eligible(node, inBar: true), let invocation = presenter.invocation(of: node), invocation.opens,
              !isConfirmation(invocation.target), let name = invocation.target.props["id"] else { return false }
        // Open before the tap's light dismiss, which would close it first.
        let open = isOpen(invocation.target)
        if open, invocation.action == .show { return true }
        lightDismiss(node)
        if open { drop(name) } else { agentShow(invocation.target, named: name, from: node.id, invoked: true) }
        return true
    }
    /// Under the agent's painted popovers and UIKit's bars (`--chrome
    /// platform`), the tap of a bar item whose button opens a menu: what a
    /// finger opens is UIKit's pull-down, which no public API opens, so its
    /// popover opens painted under the bar, as a content invoker's does, and
    /// `tap <row>` presses a row as the pull-down's action does (LLP
    /// 1116.020 D6). A confirmation it names presents as the pull-down's
    /// provider presents it. Nil when the agent shows UIKit's menus instead
    /// (`--touch platform`: a real touch opens them); false when it opens
    /// nothing now.
    package func agentOpen(barItem node: NodeView) -> Bool? {
        guard ExactEnv.agentPaints, let presenter else { return nil }
        guard eligible(node, inBar: true), let invocation = presenter.invocation(of: node), invocation.opens else { return false }
        let target = invocation.target
        if isConfirmation(target) {
            return openConfirmation(from: nil, popover: target, modal: invocation.action == .showModal, dispatchPress: false)
        }
        guard let name = target.props["id"] else { return false }
        lightDismiss(node)
        if !isOpen(target) { agentShow(target, named: name, from: node.id, invoked: true) } else { drop(name) }
        return true
    }
}

/// The popover a bar item's pull-down shows while UIKit shows it, the bar
/// button that opened it, and the bar it is in.
final class BarMenu {
    weak var popover: NodeView?
    let incarnation: UInt64
    weak var invoker: NodeView?
    let bar: () -> UIView?
    init(popover: NodeView, invoker: NodeView, bar: @escaping () -> UIView?) {
        self.popover = popover; incarnation = popover.incarnation; self.invoker = invoker; self.bar = bar
    }
}
#endif
