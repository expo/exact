// @ref LLP 1035.006.000 D3 — where the iOS core says a tip stands: in
// document order, nowhere while a clipping box or a scroller hides its
// element or a sheet covers its screen, again as a scroller brings the
// element into view, and withdrawn when the presenter resets.
#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class TipAnchorsIOSTests: XCTestCase {
    private var window: UIWindow?
    override func tearDown() { window?.isHidden = true; window = nil; super.tearDown() }

    private func presenter(in scene: UIWindowScene) -> (Presenter, UIViewController) {
        let p = Presenter(), host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 600)
        window.rootViewController = host
        window.makeKeyAndVisible()
        p.viewport.frame = host.view.bounds
        host.view.addSubview(p.viewport)
        self.window = window
        return (p, host)
    }
    private func tipped(_ id: Int, _ tip: String) -> [String: Any] {
        ["op": "create", "id": id, "kind": "button", "handlers": ["press"], "props": ["id": tip, "tip": "Tip \(tip)"]]
    }
    private func frame(_ id: Int, _ y: Double, h: Double = 40) -> [String: Any] {
        ["op": "frame", "id": id, "x": 10, "y": y, "w": 100, "h": h]
    }

    func testTipsComeInDocumentOrderAndAClippedElementHasNowhereToShow() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a tip's anchor is a view in a window scene") }
        let recorded = TipsRecorded.install(in: self), (p, _) = presenter(in: scene)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "style": ["overflow_x": "hidden", "overflow_y": "hidden"]],
            tipped(3, "cut"), tipped(4, "inside"),
            // Made 3 then 4, written 4 then 3.
            ["op": "children", "id": 2, "ids": [4, 3]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 600), frame(2, 0, h: 100), frame(4, 10), frame(3, 200),
        ]))
        XCTAssertEqual(recorded.ids, ["inside", "cut"], "the document's order, not the order the nodes were made")
        XCTAssertTrue(recorded.anchored("inside"))
        XCTAssertFalse(recorded.anchored("cut"), "outside its clipping box, though inside the window")
    }

    func testScrollingAnElementIntoViewGivesItsTipAnAnchor() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a tip's anchor is a view in a window scene") }
        let recorded = TipsRecorded.install(in: self), (p, _) = presenter(in: scene)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "style": ["overflow_y": "scroll"]],
            tipped(3, "below"),
            ["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 600), frame(2, 0, h: 150), frame(3, 600),
            ["op": "content", "id": 2, "w": 400.0, "h": 1000.0],
        ]))
        XCTAssertFalse(recorded.anchored("below"), "below the scroller's port")
        let scroll = try XCTUnwrap(p.views[2]?.scrollView)
        scroll.setContentOffset(CGPoint(x: 0, y: 560), animated: false)
        XCTAssertTrue(recorded.anchored("below"), "scrolled into the port")
    }

    func testASheetCoversItsScreensTipsAndAResetWithdrawsThem() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a sheet is presented in a window scene") }
        let recorded = TipsRecorded.install(in: self), (p, host) = presenter(in: scene)
        p.apply(wireBatch([tipped(1, "page"), ["op": "roots", "ids": [1]], frame(1, 10)]))
        XCTAssertTrue(recorded.anchored("page"))
        let sheet = UIViewController()
        host.present(sheet, animated: false)
        waitFor("the sheet presented") { host.presentedViewController === sheet && !sheet.isBeingPresented }
        p.presentTips()
        XCTAssertFalse(recorded.anchored("page"), "under the sheet")
        sheet.dismiss(animated: false)
        waitFor("the sheet gone") { host.presentedViewController == nil }
        p.presentTips()
        XCTAssertTrue(recorded.anchored("page"))
        p.reset()
        XCTAssertEqual(recorded.withdrawn, [ObjectIdentifier(p)])
    }
}
#endif
