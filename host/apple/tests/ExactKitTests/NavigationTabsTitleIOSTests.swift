#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

extension NavigationTabsIOSTests {
    func testALazyTabWithShortScrollContentOpensWithItsLargeTitle() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "a tab's large title needs UIKit appearance and layout") }
        let session = try fixture("lazy-tab-title", wholeScreen: true, plan: "EXACT_LAZY_TAB_TITLE_PLAN")
        let navigation = session.presenter.navigation
        let tabs = try XCTUnwrap(navigation.tabController)
        let home = try XCTUnwrap(tabs.selectedViewController as? UINavigationController)
        guard !home.traitCollection.placesVerticalBar else { throw XCTSkip("a vertical bar has no expanding title") }
        waitFor("the launch tab's large title is visible") { self.showsLargeTitle(home) }
        XCTAssertEqual(tabs.tabControllers?[1].stackRoutes.count, 0, "the second tab waits for selection")
        tapTab(tabs, 1)
        waitFor("the lazy tab settles") { tabs.selectedIndex == 1 && !Agent(session: session).nativeInFlight() }
        let saved = try XCTUnwrap(tabs.selectedViewController as? UINavigationController)
        waitFor("the lazy tab's large title is visible") { self.showsLargeTitle(saved) }
        let route = try XCTUnwrap(saved.topViewController as? RouteController)
        let scroll = try XCTUnwrap(try node(session, "content-saved").scroll)
        XCTAssertTrue(route.contentScrollView(for: .top) === scroll)
        XCTAssertEqual(scroll.contentOffset.y + scroll.adjustedContentInset.top, 0, accuracy: 0.5)
        tapTab(tabs, 0)
        tapTab(tabs, 1)
        waitFor("the returning tab settles") { !Agent(session: session).nativeInFlight() }
        XCTAssertTrue(showsLargeTitle(saved), "the title stays open on return")
        tapTab(tabs, 1)
        waitFor("the reselected tab settles") { !Agent(session: session).nativeInFlight() }
        XCTAssertTrue(showsLargeTitle(saved), "reselection keeps the resting title open")
    }

    func showsLargeTitle(_ nav: UINavigationController) -> Bool {
        guard let window = nav.view.window else { return false }
        func visible(_ view: UIView) -> Bool {
            if let label = view as? UILabel, label.text == nav.topViewController?.navigationItem.title,
               label.bounds.height > 0,
               label.font.pointSize > UIFont.preferredFont(forTextStyle: .headline).pointSize {
                var rect = label.convert(label.bounds, to: window)
                var ancestor: UIView? = label
                while let parent = ancestor {
                    if parent.isHidden || parent.alpha < 0.01 { return false }
                    if parent.clipsToBounds { rect = rect.intersection(parent.convert(parent.bounds, to: window)) }
                    ancestor = parent.superview
                }
                return rect.intersection(window.bounds).height >= label.bounds.height - 0.5
            }
            return view.subviews.contains(where: visible)
        }
        return visible(nav.view)
    }
}
#endif
