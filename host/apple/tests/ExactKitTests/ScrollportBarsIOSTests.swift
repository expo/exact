#if os(iOS)
@testable import ExactKit
import ExactCore
import UIKit
import XCTest

/// A scroller's own bars on its scrollport (LLP 1123 D12): the host mounts them over its scroll view,
/// where they keep their place as its content scrolls, take the touches on them and pass the rest
/// to the content, and the accessibility readout lists each of their controls once.
@MainActor
final class ScrollportBarsIOSTests: XCTestCase {
    private var window: UIWindow?

    override func tearDown() {
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    func testAScrollersBarKeepsItsPlaceTakesItsTouchesAndIsClearedOnce() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a scrollport bar needs a window") }
        let p = presenter(in: scene)
        let (scroller, bar, button, row) = (try view(p, 2), try view(p, 3), try view(p, 4), try view(p, 5))
        let sv = try XCTUnwrap(scroller.scroll)
        XCTAssertTrue(bar.superview === sv, "a bar not yet on the scrollport is in the content")
        ScrollportBars.set([3], in: p)
        let holder = try XCTUnwrap(scroller.scrollportHolder)
        XCTAssertTrue(bar.superview === holder && holder.superview === scroller)
        XCTAssertGreaterThan(try XCTUnwrap(scroller.subviews.firstIndex(of: holder)), try XCTUnwrap(scroller.subviews.firstIndex(of: sv)), "over the scroll view")
        XCTAssertEqual(holder.frame, scroller.bounds)
        XCTAssertTrue(row.superview === sv, "the content stays in the scroll view")
        // Its frames are placed on the pixel grid from its scroller's origin, 20 points down, and
        // follow the scroller as it moves
        p.apply(wireBatch([["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 300.0, "h": 50.0]]))
        XCTAssertEqual(bar.grid?.origin.layoutY, 20)
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 0.0, "y": 30.0, "w": 300.0, "h": 370.0]]))
        XCTAssertEqual(bar.grid?.origin.layoutY, 30, "its scroller moved")
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 0.0, "y": 20.0, "w": 300.0, "h": 380.0]]))
        // The holder lies at the scroller's padding box, inside a border
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["overflow_y": "auto", "border_width_top": 4.0]]]))
        XCTAssertEqual(holder.frame, CGRect(x: 0, y: 4, width: 300, height: 376), "at the padding box")
        XCTAssertEqual(bar.convert(bar.bounds, to: scroller).minY, 0, "its bar where layout put it")
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["overflow_y": "auto"]]]))
        // The content scrolls under the bar, which keeps its place
        let resting = (bar.convert(bar.bounds, to: nil), row.convert(row.bounds, to: nil))
        sv.contentOffset.y = 30
        XCTAssertEqual(bar.convert(bar.bounds, to: nil), resting.0, "the bar keeps its place")
        XCTAssertEqual(row.convert(row.bounds, to: nil).minY, resting.1.minY - 30, "the row scrolls")
        // A touch on the button is the button's, one on the bar the bar's; under a bar that takes no
        // pointer it goes to the content beneath, the button's still its own
        let window = try XCTUnwrap(self.window)
        let on = { (v: UIView, x: CGFloat, y: CGFloat) in window.hitTest(v.convert(CGPoint(x: x, y: y), to: window), with: nil) }
        XCTAssertTrue(on(button, 30, 20) === button)
        XCTAssertTrue(on(bar, 200, 25) === bar)
        p.apply(wireBatch([["op": "style", "id": 3, "style": ["pointer_events": "none"]]]))
        XCTAssertTrue(on(button, 30, 20) === button, "the button takes its own")
        let beneath = on(bar, 200, 25)
        XCTAssertTrue(beneath === row || beneath === sv, "the content takes the rest: \(String(describing: beneath))")
        // A scroller that takes no pointer still leads one to its bar's button
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["overflow_y": "auto", "pointer_events": "none"]]]))
        XCTAssertTrue(on(button, 30, 20) === button, "through a scroller that takes none")
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["overflow_y": "auto"]]]))
        // The accessibility readout lists the button once
        p.syncAccessibility()
        let named = p.axElements(roots: [p.viewport])["elements"] as? [[String: Any]] ?? []
        XCTAssertEqual(named.filter { $0["name"] as? String == "Done" }.count, 1, "\(named)")
        // The inset the kernel reserves clears the bar once: at rest the row lies at the bar's foot
        let never = { () -> ScrollInsetsIOS.Rows in var rows = ScrollInsetsIOS.Rows(); rows.adjustment = .never; return rows }()
        ScrollInsetsIOS.write(sv, in: scroller, terms: .init(reserved: [50, nil, nil, nil]), rows: never)
        sv.contentOffset.y = -sv.adjustedContentInset.top
        XCTAssertEqual(row.convert(row.bounds, to: bar).minY, bar.bounds.maxY, "the row rests at the bar's foot")
        // Its edge effect registers with its own scroller's scroll view
        if #available(iOS 26.0, *) {
            ControlBars.setBarEdgeEffects([(3, .top, 2)], in: p)
            let held = bar.interactions.compactMap { $0 as? UIScrollEdgeElementContainerInteraction }
            XCTAssertTrue(held.count == 1 && held.first?.scrollView === sv, "\(held)")
        }
        // A new scroll view leaves the bar over it
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["overflow_y": "visible"]]]))
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["overflow_y": "auto"]]]))
        let again = try XCTUnwrap(scroller.scroll)
        XCTAssertTrue(again !== sv && bar.superview === holder && holder.superview === scroller)
        XCTAssertGreaterThan(try XCTUnwrap(scroller.subviews.firstIndex(of: holder)), try XCTUnwrap(scroller.subviews.firstIndex(of: again)), "over the new scroll view")
        if #available(iOS 26.0, *) {
            let held = bar.interactions.compactMap { $0 as? UIScrollEdgeElementContainerInteraction }
            XCTAssertTrue(held.first?.scrollView === again, "its registration follows the new scroll view")
        }
        // Off the scrollport, the bar is the content's again, the holder gone
        ScrollportBars.set([], in: p)
        XCTAssertTrue(bar.superview === again && scroller.scrollportHolder == nil && holder.superview == nil)
        XCTAssertEqual(Array(again.subviews.compactMap { ($0 as? NodeView)?.id }.prefix(3)), [3, 5, 6], "in its place among them")
    }

    /// A presenter of its own in a scene window: a root (1) holding a scroller (2), 300 by 380 and 20
    /// points down, whose
    /// children are a 50-point top bar (3) holding a 60-by-40 button (4) named "Done", then a
    /// 100-point row (5) and a 2000-point box (6).
    private func presenter(in scene: UIWindowScene) -> Presenter {
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 300, height: 400)
        window.makeKeyAndVisible()
        self.window = window
        let p = Presenter()
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "style": ["overflow_y": "auto"]],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": [255, 255, 255, 255]]],
            ["op": "create", "id": 4, "kind": "button", "handlers": ["press"], "props": ["accessibilityLabel": "Done"]],
            ["op": "create", "id": 5, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "create", "id": 6, "kind": "view", "style": ["background_color": [0, 255, 0, 255]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3, 5, 6]],
            ["op": "children", "id": 3, "ids": [4]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 400.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 20.0, "w": 300.0, "h": 380.0],
            ["op": "content", "id": 2, "w": 300.0, "h": 2100.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 300.0, "h": 50.0],
            ["op": "frame", "id": 4, "x": 10.0, "y": 5.0, "w": 60.0, "h": 40.0],
            ["op": "frame", "id": 5, "x": 0.0, "y": 0.0, "w": 300.0, "h": 100.0],
            ["op": "frame", "id": 6, "x": 0.0, "y": 100.0, "w": 300.0, "h": 2000.0],
        ]))
        window.layoutIfNeeded()
        return p
    }

    private func view(_ p: Presenter, _ id: UInt32) throws -> NodeView { try XCTUnwrap(p.views[id], "node \(id)") }
}
#endif
