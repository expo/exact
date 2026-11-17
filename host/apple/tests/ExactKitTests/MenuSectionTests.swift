#if os(iOS)
import UIKit
#else
import AppKit
#endif
import XCTest
@testable import ExactKit

/// A menu's titled sections (LLP 1021 D3, amended 2026-11-12; #118): a
/// `role="group"` row is a section of its own rows, titled by its accessible
/// name, on iOS an inline `UIMenu(title:)` and on macOS a section header
/// between separators. Its rows choose as any other row. iOS on a simulator,
/// `bun host/apple/build.mjs --test --ios --only MenuSectionTests`; macOS,
/// `bun host/apple/build.mjs --test --only MenuSectionTests`.
final class MenuSectionTests: XCTestCase {
    #if os(iOS)
    private var window: UIWindow!
    #else
    private var window: NSWindow!
    #endif

    /// Presets (2): Pair (3), then "Your presets" (4: Fireball 5, Comet 6),
    /// then a group titled by its text Mine (7: the text 8, Saved 9), then
    /// an untitled group (10: Reset 11), then an empty titled one (12).
    private func presenter() -> Presenter {
        let p = Presenter()
        #if os(iOS)
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 600))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        #else
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 600), styleMask: [.titled], backing: .buffered, defer: true)
        window.isReleasedWhenClosed = false
        p.viewport.frame = NSRect(x: 0, y: 0, width: 400, height: 600)
        window.contentView = p.viewport
        #endif
        func row(_ id: Int, _ label: String) -> [[String: Any]] {
            [["op": "create", "id": 100 + id, "kind": "text", "props": ["text": label]],
             ["op": "frame", "id": 100 + id, "x": 0.0, "y": 0.0, "w": 180.0, "h": 20.0],
             ["op": "create", "id": id, "kind": "button", "handlers": ["press"], "props": ["popovertarget": "presets", "popovertargetaction": "hide"]],
             ["op": "frame", "id": id, "x": 0.0, "y": 0.0, "w": 200.0, "h": 30.0], ["op": "children", "id": id, "ids": [100 + id]]]
        }
        var ops: [[String: Any]] = [
            ["op": "create", "id": 1, "kind": "button", "props": ["popovertarget": "presets"]],
            ["op": "create", "id": 2, "kind": "view", "props": ["popover": "auto", "id": "presets", "accessibilityRole": "menu", "accessibilityLabel": "Presets"]],
            ["op": "create", "id": 4, "kind": "view", "props": ["accessibilityRole": "group", "accessibilityLabel": "Your presets"]],
            ["op": "create", "id": 7, "kind": "view", "props": ["accessibilityRole": "group", "accessibilityLabelledBy": "mine"]],
            ["op": "create", "id": 8, "kind": "text", "props": ["text": "Mine", "id": "mine"]],
            ["op": "create", "id": 10, "kind": "view", "props": ["accessibilityRole": "group"]],
            ["op": "create", "id": 12, "kind": "view", "props": ["accessibilityRole": "group", "accessibilityLabel": "Empty"]],
            ["op": "create", "id": 20, "kind": "view"],
        ]
        ops += row(3, "Pair") + row(5, "Fireball") + row(6, "Comet") + row(9, "Saved") + row(11, "Reset")
        ops += [["op": "children", "id": 4, "ids": [5, 6]], ["op": "children", "id": 7, "ids": [8, 9]], ["op": "children", "id": 10, "ids": [11]],
                ["op": "children", "id": 2, "ids": [3, 4, 7, 10, 12]],
                ["op": "children", "id": 20, "ids": [1, 2]], ["op": "roots", "ids": [20]],
                ["op": "frame", "id": 20, "x": 0.0, "y": 0.0, "w": 400.0, "h": 600.0],
                ["op": "frame", "id": 1, "x": 20.0, "y": 20.0, "w": 120.0, "h": 40.0],
                ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 220.0, "h": 300.0],
                ["op": "frame", "id": 8, "x": 0.0, "y": 0.0, "w": 180.0, "h": 20.0]]
        for id in [4, 7, 10, 12] { ops.append(["op": "frame", "id": id, "x": 0.0, "y": 0.0, "w": 200.0, "h": 60.0]) }
        p.apply(wireBatch(ops))
        return p
    }

    func testAGroupIsMenuShaped() throws {
        let p = presenter()
        defer { p.menus.reset() }
        let pop = try XCTUnwrap(p.views[2])
        XCTAssertTrue(pop.menuShaped)
        p.apply(wireBatch([["op": "create", "id": 30, "kind": "view", "props": ["semanticTag": "hr"]],
                           ["op": "children", "id": 4, "ids": [5, 30, 6]]]))
        XCTAssertFalse(pop.menuShaped, "a section holds rows only")
    }

    #if os(iOS)
    /// Each section an inline menu, its title the group's name, `|` between them.
    func testASectionIsATitledInlineMenu() throws {
        let p = presenter()
        defer { p.menus.reset() }
        let pop = try XCTUnwrap(p.views[2])
        let sections = p.menus.items(of: pop).map { element -> String in
            guard let menu = element as? UIMenu, menu.options.contains(.displayInline) else { return "?" }
            return "\(menu.title): " + menu.children.map(\.title).joined(separator: ", ")
        }
        XCTAssertEqual(sections, [": Pair", "Your presets: Fireball, Comet", "Mine: Saved", ": Reset"])
        // The pull-down keeps the popover's own label over the whole menu.
        let invoker = try XCTUnwrap(p.views[1])
        let button = try XCTUnwrap(invoker.subviews.compactMap { $0 as? MenuButton }.first)
        let outer = try XCTUnwrap(button.menu?.children.first as? UIMenu)
        XCTAssertEqual(outer.title, "Presets")
    }

    func testARowInASectionPressesItsRow() throws {
        let p = presenter()
        defer { p.menus.reset() }
        var pressed: [UInt32] = []
        p.onPress = { pressed.append($0) }
        let pop = try XCTUnwrap(p.views[2])
        let section = try XCTUnwrap(p.menus.items(of: pop)[1] as? UIMenu)
        let comet = try XCTUnwrap(section.children.last as? UIAction)
        XCTAssertEqual(comet.title, "Comet")
        comet.performWithSender(nil, target: nil)
        XCTAssertEqual(pressed, [6])
    }
    #else
    /// A separator around each section, its header the group's name.
    func testASectionHasAHeaderBetweenSeparators() throws {
        let p = presenter()
        defer { p.menus.reset() }
        let pop = try XCTUnwrap(p.views[2])
        let lines = p.menus.menu(of: pop).items.map { item -> String in
            if item.isSeparatorItem { return "—" }
            if item.isSectionHeader { return "# " + item.title }
            return item.title
        }
        XCTAssertEqual(lines, ["# Presets", "Pair", "—", "# Your presets", "Fireball", "Comet", "—", "# Mine", "Saved", "—", "Reset"])
        // A section that opens the menu heads it alone, as UIKit draws no header over another.
        p.apply(wireBatch([["op": "children", "id": 2, "ids": [4, 3]]]))
        let first = p.menus.menu(of: pop).items.map { $0.isSeparatorItem ? "—" : ($0.isSectionHeader ? "# " : "") + $0.title }
        XCTAssertEqual(first, ["# Your presets", "Fireball", "Comet", "—", "Pair"])
    }
    #endif
}
