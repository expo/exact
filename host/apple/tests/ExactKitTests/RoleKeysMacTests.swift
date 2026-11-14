// @ref LLP 1166.000 D2 — a group Exact paints takes the kernel's keys on the
// Mac: a vertical tablist moves by Up and Down, wrapping, and by Home and
// End, pressing nothing; a horizontal one by Left and Right, mirrored right
// to left; a radio group presses as it moves, past an `aria-disabled` or a
// hidden radio, its native buttons taking the focus themselves. A key a
// `key` handler prevented does nothing more, and each group is one Tab stop
// where Tab reaches buttons (LLP 1115.000 D7.2).
#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

final class RoleKeysMacTests: XCTestCase {
    private var window: NSWindow?
    private var session: ExactSession?
    private var pressed: [String] = []

    override func tearDown() {
        window?.close(); window = nil
        session?.destroy(); session = nil
    }

    private static let app = """
    component App
      state size = "s"
      state tab = "a"
      state page = "x"
      state tip = "20"
      state prevented = 0
      action setSize(v: string)
        size = v
      action setTab(v: string)
        tab = v
      action setPage(v: string)
        page = v
      action setTip(v: string)
        tip = v
      action keys(k: string, e: KeyboardEvent)
        if k == "End"
          preventDefault()
          prevented = prevented + 1
      view
        column width="100%" height="100%"
          button press=setTab("a") testId="before"
            text "Before"
          row role="radiogroup" appearance="none" aria-label="Size" testId="sizes"
            for v in ["s", "m", "l"] key=v
              button press=setSize(v) appearance="none" role="radio" aria-checked=(v == size) aria-disabled=(v == "m") testId=`size-${v}`
                text v
          column role="tablist" aria-orientation="vertical" aria-label="Sections" testId="tabs"
            for v in ["a", "b", "c"] key=v
              button press=setTab(v) key=keys appearance="none" role="tab" aria-selected=(v == tab) testId=`tab-${v}`
                text v
          row role="tablist" appearance="none" direction="rtl" aria-label="Pages" testId="pages"
            for v in ["x", "y", "z"] key=v
              button press=setPage(v) appearance="none" role="tab" aria-selected=(v == page) testId=`page-${v}`
                text v
          row role="radiogroup" appearance="none" direction="rtl" aria-label="Tip" testId="tips"
            for v in ["10", "15"] key=v
              button press=setTip(v) role="radio" aria-checked=(v == tip) testId=`tip-${v}`
                text v
            button press=setTip("20") role="radio" aria-checked=(tip == "20") visibility="hidden" testId="tip-20"
              text "20"
          text `${prevented}` testId="prevented"
          button press=setTab("a") testId="after"
            text "After"
    """

