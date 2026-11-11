#if os(iOS)
import XCTest
import QuartzCore
@testable import ExactKit

final class NavigationMarksTests: XCTestCase {
    func testTheRenderLinkStopsWhileTTIWaitsForWork() throws {
        let session = ExactApp.shared.makeSession(label: "navigation-marks")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("HEADLESS"), size: CGSize(width: 400, height: 800)).error)
        let marks = NavigationMarks()
        marks.routerChanged(session, payload: ["top": 1, "pattern": "/"])
        session.holdFillForTests()
        marks.routerChanged(session, payload: ["top": 2, "pattern": "/details"])
        waitFor("the navigation's first frame") { marks.recent.contains { $0.hasPrefix("cold_ttr ") } }
        XCTAssertNil(marks.link, "only the first frame needs a display link")
        XCTAssertFalse(marks.recent.contains { $0.hasPrefix("tti ") }, "native work still holds TTI")
        session.publishFillForTests(1, wireBatch([]), generation: session.generation)
        waitFor("the slice's landing") { !session.fillInFlight }
        marks.workEnded(session)
        waitFor("TTI after the work clears") { marks.recent.contains { $0.hasPrefix("tti ") } }
        XCTAssertNil(marks.link)
        marks.routerChanged(session, payload: ["top": 1, "pattern": "/"])
        waitFor("a warm navigation's first frame") { marks.recent.contains { $0.hasPrefix("warm_ttr ") } }
        XCTAssertNil(marks.link)
        XCTAssertEqual(marks.recent.filter { $0.hasPrefix("tti ") }.count, 1, "a warm navigation has no new TTI")
    }

    // Work that shows nothing new can still end what held TTI, so the
    // session tells the marks at every end of work, not only an apply.

    func testTTIFollowsASliceThatLandsWithNothingToShow() throws {
        let session = try heldNavigation("/landed") { $0.holdFillForTests() }
        defer { session.destroy() }
        let applied = session.appliedBatches
        session.publishFillForTests(1, wireBatch([]), generation: session.generation)
        waitForTTI("/landed")
        XCTAssertEqual(session.appliedBatches, applied, "the slice shows nothing and skips the pass")
    }

    func testTTIFollowsABatchThatSkipsThePass() throws {
        let session = try heldNavigation("/skipped") { _ in SvgFilterLive.inFlight += 1 }
        defer { session.destroy() }
        // The hold clears without notice; the empty batch that follows must re-check TTI
        SvgFilterLive.inFlight -= 1
        let applied = session.appliedBatches
        session.applyUnlessEmpty(wireBatch([]))
        XCTAssertEqual(session.appliedBatches, applied, "the batch shows nothing and skips the pass")
        waitForTTI("/skipped")
    }

    func testTTIFollowsAnSVGPictureDone() throws {
        guard SvgFilterGPU.metal != nil, SvgFilterMetal.shared != nil else { throw missing("metal", "no Metal device or SVG filter library") }
        let session = try heldNavigation("/picture") { _ in SvgFilterLive.inFlight += 1 }
        defer { session.destroy() }
        let picture = SvgFilterLive(layer: CALayer())
        let shape: [String: Any] = ["id": 1, "o": 1, "n": 1, "p": [0, 0, 0, 1, 40, 0, 1, 40, 30, 1, 0, 30, 3], "f": [220, 40, 40, 255],
                                    "w": 0, "cap": 0, "join": 0, "ml": 4, "rule": 0, "dash": [Any](), "ph": 0]
        // The chain `SvgFilterLiveTests` draws
        let sub: [Float] = [-10, -10, 80, 70]
        let saturate: [Float] = [1.6, -0.6, -0.1, 0, 0, -0.2, 1.4, -0.1, 0, 0, -0.2, -0.6, 1.9, 0, 0, 0, 0, 0, 1, 0]
        let program: [Float] = sub + [2] + [0, -1, -3] + sub + [0, 2, 2] + [5, 0, -3] + sub + [0] + saturate
        picture.update(els: [shape], rect: CGRect(x: -10, y: -10, width: 80, height: 70), w: 160, h: 140, k: 2,
                       program: program, dark: false, fonts: nil, clock: nil)
        // The picture now holds TTI alone
        SvgFilterLive.inFlight -= 1
        waitForTTI("/picture")
        XCTAssertNotNil(picture.layer.contents, "the picture landed")
    }

    func testADestroyedSessionHasNoTTI() throws {
        let session = try heldNavigation("/destroyed") { $0.holdFillForTests() }
        session.destroy()
        // A slice that lands after the session is gone, and the evaluation it asks for
        session.publishFillForTests(1, wireBatch([]), generation: session.generation)
        waitFor("the slice's landing") { !session.fillInFlight }
        waitForMainQueue("the evaluation it asked for")
        XCTAssertFalse(NavigationMarks.shared.recent.contains { $0.hasPrefix("tti /destroyed ") }, "a destroyed session's runtime has no outstanding work to read")
    }

    /// A booted session's cold navigation to `route`, shown, with TTI held by
    /// what `hold` left outstanding.
    private func heldNavigation(_ route: String, _ hold: (ExactSession) -> Void) throws -> ExactSession {
        let session = ExactApp.shared.makeSession(label: "navigation-marks\(route)")
        XCTAssertNil(session.boot(plan: try fixturePlan("HEADLESS"), size: CGSize(width: 400, height: 800)).error)
        let marks = NavigationMarks.shared
        marks.routerChanged(session, payload: ["top": 1, "pattern": "/"])
        hold(session)
        marks.routerChanged(session, payload: ["top": 2, "pattern": route])
        waitFor("the navigation's first frame") { marks.recent.contains { $0.hasPrefix("cold_ttr \(route) ") } }
        XCTAssertFalse(marks.recent.contains { $0.hasPrefix("tti \(route) ") }, "the work holds TTI")
        return session
    }

    private func waitForTTI(_ route: String) {
        waitFor("TTI once the work ends") { NavigationMarks.shared.recent.contains { $0.hasPrefix("tti \(route) ") } }
    }
}
#endif
