#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation
@testable import ExactGroupedLists

/// LLP 1127.000 D1–D5, driven through the fixture's committed router and UIKit.
final class SearchLevelsIOSTests: XCTestCase {
    private var window: UIWindow?
    private static func firstResponder(in view: UIView) -> UIView? { view.isFirstResponder ? view : view.subviews.lazy.compactMap { firstResponder(in: $0) }.first }
    private var session: ExactSession?

    override func tearDown() {
        // A test that edits leaves the next one no keyboard: one left up is laid out for a window that is gone
        if let window {
            window.endEditing(true)
            waitFor("the keyboard goes down") { KeyboardSampleIOS.existing(in: window)?.sample == nil }
        }
        session?.destroy()
        session = nil
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    func testResultsBindOnlyToAUniqueBodyRootAndKeepTheirController() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        let fixture = try boot()
        let results = try XCTUnwrap(fixture.search.results)
        let controller = fixture.search.controller
        let firstRoot = try fixture.node("results")
        let books = try fixture.node("books")
        XCTAssertTrue(controller.searchResultsController === results)
        XCTAssertTrue(results.root === firstRoot)
        XCTAssertTrue(try fixture.inactive(firstRoot))
        XCTAssertFalse(try fixture.inactive(books))

        try fixture.openQuery("Fuji")
        XCTAssertTrue(results.visible)
        XCTAssertFalse(try fixture.inactive(firstRoot))
        XCTAssertTrue(try fixture.inactive(books))
        XCTAssertTrue(books.accessibilityElementsHidden)
        XCTAssertNotNil(fixture.agent.tap(["id": Int(try fixture.node("row-a").id)])["error"])
        XCTAssertEqual(fixture.host.shownKeys.count, 1)

        let list = try fixture.node("results-list")
        let scroll = try XCTUnwrap(list.scrollView)
        func scrollTop() -> CGFloat { scroll.contentOffset.y + list.scrollTopInset(scroll) }
        func scrollFacts() -> String {
            "viewport \(results.view.bounds), mount \(String(describing: results.block?.block)), root \(firstRoot.frame), "
                + "list \(list.frame), scroll bounds \(scroll.bounds), size \(scroll.contentSize), insets \(scroll.adjustedContentInset), "
                + "saved offset \(String(describing: list.hiddenScroll)), attached \(scroll.window != nil)"
        }
        XCTAssertLessThanOrEqual(scroll.bounds.height, results.view.bounds.height + 1, scrollFacts())
        list.pendingScrollTop = 100
        list.applyPendingScroll()
        fixture.settle("the results scroll")
        XCTAssertEqual(scrollTop(), 100, accuracy: 1, scrollFacts())
        try fixture.navigate("/hidden")
        XCTAssertTrue(controller.isActive)
        XCTAssertTrue(results.root === firstRoot)
        XCTAssertFalse(results.visible)
        XCTAssertTrue(try fixture.inactive(firstRoot))
        XCTAssertFalse(books.accessibilityElementsHidden)
        let hiddenOffset = try XCTUnwrap(list.hiddenScroll, scrollFacts())
        XCTAssertEqual(hiddenOffset.y, 100, accuracy: 1, scrollFacts())
        try fixture.navigate("/normal")
        XCTAssertTrue(results.visible)
        XCTAssertTrue(list.scrollView === scroll)
        XCTAssertEqual(scrollTop(), 100, accuracy: 1, scrollFacts())

        try fixture.navigate("/ambiguous")
        XCTAssertNil(results.root)
        XCTAssertFalse(controller.showsSearchResultsController)
        XCTAssertFalse(books.accessibilityElementsHidden)
        for mode in ["/missing", "/nested", "/ineligible"] {
            try fixture.navigate(mode)
            XCTAssertNil(results.root, mode)
            XCTAssertFalse(controller.showsSearchResultsController, mode)
        }
        try fixture.navigate("/normal")
        XCTAssertTrue(results.root === (try fixture.node("results")))
        XCTAssertTrue(results.visible)
        try fixture.navigate("/alternate")
        XCTAssertTrue(results.root === (try fixture.node("other-results")))
        XCTAssertEqual(results.root?.props["id"], "alternate")
        XCTAssertTrue(results.visible, fixture.searchFacts)
        XCTAssertTrue(fixture.search.controller === controller)
        XCTAssertTrue(controller.searchResultsController === results)

        let focus = try fixture.node("other-result")
        let down = fixture.agent.tap(["id": Int(focus.id), "phase": "down"])
        XCTAssertEqual(down["delivery"] as? String, "recognized", "\(down), result bounds \(fixture.agent.box(focus)), \(fixture.searchFacts)")
        let cancel = fixture.agent.tap(["phase": "cancel"])
        XCTAssertEqual(cancel["delivery"] as? String, "recognized", "\(cancel), down answered \(down), \(fixture.searchFacts)")
        XCTAssertEqual(fixture.count("resultPointers"), 1, "projected results still bubble through their authored route")
        XCTAssertTrue(focus.becomeFirstResponder())
        XCTAssertTrue(focus.isFirstResponder)
        let focuses = try XCTUnwrap(fixture.count("searchFocuses"))
        try fixture.navigate("/missing")
        waitFor("disappearing results return focus to the visible search control") { fixture.count("searchFocuses") == focuses + 1 }
        XCTAssertFalse(fixture.search.bindEditor().isFirstResponder)
        XCTAssertTrue(controller.isActive)

        try fixture.navigate("/normal")
        try fixture.navigate("/replace-owner")
        let replacement = try XCTUnwrap(fixture.split.searchOwner)
        XCTAssertFalse(replacement === fixture.list)
        XCTAssertNil(fixture.list.navigationItem.searchController)
        XCTAssertNil(fixture.search.field)
        XCTAssertNil(results.root)
        XCTAssertFalse(controller.isActive)
        let next = try XCTUnwrap(replacement.search)
        XCTAssertFalse(next.controller === controller)
        XCTAssertTrue(next.results?.root === (try fixture.node("results")))
    }

