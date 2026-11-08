#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A reaching scroller's content inset and the reach it compensates change together, in the pass that moves the
/// reach (`SafeReach.apply`), never when UIKit only gives the safe area the reach will follow.
final class SafeReachTimingIOSTests: XCTestCase {
    private var window: UIWindow?

    override func tearDown() {
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    /// A scroller reaching the route's left edge through a 40-point safe area, its content where its box is: the safe
    /// area goes, and through UIKit's layout of the route the content stays where it was until the reach moves with
    /// it. Read from the scroll view's new safe area, its inset dropped the safe area while its frame still reached:
    /// a column opening moved a page by the whole old reach until the batch that moved the reach.
    func testAnAnnouncedSafeAreaMovesNoContentUntilTheReachMoves() throws {
        let p = Presenter()
        let page = NodeView(id: 1, kind: "view", presenter: p), scroller = NodeView(id: 2, kind: "view", presenter: p)
        page.frame = CGRect(x: 0, y: 0, width: 300, height: 400)
        scroller.frame = CGRect(x: 40, y: 0, width: 260, height: 400)
        scroller.style["overflow_y"] = .string("scroll"); scroller.syncScroll()
        let content = UIView(frame: CGRect(x: 0, y: 0, width: 260, height: 800))
        let route = RouteController(page), window = UIWindow(frame: CGRect(x: 0, y: 0, width: 300, height: 400))
        self.window = window
        window.rootViewController = route
        window.makeKeyAndVisible()
        route.view.addSubview(page)
        page.addSubview(scroller)
        let sv = try XCTUnwrap(scroller.scroll)
        sv.addSubview(content)
        sv.contentSize = content.bounds.size
        let own = route.view.safeAreaInsets
        route.additionalSafeAreaInsets = UIEdgeInsets(top: -own.top, left: 40 - own.left, bottom: -own.bottom, right: -own.right)
        route.view.layoutIfNeeded()
        SafeReach.apply(UIEdgeInsets(top: 0, left: 40, bottom: 0, right: 0), to: scroller)
        route.view.layoutIfNeeded()
        let at = content.convert(content.bounds, to: nil).minX
        XCTAssertEqual(at, 40, accuracy: 0.5, "the content at the scroller's box, inside the safe area")
        route.additionalSafeAreaInsets.left -= 40
        route.view.layoutIfNeeded()
        XCTAssertEqual(route.view.safeAreaInsets.left, 0)
        XCTAssertEqual(content.convert(content.bounds, to: nil).minX, at, accuracy: 0.5, "the content stays while the reach does")
        SafeReach.apply(.zero, to: scroller)
        route.view.layoutIfNeeded()
        XCTAssertEqual(content.convert(content.bounds, to: nil).minX, at, accuracy: 0.5, "and with the reach gone, at its box still")
    }
}
#endif
