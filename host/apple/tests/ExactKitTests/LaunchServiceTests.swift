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
        var said: [String] = []
        launch.observeLog = { said.append($0) }
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
        let process = try XCTUnwrap(launch.marks[.process])
        XCTAssertEqual(said.filter { $0.hasPrefix("world ") }, ["world \(String(format: "%.1f", (present - process) * 1000))"],
                       "said once, when the tree's first pixel lets the mark land")
    }

    func testAWorldPresentationAfterFirstPixelKeepsItsOwnTime() throws {
        let launch = ExactLaunch()
        var said: [String] = []
        launch.observeLog = { said.append($0) }
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
        // The observer says the canvas's first frame once, on the startup line's clock (ms since `process`).
        let process = try XCTUnwrap(launch.marks[.process])
        XCTAssertEqual(said.filter { $0.hasPrefix("world ") }, ["world \(String(format: "%.1f", (world - process) * 1000))"])
    }

    func testAnInterruptedCommitKeepsWatchingUntilItsPresentation() {
        let launch = ExactLaunch()
        let session = ExactApp.shared.makeSession(label: "interrupted-launch")
        // Bring the app back to the foreground as the test ends, and let `AppBackground` read UIKit again: it keeps the
        // edge it last heard, and later tests in the process would read the page hidden
        defer {
            launch.link?.invalidate()
            session.destroy()
            NotificationCenter.default.post(name: UIApplication.willEnterForegroundNotification, object: nil)
            AppBackground.invalidate()
        }
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
        waitForEvaluation("the evaluation after the work ended", launch)
        XCTAssertEqual(launch.candidate, candidate, "work that changed nothing does not move the candidate")
        launch.frame(Vsync(at: candidate, interval: 1.0 / 60))
        waitFor("the owner's answer that confirms it") { launch.ttiOutcome != nil }
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
        waitForEvaluation("the evaluation after the orientation request", launch)
        XCTAssertEqual(session.presenter.orientationLock.mask, 0)
        XCTAssertFalse(session.presenter.orientationLock.locked)
        XCTAssertEqual(launch.candidate, candidate)
        launch.frame(Vsync(at: candidate, interval: 1.0 / 60))
        waitFor("the owner's answer that confirms it") { launch.ttiOutcome != nil }
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
        waitFor("the evaluation after the change") { launch.candidate != nil }
        let moved = try XCTUnwrap(launch.candidate, "the change sets a new candidate")
        XCTAssertGreaterThan(moved, candidate, "the change shows at a later vsync")
        launch.frame(Vsync(at: candidate, interval: 1.0 / 60))
        XCTAssertNil(launch.ttiOutcome, "the old candidate's vsync is before the change shows")
        launch.frame(Vsync(at: moved, interval: 1.0 / 60))
        waitFor("the owner's answer that confirms it") { launch.ttiOutcome != nil }
        XCTAssertEqual(launch.marks[.interactive], moved)
    }

    /// LLP 1072 T3: two metric reads on the way at once each answer, with
    /// nothing outstanding: neither is work that follows the other.
    func testTwoLedgerReadsAtOnceBothAnswer() throws {
        let session = ExactApp.shared.makeSession(label: "two-ledger-reads")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("STILL_PAGE"), size: CGSize(width: 400, height: 800)).error)
        settleViewlessSession(session)
        var answers: [ExactLaunch.Ledger?] = []
        for _ in 0..<2 { ExactLaunch.readLedger(session) { answers.append($0) } }
        waitFor("both reads answer", cap: 10) { answers.count == 2 }
        XCTAssertEqual(answers.compactMap { $0?.items }, [[], []])
    }

    /// The evaluation that ended work asks for: its turn, then the owner's
    /// answers to its reads.
    private func waitForEvaluation(_ what: String, _ launch: ExactLaunch) {
        waitForMainQueue(what)
        waitFor(what) { launch.reads == 0 }
    }

    /// A launch presented and activated with a change on screen and nothing
    /// outstanding, its candidate's vsync passed and not yet confirmed.
    private func settlingLaunch(_ label: String) throws -> (ExactLaunch, ExactSession, Double) {
        let launch = ExactLaunch()
        let session = ExactApp.shared.makeSession(label: label)
        addTeardownBlock { launch.link?.invalidate(); session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("STILL_PAGE"), size: CGSize(width: 400, height: 800)).error)
        // Settle the session first: this launch evaluates only when the test says (`ExactLaunch.shared` hears the
        // session's work end), so work in flight when it evaluates, such as the tick that hears the page's first present,
        // would leave the candidate unset with nothing to evaluate again when it lands
        settleViewlessSession(session)
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
        waitFor("the evaluation that finds nothing outstanding") { launch.candidate != nil }
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
