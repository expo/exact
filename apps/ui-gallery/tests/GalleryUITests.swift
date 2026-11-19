import XCTest
import UIKit

/// Runs on an open Duo, where no test skips, and on a compact window (the folded Duo or an iPhone), where
/// the tests that need the sidebar beside the page skip.
final class GalleryUITests: XCTestCase {
    override func setUp() {
        super.setUp()
        continueAfterFailure = false
    }

    func testSidebarSelectionFinishesWithoutAnotherTouch() throws {
        let app = XCUIApplication(bundleIdentifier: "com.exact.uigallery")
        app.launch()
        let sidebar = app.tables["gallery-sidebar"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 20))
        try requireSidebarBesideThePage(app)
        for section in ["css", "profiles", "textcolors"] {
            app.cells["sidebar-panel-" + section].tap()
            let before = try sidebarPixels(app, sidebar)
            sidebar.staticTexts["Text"].firstMatch.tap()
            let after = try sidebarPixels(app, sidebar)
            XCTAssertTrue(before == after, "Sidebar selection must finish drawing without another touch. Keep existing cells when only selection changes.")
        }
    }

    /// Beside the page, the split measures the sidebar's inline bar height, not its large title's, reports it
    /// only when it changes, and leaves the sidebar's large title as it was. This process's scene is in the
    /// background, where UIKit neither collapses the split nor changes its display mode: the app's own run
    /// checks those (`testHeaderCollapsesAndTheRangeBarFollowsIt`, and the iPhone's unchanged range bar)
    func testTheSplitMeasuresTheSidebarsInlineBarBesideThePage() throws {
        let tabs = (0..<9).map { index in
            GalleryTestTab(name: "Section \(index)", controller: UINavigationController(rootViewController: UIViewController()),
                           item: UITabBarItem(title: "Section \(index)", image: nil, tag: index))
        }
        let split = GallerySplit(ExactTabContents(tabs: tabs))
        var said: [CGFloat] = []
        split.besideBarHeightChanged = { [unowned split] in said.append(split.besideBarHeight) }
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 951, height: 669)
        window.traitOverrides.horizontalSizeClass = .regular
        window.rootViewController = split
        window.isHidden = false
        defer { window.isHidden = true }
        window.layoutIfNeeded()
        let navigation = try XCTUnwrap(split.viewController(for: .primary) as? UINavigationController)
        let sidebar = try XCTUnwrap(navigation.topViewController as? GallerySidebar)
        XCTAssertFalse(split.isCollapsed)
        XCTAssertGreaterThan(split.besideBarHeight, 0)
        XCTAssertLessThan(split.besideBarHeight, navigation.navigationBar.frame.height, "The inline height, not the large title's")
        XCTAssertEqual(sidebar.navigationItem.largeTitleDisplayMode, .always, "Measuring leaves the large title as it was")
        XCTAssertEqual(said, [split.besideBarHeight], "The height is reported once")
        split.view.setNeedsLayout()
        split.view.layoutIfNeeded()
        XCTAssertEqual(said.count, 1, "An unchanged height is not reported again: \(said)")
    }

    func testSidebarSelectionPreservesVisibleCells() throws {
        let tabs = (0..<9).map { index in
            GalleryTestTab(name: "Section \(index)", controller: UINavigationController(), item: UITabBarItem(title: "Section \(index)", image: nil, tag: index))
        }
        let contents = ExactTabContents(tabs: tabs)
        let sidebar = GallerySidebar(contents)
        sidebar.loadViewIfNeeded()
        let table = try XCTUnwrap(sidebar.tableView)
        table.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        let source = GalleryDataSourceSpy(sidebar)
        table.dataSource = source
        table.reloadData()
        table.layoutIfNeeded()
        XCTAssertGreaterThan(source.configurations, 0, "The table must configure visible rows before testing reuse")
        let configurations = source.configurations
        let cells = table.visibleCells
        for selection in [2, 3, 1, 1] {
            contents.selected = selection
            sidebar.refreshSelection()
            table.layoutIfNeeded()
            XCTAssertEqual(source.configurations, configurations, "Changing selection must not rebuild unchanged rows")
            XCTAssertEqual(table.visibleCells.map(ObjectIdentifier.init), cells.map(ObjectIdentifier.init))
        }
        XCTAssertEqual(table.indexPathForSelectedRow, IndexPath(row: 1, section: 0))
    }

    func testHeaderCollapsesAndTheRangeBarFollowsIt() throws {
        let app = XCUIApplication(bundleIdentifier: "com.exact.uigallery")
        app.launch()
        let sidebar = app.tables["gallery-sidebar"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 20))
        let cell = app.cells["sidebar-panel-css"]
        for _ in 0..<4 where !cell.isHittable { sidebar.swipeDown() }
        cell.tap()
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
        // Beside the sidebar, once it is scrolled too, the docked range bar ends level with its inline bar
        if sidebarIsBesideThePage(app) {
            let rangeBar = app.otherElements["range-bar-css"].firstMatch, sidebarBar = app.navigationBars["UI Gallery"]
            let large = sidebarBar.frame.height
            sidebar.swipeUp(velocity: .slow)
            XCTAssertLessThan(sidebarBar.frame.height, large, "The sidebar's title is inline once it is scrolled")
            XCTAssertEqual(rangeBar.frame.minY, sidebarBar.frame.minY, accuracy: 0.5, "The range bar is docked where the sidebar's bar starts")
            XCTAssertEqual(rangeBar.frame.maxY, sidebarBar.frame.maxY, accuracy: 0.5, "The two headers end on one line")
            let inline = sidebarBar.frame.height
            app.buttons["Hide Sidebar"].tap()
            XCTAssertLessThan(rangeBar.frame.height, inline, "With the sidebar hidden, the range bar takes its own height")
        }
        range.buttons["standard"].tap()
        XCTAssertTrue(range.buttons["standard"].isSelected, "The control takes a tap over the scrolled page")
        // A compact window collapses the split to one stack, whose back button returns to the sidebar
        guard app.windows.firstMatch.frame.width < 500 else { return }
        let back = app.buttons["BackButton"].firstMatch
        XCTAssertTrue(back.isHittable, "The scrolled page keeps a back button")
        back.tap()
        XCTAssertTrue(sidebar.isHittable)
    }

    func testRangeControlsRemainInteractiveOverScrollingContent() throws {
        let app = XCUIApplication(bundleIdentifier: "com.exact.uigallery")
        app.launch()
        let sidebar = app.tables["gallery-sidebar"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 20))
        try requireSidebarBesideThePage(app)
        for section in ["css", "images", "css"] {
            let cell = app.cells["sidebar-panel-" + section]
            for _ in 0..<4 where !cell.isHittable { sidebar.swipeDown() }
            cell.tap()
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
        app.launch()
        let sidebar = app.tables["gallery-sidebar"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 20))
        let media = app.cells["sidebar-panel-media"]
        for _ in 0..<4 where !media.isHittable { sidebar.swipeUp() }
        media.tap()
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

    /// Every page starts below its range bar, and beside the sidebar, which floats over the content's
    /// column that UIKit insets by its width.
    func testPageSwitchesKeepContentBelowTheRangeBarAndBesideTheSidebar() throws {
        let app = XCUIApplication(bundleIdentifier: "com.exact.uigallery")
        app.launch()
        let sidebar = app.tables["gallery-sidebar"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 20))
        try requireSidebarBesideThePage(app)
        var readings: [[String: Any]] = []
        for section in ["typography", "css", "profiles", "images", "typography", "textcolors", "media", "css", "images", "profiles", "textcolors", "typography", "media"] {
            // Read the initial page before a repeated selection can scroll it back to the top
            if !readings.isEmpty {
                let cell = app.cells["sidebar-panel-" + section]
                for _ in 0..<4 where !cell.isHittable {
                    if section == "media" || section == "images" { sidebar.swipeUp() } else { sidebar.swipeDown() }
                }
                cell.tap()
            }
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

    /// Skips the test unless the sidebar shows beside the page, as on the open Duo
    private func requireSidebarBesideThePage(_ app: XCUIApplication) throws {
        guard sidebarIsBesideThePage(app) else { throw XCTSkip("Needs the open Duo, whose sidebar stays beside the page") }
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

// Test the UIKit container independently of Exact's router
struct GalleryTestTab {
    let name: String
    let controller: UIViewController
    let item: UITabBarItem
}

final class ExactTabContents {
    let tabs: [GalleryTestTab]
    var selected = 0
    var onSelect: ((Int) -> Void)?

    init(tabs: [GalleryTestTab]) { self.tabs = tabs }
    func select(_ index: Int) { selected = index; onSelect?(index) }
}

private final class GalleryDataSourceSpy: NSObject, UITableViewDataSource {
    let source: UITableViewDataSource
    private(set) var configurations = 0

    init(_ source: UITableViewDataSource) { self.source = source }
    func numberOfSections(in tableView: UITableView) -> Int { source.numberOfSections?(in: tableView) ?? 1 }
    func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
        source.tableView(tableView, numberOfRowsInSection: section)
    }
    func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        configurations += 1
        return source.tableView(tableView, cellForRowAt: indexPath)
    }
    func tableView(_ tableView: UITableView, titleForHeaderInSection section: Int) -> String? {
        source.tableView?(tableView, titleForHeaderInSection: section)
    }
}
