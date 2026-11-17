#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A custom invoker's pull-down grows out of the invoker and shrinks back
/// into it, as a stock button's menu does (#117): UIKit previews the
/// invoker node, clipped to its corners, not the transparent overlay that
/// holds the menu, and hides it while the menu shows. UIKit, so a
/// simulator runs it: `bun host/apple/build.mjs --test --ios --only PullDownPreviewIOSTests`.
final class PullDownPreviewIOSTests: XCTestCase {
    private var window: UIWindow!

    private func presenter() -> Presenter {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        // A screen (1): a glass pill (2) that opens its menu (3) of one row (4).
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "button", "props": ["popovertarget": "m", "backgroundMaterial": "glass"]],
            ["op": "style", "id": 2, "style": ["border_radius": 22]],
            ["op": "create", "id": 3, "kind": "view", "props": ["id": "m", "popover": "auto", "role": "menu"]],
            ["op": "create", "id": 4, "kind": "button", "handlers": ["press"], "props": ["role": "menuitem", "popovertarget": "m", "popovertargetaction": "hide"]],
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0],
            ["op": "frame", "id": 2, "x": 100.0, "y": 60.0, "w": 120.0, "h": 44.0],
            ["op": "frame", "id": 3, "x": 100.0, "y": 108.0, "w": 200.0, "h": 44.0],
            ["op": "frame", "id": 4, "x": 0.0, "y": 0.0, "w": 200.0, "h": 44.0],
        ]))
        return p
    }

    func testThePullDownPreviewsItsInvoker() throws {
        let p = presenter()
        defer { p.menus.reset() }
        let invoker = try XCTUnwrap(p.views[2])
        let button = try XCTUnwrap(invoker.subviews.compactMap { $0 as? MenuButton }.first)
        let interaction = try XCTUnwrap(button.contextMenuInteraction)
        let configuration = UIContextMenuConfiguration(identifier: nil, previewProvider: nil) { _ in button.menu }
        let previews = [
            ("highlight", button.contextMenuInteraction(interaction, previewForHighlightingMenuWithConfiguration: configuration)),
            ("dismissal", button.contextMenuInteraction(interaction, previewForDismissingMenuWithConfiguration: configuration)),
        ]
        for (name, preview) in previews {
            let preview = try XCTUnwrap(preview, name)
            XCTAssertTrue(preview.view === invoker, "\(name): the invoker, not the overlay")
            let path = try XCTUnwrap(preview.parameters.visiblePath, name)
            XCTAssertEqual(path.bounds, invoker.bounds, name)
            XCTAssertFalse(path.contains(CGPoint(x: 1, y: 1)), "\(name): clipped to the pill's corners")
            XCTAssertTrue(path.contains(CGPoint(x: 60, y: 22)), name)
            XCTAssertEqual(preview.parameters.backgroundColor, .clear, "\(name): nothing drawn behind the glass")
            let center = invoker.convert(CGPoint(x: invoker.bounds.midX, y: invoker.bounds.midY), to: preview.target.container)
            XCTAssertEqual(preview.target.center, center, "\(name): where the invoker is")
        }
    }

    func testAnInvokerOutOfTheWindowHasNoPreview() throws {
        let p = presenter()
        defer { p.menus.reset() }
        let invoker = try XCTUnwrap(p.views[2])
        let button = try XCTUnwrap(invoker.subviews.compactMap { $0 as? MenuButton }.first)
        let interaction = try XCTUnwrap(button.contextMenuInteraction)
        let configuration = UIContextMenuConfiguration(identifier: nil, previewProvider: nil) { _ in button.menu }
        invoker.removeFromSuperview()
        XCTAssertNil(button.contextMenuInteraction(interaction, previewForDismissingMenuWithConfiguration: configuration))
        XCTAssertNil(button.contextMenuInteraction(interaction, previewForHighlightingMenuWithConfiguration: configuration))
    }
}
#endif
