#if os(iOS)
@testable import ExactKit
import UIKit
import XCTest

/// An arranged pane's owed scroll position (`ScrollHandoff`) outlasts a late inset and gives way to a newer one (LLP
/// 1123 D14): a keyboard reveal's, and the end a list keeps.
extension ContentOwnersIOSTests {
    func testAKeyboardRevealInAnArrangedPaneOutlastsALaterInset() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged peers use the fold-aware window scene") }
        let (session, _, first, _, route, _) = try arrangedPeers(in: scene)
        let p = session.presenter, scroll = try XCTUnwrap(first.scrollView)
        let top = { scroll.contentOffset.y + first.scrollTopInset(scroll) }
        XCTAssertTrue(p.arrangements.owesScroll(first.id), "the pane owes the position it joined with")
        p.reveal(try node(session, "first-row-30"))
        XCTAssertFalse(p.arrangements.owesScroll(first.id), "the reveal releases it")
        let revealed = top(), inset = scroll.adjustedContentInset.top
        XCTAssertGreaterThan(revealed, 1, "the reveal moved the pane")
        // A later inset before the pass: the pane keeps the revealed position
        route.additionalSafeAreaInsets.top += 10
        self.window?.layoutIfNeeded()
        XCTAssertNotEqual(scroll.adjustedContentInset.top, inset, accuracy: 1, "the pane's inset moved")
        p.arrangements.sync()
        XCTAssertEqual(top(), revealed, accuracy: 1)
    }

    func testAnArrangedListThatTakesItsEndKeepsItOverTheOwedPosition() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged peers use the fold-aware window scene") }
        let (session, _, _, second, _, _) = try arrangedPeers(in: scene)
        let p = session.presenter, scroll = try XCTUnwrap(second.scrollView as? ScrollView)
        let end = { scroll.end(under: scroll.adjustedContentInset) }
        // The second pane follows its end, and owes the position it joined with, off its end
        second.props["scrollFollowEnd"] = "true"
        XCTAssertTrue(p.arrangements.owesScroll(second.id), "the pane owes the position it joined with")
        XCTAssertLessThan(scroll.contentOffset.y, end() - 1, "off its end")
        // At its end, a content inset 40 points deeper at the bottom, as a keyboard toolbar adds, before another pass:
        // the inset's reaction keeps the list at its new end and releases the handoff, and the pass leaves the end
        scroll.contentOffset.y = end()
        scroll.contentInset.bottom += 40
        XCTAssertFalse(p.arrangements.owesScroll(second.id), "the end releases it")
        XCTAssertEqual(scroll.contentOffset.y, end(), accuracy: 1)
        p.arrangements.sync()
        XCTAssertEqual(scroll.contentOffset.y, end(), accuracy: 1, "the pass keeps the end")
        scroll.contentInset.bottom -= 40
    }

    func testAnArrangedPaneKeepsItsOwedPositionThroughALateBarWithNoPassAskedFor() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged peers use the fold-aware window scene") }
        let (session, _, first, _, route, _) = try arrangedPeers(in: scene)
        let p = session.presenter, scroll = try XCTUnwrap(first.scrollView)
        let top = { scroll.contentOffset.y + first.scrollTopInset(scroll) }
        XCTAssertTrue(p.arrangements.owesScroll(first.id))
        let (owed, inset) = (top(), scroll.adjustedContentInset.top)
        // A bar 10 points deeper a layout late: UIKit moves the pane's inset, and the pass its callbacks ask for writes
        // the owed position again
        route.additionalSafeAreaInsets.top += 10
        until("the pass keeps the owed position") {
            self.window?.layoutIfNeeded()
            return abs(scroll.adjustedContentInset.top - inset) > 9 && abs(top() - owed) < 1
        }
        XCTAssertTrue(p.arrangements.owesScroll(first.id), "it still owes it")
        route.additionalSafeAreaInsets.top -= 10
    }
}
#endif
