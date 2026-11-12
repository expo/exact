#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

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
        until("editing ends as the push is written") { !field.isFirstResponder }
        until("the detail shows") { nav.topViewController !== home && nav.transitionCoordinator == nil }
        nav.popViewController(animated: true)
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

    /// The person's close takes UIKit's emptying of the field to the app, the platform's default; an app whose
    /// `cancel` calls `preventDefault()` keeps its text, which the bar shows, closed, once UIKit has dismissed it.
    /// (`isActive = false` from outside the host is the person's close, as the Duo's close button dismisses the
    /// controller without the bar's Cancel action.)
    func testTheCloseButtonClearsTheTextUnlessCancelPreventsIt() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search in a transition only in a window a scene holds") }
        for (typed, kept) in [("abc", ""), ("keep it", "keep it")] {
            let (session, _, search) = try openSearch()
            let field = search.searchBar.searchTextField, input = try node(session, "header-search")
            field.insertText(typed)
            until("the app hears \(typed)") { input.props["value"] == typed }
            search.isActive = false
            until("UIKit closes the search (\(typed))") { !search.isActive && search.transitionCoordinator == nil && !field.isFirstResponder }
            spin(0.3)
            XCTAssertEqual(input.props["value"], kept, typed)
            XCTAssertEqual(search.searchBar.text ?? "", kept, typed)
            if !kept.isEmpty {
                session.presenter.focusElement(["keyboard-search"])
                until("the search opens again with its text") { search.isActive && field.isFirstResponder && search.transitionCoordinator == nil }
                XCTAssertEqual(search.searchBar.text, kept)
            }
            session.destroy(); self.session = nil; window?.isHidden = true
        }
    }

    /// The host's own close (`HeaderSearch.close()`) fires no `cancel` and keeps the text, whatever the app's `cancel`.
    func testTheHostsOwnCloseKeepsTheText() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit presents a search in a transition only in a window a scene holds") }
        let (session, nav, search) = try openSearch()
        let field = search.searchBar.searchTextField, input = try node(session, "header-search")
        field.insertText("abc")
        until("the app hears the text") { input.props["value"] == "abc" }
        try XCTUnwrap((nav.topViewController as? RouteController)?.search).close()
        until("the search closes") { !search.isActive && search.transitionCoordinator == nil && !field.isFirstResponder }
        spin(0.3)
        XCTAssertEqual(input.props["value"], "abc")
        XCTAssertEqual(search.searchBar.text, "abc")
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
