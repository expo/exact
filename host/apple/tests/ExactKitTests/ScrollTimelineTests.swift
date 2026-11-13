import XCTest
@testable import ExactKit
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

/// LLP 1057.003 D5 on UIKit and AppKit (tests/fixtures/scroll-timelines.contract): a scroller a scroll or view
/// timeline reads presents the timeline's consumers from its own scroll callback, so a consumer has moved by the
/// time setting the offset returns, before any later turn of the run loop: in the frame the scroller moves (#114,
/// #225).
final class ScrollTimelineTests: XCTestCase {
    #if canImport(UIKit)
    private var window: UIWindow?
    #endif
    private var session: ExactSession?
    override func tearDown() {
        session?.destroy(); session = nil
        #if canImport(UIKit)
        window?.isHidden = true; window = nil
        #endif
        super.tearDown()
    }

    func testAConsumerMovesInTheScrollersOwnScroll() throws {
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_SCROLL_TIMELINES_PLAN"], "build.mjs --test compiles the scroll-timelines fixture")))
        let session = ExactApp.shared.makeSession(label: "scroll-timelines")
        self.session = session
        #if canImport(UIKit)
        let view = ExactView(session: session), host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        #else
        XCTAssertNil(session.boot(plan: plan, size: NSSize(width: 400, height: 600)).error)
        #endif
        let node = { (id: String) in session.presenter.views.values.first { $0.props["testId"] == id } }
        waitFor("the fixture is laid out and its scrollers marked") {
            node("cols")?.props["scrollTimeline"] != nil && node("list")?.props["scrollTimeline"] != nil
        }
        let track = try XCTUnwrap(node("track")), card = try XCTUnwrap(node("card"))
        XCTAssertEqual(track.translatePx.x, 0, accuracy: 1e-6)
        // The card's `cover` runs from 50 (its top at the list's bottom) to 200.
        XCTAssertEqual(card.translatePx.x, 0, accuracy: 1e-6)
        try scroll(XCTUnwrap(node("cols")), to: CGPoint(x: 120, y: 0))
        XCTAssertEqual(track.translatePx.x, -120, accuracy: 1e-3, "the track follows the columns in their own scroll")
        try scroll(XCTUnwrap(node("list")), to: CGPoint(x: 0, y: 125))
        XCTAssertEqual(card.translatePx.x, 50, accuracy: 1e-3, "the card is halfway through `cover` in the list's own scroll")
        try scroll(XCTUnwrap(node("cols")), to: CGPoint(x: 300, y: 0))
        XCTAssertEqual(track.translatePx.x, -300, accuracy: 1e-3)
        XCTAssertEqual(card.translatePx.x, 50, accuracy: 1e-3, "another scroller moves nothing it does not drive")
    }

    /// Sets `scroller`'s offset as a person's scroll does, in CSS px from its content's start; the platform calls
    /// its scroll callback before this returns.
    private func scroll(_ scroller: NodeView, to offset: CGPoint) throws {
        #if canImport(UIKit)
        let sv = try XCTUnwrap(scroller.scrollView)
        sv.setContentOffset(CGPoint(x: offset.x - sv.adjustedContentInset.left, y: offset.y - sv.adjustedContentInset.top), animated: false)
        #else
        let sv = try XCTUnwrap(scroller.scroll)
        sv.contentView.scroll(to: NSPoint(x: offset.x, y: offset.y))
        sv.reflectScrolledClipView(sv.contentView)
        #endif
    }
}
