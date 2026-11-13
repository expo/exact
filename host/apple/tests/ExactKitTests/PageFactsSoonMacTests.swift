#if os(macOS)
import AppKit
import ExactCore
import XCTest
@testable import ExactKit

/// LLP 1027.007 P1: a system notice of the page's facts does not hold the
/// main thread while the data source activates on the owner, and its batches
/// land before any batch the owner makes after them.
final class PageFactsSoonMacTests: XCTestCase {
    private static let plan: Result<Data, Error> = Result {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let source = "shape Page\n  visibilityState: string\ncomponent App\n  resource page = exactPage() as shape Page\n  state count = 0\n  action add\n    count = count + 1\n  view\n    column\n      text `${count}`\n      text `${page.visibilityState}` testId=\"visibility\"\n      button \"Add\" testId=\"add\" press=add\n"
        try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"], "build.mjs --test builds the Contract compiler and names it"))
        compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: dir.appendingPathComponent("app.plan"))
    }

    func testANoticeWhileTheOwnerIsBusyReturnsAtOnceAndLandsBeforeALaterBatch() throws {
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 200), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        let session = ExactApp.shared.makeSession(label: "page-soon")
        defer { session.destroy() }
        window.contentView = ExactView(session: session)
        XCTAssertNil(session.boot(plan: try Self.plan.get(), size: CGSize(width: 300, height: 200)).error)
        window.makeKeyAndOrderFront(nil)
        session.tellPage()
        XCTAssertNotNil(session.readsOnLineKnown, "a synchronous tell learns whether the plan reads onLine")
        waitFor("the first draw's activation settles") {
            session.dataGeneration == session.generation && !session.activationInFlight
        }
        XCTAssertEqual(session.dataGeneration, session.generation, "the first draw's activation settles")
        XCTAssertFalse(session.activationInFlight)
        session.tellPage()

        let add = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "add" })
        let press = try XCTUnwrap(session.presenter.onPress)
        var landings: [String] = []
        var unchangedNotices = 0
        session.onLandForTests = { kind, batch in
            if case .notice = kind {
                landings.append("notice")
                if session.changesNothing(batch) { unchangedNotices += 1 }
            }
        }
        session.onApplyForTests = { batch in
            if batch.input != nil { landings.append("press") }
        }
        defer { session.onLandForTests = nil; session.onApplyForTests = nil }

        // The owner busy as it is while the data source activates.
        let held = DispatchSemaphore(value: 0)
        defer { held.signal() }
        Owner.shared.post { held.wait() }
        session.dataPreparing = true
        let started = Date()
        session.tellPageSoon()
        XCTAssertLessThan(Date().timeIntervalSince(started), 0.05, "the main thread does not wait for the busy owner")
        XCTAssertEqual(session.noticesInFlight, 3, "the facts, the root font size and the display scale are on the owner")
        XCTAssertTrue(landings.isEmpty)
        held.signal()
        session.dataPreparing = false

        // AppKit click tracking can run the main-queue hop that lands the
        // notices, hiding a missing join in apply. Dispatch without yielding.
        press(add.id)
        XCTAssertEqual(landings, ["notice", "notice", "notice", "press"], "the notices land before the later press batch applies")
        XCTAssertEqual(unchangedNotices, 3, "notices that skip apply must still land before the press")
        XCTAssertEqual(session.noticesInFlight, 0)
        XCTAssertNotNil(session.presenter.views.values.first { $0.props["text"] == "1" }, "and the press applies")
    }
}
#endif
