// LLP 1163.000 §11 item 5 on a simulator: Shelf's App Icon setting, as a person uses it, with
// iOS's own alert. Each step leaves a screenshot and the accessibility hierarchy in REACH_OUT.
// Run by packages/quick-actions/test/apple-ui.mjs with Shelf installed:
//   EXACT_UI_TEST_SOURCE=packages/app-icon/test/ShelfAppIconUITests.swift \
//     bun packages/quick-actions/test/apple-ui.mjs <udid> <out dir>
import XCTest

final class ShelfAppIconUITests: XCTestCase {
    private let out = ProcessInfo.processInfo.environment["REACH_OUT"] ?? NSTemporaryDirectory()
    private let shelf = XCUIApplication(bundleIdentifier: "com.exact.shelf")
    private let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
    private var step = 0

    override func setUp() { continueAfterFailure = false }

    private func save(_ app: XCUIApplication, _ name: String) {
        step += 1
        let base = "\(out)/\(name)-\(step)"
        do {
            try XCUIScreen.main.screenshot().pngRepresentation.write(to: URL(fileURLWithPath: base + ".png"))
            try app.debugDescription.write(toFile: base + ".txt", atomically: true, encoding: .utf8)
        } catch { XCTFail("Could not save \(name): \(error)") }
    }

    /// A control by its testId; a list row's cell carries the identifier too.
    private func element(_ id: String) -> XCUIElement { shelf.buttons[id] }

    /// iOS's alert, if one comes within `timeout`: saved, then dismissed with its OK.
    @discardableResult private func alert(_ name: String, timeout: TimeInterval) -> Bool {
        let alert = springboard.alerts.firstMatch
        guard alert.waitForExistence(timeout: timeout) else { return false }
        XCTAssertTrue(alert.label.contains("changed the icon") || alert.staticTexts.allElementsBoundByIndex.contains { $0.label.contains("changed the icon") }, alert.debugDescription)
        save(springboard, name)
        alert.buttons["OK"].tap()
        return true
    }

    /// Shelf launched on its Stats tab, where the App Icon section is.
    private func openStats(_ name: String) {
        shelf.terminate()
        shelf.launch()
        // A change made before this launch may still have its alert up.
        alert("leftover", timeout: 2)
        let tab = element("tab-stats")
        XCTAssertTrue(tab.waitForExistence(timeout: 30))
        tab.tap()
        let primary = element("icon-primary")
        XCTAssertTrue(primary.waitForExistence(timeout: 10), "the App Icon section")
        for _ in 0..<5 where !element("icon-paper").isHittable { shelf.swipeUp() }
        save(shelf, name)
    }

    private func home(_ name: String) {
        XCUIDevice.shared.press(.home)
        let icon = springboard.icons["Shelf"]
        XCTAssertTrue(icon.waitForExistence(timeout: 20), "Shelf's icon")
        save(springboard, name)
    }

    func test1ChooseKeepAndRestore() {
        openStats("start")
        // From the primary: Paper, with iOS's alert.
        if element("icon-paper").isSelected {
            element("icon-primary").tap()
            alert("reset", timeout: 10)
        }
        element("icon-paper").tap()
        XCTAssertTrue(alert("paper-alert", timeout: 10), "iOS tells the person of the change")
        XCTAssertTrue(element("icon-paper").waitForSelected(), "Paper is checked")
        save(shelf, "paper-chosen")
        home("paper-home")

        // Kept across a relaunch; choosing it again asks iOS nothing.
        openStats("relaunched")
        XCTAssertTrue(element("icon-paper").isSelected, "Paper is still the icon after a relaunch")
        element("icon-paper").tap()
        XCTAssertFalse(alert("unexpected", timeout: 3), "no alert for the icon already shown")

        // The primary restored.
        element("icon-primary").tap()
        XCTAssertTrue(alert("primary-alert", timeout: 10))
        XCTAssertTrue(element("icon-primary").waitForSelected(), "Shelf's own icon is checked")
        save(shelf, "primary-chosen")
        home("primary-home")
    }
}

private extension XCUIElement {
    func waitForSelected(timeout: TimeInterval = 10) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline { if isSelected { return true }; RunLoop.current.run(until: Date().addingTimeInterval(0.2)) }
        return isSelected
    }
}
