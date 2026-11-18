// Exact's tab container and a container the app owns, on the module side of
// the native-module table (@ref LLP 1075.003 §3.6, §3.7): the `tabs` hook's
// handle, each tab a container the app owns shows, and what that container
// holds and sends to Exact. ExactNativeModule.swift hands them to the app's
// module from the host's calls.
#if os(iOS) || os(tvOS)
import UIKit

/// Exact's tab container (LLP 1075.003 §3.7).
public final class ExactTabs {
    /// The tab container. Its `delegate` is Exact's: give yours to `delegate`
    /// here, which Exact forwards to (LLP 1075.003 §3.5).
    public let controller: UITabBarController
    /// The app's tab delegate. Exact keeps the controller's own slot: a tab
    /// the bar would select presses its authored tab, after asking this
    /// delegate's `shouldSelect`; the rest is forwarded. Exact keeps it until
    /// you give another or nil, or the container goes. This property holds it
    /// weakly, as UIKit's does, so a delegate that keeps this handle makes no
    /// cycle.
    public weak var delegate: UITabBarControllerDelegate? {
        didSet {
            guard let hatches else { return }
            let object = delegate.map { Unmanaged.passUnretained($0 as AnyObject).toOpaque() }
            hatches.delegateFn(hatches.host, Unmanaged.passUnretained(controller).toOpaque(), object)
        }
    }
    weak var hatches: ExactHatches?
    init(controller: UITabBarController, hatches: ExactHatches) { self.controller = controller; self.hatches = hatches }
}

/// One tab, for a container the app owns: its name, the controller Exact
/// shows it in, and the item its authored tab gives.
public struct ExactTab {
    public let name: String
    /// The controller Exact shows the tab in, for the tab's life: a
    /// navigation controller for a tab that can push or show a header, a
    /// plain view controller for one that shows one fixed route with no
    /// header (LLP 1075.003 §3.7).
    public let controller: UIViewController
    /// The tab's item, which Exact updates in place as the authored tab changes.
    public var item: UITabBarItem { controller.tabBarItem }
}

/// What a container the app owns holds (LLP 1075.003 §3.6): each tab's
/// stack, which tab the router selects, and the way to select one.
public final class ExactTabContents {
    public let tabs: [ExactTab]
    /// The tab the router selects.
    public internal(set) var selected: Int
    /// The router selected another tab: show it.
    public var onSelect: ((Int) -> Void)?
    /// False once Exact retired the container: its intents then do nothing.
    public internal(set) var isLive = true
    var tabNodes: [UInt32]
    weak var hatches: ExactHatches?
    init(tabs: [ExactTab], selected: Int, tabNodes: [UInt32], hatches: ExactHatches) {
        self.tabs = tabs; self.selected = selected; self.tabNodes = tabNodes; self.hatches = hatches
    }
    /// The container selected a tab: Exact presses its authored tab, queued
    /// and on a later turn, and the router decides (its history, its pop to
    /// root on a second press).
    public func select(_ index: Int) {
        guard let hatches else { return }
        // The tab intent runs only while this container holds the tabs; a host without it has only the press
        let taken = isLive && tabNodes.indices.contains(index)
            && (hatches.tabIntentFn != nil ? hatches.tabIntent(2, String(index)) : hatches.act(tabNodes[index], 0))
        if !taken { hatches.log("tabs: select(\(index)) refused") }
    }
    /// The container's Back: the platform's Back of the stack in front (a
    /// sheet's, when one is presented), as the platform's Back button takes
    /// it; the controllers move as the router says. Queued as `select` is, so
    /// one sent from inside `onSelect` is the Back of the tab selected. False,
    /// and journaled, when Exact cannot queue it. When it runs, a transition,
    /// nothing beneath that stack's top, or a route whose Back goes nowhere is
    /// journaled as refused.
    @discardableResult public func back() -> Bool { intent(0, "", "back()") }
    /// The container opens `url` as a URL that arrived from the system
    /// (`ExactSession.openURL`); the router decides where it leads. Queued as
    /// `select` is. False, and journaled, when Exact cannot queue it: a URL
    /// that is empty, over 64 KB or that `URL(string:)` does not parse, a full
    /// act queue, a container that has not yet taken the place of Exact's
    /// own, a retired container, a host without the intents' entry. A
    /// location the session does not take is journaled when it runs. These
    /// refusals name the URL without its query and fragment, where tokens
    /// usually travel (a path and user info stay as given); one that begins
    /// with them by its `?` or `#`; and one whose part before them is over
    /// 64 KB by its whole length. The router's own lines name a location as
    /// they were given it.
    @discardableResult public func open(_ url: String) -> Bool {
        // Named as the host's `NavigationHost.urlName` names it, which this module cannot call; 65536 is `hatchTextBytes`
        let cut = String(url.unicodeScalars.prefix { $0 != "?" && $0 != "#" })
        let name = cut.utf8.count > 65536 ? "\(url.utf8.count) bytes" : cut.isEmpty ? url.unicodeScalars.first.map(String.init) ?? "" : cut
        return intent(1, url, "open(\(name))")
    }
    private func intent(_ kind: UInt32, _ text: String, _ call: String) -> Bool {
        guard let hatches else { return false }
        guard isLive, hatches.tabIntent(kind, text) else { hatches.log("tabs: \(call) refused"); return false }
        return true
    }
}
#endif
