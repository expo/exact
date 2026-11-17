#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// VoiceOver's swipe up and down on a drawn `slider` and `spinbutton`
/// (`range-adjust.contract`, compiled and run): each is adjustable, and an
/// increment or decrement is the keydown and keyup WebKit sends an ARIA range,
/// which the author's `key` handler answers by changing `aria-valuenow`. A
/// native range's swipe is that key first, and its handler may take it.
/// UIKit, so a simulator runs it: bun host/apple/build.mjs --test --ios
final class RangeAdjustIOSTests: XCTestCase {
    private var session: ExactSession?
    private var window: UIWindow?

    override func tearDown() {
        session?.destroy(); session = nil
        window?.isHidden = true; window = nil
        super.tearDown()
    }

    private func booted() throws -> ExactSession {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_RANGE_ADJUST_PLAN"])
        let session = ExactApp.shared.makeSession(label: "range-adjust")
        self.session = session
        let host = UIViewController(), view = ExactView(session: session)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        return session
    }

    private func node(_ name: String) throws -> NodeView {
        try XCTUnwrap(session?.presenter.views.values.first { $0.props["testId"] == name }, name)
    }

    func testIncrementAndDecrementRunTheAuthorsKeyHandler() throws {
        _ = try booted()
        let level = try node("level"), floor = try node("floor")
        XCTAssertTrue(level.accessibilityTraits.contains(.adjustable))
        XCTAssertTrue(floor.accessibilityTraits.contains(.adjustable))
        XCTAssertEqual(level.accessibilityValue, "50 percent")
        level.accessibilityIncrement()
        waitFor("the slider's ArrowRight") { level.props["accessibilityValueNow"] == "60" }
        XCTAssertEqual(level.accessibilityValue, "60 percent")
        level.accessibilityDecrement()
        level.accessibilityDecrement()
        waitFor("the slider's two ArrowLefts") { level.props["accessibilityValueNow"] == "40" }
        floor.accessibilityIncrement()
        waitFor("the spin button's ArrowUp") { floor.accessibilityValue == "4" }
        floor.accessibilityDecrement()
        waitFor("the spin button's ArrowDown") { floor.accessibilityValue == "3" }
    }

    /// A native range whose `key` handler takes the arrows with
    /// `preventDefault()` moves 10 a swipe and lands a drag where it is let
    /// go; one with no handler keeps the slider's own twentieth of the span
    /// (LLP 1069.001, amended 2026-11-16).
    func testANativeRangesSwipeRunsItsKeyHandler() throws {
        let session = try booted()
        let seek = try XCTUnwrap(session.presenter.controls.controls[try node("seek").id] as? RangeSlider)
        let plain = try XCTUnwrap(session.presenter.controls.controls[try node("plain").id] as? RangeSlider)
        let shown = try node("at")
        seek.accessibilityIncrement()
        waitFor("the handler's ten") { seek.value == 50 && shown.props["text"] == "50" }
        seek.accessibilityDecrement()
        seek.accessibilityDecrement()
        waitFor("two back") { seek.value == 30 && shown.props["text"] == "30" }
        XCTAssertEqual(seek.accessibilityValue, "30")
        seek.value = 123
        session.presenter.controls.rangeMoved(seek)
        waitFor("the drag lands off the handler's tens") { shown.props["text"] == "123" }
        plain.accessibilityIncrement()
        waitFor("the slider's own step") { plain.value == 52 }
    }

    /// The agent's `type <range> key ArrowRight`, an authored test's step, is
    /// the slider's own accessibility increment or decrement: the handler's
    /// ten where one takes the key, the slider's step where none does, and
    /// under `rtl` ArrowLeft is the increment.
    func testTheAgentsArrowAtANativeRangeIsItsAccessibilityStep() throws {
        let session = try booted()
        let agent = session.agentInstance
        let seek = try node("seek"), plain = try node("plain"), mirrored = try node("mirrored")
        let slider = try XCTUnwrap(session.presenter.controls.controls[seek.id] as? RangeSlider)
        var swipes: [Double] = []
        let adjust = slider.adjust
        slider.adjust = { slider, direction in swipes.append(direction); adjust?(slider, direction) }
        var reply = agent.type(["id": Int(seek.id), "key": "ArrowRight"])
        XCTAssertNil(reply["error"])
        XCTAssertEqual(reply["value"] as? String, "50", "the handler's ten")
        reply = agent.type(["id": Int(seek.id), "key": "ArrowLeft"])
        XCTAssertEqual(reply["value"] as? String, "40")
        XCTAssertEqual(swipes, [1, -1], "through the slider's accessibility increment and decrement")
        XCTAssertEqual(try node("at").props["text"], "40")
        reply = agent.type(["id": Int(plain.id), "key": "ArrowRight"])
        XCTAssertEqual(reply["value"] as? String, "52", "the slider's own step: nothing prevented it")
        reply = agent.type(["id": Int(mirrored.id), "key": "ArrowLeft"])
        XCTAssertEqual(reply["value"] as? String, "52", "the increment of a right-to-left range")
        XCTAssertEqual(try node("mirrored-key").props["text"], "ArrowLeft")
    }

