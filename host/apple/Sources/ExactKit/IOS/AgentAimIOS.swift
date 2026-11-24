// Where a real finger aims when UIKit draws a node in its place (LLP 1080.000
// D4, LLP 1116.003): a header button's item in the navigation bar under
// `--chrome platform`. The authored node stays hidden in the content, so
// its own box is not where a person would touch.
#if os(iOS) || os(tvOS)
import UIKit

extension Agent {
    /// A compact control can occupy only one end of its authored box.
    func nativeControlPoint(_ node: NodeView, box b: CGRect, in window: UIWindow) -> AddressedAim? {
        guard let control = presenter.controls.controls[node.id], control.window === window else { return nil }
        let cb = box(control), point = CGPoint(x: cb.midX, y: cb.midY), middle = CGPoint(x: b.midX, y: b.midY)
        guard point != middle else { return nil }
        let vp = presenter.viewport
        let toWindow = { (q: CGPoint) in vp.convert(CGPoint(x: q.x + vp.pageOrigin.x, y: q.y + vp.pageOrigin.y), to: nil) }
        guard pressReach(node, at: toWindow(point), in: window)?.isOwn(node) == true else { return nil }
        return .at(point, pressReach(node, at: toWindow(middle), in: window) ?? .nothing)
    }

    /// Under `--chrome platform`, the navigation bar a header button's item
    /// sits in and the item's box in it (r5 weather's Cities, r5 mood
    /// journal's sheet Save). Nil for any other node; a refusal for an item
    /// the bar shows no control for. UIKit answers `frame(in:)` for such an
    /// item too (#738): with its overflow button's frame for an item in the
    /// overflow menu, and with a box at the bar's leading edge, where a
    /// finger reaches Back, for a hidden item (iOS 27.0) or one it dropped
    /// (reported on iOS 26.5). An item in the overflow menu
    /// ([`overflowPresentationSource`](https://developer.apple.com/documentation/uikit/uinavigationitem/overflowpresentationsource))
    /// is refused, not pressed there: a tap is one touch on the item's own
    /// control, and that menu's rows have no identifier and can be scrolled
    /// out of view. Any other item is placed only where `ownedControl` finds
    /// its control, in the bar or beside it.
    func barItem(_ node: NodeView) -> Result<(bar: UIView, item: CGRect), BarItemMissing>? {
        let place = presenter.navigation.barItem(node)
        #if os(iOS)
        guard case .success(let (_, frame))? = place, let (bar, item, navigationItem) = presenter.navigation.barButtonItem(node) else { return place }
        // A hidden item has no control, in the bar or beside it, whatever frame UIKit reports for it
        if item.isHidden { return .failure(BarItemMissing()) }
        let middle = CGPoint(x: frame.midX, y: frame.midY)
        if navigationItem.overflowPresentationSource?.frame(in: bar)?.contains(middle) == true { return .failure(BarItemMissing(overflowed: true)) }
        // A readout's view is its item's own
        if let own = item.customView { return own.window != nil ? place : .failure(BarItemMissing()) }
        // Only a hit in the views that draw this bar's items is judged: what covers them is `drawnCovered`'s to name
        guard let hit = bar.window?.hitTest(bar.convert(middle, to: nil), with: nil), Self.itemHolders(of: bar).contains(where: { hit.isDescendant(of: $0) }) else { return place }
        return ownedControl(item, of: navigationItem, frame: frame, hit: hit, in: bar) != nil ? place : .failure(BarItemMissing())
        #else
        return place
        #endif
    }

