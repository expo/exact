#if os(iOS)
import UIKit
import XCTest
@testable import ExactCore
@testable import ExactKit

/// One display link for the app's life: made once, paused while no one
/// wants frames, at the highest rate its users ask for.
final class FrameClockIOSTests: XCTestCase {
    private final class Owner {}

    func testCanvasDemandRunsWithoutATimerAndStopsWhenCleared() {
        let session = ExactApp.shared.makeSession(label: "canvas-frame-demand")
        defer { session.destroy() }
        session.clock = nil
        session.frames.canvas2d = true
        session.scheduleClock(due: nil)
        XCTAssertFalse(session.frames.timerSoon)
        XCTAssertTrue(FrameClock.shared.wants(session.frames))
        session.frames.canvas2d = false
        session.scheduleClock(due: nil)
        XCTAssertFalse(FrameClock.shared.wants(session.frames))
    }

    func testOneLinkIsMadeAndPausedWhenNoOneWantsFrames() throws {
        let clock = FrameClock()
        let a = Owner(), b = Owner()
        XCTAssertNil(clock.link, "no link before anyone wants frames")
        clock.want(a, .scroll) { _ in }
        XCTAssertTrue(clock.running)
        let link = try XCTUnwrap(clock.link)
        clock.want(b, .heavyLeaves) { _ in }
        clock.drop(a)
        XCTAssertTrue(clock.running, "b still wants frames")
        clock.drop(b)
        XCTAssertFalse(clock.running, "paused, not torn down")
        for _ in 0..<50 { clock.want(a, .scroll) { _ in }; clock.drop(a) }
        XCTAssertTrue(clock.link === link)
        XCTAssertEqual(clock.linksMade, 1)
    }

    func testTheRateIsTheHighestAsked() {
        let clock = FrameClock()
        let a = Owner(), b = Owner()
        clock.want(a, .session) { _ in }
        XCTAssertEqual(clock.link?.preferredFrameRateRange, .default)
        clock.want(b, .heavyLeaves, rate: CAFrameRateRange(minimum: 80, maximum: 120, preferred: 120)) { _ in }
        clock.want(a, .session, rate: CAFrameRateRange(minimum: 30, maximum: 60, preferred: 60)) { _ in }
        XCTAssertEqual(clock.link?.preferredFrameRateRange, CAFrameRateRange(minimum: 80, maximum: 120, preferred: 120))
        clock.drop(b)
        XCTAssertEqual(clock.link?.preferredFrameRateRange, CAFrameRateRange(minimum: 30, maximum: 60, preferred: 60))
    }

    /// Motion asks for the display's full rate, and a paint-only cap or a
    /// timer's `.default` never lowers it (LLP 1061 D4).
    func testMotionAtTheFullRateOutranksAPaintCap() {
        let clock = FrameClock()
        let paint = Owner(), driver = Owner(), timer = Owner()
        let full = FrameClock.full(on: nil)
        XCTAssertGreaterThanOrEqual(full.maximum, 60)
        XCTAssertEqual(full.minimum, min(60, full.maximum))
        clock.want(timer, .session) { _ in }
        clock.want(paint, .svgFilter, rate: CAFrameRateRange(minimum: 30, maximum: 60, preferred: 60)) { _ in }
        clock.want(driver, .offsetDriver, rate: full) { _ in }
        XCTAssertEqual(clock.link?.preferredFrameRateRange.maximum, full.maximum)
        XCTAssertEqual(clock.link?.preferredFrameRateRange.minimum, max(30, full.minimum))
    }

    func testUsersTickInOrderAndADroppedOneDoesNot() throws {
        let clock = FrameClock()
        let session = Owner(), scroll = Owner(), leaves = Owner()
        var ticks: [String] = []
        clock.want(scroll, .scroll) { _ in ticks.append("scroll") }
        clock.want(leaves, .heavyLeaves) { _ in ticks.append("leaves") }
        clock.want(session, .session) { _ in ticks.append("session"); clock.drop(session); clock.drop(scroll) }
        let link = try XCTUnwrap(clock.link)
        clock.fire(link)
        XCTAssertEqual(ticks, ["session", "leaves"])
        XCTAssertTrue(clock.running)
        clock.drop(leaves)
        XCTAssertFalse(clock.running)
    }

