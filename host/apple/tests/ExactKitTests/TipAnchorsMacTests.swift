// @ref LLP 1035.006.000 D3 — where the Mac's core says a tip stands: in
// document order, nowhere while a clipping box or a scroller hides its
// element or a sheet covers its window, again as a scroller brings the
// element into view, and withdrawn when the presenter resets.
#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

final class TipAnchorsMacTests: XCTestCase {
    private var window: NSWindow!
    override func tearDown() { window?.close(); window = nil; super.tearDown() }

    private func presenter() -> Presenter {
        _ = NSApplication.shared
        let p = Presenter()
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        window.orderFrontRegardless()
        return p
    }
    private func tipped(_ id: Int, _ tip: String) -> [String: Any] {
        ["op": "create", "id": id, "kind": "button", "handlers": ["press"], "props": ["id": tip, "tip": "Tip \(tip)"]]
    }
    private func frame(_ id: Int, _ y: Double, h: Double = 40) -> [String: Any] {
        ["op": "frame", "id": id, "x": 10, "y": y, "w": 100, "h": h]
    }

    /// The tips come in the document's order, which a later-made node can lead.
    func testTipsComeInDocumentOrderNotTheOrderTheirNodesWereMade() {
        let recorded = TipsRecorded.install(in: self), p = presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"], tipped(2, "second"), tipped(3, "first"),
            ["op": "children", "id": 1, "ids": [3, 2]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 400), frame(3, 10), frame(2, 60),
        ]))
        XCTAssertEqual(recorded.ids, ["first", "second"])
        // An earlier element inserted later still leads.
        p.apply(wireBatch([tipped(4, "zeroth"), frame(4, 110), ["op": "children", "id": 1, "ids": [4, 3, 2]]]))
        XCTAssertEqual(recorded.ids, ["zeroth", "first", "second"])
    }

    /// An element a clipping box cuts away, though inside the window, has no
    /// anchor; one the box shows has.
    func testAnElementItsClippingBoxHidesHasNowhereToShow() {
        let recorded = TipsRecorded.install(in: self), p = presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "style": ["overflow_x": "hidden", "overflow_y": "hidden"]],
            tipped(3, "inside"), tipped(4, "cut"),
            ["op": "children", "id": 2, "ids": [3, 4]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 400), frame(2, 0, h: 100), frame(3, 10), frame(4, 200),
        ]))
        XCTAssertTrue(recorded.anchored("inside"))
        XCTAssertFalse(recorded.anchored("cut"), "outside its clipping box, though inside the window")
    }

    /// A scroller that brings an element into view gives its tip an anchor,
    /// with no batch.
    func testScrollingAnElementIntoViewGivesItsTipAnAnchor() throws {
        let recorded = TipsRecorded.install(in: self), p = presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "style": ["overflow_y": "scroll"]],
            tipped(3, "below"),
            ["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            frame(1, 0, h: 400), frame(2, 0, h: 150), frame(3, 600),
            ["op": "content", "id": 2, "w": 400.0, "h": 1000.0],
        ]))
        XCTAssertFalse(recorded.anchored("below"), "below the scroller's port")
        let scroll = try XCTUnwrap(p.views[2]?.scroll)
        scroll.contentView.scroll(to: NSPoint(x: 0, y: 560))
        scroll.reflectScrolledClipView(scroll.contentView)
        XCTAssertTrue(recorded.anchored("below"), "scrolled into the port")
    }

    /// A sheet over the window covers its tips; they stand again once it goes.
    func testASheetOverTheWindowCoversItsTips() {
        let recorded = TipsRecorded.install(in: self), p = presenter()
        p.apply(wireBatch([tipped(1, "page"), ["op": "roots", "ids": [1]], frame(1, 10)]))
        XCTAssertTrue(recorded.anchored("page"))
        let sheet = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 200, height: 100), styleMask: [.titled], backing: .buffered, defer: false)
        sheet.isReleasedWhenClosed = false
        window.beginSheet(sheet)
        waitFor("the sheet attached") { self.window.attachedSheet === sheet }
        p.presentTips()
        XCTAssertFalse(recorded.anchored("page"), "under the sheet")
        window.endSheet(sheet)
        waitFor("the sheet gone") { self.window.attachedSheet == nil }
        p.presentTips()
        XCTAssertTrue(recorded.anchored("page"))
    }

    /// A reset presenter's tips are withdrawn, so nothing of it is kept.
    func testAResetWithdrawsThePresentersTips() {
        let recorded = TipsRecorded.install(in: self), p = presenter()
        p.apply(wireBatch([tipped(1, "page"), ["op": "roots", "ids": [1]], frame(1, 10)]))
        p.reset()
        XCTAssertEqual(recorded.withdrawn, [ObjectIdentifier(p)])
    }
}
#endif