    #if os(iOS)
    /// The control UIKit shows for `item` where a finger at the middle of
    /// `frame` lands (`hit`): the control the hit is in, or, where the hit is
    /// in none, the only one with the item's box in the bar for a hit in the
    /// bar, and inside the hit beside it (a group's shared glass around its
    /// buttons); nil for no such control, or one another item or UIKit's name
    /// claims. UIKit has no public way from
    /// a control to its item, and names a control by its item's identifier
    /// only once its accessibility is loaded, which a `tap` without a real
    /// touch does not do. What it does say is the item's frame, which is its
    /// own control's box: so the item owns a control with that box, in the
    /// views that draw the bar's items, that nothing else claims. Another
    /// item reporting the same frame claims it, as does UIKit's name for it
    /// when that is not the item's. Where the system places no vertical bar,
    /// Back claims the control of the bar with none of the bar's on its
    /// leading side: no item sits there (`leftItemsSupplementBackButton`).
    /// Where it places one (a closed iPhone Duo's cover panel), Back is in that
    /// bar and claims nothing. There UIKit gives an item in the overflow menu
    /// an empty frame, for which `NavigationHost.barItem` answers the More
    /// button's; what it reports for a hidden item has not been measured (LLP
    /// 1080.000 §13).
    func ownedControl(_ item: UIBarButtonItem, of navigationItem: UINavigationItem, frame: CGRect, hit: UIView, in bar: UINavigationBar) -> UIView? {
        let holders = Self.itemHolders(of: bar), holds = { (v: UIView) in holders.contains { $0 === v } }
        guard holders.contains(where: { hit.isDescendant(of: $0) }) else { return nil }
        let same = { (a: CGRect, b: CGRect) in abs(a.minX - b.minX) <= 0.5 && abs(a.minY - b.minY) <= 0.5 && abs(a.maxX - b.maxX) <= 0.5 && abs(a.maxY - b.maxY) <= 0.5 }
        let box = { (v: UIView) in v.convert(v.bounds, to: bar) }
        // Every control of those views, shown or not: while a push settles the bar's views are no guide to what it shows
        var controls: [UIView] = [], pending = holders.flatMap(\.subviews)
        while let v = pending.popLast() {
            if v is UIControl { controls.append(v) } else { pending.append(contentsOf: v.subviews) }
        }
        // The control reached, or where the hit is in none the only one with that box, in the bar for a hit in the bar
        // and inside the hit beside it: a group's shared glass can leave the line through an item's middle out of its
        // hit shape (iOS 27.0), and beside the bar the hit then lands in the glass around the button
        let reached = sequence(first: hit, next: \.superview).prefix(while: { !holds($0) }).filter { $0 is UIControl }
        // Beside the bar the button is one the glass shows: nothing up to the hit hides it
        let within = hit.isDescendant(of: bar) ? bar : hit
        let shown = { (v: UIView) in within === bar || sequence(first: v, next: \.superview).prefix(while: { $0 !== within }).allSatisfy { !$0.isHidden } }
        let boxed = controls.filter { same(box($0), frame) && $0.isDescendant(of: within) && shown($0) }
        guard let control = reached.isEmpty && boxed.count == 1 ? boxed[0] : reached.first(where: { same(box($0), frame) }) else { return nil }
        let named = sequence(first: control, next: \.superview).prefix(while: { !holds($0) }).lazy.compactMap(\.accessibilityIdentifier).first { !$0.isEmpty }
        if let named, named != item.accessibilityIdentifier { return nil }
        let items = (navigationItem.leftBarButtonItems ?? []) + (navigationItem.rightBarButtonItems ?? [])
        if items.contains(where: { other in other !== item && other.frame(in: bar).map { same($0, frame) } == true }) { return nil }
        guard bar.backItem != nil, !navigationItem.hidesBackButton, !bar.traitCollection.placesVerticalBar, control.isDescendant(of: bar) else { return control }
        let rtl = bar.effectiveUserInterfaceLayoutDirection == .rightToLeft, own = box(control)
        return controls.contains { $0 !== control && $0.isDescendant(of: bar) && (rtl ? box($0).maxX > own.maxX + 0.5 : box($0).minX < own.minX - 0.5) } ? control : nil
    }

    /// The views UIKit may draw `bar`'s items in: the bar, and beside it each
    /// view of its navigation controller's own view that holds a control but
    /// not the controller's pages, such as the strip of the items on a closed
    /// iPhone Duo's cover panel (iOS 27.1). Public API does not say which
    /// draws them, so its toolbar is one too; a view with no control draws no
    /// item, and covers what it is over, as does another navigation bar or a
    /// view that holds one. Just the bar when the bar is not in its
    /// controller's view or no view there holds the pages.
    static func itemHolders(of bar: UINavigationBar) -> [UIView] {
        guard let controller = bar.delegate as? UINavigationController, let holder = bar.superview, holder === controller.viewIfLoaded,
              let pages = controller.topViewController?.viewIfLoaded,
              holder.subviews.contains(where: { $0 !== bar && pages.isDescendant(of: $0) }) else { return [bar] }
        let drawsItems = { (view: UIView) -> Bool in
            var pending = view.subviews, control = false
            while let v = pending.popLast() {
                if v is UINavigationBar { return false }
                control = control || v is UIControl
                pending.append(contentsOf: v.subviews)
            }
            return control
        }
        return holder.subviews.filter { $0 === bar || (!($0 is UINavigationBar) && !pages.isDescendant(of: $0) && drawsItems($0)) }
    }
    #endif

