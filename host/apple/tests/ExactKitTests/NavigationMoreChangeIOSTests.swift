#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// UIKit changing which tabs its bar and More hold, within its container's trait change (LLP 1075.003 §9.16): laid out or
/// sized before the end, the container can keep a More that UIKit let go, as a tab.
extension NavigationMoreIOSTests {
    /// UIKit moves a tab's root into More, and out again, between the start and the end of what Exact takes for its change,
    /// and leaves the app's tabs, and the router's tab, as they were.
    func testUIKitMovesATabsRoutesWithinItsContainersTraitChange() throws {
        final class Moved: UIViewController {
            weak var host: NavigationHost?
            var changes: [Int] = []
            override func willMove(toParent parent: UIViewController?) {
                super.willMove(toParent: parent)
                if parent != nil, let tabs = host?.tabController as? TabController { changes.append(tabs.changes) }
            }
        }
        let (session, tabs, more) = try moreFixture("more-trait-change")
        let navigation = session.presenter.navigationHost, all = tabs.viewControllers ?? []
        tapTab(tabs, of: more)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        let root = try XCTUnwrap(more.stackRoutes.first as? RouteController)
        let settled = { !navigation.inTransition && !navigation.syncScheduled }
        waitFor("the router's projection", settled)
        for (width, holder) in [(UIUserInterfaceSizeClass.regular, { all[4] }), (.compact, { tabs.moreNavigationController })] {
            if width == .regular { try showAllTabs(tabs) } else { setSizeClasses(tabs, horizontal: width) }
            waitFor("UIKit moves Five's root, and Exact follows") { root.navigationController === holder() && settled() }
            XCTAssertTrue(tabs.selectedViewController === all[4], "UIKit stays on Five")
            XCTAssertTrue(tabs.viewControllers?.elementsEqual(all, by: ===) == true, "the app's tabs, and no More among them")
            XCTAssertEqual(navigation.routerTab, 4)
        }
        // A controller of the test's own in the last tab's place, which says when UIKit moves it
        setSizeClasses(tabs, horizontal: .regular)
        let moved = Moved(), stack = UINavigationController(rootViewController: moved)
        tabs.setViewControllers(all.dropLast() + [stack], animated: false)
        tabs.selectedViewController = stack
        tabs.view.layoutIfNeeded()
        moved.host = navigation
        for (width, holder) in [(UIUserInterfaceSizeClass.compact, { tabs.moreNavigationController }), (.regular, { stack })] {
            moved.changes = []
            setSizeClasses(tabs, horizontal: width)
            XCTAssertTrue(moved.navigationController === holder(), "UIKit moved the root")
            XCTAssertFalse(moved.changes.isEmpty || moved.changes.contains(0), "within the change: \(moved.changes)")
            XCTAssertEqual((tabs as? TabController)?.changes, 0, "which is over")
        }
    }

    /// A pushed route that hides the tab bar goes with its tab into More and out of it, and hides the bar there; back shows it.
    func testARouteThatHidesTheTabBarMovesWithItsTab() throws {
        let (session, tabs, more, full) = try fullUnderMore("more-change-full")
        let navigation = session.presenter.navigationHost, all = tabs.viewControllers ?? [], five = try XCTUnwrap(all[4] as? UINavigationController)
        let root = try XCTUnwrap(more.stackRoutes.first as? RouteController)
        XCTAssertNil(tabs.shownTabBar, "Full hides the tab bar")
        // (UIKit makes another More once its bar has shown every tab)
        let inMore = { tabs.moreNavigationController }, inFive = { five }
        try showAllTabs(tabs)
        for (width, holder) in [(UIUserInterfaceSizeClass.regular, inFive), (.compact, inMore), (.regular, inFive)] {
            setSizeClasses(tabs, horizontal: width)
            XCTAssertTrue(holder().topViewController === full, "Full is on top as the move ends")
            waitFor("Exact follows") { settled(navigation) }
            XCTAssertTrue(holder().topViewController === full, "and stays there")
            XCTAssertTrue(full.hidesBottomBarWhenPushed, "as the app's tablist asks")
            XCTAssertNil(tabs.shownTabBar, "and hides the tab bar")
            XCTAssertEqual(navigation.selectedKey, full.key, "the router stays on it")
            XCTAssertTrue(tabs.viewControllers?.elementsEqual(all, by: ===) == true, "the app's tabs, and no More among them")
        }
        // A Back in the turn of a move is one still
        five.popViewController(animated: false)
        setSizeClasses(tabs, horizontal: .compact)
        waitFor("the router follows back to Five's root") { navigation.selectedKey == root.key && settled(navigation) }
        XCTAssertEqual(navigation.routerTab, 4)
        XCTAssertTrue(inMore().stackRoutes.elementsEqual([root], by: ===), "which More holds alone")
        XCTAssertNotNil(tabs.shownTabBar, "and the tab bar is back")
        XCTAssertFalse(navigation.tablistHidBar)
    }

