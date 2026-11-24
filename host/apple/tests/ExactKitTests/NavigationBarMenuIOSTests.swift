#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// A bar item's pull-down reports its popover's `toggle` on the edges public
/// API shows (expo/exact#1462, `MenuHost.barMenuOpened`).
extension NavigationBarIOSTests {
    private func slot(_ session: ExactSession, _ name: String) -> Any? {
        let text = session.agent(#"{"op":"state"}"#)
        let json = try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
        return (json?["slots"] as? [String: Any])?[name]
    }
    private func toggles(_ session: ExactSession) -> String { slot(session, "barMenuToggles") as? String ?? "" }

    /// The host's side of each edge, with UIKit's menu stood in for: the
    /// rows' read opens it; a pick, a touch on the page, `hidePopover`, its
    /// button leaving the bar and the next opening close it.
    func testABarPullDownReportsEachEdgeItCanKnow() throws {
        let session = try fixture("bar-menu-toggle", module: false)
        let menus = session.presenter.menus
        let pop = try XCTUnwrap(session.presenter.carrying("popover").first { $0.props["id"] == "bar-menu" })
        let invoker = try node(session, "menu-item")
        let bar = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation?.navigationBar)
        func open() { menus.barMenuOpened(pop, from: invoker, in: { bar }, afterClose: false) }
        func expect(_ edges: String, _ what: String) {
            until(what) { self.toggles(session) == edges }
        }
        open()
        XCTAssertTrue(menus.isOpen(pop), "open while UIKit shows it")
        expect("closed>open;", "the rows' read opens it")
        let count = slot(session, "count") as? Double ?? -1
        let row = try XCTUnwrap(menus.items(of: pop).first as? UIAction)
        row.performWithSender(nil, target: nil)
        XCTAssertFalse(menus.isOpen(pop))
        expect("closed>open;open>closed;", "a pick closes it")
        until("and presses its row") { self.slot(session, "count") as? Double == count + 1 }

        // Each opening is reported before it closes: changes in one turn coalesce.
        open()
        expect("closed>open;open>closed;closed>open;", "it opens again")
        menus.lightDismiss(try node(session, "fixture"))
        expect("closed>open;open>closed;closed>open;open>closed;", "a touch on the page closes it")

        open()
        expect("closed>open;open>closed;closed>open;open>closed;closed>open;", "it opens again")
        menus.action("hidePopover", ["bar-menu"])
        XCTAssertFalse(menus.isOpen(pop))
        expect("closed>open;open>closed;closed>open;open>closed;closed>open;open>closed;", "`hidePopover` closes it")
        menus.action("hidePopover", ["bar-menu"])
        menus.lightDismiss(try node(session, "fixture"))
        spin(0.1)
        XCTAssertEqual(toggles(session).components(separatedBy: ";").count, 7, "a closed menu closes once")

        func edges() -> Int { toggles(session).components(separatedBy: ";").count - 1 }
        open()
        until("it opens again") { edges() == 7 }
        session.presenter.apply(wireBatch([["op": "props", "id": Int(invoker.id), "set": ["popovertargetaction": "hide"]]]))
        XCTAssertFalse(menus.isOpen(pop), "a button that no longer opens it closes it")
        until("its close reported") { edges() == 8 && self.toggles(session).hasSuffix("open>closed;") }
        session.presenter.apply(wireBatch([["op": "props", "id": Int(invoker.id), "set": ["popovertargetaction": "toggle"]]]))

        // A close nothing reported (Escape) is reported as the next opening's rows are read, on the turn before it.
        open()
        until("it opens again") { edges() == 9 }
        let stale = menus.barMenuClosed()
        menus.barMenuOpened(pop, from: invoker, in: { bar }, afterClose: stale)
        XCTAssertTrue(stale && menus.isOpen(pop))
        until("the close and the opening are two events") { edges() == 11 && self.toggles(session).hasSuffix("open>closed;closed>open;") }
    }

    /// UIKit's own menu under real touches: a tap on the item opens it, a tap
    /// on the page and `hidePopover` close it, and a pick closes it.
    func testABarPullDownReportsUIKitsMenuUnderATouch() throws {
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "build.mjs --test --ios starts the XCTest touch runner")
        }
        let touch = try TouchRunner(self)
        defer { touch.connection.cancel() }
        let session = try fixture("bar-menu-touch", module: false)
        let window = try XCTUnwrap(self.window), menus = session.presenter.menus
        let pop = try XCTUnwrap(session.presenter.carrying("popover").first { $0.props["id"] == "bar-menu" })
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        let item = try XCTUnwrap((nav.topViewController?.navigationItem.leftBarButtonItems ?? []).first { $0.accessibilityLabel == "More" })
        func tap(_ point: CGPoint) throws {
            var request = try TouchRunner.aim(point, in: window)
            request["op"] = "tap"
            XCTAssertEqual(try touch.ask(request)["done"] as? Bool, true)
        }
        func rowShown() -> MenuRow? { Agent.menuGroups(in: window).flatMap(\.rows).first { $0.label == "Bump from the bar" } }
        func openByTouch(_ edges: String) throws {
            let frame = try XCTUnwrap(item.frame(in: nav.navigationBar))
            try tap(nav.navigationBar.convert(CGPoint(x: frame.midX, y: frame.midY), to: window))
            waitFor("the item's tap opens UIKit's menu", cap: 5) { menus.isOpen(pop) && rowShown() != nil }
            waitFor("and reports it", cap: 5) { self.toggles(session) == edges }
        }
        let page = try node(session, "fixture")
        let outside = page.convert(CGPoint(x: page.bounds.midX, y: page.bounds.maxY - 20), to: window)

        try openByTouch("closed>open;")
        try tap(outside)
        waitFor("a tap on the page closes it", cap: 5) { self.toggles(session) == "closed>open;open>closed;" && rowShown() == nil }

        try openByTouch("closed>open;open>closed;closed>open;")
        menus.action("hidePopover", ["bar-menu"])
        waitFor("`hidePopover` dismisses UIKit's menu", cap: 5) { rowShown() == nil }
        waitFor("and reports it", cap: 5) { self.toggles(session).hasSuffix("closed>open;open>closed;") && self.toggles(session).count == 48 }

        try openByTouch(toggles(session) + "closed>open;")
        let row = try XCTUnwrap(rowShown())
        try tap(CGPoint(x: row.box.midX, y: row.box.midY))
        waitFor("a pick closes it", cap: 5) { self.toggles(session).hasSuffix("open>closed;") && rowShown() == nil && !menus.isOpen(pop) }
    }
}

private typealias MenuRow = Agent.MenuRow
#endif
