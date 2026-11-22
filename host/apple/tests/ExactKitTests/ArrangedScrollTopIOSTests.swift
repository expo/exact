#if os(iOS)
@testable import ExactKit
import UIKit
import XCTest

/// An arranged pane keeps CSS `scrollTop` as UIKit adopts its inset (LLP 1123 D14): Exact keeps it across the pane's
/// joining UIKit's adjustment, UIKit across the inset changes after it that were measured, and nothing Exact holds writes
/// over a newer one.
extension ContentOwnersIOSTests {
    func testAnArrangedPaneKeepsItsScrollTopThroughADeeperSafeAreaAndAKeyboardReveal() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged peers use the fold-aware window scene") }
        let (session, _, first, _, route, _) = try arrangedPeers(in: scene)
        let p = session.presenter, scroll = try XCTUnwrap(first.scrollView)
        let top = { scroll.contentOffset.y + first.scrollTopInset(scroll) }
        let (joined, inset) = (top(), scroll.adjustedContentInset.top)
        // The route's safe area 10 points deeper, taken in a later layout: UIKit moves the inset and keeps `scrollTop`
        route.additionalSafeAreaInsets.top += 10
        until("UIKit takes the deeper safe area") { self.window?.layoutIfNeeded(); return abs(scroll.adjustedContentInset.top - inset) > 9 }
        XCTAssertEqual(top(), joined, accuracy: 1, "the deeper safe area")
        route.additionalSafeAreaInsets.top -= 10
        self.window?.layoutIfNeeded()
        // A keyboard reveal's position stands through a later inset and passes
        p.reveal(try node(session, "first-row-30"))
        let revealed = top(), shown = scroll.adjustedContentInset.top
        XCTAssertGreaterThan(revealed, joined + 1, "the reveal moved the pane")
        route.additionalSafeAreaInsets.top += 10
        until("UIKit takes the later inset") { self.window?.layoutIfNeeded(); return abs(scroll.adjustedContentInset.top - shown) > 9 }
        for _ in 0..<3 { p.arrangements.sync() }
        XCTAssertEqual(top(), revealed, accuracy: 1)
        route.additionalSafeAreaInsets.top -= 10
    }

    func testAnAuthoredScrollTopInAnArrangedPaneStandsThroughPasses() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged peers use the fold-aware window scene") }
        let (session, _, first, _, _, _) = try arrangedPeers(in: scene)
        let p = session.presenter, scroll = try XCTUnwrap(first.scrollView)
        let top = { scroll.contentOffset.y + first.scrollTopInset(scroll) }
        p.apply(wireBatch([["op": "props", "id": Int(first.id), "set": ["scrollTop": "120"]]]))
        XCTAssertEqual(top(), 120, accuracy: 1)
        p.apply(wireBatch([["op": "props", "id": Int(first.id), "set": ["scrollLeft": "0"]]]))
        for _ in 0..<5 { p.arrangements.sync(); self.window?.layoutIfNeeded() }
        XCTAssertEqual(top(), 120, accuracy: 1, "nothing writes an older position over it")
    }

    func testAnArrangedPaneKeepsItsScrollTopAsABatchSwitchesItsInsetPolicy() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged peers use the fold-aware window scene") }
        let (session, _, _, second, _, _) = try arrangedPeers(in: scene)
        let p = session.presenter, scroll = try XCTUnwrap(second.scrollView)
        let top = { scroll.contentOffset.y + second.scrollTopInset(scroll) }
        // A list that follows its end, mid-list: each batch restores the position it captured
        p.apply(wireBatch([["op": "props", "id": Int(second.id), "set": ["scrollFollowEnd": "true", "scrollTop": "200"]]]))
        XCTAssertEqual(top(), 200, accuracy: 1)
        XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .automatic)
        let inset = scroll.adjustedContentInset.top
        // Hidden, the pane gives up the route's edge and its policy in the next batch's sync, and takes them back shown
        second.isHidden = true
        p.apply(paneBatch(second))
        XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .never, "the batch switched the pane's policy")
        XCTAssertNotEqual(scroll.adjustedContentInset.top, inset, accuracy: 1, "the switch took the bar's inset away")
        XCTAssertEqual(top(), 200, accuracy: 1, "leaving")
        second.isHidden = false
        p.apply(paneBatch(second))
        XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .automatic, "the batch switched it back")
        XCTAssertEqual(top(), 200, accuracy: 1, "joining")
    }

    func testAnArrangedListAtItsEndKeepsItAsAPassSwitchesItsInsetPolicy() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged peers use the fold-aware window scene") }
        let (session, _, _, second, _, _) = try arrangedPeers(in: scene)
        let p = session.presenter, scroll = try XCTUnwrap(second.scrollView as? ScrollView)
        let end = { scroll.end(under: scroll.adjustedContentInset) }
        p.apply(wireBatch([["op": "props", "id": Int(second.id), "set": ["scrollFollowEnd": "true"]]]))
        scroll.contentOffset.y = end()
        let inset = scroll.adjustedContentInset.top
        // A pass outside a batch, which restores nothing after it: the switch alone keeps the end, where `scrollTop`
        // kept without the bar's inset would lie past it by the bar's depth
        second.isHidden = true
        p.arrangements.sync()
        XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .never, "the pass switched the pane's policy")
        XCTAssertNotEqual(scroll.adjustedContentInset.top, inset, accuracy: 1, "the switch took the bar's inset away")
        XCTAssertEqual(scroll.contentOffset.y, end(), accuracy: 1, "leaving")
        second.isHidden = false
        p.arrangements.sync()
        XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .automatic, "the pass switched it back")
        XCTAssertEqual(scroll.contentOffset.y, end(), accuracy: 1, "joining")
    }

    /// A reveal whose animation UIKit stopped without reporting its end leaves a record that counts as running for up to
    /// a second; UIKit reports the pane at rest, so the record owns nothing and the switch keeps the end.
    func testAnArrangedListAtItsEndKeepsItThroughASwitchUnderAStaleRevealRecord() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged peers use the fold-aware window scene") }
        let (session, _, _, second, _, _) = try arrangedPeers(in: scene)
        let p = session.presenter, scroll = try XCTUnwrap(second.scrollView as? ScrollView)
        let end = { scroll.end(under: scroll.adjustedContentInset) }
        p.apply(wireBatch([["op": "props", "id": Int(second.id), "set": ["scrollFollowEnd": "true"]]]))
        scroll.contentOffset.y = end()
        second.isHidden = true
        p.arrangements.sync()
        XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .never, "the pass took the pane out of UIKit's adjustment")
        XCTAssertEqual(scroll.contentOffset.y, end(), accuracy: 1, "at its end before the record")
        p.elementScrolls.revealBegan(scroll, to: scroll.contentOffset, target: (second.id, second.incarnation), realigned: { nil }, arrived: {})
        XCTAssertNotNil(p.elementScrolls.revealing(scroll), "a record with no animation under it")
        XCTAssertFalse(scroll.scrollsItself, "UIKit reports the pane at rest")
        second.isHidden = false
        p.arrangements.sync()
        XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .automatic, "the pass took it back in")
        XCTAssertEqual(scroll.contentOffset.y, end(), accuracy: 1, "at its new end, not the bar's depth above it")
    }

    func testAPaneNearItsEndIsInItsRangeOnceABatchSwitchesItsInsetPolicy() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged peers use the fold-aware window scene") }
        let (session, _, _, second, _, _) = try arrangedPeers(in: scene)
        let p = session.presenter, scroll = try XCTUnwrap(second.scrollView as? ScrollView)
        let end = { scroll.end(under: scroll.adjustedContentInset) }
        // 10 points short of its end, it keeps `scrollTop` past the end without the bar's inset until the batch ends
        scroll.contentOffset.y = end() - 10
        second.isHidden = true
        p.apply(paneBatch(second))
        XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .never, "the batch switched the pane's policy")
        XCTAssertEqual(scroll.contentOffset.y, end(), accuracy: 1)
    }

    func testANativeScrollInAnArrangedPaneStandsThroughAnInsetChangeAndPasses() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged peers use the fold-aware window scene") }
        let (session, _, _, second, _, _) = try arrangedPeers(in: scene)
        let p = session.presenter, scroll = try XCTUnwrap(second.scrollView)
        let top = { scroll.contentOffset.y + second.scrollTopInset(scroll) }
        // A position no writer tells Exact of, then an inset change before the next pass: the newer position stands
        scroll.contentOffset.y += 80
        let moved = top()
        scroll.contentInset.bottom += 30
        for _ in 0..<3 { p.arrangements.sync(); self.window?.layoutIfNeeded() }
        XCTAssertEqual(top(), moved, accuracy: 1)
        scroll.contentInset.bottom -= 30
    }

    func testAnArrangedListAtItsEndKeepsItThroughADeeperBottomInset() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged peers use the fold-aware window scene") }
        let (session, _, _, second, _, _) = try arrangedPeers(in: scene)
        let p = session.presenter, scroll = try XCTUnwrap(second.scrollView as? ScrollView)
        let end = { scroll.end(under: scroll.adjustedContentInset) }
        second.props["scrollFollowEnd"] = "true"
        scroll.contentOffset.y = end()
        // A content inset 40 points deeper at the bottom, as a keyboard toolbar adds: the list keeps its new end through a pass
        scroll.contentInset.bottom += 40
        XCTAssertEqual(scroll.contentOffset.y, end(), accuracy: 1)
        p.arrangements.sync()
        XCTAssertEqual(scroll.contentOffset.y, end(), accuracy: 1, "the pass keeps the end")
        scroll.contentInset.bottom -= 40
    }

    /// A batch that changes nothing of `pane`'s but runs an arrangement sync between its scroll captures and restores.
    private func paneBatch(_ pane: NodeView) -> Batch {
        wireBatch([["op": "props", "id": Int(pane.id), "set": ["testId": "pane-second"]]])
    }
}
#endif