    /// Only a route UIKit drops with a move of its tab is no Back: a pop of a stack whose root stays is one, followed as it is
    /// made, and so is a pop made before the move. A tab that moves twice in one change keeps its route, and one pushed in it.
    func testOnlyARouteDroppedWithItsTabsMoveIsNoBack() throws {
        let (session, tabs, _) = try moreFixture("more-change-stays")
        let navigation = session.presenter.navigationHost, controller = try XCTUnwrap(tabs as? TabController)
        let home = try XCTUnwrap(tabs.tabControllers?[0] as? UINavigationController), homeRoot = try XCTUnwrap(navigation.selectedKey)
        try tapNode(session, "open-detail")
        waitFor("Detail pushed in Home") { home.viewControllers.count == 2 && settled(navigation) }
        let back = try node(session, "back-detail").id
        navigation.whileTabsChange(controller) {
            home.popViewController(animated: false)
            home.view.layoutIfNeeded()
            XCTAssertEqual(navigation.selectedKey, homeRoot, "a pop of a stack that stays is followed as it is made")
        }
        waitFor("Exact settles") { settled(navigation) }
        XCTAssertEqual(journal(session, "press view \(back) (back)"), 1, "once")

        // Twice in one change, out of More and back
        let (twice, twiceTabs, _, full) = try fullUnderMore("more-change-twice")
        let twiceNavigation = twice.presenter.navigationHost, twiceController = try XCTUnwrap(twiceTabs as? TabController)
        let five = try XCTUnwrap(twiceTabs.tabControllers?[4] as? UINavigationController)
        try showAllTabs(twiceTabs)
        setSizeClasses(twiceTabs, horizontal: .compact)
        waitFor("Full in More") { twiceTabs.moreNavigationController.topViewController === full && settled(twiceNavigation) }
        twiceNavigation.whileTabsChange(twiceController) {
            setSizeClasses(twiceTabs, horizontal: .regular)
            setSizeClasses(twiceTabs, horizontal: .compact)
        }
        waitFor("Exact settles") { settled(twiceNavigation) }
        XCTAssertTrue(twiceTabs.moreNavigationController.topViewController === full, "Full is on top in More, as before")
        XCTAssertEqual(twiceNavigation.selectedKey, full.key, "and the router never left it")

        // Deeper, over Full, popped unheard within a change, and then the tab moves
        twice.presenter.press(try node(twice, "open-deeper").id)
        waitFor("Deeper over Full") { twiceTabs.moreNavigationController.stackRoutes.count == 3 && settled(twiceNavigation) }
        let deeperBack = try node(twice, "back-deeper").id, stack = twiceTabs.moreNavigationController, delegate = stack.delegate
        twiceNavigation.whileTabsChange(twiceController) {
            stack.delegate = nil
            stack.popViewController(animated: false)
            stack.delegate = delegate
            setSizeClasses(twiceTabs, horizontal: .regular)
        }
        waitFor("the router follows back to Full") { twiceNavigation.selectedKey == full.key && settled(twiceNavigation) }
        XCTAssertEqual(journal(twice, "press view \(deeperBack) (back)"), 1, "one Back")
        XCTAssertTrue(five.topViewController === full && five.viewControllers.count == 2, "Five's own controller shows Full over its root")

        // Full pushed within a change, and then the tab moves
        let (late, lateTabs, lateMore) = try moreFixture("more-change-late")
        let lateNavigation = late.presenter.navigationHost
        tapTab(lateTabs, of: lateMore)
        pickRow(lateTabs, 4)
        waitFor("the router follows Five, whose root More holds") { lateNavigation.routerTab == 4 && lateMore.stackRoutes.count == 1 && settled(lateNavigation) }
        let openFull = try node(late, "open-full").id, lateFive = try XCTUnwrap(lateTabs.tabControllers?[4] as? UINavigationController)
        lateNavigation.whileTabsChange(try XCTUnwrap(lateTabs as? TabController)) {
            late.presenter.press(openFull)
            lateTabs.view.layoutIfNeeded()
            XCTAssertEqual(lateMore.stackRoutes.count, 2, "Full is over Five's root in More")
            setSizeClasses(lateTabs, horizontal: .regular)
        }
        waitFor("Exact settles") { settled(lateNavigation) }
        XCTAssertEqual((lateFive.topViewController as? RouteController)?.key, lateNavigation.selectedKey, "Full is on top in Five's own controller")
        XCTAssertEqual(lateFive.viewControllers.count, 2)

        // A change within UIKit's own, while UIKit has Five's root between its stacks
        let (within, withinTabs, withinMore, top) = try fullUnderMore("more-change-within")
        let withinNavigation = within.presenter.navigationHost, withinController = try XCTUnwrap(withinTabs as? TabController)
        let root = try XCTUnwrap(withinMore.stackRoutes.first as? RouteController)
        let app = MoveHeard(), proxy = try XCTUnwrap(withinMore.delegate as? NavigationDelegateProxy)
        var between = false
        app.before = {
            guard !between, !root.inStack else { return }
            between = true
            withinNavigation.whileTabsChange(withinController) { XCTAssertFalse(withinNavigation.followPops(), "Full is no Back") }
        }
        proxy.give(app)
        defer { proxy.give(nil) }
        setSizeClasses(withinTabs, horizontal: .regular)
        waitFor("Exact settles") { settled(withinNavigation) }
        XCTAssertEqual(withinNavigation.selectedKey, top.key, "the router never left Full")
        XCTAssertTrue((withinTabs.tabControllers?[4] as? UINavigationController)?.topViewController === top)
        guard between else { throw XCTSkip("UIKit tells More's delegate of the pop after it has put the root back") }
    }

