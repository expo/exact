// Where a real finger aims when UIKit draws a node in its place (LLP 1080.000
// D4, LLP 1116.003): a header button's item in the navigation bar under
// `--chrome platform`. The authored node stays hidden in the content, so
// its own box is not where a person would touch.
#if os(iOS) || os(tvOS)
import UIKit

#if os(iOS)
extension UIBarButtonItem {
    /// UIKit can project the button into the tab rail, outside its navigation bar (LLP 1035.003).
    func projectedControl(in window: UIWindow?) -> UIView? {
        var control = customView
        #if EXACT_INSPECT
        if control == nil, responds(to: NSSelectorFromString("view")) {
            control = value(forKey: "view") as? UIView
        }
        #endif
        guard !isHidden, let window, control?.window === window else { return nil }
        return control
    }
}
#endif

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

    /// The view and region to aim at. An item in the overflow menu has More's presentation
    /// frame, or an empty frame on the Duo. Neither is a touch on the item itself.
    func barItem(_ node: NodeView) -> Result<(bar: UIView, item: CGRect), BarItemMissing>? {
        #if os(iOS)
        if let bar = presenter.navigation.backButtonBar(node) {
            guard let control = bar.drawnBackControl() else { return .failure(BarItemMissing()) }
            return .success((control, control.bounds))
        }
        if let (bar, item, owner) = presenter.navigation.barButtonItem(node) {
            guard !item.isHidden else { return .failure(BarItemMissing()) }
            if let more = owner.overflowPresentationSource?.frame(in: bar), !more.isEmpty {
                let frame = item.frame(in: bar) ?? .zero
                if frame.isEmpty || more.contains(CGPoint(x: frame.midX, y: frame.midY)) {
                    return .failure(BarItemMissing(overflowed: true))
                }
            }
            guard let control = item.projectedControl(in: bar.window) else { return .failure(BarItemMissing()) }
            return .success((control, control.bounds))
        }
        #endif
        return presenter.navigation.barItem(node)
    }

    /// The control UIKit shows for the item standing for the tab `node` is
    /// (or is in), as VoiceOver and XCUITest find it (a `UITabBarItem` has no
    /// public frame): in the tab bar its tablist is drawn as (`drawnItem`),
    /// or under `--chrome platform` in UIKit's tab bar, the one accessibility
    /// element with the item's identifier. Nil for any other node; a refusal
    /// when none, or more than one, shows, or the item is outside the window.
    func tabItem(_ node: NodeView) -> Result<UIView, AimRefusal>? {
        #if os(iOS)
        if let (bar, item) = presenter.segments.barItem(of: node) { return drawnItem(item, among: Self.elements(under: [bar])) }
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
        let shown = Self.elements(under: controller.view.subviews).filter { $0.window?.bounds.contains(Self.middle($0)) == true }
        let found = shown.filter { id.isEmpty ? $0.accessibilityLabel == title : $0.accessibilityIdentifier == id }
        // A button without one yet is found by its name (`drawnItem`), and by
        // place, as a vertical bar's buttons are not the bar's subviews.
        if found.isEmpty, let bar = controller.shownTabBar {
            let place = bar.convert(bar.bounds, to: nil)
            return drawnItem(item, among: shown.filter { place.contains(Self.middle($0)) })
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
    /// draws it. A bar item's hit must belong to its control.
    func drawnPlace(_ node: NodeView) -> Result<(view: UIView, region: CGRect, barred: Bool, inBar: Bool), AimRefusal>? {
        if let (control, segment) = presenter.segments.drawn(node) {
            return .success((control, segment, presenter.navigation.barSegment(node) != nil, false))
        }
        var item = barItem(node)
        if case .success(let place)? = item, let window = place.bar.window,
           !place.bar.isHidden, place.bar.alpha > 0.01,
           sequence(first: place.bar, next: \.superview).dropFirst().contains(where: { $0.alpha == 0 }) {
            // UIKit attaches an entering item's control before its glass fades in. Let the
            // transition deliver its frames, then resolve again in case UIKit replaced the item.
            let deadline = Date(timeIntervalSinceNow: 1.5)
            while place.bar.window === window, RouteGeometry.shownBar(place.bar) == nil, Date() < deadline {
                RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.016))
            }
            item = barItem(node)
        }
        switch item {
        case .success(let place)? where RouteGeometry.shownBar(place.bar) != nil:
            return .success((place.bar, place.item, true, true))
        case .failure(let missing)? where missing.overflowed:
            return .failure(AimRefusal(why: "its bar item is in the navigation bar's overflow menu (UIKit's More button), which the agent does not open; the bar has no room for it"))
        case .success?, .failure?:
            return .failure(AimRefusal(why: "its bar item is not placed in the navigation bar; `clock settle` first waits out a push or sheet still moving"))
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

    private static func middle(_ v: UIView) -> CGPoint { v.convert(CGPoint(x: v.bounds.midX, y: v.bounds.midY), to: nil) }

    /// The one of `elements` that a tab bar shows for `item`: UIKit's, or the
    /// one a tablist is drawn as (LLP 1059), which centres its items in a pill
    /// narrower than its box, so the hidden tab's box is not where its item
    /// is. By the item's identifier, else by its name or title: iOS 27.1's
    /// tab button carries its item's identifier only once an accessibility
    /// client has asked, and one made since (a tab out of More, as the bar
    /// widens) has none. The bar draws each button twice for its glass, so
    /// copies at one box count once: the copy a finger at its middle reaches.
    private func drawnItem(_ item: UITabBarItem, among elements: [UIView]) -> Result<UIView, AimRefusal> {
        let id = item.accessibilityIdentifier ?? "", names = [item.accessibilityLabel, item.title].compactMap { $0 }.filter { !$0.isEmpty }
        let controls = elements.filter { $0 is UIControl }
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
            return .failure(AimRefusal(why: shown.isEmpty ? "its tab bar shows no item for its tab; `clock settle` first waits out a transition"
                : byName ? "its tab bar shows \(shown.count) items named \"\(names.first ?? "")\"; give each tab its own name"
                : "its tab bar shows \(shown.count) items as \(id); give each tab its own testId"))
        }
        let (box, control) = shown[0]
        guard control.window?.bounds.contains(CGPoint(x: box.midX, y: box.midY)) == true else {
            return .failure(AimRefusal(why: "its item is outside the window; scroll it into view first"))
        }
        return .success(control)
    }
    #endif

    /// A second control inside the button intercepts the touch before it reaches the button.
    /// If an app disables interaction on the backing control of an enabled item, refuse even the glass that fires it,
    /// because a hit on that ancestor cannot establish which button it belongs to (LLP 1035.003).
    static func barItemReceives(_ hit: UIView, in button: UIView) -> Bool {
        sequence(first: hit, next: \.superview).first { $0 === button || $0 is UIControl } === button
    }

    /// Why a finger would not reach the segment or bar item UIKit draws in
    /// `node`'s place (a sheet over it, the keyboard), or nil: the tap that
    /// activates either is refused as a covered node's is.
    func drawnCovered(_ node: NodeView) -> String? {
        guard let place = drawnPlace(node) else { return nil }
        if case .failure(let refusal) = place { return refusal.why }
        guard case .success(let drawn) = place, let win = drawn.view.window else { return "its middle is off the window" }
        let p = drawn.view.convert(CGPoint(x: drawn.region.midX, y: drawn.region.midY), to: win)
        awaitAppOverlay(at: p, in: win)
        guard let hit = win.hitTest(p, with: nil) else { return "its middle is off the window" }
        return obscured(drawn.view, at: p, hit: hit, barItem: drawn.inBar)
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
