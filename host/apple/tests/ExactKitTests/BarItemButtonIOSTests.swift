#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation
@testable import ExactNativeViews

/// A bar item's button that UIKit hosts outside the bar's views: what a hit
/// at the item's middle must land in for the agent's activation and a real
/// touch's aim (`Agent.barItemTarget`).
extension NavigationBarIOSTests {
    /// A glass item's button that UIKit hosts outside the bar's views (Lexy's
    /// "Record Trace" on iOS 27.1, a `_UIButtonBarButton` under a
    /// `UIPlatformGlassInteractionView` in the session's root node): a hit in
    /// the view carrying the item's identifier, among the views the session
    /// shows, does not cover the item for the agent's activation or a real
    /// touch's aim. Any other view there still covers it, one with the
    /// identifier in another session's view, or outside the session, included.
    func testABarItemsOwnButtonOutsideTheBarDoesNotCoverIt() throws {
        let session = try fixture("bar-glass", module: false)
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        let compose = try node(session, "compose-home")
        let item = try XCTUnwrap(nav.topViewController?.navigationItem.rightBarButtonItems?.first)
        let frame = try XCTUnwrap(item.frame(in: nav.navigationBar))
        let agent = Agent(session: session)
        XCTAssertNil(agent.drawnCovered(compose), "the item's button in the bar")
        // Over the item's middle, in the session's view and outside the bar, as UIKit's glass host is
        let view = try XCTUnwrap(session.view)
        let button = UIView(frame: nav.navigationBar.convert(frame, to: view))
        view.addSubview(button)
        defer { button.removeFromSuperview() }
        let covered = agent.drawnCovered(compose)
        XCTAssertTrue(covered?.hasPrefix("UIView covers its middle") == true, "a view that is not the item's covers it: \(covered ?? "nil")")
        button.accessibilityIdentifier = "compose-home"
        XCTAssertNil(agent.drawnCovered(compose), "the item's own button, outside the bar")
        // The identifier in another session's view, mounted in this one's over the item
        let nestedSession = ExactApp.shared.makeSession(label: "bar-glass-nested")
        sessions.append(nestedSession)
        let nested = ExactView(session: nestedSession), theirs = UIView(frame: button.bounds)
        nested.frame = button.frame
        theirs.accessibilityIdentifier = "compose-home"
        nested.addSubview(theirs)
        view.addSubview(nested)
        let inNested = agent.drawnCovered(compose)
        XCTAssertTrue(inNested?.hasPrefix("UIView covers its middle") == true, "another session's view covers it: \(inNested ?? "nil")")
        nested.removeFromSuperview()
        // The identifier on a view the session does not show, as another session's would be
        let foreign = UIView(frame: nav.navigationBar.convert(frame, to: nil))
        foreign.accessibilityIdentifier = "compose-home"
        try XCTUnwrap(view.window).addSubview(foreign)
        defer { foreign.removeFromSuperview() }
        let other = agent.drawnCovered(compose)
        XCTAssertTrue(other?.hasPrefix("UIView covers its middle") == true, "a view outside the session covers it: \(other ?? "nil")")
        foreign.removeFromSuperview()
        let reply = agent.aim(["id": Int(compose.id), "aim": ["press": true]])
        try requireFront(window)
        XCTAssertNotNil(reply["aim"], "a real touch aims at the item's own button: \(reply)")
    }

