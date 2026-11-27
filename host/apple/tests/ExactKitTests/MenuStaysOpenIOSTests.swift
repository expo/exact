#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A menu whose rows stay open at a pick (`keepsMenuPresented`): which rows
/// UIKit keeps it open for, and what the host rebuilds the open menu from
/// (MenuUpdateIOS.swift). UIKit, so a simulator runs it:
/// bun host/apple/build.mjs --test --ios --only MenuStaysOpenIOSTests
final class MenuStaysOpenIOSTests: XCTestCase {
    private var window: UIWindow!

    /// An invoker (1) opening `menu` (2): Kept (4), Closes (5, `false`),
    /// Hides (6, hides `menu`), Opens (7, opens the popover `info`, 20),
    /// Link (8), More (11, opening `sub`, 9: Nested (10), Nested kept (12, `true`)
    /// and Hides sub (13, `true`, hides `sub`)).
    private func presenter(menuKeeps: String? = "true") -> Presenter {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 600))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        func row(_ id: Int, _ label: String, _ props: [String: String] = [:], press: Bool = true) -> [[String: Any]] {
            [["op": "create", "id": 100 + id, "kind": "text", "props": ["text": label]],
             ["op": "frame", "id": 100 + id, "x": 0.0, "y": 0.0, "w": 180.0, "h": 20.0],
             ["op": "create", "id": id, "kind": "button", "handlers": press ? ["press"] : [], "props": props],
             ["op": "frame", "id": id, "x": 0.0, "y": 0.0, "w": 200.0, "h": 30.0], ["op": "children", "id": id, "ids": [100 + id]]]
        }
        var menu = ["popover": "auto", "id": "menu", "accessibilityRole": "menu"]
        if let menuKeeps { menu["keepsMenuPresented"] = menuKeeps }
        var ops: [[String: Any]] = [
            ["op": "create", "id": 1, "kind": "button", "handlers": ["press"], "props": ["popovertarget": "menu"]],
            ["op": "create", "id": 2, "kind": "view", "props": menu],
            ["op": "create", "id": 9, "kind": "view", "props": ["popover": "auto", "id": "sub", "accessibilityRole": "menu"]],
            ["op": "create", "id": 20, "kind": "view", "props": ["popover": "auto", "id": "info"]],
            ["op": "create", "id": 30, "kind": "view"],
        ]
        ops += row(4, "Kept") + row(5, "Closes", ["keepsMenuPresented": "false"])
        ops += row(6, "Hides", ["popovertarget": "menu", "popovertargetaction": "hide"]) + row(7, "Opens", ["popovertarget": "info"])
        ops += row(8, "Link", ["href": "https://expo.dev"], press: false) + row(11, "More", ["popovertarget": "sub"], press: false)
        ops += row(10, "Nested") + row(12, "Nested kept", ["keepsMenuPresented": "true"])
        ops += row(13, "Hides sub", ["keepsMenuPresented": "true", "popovertarget": "sub", "popovertargetaction": "hide"])
        ops += [["op": "children", "id": 2, "ids": [4, 5, 6, 7, 8, 11]], ["op": "children", "id": 9, "ids": [10, 12, 13]],
                ["op": "children", "id": 30, "ids": [1, 2, 9, 20]], ["op": "roots", "ids": [30]],
                ["op": "frame", "id": 30, "x": 0.0, "y": 0.0, "w": 400.0, "h": 600.0],
                ["op": "frame", "id": 1, "x": 20.0, "y": 20.0, "w": 200.0, "h": 40.0],
                ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 220.0, "h": 200.0],
                ["op": "frame", "id": 9, "x": 0.0, "y": 0.0, "w": 220.0, "h": 200.0],
                ["op": "frame", "id": 20, "x": 0.0, "y": 0.0, "w": 220.0, "h": 200.0]]
        p.apply(wireBatch(ops))
        return p
    }
    /// The menu as titles: `✓` for a check mark, `⇥` for a row the menu
    /// stays open for, `▸ [...]` for a submenu.
    private func shape(_ elements: [UIMenuElement]) -> [String] {
        elements.map { element in
            if let action = element as? UIAction {
                return action.title + (action.state == .on ? " ✓" : "") + (action.attributes.contains(.keepsMenuPresented) ? " ⇥" : "")
            }
            guard let menu = element as? UIMenu else { return "?" }
            return menu.title + " ▸ [" + shape(menu.children).joined(separator: ", ") + "]"
        }
    }
    private func action(_ title: String, in elements: [UIMenuElement]) throws -> UIAction {
        try XCTUnwrap(elements.compactMap { $0 as? UIAction }.first { $0.title == title })
    }

    /// Only the rows the author keeps open, and none that follows a link,
    /// opens something, or hides its menu; a submenu says it for itself.
    func testTheMenuStaysOpenOnlyForRowsItKeeps() throws {
        let p = presenter()
        let items = p.menus.items(of: try XCTUnwrap(p.views[2]), opener: p.views[1])
        XCTAssertEqual(shape(items), ["Kept ⇥", "Closes", "Hides", "Opens", "Link", "More ▸ [Nested, Nested kept ⇥, Hides sub]"])
        XCTAssertTrue(p.menus.kept.keeps)
        XCTAssertEqual((items.last as? UIMenu)?.identifier, MenuHost.submenuIdentifier(try XCTUnwrap(p.views[11])),
                       "a submenu is named by its row, so a rebuild keeps it open")
        let unsaid = presenter(menuKeeps: nil)
        let rows = unsaid.menus.items(of: try XCTUnwrap(unsaid.views[2]), opener: unsaid.views[1])
        XCTAssertEqual(shape(rows), ["Kept", "Closes", "Hides", "Opens", "Link", "More ▸ [Nested, Nested kept ⇥, Hides sub]"])
        XCTAssertEqual(shape(p.menus.items(of: try XCTUnwrap(p.views[2]), keeping: false)),
                       ["Kept", "Closes", "Hides", "Opens", "Link", "More ▸ [Nested, Nested kept, Hides sub]"], "the keyboard's edit menu keeps none")
        XCTAssertFalse(p.menus.kept.keeps)
    }

    /// A pick on a kept row presses it and leaves a bar's pull-down open; any other pick closes it.
    func testAKeptPickLeavesTheBarMenuOpen() throws {
        let p = presenter()
        let pop = try XCTUnwrap(p.views[2]), invoker = try XCTUnwrap(p.views[1])
        var pressed: [UInt32] = []
        p.onPress = { pressed.append($0) }
        let items = p.menus.items(of: pop, opener: invoker)
        p.menus.barMenuOpened(pop, from: invoker, in: { nil }, afterClose: false)
        UIButton().sendAction(try action("Kept", in: items))
        XCTAssertEqual(pressed, [4])
        XCTAssertTrue(p.menus.barMenu?.popover === pop, "still open")
        UIButton().sendAction(try action("Closes", in: items))
        XCTAssertEqual(pressed, [4, 5])
        XCTAssertNil(p.menus.barMenu)
    }

    /// The menu's shape changes with what its rows show, and only then, so a
    /// batch that changes none of them leaves the open menu alone.
    func testTheShapeFollowsTheRows() throws {
        let p = presenter()
        let pop = try XCTUnwrap(p.views[2])
        _ = p.menus.items(of: pop, opener: p.views[1])
        let first = p.menus.kept.shape
        _ = p.menus.items(of: pop, opener: p.views[1])
        XCTAssertEqual(p.menus.kept.shape, first)
        p.apply(wireBatch([["op": "props", "id": 12, "set": ["keepsMenuPresented": "true", "accessibilityChecked": "true"]]]))
        let items = p.menus.items(of: pop, opener: p.views[1])
        XCTAssertNotEqual(p.menus.kept.shape, first)
        XCTAssertEqual(shape(items).last, "More ▸ [Nested, Nested kept ✓ ⇥, Hides sub]")
    }

    /// With no menu open, a batch builds nothing, even after a kept menu was built.
    func testNoOpenMenuIsRebuilt() throws {
        let p = presenter()
        p.menus.recordsGiven = true
        _ = p.menus.items(of: try XCTUnwrap(p.views[2]), opener: p.views[1])
        XCTAssertTrue(p.menus.kept.keeps)
        p.menus.given = [:]
        p.menus.updateKeptMenu()
        XCTAssertTrue(p.menus.given.isEmpty)
    }

    /// Exact's menu takes the new rows and an open submenu the one built for
    /// it; a menu UIKit made itself is left as it is.
    func testAnUpdateReplacesOnlyExactsMenus() {
        let sub = UIMenu(title: "Tags", identifier: UIMenu.Identifier("exact.submenu.11.1"), children: [UIAction(title: "New") { _ in }])
        let children: [UIMenuElement] = [UIAction(title: "Kept") { _ in }, sub]
        let submenus = [sub.identifier: sub]
        let top = MenuHost.updated(UIMenu(identifier: MenuHost.menuIdentifier, children: []), children: children, submenus: submenus)
        XCTAssertEqual(shape(top.children), ["Kept", "Tags ▸ [New]"])
        let open = UIMenu(title: "Tags", identifier: sub.identifier, children: [UIAction(title: "Old") { _ in }])
        XCTAssertTrue(MenuHost.updated(open, children: children, submenus: submenus) === sub)
        let foreign = UIMenu(title: "History", children: [UIAction(title: "Back") { _ in }])
        XCTAssertTrue(MenuHost.updated(foreign, children: children, submenus: submenus) === foreign)
    }

    /// A pull-down's heading, rebuilt or not, is the agent's to pass over (`givenHeaders`).
    func testShownRowsRecordTheirHeading() throws {
        let p = presenter()
        p.menus.recordsGiven = true
        let pop = try XCTUnwrap(p.views[2])
        let rows = p.menus.shown(p.menus.items(of: pop, opener: p.views[1]), of: pop, heading: "Filter", headed: true)
        XCTAssertEqual(rows.count, 6)
        XCTAssertEqual(p.menus.givenHeaders[2], ["Filter"])
    }
}
#endif
