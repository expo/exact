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

/// An app's navigation delegate that hears `didShow`.
private final class MoveHeard: NSObject, UINavigationControllerDelegate {
    var times = 0, hear: () -> Void = {}
    func navigationController(_ nav: UINavigationController, didShow controller: UIViewController, animated: Bool) {
        times += 1
        hear()
    }
}
#endif
