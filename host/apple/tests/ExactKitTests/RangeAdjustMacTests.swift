#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// An assistive app's increment and decrement of a native range over
/// `range-adjust.contract`, compiled and run: the range's arrow key first,
/// which its `key` handler may take (LLP 1069.001, amended 2026-11-16), and
/// an answer that says whether the value moved.
final class RangeAdjustMacTests: XCTestCase {
    private var session: ExactSession?
    private var window: NSWindow?

    override func tearDown() {
        session?.destroy(); session = nil
        window?.close(); window = nil
        super.tearDown()
    }

    private func booted() throws -> Presenter {
        _ = NSApplication.shared
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_RANGE_ADJUST_PLAN"], "build.mjs --test compiles the range-adjust fixture")
        let session = ExactApp.shared.makeSession(label: "range-adjust-" + UUID().uuidString)
        self.session = session
        let view = ExactView(session: session)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 500), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        self.window = window
        let mount = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 500))
        window.contentView = mount
        view.frame = mount.bounds; mount.addSubview(view)
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: CGSize(width: 400, height: 500)).error)
        return session.presenter
    }

    private func range(_ p: Presenter, _ testId: String) throws -> (NSSlider, NSSliderCell) {
        let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == testId }, testId)
        let slider = try XCTUnwrap(p.controls.controls[node.id] as? NSSlider)
        return (slider, try XCTUnwrap(slider.cell as? NSSliderCell))
    }

    func testAStepsAnswerIsWhetherTheValueMoved() throws {
        let p = try booted()
        let (seek, seekCell) = try range(p, "seek")
        XCTAssertTrue(seekCell.accessibilityPerformIncrement(), "the handler's ten")
        XCTAssertEqual(seek.doubleValue, 50)
        XCTAssertTrue(seekCell.accessibilityPerformDecrement())
        XCTAssertEqual(seek.doubleValue, 40)
        _ = p.controls.type(try XCTUnwrap(p.views.values.first { $0.props["testId"] == "seek" }), "240")
        XCTAssertEqual(seek.doubleValue, 240)
        XCTAssertFalse(seekCell.accessibilityPerformIncrement(), "the handler took the key at the end of the range")
        XCTAssertEqual(seek.doubleValue, 240)
        // Its handler puts it at 240 and prevents nothing; AppKit's step has nowhere left to go.
        let (edge, edgeCell) = try range(p, "edge")
        XCTAssertTrue(edgeCell.accessibilityPerformIncrement(), "the handler moved it onto its maximum")
        XCTAssertEqual(edge.doubleValue, 240)
        XCTAssertFalse(edge.accessibilityPerformIncrement(), "already there")
        let (plain, plainCell) = try range(p, "plain")
        XCTAssertTrue(plainCell.accessibilityPerformIncrement(), "AppKit's own step")
        XCTAssertEqual(plain.doubleValue, 52)
    }
}
#endif
