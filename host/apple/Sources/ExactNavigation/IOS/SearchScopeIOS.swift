// A search's scope bar (LLP 1134): a filter tablist right after a header
// whose search field the bar projects is that search bar's scope buttons, as
// a hand-built iPhone screen with a large title, a search and a filter has
// it, riding the bar as the title collapses.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

extension NavigationHost {
    /// Whether the search bar draws `tablist` (`SegmentHost` steps aside).
    package func scopes(tablist: NodeView) -> Bool { controllers.values.contains { $0.scoped === tablist } }

    #if EXACT_INSPECT
    /// A route's title, subtitle or search scope control and the segment drawn for `tab`.
    func barSegment(_ tab: NodeView) -> (control: UIControl, segment: CGRect)? {
        #if os(iOS)
        let found: (RouteController, [UInt32], UISegmentedControl?)? = controllers.values.lazy.compactMap { c in
            if let title = c.titleSegments, title.press.tabs.contains(tab.id) { return (c, title.press.tabs, title.control) }
            if let search = c.search, search.scopeTabs.contains(tab.id) { return (c, search.scopeTabs, search.scopeControl) }
            if #available(iOS 26.0, *), let s = c.subtitleSegments, s.tabs.contains(tab.id) { return (c, s.tabs, s.shown) }
            return nil
        }.first
        guard let (c, tabs, shown) = found, c.navigationController?.topViewController === c, let control = shown, control.window != nil,
              let index = tabs.firstIndex(of: tab.id) else { return nil }
        let views = control.segmentViews
        guard views.count == tabs.count else { return nil }
        return (control, views[index].convert(views[index].bounds, to: control))
        #else
        return nil
        #endif
    }
    #endif
}
#endif

#if os(iOS)
import UIKit

extension NavigationHost {
    /// UIKit changed a layout a projected bar depends on (where it put the search field, the width class): the bars
    /// are projected again in the next turn, unless a sync is under way, owed or held, which projects them anyway.
    func reprojectBarsNextTurn() {
        if tabOwner != nil { return syncNextTurn() }
        DispatchQueue.main.async { [weak self] in
            guard let self, !syncing, !resyncOwed, !pendingSync else { return }
            resync()
        }
    }

    /// D1, D2: the tablist the search bar takes as its scope bar, hidden
    /// and covered whole as the header is; the one it took before shown
    /// again (its cover goes with the next `reportCovers`).
    func scope(_ c: RouteController, shape: HeaderShape?) {
        c.searchLayoutRead = shape?.search == nil ? nil : c.searchLayout
        let next = shape.flatMap { scopeFilter(after: $0, in: c) }
        if next !== c.scoped {
            // The bar's height moves as the scope bar comes or goes, as the subtitle views' does (`resampleTitleInsets`).
            if shape?.search != nil, c.scoped != nil || next != nil { c.resampleTitleInsets() }
            c.scoped?.isHidden = false
            c.scoped = next
            next?.isHidden = true
        }
        // LLP 1135: under a large title with no search, the item's subtitle views.
        if #available(iOS 26.0, *) { subtitles(c, shape?.search == nil ? next : nil) }
    }

    /// LLP 1135 D2: `list` as the segmented control under the large title
    /// and under the inline one, or as a bar item where the bar is wide, or
    /// neither.
    @available(iOS 26.0, *)
    private func subtitles(_ c: RouteController, _ list: NodeView?) {
        let item = c.navigationItem
        guard let list else {
            if let mine = c.subtitleSegments {
                // Only its own: a hatch's views stay.
                if item.largeSubtitleView === mine.large { item.largeSubtitleView = nil }
                if item.subtitleView === mine.inline { item.subtitleView = nil }
                mine.leaveBar(item)
                c.subtitleSegments = nil
                c.resampleTitleInsets()
            }
            return
        }
        let segments = c.subtitleSegments ?? {
            let made = SubtitleSegments(host: self, route: c)
            c.subtitleSegments = made
            return made
        }()
        segments.place(in: c)
        segments.apply(list)
    }

