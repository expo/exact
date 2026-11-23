#if os(iOS)
import XCTest
import QuartzCore
@testable import ExactKit
@testable import ExactEffects
@testable import ExactSvg
@testable import ExactNavigation

final class NavigationMarksTests: XCTestCase {
    func testTheRenderLinkStopsWhileTTIWaitsForWork() throws {
        let session = ExactApp.shared.makeSession(label: "navigation-marks")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("STILL_PAGE"), size: CGSize(width: 400, height: 800)).error)
        settleViewlessSession(session)
        let marks = NavigationMarks()
        marks.routerChanged(session, payload: ["top": 1, "pattern": "/"])
        session.holdFillForTests()
        marks.routerChanged(session, payload: ["top": 2, "pattern": "/details"])
        waitFor("the navigation's first frame") { marks.recent.contains { $0.hasPrefix("cold_ttr ") } }
        XCTAssertNil(marks.link, "only the first frame needs a display link")
        XCTAssertFalse(marks.recent.contains { $0.hasPrefix("tti ") }, "native work still holds TTI")
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
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
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
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
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
        waitFor("the slice's landing") { !session.fillInFlight }
        waitForMainQueue("the evaluation it asked for")
        XCTAssertFalse(NavigationMarks.shared.recent.contains { $0.hasPrefix("tti /destroyed ") }, "a destroyed session's runtime has no outstanding work to read")
    }

    /// A booted session's cold navigation to `route`, shown, with TTI held by
    /// what `hold` left outstanding.
    private func heldNavigation(_ route: String, _ hold: (ExactSession) -> Void) throws -> ExactSession {
        let session = ExactApp.shared.makeSession(label: "navigation-marks\(route)")
        XCTAssertNil(session.boot(plan: try fixturePlan("STILL_PAGE"), size: CGSize(width: 400, height: 800)).error)
        settleViewlessSession(session)
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

/// What caused a navigation is what made its commit (#1385): a person's input
/// whatever control took it, the program's commit even just after a tap.
extension NavigationTabsIOSTests {
    func testANavigationIsThePersonsWhicheverControlTookItAndTheProgramsOtherwise() throws {
        let session = try fixture("navigation-cause")
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let home = try XCTUnwrap(tabs.tabControllers?[0] as? UINavigationController)
        var since = CACurrentMediaTime()
        /// The next navigation's cause and its time to render, in ms, from the journal.
        func next(_ what: String) -> (cause: String, ms: Double)? {
            let ttr = { ExactJournal.shared.events.first { $0.at >= since && $0.kind == "navigation" && ($0.fields["name"] as? String)?.hasSuffix("_ttr") == true } }
            waitFor(what) { ttr() != nil }
            defer { since = CACurrentMediaTime() }
            guard let event = ttr(), let cause = event.fields["exact.nav.cause"] as? String, let value = event.fields["value"] as? Double else { return nil }
            return (cause, value * 1000)
        }
        // The tab bar's item presses its tab inside UIKit's dispatch of the
        // lift, as the window dispatches it (`ExactWindow`)
        let marks = NavigationMarks.shared
        marks.dispatching = CACurrentMediaTime() - 0.25
        tapTab(tabs, 1)
        marks.dispatching = nil
        let tab = try XCTUnwrap(next("a tab bar tap's navigation"))
        XCTAssertEqual(tab.cause, "input")
        XCTAssertGreaterThanOrEqual(tab.ms, 250, "timed from the lift")
        // With no event in dispatch it is the person's all the same, timed from when the host took it
        tapTab(tabs, 0)
        let away = try XCTUnwrap(next("a tab bar tap with no recent event"))
        XCTAssertEqual(away.cause, "input")
        XCTAssertLessThan(away.ms, 250)
        try tapNode(session, "detail")
        XCTAssertEqual(next("a press's push")?.cause, "input")
        until("detail pushed in Home") { home.viewControllers.count == 2 && home.transitionCoordinator == nil }
        // Reselecting the tab is UIKit's pop, which the router follows as the platform's Back
        tapTab(tabs, 0)
        XCTAssertEqual(next("UIKit's pop to the root")?.cause, "input")
        until("Home popped to its root") { home.viewControllers.count == 1 && home.transitionCoordinator == nil && !session.presenter.navigationHost.changing }
        // A commit the program makes, as a timer's or a reply's is
        session.apply(session.runtime.press(try node(session, "detail").id, now: session.now()))
        XCTAssertEqual(next("the program's push")?.cause, "program")
        // A queued act (`hatchAct`): a script's `click()` is the program's, a native control's carries the person's input
        let detail = try node(session, "detail").id
        func back() {
            session.presenter.navigationHost.goBack()
            XCTAssertEqual(next("the platform's Back")?.cause, "input")
            until("Home back at its root") { home.viewControllers.count == 1 && home.transitionCoordinator == nil && !session.presenter.navigationHost.changing }
        }
        back()
        marks.acting(.program) { session.applyInput(session.runtime.press(detail, now: session.now())) }
        XCTAssertEqual(next("a script's click")?.cause, "program")
        back()
        marks.acting(.person(at: CACurrentMediaTime() - 0.25)) { session.applyInput(session.runtime.press(detail, now: session.now())) }
        let bar = try XCTUnwrap(next("a bar button's press"))
        XCTAssertEqual(bar.cause, "input")
        XCTAssertGreaterThanOrEqual(bar.ms, 250, "timed from the lift")
        // A change that work fell due made, in an input's batch (the runner's `RouterChange::due`)
        marks.routerChanged(session, payload: ["top": 999, "pattern": "/due", "due": true], input: CACurrentMediaTime())
        XCTAssertEqual(next("the due work's push")?.cause, "program")
    }
}
#endif
