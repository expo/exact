#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// LLP 1069.010: a quit the app holds fades its window and takes it out of
/// full screen first, `setWindowOpacity(opacity)` and `exitFullscreen()`.
@MainActor
final class WindowCommandsMacTests: XCTestCase {
    private var window: NSWindow?
    override func tearDown() { window?.close(); window = nil }

    private func shown() -> NSWindow {
        let window = NSWindow(contentRect: NSRect(x: 200, y: 200, width: 320, height: 240), styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.orderFrontRegardless()
        self.window = window
        return window
    }

    func testAnOpacityFadesTheWindowAndARefusalLeavesIt() {
        let window = shown()
        var said: [String] = []
        XCTAssertTrue(ExactWindowCommands.perform("setWindowOpacity", [NSNumber(value: 0)], on: window) { said.append($0) })
        XCTAssertEqual(window.alphaValue, 0)
        XCTAssertTrue(window.isVisible, "an unseen window is still shown, and still hears keys")
        XCTAssertTrue(ExactWindowCommands.perform("setWindowOpacity", [NSNumber(value: 0.5)], on: window) { said.append($0) })
        XCTAssertEqual(window.alphaValue, 0.5, accuracy: 0.001)
        XCTAssertTrue(ExactWindowCommands.perform("setWindowOpacity", [NSNumber(value: 3)], on: window) { said.append($0) })
        XCTAssertEqual(window.alphaValue, 1, "clamped to 0–1")
        XCTAssertTrue(ExactWindowCommands.perform("setWindowOpacity", ["0"], on: window) { said.append($0) })
        XCTAssertEqual(window.alphaValue, 1)
        XCTAssertEqual(said, ["setWindowOpacity refused: the opacity is a number from 0 to 1"])
        XCTAssertFalse(ExactWindowCommands.perform("close", [], on: window) { said.append($0) }, "not a window command of these")
    }

    func testBoundsPlaceAnEmbeddingAppsWindow() throws {
        let window = NSWindow(contentRect: NSRect(x: 100, y: 120, width: 480, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        let top = try XCTUnwrap(NSScreen.screens.first).frame.maxY
        XCTAssertTrue(ExactWindowCommands.perform("setBounds", [[96, 140, 510, 302]], on: window) { XCTFail($0) })
        waitForMainQueue()
        XCTAssertEqual(window.frame, NSRect(x: 96, y: top - 442, width: 510, height: 302))
    }

    func testFloatingAndClickThroughChangeIndependently() {
        let window = shown()
        let spaces = window.collectionBehavior
        func set(_ command: String, _ flag: Bool) {
            XCTAssertTrue(ExactWindowCommands.perform(command, [flag], on: window) { XCTFail($0) })
        }
        set("setWindowAlwaysOnTop", true)
        XCTAssertEqual(window.level, AgentLink.windowsBehind ? .agentBehind : .floating)
        set("setWindowIgnoreMouseEvents", true)
        set("setWindowAlwaysOnTop", false)
        XCTAssertEqual(window.collectionBehavior, spaces)
        XCTAssertTrue(window.ignoresMouseEvents)
        XCTAssertEqual(window.level, AgentLink.windowsBehind ? .agentBehind : .normal)
        set("setWindowIgnoreMouseEvents", false)
        XCTAssertFalse(window.ignoresMouseEvents)
    }

    func testTimedOpacityTakesMillisecondsAndCanBeReplaced() {
        let window = shown()
        func fade(_ a: CGFloat, _ ms: Double) {
            XCTAssertTrue(ExactWindowCommands.perform("setWindowOpacity", [a, ms], on: window) { XCTFail($0) })
        }
        fade(0.2, 500)
        if ExactEnv.agentFreezes || NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            XCTAssertEqual(window.alphaValue, 0.2, accuracy: 0.001)
        } else {
            waitFor("opacity animates") { window.alphaValue < 1 }
            XCTAssertGreaterThan(window.alphaValue, 0.2)
        }
        fade(0.8, 0)
        XCTAssertEqual(window.alphaValue, 0.8, accuracy: 0.001)
    }

    func testAspectRatioConstrainsContentAndZeroClearsIt() {
        let window = shown()
        XCTAssertTrue(ExactWindowCommands.perform("setAspectRatio", [16.0 / 9.0], on: window) { XCTFail($0) })
        let ratio = window.contentAspectRatio
        XCTAssertEqual(ratio.width / ratio.height, 16 / 9, accuracy: 0.00001)
        var refusal = ""
        XCTAssertTrue(ExactWindowCommands.perform("setAspectRatio", [-1], on: window) { refusal = $0 })
        XCTAssertTrue(refusal.contains("setAspectRatio refused"))
        XCTAssertEqual(window.contentAspectRatio, ratio)
        XCTAssertTrue(ExactWindowCommands.perform("setAspectRatio", [0], on: window) { XCTFail($0) })
        XCTAssertEqual(window.contentResizeIncrements, NSSize(width: 1, height: 1))
        XCTAssertEqual(window.contentAspectRatio, .zero)
    }

    /// LLP 1133.025 D4: `hideWindow()` puts away the window of an app that
    /// keeps running, as the person's close does (`PageFactsMacTests` holds
    /// what the page then reads), and showing it again is a restore. Where
    /// the app would end without its window, it is refused and the window
    /// stays. Which apps keep their window is the manifest's and the status
    /// item's.
    func testHidingTheWindowKeepsItOnlyWhereTheAppKeepsRunning() {
        let window = shown()
        var said: [String] = []
        ExactWindowCommands.hide(window, keeps: false) { said.append($0) }
        XCTAssertTrue(window.isVisible, "refused: the window stays")
        XCTAssertEqual(said, ["hideWindow refused: the app ends without its window; it keeps running with host.macos.window.keepRunning or a status item placed"])

        said = []
        ExactWindowCommands.hide(window, keeps: true) { said.append($0) }
        XCTAssertEqual(said, [])
        XCTAssertFalse(window.isVisible || window.isMiniaturized, "ordered out: neither on screen nor in the Dock")
        XCTAssertTrue(PageFacts.persisted(window), "put away as a close under keepRunning puts it: pagehide, persisted")
        ExactWindowCommands.hide(window, keeps: true) { said.append($0) }
        XCTAssertEqual(said, [], "a window already away is left as it is")
        window.makeKeyAndOrderFront(nil) // bare `focus()`'s `front()`
        waitFor("the window shown again is counted a restore") { PageFacts.restores(window) > 0 }
        XCTAssertEqual(PageFacts.restores(window), 1, "shown again: pageshow, persisted")
        XCTAssertFalse(PageFacts.persisted(window))
        PageFacts.closed(window)

        let app: [String: Any] = ["ExactWindow": ["keepRunning": true]]
        let documents: [String: Any] = ["ExactWindow": ["keepRunning": true], "ExactLaunchMode": "navigate-new"]
        XCTAssertTrue(ExactDocuments.keepsWindow(main: true, placesStatusItems: false, app), "keepRunning")
        XCTAssertFalse(ExactDocuments.keepsWindow(main: false, placesStatusItems: false, app), "a window other than main closes")
        XCTAssertTrue(ExactDocuments.keepsWindow(main: true, placesStatusItems: true, [:]), "a placed status item reaches the app")
        XCTAssertFalse(ExactDocuments.keepsWindow(main: true, placesStatusItems: false, [:]), "the default: the app ends with its window")
        XCTAssertFalse(ExactDocuments.keepsWindow(main: true, placesStatusItems: true, documents), "a document's window closes as a document")
    }

    func testLeavingFullScreenLeavesAWindowThatIsNotFullScreenAsItIs() {
        let window = shown()
        let frame = window.frame
        XCTAssertTrue(ExactWindowCommands.perform("exitFullscreen", [], on: window) { XCTFail($0) })
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.3))
        XCTAssertFalse(window.styleMask.contains(.fullScreen))
        XCTAssertEqual(window.frame, frame)
    }

    func testAFullScreenWindowLeavesItsSpace() throws {
        let window = shown()
        window.collectionBehavior = [.fullScreenPrimary]
        // A toggle while AppKit animates into the space is ignored, so the
        // command waits for the space to be entered.
        let entered = expectation(forNotification: NSWindow.didEnterFullScreenNotification, object: window)
        window.toggleFullScreen(nil)
        guard XCTWaiter().wait(for: [entered], timeout: 10) == .completed else { throw XCTSkip("this session gives no window a full-screen space") }
        let left = expectation(forNotification: NSWindow.didExitFullScreenNotification, object: window)
        XCTAssertTrue(ExactWindowCommands.perform("exitFullscreen", [], on: window) { XCTFail($0) })
        wait(for: [left], timeout: 10)
        XCTAssertFalse(window.styleMask.contains(.fullScreen))
    }
}
#endif