    /// D1: the next child the author shows in flow after a header the bar
    /// replaces whole whose search it projects, when it is a filter: two to
    /// five text tabs, one selected, none disabled, naming no panel, and no
    /// look of the author's on it or them.
    private func scopeFilter(after shape: HeaderShape, in c: RouteController) -> NodeView? {
        // A search's scope bar (LLP 1134), or a large title's subtitle views (LLP 1135 D1).
        // Not under a title the bar draws with more than its words (a subtitle, an
        // avatar, segments), whose subtitle views it would take.
        let large: Bool = {
            guard #available(iOS 26.0, *) else { return false }
            // Nor where a hatch set a subtitle, or a subtitle view of its own.
            let item = c.navigationItem, mine = c.subtitleSegments
            let free = item.subtitle == nil && item.largeSubtitle == nil
                && (item.largeSubtitleView == nil || item.largeSubtitleView === mine?.large) && (item.subtitleView == nil || item.subtitleView === mine?.inline)
            return shape.level == 1 && shape.segments == nil && shape.group == nil && item.largeTitleDisplayMode == .always && free
        }()
        // A search's filter is its scope bar only where the scope bar fits (`scopeBarFits`); elsewhere it stays the
        // page's, held above the list with the scroll edge under it (LLP 1131).
        let scoped = shape.search != nil && c.scopeBarFits
        guard UIDevice.current.userInterfaceIdiom == .phone, shape.authored.isEmpty, scoped || (shape.search == nil && large) else { return nil }
        let kids = c.node.container.subviews.compactMap { $0 as? NodeView }
        // What the host hides (this filter, a segmented control's tabs) is still the author's.
        let shown = { (v: NodeView) in v.style["display"]?.string != "none" && RouteGeometry.inFlow(v) }
        guard let at = kids.firstIndex(where: { $0 === shape.header }),
              let list = kids[(at + 1)...].first(where: shown),
              // Under a large title it is the list's filter, right above it (LLP 1135 D1).
              shape.search != nil || kids.firstIndex(where: { $0 === list }).flatMap({ kids[($0 + 1)...].first(where: shown) })?.scrollView != nil,
              list.props["accessibilityRole"] == "tablist", list.props["accessibilityOrientation"] != "vertical", !Self.writesLook(list) else { return nil }
        // One that takes no input stays the page's, where it shows so (SegmentHost's `available`).
        var up: UIView? = list
        while let v = up {
            if (v as? NodeView)?.props["inert"] == "true" { return nil }
            up = presenter.eventParent(of: v)
        }
        if list.cssVisibilityHidden || list.alpha < 1 { return nil }
        let members = list.container.subviews.compactMap { $0 as? NodeView }.filter { $0.style["display"]?.string != "none" }
        guard (2...5).contains(members.count), members.filter({ $0.props["accessibilitySelected"] == "true" }).count == 1,
              members.allSatisfy({ tab in
                  guard tab.isButton, tab.props["accessibilityRole"] == "tab", tab.handlers.contains("press"), !tab.handlers.contains("contextmenu"), !tab.disabled, !tab.ariaUnavailable,
                        tab.props["accessibilityControls"] == nil, case .title(let words)? = tab.segmentFace, tab.accessibleName == words else { return false }
                  return tab.alpha == 1 && !Self.writesLook(tab) && !tab.container.subviews.contains { ($0 as? NodeView).map(Self.writesLook) == true }
              }) else { return nil }
        return list
    }

    /// A colour, background, border, shadow, font or `appearance` its author
    /// wrote (opacity is the view's `alpha`, read where this is).
    private static func writesLook(_ n: NodeView) -> Bool {
        guard case .array(let rows)? = n.style["written"] else { return false }
        let looks = ["background", "text_color", "tint_color", "accent_color", "font", "border_width", "border_style", "border_color", "box_shadow", "appearance"]
        return rows.contains { row in row.string.map { key in looks.contains { key.hasPrefix($0) } } == true }
    }
}

