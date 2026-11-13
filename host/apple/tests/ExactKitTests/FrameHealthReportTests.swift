import Foundation
import QuartzCore
import XCTest
import CExact
#if os(macOS)
import AppKit
#else
import UIKit
#endif
@testable import ExactCore
@testable import ExactKit

final class FrameHealthReportTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        #if os(macOS)
        _ = NSApplication.shared
        #endif
    }
    private func populated() -> (FrameHealth, Double) {
        let health = FrameHealth(enabled: true), start = CACurrentMediaTime()
        _ = health.setAgentCapture("start")
        health.requestedRate(120)
        for index in 0..<40 {
            let at = start + Double(index) * 0.024
            health.record(arrival: at, target: at + 0.008, period: 0.008, finished: at + 0.001)
        }
        return (health, start + 1)
    }

    func testTailIsBoundedLossAndMissingEvidenceSurviveEncodingAndExpiry() throws {
        let (health, now) = populated()
        defer { health.destroy() }
        let report = FrameHealthReport.snapshot(health: health, session: nil, now: now)
        let data = FrameHealthReport.encode(report)
        XCTAssertLessThanOrEqual(data.count + 1, 4000)
        let decoded = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let columns = try XCTUnwrap(decoded["columns"] as? [String])
        let events = try XCTUnwrap(decoded["hitches"] as? [[Any]])
        XCTAssertGreaterThan(events.count, 0)
        XCTAssertEqual(events.last?[0] as? String, "39")
        XCTAssertEqual((decoded["retention"] as? [String: Any])?["omittedForBudget"] as? Int, 16 - events.count)
        // Retained hitches keep microseconds: 24 ms between arrivals, and the 120 Hz request
        XCTAssertEqual(events.first?[columns.firstIndex(of: "gap_us")!] as? UInt64, 24_000)
        XCTAssertEqual(events.first?[columns.firstIndex(of: "requested_period_us")!] as? UInt64, 8_333)
        XCTAssertTrue(events.first?[columns.firstIndex(of: "owner_queued_offset_us")!] is NSNull)
        XCTAssertEqual((decoded["time"] as? [String: Any])?["unit"] as? String, "us")
        XCTAssertEqual((decoded["retention"] as? [String: Any])?["overwrittenSinceClear"] as? String, "23")
        XCTAssertEqual((decoded["retention"] as? [String: Any])?["automaticDiskBytes"] as? Int, 0)
        XCTAssertTrue((decoded["unobserved"] as? [String])?.contains("presentation") == true)
        health.reset()
        XCTAssertEqual(health.historyStatus().retained, 16, "a reload preserves the evidence a person wants to share")
        XCTAssertEqual(health.hitch(0).generation, 0)
        XCTAssertEqual(health.snapshot().generation, 1)
        _ = health.setAgentCapture("stop")
        let expired = FrameHealthReport.snapshot(health: health, session: nil, now: now + 121)
        XCTAssertEqual((expired["hitches"] as? [[Any]])?.count, 0)
        XCTAssertEqual((expired["retention"] as? [String: Any])?["expiredAtRead"] as? Int, 16)
        XCTAssertEqual(health.historyStatus().retained, 16, "age filtering does not allocate an expiry timer or grow storage")
        let oversized = FrameHealthReport.encode(["unexpected": String(repeating: "x", count: 20_000)])
        XCTAssertLessThan(oversized.count, 1024)
        XCTAssertNotNil((try JSONSerialization.jsonObject(with: oversized) as? [String: Any])?["error"])
    }

    func testUnchangedLateOwnerWorkReachesTheReportWithoutAnApply() throws {
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        // At this uptime, (start + 0.016) + 0.008 and start + 0.024 convert to different nanoseconds.
        let start = 2_097_152.00000039, deadline = start + 0.008, landed = start + 0.012
        health.agentCaptureUntil = start + 60
        health.updateCapture(at: start)
        func land(at start: Double, finished: Double? = nil) {
            let deadline = start + 0.008, finished = finished ?? deadline
            health.sending(FrameHealth.tick, start: start, deadline: deadline)
            health.record(arrival: start, target: deadline, period: 0.008, finished: start + 0.001, sent: FrameHealth.tick)
            if finished > deadline {
                health.record(arrival: start + 0.008, target: start + 0.016, period: 0.008, finished: start + 0.009, held: true, ran: false)
            }
            health.landed(FrameHealth.tick, at: finished)
        }
        land(at: start, finished: landed)
        health.landed(FrameHealth.tick, at: landed + 0.001)
        land(at: start + 0.016)
        health.stop()
        XCTAssertEqual(health.snapshot().worked, 2)
        XCTAssertEqual(health.snapshot().late, 1)
        XCTAssertEqual(health.historyStatus().retained, 1)
        XCTAssertEqual(health.historyStatus().apply_deadlines, 0)
        let now = start + 0.025
        let data = FrameHealthReport.encode(FrameHealthReport.snapshot(health: health, session: nil, now: now))
        XCTAssertLessThanOrEqual(data.count + 1, 4000)
        let report = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let rows = try XCTUnwrap(report["hitches"] as? [[Any]])
        XCTAssertEqual(rows.count, 1)
        let event = try XCTUnwrap(rows.first)
        let columns = try XCTUnwrap(report["columns"] as? [String])
        XCTAssertEqual(event[columns.firstIndex(of: "flags")!] as? UInt64, 128 | 8)
        let age = (FrameHealth.nanoseconds(now) - FrameHealth.nanoseconds(landed)) / 1000
        XCTAssertEqual(event[columns.firstIndex(of: "age_us")!] as? UInt64, age)
        XCTAssertEqual(event[columns.firstIndex(of: "target_offset_us")!] as? Int, Int(((deadline - landed) * 1_000_000).rounded()))
        let frameStart = try XCTUnwrap(columns.firstIndex(of: "frame_start_offset_us"))
        XCTAssertEqual(event[frameStart] as? Int, -12_000)
        XCTAssertTrue(event[columns.firstIndex(of: "apply_began_offset_us")!] is NSNull)
        XCTAssertTrue(event[columns.firstIndex(of: "main_landed_offset_us")!] is NSNull)

        for index in 2..<22 { land(at: start + Double(index) * 0.016, finished: start + Double(index) * 0.016 + 0.012) }
        XCTAssertEqual(health.historyStatus().retained, 16)
        XCTAssertEqual(health.historyStatus().overwritten, 5)
        let tail = FrameHealthReport.encode(FrameHealthReport.snapshot(health: health, session: nil, now: start + 0.36))
        XCTAssertLessThanOrEqual(tail.count + 1, 4000)
        let decoded = try XCTUnwrap(JSONSerialization.jsonObject(with: tail) as? [String: Any])
        XCTAssertFalse((decoded["hitches"] as? [[Any]])?.isEmpty ?? true)
    }

    func testDenseEvidenceFitsFourKBWithoutChangingTimingPrecision() throws {
        let (health, now) = populated()
        defer { health.destroy() }
        var report = FrameHealthReport.snapshot(health: health, session: nil, now: now)
        let columns = try XCTUnwrap(report["columns"] as? [String])
        let rows: [[Any]] = (0..<16).map { index in
            columns.enumerated().map { column, _ -> Any in
                column < 3 ? String(index + 1) as Any : Int64(-8_999_999_999_999_991) as Any
            }
        }
        report["hitches"] = rows
        report["identity"] = ["app": String(repeating: "é", count: 80), "device": String(repeating: "🦊", count: 40)]
        let data = FrameHealthReport.encode(report)
        XCTAssertLessThanOrEqual(data.count + 1, 4000, "the carrier newline is part of the hard cap")
        let decoded = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let events = try XCTUnwrap(decoded["hitches"] as? [[Any]])
        XCTAssertGreaterThan(events.count, 0)
        XCTAssertLessThan(events.count, 16)
        XCTAssertEqual(events.last?[3] as? Int64, -8_999_999_999_999_991)
        XCTAssertEqual((decoded["retention"] as? [String: Any])?["omittedForBudget"] as? Int, 16 - events.count)
    }

    func testAgentReportAcknowledgesOnlyAfterItsAsynchronousReply() throws {
        let session = ExactApp.shared.makeSession(label: "frame-health-report-order")
        defer { session.destroy() }
        let pipe = Pipe(), saved = Agent.out, savedRoutes = Agent.routes
        Agent.out = pipe.fileHandleForWriting
        Agent.routes = [(session.label, session)]
        defer { Agent.out = saved; Agent.routes = savedRoutes }
        let done = expectation(description: "the carrier may read the next request after the report is written")
        var acknowledged = false
        Agent.handle("{\"op\":\"perf\",\"frames\":true,\"report\":true}") {
            acknowledged = true
            let data = pipe.fileHandleForReading.availableData
            XCTAssertLessThanOrEqual(data.count, 4000)
            XCTAssertNotNil(try? JSONSerialization.jsonObject(with: data))
            done.fulfill()
        }
        XCTAssertFalse(acknowledged)
        wait(for: [done], timeout: 5)
        XCTAssertTrue(acknowledged)
    }

    func testCaptureLeasesHudOwnershipAndReportBackpressureAreIndependent() throws {
        let (health, now) = populated()
        defer { health.destroy() }
        health.setHUDVisible(true)
        _ = health.setAgentCapture("stop")
        XCTAssertNotEqual(health.captureID, 0, "an agent stop cannot disable the person's monitor")
        health.setHUDVisible(false)
        XCTAssertEqual(health.captureID, 0)
        health.requestedRate(60)
        _ = health.setAgentCapture("start", duration: 1)
        XCTAssertEqual(health.requestedPeriodNs, 16_666_667, "policy changes are cached even while detail capture is paused")
        health.expireCapture(at: now + 1)
        XCTAssertEqual(health.captureID, 0, "an expired agent lease needs no timer or idle wakeup")

        let finished = expectation(description: "one bounded utility encoding completes")
        health.agentReport(session: nil, now: now + 1000) { data in
            XCTAssertTrue(Thread.isMainThread)
            XCTAssertLessThanOrEqual(data.count, FrameHealthReport.maximumBytes)
            finished.fulfill()
        }
        XCTAssertTrue(health.reportInFlight)
        var busy: [String: Any]?
        let secondProducer = FrameHealth(enabled: true)
        defer { secondProducer.destroy() }
        FrameHealthReport.prepare(health: secondProducer, session: nil, now: now) { data in
            busy = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        }
        XCTAssertEqual(busy?["error"] as? String, "report-busy")
        var limited: [String: Any]?
        health.agentReport(session: nil, now: now + 1001) { data in
            limited = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        }
        XCTAssertEqual(limited?["error"] as? String, "report-rate-limit")
        XCTAssertEqual(limited?["retryAfterMs"] as? Int, 14_000)
        wait(for: [finished], timeout: 5)
        XCTAssertFalse(health.reportInFlight)
    }

    func testPendingWorkExpiresTheAgentLeaseWithoutAReadOrApply() throws {
        for hudVisible in [false, true] {
            let health = FrameHealth(enabled: true)
            defer { health.destroy() }
            health.setHUDVisible(hudVisible)
            _ = health.setAgentCapture("start", duration: 10)
            let start = health.agentCaptureUntil - 0.010
            let frameStart = start + 0.001, deadline = start + 0.009, landed = start + 0.013
            health.sending(FrameHealth.tick, start: frameStart, deadline: deadline)
            health.record(arrival: frameStart, target: deadline, period: 0.008,
                          finished: start + 0.002, sent: FrameHealth.tick)
            health.landed(FrameHealth.tick, at: landed)
            XCTAssertEqual(health.agentCaptureUntil, 0, "landing expires the lease without another callback or read")
            XCTAssertEqual(health.captureID != 0, hudVisible, "the card owns capture independently")
            XCTAssertEqual(health.snapshot().worked, 1)
            XCTAssertEqual(health.snapshot().late, 1, "capture expiry preserves core counters")
            XCTAssertEqual(health.historyStatus().active, hudVisible ? 1 : 0)
            XCTAssertEqual(health.historyStatus().retained, hudVisible ? 1 : 0)
            XCTAssertEqual(health.historyStatus().apply_deadlines, 0)
            let report = FrameHealthReport.snapshot(health: health, session: nil, now: landed)
            XCTAssertEqual((report["hitches"] as? [[Any]])?.count, hudVisible ? 1 : 0)
        }
    }

    func testAsynchronousTickCarriesActualOwnerAndApplyBoundaries() throws {
        let session = ExactApp.shared.makeSession(label: "frame-health-owner")
        defer { session.destroy() }
        session.clock = nil
        XCTAssertNil(session.boot(size: CGSize(width: 390, height: 844)).error)
        _ = session.frameHealth.setAgentCapture("start")
        let done = expectation(description: "owner tick returns and applies on main")
        let frame = (CACurrentMediaTime() - 0.002 - ExactEnv.t0) * 1000
        session.runtime.tickAsync(now: session.now(), frame: frame, capture: session.frameHealth.captureID) { batch in
            DispatchQueue.main.async {
                var landed = batch
                landed.frameWorkTiming?.landed = CACurrentMediaTime()
                session.apply(landed)
                done.fulfill()
            }
        }
        wait(for: [done], timeout: 10)
        let health = session.frameHealth
        XCTAssertGreaterThan(health.historyStatus().apply_deadlines, 0)
        let events = (0..<UInt32(health.historyStatus().retained)).map { health.hitch($0) }
        let event = try XCTUnwrap(events.first { $0.flags & 4 != 0 })
        // Offsets from the event, which an apply deadline places at the apply's end
        XCTAssertNotEqual(event.work.queued_us, Int32.min)
        XCTAssertGreaterThanOrEqual(event.work.owner_began_us, event.work.queued_us)
        XCTAssertGreaterThanOrEqual(event.work.owner_finished_us, event.work.owner_began_us)
        XCTAssertGreaterThanOrEqual(event.work.landed_us, event.work.owner_finished_us)
        XCTAssertGreaterThanOrEqual(event.work.began_us, event.work.landed_us)
        XCTAssertEqual(event.work.finished_us, 0)
        XCTAssertGreaterThan(event.work.finished_us, event.work.target_us)
    }

    func testAnExpiredLeaseDoesNotReachAFramesOwnerWork() throws {
        let session = ExactApp.shared.makeSession(label: "expired-frame-capture")
        defer { session.destroy() }
        session.clock = nil
        XCTAssertNil(session.boot(size: CGSize(width: 390, height: 844)).error)
        let health = session.frameHealth
        _ = health.setAgentCapture("start")
        health.agentCaptureUntil = CACurrentMediaTime() - 1
        var captures: [UInt64] = []
        session.onApplyForTests = { _ in captures.append(health.captureID) }
        session.onLandForTests = { _, batch in
            if let timing = batch.frameWorkTiming { captures.append(timing.capture) }
        }
        #if os(macOS)
        let link = session.presenter.viewport.displayLink(target: session.frames, selector: #selector(Frames.tick(_:)))
        #else
        let link = CADisplayLink(target: session.frames, selector: #selector(Frames.tick(_:)))
        #endif
        defer { link.invalidate() }
        session.frames.motion = true
        session.frames.tick(link)
        session.drainFill()
        session.frames.motion = false
        session.frames.run(false)
        XCTAssertFalse(captures.isEmpty, "the frame did owner work")
        XCTAssertTrue(captures.allSatisfy { $0 == 0 }, "the lease expires before the frame sends or applies owner work: \(captures)")
        XCTAssertEqual(health.captureID, 0)
        XCTAssertEqual(health.agentCaptureUntil, 0)
    }

    func testAnApplyAfterTheLeaseDeadlineAddsNoCaptureEvidence() throws {
        let session = ExactApp.shared.makeSession(label: "expired-apply-capture")
        defer { session.destroy() }
        session.clock = nil
        XCTAssertNil(session.boot(size: CGSize(width: 390, height: 844)).error)
        let health = session.frameHealth
        _ = health.setAgentCapture("start")
        let now = CACurrentMediaTime()
        health.agentCaptureUntil = now - 1
        var batch = wireBatch([])
        batch.frameWorkTiming = FrameWorkTiming(capture: health.captureID, frame: (now - 0.01 - ExactEnv.t0) * 1000,
            queued: now - 0.02, began: now - 0.019, finished: now - 0.018, landed: now - 0.001)
        session.apply(batch)
        XCTAssertEqual(health.historyStatus().apply_deadlines, 0, "an expired capture retains no post-deadline apply")
        XCTAssertEqual(health.historyStatus().retained, 0)
        XCTAssertEqual(health.captureID, 0)
    }

    func testFrameCaptureAgentRequestDoesNotEnableTheLegacySampler() throws {
        let session = ExactApp.shared.makeSession(label: "frame-health-agent")
        defer { session.destroy() }
        session.clock = nil
        session.sampler?.stop(); session.sampler = nil
        let pipe = Pipe(), saved = Agent.out
        Agent.out = pipe.fileHandleForWriting
        defer { Agent.out = saved }
        let request: [String: Any] = ["op": "perf", "frames": true, "capture": "start", "durationMs": 1000]
        Agent(session: session).handle(op: "perf", request, line: "")
        let reply = try XCTUnwrap(JSONSerialization.jsonObject(with: pipe.fileHandleForReading.availableData) as? [String: Any])
        XCTAssertEqual(reply["capture"] as? Bool, true)
        XCTAssertNil(session.sampler)
        XCTAssertNotNil(session.frameHealth.setAgentCapture("start", duration: .nan)["error"])
    }

    /// Measuring the screen, the card's Share gives Exact's frames as a report of their own beside the screen's, and each
    /// gives the median, p99 and maximum of every frame's timing since the reset: an app drawn by GPU surfaces has its
    /// frame times only on Exact's clock (LLP 1079.000 §7.4)
    func testShareGivesExactsFramesBesideTheScreensWithEveryFramesTiming() throws {
        let screen = FrameHealth(policy: .always, source: FrameHealth.screenSource), exact = FrameHealth(enabled: true)
        defer { screen.destroy(); exact.destroy() }
        screen.setHUDVisible(true)
        exact.setHUDVisible(true)
        let now = CACurrentMediaTime()
        // Sixty frames a refresh apart at 60 Hz, each with 4.2 ms of work, the last 20.2 ms
        for n in 0..<60 {
            let at = now - 1.2 + Double(n) / 60
            exact.record(arrival: at, target: at + 1 / 60.0, period: 1 / 60.0, finished: at + (n == 59 ? 0.0202 : 0.0042))
        }
        screen.record(arrival: now - 0.0205, target: now - 0.012, period: 0, finished: now - 0.0102)
        let clock = FrameHealthReport.snapshot(health: exact, session: nil, now: now, beside: screen)
        XCTAssertEqual(clock["source"] as? String, exact.source)
        XCTAssertEqual(clock["selection"] as? String, "one-source; the card's Share gives ui-update as a report of its own")
        XCTAssertEqual((clock["otherSources"] as? [[String: Any]])?.compactMap { $0["source"] as? String }, ["ui-update"])
        let core = try XCTUnwrap(clock["coreSinceReload"] as? [String: Any])
        XCTAssertEqual(core["frameIntervals"] as? String, "59")
        let frameTime = try XCTUnwrap(core["frameTimeBoundsNs"] as? [String: Any])
        XCTAssertEqual(frameTime["median"] as? [UInt64], [16_500_000, 17_000_000], "16.7 ms, in its half-millisecond bin")
        XCTAssertEqual(frameTime["maximum"] as? [UInt64], [16_500_000, 17_000_000])
        let work = try XCTUnwrap(core["workBoundsNs"] as? [String: Any])
        XCTAssertEqual(work["median"] as? [UInt64], [4_000_000, 4_500_000])
        XCTAssertEqual(work["p99"] as? [UInt64], [20_000_000, 20_500_000])
        XCTAssertEqual(work["maximum"] as? [UInt64], [20_000_000, 20_500_000])
        XCTAssertLessThanOrEqual(FrameHealthReport.encode(clock).count + 1, 4000)
        // The screen's report names Exact's frames as the other, and has work but no frame times
        let shown = FrameHealthReport.snapshot(health: screen, session: nil, now: now, beside: exact)
        XCTAssertEqual(shown["selection"] as? String, "one-source; the card's Share gives \(exact.source) as a report of its own")
        let screenCore = try XCTUnwrap(shown["coreSinceReload"] as? [String: Any])
        XCTAssertTrue(screenCore["frameTimeBoundsNs"] is NSNull)
        XCTAssertEqual((screenCore["workBoundsNs"] as? [String: Any])?["maximum"] as? [UInt64], [10_000_000, 10_500_000])
        // An agent's report names no Share
        XCTAssertNil(FrameHealthReport.snapshot(health: exact, session: nil, now: now)["selection"])
    }
}
