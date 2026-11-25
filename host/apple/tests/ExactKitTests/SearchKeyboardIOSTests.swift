#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// A header's search field and the keyboard as the search's route is covered and shown again, and as editing ends
/// with nothing typed (`NavigationHost.searchCovered`, `HeaderSearch.settle`).
final class SearchKeyboardIOSTests: XCTestCase {
    private var session: ExactSession?
    private var window: UIWindow?

    override func tearDown() {
        session?.destroy(); session = nil
        window?.isHidden = true; window = nil
        super.tearDown()
    }

    /// UIKit makes a bar's search field the first responder again as the pop shows its route only when the field was
    /// editing as the push covered it (measured on the Duo's toolbar search). The host ends the editing before the
    /// push, so Back leaves the field as the person left it: the search open with its text, the keyboard down.
    func testBackToAnOpenSearchKeepsTheKeyboardDown() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search in a transition only in a window a scene holds") }
        let (session, nav, search) = try openSearch()
        let home = try XCTUnwrap(nav.topViewController), field = search.searchBar.searchTextField
        field.insertText("a")
        // As a result opened from the search does
        session.presenter.press(try node(session, "detail").id)
        let agent = Agent(session: session)
        waitFor("the detail shows") { nav.topViewController !== home }
        waitFor("the push settles") { !agent.nativeInFlight() }
        XCTAssertFalse(field.isFirstResponder)
        nav.popViewController(animated: true)
        waitFor("the search's route shows again") { nav.topViewController === home }
        waitFor("the pop settles") { !agent.nativeInFlight() }
        XCTAssertFalse(field.isFirstResponder, "the keyboard stays down")
        XCTAssertTrue(search.isActive, "the search stays open")
        XCTAssertEqual(search.searchBar.text, "a")
        session.presenter.focusElement(["keyboard-search"])
        XCTAssertTrue(field.isFirstResponder, "the field focuses again")
    }

    /// The keyboard put away from an empty field (a drag on the content, as `keyboardDismissMode` resigns it) closes
    /// the search into its button; with text it stays open.
    func testAnEmptySearchClosesAsEditingEnds() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search in a transition only in a window a scene holds") }
        let (session, _, search) = try openSearch()
        let field = search.searchBar.searchTextField
        field.insertText("a")
        XCTAssertTrue(field.resignFirstResponder())
        waitForMainQueue("the host handles the end of editing")
        XCTAssertTrue(search.isActive, "with text the search stays open")
        session.presenter.focusElement(["keyboard-search"])
        waitFor("the field edits again") { field.isFirstResponder }
        field.deleteBackward()
        XCTAssertEqual(search.searchBar.text, "")
        XCTAssertTrue(field.resignFirstResponder())
        waitFor("the empty search closes") { !search.isActive }
        waitFor("the close settles") { search.transitionCoordinator == nil }
    }

    func testPreventedDismissalAndHostCloseKeepTheQuery() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search in a transition only in a window a scene holds") }
        let (session, nav, search) = try openSearch()
        let input = try node(session, "header-search")
        let projection = try XCTUnwrap((nav.topViewController as? RouteController)?.search)
        projection.bindEditor().insertText("keep it")
        waitFor("the app hears the first query") { input.props["value"] == "keep it" }
        search.isActive = false
        waitFor("UIKit settles the prevented dismissal") { !projection.dismissing }
        XCTAssertEqual(input.props["value"], "keep it")
        XCTAssertEqual(search.searchBar.text, "keep it")
        session.presenter.focusElement(["keyboard-search"])
        waitFor("the field reopens") { projection.bindEditor().isFirstResponder }
        waitFor("the reopened search settles") { !projection.presenting }
        XCTAssertEqual(search.searchBar.text, "keep it")

        projection.bindEditor().selectAll(nil)
        projection.bindEditor().insertText("abc")
        waitFor("the app hears the new query") { input.props["value"] == "abc" }
        projection.close()
        waitFor("UIKit settles the host's close") { !projection.dismissing }
        XCTAssertEqual(input.props["value"], "abc")
        XCTAssertEqual(search.searchBar.text, "abc")
    }

    func testSearchClearanceUsesOnlyTheResidualOverlapInEachPort() {
        let sidebar = CGRect(x: 0, y: 50, width: 300, height: 550)
        let detail = CGRect(x: 300, y: 50, width: 500, height: 550)
        let fitted = CGRect(x: 0, y: 50, width: 300, height: 310)
        let keys = CGRect(x: 0, y: 400, width: 800, height: 250)
        let bar = CGRect(x: 100, y: 360, width: 450, height: 80)
        let docked = SearchClearance.Occupancy(keyboard: keys, docked: true, bar: bar)
        let resized = CGRect(x: 0, y: 50, width: 300, height: 350)
        XCTAssertEqual(docked.contentBlock(sidebar, policy: "resizes-content", ignoringKeyboard: false), resized)
        XCTAssertEqual(docked.bottomDepth(in: resized), 40, "only the bar remains after the viewport resizes")
        XCTAssertEqual(docked.contentBlock(sidebar, policy: "resizes-content", ignoringKeyboard: true), sidebar)
        for policy in ["resizes-visual", "overlays-content"] {
            XCTAssertEqual(docked.contentBlock(sidebar, policy: policy, ignoringKeyboard: false), sidebar)
        }
        XCTAssertEqual(docked.bottomDepth(in: sidebar), 240)
        XCTAssertEqual(docked.bottomDepth(in: detail), 240)
        XCTAssertEqual(docked.bottomDepth(in: fitted), 0, "UIKit already fitted this viewport")
        XCTAssertEqual(docked.bottomDepth(in: CGRect(x: 600, y: 50, width: 200, height: 550)), 200, "the bar does not reach this column")
        let floating = SearchClearance.Occupancy(keyboard: CGRect(x: 410, y: 270, width: 180, height: 150))
        XCTAssertEqual(floating.contentBlock(detail, policy: "resizes-content", ignoringKeyboard: false), detail)
        XCTAssertEqual(floating.bottomDepth(in: detail), 0, "an interior keyboard rectangle adds no bottom inset")
        XCTAssertNil(floating.floatingKeyboard(in: sidebar))
        XCTAssertEqual(floating.floatingKeyboard(in: detail), floating.keyboard)
        let straddling = SearchClearance.Occupancy(keyboard: CGRect(x: 240, y: 270, width: 180, height: 150))
        XCTAssertEqual(straddling.floatingKeyboard(in: sidebar), CGRect(x: 240, y: 270, width: 60, height: 150))
        XCTAssertEqual(straddling.floatingKeyboard(in: detail), CGRect(x: 300, y: 270, width: 120, height: 150))
        XCTAssertEqual(SearchClearance.Occupancy().bottomDepth(in: sidebar), 0, "the dismissed guide adds no clearance")

        let presenter = Presenter()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 800, height: 650))
        let view = UIView(frame: window.bounds)
        window.addSubview(view)
        let root = NodeView(id: 1, kind: "scroll", presenter: presenter)
        root.frame = sidebar
        root.style["overflow_y"] = .string("scroll")
        view.addSubview(root)
        let scroll = ScrollView(frame: root.bounds)
        scroll.contentInsetAdjustmentBehavior = .never
        root.scroll = scroll
        root.addSubview(scroll)

        let nested = NodeView(id: 2, kind: "scroll", presenter: presenter)
        nested.frame = root.bounds
        nested.style["overflow_y"] = .string("scroll")
        root.container.addSubview(nested)
        let inner = ScrollView(frame: nested.bounds)
        inner.contentInsetAdjustmentBehavior = .never
        inner.contentInset.bottom = 12
        nested.scroll = inner
        nested.addSubview(inner)
        XCTAssertEqual(inner.convert(inner.bounds, to: view).maxY, sidebar.maxY)

        let clearance = SearchClearance()
        _ = clearance.fit(root, in: view, allocation: sidebar, occupancy: docked, policy: "resizes-visual", enabled: true)
        XCTAssertEqual(scroll.contentInset.bottom, 240, "the outer port clears the keyboard and the bar above it")
        XCTAssertEqual(inner.contentInset.bottom, 12, "the nested port keeps its own inset")
        root.props["keyboardAvoidance"] = "none"
        _ = clearance.fit(root, in: view, allocation: sidebar, occupancy: docked, policy: "resizes-visual", enabled: true)
        XCTAssertEqual(scroll.contentInset.bottom, 200, "the opt-out removes only the bar's clearance")
        _ = clearance.fit(root, in: view, allocation: sidebar, occupancy: docked, policy: "overlays-content", enabled: true)
        XCTAssertEqual(scroll.contentInset.bottom, 0, "overlays-content releases the inset with the keyboard still present")
    }

    /// The fixture's home route with its header's search open and its field editing.
    private func openSearch() throws -> (ExactSession, UINavigationController, UISearchController) {
        let session = try fixture()
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        let search = try XCTUnwrap(nav.topViewController?.navigationItem.searchController)
        try node(session, "header-search").props["id"] = "keyboard-search"
        session.presenter.focusElement(["keyboard-search"])
        waitFor("the field begins editing") { search.searchBar.searchTextField.isFirstResponder }
        waitFor("the search presentation settles") { search.transitionCoordinator == nil }
        return (session, nav, search)
    }

    private func fixture() throws -> ExactSession {
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_FIXTURE_PLAN"])))
        let session = ExactApp.shared.makeSession(label: "search-keyboard")
        self.session = session
        let view = ExactView(session: session), host = UIViewController()
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        waitFor("the fixture's route") { session.presenter.navigationHost.primaryNavigation?.topViewController is RouteController }
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, testId)
    }

}
#endif
