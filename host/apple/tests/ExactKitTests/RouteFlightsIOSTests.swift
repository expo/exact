#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactFlights

/// LLP 1013.004 D1, D3: across UIKit's push, the name both routes hold once
/// flies alongside it in the transition's container, the leaver hidden, and
/// lands where its route lays it out; a name held twice stays put.
final class RouteFlightsIOSTests: XCTestCase {
    private var window: UIWindow!
    override func tearDown() { window?.isHidden = true; window = nil; super.tearDown() }

    private final class Delegate: NSObject, UINavigationControllerDelegate {
        var willShow: ((UINavigationController) -> Void)?
        func navigationController(_ nav: UINavigationController, willShow viewController: UIViewController, animated: Bool) { willShow?(nav) }
    }

    func testANameBothRoutesHoldFliesAcrossAPushAndLands() throws {
        let p = Presenter()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 800)
        window.addSubview(p.viewport)
        // Every node carries a testId, so none is drawn as a flat leaf (FlatLeavesIOS).
        func node(_ id: Int, _ name: String?) -> [String: Any] {
            var props: [String: Any] = ["testId": "n\(id)"]
            if let name { props["sharedElement"] = name }
            return ["op": "create", "id": id, "kind": "view", "props": props]
        }
        func frame(_ id: Int, _ y: Double, _ h: Double) -> [String: Any] { ["op": "frame", "id": id, "x": 0.0, "y": y, "w": 400.0, "h": h] }
        p.apply(wireBatch([
            node(1, nil), node(2, "photo"), node(3, "dup"), node(4, "dup"),
            node(5, nil), node(6, "photo"), node(7, "dup"),
            ["op": "children", "id": 1, "ids": [2, 3, 4]], ["op": "children", "id": 5, "ids": [6, 7]], ["op": "roots", "ids": [1, 5]],
            frame(1, 0, 800), frame(2, 100, 64), frame(3, 200, 20), frame(4, 240, 20),
            frame(5, 0, 800), frame(6, 0, 300), frame(7, 320, 20)]))
        let list = UIViewController(), detail = UIViewController()
        list.view.addSubview(try XCTUnwrap(p.views[1]))
        detail.view.addSubview(try XCTUnwrap(p.views[5]))
        let leaver = try XCTUnwrap(p.views[2]), arriver = try XCTUnwrap(p.views[6]), place = try XCTUnwrap(arriver.superview)
        let nav = UINavigationController(rootViewController: list)
        let delegate = Delegate()
        var crossed = false
        delegate.willShow = { nav in
            guard let coordinator = nav.transitionCoordinator else { return }
            crossed = p.crossFlights(coordinator, leaving: list.view, arriving: detail.view, zoom: nil)
        }
        nav.delegate = delegate
        window.rootViewController = nav
        window.makeKeyAndVisible()
        // On screen first: UIKit animates no push before the stack appears.
        RunLoop.main.run(until: Date().addingTimeInterval(0.1))
        nav.pushViewController(detail, animated: true)
        // UIKit asks the delegate as the transition starts, after the call returns.
        let asked = Date().addingTimeInterval(1)
        while !crossed, Date() < asked { RunLoop.main.run(until: Date().addingTimeInterval(0.005)) }
        XCTAssertTrue(crossed, "the photo pairs; the name the list holds twice does not")
        XCTAssertEqual(p.flights.keys.sorted(), [6])
        XCTAssertEqual(leaver.alpha, 0, "the leaver is hidden before UIKit draws the transition")
        let lifted = Date().addingTimeInterval(1)
        while !(arriver.superview is FlightClip), nav.transitionCoordinator != nil, Date() < lifted { RunLoop.main.run(until: Date().addingTimeInterval(0.005)) }
        XCTAssertTrue(arriver.superview is FlightClip, "the arriver flies above both routes")
        XCTAssertTrue(p.flights[6]?.fade?.view.superview === arriver.superview, "the leaver's picture dissolves into it in its clip")
        XCTAssertTrue(arriver.window != nil && !arriver.isDescendant(of: detail.view))
        let deadline = Date().addingTimeInterval(3)
        while nav.transitionCoordinator != nil || !p.flights.isEmpty, Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(arriver.superview === place, "it lands in its place")
        XCTAssertEqual(arriver.convert(arriver.bounds, to: nil).minY, detail.view.convert(CGRect(x: 0, y: 0, width: 1, height: 1), to: nil).minY, accuracy: 0.5)
        XCTAssertEqual(leaver.alpha, 1, "and the leaver shows again")
        XCTAssertEqual(arriver.alpha, 1, "the arriver, faded in, at its own opacity")
        // A zoom holds nothing: its names ride its card.
        p.holdFlights(leaving: detail.view, arriving: list.view, zoom: "photo")
        XCTAssertEqual((p.flightHost as? BuiltInFlights)?.holds.count, 0)
        // An app's Back: its batch destroys the detail before UIKit's pop starts, and the hold keeps its name.
        p.holdFlights(leaving: detail.view, arriving: list.view, zoom: nil)
        p.views.removeValue(forKey: 6)
        crossed = false
        delegate.willShow = { nav in
            guard let coordinator = nav.transitionCoordinator else { return }
            crossed = p.crossFlights(coordinator, leaving: detail.view, arriving: list.view, zoom: nil)
        }
        nav.popViewController(animated: true)
        let popped = Date().addingTimeInterval(1)
        while !crossed, Date() < popped { RunLoop.main.run(until: Date().addingTimeInterval(0.005)) }
        XCTAssertTrue(crossed, "the destroyed route's held name flies home")
        XCTAssertEqual(p.flights.keys.sorted(), [2])
    }

    func testOnlyANameEachRouteHoldsOncePairs() {
        let p = Presenter()
        func node(_ id: UInt32, _ name: String) -> NodeView {
            let n = NodeView(id: id, kind: "view", presenter: p)
            n.props["sharedElement"] = name
            return n
        }
        let leaving = [node(1, "a"), node(2, "b"), node(3, "b"), node(4, ""), node(5, "c")]
        let arriving = [node(11, "a"), node(12, "b"), node(14, ""), node(15, "c"), node(16, "c")]
        XCTAssertEqual(Presenter.routePairs(leaving: leaving, arriving: arriving).map { [$0.from.id, $0.to.id] }, [[1, 11]])
    }
}
#endif
