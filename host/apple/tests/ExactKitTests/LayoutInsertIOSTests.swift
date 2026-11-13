// Shared by both Apple hosts; the iOS runner selects *IOSTests by name.
#if os(macOS)
import AppKit
#else
import UIKit
#endif
import XCTest
@testable import ExactKit

/// `scripts/fixtures/layout-insert.contract`: a row inserted by `if` above a
/// sibling that slides under its layout transition (LLP 1063).
///   bun host/apple/build.mjs --test --ios --only LayoutInsertIOSTests
///   bun host/apple/build.mjs --test --only LayoutInsertIOSTests
final class LayoutInsertIOSTests: XCTestCase {
    #if os(macOS)
    private var window: NSWindow?
    #else
    private var window: UIWindow?
    #endif
    private var session: ExactSession?

    override func tearDown() {
        session?.destroy()
        session = nil
        #if os(macOS)
        window?.close()
        #else
        window?.isHidden = true
        #endif
        window = nil
        super.tearDown()
    }

    private func boot() throws -> ExactSession {
        let plan = try fixturePlan("LAYOUT_INSERT")
        let session = ExactApp.shared.makeSession(label: "layout-insert")
        self.session = session
        let view = ExactView(session: session)
        #if os(macOS)
        _ = NSApplication.shared
        let size = NSSize(width: 402, height: 874)
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = view
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: size).error)
        view.layoutSubtreeIfNeeded()
        #else
        let scene = try XCTUnwrap(hostedWindowScene(), "a window of the test host's scene")
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        #endif
        waitFor("the page is laid out") { self.node(session, "below")?.window != nil }
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) -> NodeView? {
        session.presenter.views.values.first { $0.props["testId"] == testId }
    }

    /// Where layout put `v` in its parent: on iOS from its centre, which its transform
    /// leaves alone (its frame does not); AppKit moves only the layer.
    private func laidY(_ v: NodeView) -> CGFloat {
        #if os(macOS)
        v.frame.minY
        #else
        v.center.y - v.bounds.height / 2
        #endif
    }
    /// Where `v` shows: there, moved by the layout transition's offset.
    private func shownY(_ v: NodeView) -> CGFloat { laidY(v) + v.layoutOffset.y }

    #if os(iOS)
    /// The slider is measured before the commit's frame shows: when the tap's batch
    /// has applied, with no turn of the run loop between, the row is laid out at the
    /// slider's own height and the sibling, laid out below it, still shows where it
    /// was. Measured a turn later, the sibling showed one frame where the default
    /// height put it, then the start, then moved (#134). AppKit's slider is the
    /// default's height, so only UIKit's is measured taller.
    func testASliderInsertedAboveAMovingSiblingIsMeasuredBeforeItsFrame() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the slider is measured in a scene's window") }
        let session = try boot()
        let below = try XCTUnwrap(node(session, "below"))
        let was = laidY(below)
        session.presenter.onPress?(try XCTUnwrap(node(session, "toggle")).id)
        let row = try XCTUnwrap(node(session, "row")), box = try XCTUnwrap(node(session, "slider"))
        let slider = try XCTUnwrap(session.presenter.controls.controls[box.id])
        XCTAssertGreaterThan(slider.frame.height, 16, "the platform's slider, taller than the default it is laid out at first")
        XCTAssertEqual(box.bounds.height, slider.frame.height, accuracy: 0.5, "the input is laid out at the slider's own height")
        XCTAssertEqual(row.frame.maxY, laidY(below), accuracy: 0.5, "the sibling is laid out below the measured row")
        XCTAssertEqual(shownY(below), was, accuracy: 0.5, "and still shows where it was")
        waitFor("the sibling slides to its place") { below.layoutOffset == .zero }
    }

    /// The insert lands as a slice the owner published, ahead of an input batch the
    /// runtime made after it that moves the slider. The slider measured as the slice
    /// applies is laid out after the input batch has applied too: laid out between
    /// them, its layout came from the runtime after the input, and the input batch's
    /// older frames then put the slider back at the default height for good.
    func testASliderMeasuredInALandedSliceIsLaidOutAfterTheBatchMadeAfterTheSlice() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the slider is measured in a scene's window") }
        try measuredAfterTheSlice { session, nudge in session.apply(session.runtime.press(nudge, now: session.now())) }
    }

    /// The same with the input batch published behind the slice, both landing in one
    /// `landFill` that no other batch's apply holds.
    func testASliderMeasuredInALandedSliceIsLaidOutAfterTheSliceBehindIt() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the slider is measured in a scene's window") }
        try measuredAfterTheSlice { session, nudge in
            session.publishTickForTests(session.runtime.press(nudge, now: session.now()))
            session.landFill()
        }
    }

    /// The same with the input batch still being made on the owner as the slice
    /// lands: a notice the owner makes only once a size is reported, so the runtime
    /// makes the size's layout after it. Nothing else in the slice's apply waits on
    /// the owner (the nudge button stands above the insert, so its face is not asked).
    func testASliderMeasuredInALandedSliceIsLaidOutAfterTheBatchStillOnTheOwner() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the slider is measured in a scene's window") }
        try measuredAfterTheSlice { session, nudge in
            let reported = DispatchSemaphore(value: 0), madeAfter = DispatchSemaphore(value: 0)
            let report = session.presenter.onIntrinsic, now = session.now()
            session.presenter.onIntrinsic = { reported.signal(); report?($0) }
            session.tellSoon([{
                if reported.wait(timeout: .now() + 5) == .success { madeAfter.signal() }
                return $0.press(nudge, now: now)
            }])
            session.landFill()
            XCTAssertEqual(madeAfter.wait(timeout: .now()), .success, "the owner made the nudge once a size was reported, not before")
        }
    }

    /// The slice lands with a cancelled reply behind it, which the landing skips: the
    /// size is reported as the landing ends, before the frame shows.
    func testASliderMeasuredInALandedSliceIsLaidOutWhenTheBatchBehindItIsSkipped() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the slider is measured in a scene's window") }
        try measuredAfterTheSlice { session, _ in
            session.publishCancelledReplyForTests()
            session.landFill()
        }
    }

    /// Publishes the insert as a tick's slice, lands it with `land` (given the
    /// nudge's id), and checks the slider is laid out at its measured height then
    /// and once the owner's work and the sizes' reports have all run.
    private func measuredAfterTheSlice(_ land: (ExactSession, UInt32) -> Void) throws {
        let session = try boot()
        session.publishTickForTests(session.runtime.press(try XCTUnwrap(node(session, "toggle")).id, now: session.now()))
        let nudge = try XCTUnwrap(node(session, "nudge")).id
        XCTAssertNil(node(session, "row"), "the insert has not landed")
        land(session, nudge)
        let row = try XCTUnwrap(node(session, "row")), box = try XCTUnwrap(node(session, "slider"))
        let below = try XCTUnwrap(node(session, "below"))
        let slider = try XCTUnwrap(session.presenter.controls.controls[box.id])
        XCTAssertGreaterThan(slider.frame.height, 16, "the platform's slider, taller than the default")
        XCTAssertEqual(box.bounds.height, slider.frame.height, accuracy: 0.5, "the input is laid out at the slider's own height")
        XCTAssertEqual(row.frame.maxY, laidY(below), accuracy: 0.5, "the sibling is laid out below the measured row")
        waitFor("the owner's batches land and the sizes are reported") { !session.ownerInFlight && !session.presenter.controls.owesSizes }
        waitForMainQueue("the landings and reports queued for the next turn")
        XCTAssertEqual(box.bounds.height, slider.frame.height, accuracy: 0.5, "and stays so")
    }
    #endif

    /// The sibling slides over the inserted row as CSS paints them, by its later place
    /// and its `z-index`, and takes touches first; on iOS the window's pixels at the
    /// slide's start show nothing of the row where the sibling stands over it (#135).
    func testTheSiblingPaintsOverTheInsertedRowAsItSlides() throws {
        #if os(iOS)
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the window's pixels are a scene window's") }
        #endif
        let session = try boot()
        session.presenter.onPress?(try XCTUnwrap(node(session, "toggle")).id)
        let row = try XCTUnwrap(node(session, "row")), below = try XCTUnwrap(node(session, "below"))
        let slider = try XCTUnwrap(session.presenter.controls.controls[try XCTUnwrap(node(session, "slider")).id])
        XCTAssertTrue(row.superview === below.superview, "siblings in one container")
        XCTAssertGreaterThan(below.paintZPosition, row.paintZPosition)
        XCTAssertTrue(slider.isDescendant(of: row), "the slider paints in its row's turn")
        let siblings = NodeView.hitOrder(try XCTUnwrap(row.superview).subviews)
        XCTAssertLessThan(try XCTUnwrap(siblings.firstIndex { $0 === below }), try XCTUnwrap(siblings.firstIndex { $0 === row }), "touches reach the sibling first")
        XCTAssertLessThan(shownY(below), row.frame.maxY - 1, "the sibling starts over the row")
        #if os(iOS)
        // Where the sibling shows over the row (both with their transforms), the window's
        // pixels are the same with the row hidden: nothing of the row is drawn over it.
        let window = try XCTUnwrap(self.window)
        let over = row.convert(row.bounds, to: window).intersection(below.convert(below.bounds, to: window))
        let x = over.minX.rounded(.up), y = over.minY.rounded(.up)
        let covered = CGRect(x: x, y: y, width: over.maxX.rounded(.down) - x, height: over.maxY.rounded(.down) - y)
        XCTAssertGreaterThan(covered.height, 20, "the sibling starts over the row's label")
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        format.preferredRange = .standard
        let picture = {
            CapturePixels.bytes(UIGraphicsImageRenderer(bounds: covered, format: format).image { _ in
                window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
            }.cgImage)
        }
        let shown = picture()
        XCTAssertTrue(CapturePixels.hasPaint(shown))
        row.isHidden = true
        defer { row.isHidden = false }
        XCTAssertEqual(shown, picture(), "the row draws nothing over the sibling")
        #endif
    }
}
