#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// LLP 1127 D15, D23 on UIKit: at a split's collapse or expansion question the
/// host reads the person's pops, finishes a column's own push or pop (N1's at
/// both, N0's too at a collapse), and writes nothing else to the split until
/// UIKit settles, when it projects the router's visits. Each Back the person
/// committed reaches the app once:
/// bun host/apple/build.mjs --test --ios --only SplitIOSTests
extension SplitIOSTests {
    /// Collapse the split now, as a fold or a narrowing window does, with
    /// whatever its columns are doing in flight; `.regular` expands it.
    private func resizeNow(_ width: UIUserInterfaceSizeClass) throws {
        let window = try XCTUnwrap(self.window)
        window.traitOverrides.horizontalSizeClass = width
        window.updateTraitsIfNeeded()
        window.layoutIfNeeded()
    }

    /// The presses the controls `ids` receive from here on, in order.
    private func pressesOf(_ session: ExactSession, _ ids: [UInt32]) -> () -> [UInt32] {
        var presses: [UInt32] = []
        let press = session.presenter.onPress
        session.presenter.onPress = { id in
            if ids.contains(id) { presses.append(id) }
            press?(id)
        }
        addTeardownBlock { [weak session] in session?.presenter.onPress = press }
        return { presses }
    }

    /// Settled: UIKit moves nothing, no move is open, and a turn has passed
    /// for any Back a fresh turn owes.
    private func settled(_ session: ExactSession, _ split: SplitIOS, _ what: String, _ done: () -> Bool) {
        let agent = Agent(session: session), navigation = session.presenter.navigationHost
        until(what) { done() && !split.moving && !navigation.splitsMoving && navigation.owedPops.isEmpty && !agent.nativeInFlight() }
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
    }

    /// The book's notes over the book, beside the list.
    private func notesBeside(_ session: ExactSession) throws -> SplitIOS {
        let split = try split(session), agent = Agent(session: session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        try tap(session, "row-a")
        until("the book is in the detail") { split.shown.count == 2 && !agent.nativeInFlight() }
        try tap(session, "notes-a")
        until("its notes are pushed") { split.shown.count == 3 && !agent.nativeInFlight() }
        return split
    }

    /// @ref LLP 1127 D23 — the split collapses during N1's own animated pop,
    /// once it has begun, which crashes plain UIKit ("Layout requested for
    /// visible navigation bar"). The question finishes the pop, the notes'
    /// Back reaches the app once, and the book shows.
    func testACollapseDuringTheDetailsPopFinishesItAndBacksOnce() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        try collapseDuringTheNotesPop(begun: true)
    }