extension HeaderSearch {
    /// D2–D4: the scope bar is `list`'s tabs, or none.
    func scope(_ list: NodeView?) {
        let bar = controller.searchBar
        let tabs = list.map { $0.container.subviews.compactMap { $0 as? NodeView }.filter { $0.style["display"]?.string != "none" } } ?? []
        guard !tabs.isEmpty else {
            guard !scopeTabs.isEmpty else { return }
            scopeList = nil
            scopeTabs = []
            bar.scopeButtonTitles = nil
            bar.showsScopeBar = false
            return
        }
        scopeList = list
        scopeTabs = tabs.map(\.id)
        let titles = tabs.map { tab in if case .title(let words)? = tab.segmentFace { return words }; return tab.accessibleName }
        if bar.scopeButtonTitles != titles { bar.scopeButtonTitles = titles; scopeSettled = nil }
        showScopes()
        let selected = tabs.firstIndex { $0.props["accessibilitySelected"] == "true" } ?? 0
        // A finger's choice not yet reported stays (D3); the settled index is
        // written first, so a delegate call setting it raises presses nothing.
        let pending = scopeControl?.isTracking == true || scopeSettled.map({ $0 != bar.selectedScopeButtonIndex }) == true
        if !pending, bar.selectedScopeButtonIndex != selected {
            scopeSettled = selected
            bar.selectedScopeButtonIndex = selected
        }
        if bar.selectedScopeButtonIndex == selected { scopeSettled = selected }
        scopeIDs = tabs.map { $0.props["testId"] }
        labelScopes()
    }

    /// UIKit resets these as it presents and dismisses the search.
    func showScopes() {
        guard !scopeTabs.isEmpty else { return }
        if !controller.searchBar.showsScopeBar { controller.searchBar.showsScopeBar = true }
        if controller.scopeBarActivation != .manual { controller.scopeBarActivation = .manual }
    }

    /// Writes its tabs' test ids on the segments an assistive client reads
    /// (`accessibilitySegments`), as the route lays out
    /// (`RouteController.viewDidLayoutSubviews`). The control is UIKit's and
    /// gives no read to hook, so a client that starts reading after the
    /// route's last layout finds the ids at its next one.
    func labelScopes() {
        guard let control = scopeControl else { return }
        control.layoutIfNeeded()
        let elements = control.accessibilitySegments
        guard elements.count == scopeIDs.count else { return }
        for (element, id) in zip(elements, scopeIDs) { UISegmentedControl.identify(element, as: id) }
    }

    /// The scope bar's segmented control: in the search bar where the field
    /// is stacked under the title, in the navigation bar where the field
    /// goes to the toolbar (an iPhone with no tab bar, iOS 26), while its
    /// route is the top one. It is UIKit's control whose segments are the
    /// scope titles: not Exact's own, nor in the item's title view.
    var scopeControl: UISegmentedControl? {
        guard let titles = controller.searchBar.scopeButtonTitles, !scopeTabs.isEmpty else { return nil }
        let title = item?.titleView
        func find(_ v: UIView) -> UISegmentedControl? {
            if v === title || v is LabelledSegmentedControl { return nil }
            if let s = v as? UISegmentedControl, s.numberOfSegments == titles.count, titles.indices.allSatisfy({ s.titleForSegment(at: $0) == titles[$0] }) { return s }
            for child in v.subviews { if let s = find(child) { return s } }
            return nil
        }
        if let s = find(controller.searchBar) { return s }
        guard let item, item.searchController === controller,
              let nav = host?.controllers.values.first(where: { $0.navigationItem === item })?.navigationController, nav.topViewController?.navigationItem === item else { return nil }
        return find(nav.navigationBar)
    }

