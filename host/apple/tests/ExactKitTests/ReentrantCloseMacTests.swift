#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// Regression tests for re-entrant `MenuHost.close` and `DialogHost.close`
/// when closing a nested popover moves first responder and synchronously fires
/// a `blur` handler that mutates `entries`.
final class ReentrantCloseMacTests: XCTestCase {
    private var window: NSWindow!
    private weak var presenter: Presenter?

    override func tearDown() {
        presenter?.dialogs.reset()
        presenter?.menus.reset()
        window?.close()
        window = nil
        super.tearDown()
    }

    private func makePresenter(_ ops: [[String: Any]]) -> Presenter {
        _ = NSApplication.shared
        let p = Presenter()
        presenter = p
        p.dialogs.painted = true
        p.menus.painted = true
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 500, height: 400),
                          styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        p.apply(wireBatch(ops))
        return p
    }

    func testMenuCloseReenteringToCloseOuterDoesNotTrap() throws {
        let p = makePresenter([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "props": ["popover": "auto", "id": "outer"]],
            ["op": "create", "id": 3, "kind": "view", "props": ["popover": "auto", "id": "inner"]],
            ["op": "create", "id": 4, "kind": "button", "handlers": ["blur"]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 500, "h": 400],
            ["op": "frame", "id": 2, "x": 10, "y": 10, "w": 120, "h": 80],
            ["op": "frame", "id": 3, "x": 20, "y": 20, "w": 120, "h": 80],
            ["op": "frame", "id": 4, "x": 10, "y": 10, "w": 80, "h": 30],
        ])
        let outer = try XCTUnwrap(p.views[2]), inner = try XCTUnwrap(p.views[3]), focused = try XCTUnwrap(p.views[4])
        p.menus.show(outer, from: nil)
        p.menus.show(inner, from: nil)
        XCTAssertTrue(window.makeFirstResponder(focused))
        var blurs = 0
        p.onBlur = { [weak p, weak outer] id in
            guard id == focused.id, let p, let outer else { return }
            blurs += 1
            p.menus.close(outer)
        }
        p.menus.close(outer)
        XCTAssertEqual(blurs, 1)
        XCTAssertTrue(p.menus.entries.isEmpty)
    }

    func testMenuCloseReenteringToCloseLowerManualLeavesHigherManualOpen() throws {
        let p = makePresenter([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "props": ["popover": "manual", "id": "low"]],
            ["op": "create", "id": 3, "kind": "view", "props": ["popover": "auto", "id": "outer"]],
            ["op": "create", "id": 4, "kind": "view", "props": ["popover": "auto", "id": "inner"]],
            ["op": "create", "id": 5, "kind": "button", "handlers": ["blur"]],
            ["op": "create", "id": 6, "kind": "view", "props": ["popover": "manual", "id": "high"]],
            ["op": "children", "id": 1, "ids": [2, 3, 6]],
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 4, "ids": [5]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 500, "h": 400],
            ["op": "frame", "id": 2, "x": 10, "y": 10, "w": 100, "h": 60],
            ["op": "frame", "id": 3, "x": 20, "y": 20, "w": 100, "h": 60],
            ["op": "frame", "id": 4, "x": 30, "y": 30, "w": 100, "h": 60],
            ["op": "frame", "id": 5, "x": 10, "y": 10, "w": 60, "h": 30],
            ["op": "frame", "id": 6, "x": 40, "y": 40, "w": 100, "h": 60],
        ])
        let low = try XCTUnwrap(p.views[2]), outer = try XCTUnwrap(p.views[3]),
            inner = try XCTUnwrap(p.views[4]), focused = try XCTUnwrap(p.views[5]), high = try XCTUnwrap(p.views[6])
        p.menus.show(low, from: nil)
        p.menus.show(outer, from: nil)
        p.menus.show(inner, from: nil)
        p.menus.show(high, from: nil)
        XCTAssertTrue(window.makeFirstResponder(focused))
        p.onBlur = { [weak p, weak low] id in
            guard id == focused.id, let p, let low else { return }
            p.menus.close(low)
        }
        p.menus.close(outer)
        XCTAssertEqual(p.menus.entries.map(\.popover.id), [high.id],
                       "only the higher manual popover survives; outer is closed without removing high")
    }

    func testDialogCloseReenteringFromNestedPopoverBlurDoesNotTrap() throws {
        let p = makePresenter([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "props": ["semanticTag": "dialog", "id": "lower"]],
            ["op": "create", "id": 3, "kind": "view", "props": ["semanticTag": "dialog", "id": "upper"]],
            ["op": "create", "id": 4, "kind": "view", "props": ["popover": "auto", "id": "pop"]],
            ["op": "create", "id": 5, "kind": "button", "handlers": ["blur"]],
            ["op": "children", "id": 1, "ids": [2, 3]],
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 4, "ids": [5]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 500, "h": 400],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 200, "h": 150],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 200, "h": 150],
            ["op": "frame", "id": 4, "x": 10, "y": 10, "w": 100, "h": 60],
            ["op": "frame", "id": 5, "x": 10, "y": 10, "w": 60, "h": 30],
        ])
        let lower = try XCTUnwrap(p.views[2]), upper = try XCTUnwrap(p.views[3]),
            pop = try XCTUnwrap(p.views[4]), focused = try XCTUnwrap(p.views[5])
        // Subcase 1: blur inside popover closes `upper` itself re-entrantly.
        p.dialogs.show(lower)
        p.dialogs.show(upper)
        p.menus.show(pop, from: nil)
        XCTAssertTrue(window.makeFirstResponder(focused))
        p.onBlur = { [weak p, weak upper] id in
            guard id == focused.id, let p, let upper else { return }
            p.dialogs.close(upper)
        }
        p.dialogs.close(upper)
        XCTAssertEqual(p.dialogs.entries.map(\.dialog.id), [lower.id])
        XCTAssertTrue(p.menus.entries.isEmpty)

        // Subcase 2: blur inside popover closes `lower` beneath `upper`, shifting `upper`'s index.
        p.dialogs.show(upper)
        p.menus.show(pop, from: nil)
        XCTAssertTrue(window.makeFirstResponder(focused))
        p.onBlur = { [weak p, weak lower] id in
            guard id == focused.id, let p, let lower else { return }
            p.dialogs.close(lower)
        }
        p.dialogs.close(upper)
        XCTAssertTrue(p.dialogs.entries.isEmpty)
        XCTAssertTrue(p.menus.entries.isEmpty)
    }
}
#endif