    /// The control UIKit shows for the item standing for the tab `node` is
    /// (or is in), as VoiceOver and XCUITest find it (a `UITabBarItem` has no
    /// public frame): in the tab bar its tablist is drawn as (`drawnItem`),
    /// or under `--chrome platform` in UIKit's tab bar, the one accessibility
    /// element with the item's identifier. Nil for any other node; a refusal
    /// when none, or more than one, shows, or the item is outside the window.
    func tabItem(_ node: NodeView) -> Result<UIView, AimRefusal>? {
        #if os(iOS)
        if let (bar, item) = presenter.segments.barItem(of: node) { return drawnItem(item, in: bar) }
        let nav = presenter.navigation
        guard nav.tabBarShows, let tabs = nav.container.flatMap({ NavigationTabs.of($0, presenter) }),
              let index = tabs.tabs.firstIndex(where: { node === $0 || node.isDescendant(of: $0) }),
              let controller = nav.tabController, controller.tabBar.items != nil else { return nil }
        // Find the tab's item by its controller: a tab whose item the bar does not show is a row of More's list
        guard let item = nav.barItem(ofTab: index) else {
            return .failure(AimRefusal(why: "its tab is a row of UIKit's More list, which the bar has no item for; tap More, then its row"))
        }
        // By the item's identifier (its tab's test id, `syncItems`), which
        // UIKit gives the control it shows for it, in the bar or, on an
        // iPad, at the top or in the sidebar; by its title only without one.
        let id = item.accessibilityIdentifier ?? "", title = item.title ?? ""
        guard !id.isEmpty || !title.isEmpty else { return .failure(AimRefusal(why: "its tab has neither a test id nor a title UIKit's tab bar shows; give it a testId")) }
        let found = Self.elements(under: controller.view.subviews).filter { v in
            let box = v.convert(v.bounds, to: nil)
            return (id.isEmpty ? v.accessibilityLabel == title : v.accessibilityIdentifier == id) && v.window?.bounds.contains(CGPoint(x: box.midX, y: box.midY)) == true
        }
        guard found.count == 1 else {
            return .failure(AimRefusal(why: found.isEmpty ? "UIKit's tab bar shows no item for its tab; `clock settle` first waits out a transition"
                : "UIKit shows \(found.count) tab items as \(id.isEmpty ? "\"\(title)\"" : id); give each tab its own testId"))
        }
        return .success(found[0])
        #else
        return nil
        #endif
    }

    /// The segment or bar button item UIKit draws in `node`'s place, where a
    /// finger aims at it and what covers it is judged. The node's own box is
    /// not that place: a header the bar covers whole is laid out as
    /// `display: none`, so its nodes' boxes are no place on the screen (LLP
    /// 1080.000 D4). `region` is in `view`'s space; `barred`, that a bar
    /// draws it; `inBar`, that it is a bar item, whose hit `barItemTarget`
    /// judges. Nil for any other node; a refusal for a bar item the bar
    /// shows no control for.
    func drawnPlace(_ node: NodeView) -> Result<(view: UIView, region: CGRect, barred: Bool, inBar: Bool), AimRefusal>? {
        if let (control, segment) = presenter.segments.drawn(node) {
            return .success((control, segment, presenter.navigation.barSegment(node) != nil, false))
        }
        switch barItem(node) {
        case .success(let (bar, frame))?: return .success((bar, frame, true, true))
        case .failure(let missing)?: return .failure(AimRefusal(why: missing.overflowed
            ? "its bar item is in the navigation bar's overflow menu (UIKit's More button), which the agent does not open; the bar has no room for it"
            : "its bar item is not placed in the navigation bar; `clock settle` first waits out a push or sheet still moving"))
        case nil: return nil
        }
    }

    #if os(iOS)
    /// The views under `roots` that an accessibility client reads as
    /// elements, shown and with a box; the walk does not enter nodes.
    private static func elements(under roots: [UIView]) -> [UIView] {
        var found: [UIView] = [], pending = roots
        while !pending.isEmpty {
            let v = pending.removeFirst()
            guard !v.isHidden, v.alpha > 0.01, !(v is NodeView) else { continue }
            if v.isAccessibilityElement {
                if v.bounds.width > 0, v.bounds.height > 0 { found.append(v) }
                continue
            }
            pending.append(contentsOf: v.subviews)
        }
        return found
    }

    /// The control the tab bar a tablist is drawn as (LLP 1059) shows for
    /// `item`. That bar centres its items in a pill narrower than its box, so
    /// the hidden tab's box is not where its item is. By the item's
    /// identifier, else by its name or title: iOS 27.1's tab button carries
    /// its item's identifier only once an accessibility client has asked. The
    /// bar draws each button twice for its glass, so copies at one box count
    /// once: the copy a finger at its middle reaches.
    private func drawnItem(_ item: UITabBarItem, in bar: UITabBar) -> Result<UIView, AimRefusal> {
        let id = item.accessibilityIdentifier ?? "", names = [item.accessibilityLabel, item.title].compactMap { $0 }.filter { !$0.isEmpty }
        let controls = Self.elements(under: [bar]).filter { $0 is UIControl }
        var found = id.isEmpty ? [] : controls.filter { $0.accessibilityIdentifier == id }
        let byName = found.isEmpty
        if byName { found = controls.filter { $0.accessibilityLabel.map(names.contains) == true } }
        var shown: [(box: CGRect, control: UIView)] = []
        for v in found {
            let box = v.convert(v.bounds, to: nil), middle = CGPoint(x: box.midX, y: box.midY)
            let reached = v.window?.hitTest(middle, with: nil).map { $0 === v || $0.isDescendant(of: v) } == true
            if let i = shown.firstIndex(where: { abs($0.box.midX - box.midX) < 1 && abs($0.box.midY - box.midY) < 1 }) {
                if reached { shown[i].control = v }
            } else { shown.append((box, v)) }
        }
        guard shown.count == 1 else {
            return .failure(AimRefusal(why: shown.isEmpty ? "the tab bar its tablist is drawn as shows no item for its tab; `clock settle` first waits out a transition"
                : byName ? "the tab bar its tablist is drawn as shows \(shown.count) items named \"\(names.first ?? "")\"; give each tab its own name"
                : "the tab bar its tablist is drawn as shows \(shown.count) items as \(id); give each tab its own testId"))
        }
        let (box, control) = shown[0]
        guard control.window?.bounds.contains(CGPoint(x: box.midX, y: box.midY)) == true else {
            return .failure(AimRefusal(why: "its item is outside the window; scroll it into view first"))
        }
        return .success(control)
    }
    #endif

    /// Why a finger would not reach the segment or bar item UIKit draws in
    /// `node`'s place (a sheet over it, the keyboard), or nil: the tap that
    /// activates either is refused as a covered node's is.
    func drawnCovered(_ node: NodeView) -> String? {
        guard case .success(let drawn)? = drawnPlace(node), let win = drawn.view.window else { return nil }
        let p = drawn.view.convert(CGPoint(x: drawn.region.midX, y: drawn.region.midY), to: nil)
        awaitAppOverlay(at: p, in: win)
        guard let hit = win.hitTest(p, with: nil) else { return "its middle is off the window" }
        return obscured(drawn.inBar ? barItemTarget(node, bar: drawn.view, frame: drawn.region, hit: hit) : drawn.view, at: p, hit: hit)
    }

    /// What a hit at a bar item's middle must land in: the item's own button
    /// when the hit is in it beside the bar, otherwise the bar or the view
    /// beside it that draws the item. UIKit can host an item's
    /// button outside the bar's views: a glass item's among the views this
    /// session shows, found by the identifier Exact gives the item (its test
    /// id); and on a closed iPhone Duo's cover panel a shown item's, in a
    /// strip beside the bar (iOS 27.1, measured for Exact Chat's Options),
    /// whose buttons carry no identifier while accessibility is not loaded,
    /// found as `barItem` found it (`ownedControl`). `frame` is the item's, in
    /// `bar`'s space.
    func barItemTarget(_ node: NodeView, bar: UIView, frame: CGRect, hit: UIView) -> UIView {
        let chain = sequence(first: hit, next: { $0.superview })
        // Fall back to the bar in a nested session's view, which `Agent.shows` counts through this session's view
        if let nearest = chain.lazy.compactMap({ $0 as? ExactView }).first, nearest.session !== session { return bar }
        if let id = node.props["testId"], let named = chain.first(where: { $0.accessibilityIdentifier == id && Agent.shows(session, $0) }) {
            return named
        }
        #if os(iOS)
        // A hit in the bar lands in the bar, which `barItem` has judged
        guard let (navigationBar, item, navigationItem) = presenter.navigation.barButtonItem(node), navigationBar === bar, !hit.isDescendant(of: bar),
              let control = ownedControl(item, of: navigationItem, frame: frame, hit: hit, in: navigationBar) else { return bar }
        // A control found by its box inside the hit (a group's shared glass): the hit lands in the glass, as in the bar
        return hit.isDescendant(of: control) ? control : hit
        #else
        return bar
        #endif
    }

    /// Whether `view` is in what `session` shows: its viewport, its view, or
    /// a navigation controller or tab bar controller of its own, a presented
    /// sheet's included, whose bars are outside both.
    static func shows(_ session: ExactSession, _ view: UIView) -> Bool {
        if session.presenter.mounts(view) || session.view.map({ view.isDescendant(of: $0) }) == true { return true }
        // A dialog's sheet holds the session's own nodes, outside its view (LLP 1115.000 D11.5).
        if session.presenter.dialogs.shows(view) { return true }
        let nav = session.presenter.navigation
        if let tabs = nav.tabController, view.isDescendant(of: tabs.view) { return true }
        return nav.allNavigations.contains { $0.viewIfLoaded.map { view.isDescendant(of: $0) } == true }
    }
}
#endif
