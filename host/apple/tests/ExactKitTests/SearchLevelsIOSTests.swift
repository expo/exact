#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A header search's results as a level of the stack (`placingResults`), over `scripts/fixtures/search-levels.contract`:
/// using the field enters the results route. Collapsed they are drawn in the list's controller, over its whole view,
/// the field staying in its bar; expanded they are pushed on the detail. UIKit, so a simulator runs it.
final class SearchLevelsIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?

    override func tearDown() {
        session?.destroy(); session = nil
        window?.isHidden = true; window = nil
        super.tearDown()
    }

    /// Collapsed at the list, the results cover the list in its own controller and the field stays the person's;
    /// a result pushes over both and Back returns to them with the keyboard down; the close button leaves them and
    /// keeps the text.
    func testTheResultsCoverTheCollapsedListAndKeepItsField() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search in a transition only in a window a scene holds") }
        let (session, split) = try boot()
        let navigation = session.presenter.navigation, agent = Agent(session: session)
        let (list, search) = try listSearch(split)
        // UIKit's floating bar can give the bar another field as it moves into search
        var field: UISearchTextField { search.controller.searchBar.searchTextField }
        session.presenter.focusElement(["levels-search"])
        until("the results route is on top") { navigation.selectedKey == routeKey(session, "results") && !agent.nativeInFlight() }
        until("the search is open") { opened(search) }
        let results = try XCTUnwrap(navigation.resultsInPlace)
        XCTAssertTrue(results.parent === list, "drawn in the list's controller")
        XCTAssertEqual(results.view.frame, list.view.bounds, "over the list's whole view")
        // Core Animation paints by depth, then by place: the results' view must not stand under the list's ranked route
        let listContent = try XCTUnwrap(list.view.subviews.first { list.node.isDescendant(of: $0) })
        XCTAssertGreaterThan(list.view.subviews.firstIndex(of: results.view) ?? -1, list.view.subviews.firstIndex(of: listContent) ?? .max)
        XCTAssertGreaterThanOrEqual(results.view.layer.zPosition, listContent.layer.zPosition, "painted over the list's route")
        XCTAssertEqual(split.list.viewControllers.map { $0 as? RouteController }, [list], "no push: the field stays in the list's bar")
        XCTAssertTrue(field.isFirstResponder)
        XCTAssertFalse(navigation.isInactiveRoute(containing: try node(session, "search")), "the list is live under its results")
        field.insertText("x")
        until("the app hears the text") { (try? node(session, "search"))?.props["value"] == "x" }
        XCTAssertTrue(navigation.resultsInPlace === results)
        XCTAssertNil(agent.tap(["id": Int(try node(session, "result-a").id)])["error"])
        until("the result is pushed") { has(session, "book-a") && navigation.selectedKey == routeKey(session, "book-a") && !agent.nativeInFlight() }
        XCTAssertNil(agent.back()["error"])
        until("back at the results") { navigation.selectedKey == routeKey(session, "results") && !has(session, "book-a") && !agent.nativeInFlight() }
        XCTAssertTrue(navigation.resultsInPlace === results, "still drawn in the list")
        XCTAssertTrue(search.controller.isActive)
        search.controller.isActive = false
        until("the close leaves the results") { navigation.resultsInPlace == nil && !has(session, "results") && !agent.nativeInFlight() && search.controller.transitionCoordinator == nil }
        XCTAssertNil(results.parent)
        XCTAssertEqual(try node(session, "search").props["value"], "x", "`preventDefault()` on `cancel` keeps the text")
        XCTAssertEqual(search.controller.searchBar.text, "x")
    }

    /// At results drawn in the list UIKit shows no Back button: the agent's `back` is the search's close, as the ✕ makes
    /// it, which leaves the results and keeps the text the app's `cancel` keeps.
    func testTheAgentsBackAtResultsDrawnInTheListIsTheSearchsClose() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search in a transition only in a window a scene holds") }
        let (session, split) = try boot()
        let navigation = session.presenter.navigation, agent = Agent(session: session)
        let (_, search) = try listSearch(split)
        session.presenter.focusElement(["levels-search"])
        until("the results route is on top") { navigation.resultsInPlace != nil && !agent.nativeInFlight() }
        until("the search is open") { opened(search) }
        search.controller.searchBar.searchTextField.insertText("x")
        until("the app hears the text") { (try? node(session, "search"))?.props["value"] == "x" }
        XCTAssertNil(agent.back()["error"])
        until("the close leaves the results") {
            !has(session, "results") && navigation.resultsInPlace == nil && !search.controller.isActive && !agent.nativeInFlight()
                && search.controller.transitionCoordinator == nil
        }
        XCTAssertFalse(search.controller.searchBar.searchTextField.isFirstResponder)
        XCTAssertEqual(try node(session, "search").props["value"], "x", "`preventDefault()` on `cancel` keeps the text")
        XCTAssertEqual(search.controller.searchBar.text, "x")
    }

    /// The keyboard put away with nothing typed closes the search into its button and leaves the results.
    func testAnEmptySearchPutAwayLeavesItsResults() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search in a transition only in a window a scene holds") }
        let (session, split) = try boot()
        let navigation = session.presenter.navigation, agent = Agent(session: session)
        let (_, search) = try listSearch(split)
        session.presenter.focusElement(["levels-search"])
        until("the results route is on top") { navigation.resultsInPlace != nil && !agent.nativeInFlight() }
        until("the search is open") { opened(search) }
        XCTAssertTrue(search.controller.searchBar.searchTextField.resignFirstResponder())
        until("the empty search closes and leaves its results") { !search.controller.isActive && !has(session, "results") && navigation.resultsInPlace == nil && !agent.nativeInFlight() }
    }

    /// Typed at the collapsed list after a Back from a page, an unfold puts that page back under the results, so Back
    /// from them shows it (LLP 1127 D5.9).
    func testAnUnfoldPutsThePageLeftUnderTheResults() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search in a transition only in a window a scene holds") }
        let (session, split) = try boot()
        let navigation = session.presenter.navigation, agent = Agent(session: session)
        XCTAssertNil(agent.tap(["id": Int(try node(session, "row-a").id)])["error"])
        until("the book is pushed") { split.shown.count == 2 && !agent.nativeInFlight() }
        XCTAssertNil(agent.back()["error"])
        until("back at the list") { split.shown.count == 1 && !has(session, "book-a") && !agent.nativeInFlight() }
        let (_, search) = try listSearch(split)
        session.presenter.focusElement(["levels-search"])
        until("the results route is on top") { navigation.resultsInPlace != nil && !agent.nativeInFlight() }
        until("the search is open") { opened(search) }
        search.controller.searchBar.searchTextField.insertText("x")
        until("the app hears the text") { (try? node(session, "search"))?.props["value"] == "x" }
        expand()
        until("the book is back under the results") {
            !split.controller.isCollapsed && has(session, "book-a") && navigation.selectedKey == routeKey(session, "results")
                && split.detail.viewControllers.count == 2 && !agent.nativeInFlight()
        }
        XCTAssertNil(navigation.resultsInPlace, "expanded, the results are the detail's")
        XCTAssertTrue(split.detail.topViewController is RouteController)
        XCTAssertEqual((split.detail.topViewController as? RouteController)?.key, routeKey(session, "results"))
        XCTAssertEqual(try node(session, "search").props["value"], "x")
    }

    /// Expanded, the results are pushed on the detail over its page; Back pops them and the search closes, keeping its
    /// text.
    func testExpandedTheResultsArePushedOverThePageAndBackClosesTheSearch() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search in a transition only in a window a scene holds") }
        let (session, split) = try boot(size: CGSize(width: 1100, height: 800), regular: true)
        let navigation = session.presenter.navigation, agent = Agent(session: session)
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        XCTAssertNil(agent.tap(["id": Int(try node(session, "row-a").id)])["error"])
        until("the book shows") { has(session, "book-a") && split.detail.viewControllers.count == 1 && !agent.nativeInFlight() }
        let (_, search) = try listSearch(split)
        session.presenter.focusElement(["levels-search"])
        until("the results are pushed over the book") { split.detail.viewControllers.count == 2 && !agent.nativeInFlight() }
        XCTAssertEqual((split.detail.topViewController as? RouteController)?.key, routeKey(session, "results"))
        XCTAssertNil(navigation.resultsInPlace)
        until("the search is open") { opened(search) }
        search.controller.searchBar.searchTextField.insertText("x")
        until("the app hears the text") { (try? node(session, "search"))?.props["value"] == "x" }
        XCTAssertNil(agent.back()["error"])
        until("Back shows the book and closes the search") {
            split.detail.viewControllers.count == 1 && !has(session, "results") && !search.controller.isActive && !agent.nativeInFlight()
        }
        XCTAssertEqual(try node(session, "search").props["value"], "x", "Back keeps the text")
    }

    // MARK: helpers

    private func boot(size: CGSize = CGSize(width: 402, height: 874), regular: Bool = false) throws -> (ExactSession, SplitIOS) {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        if ExactEnv.authoredChrome { throw XCTSkip("under the agent's chrome the stack is not split") }
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_SEARCH_LEVELS_PLAN"], "build.mjs --test --ios compiles the fixture")))
        let session = ExactApp.shared.makeSession(label: "search-levels-\(UUID().uuidString)")
        self.session = session
        let view = ExactView(session: session), host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(origin: .zero, size: size)
        if regular { window.traitOverrides.horizontalSizeClass = .regular }
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        let agent = Agent(session: session)
        until("a split") { session.presenter.navigation.split != nil && !agent.nativeInFlight() }
        return (session, try XCTUnwrap(session.presenter.navigation.split))
    }

    private func expand() {
        guard let window else { return }
        window.frame = CGRect(x: 0, y: 0, width: 1100, height: 800)
        window.rootViewController?.view.subviews.first?.frame = window.bounds
        window.traitOverrides.horizontalSizeClass = .regular
    }

    /// UIKit has moved the bar into search and its field is the person's to type in: during that move UIKit's floating
    /// bar can take the field from one view to another, and text typed then can be lost.
    private func opened(_ search: HeaderSearch) -> Bool {
        search.controller.isActive && search.controller.transitionCoordinator == nil && search.controller.searchBar.searchTextField.isFirstResponder
    }

    private func listSearch(_ split: SplitIOS) throws -> (RouteController, HeaderSearch) {
        let list = try XCTUnwrap(split.list.viewControllers.first as? RouteController)
        return (list, try XCTUnwrap(list.search, "the list's header search"))
    }

    private func routeKey(_ session: ExactSession, _ testId: String) -> String? {
        guard let node = session.presenter.views.values.first(where: { $0.props["testId"] == testId }) else { return nil }
        return session.presenter.navigation.routeKey(containing: node)
    }

    private func until(_ what: String, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(20)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05)) }
        XCTAssertTrue(done(), "\(what): \(state())")
    }

    /// The stacks and the search as the router and UIKit hold them, for a failed wait.
    private func state() -> String {
        guard let session, let split = session.presenter.navigation.split else { return "no split" }
        let navigation = session.presenter.navigation
        let keys = { (nav: UINavigationController) in nav.viewControllers.map { ($0 as? RouteController)?.key ?? "?" }.joined(separator: ",") }
        let search = (split.list.viewControllers.first as? RouteController)?.search
        return "router \(navigation.shownKeys) selected \(navigation.selectedKey ?? "-"); list [\(keys(split.list))] detail [\(keys(split.detail))]"
            + " collapsed \(split.controller.isCollapsed); drawn \(navigation.resultsInPlace?.key ?? "-"); search active \(search?.controller.isActive == true)"
            + " editing \(search?.controller.searchBar.searchTextField.isFirstResponder == true) text \"\(search?.controller.searchBar.text ?? "")\""
            + " value \"\((try? node(session, "search"))?.props["value"] ?? "-")\""
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, testId)
    }

    private func has(_ session: ExactSession, _ testId: String) -> Bool {
        session.presenter.views.values.contains { $0.props["testId"] == testId }
    }
}
#endif