    private func compile(_ source: String) throws -> Data {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let contract = dir.appendingPathComponent("app.contract"), plan = dir.appendingPathComponent("app.plan")
        try source.write(to: contract, atomically: true, encoding: .utf8)
        let compiler = Process(), said = Pipe()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
        compiler.arguments = ["build", contract.path, "-o", plan.path]
        compiler.standardError = said
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0, String(decoding: said.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self))
        return try Data(contentsOf: plan)
    }

    /// The app in a window of its own, every press it hears named in `pressed`.
    /// Without a plan, `app`'s; tests/fixtures/painted-groups.contract's is
    /// `groups()`.
    private func fixture(plan: Data? = nil) throws -> (Presenter, NSWindow) {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "role-keys-" + UUID().uuidString)
        self.session = session
        let view = ExactView(session: session)
        let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 500), styleMask: [.titled], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        w.autorecalculatesKeyViewLoop = false
        window = w
        let mount = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 500))
        w.contentView = mount
        view.frame = mount.bounds; mount.addSubview(view)
        XCTAssertNil(session.boot(plan: try plan ?? compile(Self.app), size: CGSize(width: 400, height: 500)).error)
        let p = session.presenter
        let press = p.onPress
        p.onPress = { [unowned self, unowned p] id in pressed.append(p.views[id]?.props["testId"] ?? "#\(id)"); press?(id) }
        return (p, w)
    }

    private func groups() throws -> (Presenter, NSWindow) {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_PAINTED_GROUPS_PLAN"], "build.mjs --test compiles tests/fixtures/painted-groups.contract")
        return try fixture(plan: try Data(contentsOf: URL(fileURLWithPath: path)))
    }

    private func node(_ p: Presenter, _ testId: String) throws -> NodeView {
        try XCTUnwrap(p.views.values.first { $0.props["testId"] == testId }, testId)
    }

    /// The node that has the focus, by its `testId`.
    private func focused(_ p: Presenter, _ w: NSWindow) -> String? { p.keyTarget(w.firstResponder)?.props["testId"] }

    /// A key as a person's arrives: the session's route (shortcuts, then the
    /// `key` handlers), and, unless that took it, the window's first responder.
    private func key(_ name: String, _ p: Presenter, _ w: NSWindow) throws {
        let (code, character): (UInt16, UInt32) = try XCTUnwrap([
            "ArrowLeft": (123, 0xF702), "ArrowRight": (124, 0xF703), "ArrowDown": (125, 0xF701), "ArrowUp": (126, 0xF700),
            "Home": (115, 0xF729), "End": (119, 0xF72B), "Tab": (48, 0x09), "Shift+Tab": (48, 0x19),
        ][name], name)
        let text = String(UnicodeScalar(character)!)
        let flags: NSEvent.ModifierFlags = name == "Tab" ? [] : name == "Shift+Tab" ? .shift : [.numericPad, .function]
        let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0, windowNumber: w.windowNumber,
                                                   context: nil, characters: text, charactersIgnoringModifiers: text, isARepeat: false, keyCode: code))
        p.flushKeyViewLoop()
        if !p.routeKey(event, focused: true, in: w) { w.sendEvent(event) }
    }

    /// The stops Tab visits from the page's start, once around, by `testId`.
    private func stops(_ p: Presenter) -> [String] {
        p.flushKeyViewLoop()
        var order: [String] = []
        var at = p.viewport.nextValidKeyView
        while let view = at, let id = p.keyTarget(view)?.props["testId"], !order.contains(id) {
            order.append(id)
            at = view.nextValidKeyView
        }
        return order
    }

    func testAPaintedVerticalTablistMovesByUpAndDownWrapsAndPressesNothing() throws {
        let (p, w) = try fixture()
        XCTAssertTrue(w.makeFirstResponder(try node(p, "tab-a")))
        var walk: [String?] = []
        for name in ["ArrowDown", "ArrowDown", "ArrowDown", "ArrowUp", "End", "Home", "ArrowRight", "ArrowLeft"] {
            try key(name, p, w)
            walk.append(focused(p, w))
        }
        // End is the author's: each tab's `key` handler prevents it.
        XCTAssertEqual(walk, ["tab-b", "tab-c", "tab-a", "tab-c", "tab-c", "tab-a", "tab-a", "tab-a"],
                       "Down and Up move and wrap, Home goes to the first, Left and Right are not a vertical tablist's")
        XCTAssertEqual(try node(p, "prevented").props["text"], "1", "the handler heard End")
        XCTAssertEqual(pressed, [], "a tab's press is the app's action: the arrows press nothing")
        XCTAssertEqual(try node(p, "tab-a").props["accessibilitySelected"], "true")
    }

    func testAPaintedHorizontalTablistMovesByLeftAndRightMirroredRightToLeft() throws {
        let (p, w) = try fixture()
        XCTAssertTrue(w.makeFirstResponder(try node(p, "page-x")))
        var walk: [String?] = []
        for name in ["ArrowLeft", "End", "Home", "ArrowRight", "ArrowLeft", "ArrowDown"] {
            try key(name, p, w)
            walk.append(focused(p, w))
        }
        XCTAssertEqual(walk, ["page-y", "page-z", "page-x", "page-z", "page-x", "page-x"],
                       "right to left, Left is the next tab and Right the previous, wrapping; Down is not a horizontal tablist's")
        XCTAssertEqual(pressed, [])
    }

    func testAPaintedRadioGroupPressesAsItMovesAndPassesAnAriaDisabledRadio() throws {
        let (p, w) = try fixture()
        XCTAssertTrue(w.makeFirstResponder(try node(p, "size-s")))
        try key("ArrowRight", p, w)
        XCTAssertEqual(focused(p, w), "size-l", "the aria-disabled radio is passed over")
        try key("ArrowDown", p, w)
        XCTAssertEqual(focused(p, w), "size-s", "and the last wraps to the first")
        try key("ArrowUp", p, w)
        XCTAssertEqual(focused(p, w), "size-l")
        try key("Home", p, w)
        XCTAssertEqual(focused(p, w), "size-l", "a radio group has no Home")
        XCTAssertEqual(pressed, ["size-l", "size-s", "size-l"], "each move presses the radio it reaches")
        waitFor("the last press checks its radio") { (try? node(p, "size-l").props["accessibilityChecked"]) == "true" }
        XCTAssertEqual(try node(p, "size-m").props["accessibilityChecked"], "false")
    }

    func testNativeRadiosOfAPaintedGroupTakeTheFocusThemselves() throws {
        let (p, w) = try fixture()
        let button = { (id: String) in try XCTUnwrap(p.controls.controls[try self.node(p, id).id] as? NativeButtonMac, id) }
        XCTAssertTrue(w.makeFirstResponder(try button("tip-10")))
        try key("ArrowLeft", p, w)
        XCTAssertTrue(w.firstResponder === (try button("tip-15")), "right to left, Left is the next radio, and its own NSButton takes the focus")
        try key("ArrowLeft", p, w)
        XCTAssertTrue(w.firstResponder === (try button("tip-10")), "the hidden radio is passed over")
        XCTAssertEqual(pressed, ["tip-15", "tip-10"])
    }

    func testEachPaintedGroupIsOneTabStopWhereTabReachesButtons() throws {
        keyboardNavigation(true)
        let (p, w) = try fixture()
        XCTAssertEqual(stops(p), ["before", "size-s", "tab-a", "page-x", "tip-10", "after"],
                       "each group's stop is its checked or selected member, else (its checked radio hidden) its first")
        // From a member the arrows reached, Tab leaves the group; back into
        // it, its stop is its selected tab again, which no arrow pressed.
        XCTAssertTrue(w.makeFirstResponder(try node(p, "tab-a")))
        try key("ArrowDown", p, w)
        XCTAssertEqual(focused(p, w), "tab-b")
        try key("Tab", p, w)
        XCTAssertEqual(focused(p, w), "page-x")
        XCTAssertEqual(stops(p), ["before", "size-s", "tab-a", "page-x", "tip-10", "after"])
        // Keyboard Navigation off, a tab is no stop, as a button is none.
        keyboardNavigation(false)
        XCTAssertEqual(stops(p), [])
    }

    /// An `input type="radio"` in a `role="radiogroup"` is HTML's radio, its
    /// group its `name` (LLP 1166.000 D2): the checked one is the stop, and an
    /// arrow checks the next, its `change` heard once, pressing nothing.
    func testInputRadiosInARadiogroupKeepHTMLsKeys() throws {
        keyboardNavigation(true)
        let (p, w) = try groups()
        XCTAssertEqual(stops(p).filter { $0.hasPrefix("size-") }, ["size-m"], "the checked input is its group's one stop")
        XCTAssertTrue(w.makeFirstResponder(try node(p, "size-m")))
        try key("ArrowDown", p, w)
        XCTAssertEqual(focused(p, w), "size-l")
        waitFor("the arrow's change lands") { (try? node(p, "state").props["text"]) == "l 1 0 0" }
        XCTAssertEqual(try node(p, "size-l").props["checked"], "true")
        XCTAssertEqual(pressed, [], "a radio input is checked, never pressed")
    }

    /// A menu Exact paints in the page (LLP 1166.000 D1, the `menu` row): Up
    /// and Down move the focus among its items, wrapping, Home and End to its
    /// ends, pressing nothing; Left and Right are not its keys; a key an
    /// item's `key` handler prevented (End here) does nothing more. It sets
    /// no Tab stop: each item keeps a button's own.
    func testAPaintedPageMenuMovesByUpDownHomeAndEndAndPressesNothing() throws {
        keyboardNavigation(true)
        let (p, w) = try groups()
        XCTAssertEqual(stops(p).filter { $0.hasPrefix("item-") }, ["item-Cut", "item-Copy", "item-Paste"], "a menu sets no stop")
        XCTAssertTrue(w.makeFirstResponder(p.keyView(of: try node(p, "item-Cut"))))
        var walk: [String?] = []
        for name in ["ArrowDown", "ArrowDown", "ArrowDown", "ArrowUp", "Home", "End", "ArrowRight"] {
            try key(name, p, w)
            walk.append(focused(p, w))
        }
        XCTAssertEqual(walk, ["item-Copy", "item-Paste", "item-Cut", "item-Paste", "item-Cut", "item-Cut", "item-Cut"])
        waitFor("the handler heard End") { (try? node(p, "state").props["text"]) == "m 0 0 1" }
        XCTAssertEqual(pressed, [], "an item's press is the app's action: the arrows press nothing")
    }

    /// Tab and Shift-Tab from a member the arrows reached go on from it, to
    /// the stops after and before its group, though while it has the focus
    /// the member the stops began at is no stop: one with `tabindex=0` and
    /// one with `tabindex=-1`, which is never a stop itself.
    func testTabFromAMemberTheArrowsReachedGoesOnFromIt() throws {
        keyboardNavigation(true)
        let (p, w) = try groups()
        for (to, name, expected) in [("stop-B", "Shift+Tab", "before"), ("stop-B", "Tab", "after"), ("stop-C", "Shift+Tab", "before"), ("stop-C", "Tab", "after")] {
            XCTAssertTrue(w.makeFirstResponder(try node(p, "stop-A")))
            try key("ArrowRight", p, w)
            if to == "stop-C" { try key("ArrowRight", p, w) }
            XCTAssertEqual(focused(p, w), to)
            try key(name, p, w)
            XCTAssertEqual(focused(p, w), expected, "\(name) from \(to)")
        }
    }

    /// A link in a line of text inside a menu is a member, as the web counts
    /// an `a[href]` (LLP 1166.000 D1): the arrows reach it from the buttons
    /// on either side and leave it to them, Home and End go on from it, and
    /// a key its `key` handler prevented (End) does nothing more.
    func testAMenusInlineLinkIsAMemberTheArrowsReachAndLeave() throws {
        keyboardNavigation(true)
        let (p, w) = try groups()
        let note = try node(p, "edit-note")
        let onLink = { (w.firstResponder as? InlineKeyView)?.owner === note }
        XCTAssertTrue(w.makeFirstResponder(p.keyView(of: try node(p, "edit-cut"))))
        try key("ArrowDown", p, w)
        XCTAssertTrue(onLink(), "Down from Cut: the link")
        try key("ArrowDown", p, w)
        XCTAssertEqual(focused(p, w), "edit-paste", "Down from the link: Paste")
        try key("ArrowUp", p, w)
        XCTAssertTrue(onLink(), "Up from Paste: the link")
        try key("ArrowUp", p, w)
        XCTAssertEqual(focused(p, w), "edit-cut", "Up from the link: Cut")
        // The link Tab reached, with Keyboard Navigation on.
        try key("Tab", p, w)
        XCTAssertTrue(onLink(), "Tab from Cut: the link, a stop of its own")
        try key("End", p, w)
        XCTAssertTrue(onLink(), "End is its handler's")
        waitFor("the handler heard End") { (try? node(p, "state").props["text"]) == "m 0 0 1" }
        try key("Home", p, w)
        XCTAssertEqual(focused(p, w), "edit-cut")
        XCTAssertEqual(pressed, [], "nothing pressed")
    }

    /// HTML goes on from a focused element outside the sequential order by
    /// its place in tree order: from a focused member that can never be a
    /// stop (`tabindex=-1`) between a `tabindex=1` and a `tabindex=2`, Tab
    /// reaches the `tabindex=2` after it and Shift-Tab the `tabindex=1`
    /// before it, not the stops the positive `tabindex`es put first.
    func testTabFromAMemberOutsideTheOrderGoesOnByItsPlaceInTheTree() throws {
        keyboardNavigation(true)
        let (p, w) = try fixture(plan: try compile("""
        component App
          state tab = "a"
          action setTab(v: string)
            tab = v
          view
            column width="100%" height="100%"
              button press=setTab("a") tabindex=1 testId="before"
                text "Before"
              row role="tablist" appearance="none" aria-label="Tabs" testId="tabs"
                button press=setTab("a") appearance="none" role="tab" aria-selected=(tab == "a") testId="tab-a"
                  text "A"
                button press=setTab("b") appearance="none" role="tab" tabindex=-1 testId="tab-b"
                  text "B"
              button press=setTab("a") tabindex=2 testId="after"
                text "After"
              button press=setTab("a") tabindex=0 testId="tail"
                text "Tail"
        """))
        for (name, expected) in [("Tab", "after"), ("Shift+Tab", "before")] {
            XCTAssertTrue(w.makeFirstResponder(try node(p, "tab-a")))
            try key("ArrowRight", p, w)
            XCTAssertEqual(focused(p, w), "tab-b")
            try key(name, p, w)
            XCTAssertEqual(focused(p, w), expected, name)
        }
    }
}
#endif
