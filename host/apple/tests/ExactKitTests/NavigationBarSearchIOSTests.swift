#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// A header's search field as UIKit moves its bar into search (`HeaderSearch`).
extension NavigationBarIOSTests {
    /// An app's blur while UIKit still moves the bar into search: UIKit makes the field the first responder again as
    /// that move completes when editing ended during it, which brought the field's `focus` back with no one there
    /// (Landscapes' results came back over a page chosen as typing began). The blur waits for the move to end
    /// (`HeaderSearch.resign`); the search, empty and put away, then closes into its button, and the field focuses
    /// again.
    func testABlurWhileTheBarMovesIntoSearchStays() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit moves a bar into search in a transition only in a window a scene holds") }
        let session = try fixture("action-search", module: false)
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        let search = try XCTUnwrap(nav.topViewController?.navigationItem.searchController)
        let source = try node(session, "header-search")
        source.props["id"] = "action-search"
        let field = search.searchBar.searchTextField
        session.presenter.focusElement(["action-search"])
        XCTAssertTrue(field.isFirstResponder)
        guard search.transitionCoordinator != nil else { throw XCTSkip("UIKit moved the bar into search at once: no transition to outlast") }
        session.presenter.blurElement(["action-search"])
        XCTAssertTrue(field.isFirstResponder, "the blur waits for the move into search")
        let over = Date().addingTimeInterval(10)
        while (search.isActive || search.transitionCoordinator != nil) && Date() < over { RunLoop.main.run(until: Date().addingTimeInterval(0.05)) }
        XCTAssertNil(search.transitionCoordinator, "the transitions are over")
        XCTAssertFalse(search.isActive, "the empty search closes")
        XCTAssertFalse(field.isFirstResponder, "UIKit's transition does not make the field the first responder again")
        session.presenter.focusElement(["action-search"])
        XCTAssertTrue(field.isFirstResponder, "the field focuses again")
    }
}
#endif
