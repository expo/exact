#if os(iOS)
import ObjectiveC
import UIKit
import XCTest
@testable import ExactKit

/// What `setRightBarButtonItems(_:animated:)` was asked, in order: UIKit
/// shows no animation of the bar's items in its layers to test for.
private var trailingSets: [Bool] = []

extension UINavigationItem {
    /// Swapped with `setRightBarButtonItems(_:animated:)`, so this name calls UIKit's.
    @objc fileprivate func recordedRightBarButtonItems(_ items: [UIBarButtonItem]?, animated: Bool) {
        trailingSets.append(animated)
        recordedRightBarButtonItems(items, animated: animated)
    }
}

extension NavigationBarIOSTests {
    /// A press that changes the bar of a route that shows sets its items with
    /// UIKit's animation, as a hand-built bar's
    /// `setRightBarButtonItems(_:animated: true)` does, whether the bar held
    /// controls or only a readout.
    func testABarsItemsChangedOnARouteThatShowsAreSetWithUIKitsAnimation() throws {
        let session = try fixture("bar-items-animated", module: false, plan: "BAR_SHARE")
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        nav.view.layoutIfNeeded()
        let uikit = try XCTUnwrap(class_getInstanceMethod(UINavigationItem.self, #selector(UINavigationItem.setRightBarButtonItems(_:animated:))))
        let recorded = try XCTUnwrap(class_getInstanceMethod(UINavigationItem.self, #selector(UINavigationItem.recordedRightBarButtonItems(_:animated:))))
        method_exchangeImplementations(uikit, recorded)
        defer { method_exchangeImplementations(uikit, recorded) }
        trailingSets = []
        let items = { nav.topViewController?.navigationItem.rightBarButtonItems ?? [] }
        let more = try XCTUnwrap(items().first { $0.accessibilityIdentifier == "more-home" })
        // UIKit's send; a test process with no connected scene (a parallel worker's bare xctest) refuses it (false), so
        // there the item's own target and action, which UIKit's send calls
        if !UIApplication.shared.sendAction(try XCTUnwrap(more.action), to: more.target, from: more, for: nil) {
            _ = (more.target as AnyObject?)?.perform(more.action)
        }
        until("the press adds its control") { items().contains { $0.accessibilityIdentifier == "star-home" } }
        XCTAssertEqual(trailingSets, [true], "the items are set once, animated")
        // A bar left only a readout, then given its controls back: each change animates
        let toggle = try node(session, "bare-toggle").id
        trailingSets = []
        session.presenter.onPress?(toggle)
        until("the controls go, the readout stays") { !items().contains { $0.accessibilityIdentifier == "share-home" } }
        session.presenter.onPress?(toggle)
        until("the controls come back") { items().contains { $0.accessibilityIdentifier == "share-home" } }
        XCTAssertEqual(trailingSets, [true, true], "the readout-only bar's changes animate too")
    }

    /// The bar leaves a touch on none of its own controls to an authored
    /// title that lies under it, and keeps every other touch.
    func testTheBarLeavesATouchOnAnAuthoredTitleUnderItToTheTitle() throws {
        let session = try fixture("authored-title-touch", module: false)
        let p = session.presenter
        p.press(try node(session, "chat").id)
        let nav = try XCTUnwrap(p.navigation.primaryNavigation)
        until("chat is pushed") { nav.transitionCoordinator == nil }
        let bar = try XCTUnwrap(nav.navigationBar as? RouteNavigationBar)
        let tap = try node(session, "title-group")
        p.apply(wireBatch([["op": "style", "id": Int(tap.id), "style": ["appearance": "none", "written": ["appearance"]]]]))
        nav.view.layoutIfNeeded()
        // Where a route drawn under its bar would place it: across the bar's middle, which shows no title now.
        let middle = CGPoint(x: bar.bounds.midX, y: bar.bounds.midY)
        tap.frame = try XCTUnwrap(tap.superview).convert(CGRect(x: middle.x - 40, y: middle.y - 10, width: 80, height: 20), from: bar)
        XCTAssertTrue(bar.authoredControl(at: middle) === tap)
        XCTAssertNil(bar.hitTest(middle, with: nil), "the touch goes on to the title")
        let beside = CGPoint(x: middle.x + 80, y: middle.y)
        XCTAssertNil(bar.authoredControl(at: beside))
        XCTAssertNotNil(bar.hitTest(beside, with: nil), "the bar keeps its empty areas")
        tap.isHidden = true
        XCTAssertNil(bar.authoredControl(at: middle), "a hidden control takes no touch")
        tap.isHidden = false
        p.apply(wireBatch([["op": "style", "id": Int(tap.id), "style": ["appearance": "auto", "written": ["appearance"]]]]))
        XCTAssertNil(bar.authoredControl(at: middle), "a projected title is the bar's own")
    }

    /// A header control the navigation bar draws as its item is hidden in
    /// the page, and its `aria-keyshortcuts` chord is still a key command
    /// that presses it, as the same button's in the page is (#501).
    func testABarItemsShortcutIsAKeyCommandThatPressesIt() throws {
        let session = try fixture("bar-item-shortcut", module: false, plan: "BAR_SHARE")
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        nav.view.layoutIfNeeded()
        let more = try node(session, "more-home")
        session.presenter.apply(wireBatch([["op": "props", "id": Int(more.id), "set": ["accessibilityKeyShortcuts": "Meta+Z"]]]))
        XCTAssertFalse(more.accessibilityVisible, "the bar's item stands for the control")
        XCTAssertNotNil(session.presenter.navigation.tipItem(more), "the bar that shows draws it")
        let commands = session.presenter.shortcutCommands(#selector(UIResponder.becomeFirstResponder))
        XCTAssertEqual(commands.map { [$0.input, $0.title] }, [["z", "More"]], "one command, named for the button")
        XCTAssertTrue(session.presenter.shortcut(key: "z", held: "Meta+", focus: nil) === more)
    }
}
#endif
