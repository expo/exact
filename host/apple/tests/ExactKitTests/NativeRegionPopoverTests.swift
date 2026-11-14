import XCTest
@testable import ExactKit
#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif

#if os(macOS) || os(iOS)
/// Consumer half of content_region.rs's native publication regression: the
/// row's captured child list omits its separately published top-layer subtree.
final class NativeRegionPopoverTests: XCTestCase {
    func testAPopoverLiftsFromItsPublishedRowAndReturnsWithoutLosingContent() throws {
        #if os(macOS)
        _ = NSApplication.shared
        let window = NSWindow(contentRect: CGRect(x: 100, y: 100, width: 800, height: 600),
                              styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        #else
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 800, height: 600))
        window.frame = CGRect(x: 0, y: 0, width: 800, height: 600)
        let controller = UIViewController()
        window.rootViewController = controller
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        #endif
        let p = Presenter()
        defer { p.menus.reset() }
        #if os(macOS)
        p.menus.painted = true
        window.contentView = p.viewport
        window.orderFrontRegardless()
        #else
        controller.view.addSubview(p.viewport)
        p.viewport.frame = controller.view.bounds
        #endif
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 7, "kind": "view", "props": ["id": "pop", "popover": "auto", "popoverParent": "4"]],
            ["op": "create", "id": 8, "kind": "text", "props": ["text": "Popover content"]],
            ["op": "children", "id": 7, "ids": [8]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 800, "h": 600],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 400, "h": 200],
            ["op": "frame", "id": 7, "x": 0, "y": 0, "w": 400, "h": 60],
            ["op": "frame", "id": 8, "x": 0, "y": 0, "w": 200, "h": 20],
        ]))
        let pop = try XCTUnwrap(p.views[7])
        XCTAssertNil(pop.superview, "the native row is waiting on its text artifact")
        p.apply(wireBatch([
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "create", "id": 4, "kind": "view"],
            ["op": "create", "id": 5, "kind": "text", "props": ["text": "Row content"]],
            ["op": "create", "id": 6, "kind": "button", "props": ["id": "source", "popovertarget": "pop"]],
            ["op": "children", "id": 4, "ids": [5, 6]],
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 400, "h": 200],
            ["op": "frame", "id": 4, "x": 0, "y": 0, "w": 400, "h": 80],
            ["op": "frame", "id": 5, "x": 0, "y": 0, "w": 200, "h": 20],
            ["op": "frame", "id": 6, "x": 20, "y": 30, "w": 80, "h": 30],
        ]))
        let row = try XCTUnwrap(p.views[4]), body = try XCTUnwrap(p.views[5])
        XCTAssertTrue(pop.superview === row.container)
        XCTAssertTrue(body.superview === row.container)
        p.menus.action("showPopover", ["pop", "source"])
        XCTAssertTrue(p.menus.isOpen(pop))
        let layer = try XCTUnwrap(pop.superview)
        XCTAssertFalse(layer === row.container)
        let box = pop.convert(pop.bounds, to: layer)
        XCTAssertEqual(box.width, 400, accuracy: 0.5)
        XCTAssertEqual(box.minX, 20, accuracy: 0.5)
        XCTAssertEqual(box.minY, 60, accuracy: 0.5)
        XCTAssertFalse(pop.isHidden)
        XCTAssertTrue(p.views[8]?.isDescendant(of: pop) == true)
        // A replacement native publication still omits the open popover.
        p.apply(wireBatch([
            ["op": "props", "id": 5, "set": ["text": "Updated row content"], "clear": []],
            ["op": "children", "id": 4, "ids": [5, 6]],
        ]))
        XCTAssertTrue(p.menus.isOpen(pop))
        XCTAssertTrue(pop.superview === layer)
        XCTAssertTrue(body.superview === row.container)
        XCTAssertEqual(body.props["text"], "Updated row content")
        p.menus.action("hidePopover", ["pop"])
        // Transfer the same views into and out of the row's publication.
        p.apply(wireBatch([
            ["op": "props", "id": 7, "set": [:], "clear": ["popoverParent"]],
            ["op": "style", "id": 7, "style": ["position_type": "sticky"]],
            ["op": "children", "id": 4, "ids": [5, 6, 7]],
        ]))
        XCTAssertTrue(pop.superview === row.container)
        p.menus.action("showPopover", ["pop", "source"])
        XCTAssertTrue(p.menus.isOpen(pop))
        let transferredLayer = try XCTUnwrap(pop.superview)
        p.apply(wireBatch([
            ["op": "props", "id": 7, "set": ["popoverParent": "4"], "clear": []],
            ["op": "children", "id": 4, "ids": [5, 6]],
            ["op": "style", "id": 7, "style": ["position_type": "absolute"]],
        ]))
        XCTAssertTrue(p.views[7] === pop)
        XCTAssertTrue(p.menus.isOpen(pop), "transfer preserves an open popover")
        XCTAssertTrue(pop.superview === transferredLayer)
        XCTAssertTrue(p.views[8]?.isDescendant(of: pop) == true)
        XCTAssertTrue(body.superview === row.container)
        p.menus.action("hidePopover", ["pop"])
        XCTAssertFalse(p.menus.isOpen(pop))
        XCTAssertTrue(pop.isHidden)
        XCTAssertTrue(pop.superview === row.container)
        XCTAssertTrue(p.views[8]?.isDescendant(of: pop) == true)
        XCTAssertTrue(p.views[5] === body)
        p.menus.action("showPopover", ["pop", "source"])
        XCTAssertTrue(p.menus.isOpen(pop), "the same popover can reopen")
        p.menus.action("hidePopover", ["pop"])
    }
}
#endif
