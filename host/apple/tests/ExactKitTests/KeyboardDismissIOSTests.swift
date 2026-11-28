#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// A scroller's keyboard dismiss padding (LLP 1123 D7): the window's one padding, which the newest interactive drag
/// holds until it ends, and a real drag that starts the keyboard following that far above it.
final class KeyboardDismissIOSTests: XCTestCase {
    func testTheWindowsPaddingIsTheNewestDragsUntilNoneHoldsIt() {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        window.isHidden = false
        defer { window.isHidden = true }
        let sampler = KeyboardSampleIOS.install(in: window)
        let a = UIScrollView(), b = UIScrollView(), leaving = UIScrollView()
        // Scrollers whose drags the keys follow, as only those hold the padding
        for scroll in [a, b, leaving] {
            scroll.keyboardDismissMode = .interactive
            window.addSubview(scroll)
        }
        sampler.hold(150, for: a)
        XCTAssertEqual(sampler.dismissPadding, 150)
        sampler.hold(60, for: b)
        XCTAssertEqual(sampler.dismissPadding, 60, "the newest drag's")
        sampler.hold(140, for: a)
        XCTAssertEqual(sampler.dismissPadding, 60, "a drag that holds it again keeps its place")
        sampler.release(b)
        XCTAssertEqual(sampler.dismissPadding, 140)
        sampler.hold(90, for: leaving)
        leaving.removeFromSuperview()
        sampler.hold(140, for: a)
        XCTAssertEqual(sampler.dismissPadding, 140, "a drag out of the window holds nothing")
        sampler.release(a)
        XCTAssertEqual(sampler.dismissPadding, 0, "none holds it")
    }

    private final class Tops: NSObject {
        var lowest: CGFloat = 0
        let read: () -> CGFloat?
        private var link: CADisplayLink?
        init(_ read: @escaping () -> CGFloat?) { self.read = read }
        func start(at top: CGFloat) {
            lowest = top
            link = CADisplayLink(target: self, selector: #selector(tick))
            link?.add(to: .main, forMode: .common)
        }
        func stop() { link?.invalidate(); link = nil }
        @objc private func tick() { if let top = read() { lowest = max(lowest, top) } }
    }

