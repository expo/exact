#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// LLP 1134: a filter tablist right after a header whose search field the
/// bar projects is that search bar's scope bar (`scripts/fixtures/search-scope.contract`).
final class SearchScopeIOSTests: XCTestCase {
    private var window: UIWindow?
    private var sessions: [ExactSession] = []

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }

    private func fixture() throws -> (ExactSession, RouteController) {
        let plan = try fixturePlan("SEARCH_SCOPE")
        let session = ExactApp.shared.makeSession(label: "search-scope")
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        until("the route's search is the bar's") { (session.presenter.navigationHost.primaryNavigation?.topViewController as? RouteController)?.search != nil }
        return (session, try XCTUnwrap(session.presenter.navigationHost.primaryNavigation?.topViewController as? RouteController))
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    private func state(_ session: ExactSession, _ slot: String) -> Any? {
        let json = try? JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any]
        return (json?["slots"] as? [String: Any])?[slot] ?? json?[slot]
    }

    func testAFilterAfterTheSearchIsItsScopeBar() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("the scope bar is the iPhone's (D5)") }
        let (session, route) = try fixture()
        guard !route.inVerticalBar else { throw XCTSkip("under a vertical bar the filter is the page's or a bar item (LLP 1134, 1135, amended)") }
        let p = session.presenter, filter = try node(session, "filter"), list = try node(session, "list")
        let search = try XCTUnwrap(route.search), bar = search.controller.searchBar
        until("the tabs are the scope buttons") { bar.scopeButtonTitles == ["All", "Mine", "Done"] }
        XCTAssertTrue(bar.showsScopeBar)
        XCTAssertEqual(search.controller.scopeBarActivation, .manual, "shown whether or not the search is active")
        XCTAssertEqual(bar.selectedScopeButtonIndex, 0)
        XCTAssertTrue(filter.isHidden, "the tablist is the bar's")
        XCTAssertTrue(p.navigationHost.hiddenForBar(filter))
        XCTAssertFalse(p.segments.projects(filter.id), "no segmented control of its own")
        XCTAssertTrue(route.collapseScroll === list, "the list after it is the bar's collapse scroller")
        until("it takes no room: the list starts where the tablist did") { abs(list.frame.minY - filter.frame.minY) < 0.5 }
        // D4: the agent aims at a scope segment (its test id, read with accessibility on, below).
        XCTAssertNotNil(p.segments.drawn(try node(session, "filter-done")), "a tap aims at the scope segment")
        let tab = try node(session, "filter-done"), agent = Agent(session: session)
        let row = try XCTUnwrap((agent.layout()["nodes"] as? [[String: Any]])?.first { $0["id"] as? Int == Int(tab.id) })
        XCTAssertNil(row["radii"], "a native segment has no authored corner geometry")
        let target = try XCTUnwrap(agent.layout(["id": Int(tab.id)])["node"] as? [String: Any])
        XCTAssertNil(target["radii"], "list and targeted layout omit the same native corners")


        // D3: a person's choice presses the tab, once.
        bar.selectedScopeButtonIndex = 2
        bar.delegate?.searchBar?(bar, selectedScopeButtonIndexDidChange: 2)
        until("the press chose Done") { state(session, "filter") as? String == "done" }
        XCTAssertEqual(state(session, "presses") as? Int, 1)
        XCTAssertEqual(bar.selectedScopeButtonIndex, 2)
        // The Contract's selection moves the segment, and presses nothing more.
        p.press(try node(session, "filter-mine").id)
        until("the segment follows the Contract") { bar.selectedScopeButtonIndex == 1 }
        XCTAssertEqual(state(session, "presses") as? Int, 2)
        // While the search is active, with the keyboard up, the text and the responder stay.
        search.controller.isActive = true
        until("the search is active") { search.controller.isActive }
        bar.text = "dune"
        bar.delegate?.searchBar?(bar, textDidChange: "dune")
        XCTAssertTrue(bar.searchTextField.becomeFirstResponder())
        p.press(try node(session, "filter-all").id)
        until("the segment moves while the search is active") { bar.selectedScopeButtonIndex == 0 }
        XCTAssertTrue(bar.searchTextField.isFirstResponder, "the keyboard stays")
        XCTAssertEqual(bar.text, "dune")
        XCTAssertTrue(search.controller.isActive)
        XCTAssertTrue(bar.showsScopeBar, "still shown while active")
        search.controller.isActive = false
        until("the search is dismissed") { !search.controller.isActive }
        until("still shown after the search") { bar.showsScopeBar }

        // A press the Contract declines puts the segment back.
        p.press(try node(session, "pick-stay").id)
        until("still the scope bar") { bar.scopeButtonTitles?.count == 3 }
        let before = state(session, "presses") as? Int ?? 0
        bar.selectedScopeButtonIndex = 2
        bar.delegate?.searchBar?(bar, selectedScopeButtonIndexDidChange: 2)
        until("the declined choice goes back") { bar.selectedScopeButtonIndex == 0 }
        XCTAssertEqual(state(session, "presses") as? Int, before + 1)
        XCTAssertEqual(state(session, "filter") as? String, "all")
    }

    func testScopeAccessibilityElementsJoinTheirAuthoredTabs() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit's scope segments need its accessibility runtime") }
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("the scope bar is the iPhone's (D5)") }
        let (session, route) = try fixture()
        guard !route.inVerticalBar else { throw XCTSkip("under a vertical bar the filter is the page's or a bar item (LLP 1134, 1135, amended)") }
        let search = try XCTUnwrap(route.search)
        until("the scope segments carry their tabs' identifiers") { search.scopeControl?.accessibilitySegments.map(\.readIdentifier) == ["filter-all", "filter-mine", "filter-done"] }
        let control = try XCTUnwrap(search.scopeControl)
        for (segment, name) in zip(control.accessibilitySegments, ["All", "Mine", "Done"]) {
            XCTAssertTrue(segment.isAccessibilityElement)
            XCTAssertEqual(segment.accessibilityLabel, name)
            let testId = try XCTUnwrap(segment.readIdentifier)
            let tab = try node(session, testId)
            let reply = Agent(session: session).accessibilityElementsTree(["target": testId])
            let ax = try XCTUnwrap(reply["ax"] as? [String: Any])
            let elements = try XCTUnwrap(ax["elements"] as? [[String: Any]])
            XCTAssertEqual(elements.count, 1, testId)
            let element = try XCTUnwrap(elements.first)
            XCTAssertEqual(element["id"] as? UInt32, tab.id)
            XCTAssertEqual(element["testId"] as? String, testId)
            XCTAssertEqual(element["name"] as? String, name)
            XCTAssertEqual(element["via"] as? String, "owner")
        }
    }

    /// LLP 1135: with no search, the filter under a large title is the
    /// item's subtitle views, under the large title and under the inline one.
    func testAFilterUnderALargeTitleIsItsSubtitleViews() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("the iPhone's (LLP 1135 D4)") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("the subtitle views are iOS 26's") }
        let (session, route) = try fixture()
        guard !route.inVerticalBar else { throw XCTSkip("under a vertical bar the filter is the page's or a bar item (LLP 1134, 1135, amended)") }
        let p = session.presenter, filter = try node(session, "filter"), list = try node(session, "list")
        p.press(try node(session, "pick-plain").id)
        until("the filter is the subtitle views") { route.navigationItem.largeSubtitleView is UISegmentedControl && route.navigationItem.subtitleView is UISegmentedControl }
        let large = try XCTUnwrap(route.navigationItem.largeSubtitleView as? UISegmentedControl)
        let inline = try XCTUnwrap(route.navigationItem.subtitleView as? UISegmentedControl)
        for control in [large, inline] {
            XCTAssertEqual((0..<control.numberOfSegments).map { control.titleForSegment(at: $0) }, ["All", "Mine", "Done"])
            XCTAssertEqual(control.selectedSegmentIndex, 0)
        }
        XCTAssertNil(route.search?.controller.searchBar.scopeButtonTitles, "no scope bar: there is no search")
        XCTAssertTrue(filter.isHidden && p.navigationHost.hiddenForBar(filter), "the tablist is the bar's")
        XCTAssertFalse(p.segments.projects(filter.id))
        until("the list is the collapse scroller") { route.collapseScroll === list }
        // A choice under the inline title presses its tab once and sets the other.
        let before = state(session, "presses") as? Int ?? 0
        inline.selectedSegmentIndex = 2
        inline.sendActions(for: .valueChanged)
        until("Done is chosen") { state(session, "filter") as? String == "done" }
        XCTAssertEqual(state(session, "presses") as? Int, before + 1)
        XCTAssertEqual(large.selectedSegmentIndex, 2, "the other follows")
        // The Contract's selection moves both.
        p.press(try node(session, "filter-mine").id)
        until("both follow the Contract") { large.selectedSegmentIndex == 1 && inline.selectedSegmentIndex == 1 }
        // A search back: the scope bar, and the subtitle views go.
        p.press(try node(session, "pick-scope").id)
        until("the subtitle views go") { route.navigationItem.largeSubtitleView == nil && route.navigationItem.subtitleView == nil }
        until("the scope bar is back") { route.search?.controller.searchBar.scopeButtonTitles?.count == 3 }
    }

    /// LLP 1134, amended: where UIKit sets the search field in the bar's row (regular width: an open iPhone Duo), it
    /// draws no scope bar, so the filter stays the page's; at compact width it is the scope bar again.
    func testAFilterIsNoScopeBarWhereTheFieldIsInTheBarsRow() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("the scope bar is the iPhone's (D5)") }
        let (session, route) = try fixture()
        guard !route.inVerticalBar else { throw XCTSkip("under a vertical bar the filter is the page's or a bar item (LLP 1134, 1135, amended)") }
        let p = session.presenter, filter = try node(session, "filter"), bar = try XCTUnwrap(route.search).controller.searchBar
        let window = try XCTUnwrap(self.window)
        until("the scope bar first") { bar.scopeButtonTitles?.count == 3 && filter.isHidden }
        window.traitOverrides.horizontalSizeClass = .regular
        until("UIKit sets the field in the bar's row") { !route.scopeBarFits }
        until("the tablist is the page's") { bar.scopeButtonTitles == nil && !bar.showsScopeBar && !filter.isHidden && p.segments.projects(filter.id) }
        XCTAssertFalse(p.navigationHost.hiddenForBar(filter))
        window.traitOverrides.horizontalSizeClass = .compact
        until("the scope bar again") { bar.scopeButtonTitles?.count == 3 && filter.isHidden && !p.segments.projects(filter.id) }
    }

    /// LLP 1135 D2, amended: at regular width the filter is the bar's last trailing item, beside the large title,
    /// and at compact width the subtitle views again.
    func testAFilterUnderALargeTitleIsABarItemWhereTheBarIsWide() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("the iPhone's (LLP 1135 D4)") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("the subtitle views are iOS 26's") }
        let (session, route) = try fixture()
        guard !route.inVerticalBar else { throw XCTSkip("under a vertical bar the filter is the page's or a bar item (LLP 1134, 1135, amended)") }
        let p = session.presenter, filter = try node(session, "filter"), item = route.navigationItem
        let window = try XCTUnwrap(self.window)
        p.press(try node(session, "pick-plain").id)
        until("the subtitle views first") { item.largeSubtitleView is UISegmentedControl }
        let segmentedItems = { (item.rightBarButtonItems ?? []).filter { $0.customView is UISegmentedControl } }
        window.traitOverrides.horizontalSizeClass = .regular
        until("the filter is a bar item") { segmentedItems().count == 1 && item.largeSubtitleView == nil && item.subtitleView == nil }
        XCTAssertTrue(item.rightBarButtonItems?.last === segmentedItems().first, "the last trailing item, nearest the title")
        let control = try XCTUnwrap(segmentedItems().first?.customView as? UISegmentedControl)
        XCTAssertEqual((0..<control.numberOfSegments).map { control.titleForSegment(at: $0) }, ["All", "Mine", "Done"])
        XCTAssertTrue(filter.isHidden && p.navigationHost.hiddenForBar(filter), "the tablist is the bar's")
        // A choice there presses its tab once; the Contract's selection moves it.
        let before = state(session, "presses") as? Int ?? 0
        control.selectedSegmentIndex = 2
        control.sendActions(for: .valueChanged)
        until("Done is chosen") { state(session, "filter") as? String == "done" }
        XCTAssertEqual(state(session, "presses") as? Int, before + 1)
        p.press(try node(session, "filter-mine").id)
        until("it follows the Contract") { control.selectedSegmentIndex == 1 }
        window.traitOverrides.horizontalSizeClass = .compact
        until("the subtitle views again, and no bar item") { segmentedItems().isEmpty && item.largeSubtitleView is UISegmentedControl && item.subtitleView is UISegmentedControl }
    }

    /// A reselect from down the list comes back to its top with the title and its filter open, the scope bar's
    /// and the subtitle views': not short of them by what collapsed (a closed iPhone Duo's, where the title itself
    /// keeps one row), nor past them by an inset sampled under another layout of the bar.
    func testAReselectFromDownTheListOpensTheTitleAndItsFilter() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("the iPhone's bar") }
        let (session, route) = try fixture()
        let p = session.presenter, host = p.navigationHost, list = try node(session, "list"), sv = try XCTUnwrap(list.scrollView)
        let atTop = { abs(sv.contentOffset.y + sv.adjustedContentInset.top) < 0.5 }
        for shape in ["scope", "plain"] {
            p.press(try node(session, "pick-\(shape)").id)
            until("\(shape): the list rests at its top under the bar") { route.collapseScroll === list && sv.adjustedContentInset.top > 0 && atTop() && !Agent(session: session).nativeInFlight() }
            let open = sv.adjustedContentInset.top
            list.pendingScrollTop = 600
            list.applyPendingScroll()
            list.scrollOrigin = open + 50 // an inset sampled under a taller layout of the bar
            until("\(shape): the list is down") { abs(sv.contentOffset.y + sv.adjustedContentInset.top - 600) < 0.5 && !Agent(session: session).nativeInFlight() }
            host.scrollToTop(route)
            until("\(shape): back at its top, the bar open") { atTop() && abs(sv.adjustedContentInset.top - open) < 0.5 && !sv.isDecelerating && !Agent(session: session).nativeInFlight() }
            XCTAssertEqual(sv.adjustedContentInset.top, open, accuracy: 0.5, "\(shape): the bar's inset; offset \(sv.contentOffset.y), origin \(list.scrollOrigin)")
            XCTAssertTrue(atTop(), "\(shape): offset \(sv.contentOffset.y), inset \(sv.adjustedContentInset.top), origin \(list.scrollOrigin), decelerating \(sv.isDecelerating), in flight \(Agent(session: session).nativeInFlight())")
        }
    }

    /// LLP 1070.000 §2.6: a reselect gives way to a scroll the tab's press asked of the list, a `scrollTop` write
    /// here, which lands within the press; one whose press asked nothing goes to the top.
    func testAReselectGivesWayToAScrollTopThePressWrote() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        let (session, route) = try fixture()
        let host = session.presenter.navigationHost, list = try node(session, "list"), sv = try XCTUnwrap(list.scrollView)
        let css = { sv.contentOffset.y + sv.adjustedContentInset.top }
        let still = { !sv.isDecelerating && !Agent(session: session).nativeInFlight() }
        until("the list is under the bar") { route.collapseScroll === list && sv.adjustedContentInset.top > 0 && still() }
        list.pendingScrollTop = 600
        list.applyPendingScroll()
        until("the list is down") { abs(css() - 600) < 0.5 && still() }
        let asks = session.presenter.scrollAskCount
        list.applyProps(set: ["scrollTop": "200"], clear: []) // the press's write
        list.applyPendingScroll()
        host.scrollToTop(route, asks: asks)
        // The reselect decides in the call: it starts no scroll
        let animating: Bool = { if #available(iOS 17.4, *) { return sv.isScrollAnimating }; return false }()
        XCTAssertFalse(animating, "no scroll to the top began")
        XCTAssertEqual(css(), 200, accuracy: 0.5, "the press's write stands")
        host.scrollToTop(route, asks: session.presenter.scrollAskCount)
        until("a reselect whose press asked nothing goes to the top") { abs(css()) < 0.5 && still() }
    }

    /// A reselect whose bar changes in flight (a filter moving into it, a fold) comes to rest at the top under the
    /// bar as it is when the scroll lands, not where the top was when it began.
    func testAReselectRestsAtTheTopUnderTheBarAsItIsWhenItLands() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        let (session, route) = try fixture()
        let host = session.presenter.navigationHost, list = try node(session, "list"), sv = try XCTUnwrap(list.scrollView)
        let root = try XCTUnwrap(window?.rootViewController)
        defer { root.additionalSafeAreaInsets = .zero }
        let still = { !sv.isDecelerating && !Agent(session: session).nativeInFlight() }
        until("the list is under the bar") { route.collapseScroll === list && sv.adjustedContentInset.top > 0 && still() }
        let open = sv.adjustedContentInset.top
        list.pendingScrollTop = 600
        list.applyPendingScroll()
        until("the list is down") { abs(sv.contentOffset.y + sv.adjustedContentInset.top - 600) < 0.5 && still() }
        host.scrollToTop(route)
        if #available(iOS 17.4, *) { XCTAssertTrue(sv.isScrollAnimating, "the reselect animates, and the bar moves in flight") }
        root.additionalSafeAreaInsets = UIEdgeInsets(top: 30, left: 0, bottom: 0, right: 0)
        // How far the bar moves for the root's safe area is the device's (6 points under the iPhone Duo's vertical bar)
        until("at its top under the lower bar") {
            sv.adjustedContentInset.top > open + 0.5 && abs(sv.contentOffset.y + sv.adjustedContentInset.top) < 0.5 && still()
        }
    }

    /// LLP 1134, amended: under a vertical bar (an iPhone Duo) a filter after the search stays the page's, with no
    /// scope buttons: UIKit would count a scope bar shown there in no bar height, and leave it over the rows.
    func testUnderAVerticalBarAFilterAfterTheSearchStaysThePages() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        let (session, route) = try fixture()
        guard route.inVerticalBar else { throw XCTSkip("no vertical bar here (an iPhone Duo places one)") }
        let p = session.presenter, filter = try node(session, "filter"), bar = try XCTUnwrap(route.search).controller.searchBar
        until("the tablist is the page's") { bar.scopeButtonTitles == nil && !bar.showsScopeBar && !filter.isHidden && p.segments.projects(filter.id) }
    }

    /// LLP 1135, amended: under a vertical bar a large title shares its row with the bar's items, and a filter
    /// under it is the bar's last trailing item, not the subtitle views.
    func testUnderAVerticalBarAFilterUnderALargeTitleIsABarItem() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("the subtitle views are iOS 26's") }
        let (session, route) = try fixture()
        guard route.inVerticalBar else { throw XCTSkip("no vertical bar here (an iPhone Duo places one)") }
        let item = route.navigationItem
        session.presenter.press(try node(session, "pick-plain").id)
        until("the filter is the bar's last item") { item.rightBarButtonItems?.last?.customView is UISegmentedControl && item.largeSubtitleView == nil && item.subtitleView == nil }
    }

    /// A filter that moves into the bar or out of it as the width class changes (a fold) leaves the list at its top
    /// with its first row below all the bar draws: a scope bar shown at rest that UIKit counted in no bar height
    /// left the row under its segments.
    func testAFilterMovingWithTheWidthLeavesTheListsFirstRowUncovered() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("the iPhone's bar") }
        let (session, route) = try fixture()
        let list = try node(session, "list"), sv = try XCTUnwrap(list.scrollView), top = try node(session, "list-top"), window = try XCTUnwrap(self.window)
        let nav = try XCTUnwrap(route.navigationController)
        let settled = { abs(sv.contentOffset.y + sv.adjustedContentInset.top) < 0.5 && !Agent(session: session).nativeInFlight() }
        // The lowest edge of what the bar draws: the bar, and the scope control where UIKit hangs it below the bar.
        let barEnd = { () -> CGFloat in
            let scope = route.search?.scopeControl.flatMap { $0.window == nil ? nil : $0.convert($0.bounds, to: nil).maxY } ?? 0
            return max(nav.navigationBar.convert(nav.navigationBar.bounds, to: nil).maxY, scope)
        }
        let bar = try XCTUnwrap(route.search).controller.searchBar
        for width in [UIUserInterfaceSizeClass.regular, .compact] {
            window.traitOverrides.horizontalSizeClass = width
            // The bars projected again for the width: the filter is the scope bar exactly where one fits
            until("\(width.rawValue): the filter moved, and the list rests at its top") {
                route.traitCollection.horizontalSizeClass == width && (bar.scopeButtonTitles != nil) == route.scopeBarFits
                    && route.collapseScroll === list && sv.adjustedContentInset.top > 0 && settled()
            }
            XCTAssertGreaterThanOrEqual(top.convert(top.bounds, to: nil).minY, barEnd() - 0.5, "\(width.rawValue): the first row is below the bar")
        }
    }

    /// The bar's search field takes its node's own `accent-color` as UIKit's field's tint, which
    /// UIKit does not carry to the bar: its text, placeholder and caret in the tint resolve by it
    func testASearchFieldTakesItsOwnAccentAsItsTint() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        let (session, route) = try fixture()
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        let field = try XCTUnwrap(route.navigationItem.searchController).searchBar.searchTextField
        let source = try node(session, "search")
        func project() { session.presenter.navigationHost.prepareRoutes(nav.viewControllers.compactMap { $0 as? RouteController }, in: nav) }
        func rgb() -> [CGFloat] {
            var (r, g, b, a): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
            _ = field.tintColor.resolvedColor(with: field.traitCollection).getRed(&r, green: &g, blue: &b, alpha: &a)
            return [r, g, b]
        }
        let green: BatchValue = .array([.number(0), .number(200), .number(0), .number(255)])
        source.applyStyle(source.style.merging(["accent_color": green]) { $1 })
        project()
        for (got, want) in zip(rgb(), [0, 200.0 / 255, 0]) { XCTAssertEqual(got, want, accuracy: 0.01, "the node's own green: \(rgb())") }
        source.applyStyle(source.style.filter { $0.key != "accent_color" })
        project()
        XCTAssertNotEqual(rgb(), [0, 200.0 / 255, 0], "the bar's again")
    }

    func testAFilterThatIsNotAScopeKeepsItsSegmentedControl() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bar is a scene window's") }
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("the scope bar is the iPhone's (D5)") }
        let (session, route) = try fixture()
        guard !route.inVerticalBar else { throw XCTSkip("under a vertical bar the filter is the page's or a bar item (LLP 1134, 1135, amended)") }
        let p = session.presenter, filter = try node(session, "filter"), bar = try XCTUnwrap(route.search).controller.searchBar
        until("the scope bar first") { bar.scopeButtonTitles?.count == 3 }
        for shape in ["image", "menu", "panel", "painted", "faded", "caption", "six"] {
            p.press(try node(session, "pick-\(shape)").id)
            until("\(shape): the scope bar goes") { bar.scopeButtonTitles == nil && !bar.showsScopeBar }
            until("\(shape): the tablist is the page's again") { !filter.isHidden && p.segments.projects(filter.id) }
            XCTAssertFalse(p.navigationHost.hiddenForBar(filter), shape)
            p.press(try node(session, "pick-scope").id)
            until("\(shape) → scope: the scope bar again") { bar.scopeButtonTitles?.count == 3 && filter.isHidden && !p.segments.projects(filter.id) }
        }
    }
}
#endif
