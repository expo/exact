#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// More and a node lifted over the tab container (LLP 1083.000 D4): More's list, inside the container, shows no
/// route for the node to be over, so it goes back into its column while the list shows.
extension NavigationMoreIOSTests {
    /// A button positioned over the routes, beside the panels in their
    /// column, paints over the native container from the layer after it
    /// (LLP 1083.000 D4). More's list shows inside the container, where no
    /// route shows for the button to be over: the button goes back into its
    /// column, under the container, as a tap selects More, before the list's
    /// first frame, and again as More's Back leaves a tab shown in More for
    /// the list, as the pop ends.
    func testMoresListPutsBackANodeLiftedOverTheContainer() throws {
        let (session, tabs, more) = try moreFixture("more-lifted")
        let navigation = session.presenter.navigationHost
        try tapNode(session, "show-overlay")
        let overlay = try node(session, "overlay")
        waitFor("the button is lifted over the container") { overlay.window != nil && navigation.siblings.holds(overlay) }
        tapTab(tabs, of: more)
        XCTAssertTrue(more.viewIfLoaded?.window != nil, "More shows its list")
        XCTAssertFalse(navigation.siblings.holds(overlay), "the button is back in its column")
        waitForMainQueue()
        XCTAssertFalse(navigation.siblings.holds(overlay), "and stays there")
        pickRow(tabs, 4)
        waitFor("Five's root shows in More, and the button over it") {
            navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil && navigation.siblings.holds(overlay)
        }
        more.popViewController(animated: false)
        // The layout pass before the list's first frame
        more.view.window?.layoutIfNeeded()
        XCTAssertTrue(more.stackRoutes.isEmpty, "More shows its list")
        XCTAssertFalse(navigation.siblings.holds(overlay), "More's Back puts the button back")
        waitFor("the pop settles") { !navigation.changing }
        waitForMainQueue()
        XCTAssertFalse(navigation.siblings.holds(overlay), "and keeps it there")
    }

    /// More's Back animated, as a person's tap animates it: the lifted button is back in its column once the
    /// pop ends at the list (LLP 1083.000 D4).
    func testMoresAnimatedBackPutsBackANodeLiftedOverTheContainer() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "an animated pop runs its frames only in a scene's window") }
        let (session, tabs, more) = try moreFixture("more-lifted-animated")
        let navigation = session.presenter.navigationHost
        try tapNode(session, "show-overlay")
        let overlay = try node(session, "overlay")
        tapTab(tabs, of: more)
        pickRow(tabs, 4)
        waitFor("Five's root shows in More, and the button over it") {
            navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil && navigation.siblings.holds(overlay)
        }
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        more.popViewController(animated: true)
        waitForAnimationsToStart(more)
        waitFor("the pop ends at the list") { more.stackRoutes.isEmpty && more.transitionCoordinator == nil && !navigation.changing }
        waitForMainQueue()
        XCTAssertFalse(navigation.siblings.holds(overlay), "the button is back in its column")
    }

    /// A report during an interactive pop toward More's list puts the button back, as the list comes in, and once the
    /// pop is cancelled, with no batch after it, the button is lifted again over the route More still shows (LLP
    /// 1083.000 D4). The test's own report stands for one a batch makes during the swipe.
    func testACancelledSwipeTowardMoresListLiftsTheNodeAgain() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "an interactive pop of More runs as an app's only in a scene's window") }
        let (session, tabs, more) = try moreFixture("more-lifted-cancel")
        let navigation = session.presenter.navigationHost
        try tapNode(session, "show-overlay")
        let overlay = try node(session, "overlay")
        tapTab(tabs, of: more)
        pickRow(tabs, 4)
        waitFor("Five's root shows in More, and the button over it") {
            navigation.routerTab == 4 && more.stackRoutes.count == 1 && more.transitionCoordinator == nil && navigation.siblings.holds(overlay)
        }
        let pop = InteractivePop()
        navigation.setAppDelegate(more, pop)
        pop.interactive = true
        UIView.setAnimationsEnabled(true)
        defer { UIView.setAnimationsEnabled(false) }
        more.popViewController(animated: true)
        waitForAnimationsToStart(more)
        pop.interaction.update(0.4)
        navigation.reportCovers()
        XCTAssertFalse(navigation.siblings.holds(overlay), "a report during the swipe puts the button back")
        pop.interaction.cancel()
        waitFor("the cancelled swipe leaves More on Five's root") { more.transitionCoordinator == nil && more.stackRoutes.count == 1 }
        waitFor("the button is lifted again over Five's root") { navigation.siblings.holds(overlay) }
    }
}
#endif
