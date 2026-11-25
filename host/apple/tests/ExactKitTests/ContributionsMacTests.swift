#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// LLP 1123 D11 on AppKit: Exact's tagged writes to a view's additional insets (`ContributionsMac`).
final class ContributionsMacTests: XCTestCase {
    private var window: NSWindow?

    override func tearDown() {
        window?.close()
        window = nil
        super.tearDown()
    }

    /// Exact's writes add to the app's own, each its own effect on the view whatever its sign, a stale
    /// one is refused, and taking one away gives its part back with the app's kept.
    func testExactsWritesAddToTheAppsAndAreGivenBackWithTheAppsKept() throws {
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        self.window = window
        let root = try XCTUnwrap(window.contentView)
        let view = NSView(frame: root.bounds)
        root.addSubview(view)
        root.layoutSubtreeIfNeeded()
        let band = view.safeAreaInsets.top
        XCTAssertGreaterThan(band, 0, "a title bar over full-size content")
        view.additionalSafeAreaInsets = NSEdgeInsets(top: 0, left: 7, bottom: 0, right: 0)
        let ledger = ContributionsMac.of(view)
        XCTAssertTrue(ContributionsMac.existing(on: view) === ledger)
        let clearance = ContributionIdentity(node: 4, operation: .nativeViewInsets)
        let additional = ContributionIdentity(node: 4, operation: .additionalInsets)
        ledger.write(clearance, NSEdgeInsets(top: 30, left: 0, bottom: 0, right: 0))
        ledger.write(additional, NSEdgeInsets(top: -100, left: 0, bottom: 5, right: 0))
        root.layoutSubtreeIfNeeded()
        XCTAssertEqual(view.additionalSafeAreaInsets.left, 7, "the app's kept")
        XCTAssertEqual(view.safeAreaInsets.top, band + 30 - 100, "each its own effect, below 0 too")
        XCTAssertEqual(view.safeAreaInsets.bottom, 5)
        // The app adds to what it reads
        view.additionalSafeAreaInsets.left += 2
        XCTAssertFalse(ledger.set(clearance, NSEdgeInsets(top: 50, left: 0, bottom: 0, right: 0), generation: 0), "a stale write")
        ledger.write(additional, NSEdgeInsetsZero)
        root.layoutSubtreeIfNeeded()
        XCTAssertEqual(view.safeAreaInsets.top, band + 30)
        XCTAssertEqual(view.safeAreaInsets.bottom, 0)
        XCTAssertEqual(view.additionalSafeAreaInsets.left, 9, "the app's change kept")
        ledger.write(clearance, NSEdgeInsetsZero)
        XCTAssertEqual(view.additionalSafeAreaInsets.top, 0)
        XCTAssertEqual(view.additionalSafeAreaInsets.left, 9)
        XCTAssertTrue(ledger.entries.isEmpty)
    }
}
#endif