    /// A route UIKit drops with a move of its tab is written back once the change is over, with no animation, and again where
    /// UIKit declines that.
    func testARouteDroppedWithItsTabsMoveIsWrittenBack() throws {
        let (session, tabs, _, full) = try fullUnderMore("more-change-declined")
        let five = try XCTUnwrap(tabs.tabControllers?[4] as? UINavigationController), navigation = session.presenter.navigationHost
        // (Exact's writes, not UIKit's own of the routes it moves)
        var animated: [Bool] = [], within: [Int] = []
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        try withNavigationStackWrites(shouldRefuse: { nav, routes in navigation.syncing && nav === five && routes.last === full && animated.count == 1 },
                                      observe: { nav, routes, wanted in
            guard navigation.syncing, nav === five, routes.last === full else { return }
            animated.append(wanted)
            within.append((tabs as? TabController)?.changes ?? 0)
        }, {
            try showAllTabs(tabs)
            waitFor("Full is over Five's root in its own controller") { five.topViewController === full && settled(navigation) }
        })
        XCTAssertEqual(navigation.selectedKey, full.key, "the router never left it")
        if !animated.isEmpty {
            XCTAssertGreaterThan(animated.count, 1, "written again, once declined")
            XCTAssertEqual(animated.first, false)
            XCTAssertFalse(within.contains { $0 > 0 }, "none within UIKit's change")
        }

        // UIKit's pop to More's list, which puts Five's root in its own controller alone (LLP 1075.003 §9.16), within a change
        // that a projection comes in, and one that leaves no other work to project
        for projects in [true, false] {
            let (session, tabs, more, full) = try fullUnderMore("more-change-list-\(projects)")
            let navigation = session.presenter.navigationHost, controller = try XCTUnwrap(tabs as? TabController)
            let five = try XCTUnwrap(tabs.tabControllers?[4] as? UINavigationController), root = try XCTUnwrap(more.stackRoutes.first)
            navigation.whileTabsChange(controller) {
                more.popToRootViewController(animated: false)
                more.view.layoutIfNeeded()
                XCTAssertTrue(five.viewControllers.elementsEqual([root], by: ===), "UIKit has put Five's root in its own controller")
                if projects { navigation.resync() }
                XCTAssertFalse(full.inStack, "no projection writes Full within the change")
                if !projects {
                    if controller.left.fit { session.presenter.owners.endViewChange() }
                    controller.left = LeftToTabsChange()
                }
            }
            XCTAssertTrue(full.inStack, "Full is back as the change ends")
            waitFor("Exact settles") { settled(navigation) }
            XCTAssertEqual(navigation.selectedKey, full.key, "the router never left it")
        }
        guard !animated.isEmpty else { throw XCTSkip("UIKit keeps the route as it moves the tab here") }
    }