    func testAUIKitDismissalInterruptedByAFoldKeepsTheQueryWithoutCancel() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        let fixture = try boot(regular: true)
        try fixture.navigate("/take-query")
        try fixture.openQuery("Fuji")
        // The owner starts to fold after UIKit reports the dismissal, then
        // reverses before the dismissal finishes. The primary is visible again.
        fixture.search.willDismissSearchController(fixture.search.controller)
        _ = fixture.split.splitViewController(fixture.split.controller, topColumnForCollapsingToProposedTopColumn: .secondary)
        _ = fixture.split.splitViewController(fixture.split.controller, displayModeForExpandingToProposedDisplayMode: .oneBesideSecondary)
        fixture.split.splitViewControllerDidExpand(fixture.split.controller)
        XCTAssertTrue(fixture.host.exposesSearchOwner(fixture.search))
        fixture.search.controller.isActive = false
        fixture.settle("the reversed fold and dismissal finish")
        XCTAssertTrue(fixture.split.settledExpanded)
        XCTAssertFalse(fixture.search.controller.isActive)
        XCTAssertEqual(try fixture.value(), "Fuji")
        XCTAssertEqual(fixture.search.controller.searchBar.text, "Fuji")
        XCTAssertEqual(fixture.count("cancelCount"), 0)
        XCTAssertEqual(fixture.count("emptyEdits"), 0)
    }

    func testAResultRevealsRetainedN1AndBackReturnsToTheSameSearch() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        let fixture = try boot()
        let detail = fixture.split.detail
        let controller = fixture.search.controller
        let results = try XCTUnwrap(fixture.search.results)
        try fixture.openQuery("Fuji")
        try fixture.scrollResults(to: 12)
        let offset = try fixture.resultsOffset()
        try fixture.tap("result-a")
        waitFor("UIKit nests the existing secondary") { fixture.split.nested }
        fixture.settle("the selected detail covers search")
        let book = try XCTUnwrap(detail.topViewController as? RouteController)
        XCTAssertTrue(fixture.split.list.topViewController === detail)
        XCTAssertTrue(fixture.split.controller.viewController(for: .secondary) === detail)
        XCTAssertEqual(fixture.host.shownKeys, [fixture.list.key, book.key])
        XCTAssertFalse(results.visible)
        XCTAssertFalse(fixture.search.bindEditor().isFirstResponder)

        resize(regular: true)
        waitFor("UIKit expands the split") { fixture.split.settledExpanded }
        fixture.settle("the expanded columns settle")
        XCTAssertTrue(fixture.split.detail.topViewController === book)
        XCTAssertTrue(fixture.search.controller === controller)
        XCTAssertEqual(try fixture.value(), "Fuji")
        XCTAssertTrue(results.visible, fixture.searchFacts)
        XCTAssertEqual(try fixture.resultsOffset(), offset, accuracy: 1)
        XCTAssertTrue(fixture.search.bindEditor().becomeFirstResponder())
        fixture.settle("the reader refocuses the visible editor")
        XCTAssertTrue(fixture.split.searchState.primaryForeground)
        resize(regular: false)
        waitFor("UIKit collapses the split") { fixture.split.settledCollapsed }
        fixture.settle("the compact primary covers the retained detail")
        XCTAssertFalse(fixture.split.nested)
        XCTAssertTrue(detail.topViewController === book)
        XCTAssertTrue(try fixture.inactive(book.node))
        XCTAssertTrue(results.visible, fixture.searchFacts)
        XCTAssertEqual(try fixture.resultsOffset(), offset, accuracy: 1)
        try fixture.openQuery("Fuji")
        XCTAssertTrue(fixture.search.bindEditor().isFirstResponder, fixture.searchFacts)
        try fixture.tap("result-a")
        waitFor("UIKit reveals the secondary again") { fixture.split.nested }
        fixture.settle("the repeated selection settles")
        XCTAssertTrue(detail.topViewController === book)
        XCTAssertFalse(fixture.search.bindEditor().isFirstResponder, fixture.searchFacts)
        fixture.split.list.popViewController(animated: true)
        waitFor("Back selects the library visit") { fixture.host.selectedKey == fixture.list.key }
        fixture.settle("the library and its search settle")
        XCTAssertFalse(fixture.split.nested)
        XCTAssertEqual(fixture.count("backs"), 1)
        XCTAssertTrue(fixture.search.results === results)
        XCTAssertTrue(results.visible, fixture.searchFacts)
        // UIKit's search controller ends editing as the push begins and resumes it as the pop ends
        XCTAssertTrue(fixture.search.bindEditor().isFirstResponder, fixture.searchFacts)
        XCTAssertEqual(try fixture.resultsOffset(), offset, accuracy: 1)
        XCTAssertEqual(try fixture.value(), "Fuji")
    }

    func testAnOrdinarySplitHoldsTheLatestRouteThroughACoordinatorlessFold() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit moves a split in a window scene") }
        let fixture = try boot(regular: true)
        try fixture.navigate("/ordinary")
        let search = try XCTUnwrap(fixture.list.search)
        XCTAssertNil(search.controller.searchResultsController)
        XCTAssertTrue(search.controller.hidesNavigationBarDuringPresentation)
        if #available(iOS 26.0, *) { XCTAssertTrue(fixture.list.navigationItem.searchBarPlacementAllowsExternalIntegration) }
        XCTAssertFalse(fixture.split.paired)

        _ = fixture.split.splitViewController(fixture.split.controller, topColumnForCollapsingToProposedTopColumn: .secondary)
        XCTAssertTrue(fixture.split.moving)
        XCTAssertNil(fixture.split.list.transitionCoordinator)
        XCTAssertNil(fixture.split.detail.transitionCoordinator)
        XCTAssertTrue(fixture.session.navigate("/book/a"))
        XCTAssertTrue(fixture.session.navigate("/chapter/c"))
        let latest = try XCTUnwrap(fixture.host.selectedKey)
        XCTAssertTrue(fixture.host.pendingSync, "the committed route owes a projection when the fold ends")
        XCTAssertTrue(fixture.split.detail.topViewController === fixture.split.empty)

        resize(regular: false)
        waitFor("UIKit finishes the fold") { fixture.split.settledCollapsed }
        fixture.settle("the latest route projects after the fold")
        XCTAssertEqual((fixture.split.detail.topViewController as? RouteController)?.key, latest)
        XCTAssertTrue(fixture.split.nested)
        XCTAssertEqual(fixture.count("backs"), 0)
    }

    func testRouterRevisionFollowsCommitsWithoutSplitEligibility() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the fixture has a UIKit split") }
        let fixture = try boot()
        try fixture.navigate("/unsplit")
        XCTAssertNil(fixture.host.split)
        let revision = fixture.host.routerRevision
        try fixture.navigate("/book/a")
        XCTAssertEqual(fixture.host.routerRevision, revision + 1)
        let key = fixture.host.selectedKey
        try fixture.navigate("/book/a")
        XCTAssertEqual(fixture.host.selectedKey, key)
        XCTAssertEqual(fixture.host.routerRevision, revision + 2)
    }

    func testLastNestedResultClearsTheNativeKeyboardAndBar() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents search and its keyboard in a window scene") }
        let fixture = try boot(fullScreen: true)
        let window = try XCTUnwrap(window)
        try fixture.navigate("/cover-search")
        try fixture.navigate("/clearance")
        let shown = expectation(forNotification: UIResponder.keyboardDidShowNotification, object: nil)
        try fixture.openQuery("Fuji")
        wait(for: [shown], timeout: 15)
        fixture.settle("the real keyboard and search bar are in place")

        let results = try XCTUnwrap(fixture.search.results)
        let outer = try XCTUnwrap(try fixture.node("clearance-outer").scrollView)
        let inner = try XCTUnwrap(try fixture.node("clearance-inner").scrollView)
        let last = try fixture.node("clearance-row-23")
        let view = results.view!
        let bar = fixture.search.controller.searchBar
        func screen(_ rect: CGRect, from view: UIView) -> CGRect { view.convert(rect, to: window) }
        func nativeSafeBottom() -> CGFloat { screen(view.bounds.inset(by: view.safeAreaInsets), from: view).maxY }
        func mountBottom() throws -> CGFloat {
            let block = try XCTUnwrap(results.block)
            return screen(block.block, from: block.view).maxY
        }
        func scrollToEnd(_ scroll: UIScrollView) {
            let insets = scroll.adjustedContentInset
            let end = max(-insets.top, scroll.contentSize.height + insets.bottom - scroll.bounds.height)
            scroll.setContentOffset(CGPoint(x: scroll.contentOffset.x, y: end), animated: false)
        }
        func lastRowClearsChrome(file: StaticString = #filePath, line: UInt = #line) throws {
            scrollToEnd(inner)
            scrollToEnd(outer)
            fixture.settle("the last row settles")
            let keys = try XCTUnwrap(fixture.session.presenter.keyboardTop)
            let row = screen(last.bounds, from: last)
            XCTAssertLessThanOrEqual(row.maxY, keys + 1, "last row \(row), keyboard top \(keys)", file: file, line: line)
            let search = screen(bar.bounds, from: bar)
            XCTAssertFalse(row.insetBy(dx: 1, dy: 1).intersects(search), "last row \(row), search bar \(search)", file: file, line: line)
            XCTAssertTrue(screen(outer.bounds, from: outer).insetBy(dx: -1, dy: -1).contains(row), file: file, line: line)
        }

        // An earlier class in this process can leave the scene so that the simulator lays the keyboard out beside this
        // window: then nothing is over the results to clear, and this test measures nothing
        if let keys = KeyboardSampleIOS.existing(in: window)?.sample, !keys.frame.intersects(window.bounds) {
            throw XCTSkip("the simulator laid the keyboard out at \(keys.frame), outside this window \(window.bounds); run this class alone")
        }
        // The keyboard reaches the results' owner in an update of its own, after UIKit says it showed
        waitFor("the results clear the keyboard", facts: {
            let occupancy = fixture.host.searchOccupancy(in: view).map { "docked \($0.docked) keys \($0.keyboard) bar \($0.bar)" } ?? "none"
            let sessionView = window.rootViewController?.view.subviews.first
            let sampler = view.window.flatMap { KeyboardSampleIOS.existing(in: $0) }
            return "sampler \(sampler != nil) sample \(String(describing: sampler?.sample)), session guide \(String(describing: sessionView?.keyboardLayoutGuide.layoutFrame)), "
                + "same window \(view.window === sessionView?.window), results window \(String(describing: view.window.map { ObjectIdentifier($0) })), "
                + "keyboard top \(String(describing: fixture.session.presenter.keyboardTop)), policy \(fixture.session.presenter.keyboardPolicy), "
                + "occupancy \(occupancy), mount bottom \(String(describing: try? mountBottom())), safe bottom \(nativeSafeBottom()), "
                + "results frame \(view.frame), safe insets \(view.safeAreaInsets). \(fixture.searchFacts)"
        }) {
            guard let keys = fixture.session.presenter.keyboardTop, let bottom = try? mountBottom() else { return false }
            return bottom <= min(nativeSafeBottom(), keys) + 1
        }
        let keys = try XCTUnwrap(fixture.session.presenter.keyboardTop)
        let nativeBottom = nativeSafeBottom()
        XCTAssertGreaterThan(keys, window.bounds.minY)
        XCTAssertLessThan(keys, window.bounds.maxY)
        XCTAssertLessThanOrEqual(try mountBottom(), min(nativeBottom, keys) + 1)
        try lastRowClearsChrome()

        try fixture.navigate("/visual")
        XCTAssertEqual(try mountBottom(), nativeSafeBottom(), accuracy: 1)
        try lastRowClearsChrome()

        try fixture.navigate("/layout")
        try fixture.navigate("/none")
        XCTAssertEqual(try mountBottom(), min(nativeSafeBottom(), keys), accuracy: 1, "the root still resizes")

        try fixture.navigate("/overlays")
        XCTAssertEqual(try mountBottom(), nativeSafeBottom(), accuracy: 1)
        XCTAssertTrue(fixture.search.bindEditor().isFirstResponder)
    }

    func testExpansionStagesTheCommittedEmptyDetailBeforeItsFirstFrame() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        let fixture = try boot()
        try fixture.navigate("/book/a")
        let departing = try XCTUnwrap(fixture.split.detail.topViewController as? RouteController)
        // Hold projection across a collapse reversing into an expansion.
        _ = fixture.split.splitViewController(fixture.split.controller, topColumnForCollapsingToProposedTopColumn: .secondary)
        XCTAssertTrue(fixture.session.navigate("/"))
        XCTAssertEqual(fixture.host.shownKeys, [fixture.list.key])
        XCTAssertTrue(fixture.split.detail.topViewController === departing)
        _ = fixture.split.splitViewController(fixture.split.controller, displayModeForExpandingToProposedDisplayMode: .oneBesideSecondary)
        XCTAssertTrue(fixture.split.detail.topViewController === fixture.split.empty || fixture.split.searchState.detailConcealed,
                      "the departed controller is covered or replaced before UIKit exposes N1")
        resize(regular: true)
        waitFor("UIKit expands the split") { fixture.split.settledExpanded }
        fixture.settle("the empty detail remains after expansion")
        XCTAssertTrue(fixture.split.detail.topViewController === fixture.split.empty)
        XCTAssertFalse(fixture.split.searchState.detailConcealed)
        XCTAssertEqual(fixture.count("backs"), 0)
    }

    func testHeldPopAndFoldLeaveANewerOpenOnTheSameDepth() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        let fixture = try boot()
        try fixture.navigate("/book/a")
        let held = Int(try fixture.node("held-contact").id)
        XCTAssertEqual(fixture.agent.tap(["id": held, "phase": "down"])["delivery"] as? String, "recognized")
        let source = try XCTUnwrap(fixture.host.selectedKey)
        let pop = try holdPop(fixture)
        defer { fixture.host.setAppDelegate(fixture.split.list, nil) }
        XCTAssertNotNil(try fixture.treeReply()["error"])
        XCTAssertEqual(fixture.agent.aim(["id": held, "aim": true])["error"] as? String,
                       "Tap is held by a UIKit transition. Run clock settle first.")
        XCTAssertEqual(fixture.agent.tap(["phase": "up"])["delivery"] as? String, "recognized")
        XCTAssertNil(fixture.agent.panContact)
        XCTAssertEqual(fixture.count("contactLifts"), 1)
        XCTAssertEqual(fixture.count("contactPresses"), 0)
        XCTAssertTrue(fixture.session.navigate("/book/b"))
        let newer = try XCTUnwrap(fixture.host.selectedKey)
        XCTAssertNotEqual(newer, source)
        XCTAssertEqual(fixture.host.shownKeys.count, 2)
        XCTAssertNotNil(fixture.agent.back()["error"])
        XCTAssertNotNil(fixture.agent.tap(["id": Int(try fixture.node("row-a").id)])["error"])
        XCTAssertNotNil(fixture.agent.type(["id": Int(try fixture.node("search").id), "text": "stray"])["error"])
        // Replay the expansion question while the pop still owns the secondary.
        _ = fixture.split.splitViewController(fixture.split.controller, displayModeForExpandingToProposedDisplayMode: .oneBesideSecondary)
        XCTAssertTrue(fixture.split.searchState.detailConcealed)
        pop.interaction.finish()
        pop.interactive = false
        // Changing size class during the test animator's pop makes UIKit attach the same column to two parents.
        waitFor("UIKit finishes the custom pop") { fixture.split.list.transitionCoordinator == nil }
        resize(regular: true)
        waitFor("UIKit expands the split") { fixture.split.settledExpanded }
        fixture.settle("the new destination projects after UIKit settles")
        XCTAssertEqual(fixture.host.selectedKey, newer)
        XCTAssertEqual((fixture.split.detail.topViewController as? RouteController)?.key, newer)
        XCTAssertEqual(fixture.count("backs"), 0)
        XCTAssertEqual(try fixture.value(), "")
        let before = try XCTUnwrap(fixture.host.selectedKey)
        let startsRoute = Int(try fixture.node("contact-starts-route").id)
        XCTAssertEqual(fixture.agent.tap(["id": startsRoute, "phase": "down"])["delivery"] as? String, "recognized")
        XCTAssertNotEqual(fixture.host.selectedKey, before)
        XCTAssertTrue(fixture.host.holdsAgentTargets)
        XCTAssertEqual(fixture.agent.tap(["phase": "cancel"])["delivery"] as? String, "recognized")
        XCTAssertNil(fixture.agent.panContact)
        XCTAssertEqual(fixture.count("contactPresses"), 0)
        fixture.settle("the route opened by pointerdown settles")
    }

    func testCompletedPopFromAnInPlaceReplacementDispatchesOneBackAndCancelledPopDispatchesNone() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        let fixture = try boot()
        try fixture.navigate("/book/a")
        let key = try XCTUnwrap(fixture.host.selectedKey)
        let initial = try XCTUnwrap(fixture.split.detail.topViewController as? RouteController)
        defer { fixture.host.setAppDelegate(fixture.split.list, nil) }
        let cancelled = try holdPop(fixture)
        XCTAssertTrue(fixture.session.navigate("/replace-chapter"))
        XCTAssertEqual(fixture.host.selectedKey, key, "replace keeps the visit")
        cancelled.interaction.cancel()
        cancelled.interactive = false
        fixture.settle("the cancellation restores the replacement")
        XCTAssertEqual(fixture.count("backs"), 0)
        XCTAssertTrue(fixture.split.nested)
        let replacement = try XCTUnwrap(fixture.split.detail.topViewController as? RouteController)
        XCTAssertFalse(replacement === initial, "the replacement mounts a fresh route node")
        XCTAssertEqual(replacement.key, key)

        let completed = try holdPop(fixture)
        XCTAssertTrue(fixture.session.navigate("/replace-book"))
        XCTAssertEqual(fixture.host.selectedKey, key)
        completed.interaction.finish()
        completed.interactive = false
        waitFor("UIKit's completed Back selects the library visit") { fixture.host.selectedKey == fixture.list.key }
        fixture.settle("UIKit's completed Back settles")
        XCTAssertEqual(fixture.count("backs"), 1)
    }

    func testEmptySearchShowsSuggestionsOrLeavesTheSidebarReachable() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        let fixture = try boot()
        try fixture.openQuery("")
        let results = try XCTUnwrap(fixture.search.results)
        XCTAssertFalse(results.visible)
        try fixture.navigate("/suggestions")
        XCTAssertTrue(results.visible)
        XCTAssertTrue(fixture.search.bindEditor().isFirstResponder, fixture.searchFacts)
        let refused = try fixture.tap("result-refused")
        fixture.settle("the refused selection leaves the suggestions open")
        XCTAssertEqual(fixture.count("refusedPresses"), 1)
        XCTAssertEqual(fixture.host.shownKeys, [fixture.list.key])
        XCTAssertTrue(results.visible)
        XCTAssertTrue(fixture.search.bindEditor().isFirstResponder, "tap answered \(refused), \(fixture.searchFacts)")
        XCTAssertEqual(fixture.count("cancelCount"), 0)

        try fixture.navigate("/normal")
        XCTAssertFalse(results.visible)
        try fixture.tap("row-c")
        fixture.settle("Chapter c covers the library")
        XCTAssertTrue(fixture.split.nested)
        XCTAssertEqual(fixture.host.selectedKey, try fixture.node("chapter-c").props["navigationKey"])
        XCTAssertFalse(fixture.search.controller.isActive)
        XCTAssertEqual(fixture.count("cancelCount"), 0)
        try fixture.navigate("/")
        try fixture.openQuery("Fuji")
        try fixture.navigate("/hidden")
        try fixture.tap("row-c")
        fixture.settle("the fallback accepts the link while the results are hidden")
        XCTAssertTrue(fixture.split.nested)
        XCTAssertEqual(try fixture.value(), "Fuji")
        XCTAssertEqual(fixture.count("cancelCount"), 0)
    }

    func testAgentBackAddressesSheetThenForegroundSearchAndNativeDetailBackStillPops() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        let fixture = try boot(regular: true)
        try fixture.navigate("/book/a")
        try fixture.navigate("/push-b")
        try fixture.openQuery("Asia")
        try fixture.scrollResults(to: 80)
        let offset = try fixture.resultsOffset()
        fixture.split.detail.popViewController(animated: true)
        fixture.settle("the detail Back leaves search open")
        XCTAssertEqual(fixture.host.shownKeys.count, 2)
        XCTAssertTrue(fixture.search.controller.isActive)
        XCTAssertTrue(fixture.search.results?.visible == true, fixture.searchFacts)
        XCTAssertEqual(fixture.count("backs"), 1)
        XCTAssertEqual(try fixture.resultsOffset(), offset, accuracy: 1)

        // The reader edits again before opening a presentation over the foreground search.
        try fixture.openQuery("Fuji")
        XCTAssertTrue(fixture.split.searchState.primaryForeground, fixture.searchFacts)
        let foregroundOffset = try fixture.resultsOffset()
        if #available(iOS 26.0, *) {
            let detailVisits = fixture.host.shownKeys
            try fixture.navigate("/inspect")
            let inspector = try XCTUnwrap(fixture.split.inspecting, fixture.splitFacts)
            XCTAssertEqual(fixture.host.selectedKey, inspector.key, fixture.splitFacts)
            let whileInspecting = fixture.splitFacts
            XCTAssertTrue(fixture.split.searchState.primaryForeground, whileInspecting)
            let backAnswer = fixture.agent.back()
            func inspectorFacts() -> String {
                "back answered \(backAnswer), inspector \(inspector.key), column showing \(fixture.split.controller.isShowing(.inspector)), "
                    + "while inspecting [\(whileInspecting)], after Back [\(fixture.searchFacts)]"
            }
            XCTAssertNil(backAnswer["error"], inspectorFacts())
            waitFor("Back hides the inspector and releases its route", facts: inspectorFacts) {
                !fixture.agent.nativeInFlight() && fixture.split.inspecting == nil && !fixture.split.controller.isShowing(.inspector)
            }
            XCTAssertEqual(fixture.host.shownKeys, detailVisits, inspectorFacts())
            XCTAssertEqual(fixture.count("cancelCount"), 0)
            XCTAssertTrue(fixture.split.searchState.primaryForeground, inspectorFacts())
        }
        XCTAssertTrue(fixture.search.results?.visible == true, fixture.searchFacts)
        try fixture.navigate("/present-child")
        XCTAssertFalse(fixture.search.results?.visible == true)
        XCTAssertTrue(fixture.split.searchState.primaryForeground, fixture.searchFacts)
        XCTAssertNil(fixture.agent.back()["error"])
        fixture.settle("Back pops the child in the covering sheet")
        XCTAssertEqual(fixture.host.modalNavigation?.viewControllers.count, 1)
        XCTAssertEqual(fixture.count("cancelCount"), 0)
        XCTAssertNil(fixture.agent.back()["error"])
        fixture.settle("the covering sheet is dismissed")
        XCTAssertNil(fixture.host.modalNavigation)
        XCTAssertTrue(fixture.search.controller.isActive, fixture.searchFacts)
        XCTAssertTrue(fixture.search.results?.visible == true, fixture.searchFacts)
        XCTAssertEqual(try fixture.resultsOffset(), foregroundOffset, accuracy: 1, fixture.searchFacts)
        XCTAssertTrue(fixture.split.searchState.primaryForeground, fixture.searchFacts)
        XCTAssertNil(fixture.agent.back()["error"])
        fixture.settle("the foreground search closes")
        XCTAssertFalse(fixture.search.controller.isActive)
        XCTAssertEqual(fixture.count("cancelCount"), 1)
        XCTAssertEqual(try fixture.value(), "Fuji")
    }

    func testNativeCloseReopensFromTheToolbarButtonAndTypesIntoTheCurrentEditor() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("integrated search is available from iOS 26") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "build.mjs --test --ios starts the XCTest touch runner")
        }
        let fixture = try boot(fullScreen: true)
        // Use Landscapes' untitled Home and full viewport with UIKit's minimized toolbar search.
        try fixture.navigate("/toolbar-search")
        try fixture.navigate("/book/a")
        waitFor("UIKit shows the detail", facts: { fixture.splitFacts }) { fixture.split.nested }
        _ = try XCTUnwrap(fixture.split.list.popViewController(animated: true), fixture.splitFacts)
        waitFor("UIKit returns to the library", facts: { fixture.splitFacts }) {
            fixture.host.selectedKey == fixture.list.key && !fixture.agent.nativeInFlight()
        }

        let touch = try TouchRunner(self)
        defer { touch.connection.cancel() }
        let sceneWindow = try XCTUnwrap(window)
        let scene = try XCTUnwrap(sceneWindow.windowScene)
        let nav = fixture.split.list
        let item = fixture.list.navigationItem
        XCTAssertEqual(item.preferredSearchBarPlacement, .integratedButton, fixture.searchFacts)
        let barButton = item.searchBarPlacementBarButtonItem
        let node = try fixture.node("search")
        func frameOf(_ element: [String: Any]) -> CGRect? {
            guard let frame = element["frame"] as? [Double], frame.count == 4 else { return nil }
            return CGRect(x: frame[0], y: frame[1], width: frame[2], height: frame[3])
        }
        func isButton(_ element: [String: Any]) -> Bool {
            ["Button", "ToolbarButton"].contains(element["type"] as? String ?? "") && element["enabled"] as? Bool == true
        }
        func midpointInWindow(_ frame: CGRect) -> CGPoint {
            sceneWindow.convert(CGPoint(x: frame.midX, y: frame.midY), from: sceneWindow.screen.coordinateSpace)
        }
        func keyboardFrame() -> CGRect {
            let splitView = fixture.split.controller.view!
            return splitView.convert(splitView.keyboardLayoutGuide.layoutFrame, to: sceneWindow)
        }
        func buttonFacts(_ tree: [[String: Any]], button: CGRect? = nil) -> String {
            let editor = fixture.search.controller.searchBar.searchTextField
            let hit = button.flatMap { sceneWindow.hitTest(midpointInWindow($0), with: nil) }
            let landing = hit.map { view in
                "Hit \(String(describing: type(of: view))), in navigation controller \(view.isDescendant(of: nav.view)), "
                    + "in route content \(view.isDescendant(of: fixture.list.view)), "
                    + "in toolbar \(view.isDescendant(of: nav.toolbar)), in navigation bar \(view.isDescendant(of: nav.navigationBar)). "
            } ?? ""
            return "\(fixture.searchFacts), editor attached \(editor.window != nil), toolbar hidden \(nav.isToolbarHidden), "
                + "assigned toolbar item \(fixture.list.toolbarItems?.contains { $0 === barButton } == true), "
                + "item \(barButton.accessibilityIdentifier ?? "no identifier") labelled \(barButton.accessibilityLabel ?? ""), "
                + "item frames in window \(String(describing: barButton.frame(in: sceneWindow))), "
                + "toolbar \(String(describing: barButton.frame(in: nav.toolbar))), navigation bar \(String(describing: barButton.frame(in: nav.navigationBar))). "
                + "Window \(sceneWindow.bounds), safe area \(sceneWindow.safeAreaInsets). \(landing)Touch tree \(tree)"
        }
        func searchFrame(_ tree: [[String: Any]]) throws -> CGRect {
            let buttons = tree.filter { isButton($0) && $0["label"] as? String == "Search" }
            let frame = try XCTUnwrap(buttons.count == 1 ? frameOf(buttons[0]) : nil,
                                      "UIKit exposed \(buttons.count) Search buttons. \(buttonFacts(tree))")
            XCTAssertEqual(item.searchBarPlacement, .integratedButton, buttonFacts(tree, button: frame))
            let point = midpointInWindow(frame)
            let navigationBar = nav.navigationBar.convert(nav.navigationBar.bounds, to: sceneWindow)
            XCTAssertTrue(point.y > sceneWindow.bounds.midY && !navigationBar.contains(point),
                          "Search is in the bottom toolbar. \(buttonFacts(tree, button: frame))")
            return frame
        }
        func tapUIKit(at frame: CGRect) throws {
            let point = scene.coordinateSpace.convert(CGPoint(x: frame.midX, y: frame.midY), from: sceneWindow.screen.coordinateSpace)
            var request = try TouchRunner.aim(point, in: sceneWindow)
            request["op"] = "tap"
            XCTAssertEqual(try touch.ask(request)["done"] as? Bool, true, "UIKit frame \(frame)")
        }

        try tapUIKit(at: searchFrame(fixture.touchTree(touch)))
        waitFor("UIKit opens the search editor", facts: { fixture.searchFacts }) { fixture.search.bindEditor().isFirstResponder }
        fixture.settle("the search presentation settles")
        XCTAssertNil(fixture.agent.type(["id": Int(node.id), "text": "Fuji"])["error"], fixture.searchFacts)
        fixture.settle("the query and results settle")
        XCTAssertEqual(try fixture.value(), "Fuji")
        // A person closes search while editing: a docked keyboard lifts the bar, so Close is read once it sits above it
        waitFor("the search bar settles above the keyboard", facts: { "keyboard \(keyboardFrame()), \(fixture.searchFacts)" }) {
            let field = fixture.search.controller.searchBar.searchTextField
            let box = field.convert(field.bounds, to: sceneWindow), keyboard = keyboardFrame()
            return !fixture.agent.nativeInFlight() && field.window === sceneWindow && (keyboard.isEmpty || box.maxY <= keyboard.minY)
        }
        XCTAssertTrue(fixture.search.controller.isActive, fixture.searchFacts)
        let active = try fixture.touchTree(touch)
        let editors = active.filter {
            $0["type"] as? String == "SearchField" && $0["identifier"] as? String == "search"
        }
        let editor = try XCTUnwrap(editors.count == 1 ? editors.first.flatMap(frameOf) : nil,
                                  "UIKit exposed \(editors.count) Search fields with the fixture's identifier. \(buttonFacts(active))")
        let keyboard = keyboardFrame()
        // UIKit can expose Close beside the field in the accessibility tree.
        let closes = active.compactMap { element -> CGRect? in
            guard isButton(element), ["close", "cancel"].contains((element["label"] as? String ?? "").lowercased()),
                  let button = frameOf(element), !button.isEmpty else { return nil }
            let gap = button.minX - editor.maxX
            let point = midpointInWindow(button)
            guard (0...editor.height).contains(gap), (editor.minY...editor.maxY).contains(button.midY),
                  sceneWindow.bounds.contains(point), !keyboard.contains(point) else { return nil }
            return button
        }
        let close = try XCTUnwrap(closes.count == 1 ? closes.first : nil,
                                 "UIKit exposed \(closes.count) Close buttons beside Search at \(editor), keyboard \(keyboard). \(buttonFacts(active))")
        try tapUIKit(at: close)
        waitFor("the native Close collapses search", facts: { "Close \(close), keyboard \(keyboard), \(fixture.searchFacts)" }) {
            !fixture.search.controller.isActive && !fixture.agent.nativeInFlight()
        }
        XCTAssertEqual(fixture.count("cancelCount"), 1)
        XCTAssertEqual(fixture.count("emptyEdits"), 0)
        XCTAssertEqual(try fixture.value(), "Fuji")

        let inactive = try fixture.touchTree(touch)
        let button = try searchFrame(inactive)
        let reply = fixture.agent.aim(["id": Int(node.id), "aim": ["press": true]])
        let aim = try XCTUnwrap(reply["aim"] as? [String: Any], "\(reply), \(buttonFacts(inactive, button: button))")
        let point = try XCTUnwrap((aim["point"] as? [Double]).flatMap { $0.count == 2 ? CGPoint(x: $0[0], y: $0[1]) : nil }, "\(aim)")
        let frame = scene.coordinateSpace.convert(button, from: sceneWindow.screen.coordinateSpace)
        _ = try XCTUnwrap(frame.contains(point) ? point : nil, "Agent aimed at \(point), Search button at \(frame). \(buttonFacts(inactive, button: button))")
        let request = ["op": "tap", "point": [point.x, point.y],
                       "screen": try XCTUnwrap(aim["fixed"]), "display": try XCTUnwrap(aim["display"])]
        XCTAssertEqual(try touch.ask(request)["done"] as? Bool, true)
        waitFor("the native button reopens the editor", facts: { "\(aim), \(fixture.searchFacts)" }) { fixture.search.bindEditor().isFirstResponder }
        fixture.settle("the reopened search settles")
        XCTAssertEqual(fixture.search.bindEditor().text, "Fuji")
        XCTAssertTrue(fixture.search.results?.exposed == true, fixture.searchFacts)
        XCTAssertNil(fixture.agent.type(["id": Int(node.id), "text": "Asia"])["error"])
        fixture.settle("the new editor delivers the edit")
        XCTAssertEqual(try fixture.value(), "Asia")
    }

    func testAgentTapOnTheDrawnFieldBeginsEditing() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        let fixture = try boot()
        try fixture.openQuery("Fuji")
        // End editing as a drag on the results does: the search stays open and UIKit's field stays drawn
        XCTAssertTrue(fixture.search.bindEditor().resignFirstResponder(), fixture.searchFacts)
        fixture.settle("the editor ends")
        let node = try fixture.node("search")
        XCTAssertNotNil(fixture.session.presenter.navigation.drawnSearch(node), fixture.searchFacts)
        XCTAssertEqual(try fixture.tap("search")["native"] as? String, "search-field")
        waitFor("the tap resumes editing", facts: { fixture.searchFacts }) { fixture.search.bindEditor().isFirstResponder }
        XCTAssertTrue(fixture.search.controller.isActive, fixture.searchFacts)
        XCTAssertNil(fixture.agent.type(["id": Int(node.id), "text": "Asia"])["error"], fixture.searchFacts)
        fixture.settle("the query settles")
        XCTAssertEqual(try fixture.value(), "Asia")
    }

    func testAgentTapOpensTheCollapsedSearchWithoutTheAccessibilityRuntime() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("integrated search is available from iOS 26") }
        let fixture = try boot(fullScreen: true)
        // Use Landscapes' untitled Home, whose compact search UIKit collapses into a button
        try fixture.navigate("/cover-search")
        let node = try fixture.node("search")
        XCTAssertNil(fixture.session.presenter.navigation.drawnSearch(node), fixture.searchFacts)
        XCTAssertEqual(try fixture.tap("search")["native"] as? String, "search-button", fixture.searchFacts)
        waitFor("the button opens UIKit's search editor", facts: { fixture.searchFacts }) { fixture.search.bindEditor().isFirstResponder }
        fixture.settle("the search presentation settles")
        XCTAssertNil(fixture.agent.type(["id": Int(node.id), "text": "Fuji"])["error"], fixture.searchFacts)
        fixture.settle("the query settles")
        XCTAssertEqual(try fixture.value(), "Fuji")
    }

    func testUnpreventedCloseProducesOneEmptyEditAndLateCompletionKeepsNewerText() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        let fixture = try boot(regular: true)
        try fixture.navigate("/take-query")
        try fixture.openQuery("Fuji")
        XCTAssertNil(fixture.agent.back()["error"])
        fixture.settle("agent Back dismisses search")
        XCTAssertFalse(fixture.search.controller.isActive)
        XCTAssertEqual(fixture.count("cancelCount"), 1)
        XCTAssertEqual(fixture.count("emptyEdits"), 1)

        try fixture.openQuery("Asia")
        fixture.search.willDismissSearchController(fixture.search.controller)
        XCTAssertTrue(fixture.session.navigate("/new-query"))
        fixture.search.controller.isActive = false
        fixture.settle("UIKit settles a close that had a newer write")
        XCTAssertEqual(fixture.count("cancelCount"), 2)
        XCTAssertEqual(fixture.count("emptyEdits"), 1)
        XCTAssertEqual(fixture.search.bindEditor().text, "newer")
        try fixture.openQuery("Europe")
        fixture.search.didDismissSearchController(fixture.search.controller)
        fixture.settle("an older completion arrives after reopening")
        XCTAssertEqual(fixture.search.bindEditor().text, "Europe")
        XCTAssertEqual(fixture.count("emptyEdits"), 1)

        try fixture.tap("result-a")
        fixture.settle("the result brings expanded detail to the foreground")
        XCTAssertFalse(fixture.split.searchState.primaryForeground)
        XCTAssertTrue(fixture.search.results?.visible == true, fixture.searchFacts)
        XCTAssertFalse(fixture.search.bindEditor().isFirstResponder)
        let detail = fixture.host.selectedKey
        fixture.search.willDismissSearchController(fixture.search.controller)
        fixture.search.controller.isActive = false
        fixture.settle("UIKit's delegate-only close settles with detail still visible")
        XCTAssertEqual(fixture.host.selectedKey, detail)
        XCTAssertEqual(fixture.count("cancelCount"), 3)
        XCTAssertEqual(fixture.count("emptyEdits"), 2)
        XCTAssertEqual(try fixture.value(), "")
    }

    func testHostCloseWithoutDelegateCallbackReleasesTheProjection() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search only in a window scene") }
        let fixture = try boot()
        try fixture.openQuery("Fuji")
        fixture.search.controller.delegate = nil
        fixture.search.close()
        fixture.settle("the explicit close releases without a delegate callback")
        XCTAssertFalse(fixture.search.controller.isActive)
        XCTAssertEqual(fixture.search.bindEditor().text, "Fuji")
        XCTAssertEqual(fixture.count("cancelCount"), 0)
        fixture.search.controller.delegate = fixture.search
        try fixture.tap("row-c")
        fixture.settle("the next sidebar tap advances")
        XCTAssertEqual(fixture.host.shownKeys.count, 2)
    }

    private final class ObservedPop: InteractivePop {
        var callbacks: [String] = []

        override func navigationController(_ nav: UINavigationController, animationControllerFor operation: UINavigationController.Operation,
                                           from: UIViewController, to: UIViewController) -> UIViewControllerAnimatedTransitioning? {
            callbacks.append("animator \(operation.rawValue) from \(type(of: from)) to \(type(of: to))")
            return super.navigationController(nav, animationControllerFor: operation, from: from, to: to)
        }

        @objc func navigationController(_ nav: UINavigationController, willShow controller: UIViewController, animated: Bool) {
            callbacks.append("willShow \(type(of: controller))")
        }

        @objc func navigationController(_ nav: UINavigationController, didShow controller: UIViewController, animated: Bool) {
            callbacks.append("didShow \(type(of: controller))")
        }
    }

    private func holdPop(_ fixture: Fixture) throws -> InteractivePop {
        let pop = ObservedPop()
        let nav = fixture.split.list
        let before = fixture.splitFacts
        fixture.host.setAppDelegate(nav, pop)
        pop.interactive = true
        let removed = nav.popViewController(animated: true)
        func facts() -> String {
            let transition = nav.transitionCoordinator
            let proxy = nav.delegate as? NavigationDelegateProxy
            return "before \(before), after \(fixture.splitFacts), removed secondary \(removed === fixture.split.detail), "
                + "proxy owns delegate \(proxy?.host === fixture.host), app delegate installed \(proxy?.app === pop), callbacks \(pop.callbacks), "
                + "UIKit transition \(String(describing: transition?.isInteractive)), "
                + "from secondary \(transition?.viewController(forKey: .from) === fixture.split.detail), "
                + "to library \(transition?.viewController(forKey: .to) === fixture.list)"
        }
        waitFor("the native pop is interactive", facts: facts) { nav.transitionCoordinator?.isInteractive == true }
        do {
            _ = try XCTUnwrap(fixture.split.searchState.pop, facts())
        } catch {
            pop.interactive = false
            if nav.transitionCoordinator?.isInteractive == true { pop.interaction.cancel() }
            waitFor("UIKit releases the fixture's pop", facts: facts) { nav.transitionCoordinator == nil }
            fixture.host.setAppDelegate(nav, nil)
            throw error
        }
        pop.interaction.update(0.4)
        return pop
    }

    private func boot(regular: Bool = false, fullScreen: Bool = false) throws -> Fixture {
        guard !ExactEnv.authoredChrome else { throw XCTSkip("the authored chrome draws results in flow") }
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene, "the test method says it needs a scene")
        ExactGroupedLists.install()
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_SEARCH_LEVELS_PLAN"])
        let session = ExactApp.shared.makeSession(label: "header-search-\(UUID().uuidString)")
        self.session = session
        let window = UIWindow(windowScene: scene)
        self.window = window
        let host = UIViewController()
        window.rootViewController = host
        window.makeKeyAndVisible()
        let view = ExactView(session: session)
        host.view.addSubview(view)
        if fullScreen {
            window.traitOverrides.horizontalSizeClass = regular ? .regular : .compact
            window.frame = window.screen.bounds
            view.frame = window.bounds
        } else {
            resize(regular: regular)
        }
        XCTAssertNil(try session.boot(plan: Data(contentsOf: URL(fileURLWithPath: path)), size: window.bounds.size).error)
        view.layoutIfNeeded()
        let agent = Agent(session: session)
        let navigation = session.presenter.navigationHost
        waitFor("the split is installed") { navigation.split != nil }
        let split = try XCTUnwrap(navigation.split)
        split.controller.preferredDisplayMode = .oneBesideSecondary
        waitFor("the split has the requested width class") { split.controller.isCollapsed != regular }
        waitFor("the fixture settles") { !agent.nativeInFlight() }
        return try Fixture(test: self, session: session, split: split)
    }

    /// Change the width class and the size as a fold does: the traits reach the views before the session view's new
    /// frame lays them out, which a split never sees apart on a device.
    func resize(regular: Bool) {
        guard let window else { return }
        window.traitOverrides.horizontalSizeClass = regular ? .regular : .compact
        window.updateTraitsIfNeeded()
        window.frame = CGRect(origin: .zero, size: regular ? CGSize(width: 951, height: 669) : CGSize(width: 466, height: 678))
        window.rootViewController?.view.subviews.first?.frame = window.bounds
    }

    struct Fixture {
        unowned let test: XCTestCase
        let session: ExactSession
        let split: SplitIOS
        let list: RouteController
        let search: HeaderSearch
        let agent: Agent
        var host: NavigationHost { session.presenter.navigationHost }
        var splitFacts: String {
            "router \(host.shownKeys) selected \(host.selectedKey ?? "none"), paired \(split.paired), moving \(split.moving), writing \(split.writing), "
                + "collapsed \(split.controller.isCollapsed), nested \(split.nested), chose primary \(split.searchState.collapseToPrimary), "
                + "display mode \(split.controller.displayMode.rawValue), foreground primary \(split.searchState.primaryForeground), "
                + "primary exposed \(split.searchState.primaryExposed), detail exposed \(split.searchState.detailExposed), "
                + "split \(split.shown.map(\.key)), list \(split.list.viewControllers.map { String(describing: type(of: $0)) }), "
                + "pending \(split.searchState.pending), sheet \(host.modalNavigation != nil)"
        }
        var searchFacts: String {
            let editor = search.controller.searchBar.searchTextField
            let responder = editor.window.flatMap { SearchLevelsIOSTests.firstResponder(in: $0) }
            let item = list.navigationItem
            return "active \(search.controller.isActive), editing \(editor.isEditing), first responder \(String(describing: responder)), "
                + "results exposed \(search.results?.exposed == true), preferred placement \(item.preferredSearchBarPlacement.rawValue), "
                + "realized placement \(item.searchBarPlacement.rawValue), \(splitFacts)"
        }

        init(test: XCTestCase, session: ExactSession, split: SplitIOS) throws {
            (self.test, self.session, self.split) = (test, session, split)
            list = try XCTUnwrap(split.list.viewControllers.first as? RouteController)
            search = try XCTUnwrap(list.search)
            agent = Agent(session: session)
        }

        func node(_ name: String) throws -> NodeView {
            try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == name }, name)
        }

        func settle(_ what: String) {
            test.waitFor(what, facts: { "router \(host.shownKeys), split \(split.shown.map(\.key)), active search \(search.controller.isActive)" }) {
                !agent.nativeInFlight()
            }
        }

        @discardableResult func tap(_ name: String) throws -> [String: Any] {
            let reply = agent.tap(["id": Int(try node(name).id)])
            _ = try XCTUnwrap(reply["tapped"] as? Int, "\(name): \(reply)")
            return reply
        }

        func touchTree(_ touch: TouchRunner) throws -> [[String: Any]] {
            let reply = try touch.ask(["op": "tree"])
            return try XCTUnwrap(reply["elements"] as? [[String: Any]], "\(reply)")
        }

        func navigate(_ location: String) throws {
            XCTAssertTrue(session.navigate(location))
            settle(location)
        }

        func value() throws -> String { try node("search").props["value"] ?? "" }

        func resultsOffset() throws -> CGFloat {
            let list = try node("results-list"), scroll = try XCTUnwrap(list.scrollView)
            return scroll.contentOffset.y + list.scrollTopInset(scroll)
        }

        func scrollResults(to offset: Double) throws {
            let list = try node("results-list")
            list.pendingScrollTop = offset
            list.applyPendingScroll()
            settle("the requested results scroll")
            XCTAssertEqual(try resultsOffset(), offset, accuracy: 1)
            let scroll = try XCTUnwrap(list.scrollView)
            XCTAssertGreaterThan(scroll.contentOffset.y + scroll.adjustedContentInset.top, 1,
                                 "the results are scrolled past UIKit's starting offset")
        }

        func treeReply() throws -> [String: Any] {
            let pipe = Pipe(), previous = Agent.out
            Agent.out = pipe.fileHandleForWriting
            defer { Agent.out = previous }
            agent.handle(op: "tree", ["op": "tree"], line: #"{"op":"tree"}"#)
            return try XCTUnwrap(try JSONSerialization.jsonObject(with: pipe.fileHandleForReading.availableData) as? [String: Any])
        }

        func openQuery(_ text: String) throws {
            search.controller.isActive = true
            test.waitFor("the search presentation is open") { search.controller.isActive }
            settle("the search presentation settles")
            if !search.bindEditor().isFirstResponder { XCTAssertTrue(search.bindEditor().becomeFirstResponder()) }
            test.waitFor("the current search editor is live") { search.bindEditor().isFirstResponder }
            if try value() != text {
                search.bindEditor().selectAll(nil)
                search.bindEditor().insertText(text)
                test.waitFor("UIKit delivered the query") { (try? value()) == text }
            }
            settle("the search editor and results settle")
        }

        func count(_ name: String) -> Int? {
            let state = try? JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any]
            return (state?["slots"] as? [String: Any])?[name] as? Int
        }

        func inactive(_ node: NodeView) throws -> Bool {
            let tree = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"tree"}"#).utf8)) as? [String: Any])
            let rows = try XCTUnwrap(tree["nodes"] as? [[String: Any]])
            return rows.first { ($0["id"] as? NSNumber)?.uint32Value == node.id }?["inactive"] as? Bool == true
        }
    }
}
#endif