    func testReadyPublicationsAllowWorkOnEveryConsecutiveFrame() throws {
        let wasAsync = ExactSession.asyncFills
        ExactSession.asyncFills = true
        defer { ExactSession.asyncFills = wasAsync }
        for (tasks, motion) in [(false, true), (true, false), (true, true)] {
            let session = ExactApp.shared.makeSession(label: "consecutive-frames")
            let demand = Owner()
            let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 700))
            defer { FrameClock.shared.drop(demand); session.destroy(); window.isHidden = true }
            session.clock = nil
            if tasks {
                window.addSubview(session.presenter.viewport)
                session.presenter.viewport.frame = window.bounds
                window.makeKeyAndVisible()
                XCTAssertNil(session.boot(plan: try fixturePlan("FRAME_HEALTH"), size: window.bounds.size).error)
                waitForActivation(session)
                session.drainFill()
                XCTAssertTrue(session.frames.tasks)
            } else {
                // Keep requesting motion samples even though this empty runtime has no animation.
                FrameClock.shared.want(demand, .session) { _ in
                    session.frames.submitMotionTick(now: session.now(), frame: nil)
                }
            }
            var landed = 0, motionLanded = 0
            session.onLandForTests = { kind, _ in
                switch kind {
                case .tick: motionLanded += 1; if !tasks { landed += 1 }
                case .advance where tasks: landed += 1
                default: break
                }
                // This runtime has frame tasks but no animation; keep the motion demand live across its batches.
                if tasks && motion { session.frames.motion = true }
            }
            let clock = FrameClock.shared, link = try XCTUnwrap(clock.link)
            for frame in 1...6 {
                if tasks && motion { session.frames.motion = true }
                clock.fire(link)
                ExactCore.Owner.shared.sync {}
                XCTAssertTrue(tasks ? session.advanceInFlight : session.tickInFlight,
                              "tasks=\(tasks): frame \(frame) submits work after the previous publication lands")
                session.landPublished()
                XCTAssertEqual(landed, frame, "tasks=\(tasks) motion=\(motion): no alternate-frame hold")
                if motion { XCTAssertEqual(motionLanded, frame, "motion arrives on every frame alongside tasks") }
                XCTAssertFalse(session.advanceInFlight || session.tickInFlight)
            }
            session.drainFill()
            XCTAssertEqual(landed, 6, "tasks=\(tasks) motion=\(motion): six display frames produce six owner updates")
            if motion { XCTAssertEqual(motionLanded, 6) }
        }
    }

    func testAGoneOwnerStopsTicking() throws {
        let clock = FrameClock()
        var owner: Owner? = Owner()
        var ticks = 0
        clock.want(owner!, .heavyLeaves) { _ in ticks += 1 }
        owner = nil
        clock.fire(try XCTUnwrap(clock.link))
        XCTAssertEqual(ticks, 0)
        XCTAssertFalse(clock.running)
    }

    /// A list's smooth correction takes the app's frames, not a link of its
    /// own, and gives them back when it ends or is cancelled.
    func testASmoothCorrectionRunsOnTheAppsClock() throws {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 300, height: 400))
        let scroll = UIScrollView(frame: window.bounds)
        scroll.contentSize = CGSize(width: 300, height: 2000)
        window.addSubview(scroll)
        window.makeKeyAndVisible()
        let made = FrameClock.shared.linksMade
        var ended: Bool?
        let driver = OffsetDriver(scroll: scroll, to: CGPoint(x: 0, y: 600), duration: 0.3, serial: 1,
                                  done: { _, finished in ended = finished }, reclamp: { _ in })
        driver.start()
        XCTAssertTrue(FrameClock.shared.wants(driver))
        XCTAssertLessThanOrEqual(FrameClock.shared.linksMade, max(made, 1))
        let link = try XCTUnwrap(FrameClock.shared.link)
        XCTAssertEqual(link.preferredFrameRateRange.maximum, FrameClock.full(on: window.screen).maximum, "a correction moves at the display's rate")
        FrameClock.shared.fire(link)
        XCTAssertNil(ended)
        driver.cancel()
        XCTAssertFalse(FrameClock.shared.wants(driver))
    }
}
#endif