    private func settled(_ navigation: NavigationHost) -> Bool {
        !navigation.inTransition && !navigation.syncScheduled && navigation.restDifference == nil
    }

    /// Full, which hides the tab bar, over Five's root in More.
    private func fullUnderMore(_ label: String) throws -> (ExactSession, UITabBarController, UINavigationController, RouteController) {
        let (session, tabs, more) = try moreFixture(label)
        let navigation = session.presenter.navigationHost
        tapTab(tabs, of: more)
        pickRow(tabs, 4)
        waitFor("the router follows Five, whose root More holds") { navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil }
        try tapNode(session, "open-full")
        waitFor("Full pushed onto More") { more.stackRoutes.count == 2 && more.transitionCoordinator == nil && settled(navigation) }
        return (session, tabs, more, try XCTUnwrap(more.topViewController as? RouteController))
    }

    /// While UIKit changes its tabs, a transition's end leaves its fit, a projection its layout of the container, and a report
    /// of the covers itself, to the end; the app's delegate and the rest of the projection run as they come.
    func testExactLaysOutItsTabContainerOnceUIKitHasChangedItsTabs() throws {
        final class Laid: UIView {
            var times = 0, laying = false, during: () -> Void = {}
            override func layoutSubviews() {
                super.layoutSubviews()
                times += 1
                laying = true
                during()
                laying = false
            }
        }
        let session = try fixture("more-change", wholeScreen: true)
        let presenter = session.presenter, navigation = presenter.navigationHost, tabs = try XCTUnwrap(navigation.tabController)
        let home = try XCTUnwrap(tabs.selectedViewController as? UINavigationController), root = try XCTUnwrap(home.topViewController as? RouteController)
        let app = MoveHeard(), proxy = try XCTUnwrap(home.delegate as? NavigationDelegateProxy)
        proxy.give(app)
        waitForMainQueue()
        let laid = Laid(), full = tabs.view.frame, reported = presenter.onCovers
        tabs.view.addSubview(laid)
        var laidOutAsTold: [Bool] = [], times = 0, settled = full
        presenter.onCovers = { [unowned tabs] in
            laidOutAsTold.append(tabs.view.frame == settled && laid.times > times && !laid.laying)
            reported?($0)
        }
        laid.during = { [weak navigation] in navigation?.reportCovers() }
        defer {
            presenter.onCovers = reported
            proxy.give(nil)
            laid.removeFromSuperview()
        }
        let block = { presenter.owners.told[root.node.id]?.block }, whole = block(), page = presenter.viewport.frame
        for (fits, projects) in [(true, false), (false, true), (true, true)] {
            tabs.view.frame = CGRect(x: full.minX, y: full.minY, width: full.width, height: full.height - 100)
            let partway = tabs.view.frame, heard = app.times
            times = laid.times
            laidOutAsTold = []
            // A transition's fit restores the shortened viewport. A projection alone keeps the one-point offset, and lays out
            // the container at it.
            let placed = fits ? CGRect(x: page.minX, y: page.minY, width: page.width, height: page.height - 100) : page.offsetBy(dx: 0, dy: 1)
            presenter.viewport.frame = placed
            settled = fits ? full : full.offsetBy(dx: 0, dy: 1)
            navigation.whileTabsChange(try XCTUnwrap(tabs as? TabController)) {
                for each in [laid, tabs.view!] { each.setNeedsLayout() }
                root.additionalSafeAreaInsets.bottom = root.additionalSafeAreaInsets.bottom == 0 ? 20 : 0
                root.view.layoutIfNeeded()
                if projects {
                    navigation.projectionHeld = true
                    navigation.resync()
                    XCTAssertFalse(navigation.projectionHeld, "it projects at once")
                }
                if fits {
                    home.delegate?.navigationController?(home, didShow: root, animated: false)
                    XCTAssertEqual(app.times, heard + 1, "the app's delegate hears the call as it comes")
                    let told = block()
                    XCTAssertNotEqual(told, whole, "UIKit's own layout of the stack was told")
                    root.view.bounds.size.height -= 50
                    presenter.owners.publish()
                    XCTAssertEqual(block(), told, "a block waits for the fit, which tells it with the viewport")
                }
                XCTAssertEqual(presenter.viewport.frame, placed, "nothing fits the session within the change")
                XCTAssertEqual(tabs.view.frame, partway, "or sizes the container")
                XCTAssertEqual(laid.times, times, "or lays it out")
                XCTAssertEqual(laidOutAsTold, [], "or tells a cover")
                XCTAssertTrue(navigation.coversAnswerOwed, "for which carried positions wait")
            }
            XCTAssertEqual(presenter.viewport.frame, fits ? page : placed, "the session is fitted where a transition ended")
            presenter.viewport.frame = page
            XCTAssertEqual(tabs.view.frame, settled)
            XCTAssertGreaterThan(laid.times, times)
            XCTAssertEqual(Set(laidOutAsTold), [true], "the covers are told once the container is laid out")
            XCTAssertEqual(block(), whole, "and the routes' blocks")
            XCTAssertFalse(navigation.coversAnswerOwed)
        }
        root.additionalSafeAreaInsets.bottom = 0
        waitFor("UIKit and the router agree") { !navigation.inTransition && !navigation.syncScheduled && navigation.restDifference == nil }
    }

