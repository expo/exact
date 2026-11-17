import CoreFoundation
import XCTest
#if os(macOS)
import AppKit
#else
import UIKit
#endif
@testable import ExactKit

final class HatchClockTests: XCTestCase {
    func testCoalescedAftersDoNotDriveSeeksOrSettlement() {
        let clock = HatchClock()
        clock.after(1, ms: 100, coalescing: true)
        XCTAssertNil(clock.nextAfter)
        XCTAssertNil(clock.nextInstant)
        clock.after(2, ms: 200)
        XCTAssertEqual(clock.nextAfter, 200)
        XCTAssertEqual(clock.nextInstant, 200)
        clock.after(2, ms: -1)
        XCTAssertNil(clock.nextAfter)
        XCTAssertNil(clock.nextInstant)
    }

    func testAfterFiresWhileTracking() {
        XCTAssertTrue(Thread.isMainThread)
        #if os(macOS)
        _ = NSApplication.shared
        let mode = RunLoop.Mode.eventTracking
        #else
        let mode = RunLoop.Mode.tracking
        #endif
        // Set up common-mode membership without relying on application startup.
        CFRunLoopAddCommonMode(CFRunLoopGetMain(), CFRunLoopMode(rawValue: mode.rawValue as CFString))
        let session = ExactApp.shared.makeSession()
        let clock = session.natives.hatchClock
        defer { clock.reset(); session.destroy() }
        session.clock = nil
        clock.natives = session.natives
        clock.after(7, ms: 20)
        XCTAssertNotNil(clock.nextAfter)

        let end = Date(timeIntervalSinceNow: 2)
        while clock.nextAfter != nil && Date() < end {
            RunLoop.main.run(mode: mode, before: min(end, Date(timeIntervalSinceNow: 0.01)))
        }
        XCTAssertNil(clock.nextAfter, "a due hatch after must fire before tracking ends")
    }

    /// A session's end stops its display link although a module's frame
    /// ticket was live and motion ran.
    func testDestroyStopsTheLinkATicketKept() {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let session = ExactApp.shared.makeSession()
        session.clock = nil
        let clock = session.natives.hatchClock
        clock.natives = session.natives
        clock.frames(1, on: true)
        XCTAssertNotNil(session.frames.link, "a ticket runs the link")
        session.frames.motion = true
        session.destroy()
        XCTAssertEqual(clock.liveTickets, 0)
        XCTAssertNil(session.frames.link, "destroy left the display link running")
    }

    /// On the wall the hatch clock's 4,096-call bound is one presented frame's
    /// (LLP 1075.003.000.001 §2.4): a ticket ticks for as long as frames come,
    /// and one frame still stops at the bound.
    func testAWallTicketTicksPastTheBoundAndOneFrameStopsAtIt() {
        let clock = HatchClock()
        clock.frames(1, on: true)
        let frames = HatchClock.limit + 100
        let ticked = (0..<frames).filter { clock.presented(Double($0) * 1000 / 120) }.count
        XCTAssertEqual(ticked, frames, "every frame's ticket ticks, 4,096 frames in and after")

        let crowded = HatchClock()
        for token in 1...UInt64(HatchClock.limit + 1) { crowded.frames(token, on: true) }
        XCTAssertFalse(crowded.presented(0), "one frame calls 4,096 ticks at most")
    }
}
