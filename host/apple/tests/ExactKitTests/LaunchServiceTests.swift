import QuartzCore
import XCTest
#if os(iOS)
import UIKit
#endif
@testable import ExactKit

final class LaunchServiceTests: XCTestCase {
    #if os(iOS)
    func testAnEarlyWorldPresentationIsKeptUntilTheTreePresents() throws {
        let launch = ExactLaunch()
        let session = ExactApp.shared.makeSession(label: "early-world-presentation")
        defer { launch.link?.invalidate(); session.destroy() }
        launch.delegateInit()
        launch.didFinishLaunching()
        launch.bootEntered(session)
        launch.treeApplied(session, path: "/")
        // The GPU's completed drawable can be reported before UIKit draws the root.
        launch.worldPresented()
        XCTAssertNil(launch.marks[.world])
        launch.drawn(session)
        launch.link?.isPaused = true
        let commit = try XCTUnwrap(launch.marks[.commit])
        launch.frame(Vsync(at: commit, interval: 1.0 / 60))
        let present = try XCTUnwrap(launch.marks[.present])
        XCTAssertEqual(launch.marks[.world], present)
        launch.worldPresented()
        XCTAssertEqual(launch.marks[.world], present, "later frames keep the first mark")
    }

    func testAWorldPresentationAfterFirstPixelKeepsItsOwnTime() throws {
        let launch = ExactLaunch()
        let session = ExactApp.shared.makeSession(label: "late-world-presentation")
        defer { launch.link?.invalidate(); session.destroy() }
        launch.delegateInit()
        launch.didFinishLaunching()
        launch.bootEntered(session)
        launch.treeApplied(session, path: "/")
        launch.drawn(session)
        launch.link?.isPaused = true
        let commit = try XCTUnwrap(launch.marks[.commit])
        launch.frame(Vsync(at: commit, interval: 1.0 / 60))
        let present = try XCTUnwrap(launch.marks[.present])
        waitFor("the first-pixel timestamp to pass") { CACurrentMediaTime() > present }
        let before = CACurrentMediaTime()
        launch.worldPresented()
        let world = try XCTUnwrap(launch.marks[.world])
        XCTAssertGreaterThan(world, present)
        XCTAssertGreaterThanOrEqual(world, before)
        XCTAssertLessThanOrEqual(world, CACurrentMediaTime())
        launch.worldPresented()
        XCTAssertEqual(launch.marks[.world], world, "later frames keep the first mark")
    }

    func testAnInterruptedCommitKeepsWatchingUntilItsPresentation() {
        let launch = ExactLaunch()
        let session = ExactApp.shared.makeSession(label: "interrupted-launch")
        defer { session.destroy() }
        launch.delegateInit()
        launch.didFinishLaunching()
        launch.bootEntered(session)
        launch.treeApplied(session, path: "/")
        launch.drawn(session)
        NotificationCenter.default.post(name: UIApplication.didEnterBackgroundNotification, object: nil)
        XCTAssertNotNil(launch.link, "the committed frame still needs its first vsync")
        waitFor("the interrupted launch's first presentation") { launch.marks[.present] != nil }
        XCTAssertNil(launch.link, "the interrupted launch stops watching after first pixel")
    }
    #endif

    /// A settled screen becomes interactive at the next vsync, confirmed by
    /// the display link's callback for it. Work that ends without changing
    /// the screen after that vsync, before a late callback, leaves the mark
    /// there; a repeating timer whose action writes nothing visible could
    /// otherwise move it a frame later every period. The test supplies the
    /// frames, so the link's own callbacks are paused.
    func testWorkEndingWithoutAChangeKeepsTheSettledVsync() throws {
        let (launch, session, candidate) = try settlingLaunch("settled-launch")
        launch.workEnded(session)
        waitForMainQueue("the evaluation after the work ended")
        XCTAssertEqual(launch.candidate, candidate, "work that changed nothing does not move the candidate")
        launch.frame(Vsync(at: candidate, interval: 1.0 / 60))
        XCTAssertEqual(launch.ttiOutcome, "settled")
        XCTAssertEqual(launch.marks[.interactive], candidate)
    }