    /// A scroll made as the covers are told, as a batch that answers them makes it, is for the container as it is laid out. A
    /// view the app takes out of its window as it hears a move is not fitted.
    func testTheCoversAreToldOnceTheContainerIsLaidOut() throws {
        let session = try fixture("more-change-answer", wholeScreen: true)
        let presenter = session.presenter, navigation = presenter.navigationHost, view = try XCTUnwrap(session.view)
        let tabs = try XCTUnwrap(navigation.tabController as? TabController)
        let home = try XCTUnwrap(tabs.selectedViewController as? UINavigationController), root = try XCTUnwrap(home.topViewController)
        let app = MoveHeard(), proxy = try XCTUnwrap(home.delegate as? NavigationDelegateProxy)
        proxy.give(app)
        waitForMainQueue()
        let scroller = try XCTUnwrap(node(session, "list-home").scrollView)
        let middle = try XCTUnwrap(presenter.views.values.first { $0.props["id"] == "list-middle" })
        // How far the middle's end is from the port's
        let off = { scroller.convert(middle.bounds, from: middle).maxY - (scroller.bounds.maxY - scroller.adjustedContentInset.bottom) }
        let reported = presenter.onCovers
        presenter.onCovers = {
            reported?($0)
            presenter.scrollElementIntoView(["list-middle", "end", "nearest", "instant"])
        }
        tabs.view.frame.size.height -= 100
        XCTAssertGreaterThan(abs(off()), 1, "the middle's end is elsewhere to begin with")
        navigation.whileTabsChange(tabs) {
            home.delegate?.navigationController?(home, didShow: root, animated: false)
            root.additionalSafeAreaInsets.bottom = 20
            root.view.layoutIfNeeded()
        }
        presenter.onCovers = reported
        XCTAssertEqual(off(), 0, accuracy: 1, "the scroll is for the container as it is laid out")
        let size = session.rootSize
        app.hear = {
            view.removeFromSuperview()
            view.frame.size = CGSize(width: size.width - 100, height: size.height - 100)
        }
        navigation.whileTabsChange(tabs) { home.delegate?.navigationController?(home, didShow: root, animated: false) }
        XCTAssertEqual(session.rootSize, size)
        app.hear = {}
        proxy.give(nil)
        waitForMainQueue()
    }
}

/// An app's navigation delegate that hears `willShow` and `didShow`.
private final class MoveHeard: NSObject, UINavigationControllerDelegate {
    var times = 0, hear: () -> Void = {}, before: () -> Void = {}
    func navigationController(_ nav: UINavigationController, willShow controller: UIViewController, animated: Bool) { before() }
    func navigationController(_ nav: UINavigationController, didShow controller: UIViewController, animated: Bool) {
        times += 1
        hear()
    }
}
#endif
