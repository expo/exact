import XCTest
import UIKit

/// Runs on an open Duo, where no test skips, and on a compact window (the folded Duo or an iPhone), where
/// the gallery's list and the section chosen from it are one stack and the tests that need the list beside
/// the section skip.
final class GalleryUITests: XCTestCase {
    override func setUp() {
        super.setUp()
        continueAfterFailure = false
    }

    func testSidebarSelectionFinishesWithoutAnotherTouch() throws {
        let app = XCUIApplication(bundleIdentifier: "com.exact.uigallery")
        launchShowingTheList(app)
        let sidebar = app.collectionViews["gallery-sidebar"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 20))
        try requireSidebarBesideThePage(app)
        for section in ["css", "profiles", "textcolors"] {
            open(section, in: app)
            let before = try sidebarPixels(app, sidebar)
            sidebar.staticTexts["Text"].firstMatch.tap()
            let after = try sidebarPixels(app, sidebar)
            XCTAssertTrue(before == after, "The list must finish drawing the chosen row without another touch")
        }
    }

    func testHeaderCollapsesAndTheRangeBarFollowsIt() throws {
        let app = XCUIApplication(bundleIdentifier: "com.exact.uigallery")
        launchShowingTheList(app)
        let sidebar = app.collectionViews["gallery-sidebar"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 20))
        open("css", in: app)
        let bar = app.navigationBars["CSS color spaces"]
        let range = app.segmentedControls["range-css"]
        XCTAssertTrue(range.waitForExistence(timeout: 10))
        XCTAssertEqual(app.staticTexts["range-css-title"].label, "Dynamic range limit")
        XCTAssertFalse(app.staticTexts["range-css-note"].exists)
        let scroll = app.otherElements["gallery"].firstMatch
        var readings: [[String: Any]] = []
        func record(_ step: String) {
            let rangeTitle = app.staticTexts["range-css-title"], title = bar.staticTexts["CSS color spaces"].firstMatch
            readings.append(["step": step, "headerBottom": bar.frame.maxY, "barShown": title.exists && title.isHittable,
                             "barTop": app.otherElements["range-bar-css"].firstMatch.frame.minY,
                             "rangeTop": range.frame.minY,
                             "titleControlGap": range.frame.minX - rangeTitle.frame.maxX,
                             "titleCenter": rangeTitle.frame.midY, "controlCenter": range.frame.midY,
                             "hittable": range.isHittable])
        }
        record("initial")
        scroll.swipeUp(velocity: .slow)
        record("slow-up")
        scroll.swipeUp(velocity: .fast)
        record("fast-up")
        // The CSS page is short: one fast swipe returns it to the top, where the large title shows again
        scroll.swipeDown(velocity: .fast)
        record("fast-down")
        scroll.swipeUp(velocity: .fast)
        record("fast-up-again")
        try save(readings, as: "header-range.json")
        let tops = try readings.map { try XCTUnwrap($0["rangeTop"] as? CGFloat) }
        let expanded = tops[0], collapsed = tops[2]
        XCTAssertLessThan(collapsed, expanded - 20, "Scrolling collapses the header, and the range bar follows it")
        XCTAssertEqual(tops[3], expanded, accuracy: 1, "Scrolling back expands the header")
        for (reading, top) in zip(readings, tops) {
            XCTAssertTrue(abs(top - expanded) < 1 || abs(top - collapsed) < 1,
                          "A swipe of either velocity leaves the header expanded or collapsed: \(reading)")
        }
        // Read the bar's foot only while it shows its title: a bar UIKit hides keeps its accessibility frame
        for reading in readings where reading["barShown"] as? Bool == true {
            XCTAssertEqual(try XCTUnwrap(reading["barTop"] as? CGFloat),
                           try XCTUnwrap(reading["headerBottom"] as? CGFloat), accuracy: 1,
                           "The range bar starts at the bottom of the header: \(reading)")
        }
        XCTAssertEqual(readings.first?["barShown"] as? Bool, true)
        for reading in readings {
            XCTAssertEqual(reading["hittable"] as? Bool, true)
            XCTAssertEqual(try XCTUnwrap(reading["titleControlGap"] as? CGFloat), 16, accuracy: 0.5,
                           "The title and segmented control use system horizontal spacing")
            XCTAssertEqual(try XCTUnwrap(reading["titleCenter"] as? CGFloat),
                           try XCTUnwrap(reading["controlCenter"] as? CGFloat), accuracy: 0.5,
                           "The label is vertically centered beside the control")
        }
        // Beside the list, once it is scrolled too, the docked range bar ends level with the list's inline bar. The
        // list scrolled to its end shows its last row, which then says whether the list shows.
        if sidebarIsBesideThePage(app) {
            XCTAssertTrue(sidebar.cells["sidebar-css"].isHittable, "The list shows beside the page")
            let rangeBar = app.otherElements["range-bar-css"].firstMatch, sidebarBar = app.navigationBars["UI Gallery"]
            let large = sidebarBar.frame.height, lastRow = sidebar.cells["sidebar-drag"]
            sidebar.swipeUp(velocity: .slow)
            for _ in 0..<2 where !lastRow.isHittable { sidebar.swipeUp(velocity: .slow) }
            let pinned = try settled(rangeBar, sidebarBar), docked = pinned[0], listBar = pinned[1]
            XCTAssertLessThan(listBar.height, large, "The list's title is inline once it is scrolled")
            XCTAssertEqual(docked.minY, listBar.minY, accuracy: 0.5, "The range bar is docked where the list's bar starts")
            XCTAssertEqual(docked.maxY, listBar.maxY, accuracy: 0.5, "The two headers end on one line")
            XCTAssertTrue(lastRow.isHittable, "The list's last row shows once the list is scrolled")
            // UIKit keeps a hidden list hidden at the next launch: show it again however this test ends
            addTeardownBlock { [unowned self] in showSidebar(app) }
            app.buttons["Hide Sidebar"].tap()
            XCTAssertTrue(waitForList(app, shown: false, rows: [lastRow]), "The list is hidden")
            let hidden = try settled(rangeBar)[0]
            XCTAssertEqual(hidden.minY, docked.minY, accuracy: 0.5, "Hiding the list moves nothing in the page's header")
            XCTAssertEqual(hidden.height, docked.height, accuracy: 0.5, "Hiding the list keeps the range bar's height")
            // The scrolled page hides its bar and the list's button with it, so drag the list in from the left edge
            let edge = app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.002, dy: 0.5))
            edge.press(forDuration: 0.1, thenDragTo: edge.withOffset(CGVector(dx: 320, dy: 0)))
            XCTAssertTrue(waitForList(app, shown: true, rows: [lastRow]), "The list shows again")
            let dragged = try settled(rangeBar)[0]
            XCTAssertEqual(dragged.minY, docked.minY, accuracy: 0.5, "Dragging the list in moves nothing in the page's header")
            XCTAssertEqual(dragged.height, docked.height, accuracy: 0.5, "Dragging the list in keeps the range bar's height")
            // Hide it again, as a tap on the page would only dismiss a list laid over it
            app.buttons["Hide Sidebar"].tap()
            XCTAssertTrue(waitForList(app, shown: false, rows: [lastRow]), "The list is hidden again")
        }
        range.buttons["standard"].tap()
        XCTAssertTrue(range.buttons["standard"].isSelected, "The control takes a tap over the scrolled page")
        // A compact window collapses the split to one stack, whose back button returns to the list
        guard app.windows.firstMatch.frame.width < 500 else { return }
        let back = app.buttons["BackButton"].firstMatch
        XCTAssertTrue(back.isHittable, "The scrolled page keeps a back button")
        back.tap()
        XCTAssertTrue(sidebar.cells["sidebar-css"].isHittable)
    }

    /// A section chosen from a list hidden since launch, dragged in over the page, docks its range bar at the list's
    /// inline bar height: as tall as once the list shows beside the page and both are scrolled.
    func testAListHiddenSinceLaunchIsMeasuredOverThePage() throws {
        let app = XCUIApplication(bundleIdentifier: "com.exact.uigallery")
        launchShowingTheList(app)
        let sidebar = app.collectionViews["gallery-sidebar"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 20))
        try requireSidebarBesideThePage(app)
        // UIKit keeps a hidden list hidden at the next launch: show it again however this test ends
        addTeardownBlock { [unowned self] in showSidebar(app) }
        app.buttons["Hide Sidebar"].tap()
        XCTAssertTrue(waitForList(app, shown: false), "The list is hidden")
        app.terminate()
        app.launch()
        let show = app.buttons["Show Sidebar"]
        XCTAssertTrue(show.waitForExistence(timeout: 20) && waitForList(app, shown: false), "The list is hidden since launch")
        let edge = app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.002, dy: 0.5))
        edge.press(forDuration: 0.1, thenDragTo: edge.withOffset(CGVector(dx: 320, dy: 0)))
        XCTAssertTrue(waitForList(app, shown: true), "The list lies over the page")
        sidebar.cells["sidebar-css"].tap()
        let rangeBar = app.otherElements["range-bar-css"].firstMatch, page = app.otherElements["gallery"].firstMatch
        let range = app.segmentedControls["range-css"]
        XCTAssertTrue(rangeBar.waitForExistence(timeout: 10))
        XCTAssertTrue(waitForList(app, shown: false), "Choosing a section puts the list away")
        page.swipeUp(velocity: .fast)
        let overPage = try settled(rangeBar)[0]
        for _ in 0..<3 where !show.isHittable { page.swipeDown(velocity: .fast) }
        show.tap()
        XCTAssertTrue(waitForList(app, shown: true), "The list shows again")
        let shown = try settled(range, sidebar)
        XCTAssertGreaterThanOrEqual(shown[0].minX, shown[1].maxX - 0.5, "The list shows beside the page, which makes room for it")
        let large = try settled(app.navigationBars["UI Gallery"])[0].height
        page.swipeUp(velocity: .fast)
        sidebar.swipeUp(velocity: .slow)
        let pinned = try settled(rangeBar, app.navigationBars["UI Gallery"]), beside = pinned[0], sidebarBar = pinned[1]
        XCTAssertLessThan(sidebarBar.height, large, "The list's title is inline once it is scrolled")
        XCTAssertEqual(beside.minY, sidebarBar.minY, accuracy: 0.5, "The range bar is docked where the list's bar starts")
        XCTAssertEqual(beside.maxY, sidebarBar.maxY, accuracy: 0.5, "The two headers end on one line")
        XCTAssertEqual(overPage.minY, beside.minY, accuracy: 0.5, "The range bar was docked at the top before the list showed beside it")
        XCTAssertEqual(overPage.height, beside.height, accuracy: 0.5, "The range bar was as tall before the list showed beside it")
    }

    func testRangeControlsRemainInteractiveOverScrollingContent() throws {
        let app = XCUIApplication(bundleIdentifier: "com.exact.uigallery")
        launchShowingTheList(app)
        XCTAssertTrue(app.collectionViews["gallery-sidebar"].waitForExistence(timeout: 20))
        for section in ["css", "images", "css"] {
            open(section, in: app)
            let range = app.segmentedControls["range-" + section]
            XCTAssertTrue(range.waitForExistence(timeout: 10))
            range.buttons["standard"].tap()
            let scroll = app.otherElements["gallery"].firstMatch
            let intro = app.staticTexts["intro-" + section]
            for _ in 0..<6 where !intro.isHittable { scroll.swipeDown(velocity: .fast) }
            scroll.swipeUp(velocity: .fast)
            let bar = app.otherElements["range-bar-" + section].firstMatch
            let before = range.frame
            _ = try rangeBarPixels(app, bar, name: section + "-before")
            scroll.swipeUp(velocity: .fast)
            _ = try rangeBarPixels(app, bar, name: section + "-after")
            XCTAssertEqual(range.frame, before, "Scrolling keeps the control in place")
            XCTAssertTrue(range.isHittable)
            range.buttons["constrained"].tap()
            XCTAssertTrue(range.buttons["constrained"].isSelected)
        }
    }

    func testVideoFormatAndPlaybackControls() throws {
        let app = XCUIApplication(bundleIdentifier: "com.exact.uigallery")
        launchShowingTheList(app)
        XCTAssertTrue(app.collectionViews["gallery-sidebar"].waitForExistence(timeout: 20))
        open("media", in: app)
        let formats = app.segmentedControls["video-formats"]
        XCTAssertTrue(formats.waitForExistence(timeout: 10))
        let scroll = app.otherElements["gallery"].firstMatch
        for _ in 0..<4 where !formats.isHittable { scroll.swipeUp() }
        for label in ["PQ", "HLG", "P3 SDR", "SDR"] {
            let format = formats.buttons[label]
            format.tap()
            XCTAssertTrue(format.isSelected)
            let toggle = app.buttons["video-toggle"]
            XCTAssertEqual(toggle.label, "Pause videos")
            toggle.tap()
            XCTAssertEqual(toggle.label, "Play videos")
            toggle.tap()
            XCTAssertEqual(toggle.label, "Pause videos")
            XCTAssertEqual(app.state, .runningForeground)
        }
        let image = try screenImage(app)
        let data = try XCTUnwrap(UIImage(cgImage: image).pngData())
        try data.write(to: output.appendingPathComponent("video-controls.png"))
    }

    /// Every page starts below its range bar, and beside the list.
    func testPageSwitchesKeepContentBelowTheRangeBarAndBesideTheSidebar() throws {
        let app = XCUIApplication(bundleIdentifier: "com.exact.uigallery")
        launchShowingTheList(app)
        let sidebar = app.collectionViews["gallery-sidebar"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 20))
        try requireSidebarBesideThePage(app)
        var readings: [[String: Any]] = []
        for section in ["typography", "css", "profiles", "images", "typography", "textcolors", "media", "css", "images", "profiles", "textcolors", "typography", "media"] {
            open(section, in: app)
            let bar = app.otherElements["range-bar-" + section].firstMatch
            let intro = app.staticTexts["intro-" + section]
            XCTAssertTrue(intro.waitForExistence(timeout: 10))
            readings.append(["section": section, "barBottom": bar.frame.maxY, "contentTop": intro.frame.minY,
                             "sidebarEnd": sidebar.frame.maxX, "introStart": intro.frame.minX,
                             "rangeStart": app.staticTexts["range-\(section)-title"].frame.minX])
            try save(readings, as: "page-switch-insets.json")
        }
        for reading in readings {
            let section = reading["section"] as? String ?? ""
            XCTAssertGreaterThanOrEqual(try XCTUnwrap(reading["contentTop"] as? CGFloat),
                                       try XCTUnwrap(reading["barBottom"] as? CGFloat) + 10, section)
            let sidebarEnd = try XCTUnwrap(reading["sidebarEnd"] as? CGFloat)
            XCTAssertGreaterThanOrEqual(try XCTUnwrap(reading["introStart"] as? CGFloat), sidebarEnd, "The page's text starts beside the sidebar: \(section)")
            XCTAssertGreaterThanOrEqual(try XCTUnwrap(reading["rangeStart"] as? CGFloat), sidebarEnd, "The range controls start beside the sidebar: \(section)")
        }
    }

    /// Chooses `section` from the list, first going back to the list where a compact window shows a section
    /// over it. The list's collection view is never hittable itself, only its cells, so Back's presence says
    /// whether the list is covered.
    private func open(_ section: String, in app: XCUIApplication) {
        let list = app.collectionViews["gallery-sidebar"], back = app.buttons["BackButton"].firstMatch
        if back.exists { back.tap() }
        let cell = list.cells["sidebar-" + section], later = ["hdr", "effects", "media", "menus", "drag"].contains(section)
        for _ in 0..<4 where !cell.isHittable { if later { list.swipeUp() } else { list.swipeDown() } }
        cell.tap()
    }

    /// Launches the gallery with its list showing: UIKit keeps a list hidden at the next launch, so a test does not
    /// rely on the one before it to have shown the list again
    private func launchShowingTheList(_ app: XCUIApplication) {
        app.launch()
        let sidebar = app.collectionViews["gallery-sidebar"], show = app.buttons["Show Sidebar"]
        let rows = [sidebar.cells["sidebar-typography"], sidebar.cells["sidebar-drag"]], deadline = Date().addingTimeInterval(20)
        while Date() < deadline, !rows.contains(where: \.isHittable), !show.isHittable { RunLoop.current.run(until: Date().addingTimeInterval(0.3)) }
        if show.isHittable { show.tap() }
        XCTAssertTrue(waitForList(app, shown: true), "The list shows at launch")
    }

    /// Skips the test unless the sidebar shows beside the page, as on the open Duo
    private func requireSidebarBesideThePage(_ app: XCUIApplication) throws {
        guard sidebarIsBesideThePage(app) else { throw XCTSkip("Needs the open Duo, whose sidebar stays beside the page") }
    }

    /// Shows the list beside the page from the page's bar, at the page's top, after putting away a list that shows,
    /// which may lie over the page: UIKit keeps the list where the page's button put it at the next launch
    private func showSidebar(_ app: XCUIApplication) {
        let show = app.buttons["Show Sidebar"], hide = app.buttons["Hide Sidebar"], scroll = app.otherElements["gallery"].firstMatch
        let sidebar = app.collectionViews["gallery-sidebar"]
        // Let a list sliding in or out finish first
        guard listSettles(app, in: [true, false]) != nil else { return XCTFail("The list keeps sliding in or out") }
        if hide.isHittable {
            hide.tap()
            guard waitForList(app, shown: false) else { return XCTFail("The list is put away") }
        }
        for _ in 0..<3 where !show.isHittable && scroll.exists { scroll.swipeDown(velocity: .fast) }
        show.tap()
        guard wait(for: show, hittable: false), waitForList(app, shown: true) else { return XCTFail("The list shows again") }
        // Check where a section shows that the page makes room for the list; an empty detail, as a test that stops
        // before choosing a section leaves it, only centres its message
        let title = app.staticTexts.matching(NSPredicate(format: "identifier BEGINSWITH 'range-' AND identifier ENDSWITH '-title'")).firstMatch
        if title.waitForExistence(timeout: 5) {
            XCTAssertGreaterThanOrEqual(title.frame.minX, sidebar.frame.maxX - 0.5, "The list shows beside the page, which makes room for it")
        }
    }

    /// Whether the list has finished sliding in (`shown`) or out within 5 seconds (`listSettles`)
    private func waitForList(_ app: XCUIApplication, shown: Bool, rows: [XCUIElement]? = nil) -> Bool {
        listSettles(app, in: [shown], rows: rows) == shown
    }

    /// Whether the list has finished sliding in (true) or out (false) within 5 seconds, to one of `states`, else nil:
    /// shown, it lies within the window and a row of it takes taps (`rows` first, by default its first and last, one
    /// of which shows however it is scrolled); hidden, it is gone or wholly off screen; and either way its frame reads
    /// the same twice, 0.3 seconds apart
    private func listSettles(_ app: XCUIApplication, in states: Set<Bool>, rows: [XCUIElement]? = nil) -> Bool? {
        let sidebar = app.collectionViews["gallery-sidebar"], deadline = Date().addingTimeInterval(5)
        let rows = rows ?? [sidebar.cells["sidebar-typography"], sidebar.cells["sidebar-drag"]]
        var last: (shown: Bool, frame: CGRect)?
        while Date() < deadline {
            let frame = sidebar.exists ? sidebar.frame : .null
            var state: Bool?
            if frame.isNull || frame.maxX <= 0.5 {
                state = false
            } else if states.contains(true), app.windows.firstMatch.frame.insetBy(dx: -0.5, dy: -0.5).contains(frame),
                      rows.contains(where: \.isHittable) || sidebar.cells.allElementsBoundByIndex.contains(where: \.isHittable) {
                state = true
            }
            if let state, states.contains(state) {
                if let last, last.shown == state, last.frame == frame { return state }
                last = (state, frame)
            } else {
                last = nil
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.3))
        }
        return nil
    }

    /// The elements' frames once they all read the same twice, 0.3 seconds apart, within 5 seconds, else a failure
    private func settled(_ elements: XCUIElement..., file: StaticString = #filePath, line: UInt = #line) throws -> [CGRect] {
        let deadline = Date().addingTimeInterval(5)
        var last = elements.map(\.frame)
        while Date() < deadline {
            RunLoop.current.run(until: Date().addingTimeInterval(0.3))
            let frames = elements.map(\.frame)
            if frames == last, !frames.contains(where: \.isEmpty) { return frames }
            last = frames
        }
        return try XCTUnwrap(nil as [CGRect]?, "\(elements) still move or have no frame", file: file, line: line)
    }

    /// Whether `element` is (or is not) hittable within 5 seconds
    private func wait(for element: XCUIElement, hittable: Bool) -> Bool {
        let settled = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isHittable == %@", NSNumber(value: hittable)), object: element)
        return XCTWaiter().wait(for: [settled], timeout: 5) == .completed
    }

    /// Whether the window is wide enough for the sidebar to stay beside the page, as on the open Duo
    private func sidebarIsBesideThePage(_ app: XCUIApplication) -> Bool {
        app.windows.firstMatch.frame.width > 800
    }

    private var output: URL {
        URL(fileURLWithPath: ProcessInfo.processInfo.environment["REACH_OUT"]!)
    }

    private func save(_ value: Any, as name: String) throws {
        let data = try JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: output.appendingPathComponent(name))
    }

    private func rangeBarPixels(_ app: XCUIApplication, _ bar: XCUIElement, name: String) throws -> Data {
        let window = app.windows.firstMatch.frame
        let image = try screenImage(app)
        let scale = CGFloat(image.width) / window.width
        let frame = bar.frame
        let box = CGRect(x: (frame.minX - window.minX + 16) * scale,
                         y: (frame.minY - window.minY) * scale,
                         width: (frame.width - 32) * scale, height: (frame.height - 12) * scale).integral
        let cropped = try XCTUnwrap(image.cropping(to: box))
        try XCTUnwrap(UIImage(cgImage: cropped).pngData()).write(to: output.appendingPathComponent("range-body-" + name + ".png"))
        return try pixels(cropped)
    }

    private func sidebarPixels(_ app: XCUIApplication, _ sidebar: XCUIElement) throws -> Data {
        let window = app.windows.firstMatch.frame
        let image = try screenImage(app)
        let scale = CGFloat(image.width) / window.width
        let first = sidebar.cells.firstMatch.frame.minY
        var box = sidebar.frame.insetBy(dx: 24, dy: 0)
        box.size.height = box.maxY - first
        box.origin.y = first
        box = box.offsetBy(dx: -window.minX, dy: -window.minY)
        box = CGRect(x: box.minX * scale, y: box.minY * scale,
                     width: box.width * scale, height: box.height * scale).integral
        return try pixels(XCTUnwrap(image.cropping(to: box)))
    }

    private func screenImage(_ app: XCUIApplication) throws -> CGImage {
        let window = app.windows.firstMatch.frame
        for (index, screen) in XCUIScreen.screens.enumerated() {
            let raw = screen.screenshot().pngRepresentation
            try raw.write(to: output.appendingPathComponent("screen-\(index).png"))
            guard let source = UIImage(data: raw) else { continue }
            let format = UIGraphicsImageRendererFormat()
            format.scale = 1
            let normalized = UIGraphicsImageRenderer(size: source.size, format: format).image { _ in
                source.draw(in: CGRect(origin: .zero, size: source.size))
            }
            guard let image = normalized.cgImage,
                  abs(CGFloat(image.width) / CGFloat(image.height) - window.width / window.height) < 0.01 else { continue }
            let data = try pixels(image)
            if data.enumerated().contains(where: { $0.offset % 4 < 3 && $0.element != 0 }) { return image }
        }
        throw NSError(domain: "GalleryUITests", code: 1, userInfo: [NSLocalizedDescriptionKey: "No active display matches the app window. Set the Duo pose before starting the UI test and retry."])
    }

    private func pixels(_ image: CGImage) throws -> Data {
        var data = Data(count: image.width * image.height * 4)
        try data.withUnsafeMutableBytes { bytes in
            let context = try XCTUnwrap(CGContext(data: bytes.baseAddress, width: image.width, height: image.height,
                                                bitsPerComponent: 8, bytesPerRow: image.width * 4,
                                                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        }
        return data
    }
}
