#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactDrag

/// A hold's samples are taken at the input's own time, carried onto the
/// session clock from where the hold began (LLP 1057.001 §3): with the clock
/// standing still, as a driver may hold it while a finger moves, the release
/// is still the finger's. The holds are shared code; macOS compiles the plans.
@MainActor
final class HoldInputTimeMacTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactDrag.install() } // LLP 1047.001 D4
    private var window: NSWindow?
    private var session: ExactSession?
    override func tearDown() { session?.destroy(); session = nil; window?.close(); window = nil; super.tearDown() }

    private func compile(_ source: String) throws -> Data {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"], "build.mjs --test builds the Contract compiler and names it"))
        compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
        compiler.standardError = FileHandle.nullDevice
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: dir.appendingPathComponent("app.plan"))
    }

    /// A session showing `source` in a window, its clock the driver's, taken
    /// over where the wall's stood.
    private func booted(_ source: String) throws -> ExactSession {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "hold-input-time")
        self.session = session
        let view = ExactView(session: session)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 600), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let mount = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 600))
        window.contentView = mount
        view.frame = mount.bounds; mount.addSubview(view)
        window.orderFrontRegardless()
        self.window = window
        XCTAssertNil(session.boot(plan: try compile(source), size: CGSize(width: 400, height: 600)).error)
        window.layoutIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.2))
        session.clock = session.now()
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, testId)
    }

    private func slot(_ session: ExactSession, _ name: String) -> Double? {
        let json = try? JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any]
        return (json?["slots"] as? [String: Any])?[name] as? Double
    }

    func testAHeightDragReleasesAtTheFingersPaceWhileTheClockStandsStill() throws {
        let session = try booted("""
        component App
          state seen = 0
          action release(height: number, velocity: number)
            seen = velocity
          view
            column id="sheet" height=300 box-sizing="border-box"
              box testId="header" heightDragFor="sheet" heightrelease=release height=40

        """)
        let header = try node(session, "header"), clock = session.now()
        // 80 points up in 40 ms, a touch every 10 ms, then a lift where the
        // finger was 8 ms later, which is no sample.
        let start = 500.0
        let hold = try XCTUnwrap(HeightDragHold(header, time: start))
        for i in 1...4 { XCTAssertTrue(hold.move(downward: -20 * Double(i), time: start + 0.01 * Double(i))) }
        hold.finish(downward: -80, time: start + 0.048, cancel: false)
        XCTAssertEqual(session.now(), clock, "the clock stood still")
        let seen = try XCTUnwrap(slot(session, "seen"))
        XCTAssertEqual(seen, 2000, accuracy: 1e-6)
    }

    func testARecognizedPinchReleasesAtItsPaceWhileTheClockStandsStill() throws {
        let session = try booted("""
        component App
          state vs = 0
          action geometry(w: number, h: number, pw: number, ph: number)
            vs = vs
          action release(x: number, y: number, s: number, vx: number, vy: number, v: number)
            vs = v
          view
            column testId="clip" width=300 height=300 overflow="hidden" padding=0 border-width=0
              column id="photo" width="100%" height="100%" box-sizing="border-box" padding=0 border-width=0 scale=1 transition="scale -exact-spring(180, 12, 1)"
                box testId="handle" transformDragFor="photo" transformgeometry=geometry transformrelease=release width="100%" height="100%"

        """)
        RunLoop.main.run(until: Date().addingTimeInterval(0.2)) // the geometry report
        let clock = session.now()
        XCTAssertNil(TransformDragHold.recognizedPinch(try node(session, "handle"), scale: 2, focal: .zero))
        XCTAssertEqual(session.now(), clock, "the clock stood still")
        // Eight steps to 2× at 60 Hz: the scale grew 0.125 a frame, 7.5 a second.
        XCTAssertEqual(try XCTUnwrap(slot(session, "vs")), 7.5, accuracy: 1e-6)
    }
}
#endif