    /// The same in the turn the pop is asked for, before it has begun: it
    /// would begin at UIKit's next layout with N1 nested, which crashes too.
    func testACollapseInTheTurnOfTheDetailsPopFinishesItAndBacksOnce() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        try collapseDuringTheNotesPop(begun: false)
    }

    private func collapseDuringTheNotesPop(begun: Bool) throws {
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let split = try notesBeside(session)
        let backs = pressesOf(session, [try node(session, "back-notes-a").id, try node(session, "back-a").id])
        let book = try XCTUnwrap(split.detail.viewControllers.first)
        split.detail.popViewController(animated: true)
        if begun { until("the pop has begun") { book.viewIfLoaded?.window != nil } }
        XCTAssertNotNil(NavigationHost.pushOrPop(on: split.detail), "N1's own pop is in flight")
        try resizeNow(.compact)
        XCTAssertNil(NavigationHost.pushOrPop(on: split.detail), "the collapse's question finished the pop")
        settled(session, split, "collapsed at the book") { split.controller.isCollapsed && split.shown.count == 2 }
        XCTAssertEqual(backs().count, 1, "the notes' Back, once")
        XCTAssertTrue(has(session, "book-a") && !has(session, "notes-page"), "the router is at the book")
    }

    /// @ref LLP 1127 D23 — the split collapses during N1's own push of the
    /// notes, which plain UIKit crashes on too: the push ends at once and the
    /// notes show, with no Back.
    func testACollapseDuringTheDetailsPushFinishesItWithNoBack() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a push only in a window a scene holds") }
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .regular)
        let split = try split(session), agent = Agent(session: session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        try tap(session, "row-a")
        until("the book is in the detail") { split.shown.count == 2 && !agent.nativeInFlight() }
        let backs = pressesOf(session, [try node(session, "back-a").id])
        try tap(session, "notes-a")
        until("the notes' push is in flight") { NavigationHost.pushOrPop(on: split.detail) != nil }
        try resizeNow(.compact)
        XCTAssertNil(NavigationHost.pushOrPop(on: split.detail), "the collapse's question finished the push")
        settled(session, split, "collapsed at the notes") { split.controller.isCollapsed && split.shown.count == 3 }
        XCTAssertEqual(backs(), [], "a push owes no Back")
        XCTAssertTrue(has(session, "notes-page"))
    }

    /// @ref LLP 1127 D23 — collapsed, N1 pops its notes and the split expands
    /// once the pop has begun, which crashes plain UIKit: the expansion's
    /// question finishes N1's pop, its Back reaches the app once, and the book
    /// shows beside the list.
    func testAnExpansionDuringTheDetailsPopFinishesItAndBacksOnce() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .compact)
        let split = try collapsedBook(session), agent = Agent(session: session)
        try tap(session, "notes-a")
        until("its notes are pushed") { split.shown.count == 3 && !agent.nativeInFlight() }
        let backs = pressesOf(session, [try node(session, "back-notes-a").id, try node(session, "back-a").id])
        let book = try XCTUnwrap(split.detail.viewControllers.first)
        split.detail.popViewController(animated: true)
        until("N1's pop has begun") { book.viewIfLoaded?.window != nil && NavigationHost.pushOrPop(on: split.detail) != nil }
        try resizeNow(.regular)
        settled(session, split, "expanded at the book") { !split.controller.isCollapsed && split.shown.count == 2 }
        XCTAssertEqual(backs().count, 1, "the notes' Back, once")
        XCTAssertTrue(has(session, "book-a") && !has(session, "notes-page"))
    }

    /// The same in the turn the pop is asked for, before it has begun. The
    /// question does not lay N1 out to begin it: inside an expansion's question
    /// that layout runs UIKit's scheduled expansion early, and UIKit traps
    /// ("expected scheduled transition"). UIKit then leaves the pop and its
    /// expansion waiting on a layout N1 never gets in no window; the next
    /// turn's check gives it one, the pop runs, the split expands with the book
    /// beside the list, and the notes' Back reaches the app once.
    func testAnExpansionInTheTurnOfTheDetailsPopBacksOnce() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .compact)
        let split = try collapsedBook(session), agent = Agent(session: session)
        try tap(session, "notes-a")
        until("its notes are pushed") { split.shown.count == 3 && !agent.nativeInFlight() }
        let backs = pressesOf(session, [try node(session, "back-notes-a").id, try node(session, "back-a").id])
        split.detail.popViewController(animated: true)
        try resizeNow(.regular)
        settled(session, split, "expanded at the book") { !split.controller.isCollapsed && split.shown.count == 2 }
        XCTAssertEqual(backs().count, 1, "the notes' Back, once")
        XCTAssertTrue(has(session, "book-a") && !has(session, "notes-page"), "the router is at the book")
        let window = try XCTUnwrap(self.window)
        XCTAssertLessThan(split.list.view.frame.width, window.bounds.width / 2, "the list in its column")
        XCTAssertNotNil(split.detail.viewIfLoaded?.window, "the book beside it")
    }

    /// @ref LLP 1127 D15, D23 — collapsed, the person's Back pops N1 off N0,
    /// and the split expands before that pop ends. UIKit finishes N0's pop
    /// itself (ending it in the question's turn trapped); once expanded, N1
    /// shows again, so the Back is read at the question, before UIKit's move
    /// makes the book look shown: it reaches the app once, and the list shows
    /// beside an empty detail.
    func testAnExpansionDuringTheListsPopOfTheDetailBacksOnce() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .compact)
        let split = try collapsedBook(session)
        let backs = pressesOf(session, [try node(session, "back-a").id])
        split.list.popViewController(animated: true)
        until("N0's pop of N1 is in flight") { NavigationHost.pushOrPop(on: split.list) != nil }
        try resizeNow(.regular)
        settled(session, split, "expanded at the list") { !split.controller.isCollapsed && split.detailRoutes.isEmpty }
        XCTAssertEqual(backs().count, 1, "the book's Back, once")
        XCTAssertFalse(has(session, "book-a"), "the router left the book")
        XCTAssertTrue(split.detail.viewIfLoaded?.window != nil, "the empty detail is in the window")
    }

    /// LLP 1127 D15 (Feedback report 2) — collapsed at the notes, the router
    /// goes back to the list in one batch, and the split then expands. Only
    /// N0's pop of N1 begins (N1 keeps its routes until it has left N0), so
    /// UIKit expands to the list in its column beside the detail, not to the
    /// list across the window with the detail in no window.
    func testAReturnToTheListThenAnExpansionShowsBothColumns() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let session = try boot(size: CGSize(width: 1100, height: 800), width: .compact)
        let split = try collapsedBook(session), agent = Agent(session: session)
        try tap(session, "notes-a")
        until("its notes are pushed") { split.shown.count == 3 && !agent.nativeInFlight() }
        try tap(session, "home-a")
        settled(session, split, "back at the list, collapsed") { split.shown.count == 1 }
        try resizeNow(.regular)
        settled(session, split, "expanded") { !split.controller.isCollapsed }
        let window = try XCTUnwrap(self.window)
        XCTAssertLessThan(split.list.view.frame.width, window.bounds.width / 2, "the list in its column")
        XCTAssertNotNil(split.detail.viewIfLoaded?.window, "the detail in the window")
    }
}
#endif
