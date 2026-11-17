#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// `ariaUnavailable` answers false without a walk while no view carries
/// `accessibilityDisabled`, and walks as before once one does: set, cleared,
/// let go by its presenter while still shown, and across a presenter's reset.
@MainActor
final class AriaUnavailableTests: XCTestCase {
    // Freed carriers from other tests in this process go first, so a new
    // view cannot reuse a stale entry's address.
    override func setUp() { super.setUp(); AriaCarriers.prune() }
    private func tree(_ p: Presenter) -> (NodeView, NodeView) {
        let outer = NodeView(id: 1, kind: "view", presenter: p)
        let button = NodeView(id: 2, kind: "button", presenter: p)
        p.root.addSubview(outer); p.views[1] = outer
        outer.addSubview(button); p.views[2] = button
        return (outer, button)
    }

    func testAnAncestorsAriaDisabledStillReachesItsDescendants() {
        let before = AriaCarriers.count
        let p = Presenter()
        let (outer, button) = tree(p)
        XCTAssertFalse(button.ariaUnavailable)
        outer.props["accessibilityDisabled"] = "true"
        XCTAssertEqual(AriaCarriers.count, before + 1)
        XCTAssertTrue(button.ariaUnavailable, "the walk finds the ancestor")
        outer.props["accessibilityDisabled"] = "false"
        XCTAssertEqual(AriaCarriers.count, before + 1, "any value is carried")
        XCTAssertFalse(button.ariaUnavailable, "the nearest value decides, as before")
        button.props["accessibilityDisabled"] = "true"
        XCTAssertEqual(AriaCarriers.count, before + 2)
        XCTAssertTrue(button.ariaUnavailable)
        button.props = [:]
        outer.props = [:]
        XCTAssertEqual(AriaCarriers.count, before)
        XCTAssertFalse(button.ariaUnavailable)
    }

    /// A mounted subtree whose ancestor takes the prop mid-life: the next
    /// read below it walks, with no carrier before.
    func testAnAncestorThatTakesThePropLaterIsSeenAtOnce() throws {
        try XCTSkipIf(AriaCarriers.any, "a carrier from another test is still live")
        let p = Presenter()
        let (outer, button) = tree(p)
        let leaf = NodeView(id: 3, kind: "button", presenter: p)
        button.addSubview(leaf); p.views[3] = leaf
        XCTAssertFalse(leaf.ariaUnavailable)
        outer.props["accessibilityDisabled"] = "true"
        XCTAssertTrue(AriaCarriers.any)
        XCTAssertTrue(button.ariaUnavailable)
        XCTAssertTrue(leaf.ariaUnavailable, "two levels down, at once")
        outer.props = [:]
        XCTAssertFalse(leaf.ariaUnavailable)
    }

    /// An exiting ancestor leaves the presenter's maps and keeps its props
    /// while it is shown: its descendants still read it (review r1).
    func testAViewItsPresenterLetGoStillCounts() {
        let before = AriaCarriers.count
        let p = Presenter()
        let (outer, button) = tree(p)
        outer.props["accessibilityDisabled"] = "true"
        _ = p.release(1, forget: false)
        XCTAssertTrue(button.ariaUnavailable, "as the walk answered before")
        XCTAssertEqual(AriaCarriers.count, before + 1)
        outer.props = [:]
        XCTAssertEqual(AriaCarriers.count, before)
    }

    func testAFreedCarrierLeavesTheCount() {
        let before = AriaCarriers.count
        autoreleasepool {
            let p = Presenter()
            let (outer, _) = tree(p)
            outer.props["accessibilityDisabled"] = "true"
            XCTAssertEqual(AriaCarriers.count, before + 1)
            outer.removeFromSuperview(); p.views.removeAll()
        }
        AriaCarriers.prune()
        XCTAssertEqual(AriaCarriers.count, before, "freed with its prop, it is let go")
    }

    func testAPresentersResetTakesItsCarriersOutOfTheCount() {
        let before = AriaCarriers.count
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 200, height: 200), styleMask: [.titled], backing: .buffered, defer: false)
        let p = Presenter()
        window.contentView = p.viewport
        let (outer, button) = tree(p)
        outer.props["accessibilityDisabled"] = "false"
        button.props["accessibilityDisabled"] = "true"
        XCTAssertEqual(AriaCarriers.count, before + 2)
        p.reset()
        XCTAssertEqual(AriaCarriers.count, before, "a reset presenter's views are in no window")
    }

    func testWithNoCarrierAnywhereNoViewIsWalked() throws {
        // Another suite in this process may still hold one.
        try XCTSkipIf(AriaCarriers.any, "a carrier from another test is still live")
        let p = Presenter()
        let (outer, button) = tree(p)
        outer.props["semanticTag"] = "main"
        XCTAssertFalse(button.ariaUnavailable)
        // A view its presenter forgot is noted no more: its prop, set now,
        // is seen only by a walk, and none runs.
        outer.forget()
        outer.props["accessibilityDisabled"] = "true"
        XCTAssertFalse(AriaCarriers.any)
        XCTAssertFalse(button.ariaUnavailable, "answered without a walk")
        outer.props = [:]
    }
}
#endif