    func testAnInertOrientationRequestKeepsTheSettledVsync() throws {
        let (launch, session, candidate) = try settlingLaunch("settled-orientation")
        var batch = wireBatch([])
        batch.orientation = .init()
        batch.orientation?.types = 1
        session.applyUnlessEmpty(batch)
        launch.applied(session, batch)
        launch.workEnded(session)
        waitForMainQueue("the evaluation after the orientation request")
        XCTAssertEqual(session.presenter.orientationLock.mask, 0)
        XCTAssertFalse(session.presenter.orientationLock.locked)
        XCTAssertEqual(launch.candidate, candidate)
        launch.frame(Vsync(at: candidate, interval: 1.0 / 60))
        XCTAssertEqual(launch.marks[.interactive], candidate)
    }

    /// A batch that changes the screen with no ops, such as a control's
    /// viewless contents, moves the mark to the vsync that shows it, as a
    /// batch with ops does.
    func testAChangeWithNoOpsMovesTheSettledVsync() throws {
        let (launch, session, candidate) = try settlingLaunch("settled-controls")
        var controls = wireBatch([])
        controls.controls = true
        launch.applied(session, controls)
        launch.workEnded(session)
        waitForMainQueue("the evaluation after the change")
        let moved = try XCTUnwrap(launch.candidate, "the change sets a new candidate")
        XCTAssertGreaterThan(moved, candidate, "the change shows at a later vsync")
        launch.frame(Vsync(at: candidate, interval: 1.0 / 60))
        XCTAssertNil(launch.ttiOutcome, "the old candidate's vsync is before the change shows")
        launch.frame(Vsync(at: moved, interval: 1.0 / 60))
        XCTAssertEqual(launch.marks[.interactive], moved)
    }

    /// A launch presented and activated with a change on screen and nothing
    /// outstanding, its candidate's vsync passed and not yet confirmed.
    private func settlingLaunch(_ label: String) throws -> (ExactLaunch, ExactSession, Double) {
        let launch = ExactLaunch()
        let session = ExactApp.shared.makeSession(label: label)
        addTeardownBlock { launch.link?.invalidate(); session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("HEADLESS"), size: CGSize(width: 400, height: 800)).error)
        launch.delegateInit()
        launch.didFinishLaunching()
        launch.bootEntered(session)
        launch.treeApplied(session, path: "/")
        var batch = wireBatch([])
        batch.ops = [BatchOp(op: .props, nodeID: 1)]
        launch.applied(session, batch)
        launch.drawn(session)
        launch.link?.isPaused = true
        let commit = try XCTUnwrap(launch.marks[.commit])
        launch.frame(Vsync(at: commit, interval: 1.0 / 60))
        XCTAssertNotNil(launch.marks[.present])
        waitForMainQueue("the evaluation after the first frame, which activation holds")
        launch.activated(session, ok: true)
        waitForMainQueue("the evaluation that finds nothing outstanding")
        let candidate = try XCTUnwrap(launch.candidate, "the screen changed, so it settles at the next vsync")
        waitFor("the candidate's vsync to pass") { CACurrentMediaTime() > candidate }
        return (launch, session, candidate)
    }

    func testSuppressedLaunchesStillWaitForFirstPixelBeforeLoadingServices() {
        for when in [ExactServiceLoad.afterFirstPixel, .afterStartup] {
            XCTAssertFalse(ExactLaunch.serviceIsReady(when, presented: false, settled: false, suppressed: true), "prewarming and agent launches have not shown a frame")
            XCTAssertTrue(ExactLaunch.serviceIsReady(when, presented: true, settled: false, suppressed: true), "a suppressed launch can load once it has shown a frame")
        }
        XCTAssertFalse(ExactLaunch.serviceIsReady(.afterStartup, presented: false, settled: true, suppressed: false), "failed activation does not load a service before first pixel")
        XCTAssertTrue(ExactLaunch.serviceIsReady(.afterFirstPixel, presented: true, settled: false, suppressed: false))
        XCTAssertFalse(ExactLaunch.serviceIsReady(.afterStartup, presented: true, settled: false, suppressed: false), "normal startup still waits for interactivity")
        XCTAssertTrue(ExactLaunch.serviceIsReady(.afterStartup, presented: true, settled: true, suppressed: false))
    }
}
