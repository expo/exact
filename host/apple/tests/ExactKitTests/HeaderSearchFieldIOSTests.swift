#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// A route's bar search on the folded iPhone Duo: as the search opens in the toolbar, the search bar takes a new field
/// there, outside its own views, and the field read before then stays collapsed, so what reads the field reads it fresh.
final class HeaderSearchFieldIOSTests: XCTestCase {
    /// The field that edits is the search bar's field as it reads now, a new one on the folded iPhone Duo, and the
    /// route takes it as its search: the keys become its safe area, their edge on the keys' top, and a real drag of its
    /// scroller puts them away while the search is typed in, as SwiftUI's `searchable` list does, the search staying
    /// open; then the scroller's drag is the keys' again and the safe area is as it was (`header-search-keys.contract`).
    func testTheFieldThatEditsTheBarsSearchIsTheRoutesSearch() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit places a search field only in a scene") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("integrated search is iOS 26's") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "a real tap gives a search field the keys where focusing it in code does not keep them")
        }
        let plan = try fixturePlan("HEADER_SEARCH_KEYS")
        let touch = try TouchRunner(self)
        let session = ExactApp.shared.makeSession(label: "header-search-keys")
        let view = ExactView(session: session), host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer { touch.connection.cancel(); window.endEditing(true); window.isHidden = true; session.destroy() }
        host.view.addSubview(view)
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        let p = session.presenter
        let route = { p.navigationHost.primaryNavigation?.topViewController as? RouteController }
        waitFor("the route's search is the bar's", cap: 5) { route()?.search != nil }
        let controller = try XCTUnwrap(route()), search = try XCTUnwrap(controller.navigationItem.searchController)
        let surface = controller.surface
        let list = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "list" }?.scroll)
        XCTAssertEqual(list.keyboardDismissMode, .interactive, "unsaid, a drag the keys follow")
        let sampler = KeyboardSampleIOS.install(in: window)
        waitFor("the window receives touches") { Agent.frontRefusal(window) == nil }
        XCTAssertFalse(p.editsInBar(of: surface), "no search edits yet")
        let baseline = surface.safeAreaInsets.bottom, frame = surface.frame
        let before: UITextField = search.searchBar.searchTextField
        search.isActive = true
        let editing = try focusSearch(search, in: window, sampler: sampler, touch: touch)
        XCTAssertTrue(editing === search.searchBar.searchTextField, "the field that edits is the search bar's as it reads now")
        // On the folded iPhone Duo the search bar took a new field in the toolbar, out of its own views, which keeps the
        // bar's search as its delegate, while the field read before stays collapsed
        let foldedDuo = p.hasFold && window.bounds.width < 600
        if foldedDuo {
            XCTAssertFalse(editing === before, "folded, the search bar takes a new field as its search opens")
            XCTAssertFalse(sequence(first: editing as UIView, next: { $0.superview }).contains { $0 is UISearchBar }, "the new field is out of the search bar's views")
            XCTAssertTrue(editing.delegate === before.delegate, "it keeps the bar's search as its delegate")
            XCTAssertLessThan(before.bounds.width, 2 * before.bounds.height, "the field read before is collapsed")
        }
        XCTAssertTrue(p.editsInBar(of: surface), "the presenter takes it as the bar's search editing")
        waitFor("a drag of the scroller puts the keys away while the search is typed in", cap: 5) { list.keyboardDismissMode == .onDrag }
        // The keys as the route's safe area: its edge above the field that rests on them, by the toolbar that carries
        // the field (20 points on both devices), and the route's frame as it was
        let keys = try XCTUnwrap(sampler.sample).frame
        let fieldTop = surface.convert(editing.bounds, from: editing).minY
        let edge = { surface.bounds.maxY - surface.safeAreaInsets.bottom }
        waitFor("the route takes the keys and the field on them as its safe area", cap: 5,
                facts: { "safe-area edge \(edge()), field's top \(fieldTop), baseline \(baseline)" }) {
            edge() <= fieldTop + 0.5 && fieldTop - edge() <= 24
        }
        XCTAssertGreaterThan(surface.safeAreaInsets.bottom, baseline, "the keys add to the route's safe area")
        XCTAssertEqual(surface.frame, frame, "the route keeps its frame")
        // A real drag up the scroller, above that edge, scrolls it, puts the keys away and leaves the search open
        let offset = list.contentOffset.y, x = window.bounds.midX, top = window.convert(CGPoint(x: 0, y: edge()), from: surface).y
        var drag = try TouchRunner.aim(CGPoint(x: x, y: top - 30), in: window)
        let end = try TouchRunner.aim(CGPoint(x: x, y: top - 190), in: window)
        drag.merge(["op": "drag", "to": end["point"]!, "screenTo": end["screen"]!, "velocity": 300, "press": 0.05, "hold": 0.2]) { _, new in new }
        XCTAssertEqual(try touch.ask(drag)["done"] as? Bool, true, "a real drag of the scroller")
        waitFor("the drag puts the keys away", cap: 5, facts: { "keys \(String(describing: sampler.sample?.frame)), first responder \(search.searchBar.searchTextField.isFirstResponder)" }) {
            !search.searchBar.searchTextField.isFirstResponder && (sampler.sample.map { $0.frame.minY >= window.bounds.maxY - 0.5 } ?? true)
        }
        XCTAssertGreaterThan(list.contentOffset.y, offset, "the drag scrolled the scroller")
        XCTAssertTrue(search.isActive, "the search stays open")
        XCTAssertFalse(p.editsInBar(of: surface), "no search edits once the keys are away")
        waitFor("a drag the keys follow again", cap: 5) { list.keyboardDismissMode == .interactive }
        // The keys no longer count: what stays is the open search's own bar, if any
        waitFor("the route's safe area without the keys", cap: 5, facts: { "safe area \(surface.safeAreaInsets.bottom), keys were \(window.bounds.maxY - keys.minY)" }) {
            surface.safeAreaInsets.bottom < window.bounds.maxY - keys.minY
        }
    }
}
#endif
