#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// What the tabs' stack cannot do while a transition runs over it, a tablist's move of the tab bar (LLP 1075.003 §3.7)
/// or a push, it does as the transition ends, with no batch after it.
extension NavigationTabsIOSTests {
    /// The root's tablist shown while a sheet presents over the tabs brings the bar back as the presentation ends.
    func testTheRootsTablistShownDuringATransitionBringsTheBarBackAfterIt() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("setTabBarHidden is iOS 18") }
        guard hostedWindowScene() != nil else { throw missing("scene", "a presentation completes only in a scene's window") }
        let session = try fixture("basics-roottabbar")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let nav = try XCTUnwrap(tabs.selectedViewController as? UINavigationController)
        let root = try XCTUnwrap(nav.stackRoutes.first as? RouteController)
        try tapNode(session, "toggle-choosing")
        until("the bar left") { tabs.isTabBarHidden }
        let hidden = try whenAHeldPresentationEnds(over: tabs, in: session, during: {
            // Press the button through the runtime: it is under the sheet
            session.apply(session.runtime.press(try node(session, "toggle-choosing").id, now: session.now()))
            until("the stack takes the shown tablist") { root.tablistHidden == false }
            XCTAssertTrue(tabs.isTabBarHidden, "the bar waits for the transition")
            // Hold the session's later batches, so none, such as the layout new text asks for or a tick, can do at
            // rest what only the transition's end should
            session.holdFillForTests()
        }, read: { tabs.isTabBarHidden })
        XCTAssertEqual(hidden, false, "the bar came back as the transition ended")
    }

    /// A pushed route's tablist hidden in place while a sheet presents over the tabs hides the bar as the presentation
    /// ends.
    func testAPushedRoutesTablistHiddenDuringATransitionHidesTheBarAfterIt() throws {
        guard #available(iOS 18.0, *) else { throw XCTSkip("setTabBarHidden is iOS 18") }
        guard hostedWindowScene() != nil else { throw missing("scene", "a presentation completes only in a scene's window") }
        let session = try fixture("pushed-tablist-transition")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let nav = try XCTUnwrap(tabs.selectedViewController as? UINavigationController)
        try tapNode(session, "detail")
        until("Detail pushed, its tablist shown") { nav.viewControllers.count == 2 && nav.transitionCoordinator == nil && !tabs.isTabBarHidden }
        let list = try node(session, "tabs")
        let hidden = try whenAHeldPresentationEnds(over: tabs, in: session, during: {
            // Hidden in place over Detail, which the fixture's own tablist never is
            list.style["display"] = .string("none")
            session.presenter.navigationHost.followTablist(nav.viewControllers.compactMap { $0 as? RouteController }, in: nav)
            XCTAssertFalse(tabs.isTabBarHidden, "the bar waits for the transition")
            // Hold the session's later batches, so none, such as the layout new text asks for or a tick, can do at
            // rest what only the transition's end should
            session.holdFillForTests()
        }, read: { tabs.isTabBarHidden })
        XCTAssertEqual(hidden, true, "the bar left as the transition ended")
    }

    /// A route the router pushes while a sheet presents over the tabs waits for the presentation's end and is pushed
    /// after it, by whichever sync first writes the stack then. One that runs within a batch lifts the route's header as
    /// the batch applies, and the push then waits for the runtime's answer to its cover (`liftHeld`), a later batch, so
    /// the test lets later batches through.
    func testARoutePushedDuringATransitionIsPushedAfterIt() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "a presentation completes only in a scene's window") }
        let session = try fixture("pushed-during-transition")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let nav = try XCTUnwrap(tabs.selectedViewController as? UINavigationController)
        _ = try whenAHeldPresentationEnds(over: tabs, in: session, during: {
            // Press the button through the runtime: it is under the sheet
            session.apply(session.runtime.press(try node(session, "detail").id, now: session.now()))
            XCTAssertEqual(nav.viewControllers.count, 1, "the push waits for the transition")
            XCTAssertTrue(session.presenter.navigationHost.pendingSync, "which holds the visible stack's write")
        }, read: { nav.viewControllers.count })
        until("Detail is pushed after the transition") { nav.viewControllers.count == 2 }
    }

    /// `read` as a presentation over `tabs`, held open while `during` runs, ends: on the turn after the transition's
    /// completions, this one added after any that `during` added.
    private func whenAHeldPresentationEnds<Value>(over tabs: UITabBarController, in session: ExactSession, during: () throws -> Void, read: @escaping () -> Value) throws -> Value? {
        let held = HeldPresentation(), sheet = UIViewController()
        defer { held.end() }
        sheet.modalPresentationStyle = .overFullScreen
        sheet.transitioningDelegate = held
        tabs.present(sheet, animated: true)
        let presentation = try XCTUnwrap(tabs.transitionCoordinator, "the sheet's transition holds the tabs")
        until("the presentation animates") { held.animating }
        try during()
        var value: Value?
        presentation.animate(alongsideTransition: nil) { _ in DispatchQueue.main.async { value = read() } }
        held.end()
        until("the presentation ends") { value != nil }
        return value
    }
}

/// A presentation whose transition runs until `end`, so what a test does meanwhile lands mid-transition in every run.
private final class HeldPresentation: NSObject, UIViewControllerTransitioningDelegate, UIViewControllerAnimatedTransitioning {
    private var context: UIViewControllerContextTransitioning?
    /// UIKit has asked for the animation, which `end` completes
    var animating: Bool { context != nil }
    func animationController(forPresented presented: UIViewController, presenting: UIViewController, source: UIViewController) -> UIViewControllerAnimatedTransitioning? { self }
    func transitionDuration(using context: UIViewControllerContextTransitioning?) -> TimeInterval { 0 }
    func animateTransition(using context: UIViewControllerContextTransitioning) {
        if let view = context.view(forKey: .to) { context.containerView.addSubview(view) }
        self.context = context
    }
    func end() { context?.completeTransition(true); context = nil }
}
#endif
