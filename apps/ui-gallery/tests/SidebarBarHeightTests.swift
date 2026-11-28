import XCTest
import UIKit

final class SidebarBarHeightTests: XCTestCase {
    /// The page asks at launch, before the module's first check; answering then gave 0, and the range bar fell from
    /// the height kept from the last launch and rose again once the check measured the bar.
    @MainActor
    func testAnAnswerAskedBeforeTheFirstCheckIsTheMeasuredHeight() throws {
        let (list, split) = expandedSplit()
        let window = show(split)
        let bar = try XCTUnwrap(list.navigationController?.navigationBar)

        let height = SidebarBarHeight()
        var answers: [CGFloat] = []
        let answered = expectation(description: "the first check answers")
        height.follow(list: list.view)
        height.whenSettled { answers.append($0); answered.fulfill() }
        XCTAssertEqual(answers, [], "Nothing is measured before the first check, so nothing is answered")
        wait(for: [answered], timeout: 5)

        XCTAssertFalse(window.isHidden)
        XCTAssertGreaterThan(bar.bounds.height, 0)
        XCTAssertEqual(answers.count, 1)
        XCTAssertEqual(try XCTUnwrap(answers.first), bar.bounds.height, accuracy: 0.5,
                       "The first answer is the list's inline bar height, not 0")
        height.whenSettled { answers.append($0) }
        XCTAssertEqual(answers.count, 2, "Once settled, an answer is given at once")
        XCTAssertEqual(answers.last, answers.first)
    }

    /// A split out of any window has no bar to measure: the first check answers 0, as `main` did, and once the list's
    /// watch hears the split enter a window the module says the measured height.
    @MainActor
    func testAnAnswerWithTheSplitOutOfAWindowIsZeroUntilItEntersOne() throws {
        let (list, split) = expandedSplit()
        let height = SidebarBarHeight()
        let watch = SidebarBarWatch(frame: list.view.bounds)
        watch.onChange = { [weak height] in height?.scheduleUpdate() }
        list.view.addSubview(watch)
        var answers: [CGFloat] = []
        let answered = expectation(description: "the first check answers")
        height.follow(list: list.view)
        height.whenSettled { answers.append($0); answered.fulfill() }
        wait(for: [answered], timeout: 5)
        XCTAssertEqual(answers, [0])

        let changed = expectation(description: "the module says the measured height")
        changed.assertForOverFulfill = false
        height.changed = { changed.fulfill() }
        show(split)
        let bar = try XCTUnwrap(list.navigationController?.navigationBar)
        wait(for: [changed], timeout: 5)
        XCTAssertGreaterThan(bar.bounds.height, 0)
        XCTAssertEqual(height.height, bar.bounds.height, accuracy: 0.5)
    }

    /// An expanded split whose list column is hidden has no list bar showing: the answer is 0, not a wait.
    @MainActor
    func testAnAnswerWithTheListHiddenIsZero() {
        let (list, split) = expandedSplit()
        split.preferredDisplayMode = .secondaryOnly
        show(split)
        XCTAssertEqual(split.displayMode, .secondaryOnly)
        let height = SidebarBarHeight()
        var answers: [CGFloat] = []
        let answered = expectation(description: "the first check answers")
        height.whenSettled { answers.append($0); answered.fulfill() }
        height.follow(list: list.view)
        wait(for: [answered], timeout: 5)
        XCTAssertEqual(answers, [0])
    }

    /// With no list hatched, as with hatches off, a request settles the height at 0 instead of waiting for good.
    @MainActor
    func testAnAnswerWithNoListIsZero() {
        let height = SidebarBarHeight()
        let answered = expectation(description: "the check answers")
        var answers: [CGFloat] = []
        height.whenSettled { answers.append($0); answered.fulfill() }
        wait(for: [answered], timeout: 5)
        XCTAssertEqual(answers, [0])
        XCTAssertTrue(height.settled)
    }

    /// A list in the primary column of a two-column split that shows both columns.
    @MainActor
    private func expandedSplit() -> (UIViewController, UISplitViewController) {
        let list = UIViewController()
        list.title = "List"
        list.navigationItem.largeTitleDisplayMode = .never
        let split = UISplitViewController(style: .doubleColumn)
        split.setViewController(list, for: .primary)
        split.setViewController(UIViewController(), for: .secondary)
        split.preferredDisplayMode = .oneBesideSecondary
        split.traitOverrides.horizontalSizeClass = .regular
        return (list, split)
    }

    /// Shows `split` in a wide window of the runner's scene, hidden again when the test ends.
    @MainActor
    @discardableResult
    private func show(_ split: UISplitViewController) -> UIWindow {
        let scene = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first
        let window = scene.map(UIWindow.init(windowScene:)) ?? UIWindow()
        window.frame = CGRect(x: 0, y: 0, width: 1000, height: 800)
        window.rootViewController = split
        window.isHidden = false
        addTeardownBlock { @MainActor in window.isHidden = true }
        window.layoutIfNeeded()
        XCTAssertFalse(split.isCollapsed)
        return window
    }
}