    /// An unnamed control UIKit draws beside the bar with the item's box (the
    /// strip of a closed iPhone Duo's cover panel, a sibling of the bar in
    /// its navigation controller's view, iOS 27.1) is the item's button, for
    /// the agent's activation and a real touch's aim, and where the hit is in
    /// no control it lands in the glass around it. One named for another item,
    /// or one off the item's box, named for it or not, is not, and the item
    /// is refused as not placed, as a hidden item is, and as it is for a hit
    /// on a view over the button in the strip or in another view beside the
    /// bar that draws controls;
    /// a view there with none covers it, as does another navigation bar, or
    /// one over the bar in the view
    /// that holds the pages, or in the session's view, outside the views that
    /// draw its items, as another navigation's bar would be.
    func testAnUnnamedControlBesideTheBarWithTheItemsBoxIsTheItemsButton() throws {
        let session = try fixture("bar-glass", module: false)
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        let compose = try node(session, "compose-home")
        let item = try XCTUnwrap(nav.topViewController?.navigationItem.rightBarButtonItems?.first)
        let frame = try XCTUnwrap(item.frame(in: nav.navigationBar))
        let agent = Agent(session: session)
        let refused = { if case .failure? = agent.drawnPlace(compose) { return true }; return false }
        let strip = UIView(frame: nav.view.bounds)
        nav.view.addSubview(strip)
        defer { strip.removeFromSuperview() }
        let button = UIControl(frame: nav.navigationBar.convert(frame, to: strip))
        strip.addSubview(button)
        XCTAssertNil(agent.drawnCovered(compose), "the item's button beside the bar")
        XCTAssertFalse(refused())
        button.accessibilityIdentifier = "compose-home"
        XCTAssertNil(agent.drawnCovered(compose), "named for its item")
        button.frame = button.frame.offsetBy(dx: 1, dy: 0)
        XCTAssertTrue(refused(), "named for its item, a point off its box: beside the bar UIKit's frame decides")
        button.accessibilityIdentifier = "another-item"
        XCTAssertTrue(refused(), "named for another item")
        button.accessibilityIdentifier = nil
        XCTAssertTrue(refused(), "a point off the item's box")
        button.frame = button.frame.offsetBy(dx: -1, dy: 0)
        // A control the box finds inside the hit where the hit is in none (shared glass around it): placed, the hit
        // landing in the glass, as in the bar
        button.isUserInteractionEnabled = false
        XCTAssertFalse(refused(), "the strip's one control with the item's box, inside the hit")
        XCTAssertTrue(agent.barItemTarget(compose, bar: nav.navigationBar, frame: frame, hit: strip) === strip)
        XCTAssertNil(agent.drawnCovered(compose), "the strip holds the hit, around the item's button")
        button.isHidden = true
        XCTAssertTrue(refused(), "a hidden button inside the hit, though the item shows")
        button.isHidden = false
        // A view over the button in the strip, beside it: the button is not inside its hit
        let over = UIView(frame: button.frame)
        strip.addSubview(over)
        XCTAssertTrue(refused(), "a view over the button inside the strip")
        over.removeFromSuperview()
        // A view over the item beside the bar: the strip's button under it is no match for its hit, which covers it
        let cover = UIView(frame: nav.view.bounds)
        nav.view.addSubview(cover)
        let coveredBeside = agent.drawnCovered(compose)
        XCTAssertTrue(coveredBeside?.contains("covers its middle") == true, "a view beside the bar with no control covers it: \(coveredBeside ?? "nil")")
        // Another navigation bar beside the bar, with an unnamed control of the item's box, covers it
        cover.removeFromSuperview()
        let sibling = UINavigationBar(frame: nav.view.bounds), send = UIControl(frame: nav.navigationBar.convert(frame, to: nav.view))
        sibling.addSubview(send)
        nav.view.addSubview(sibling)
        let coveredBySibling = agent.drawnCovered(compose)
        XCTAssertTrue(coveredBySibling?.contains("covers its middle") == true, "another navigation bar beside the bar covers it: \(coveredBySibling ?? "nil")")
        sibling.removeFromSuperview()
        nav.view.addSubview(cover)
        let another = UINavigationBar(frame: .zero)
        another.addSubview(UIControl(frame: .zero))
        cover.addSubview(another)
        let coveredByBar = agent.drawnCovered(compose)
        XCTAssertTrue(coveredByBar?.contains("covers its middle") == true, "a view holding another navigation bar covers it: \(coveredByBar ?? "nil")")
        another.removeFromSuperview()
        let shelf = UIControl(frame: .zero)
        cover.addSubview(shelf)
        XCTAssertTrue(refused(), "a hit in another view that draws controls is not the strip's button's")
        cover.removeFromSuperview()
        button.isUserInteractionEnabled = true
        strip.removeFromSuperview()
        // Over the bar in the view that holds the controller's pages, which draws no item
        let pages = try XCTUnwrap(nav.view.subviews.first { view in nav.topViewController?.view.isDescendant(of: view) == true && view !== nav.navigationBar })
        let index = try XCTUnwrap(nav.view.subviews.firstIndex(of: pages))
        nav.view.bringSubviewToFront(pages)
        let inPages = UIControl(frame: nav.navigationBar.convert(frame, to: pages))
        pages.addSubview(inPages)
        let coveredByPages = agent.drawnCovered(compose)
        XCTAssertTrue(coveredByPages?.contains("covers its middle") == true, "a control in the pages' view covers it: \(coveredByPages ?? "nil")")
        inPages.removeFromSuperview()
        nav.view.insertSubview(pages, at: index)
        // Over the bar in the session's view, outside the views that draw its items
        let view = try XCTUnwrap(session.view)
        let theirs = UIControl(frame: nav.navigationBar.convert(frame, to: view))
        view.addSubview(theirs)
        let covered = agent.drawnCovered(compose)
        XCTAssertTrue(covered?.contains("covers its middle") == true, "a control outside the item's views covers it: \(covered ?? "nil")")
        theirs.removeFromSuperview()
        // A real touch aims at the item's button beside the bar, at the box the bar gives the item now
        nav.view.addSubview(strip)
        // A hidden item shows no control, though one with its box is beside the bar
        item.isHidden = true
        XCTAssertTrue(refused(), "a hidden item")
        item.isHidden = false
        nav.navigationBar.layoutIfNeeded()
        button.frame = nav.navigationBar.convert(try XCTUnwrap(item.frame(in: nav.navigationBar)), to: strip)
        try requireFront(window)
        let aimed = agent.aim(["id": Int(compose.id), "aim": ["press": true]])
        XCTAssertNotNil(aimed["aim"], "a real touch aims at it: \(aimed)")
    }
}
#endif
