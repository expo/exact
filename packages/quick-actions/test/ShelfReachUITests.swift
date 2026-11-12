// LLP 1133.023 stage 2's simulator drives of Shelf, as a person reaches it
// from outside the app: the Home Screen quick action, Spotlight, and the
// Shortcuts app's Log Pages. Each test leaves a screenshot and the
// accessibility hierarchy of each step in REACH_OUT. `apple-ui.mjs` builds
// and runs it (no Xcode project).
import XCTest

final class ShelfReachUITests: XCTestCase {
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

    private func element(_ app: XCUIApplication, _ id: String) -> XCUIElement { app.descendants(matching: .any)[id] }

    /// Shelf's icon on the Home Screen, long pressed: Add Book opens the add sheet.
    private func quickAction(cold: Bool) {
        if cold { shelf.terminate() } else {
            shelf.launch()
            XCTAssertTrue(element(shelf, "route-library").waitForExistence(timeout: 30))
        }
        XCUIDevice.shared.press(.home)
        XCUIDevice.shared.press(.home)
        let icon = springboard.icons["Shelf"]
        XCTAssertTrue(icon.waitForExistence(timeout: 20), "Shelf's icon")
        for _ in 0..<5 where !icon.isHittable { springboard.swipeLeft() }
        save(springboard, cold ? "quick-cold-home" : "quick-warm-home")
        XCTAssertTrue(icon.isHittable, "Shelf's icon is on the visible Home Screen page")
        icon.press(forDuration: 1.5)
        let add = springboard.buttons["Add Book"]
        XCTAssertTrue(add.waitForExistence(timeout: 10), "the manifest's Add Book in the icon's menu")
        save(springboard, cold ? "quick-cold" : "quick-warm")
        add.tap()
        XCTAssertTrue(element(shelf, "route-add").waitForExistence(timeout: 30), "the add sheet is up")
        save(shelf, cold ? "quick-cold" : "quick-warm")
    }

    func test1QuickActionCold() { quickAction(cold: true) }
    func test2QuickActionWarm() { quickAction(cold: false) }

    /// Spotlight's own search field finds a book Shelf indexed, and the result opens it.
    func test3SpotlightOpensTheBook() {
        // A launch outside the agent sets the index (Shelf's `index` task).
        shelf.launch()
        XCTAssertTrue(element(shelf, "route-library").waitForExistence(timeout: 30))
        XCUIDevice.shared.press(.home)
        springboard.swipeDown()
        let spotlight = XCUIApplication(bundleIdentifier: "com.apple.Spotlight")
        let field = spotlight.textFields["SpotlightSearchField"]
        XCTAssertTrue(field.waitForExistence(timeout: 10), "Spotlight's field")
        field.typeText("Piranesi")
        let result = spotlight.cells.matching(NSPredicate(format: "label == %@", "Piranesi, Susanna Clarke")).firstMatch
        XCTAssertTrue(result.waitForExistence(timeout: 30), "Shelf's book among Spotlight's results")
        // XCTest's automatic scroll can leave the row behind Spotlight's floating search field.
        for _ in 0..<5 where result.frame.maxY > field.frame.minY {
            let start = field.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0)).withOffset(CGVector(dx: 0, dy: -70))
            start.press(forDuration: 0.05, thenDragTo: start.withOffset(CGVector(dx: 0, dy: -260)))
        }
        XCTAssertLessThan(result.frame.maxY, field.frame.minY, "the book is above the search field")
        save(spotlight, "spotlight")
        result.tap()
        XCTAssertTrue(element(shelf, "route-book").waitForExistence(timeout: 30), "the book's screen is up")
        XCTAssertTrue(shelf.navigationBars["Piranesi"].exists, "the opened book is Piranesi")
        save(shelf, "spotlight")
    }

    /// The Shortcuts app lists Shelf's Log Pages among the actions it can add: the system read the metadata from a
    /// bundle `build.mjs` assembled and found the intent's type in the executable (the stage's kill line).
    func test4ShortcutsListsLogPages() {
        let shortcuts = XCUIApplication(bundleIdentifier: "com.apple.shortcuts")
        shortcuts.launch()
        save(shortcuts, "shortcuts")
        for label in ["Continue", "Not Now", "OK"] where shortcuts.buttons[label].exists { shortcuts.buttons[label].tap() }
        if shortcuts.buttons["Close"].exists { shortcuts.buttons["Close"].tap() }
        let add = shortcuts.buttons["main.button.newshortcut"]
        if !add.exists, shortcuts.navigationBars.buttons["BackButton"].exists {
            shortcuts.navigationBars.buttons["BackButton"].tap()
        }
        XCTAssertTrue(add.waitForExistence(timeout: 10), "New Shortcut is available")
        add.tap()
        let editor = shortcuts.buttons["Editor"]
        XCTAssertTrue(editor.waitForExistence(timeout: 10), "Edit opens the shortcut's actions")
        editor.tap()
        save(shortcuts, "shortcuts")
        let search = shortcuts.searchFields.firstMatch
        XCTAssertTrue(search.waitForExistence(timeout: 10), "the actions' search field")
        // Xcode 27.2 can miss Shortcuts' animation-completion notification. Its private
        // XCTest switch skips that wait; the field and action still have to appear.
        shortcuts.setValue(false, forKey: "idleAnimationWaitEnabled")
        defer { shortcuts.setValue(true, forKey: "idleAnimationWaitEnabled") }
        search.tap()
        search.typeText("Log Pages")
        let found = shortcuts.staticTexts["Log Pages"].waitForExistence(timeout: 20)
        save(shortcuts, "shortcuts")
        XCTAssertTrue(found, "Shelf's Log Pages among the actions")
    }
}
