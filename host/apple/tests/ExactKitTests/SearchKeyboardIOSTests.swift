#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A header's search field and the keyboard as the search's route is covered and shown again, and as editing ends
/// with nothing typed (`HeaderSearch.refusesBegin`, `HeaderSearch.closeIfIdle`).
final class SearchKeyboardIOSTests: XCTestCase {
    private var session: ExactSession?
    private var window: UIWindow?

    override func tearDown() {
        session?.destroy(); session = nil
        window?.isHidden = true; window = nil
        super.tearDown()
    }

    /// UIKit's search controller makes a field it resigned for a push the first responder again in the completion of
    /// the pop that shows its route (`_navigationControllerWillShowViewController:`, with the Duo's toolbar search);
    /// the test asks there as UIKit does. The field stays as it was left: the search open with its text, the
    /// keyboard down.
    func testBackToAnOpenSearchKeepsTheKeyboardDown() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search in a transition only in a window a scene holds") }
        let (session, nav, search) = try openSearch()
        let home = try XCTUnwrap(nav.topViewController), field = search.searchBar.searchTextField
        field.insertText("a")
        // As a result opened from the search does
        session.presenter.press(try node(session, "detail").id)
        until("the detail shows") { nav.topViewController !== home && nav.transitionCoordinator == nil }
        XCTAssertFalse(field.isFirstResponder, "UIKit resigns the field for the push")
        nav.popViewController(animated: true)
        let pop = try XCTUnwrap(nav.transitionCoordinator)
        pop.animate(alongsideTransition: nil) { _ in _ = field.becomeFirstResponder() }
        until("the search's route shows again") { nav.topViewController === home && nav.transitionCoordinator == nil }
        spin(0.5)
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
        spin(0.5)
        XCTAssertTrue(search.isActive, "with text the search stays open")
        session.presenter.focusElement(["keyboard-search"])
        until("the field edits again") { field.isFirstResponder && search.transitionCoordinator == nil }
        field.deleteBackward()
        XCTAssertEqual(search.searchBar.text, "")
        XCTAssertTrue(field.resignFirstResponder())
        until("the empty search closes", 10) { !search.isActive }
        until("its close ends", 10) { search.transitionCoordinator == nil }
    }

    /// The fixture's home route with its header's search open and its field editing.
    private func openSearch() throws -> (ExactSession, UINavigationController, UISearchController) {
        let session = try fixture()
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        let search = try XCTUnwrap(nav.topViewController?.navigationItem.searchController)
        try node(session, "header-search").props["id"] = "keyboard-search"
        session.presenter.focusElement(["keyboard-search"])
        until("the bar is in search") { search.isActive && search.transitionCoordinator == nil }
        XCTAssertTrue(search.searchBar.searchTextField.isFirstResponder)
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
        until("the fixture's route") { session.presenter.navigation.primaryNavigation?.topViewController is RouteController }
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, testId)
    }

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }

    private func spin(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }
}
#endif
