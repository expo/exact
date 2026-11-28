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
        // Whether the device folds is known once UIKit 27.1 has reported its hinge, or that it has none
        if #available(iOS 27.1, *) { waitFor("the device's fold is known", cap: 5) { p.hingeReported } }
        let folded = p.hasFold && window.bounds.width < 600
        let barSearch = try XCTUnwrap(controller.search)
        let before: UITextField = search.searchBar.searchTextField
        search.isActive = true
        let editing = try focusSearch(search, within: controller.navigationController?.view ?? window, in: window, sampler: sampler, touch: touch)
        XCTAssertTrue(editing === search.searchBar.searchTextField, "the field that edits is the search bar's as it reads now")
        XCTAssertTrue(editing.delegate === barSearch, "the field that edits has the bar's search as its delegate")
        // As a run once failed: the route's editor was the collapsed field read before while the keys' editor was the new one
        XCTAssertTrue(controller.headerSearchEditor === editing, "the route's header field is the field that edits")
        XCTAssertTrue(editing.pasteDelegate === barSearch.field, "the field that edits pastes through the authored field")
        XCTAssertEqual(editing.accessibilityIdentifier, "search", "the field that edits carries the test id")
        XCTAssertEqual(editing.accessibilityLabel, "Search chats", "the field that edits is named by the field's aria-label")
        XCTAssertEqual(editing.placeholder, "Find a chat", "the field that edits shows the authored placeholder")
        // Folded, on the iPhone Duo, the search bar took a new field in the toolbar, out of its own views, while the
        // field read before stays collapsed
        if folded {
            XCTAssertFalse(editing === before, "folded, the search bar takes a new field as its search opens")
            XCTAssertFalse(sequence(first: editing as UIView, next: { $0.superview }).contains { $0 is UISearchBar }, "the new field is out of the search bar's views")
            XCTAssertLessThan(before.bounds.width, 2 * before.bounds.height, "the field read before is collapsed")
        }
        waitFor("the presenter takes it as the bar's search editing", cap: 3, facts: {
            "the route's header field is the one that edits \(controller.headerSearchEditor === editing) (\(String(describing: controller.headerSearchEditor.map { $0.convert($0.bounds, to: window) }))), the keyboard's editor is it \(p.keyboardEditor === editing), first responder \(String(describing: FirstResponder.current.map { type(of: $0) }))"
        }) { p.editsInBar(of: surface) }
        waitFor("a drag of the scroller puts the keys away while the search is typed in", cap: 5) { list.keyboardDismissMode == .onDrag }
        // The route's safe area, UIKit's own, reaches the toolbar that carries the field on the keys: read together
        // with the field that edits and the docked keys, its edge stands above the field by that toolbar's room
        let keys = try XCTUnwrap(sampler.sample).frame
        let edge = { surface.bounds.maxY - surface.safeAreaInsets.bottom }
        let onKeys = { () -> Bool in
            let field = search.searchBar.searchTextField
            guard field.isFirstResponder, let sample = sampler.sample, sample.docked else { return false }
            let shown = surface.convert(field.bounds, from: field), keysTop = surface.convert(CGPoint(x: 0, y: sample.frame.minY), from: window).y
            return edge() <= shown.minY + 0.5 && shown.minY - edge() <= 24 && shown.maxY <= keysTop + 0.5
        }
        waitFor("the route takes the keys and the field on them as its safe area", cap: 5,
                facts: { "safe-area edge \(edge()), field \(search.searchBar.searchTextField.convert(search.searchBar.searchTextField.bounds, to: surface)), keys \(String(describing: sampler.sample?.frame)), baseline \(baseline)" }) { onKeys() }
        XCTAssertGreaterThan(surface.safeAreaInsets.bottom, baseline, "the keys add to the route's safe area")
        XCTAssertEqual(surface.frame, frame, "the route keeps its frame")
        // A real drag up the scroller, above that edge, scrolls it, puts the keys away and leaves the search open
        let offset = list.contentOffset.y, x = window.bounds.midX, top = window.convert(CGPoint(x: 0, y: edge()), from: surface).y
        let start = CGPoint(x: x, y: top - 30)
        XCTAssertTrue(window.hitTest(start, with: nil)?.isDescendant(of: list) == true, "the drag starts in the scroller")
        var drag = try TouchRunner.aim(start, in: window)
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
        // The keys no longer count: what stays is UIKit's for the open search's own bar, if any, and Exact adds nothing
        let added = { ContributionsIOS.of(controller).insets(of: ContributionsIOS.Identity(node: controller.node.id, operation: .keyboardInsets)).bottom }
        waitFor("the route's safe area without the keys", cap: 5, facts: { "safe area \(surface.safeAreaInsets.bottom), keys were \(window.bounds.maxY - keys.minY), Exact's keyboard inset \(added())" }) {
            surface.safeAreaInsets.bottom < window.bounds.maxY - keys.minY && added() == 0
        }
        // The bar's search names its item for the keyboard's drag only while the item holds the search's controller
        XCTAssertTrue(barSearch.searchItem === controller.navigationItem, "the bar's search names its item")
        controller.navigationItem.searchController = UISearchController(searchResultsController: nil)
        XCTAssertNil(barSearch.searchItem, "nor once another controller replaces its own")
    }

    /// A paste UIKit resolves for an editor that no longer stands for the authored field (a bar's search retired or
    /// set up again before the paste lands) reaches neither field; one from the field that stands for it goes in.
    func testAPasteFromAnEditorThatNoLongerStandsForTheFieldReachesNeither() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit places a search field only in a scene") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("integrated search is iOS 26's") }
        let plan = try fixturePlan("HEADER_SEARCH_KEYS")
        let session = ExactApp.shared.makeSession(label: "header-search-paste")
        let view = ExactView(session: session), host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer { window.isHidden = true; session.destroy() }
        host.view.addSubview(view)
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        let p = session.presenter
        let route = { p.navigationHost.primaryNavigation?.topViewController as? RouteController }
        waitFor("the route's search is the bar's", cap: 5) { route()?.search != nil }
        let controller = try XCTUnwrap(route()), search = try XCTUnwrap(controller.navigationItem.searchController)
        let authored = try XCTUnwrap(controller.search?.field)
        let end = { (field: UITextField) in try XCTUnwrap(field.textRange(from: field.endOfDocument, to: field.endOfDocument)) }
        let retired = UITextField()
        retired.text = "kept"
        let text = authored.fieldText
        _ = authored.textPasteConfigurationSupporting(retired, performPasteOf: NSAttributedString(string: "late"), to: try end(retired))
        XCTAssertEqual(retired.text, "kept", "an editor that does not stand for the field takes no paste through it")
        XCTAssertEqual(authored.fieldText, text, "nor does the field")
        let field = search.searchBar.searchTextField, shown = field.text ?? ""
        _ = authored.textPasteConfigurationSupporting(field, performPasteOf: NSAttributedString(string: "now"), to: try end(field))
        XCTAssertEqual(field.text, shown + "now", "the field that stands for it takes the paste")
    }
}
#endif