    /// D3: a person's choice presses that segment's tab.
    func searchBar(_ bar: UISearchBar, selectedScopeButtonIndexDidChange index: Int) {
        guard index != scopeSettled, scopeTabs.indices.contains(index), let presenter = host?.presenter else { return }
        scopeSettled = index
        if let tab = presenter.views[scopeTabs[index]], !tab.disabled, !tab.ariaUnavailable { presenter.press(tab.id) }
        // The press's batch may not reach the bar (one that changes only list rows): its answer is read here.
        if let list = scopeList, presenter.views[list.id] === list { scope(list) }
    }
    func didPresentSearchController(_ search: UISearchController) { showScopes() }
}
/// LLP 1135: a filter as the segmented controls under a large title and
/// under the inline title it collapses to, kept in step.
@available(iOS 26.0, *)
final class SubtitleSegments: NSObject {
    let large = LabelledSegmentedControl(), inline = LabelledSegmentedControl()
    private(set) var tabs: [UInt32] = []
    private weak var host: NavigationHost?
    private weak var list: NodeView?
    /// The segment last chosen or set (LLP 1134 D3).
    private var settled = UISegmentedControl.noSegment
    /// `large` as the bar's item where the title shares its row (`place`), with no shared glass behind it: the
    /// segmented control draws its own track, and the shared background wrapped it in a second one.
    private(set) lazy var barItem: UIBarButtonItem = {
        let item = UIBarButtonItem(customView: large)
        item.hidesSharedBackground = true
        return item
    }()
    /// Whether the filter is that item rather than the subtitle views; nil before it is placed.
    private(set) var onBar: Bool?
    init(host: NavigationHost, route: RouteController) {
        self.host = host
        super.init()
        for control in [large, inline] { control.addTarget(self, action: #selector(changed(_:)), for: .valueChanged) }
        // A text size or Bold Text changes their height, and so the bar's.
        MainActor.assumeIsolated {
            large.registerForTraitChanges([UITraitPreferredContentSizeCategory.self, UITraitLegibilityWeight.self]) { [weak route] (_: UISegmentedControl, _: UITraitCollection) in
                route?.resampleTitleInsets()
            }
        }
    }

    /// D2, amended 2026-11-25: where UIKit sets a large title in one row with the bar's items (`titleShareRow`: an
    /// iPhone Duo, regular width), the filter is the last trailing item, beside the title; elsewhere it is the
    /// subtitle views. The bar's projection keeps the item (`filterBarItems`).
    func place(in c: RouteController) {
        let item = c.navigationItem, wide = c.titleShareRow
        if wide {
            if item.largeSubtitleView === large { item.largeSubtitleView = nil }
            if item.subtitleView === inline { item.subtitleView = nil }
            if !(item.rightBarButtonItems ?? []).contains(barItem) { item.rightBarButtonItems = (item.rightBarButtonItems ?? []) + [barItem] }
        } else {
            leaveBar(item)
            if item.largeSubtitleView == nil { item.largeSubtitleView = large }
            if item.subtitleView == nil { item.subtitleView = inline }
        }
        if onBar != wide { onBar = wide; c.resampleTitleInsets() }
    }

    func leaveBar(_ item: UINavigationItem) {
        if let items = item.rightBarButtonItems, items.contains(barItem) { item.rightBarButtonItems = items.filter { $0 !== barItem } }
    }

    /// The one the bar shows now: the bar's item, or under the large title while it is out.
    var shown: UISegmentedControl? {
        let visible = { (v: UIView) in v.window != nil && v.bounds.height > 1 && sequence(first: v, next: \.superview).allSatisfy { !$0.isHidden && $0.alpha > 0.01 } }
        return visible(large) ? large : visible(inline) ? inline : nil
    }

    func apply(_ list: NodeView) {
        self.list = list
        let members = list.container.subviews.compactMap { $0 as? NodeView }.filter { $0.style["display"]?.string != "none" }
        tabs = members.map(\.id)
        let titles = members.map { tab in if case .title(let words)? = tab.segmentFace { return words }; return tab.accessibleName }
        let selected = members.firstIndex { $0.props["accessibilitySelected"] == "true" } ?? UISegmentedControl.noSegment
        // A finger on either, or a choice UIKit has not reported yet, stays (`SegmentHost.sync`).
        let pending = [large, inline].contains { $0.isTracking || $0.selectedSegmentIndex != settled }
        for control in [large, inline] {
            // Rebuilt when no choice is pending: a pending one is read first.
            if !pending, control.numberOfSegments != titles.count || titles.indices.contains(where: { control.titleForSegment(at: $0) != titles[$0] }) {
                // Settled first: a change the removal raises is no person's.
                settled = UISegmentedControl.noSegment
                control.removeAllSegments()
                for (i, t) in titles.enumerated() { control.insertSegment(withTitle: t, at: i, animated: false) }
            }
            // The settled index first, so a change UIKit raises presses nothing.
            if !pending, control.selectedSegmentIndex != selected { settled = selected; control.selectedSegmentIndex = selected }
            control.accessibilityLabel = list.props["accessibilityLabel"]
            control.segmentIDs = members.map { $0.props["testId"] }
            control.segmentOwners = members.map(\.id)
        }
        if !pending, large.selectedSegmentIndex == selected { settled = selected }
    }

    /// A person's choice presses its tab now; its answer is read back.
    @objc private func changed(_ control: UISegmentedControl) {
        let i = control.selectedSegmentIndex
        guard i != settled else { return }
        settled = i
        for other in [large, inline] where other !== control { other.selectedSegmentIndex = i }
        guard tabs.indices.contains(i), let presenter = host?.presenter, let tab = presenter.views[tabs[i]], !tab.disabled, !tab.ariaUnavailable else { return }
        presenter.press(tab.id)
        if let list, presenter.views[list.id] === list { apply(list) }
    }
}

private var subtitleSegmentsKey: UInt8 = 0
extension RouteController {
    /// LLP 1135's controls, while the route's filter is them.
    @available(iOS 26.0, *)
    var subtitleSegments: SubtitleSegments? {
        get { objc_getAssociatedObject(self, &subtitleSegmentsKey) as? SubtitleSegments }
        set { objc_setAssociatedObject(self, &subtitleSegmentsKey, newValue, .OBJC_ASSOCIATION_RETAIN_NONATOMIC) }
    }

    /// The bar's height moves as a filter comes or goes in it (the scope bar, the subtitle views, their text size):
    /// the collapse scroller keeps its place (CSS `scrollTop`, read with the insets it had) and its insets are
    /// sampled afresh (`reportCovers`).
    func resampleTitleInsets() {
        guard let node = collapseScroll, let sv = collapseScrollView, sv === node.scrollView else { return }
        node.retainScrollPosition(from: sv)
        node.scrollOrigin = 0
    }

    /// The width class changed (a fold, a turn): a filter the bar projects may move (LLP 1134 and 1135, amended
    /// 2026-11-25).
    func widthClassChanged() {
        let filters = searchLayoutRead != nil || { if #available(iOS 26.0, *) { subtitleSegments != nil } else { false } }()
        if filters { host?.reprojectBarsNextTurn() }
    }

    /// Where UIKit has put the search field and the title: what `scope` read for the filter.
    var searchLayout: [Int] { [navigationItem.searchBarPlacement.rawValue, traitCollection.horizontalSizeClass.rawValue, inVerticalBar ? 1 : 0] }
    /// Whether a search's filter can be its scope bar (LLP 1134, amended 2026-11-25). Not at regular width where UIKit
    /// sets the field in the bar's row, as on an open iPhone Duo: it draws no scope bar there. Nor under a vertical
    /// bar (an iPhone Duo, open or closed): a scope bar shown at rest counts in no bar height there, so the list lay
    /// under it after a fold, and as the field collapsed on a scroll it hung over the rows with nothing behind it.
    var scopeBarFits: Bool {
        !(navigationItem.searchBarPlacement != .stacked && traitCollection.horizontalSizeClass == .regular) && !inVerticalBar
    }
    /// Whether UIKit sets a large title in one row with the bar's items: at regular width, and under a vertical bar.
    /// A filter under the title is a bar item there (LLP 1135, amended 2026-11-25).
    var titleShareRow: Bool { traitCollection.horizontalSizeClass == .regular || inVerticalBar }

    /// A layout UIKit changed under the bar moved where a projected filter belongs: the bars are projected again.
    func filterPlacementMoved() {
        if let read = searchLayoutRead, searchLayout != read { searchLayoutRead = nil; host?.reprojectBarsNextTurn(); return }
        if #available(iOS 26.0, *), let onBar = subtitleSegments?.onBar, onBar != titleShareRow { host?.reprojectBarsNextTurn() }
    }

    /// The filter's bar item, which the bar's projection keeps after the header's own trailing items.
    var filterBarItems: [UIBarButtonItem] {
        guard #available(iOS 26.0, *), let segments = subtitleSegments, segments.onBar == true else { return [] }
        return [segments.barItem]
    }
}
#endif