    /// The agent's `down` and `up` are the two halves of its press at a range:
    /// the keydown with the slider's step, then the keyup when it is asked
    /// for. A held key's release by its token and its repeats step the same way.
    func testTheAgentsArrowHalvesAtARangeAreItsPress() throws {
        let session = try booted()
        let agent = session.agentInstance
        let held = try node("held"), heard = try node("held-heard")
        let send = { (request: [String: Any]) -> [String: Any] in agent.releaseCanvasKey(request) ?? agent.type(request) }
        var reply = send(["id": Int(held.id), "key": "ArrowRight"])
        XCTAssertEqual(reply["value"] as? String, "45")
        XCTAssertEqual(heard.props["text"], "45 0 1 45", "the press: one step, then its keyup")

        reply = send(["id": Int(held.id), "key": "ArrowRight", "phase": "down"])
        XCTAssertNil(reply["error"])
        XCTAssertEqual(reply["value"] as? String, "50", "the down half steps as the press does")
        XCTAssertEqual(heard.props["text"], "50 0 1 45", "no keyup before the up half")
        reply = send(["id": Int(held.id), "key": "ArrowRight", "phase": "up"])
        XCTAssertNil(reply["error"])
        XCTAssertEqual(heard.props["text"], "50 0 2 50", "the up half is the keyup alone")

        // `for <ms>`: a down that owns its release, the repeats of the hold, the release by token.
        reply = send(["id": Int(held.id), "key": "ArrowLeft", "phase": "down", "releaseKey": "hold"])
        XCTAssertEqual(reply["value"] as? String, "45")
        reply = send(["id": Int(held.id), "key": "ArrowLeft", "phase": "down", "repeat": true])
        XCTAssertEqual(reply["value"] as? String, "40", "a repeat steps again")
        XCTAssertEqual(heard.props["text"], "40 1 2 50", "as a repeated keydown, with no keyup yet")
        reply = send(["phase": "up", "releaseKey": "hold"])
        XCTAssertNil(reply["error"])
        XCTAssertEqual(heard.props["text"], "40 1 3 40")
    }

    /// A range `appearance="none"` shows no `UISlider`: the agent's arrow at
    /// it is a keyboard's, and the slider the host keeps off the window, which
    /// holds none of the range's value or bounds, moves nothing.
    func testTheAgentsArrowAtARangeWithNoSliderMovesNothing() throws {
        let session = try booted()
        let bare = try node("bare")
        XCTAssertNil((session.presenter.controls.controls[bare.id] as? RangeSlider)?.window)
        for request in [["key": "ArrowRight"], ["key": "ArrowRight", "phase": "down"], ["key": "ArrowLeft", "phase": "down", "repeat": true]] as [[String: Any]] {
            let reply = session.agentInstance.type(request.merging(["id": Int(bare.id)]) { a, _ in a })
            XCTAssertNil(reply["error"], "\(request)")
            XCTAssertNotEqual(reply["delivery"] as? String, "host-activation", "\(request)")
        }
        XCTAssertEqual(try node("bare-at").props["text"], "40")
    }

    /// A range its keydown's handler disables does not step after the key.
    func testARangeItsKeyHandlerDisablesDoesNotStep() throws {
        let session = try booted()
        let late = try XCTUnwrap(session.presenter.controls.controls[try node("late").id] as? RangeSlider)
        late.accessibilityIncrement()
        waitFor("the handler's disabling") { !late.isEnabled }
        XCTAssertEqual(late.value, 40)
    }

    /// A horizontal slider's increment follows its own computed `direction`:
    /// ArrowLeft for one that inherits `rtl` in a left-to-right document, and
    /// ArrowRight for one that says `ltr` under it.
    func testASlidersIncrementFollowsItsOwnDirection() throws {
        _ = try booted()
        let back = try node("back"), ahead = try node("ahead")
        back.accessibilityIncrement()
        waitFor("the inherited rtl slider's ArrowLeft") { back.props["accessibilityValueNow"] == "60" }
        ahead.accessibilityIncrement()
        waitFor("the ltr slider's ArrowRight") { ahead.props["accessibilityValueNow"] == "60" }
    }
}
#endif