    func testAScrollersPaddingStartsTheKeyboardFollowingThatFarAboveIt() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a real drag reaches UIKit") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "the keyboard follows a real drag of the scroller")
        }
        let touch = try TouchRunner(self)
        let session = ExactApp.shared.makeSession(label: "keyboard-dismiss-padding")
        let window = UIWindow(windowScene: scene), host = UIViewController()
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer { touch.connection.cancel(); window.endEditing(true); window.isHidden = true; session.destroy() }
        let p = session.presenter, size = host.view.bounds.size
        p.viewport.frame = host.view.bounds
        host.view.addSubview(p.viewport)
        func scroller(_ padding: Double) -> [String: Any] { ["overflow_y": "scroll", "keyboard_dismiss_padding": padding] }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "style": scroller(150)],
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": size.width, "h": size.height],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": size.width, "h": size.height],
            ["op": "content", "id": 2, "w": size.width, "h": 4000.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": size.width, "h": 4000.0],
        ]))
        let scroll = try XCTUnwrap(p.views[2]?.scroll)
        XCTAssertEqual(scroll.keyboardDismissMode, .interactive)
        let field = UITextField(frame: CGRect(x: 20, y: 120, width: 240, height: 44))
        try XCTUnwrap(p.views[3]).addSubview(field)
        let sampler = KeyboardSampleIOS.install(in: window)
        let tops = Tops { sampler.sample?.frame.minY }
        waitFor("the window receives touches") { Agent.frontRefusal(window) == nil }
        // How far the keyboard's top goes down while a finger drags the scroller from 260 points above it to 30
        // above it and holds there, never reaching it
        func followed() throws -> CGFloat {
            scroll.setContentOffset(.zero, animated: false)
            if !field.isFirstResponder { field.becomeFirstResponder() }
            waitFor("the keyboard is up", cap: 15) { sampler.sample.map { $0.frame.maxY >= window.bounds.maxY - 0.5 } ?? false }
            waitForRunLoop(seconds: 1)
            let top = try XCTUnwrap(sampler.sample).frame.minY, x = window.bounds.midX
            var request = try TouchRunner.aim(CGPoint(x: x, y: top - 260), in: window)
            let end = try TouchRunner.aim(CGPoint(x: x, y: top - 30), in: window)
            request.merge(["op": "drag", "to": end["point"]!, "screenTo": end["screen"]!,
                           "velocity": 150, "press": 0.05, "hold": 0.6]) { _, new in new }
            tops.start(at: top)
            XCTAssertEqual(try touch.ask(request)["done"] as? Bool, true)
            tops.stop()
            waitFor("the drag's end lets the padding go", cap: 5) { sampler.dismissPadding == 0 }
            return tops.lowest - top
        }
        XCTAssertGreaterThan(try followed(), 100, "150 points of padding: the keyboard follows a finger 30 points above it")
        p.apply(wireBatch([["op": "style", "id": 2, "style": scroller(0)]]))
        XCTAssertEqual(try followed(), 0, accuracy: 0.5, "with none it waits for the finger")
    }

    /// A route bar's search field is part of the keys for their dismissal
    /// only where UIKit carries it onto them, with the keys up: an integrated
    /// search, out of every navigation bar and resting on the keys while it
    /// edits (the toolbar's on an iPhone); a stacked one, which the bar keeps
    /// at the top, is not, nor an integrated one kept out of the toolbar,
    /// which UIKit integrates into the navigation bar at the top.
    func testABarSearchFieldIsTheKeysOnlyWhereUIKitCarriesItOntoThem() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit places a search field only in a scene") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("integrated search is iOS 26's") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "a real tap gives a search field the keys where focusing it in code does not keep them")
        }
        let touch = try TouchRunner(self)
        defer { touch.connection.cancel() }
        let cases: [(placement: UINavigationItem.SearchBarPlacement, toolbar: Bool, carried: Bool)] = [(.integrated, true, true), (.stacked, false, false), (.integrated, false, false)]
        for (placement, toolbar, expected) in cases {
            let window = UIWindow(windowScene: scene)
            window.frame = scene.coordinateSpace.bounds
            let list = UITableViewController(style: .plain)
            let search = UISearchController(searchResultsController: nil)
            list.navigationItem.searchController = search
            list.navigationItem.preferredSearchBarPlacement = placement
            list.navigationItem.searchBarPlacementAllowsToolbarIntegration = toolbar
            let nav = UINavigationController(rootViewController: list)
            nav.isToolbarHidden = !toolbar
            window.rootViewController = nav
            window.makeKeyAndVisible()
            defer { window.endEditing(true); window.isHidden = true }
            let sampler = KeyboardSampleIOS.install(in: window)
            waitFor("the list shows", cap: 5) { list.viewIfLoaded?.window != nil }
            search.isActive = true
            let field = try focusSearch(search, within: nav.view, in: window, sampler: sampler, touch: touch)
            let keys = try XCTUnwrap(sampler.sample).frame
            let carried = KeyboardDrag.carriedOnKeys(field, item: list.navigationItem, keys: keys, in: window)
            let shown = "realized \(list.navigationItem.searchBarPlacement.rawValue), in a navigation bar \(sequence(first: field as UIView, next: { $0.superview }).contains { $0 is UINavigationBar }), field \(field.convert(field.bounds, to: window)), keys \(keys)"
            XCTAssertEqual(carried, expected, "\(placement.rawValue), toolbar integration \(toolbar): \(shown)")
            // Each guard alone, against keys made to meet the others: right under the field and across it
            let fieldFrame = field.convert(field.bounds, to: window)
            let under = CGRect(x: fieldFrame.minX, y: fieldFrame.maxY, width: fieldFrame.width, height: 300)
            if expected {
                let aside = under.offsetBy(dx: window.bounds.width, dy: 0)
                XCTAssertFalse(KeyboardDrag.carriedOnKeys(field, item: list.navigationItem, keys: aside, in: window), "keys beside the field do not hold it: \(shown)")
                XCTAssertTrue(KeyboardDrag.carriedOnKeys(field, item: list.navigationItem, keys: under, in: window), "keys right under it do: \(shown)")
            } else if placement == .integrated {
                XCTAssertTrue(list.navigationItem.searchBarPlacement == .integrated || list.navigationItem.searchBarPlacement == .integratedButton
                              || list.navigationItem.searchBarPlacement == .integratedCentered, "integrated, kept out of the toolbar: \(shown)")
                XCTAssertTrue(sequence(first: field as UIView, next: { $0.superview }).contains { $0 is UINavigationBar }, "in the navigation bar at the top: \(shown)")
                XCTAssertFalse(KeyboardDrag.carriedOnKeys(field, item: list.navigationItem, keys: under, in: window), "a field in a navigation bar is not the keys', even with keys right under it: \(shown)")
            }
        }
    }

    /// The window's padding is its newest drag's whose scroller the keys still follow, each drag driven through its
    /// pan's states: a drag whose dismissal changes and comes back keeps its place, the bar's field the keys carried
    /// counts for a drag outside it and not for one whose scroller holds it, none leaves 0, and the field is forgotten
    /// once it stops editing.
    func testTheWindowsPaddingAndCarriedFieldFollowTheirHolders() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keyboard's guide lives in a scene's window") }
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        defer { window.endEditing(true); window.isHidden = true }
        let sampler = KeyboardSampleIOS.install(in: window)
        let older = UIScrollView(frame: window.bounds), newer = UIScrollView(frame: window.bounds)
        for scroll in [older, newer] {
            scroll.keyboardDismissMode = .interactive
            window.rootViewController?.view.addSubview(scroll)
        }
        let olderDrag = KeyboardDrag.watch(older) { nil }, newerDrag = KeyboardDrag.watch(newer) { nil }
        let item = UINavigationItem()
        item.searchController = UISearchController(searchResultsController: nil)
        let owner = SearchOwner(item), field = UITextField(frame: CGRect(x: 0, y: 100, width: 200, height: 36))
        field.delegate = owner
        older.addSubview(field)
        XCTAssertTrue(field.becomeFirstResponder())
        olderDrag.track(.began)
        newerDrag.track(.began)
        sampler.carried = KeyboardDrag.Carried(field: field, offset: 50)
        newerDrag.track(.changed)
        XCTAssertEqual(sampler.dismissPadding, 50, "the newest drag's, the carried field's top outside its scroller")
        older.keyboardDismissMode = .none
        olderDrag.track(.changed)
        older.keyboardDismissMode = .interactive
        olderDrag.track(.changed)
        XCTAssertEqual(sampler.dismissPadding, 50, "a drag whose dismissal changed and came back keeps its place")
        newer.keyboardDismissMode = .onDrag
        newerDrag.track(.changed)
        XCTAssertEqual(sampler.dismissPadding, 0, "the drag before, whose scroller holds the field, takes only its own padding")
        older.keyboardDismissMode = .none
        olderDrag.track(.changed)
        XCTAssertEqual(sampler.dismissPadding, 0, "none whose drags the keys follow")
        XCTAssertNotNil(sampler.carried, "kept while the drags hold the padding")
        XCTAssertTrue(field.resignFirstResponder())
        waitFor("the carried field is forgotten as it stops editing", cap: 2) { sampler.carried == nil }
        olderDrag.track(.ended)
        newerDrag.track(.ended)
        XCTAssertFalse(sampler.holdsNow())
        withExtendedLifetime(owner) {}
    }

    private final class SearchOwner: NSObject, UITextFieldDelegate, BarSearchOwner {
        let searchItem: UINavigationItem?
        init(_ item: UINavigationItem) { searchItem = item }
    }

    /// A real drag of a route's scroller under its bar's search field that
    /// UIKit carried onto the keys (LLP 1123 D7, `bar-field-drag.contract`):
    /// with no padding of its own the window holds the field's top, and the
    /// keys follow a finger that crosses it though it never reaches them; with
    /// a larger padding of its own the window holds that.
    func testTheKeysFollowADragFromTheTopOfABarFieldTheyCarry() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a real drag reaches UIKit") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("integrated search is iOS 26's") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "the keys follow a real drag of the scroller")
        }
        let plan = try fixturePlan("BAR_FIELD_DRAG")
        let touch = try TouchRunner(self)
        let session = ExactApp.shared.makeSession(label: "bar-field-drag")
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
        let item = try XCTUnwrap(route()).navigationItem
        // The field that edits, which the search bar may replace as its search opens (`focus`)
        var field: UITextField = try XCTUnwrap(item.searchController).searchBar.searchTextField
        let list = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "list" }?.scroll)
        XCTAssertEqual(list.keyboardDismissMode, .interactive)
        let sampler = KeyboardSampleIOS.install(in: window)
        var held: CGFloat = 0, lowestKeys: CGFloat = 0, fieldThen: CGFloat = 0
        // The keys' top, the padding held, and the field's top on the last frame the keys were at their lowest, as the
        // finger holds still
        let tops = Tops {
            held = max(held, sampler.dismissPadding)
            guard let top = sampler.sample?.frame.minY else { return nil }
            if top >= lowestKeys { (lowestKeys, fieldThen) = (top, field.convert(field.bounds, to: window).minY) }
            return top
        }
        waitFor("the window receives touches") { Agent.frontRefusal(window) == nil }
        // How far the keys' top goes down, and the padding the window held, while a finger drags the scroller from
        // 200 points above the field to 2 points above its bottom, which is above the keys
        func dragged() throws -> (followed: CGFloat, held: CGFloat, offset: CGFloat, crossed: CGFloat, fieldFollowed: CGFloat) {
            list.setContentOffset(CGPoint(x: 0, y: -list.adjustedContentInset.top), animated: false)
            item.searchController?.isActive = true
            field = try focusSearch(try XCTUnwrap(item.searchController), within: window, in: window, sampler: sampler, touch: touch)
            let keys = try XCTUnwrap(sampler.sample).frame, shown = field.convert(field.bounds, to: window)
            XCTAssertTrue(KeyboardDrag.carriedOnKeys(field, keys: keys, in: window),
                          "realized \(item.searchBarPlacement.rawValue), field \(shown), keys \(keys)")
            let x = window.bounds.midX, stop = shown.maxY - 2
            XCTAssertLessThan(stop, keys.minY, "the finger stops above the keys")
            var request = try TouchRunner.aim(CGPoint(x: x, y: shown.minY - 200), in: window)
            let end = try TouchRunner.aim(CGPoint(x: x, y: stop), in: window)
            request.merge(["op": "drag", "to": end["point"]!, "screenTo": end["screen"]!,
                           "velocity": 150, "press": 0.05, "hold": 0.6]) { _, new in new }
            (held, lowestKeys, fieldThen) = (0, keys.minY, shown.minY)
            tops.start(at: keys.minY)
            XCTAssertEqual(try touch.ask(request)["done"] as? Bool, true)
            tops.stop()
            waitFor("the drag's end lets the padding go", cap: 5) { sampler.dismissPadding == 0 }
            return (tops.lowest - keys.minY, held, keys.minY - shown.minY, stop - shown.minY, fieldThen - shown.minY)
        }
        let carried = try dragged()
        XCTAssertEqual(carried.held, carried.offset, accuracy: 0.5, "no padding of its own: the window holds the field's top")
        XCTAssertGreaterThan(carried.followed, carried.crossed / 2, "the keys follow from the field's top (\(carried))")
        XCTAssertEqual(carried.fieldFollowed, carried.followed, accuracy: 1, "held still, the field rides on the keys at its gap (\(carried))")
        p.press(try XCTUnwrap(p.views.values.first { $0.props["testId"] == "pad" }).id)
        waitFor("the scroller's own padding", cap: 5) { p.views.values.first { $0.props["testId"] == "list" }?.style["keyboard_dismiss_padding"]?.number == 300 }
        let padded = try dragged()
        XCTAssertEqual(padded.held, 300, accuracy: 0.5, "its own padding, the larger, is the window's (\(padded))")
    }

    private func waitForRunLoop(seconds: TimeInterval) {
        let until = Date().addingTimeInterval(seconds)
        while Date() < until { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01)) }
    }
}
#endif
