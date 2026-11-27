#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// exact2's menu rule on UIKit (LLP 1021; theme2 2026-10-21 19:47, 22:05):
/// a popover a row of a menu opens, no menu itself, is no submenu: the row is
/// an action that presses it and then opens the popover as if by the menu's
/// own invoker (the node the context menu opened on), so the menu closing
/// as its row is chosen doesn't take it along. A row opening a menu is still
/// its submenu. UIKit, so a simulator runs it.
final class MenuRowPopoverIOSTests: XCTestCase {
    private var window: UIWindow!

    /// A message (1) whose context menu is `message-menu` (2): React (3,
    /// opening `picker`, 5, a popover of an emoji button, 6) and More (4,
    /// opening `more-menu`, 7, a menu of one row, 8).
    private func presenter(session: ExactSession? = nil, menuRole: Bool = true) -> Presenter {
        let p = session?.presenter ?? Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 600))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        func button(_ id: Int, _ label: String, _ props: [String: String] = [:]) -> [[String: Any]] {
            [["op": "create", "id": 100 + id, "kind": "text", "props": ["text": label]],
             ["op": "frame", "id": 100 + id, "x": 0.0, "y": 0.0, "w": 180.0, "h": 20.0],
             ["op": "create", "id": id, "kind": "button", "handlers": ["press"], "props": props],
             ["op": "frame", "id": id, "x": 0.0, "y": 0.0, "w": 200.0, "h": 30.0], ["op": "children", "id": id, "ids": [100 + id]]]
        }
        var menuProps = ["popover": "auto", "id": "message-menu"]
        if menuRole { menuProps["accessibilityRole"] = "menu" }
        var ops: [[String: Any]] = [
            ["op": "create", "id": 1, "kind": "button", "handlers": ["press"], "props": ["contextPopover": "message-menu", "id": "message"]],
            ["op": "create", "id": 2, "kind": "view", "props": menuProps],
            ["op": "create", "id": 5, "kind": "view", "props": ["popover": "auto", "id": "picker"]],
            ["op": "create", "id": 7, "kind": "view", "props": ["popover": "auto", "id": "more-menu", "accessibilityRole": "menu"]],
            ["op": "create", "id": 9, "kind": "view"],
        ]
        ops += button(3, "Add Reaction", ["popovertarget": "picker"]) + button(4, "More", ["popovertarget": "more-menu"])
        ops += button(6, "👍") + button(8, "Deep")
        ops += [["op": "children", "id": 2, "ids": [3, 4]], ["op": "children", "id": 5, "ids": [6]], ["op": "children", "id": 7, "ids": [8]],
                ["op": "children", "id": 9, "ids": [1, 2, 5, 7]], ["op": "roots", "ids": [9]],
                ["op": "frame", "id": 9, "x": 0.0, "y": 0.0, "w": 400.0, "h": 600.0],
                ["op": "frame", "id": 1, "x": 20.0, "y": 300.0, "w": 200.0, "h": 40.0],
                ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 220.0, "h": 120.0],
                ["op": "frame", "id": 5, "x": 0.0, "y": 0.0, "w": 220.0, "h": 60.0],
                ["op": "frame", "id": 7, "x": 0.0, "y": 0.0, "w": 220.0, "h": 60.0]]
        p.apply(wireBatch(ops))
        return p
    }

    func testAnAgentsMenuRowKeepsItsButtonShapedPickerMeasured() throws {
        for menuRole in [true, false] {
            let session = ExactApp.shared.makeSession(label: "menu-row-check")
            defer { session.destroy(); window?.isHidden = true; window = nil }
            let p = presenter(session: session, menuRole: menuRole), agent = Agent(session: session)
            let menu = try XCTUnwrap(p.views[2]), picker = try XCTUnwrap(p.views[5])
            XCTAssertTrue(picker.menuShaped, "the role-less emoji picker contains only buttons")
            p.views[1]?.props["popovertarget"] = "message-menu"
            p.views[1]?.style["appearance"] = .string("none")
            p.menus.invoke(try XCTUnwrap(p.views[1]))
            XCTAssertTrue(p.menus.platformMenus.contains { $0.popover === menu })
            p.press(3)
            XCTAssertTrue(p.menus.isOpen(picker))
            XCTAssertFalse(p.menus.isOpen(menu), "choosing content dismisses the menu")
            let reply = agent.check([:])
            let items = try XCTUnwrap(reply["items"] as? [[String: Any]])
            XCTAssertTrue(items.contains { $0["id"] as? Int == 6 }, "the picker's emoji button stays measured")
            XCTAssertTrue(p.menus.platformMenus.isEmpty, "the largest-text pass must also measure the picker")
        }
    }

    func testACommandWithAMatchingSourceKeepsItsNestedPopoverAndStaysJudged() throws {
        for menuRole in [false, true] {
            let session = ExactApp.shared.makeSession(label: "command-popover-check")
            defer { session.destroy(); window?.isHidden = true; window = nil }
            let p = presenter(session: session, menuRole: menuRole), agent = Agent(session: session)
            let choices = try XCTUnwrap(p.views[2]), picker = try XCTUnwrap(p.views[5])
            p.apply(wireBatch([
                ["op": "create", "id": 10, "kind": "view"],
                ["op": "children", "id": 10, "ids": [6]], ["op": "children", "id": 5, "ids": [10]],
                ["op": "frame", "id": 10, "x": 0, "y": 0, "w": 220, "h": 60],
            ]))
            XCTAssertFalse(picker.menuShaped, "the row opens ordinary content")
            let source = try XCTUnwrap(p.views[1])
            source.props["popovertarget"] = "message-menu"
            source.style["appearance"] = .string("none")
            XCTAssertTrue(p.menus.platformMenu(choices, from: source))
            p.menus.action("showPopover", ["message-menu", "message"])
            XCTAssertTrue(p.menus.platformMenus.isEmpty)
            let observed = agent.check(["capture": true])
            XCTAssertFalse(choices.isHidden, "command-opened content stays in the carrier capture")
            XCTAssertTrue((observed["items"] as? [[String: Any]])?.contains { $0["id"] as? Int == 3 } == true)
            _ = agent.check(["release": true])
            p.press(3)
            XCTAssertTrue(p.menus.isOpen(choices), "the row's child keeps its content holder")
            XCTAssertTrue(p.menus.isOpen(picker))
            XCTAssertTrue(p.menus.platformMenus.isEmpty)
            let nested = agent.check(["capture": true])
            XCTAssertFalse(choices.isHidden)
            XCTAssertFalse(picker.isHidden)
            XCTAssertTrue((nested["items"] as? [[String: Any]])?.contains { $0["id"] as? Int == 6 } == true)
            _ = agent.check(["release": true])
            p.menus.drop("message-menu")
            XCTAssertFalse(p.menus.isOpen(picker), "closing the parent closes its nested child")
        }
    }

    func testAContextCardKeepsItsNestedPopoverAndStaysJudged() throws {
        let session = ExactApp.shared.makeSession(label: "context-card-check")
        defer { session.destroy(); window?.isHidden = true; window = nil }
        let p = presenter(session: session, menuRole: false), agent = Agent(session: session)
        let card = try XCTUnwrap(p.views[2]), picker = try XCTUnwrap(p.views[5])
        p.apply(wireBatch([
            ["op": "create", "id": 10, "kind": "view"],
            ["op": "children", "id": 10, "ids": [3, 4]], ["op": "children", "id": 2, "ids": [10]],
            ["op": "frame", "id": 10, "x": 0, "y": 0, "w": 220, "h": 120],
        ]))
        XCTAssertFalse(card.menuShaped)
        p.menus.agentShow(card, named: "message-menu", from: 1, context: true)
        XCTAssertTrue(p.menus.platformMenus.isEmpty)
        let observed = agent.check(["capture": true])
        XCTAssertFalse(card.isHidden, "the context card remains painted during the carrier capture")
        XCTAssertTrue((observed["items"] as? [[String: Any]])?.contains { $0["id"] as? Int == 3 } == true)
        _ = agent.check(["release": true])
        p.press(3)
        XCTAssertTrue(p.menus.isOpen(card), "content opened from a card keeps its holder")
        XCTAssertTrue(p.menus.isOpen(picker))
        p.menus.drop("message-menu")
        XCTAssertFalse(p.menus.isOpen(picker), "the picker is nested in the card")
    }

    func testAMenuRowsPopoverOpensFromTheMenusInvokerAndAMenuIsStillASubmenu() throws {
        let p = presenter()
        let items = p.menus.items(of: try XCTUnwrap(p.views[2]), opener: p.views[1])
        let titles = items.flatMap { ($0 as? UIMenu)?.options.contains(.displayInline) == true ? ($0 as! UIMenu).children : [$0] }
        let react = try XCTUnwrap(titles.first { $0.title == "Add Reaction" } as? UIAction, "the picker is no submenu: an action")
        XCTAssertNotNil(titles.first { $0.title == "More" } as? UIMenu, "a menu is still its submenu")
        var pressed: [UInt32] = []
        p.onPress = { pressed.append($0) }
        UIButton().sendAction(react)
        let picker = try XCTUnwrap(p.views[5])
        let end = Date(timeIntervalSinceNow: 5) // the picker opens a turn after the press
        while !p.menus.isOpen(picker) && Date() < end { RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.02)) }
        XCTAssertEqual(pressed, [3], "the row's own press")
        XCTAssertTrue(p.menus.isOpen(picker), "then the picker opens")
        // From the message the menu opened on: beside it, not where the menu's row was.
        let at = picker.convert(picker.bounds, to: window), message = try XCTUnwrap(p.views[1]).convert(p.views[1]!.bounds, to: window)
        XCTAssertLessThan(min(abs(at.minY - message.maxY), abs(at.maxY - message.minY)), 40, "anchored to the message: \(at) by \(message)")
        // Chosen again, the row toggles it closed, as its invoker would.
        UIButton().sendAction(react)
        let closing = Date(timeIntervalSinceNow: 5)
        while p.menus.isOpen(picker) && Date() < closing { RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.02)) }
        XCTAssertFalse(p.menus.isOpen(picker), "toggled closed")
    }

    /// A link row of a menu (`link href=…`, a button with an `href` and no
    /// `press`) is an item, as a button row is, and choosing it follows the
    /// link, as a press on it does: Exact Chat's contact menu of Call,
    /// FaceTime and Mail.
    func testAMenusLinkRowIsAnItemThatFollowsItsLink() throws {
        let session = ExactApp.shared.makeSession(label: "menu-link-row")
        defer { session.destroy(); window?.isHidden = true; window = nil }
        let opened = OpenedURLs()
        session.delegate = opened
        let p = presenter(session: session)
        p.apply(wireBatch([
            ["op": "create", "id": 110, "kind": "text", "props": ["text": "Call"]],
            ["op": "create", "id": 10, "kind": "button", "props": ["href": "tel:5550100"]],
            ["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 200.0, "h": 30.0], ["op": "children", "id": 10, "ids": [110]],
            ["op": "children", "id": 2, "ids": [3, 4, 10]],
        ]))
        let items = p.menus.items(of: try XCTUnwrap(p.views[2]), opener: p.views[1])
        let call = try XCTUnwrap(items.first { $0.title == "Call" } as? UIAction, "the link row is an action: \(items.map(\.title))")
        UIButton().sendAction(call)
        XCTAssertEqual(opened.urls, ["tel:5550100"])
    }

    private final class OpenedURLs: ExactSessionDelegate {
        var urls: [String] = []
        func exactSession(_ session: ExactSession, command: String, args: [Any]) {
            if command == "openURL", let url = args.first as? String { urls.append(url) }
        }
    }
}
#endif
