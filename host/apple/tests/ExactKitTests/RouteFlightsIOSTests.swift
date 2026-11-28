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
            node(1, nil), node(2, "photo"), node(3, "dup"), node(4, "dup"), node(8, "badge"),
            node(5, nil), node(6, "photo"), node(7, "dup"), node(9, "badge"),
            ["op": "children", "id": 1, "ids": [2, 3, 4]], ["op": "children", "id": 2, "ids": [8]],
            ["op": "children", "id": 5, "ids": [6, 7]], ["op": "children", "id": 6, "ids": [9]], ["op": "roots", "ids": [1, 5]],
            frame(1, 0, 800), frame(2, 100, 64), frame(3, 200, 20), frame(4, 240, 20), frame(8, 10, 20),
            frame(5, 0, 800), frame(6, 0, 300), frame(7, 320, 20), frame(9, 10, 20)]))
        let list = UIViewController(), detail = UIViewController()
        list.view.addSubview(try XCTUnwrap(p.views[1]))
        detail.view.addSubview(try XCTUnwrap(p.views[5]))
        let leaver = try XCTUnwrap(p.views[2]), arriver = try XCTUnwrap(p.views[6]), place = try XCTUnwrap(arriver.superview)
        let nav = UINavigationController(rootViewController: list)
        let delegate = Delegate()
        var crossed = false
        // A picture of the route taken before, as a frozen one is.
        let picture = UIView()
        delegate.willShow = { nav in
            guard let coordinator = nav.transitionCoordinator else { return }
            picture.frame = list.view.bounds
            list.view.addSubview(picture)
            crossed = p.crossFlights(coordinator, leaving: list.view, arriving: detail.view, zoom: nil, picture: picture)
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
        XCTAssertEqual(p.flights.keys.sorted(), [6, 9])
        XCTAssertLessThan(leaver.alpha, 0.01, "the leaver is hidden before UIKit draws the transition")
        let cut = try XCTUnwrap(picture.layer.mask as? CAShapeLayer)
        XCTAssertFalse(cut.path?.contains(CGPoint(x: 200, y: 132)) ?? true, "and cut out of the route's picture")
        XCTAssertFalse(cut.path?.contains(CGPoint(x: 200, y: 120)) ?? true, "a name inside it too")
        XCTAssertTrue(cut.path?.contains(CGPoint(x: 200, y: 400)) ?? false)
        let lifted = Date().addingTimeInterval(1)
        while !(arriver.superview is FlightClip), nav.transitionCoordinator != nil, Date() < lifted { RunLoop.main.run(until: Date().addingTimeInterval(0.005)) }
        XCTAssertTrue(arriver.superview is FlightClip, "the arriver flies above both routes")
        XCTAssertTrue(p.flights[6]?.fade?.view.superview === arriver.superview, "the leaver's picture dissolves into it in its clip")
        XCTAssertTrue(arriver.window != nil && !arriver.isDescendant(of: detail.view))
        // An in-page animation's landing, which the route flight took the id from, does not end it.
        p.flightHost.land(6)
        XCTAssertTrue(arriver.superview is FlightClip)
        // A batch's opacity while it flies, 0 included, is the one it lands with.
        leaver.alpha = 0
        let deadline = Date().addingTimeInterval(3)
        while nav.transitionCoordinator != nil || !p.flights.isEmpty, Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(arriver.superview === place, "it lands in its place")
        XCTAssertEqual(arriver.convert(arriver.bounds, to: nil).minY, detail.view.convert(CGRect(x: 0, y: 0, width: 1, height: 1), to: nil).minY, accuracy: 0.5)
        XCTAssertEqual(leaver.alpha, 0, "and the leaver lands at the opacity set in flight")
        leaver.alpha = 1
        XCTAssertEqual(arriver.alpha, 1, "the arriver, faded in, at its own opacity")
        // A zoom holds nothing: its names ride its card.
        p.holdFlights(leaving: detail.view, arriving: list.view, zoom: "photo")
        XCTAssertEqual((p.flightHost as? BuiltInFlights)?.holds.count, 0)
        // An app's Back: its batch destroys the detail before UIKit's pop starts, and the hold keeps its name.
        p.holdFlights(leaving: detail.view, arriving: list.view, zoom: nil)
        let held = (p.flightHost as? BuiltInFlights)?.holds[ObjectIdentifier(arriver)]?.picture
        XCTAssertNotNil(held, "taken before the destroy")
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
        XCTAssertEqual(p.flights.keys.sorted(), [2, 8])
        XCTAssertTrue(p.flights[2]?.fade?.view === held)
        // A held destination the list pool rebinds to another row before the pop starts is no longer the pair's.
        let landed = Date().addingTimeInterval(3)
        while nav.transitionCoordinator != nil || !p.flights.isEmpty, Date() < landed { RunLoop.main.run(until: Date().addingTimeInterval(0.02)) }
        p.views[6] = arriver
        delegate.willShow = nil
        nav.pushViewController(detail, animated: false)
        RunLoop.main.run(until: Date().addingTimeInterval(0.1))
        p.holdFlights(leaving: detail.view, arriving: list.view, zoom: nil)
        XCTAssertTrue((p.flightHost as? BuiltInFlights)?.holds[ObjectIdentifier(arriver)]?.to === leaver)
        p.views.removeValue(forKey: 2)
        leaver.id = 20
        leaver.props["sharedElement"] = "row 20"
        p.views[20] = leaver
        crossed = false
        delegate.willShow = { nav in
            guard let coordinator = nav.transitionCoordinator else { return }
            crossed = p.crossFlights(coordinator, leaving: detail.view, arriving: list.view, zoom: nil)
        }
        nav.popViewController(animated: true)
        let rebound = Date().addingTimeInterval(1)
        while !crossed, Date() < rebound { RunLoop.main.run(until: Date().addingTimeInterval(0.005)) }
        XCTAssertEqual(p.flights.keys.sorted(), [8], "the badge still flies home, and nothing flies to the rebound row")
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
