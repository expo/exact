#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// LLP 1059 D2a on AppKit: a tablist projected to `NSSegmentedControl`
/// reports the control's height as its automatic minimum, as on iOS, so an
/// unsized row of text tabs does not squash the control.
final class SegmentsMacTests: XCTestCase {
    func testARIAUnavailableTabStaysOperableInNativeSegments() throws {
        _ = NSApplication.shared
        let p = Presenter()
        let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 200), styleMask: [.titled], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        w.contentView = p.viewport
        defer { p.reset(); w.close() }
        p.apply(wireBatch([
            ["op": "create", "id": 10, "kind": "view", "props": ["accessibilityRole": "tablist"]],
            ["op": "create", "id": 11, "kind": "button", "props": ["accessibilityRole": "tab", "testId": "one", "accessibilityDisabled": "true"], "handlers": ["press"]],
            ["op": "create", "id": 12, "kind": "button", "props": ["accessibilityRole": "tab", "testId": "two"], "handlers": ["press"]],
            ["op": "create", "id": 13, "kind": "text", "props": ["text": "One"]],
            ["op": "create", "id": 14, "kind": "text", "props": ["text": "Two"]],
            ["op": "children", "id": 11, "ids": [13]],
            ["op": "children", "id": 12, "ids": [14]],
            ["op": "children", "id": 10, "ids": [11, 12]],
            ["op": "roots", "ids": [10]],
            ["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 400.0, "h": 40.0],
        ]))
        let control = try XCTUnwrap(p.segments.control(of: 10))
        XCTAssertTrue(control.isEnabled(forSegment: 0), "ARIA does not suppress the native action")
        let top = try XCTUnwrap(control.accessibilityChildren() as? [NSObject])
        let children = top.count == 1
            ? (top[0].accessibilityAttributeValue(.children) as? [NSObject] ?? []) : top
        XCTAssertEqual(children.count, 2)
        XCTAssertEqual(children[0].accessibilityAttributeValue(.role) as? String, "AXRadioButton")
        XCTAssertEqual(children[0].accessibilityAttributeValue(.description) as? String, "One")
        XCTAssertTrue(children[0].accessibilityActionNames().contains(.press))
        XCTAssertEqual(children[0].accessibilityAttributeValue(.enabled) as? Bool, false, "VoiceOver's projected segment carries aria-disabled")
        XCTAssertEqual(children[1].accessibilityAttributeValue(.enabled) as? Bool, true)
        XCTAssertEqual(children[0].accessibilityAttributeValue(.identifier) as? String, "one")
        XCTAssertEqual(children[1].accessibilityAttributeValue(.identifier) as? String, "two")
        let ax = p.axElements(roots: [p.viewport])
        let projected = ax["elements"] as? [[String: Any]] ?? []
        XCTAssertEqual(projected.first { $0["name"] as? String == "One" }?["id"] as? UInt32, 11)
        XCTAssertEqual(projected.first { $0["name"] as? String == "Two" }?["id"] as? UInt32, 12)
        XCTAssertFalse(((ax["coverage"] as? [String: Any])?["limits"] as? [String] ?? []).contains("segments"))
        for prop in ["accessibilityElementsHidden", "inert"] {
            p.apply(wireBatch([["op": "props", "id": 10, "set": [prop: "true"]]]))
            let suppressed = p.axElements(roots: [p.viewport])["elements"] as? [[String: Any]] ?? []
            XCTAssertFalse(suppressed.contains { [UInt32(11), 12].contains($0["id"] as? UInt32 ?? 0) })
            p.apply(wireBatch([["op": "props", "id": 10, "clear": [prop]]]))
            let restored = p.axElements(roots: [p.viewport])["elements"] as? [[String: Any]] ?? []
            XCTAssertEqual(Set(restored.compactMap { $0["testId"] as? String }), ["one", "two"])
        }
        var pressed: [UInt32] = []
        p.onPress = { pressed.append($0) }
        XCTAssertTrue(p.segments.activate(try XCTUnwrap(p.views[11])) == true)
        XCTAssertEqual(pressed, [11])
        p.apply(wireBatch([["op": "props", "id": 11, "set": ["accessibilityDisabled": "false"]]]))
        XCTAssertEqual(children[0].accessibilityAttributeValue(.enabled) as? Bool, true, "clearing ARIA restores AXEnabled")
    }

    func testAppearanceNoneRestoresAuthoredTabsAndTheirActions() throws {
        for (role, group, member) in [("tablist", "tabs", "tab-"), ("radiogroup", "pair", "pair-")] { try appearanceNone(role, group, member) }
    }

    /// The presses the presenter heard, in order, before they reached the runner.
    private var pressed: [UInt32] = []

    /// tests/fixtures/painted-groups.contract in a window: a radio group's or
    /// a tablist's keys and Tab stop are the kernel's (LLP 1166.000), so the
    /// groups are booted, not written as a batch. `node` finds a node by its
    /// `testId`; `act` presses one of the fixture's own buttons in the runner,
    /// changing what its groups show.
    private func paintedGroups() throws -> (p: Presenter, w: NSWindow, node: (String) throws -> NodeView, act: (String) throws -> Void) {
        _ = NSApplication.shared
        let plan = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_PAINTED_GROUPS_PLAN"], "build.mjs --test compiles tests/fixtures/painted-groups.contract")
        let session = ExactApp.shared.makeSession(label: "painted-groups-" + UUID().uuidString)
        let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        let mount = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 400)), view = ExactView(session: session)
        w.contentView = mount
        view.frame = mount.bounds; mount.addSubview(view)
        addTeardownBlock { w.close(); session.destroy() }
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: plan)), size: CGSize(width: 400, height: 400)).error)
        let p = session.presenter, press = p.onPress
        p.onPress = { [unowned self] id in pressed.append(id); press?(id) }
        let node = { (testId: String) in try XCTUnwrap(p.views.values.first { $0.props["testId"] == testId }, testId) }
        return (p, w, node, { session.apply(session.runtime.press(try node($0).id, now: session.now())) })
    }

    /// An authored radio group of native buttons: an arrow at one's own
    /// `NSButton` presses the next and gives that one's `NSButton` the focus;
    /// a right-to-left group swaps Left and Right; a hidden radio is neither
    /// the Tab stop nor an arrow's target.
    func testAnAuthoredGroupOfNativeRadiosMovesTheirOwnFocus() throws {
        let (p, w, node, act) = try paintedGroups()
        try act("paint")
        XCTAssertNil(p.segments.control(of: try node("period").id))
        let (day, week, month) = (try node("period-Day").id, try node("period-Week").id, try node("period-Month").id)
        let button = { (id: UInt32) in try XCTUnwrap(p.controls.controls[id] as? NativeButtonMac) }
        func arrow(_ code: UInt16, at id: UInt32) throws {
            let character = String(UnicodeScalar([123: 0xF702, 124: 0xF703, 125: 0xF701, 126: 0xF700][Int(code)]!)!)
            let event = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [.numericPad, .function], timestamp: 0, windowNumber: w.windowNumber,
                                         context: nil, characters: character, charactersIgnoringModifiers: character, isARepeat: false, keyCode: code)!
            try button(id).keyDown(with: event)
        }
        XCTAssertTrue(w.makeFirstResponder(try button(day)))
        try arrow(124, at: day)
        XCTAssertEqual(pressed, [week])
        XCTAssertTrue(w.firstResponder === (try button(week)), "the pressed radio's own NSButton takes the focus")
        try act("flip")
        pressed = []
        try arrow(124, at: week)
        try arrow(123, at: week)
        XCTAssertEqual(pressed, [day, month], "right-to-left: Right is the previous radio, Left the next")
        try act("flip")
        try act("hide")
        try act("choose-Month")
        pressed = []
        try arrow(125, at: week)
        XCTAssertEqual(pressed, [day], "the hidden radio is passed over")
        // While a member has the focus it is the stop whatever is checked
        // (LLP 1166.000 D1): the focus leaves the group first.
        XCTAssertTrue(w.makeFirstResponder(nil))
        XCTAssertEqual([day, week, month].map { p.views[$0]!.ariaRadioTabStop }, [true, false, false], "a hidden checked radio is not the stop")
    }

    private func appearanceNone(_ role: String, _ group: String, _ member: String) throws {
        let (p, _, node, act) = try paintedGroups()
        let owner = try node(group).id
        XCTAssertNotNil(p.segments.control(of: owner))
        try act("paint")
        XCTAssertNil(p.segments.control(of: owner))
        let (one, two) = (try node(member + "One"), try node(member + "Two"))
        XCTAssertFalse(one.isHidden)
        pressed = []
        p.press(one.id)
        XCTAssertEqual(pressed, [one.id])
        // An authored radio group keeps WAI-ARIA's keys: one Tab stop, and the
        // arrows press the next or previous radio, wrapping.
        if role == "radiogroup" {
            XCTAssertEqual([one, two].map(\.ariaRadioTabStop), [true, false], "none checked: the first is the stop")
            XCTAssertFalse(two.tabbable)
            pressed = []
            XCTAssertTrue(p.controls.roleKey(one, "ArrowDown", held: ""))
            XCTAssertTrue(p.controls.roleKey(one, "ArrowUp", held: ""))
            XCTAssertEqual(pressed, [two.id, two.id])
        } else {
            XCTAssertNil(one.ariaRadioGroup)
        }
        try act("paint")
        XCTAssertNotNil(p.segments.control(of: owner))
    }

    private func drain() {
        let delivered = expectation(description: "intrinsic size delivered after the batch")
        DispatchQueue.main.async { delivered.fulfill() }
        wait(for: [delivered], timeout: 2)
    }

    func testSegmentsReportNativeHeightWithoutRepeatedOrStaleMeasurements() throws {
        _ = NSApplication.shared
        let p = Presenter()
        let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 200), styleMask: [.titled], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        w.contentView = p.viewport
        defer { p.reset(); w.close() }
        var reports: [CGSize?] = []
        p.onIntrinsic = { sizes in
            for (id, size) in sizes where id == 10 { reports.append(size) }
        }
        p.apply(wireBatch([
            ["op": "create", "id": 10, "kind": "view", "props": ["accessibilityRole": "tablist"]],
            ["op": "create", "id": 11, "kind": "button", "props": ["accessibilityRole": "tab", "accessibilitySelected": "true"], "handlers": ["press"]],
            ["op": "create", "id": 12, "kind": "button", "props": ["accessibilityRole": "tab"], "handlers": ["press"]],
            ["op": "create", "id": 14, "kind": "text", "props": ["text": "One"]],
            ["op": "create", "id": 16, "kind": "text", "props": ["text": "Two"]],
            ["op": "children", "id": 11, "ids": [14]],
            ["op": "children", "id": 12, "ids": [16]],
            ["op": "children", "id": 10, "ids": [11, 12]],
            ["op": "roots", "ids": [10]],
            ["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 400.0, "h": 16.0],
        ]))
        let owner = try XCTUnwrap(p.views[10])
        let control = try XCTUnwrap(p.segments.control(of: 10))
        drain()
        let size = try XCTUnwrap(reports.last ?? nil)
        XCTAssertEqual(size, control.intrinsicContentSize, "the control's own size, not the box's")
        XCTAssertGreaterThan(size.height, 16, "an unsized text row must not squash the native control")
        XCTAssertEqual(control.frame, owner.contentBox(), "the kernel still owns the final box")
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0,
                            "w": 400.0, "h": Double(size.height)]]))
        drain()
        let count = reports.count
        p.segments.sync()
        drain()
        XCTAssertEqual(reports.count, count, "unchanged native sizes are not published again")
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0,
                            "w": 300.0, "h": Double(size.height)]]))
        drain()
        XCTAssertEqual(reports.count, count, "its height does not follow the box's width, so a resize does not remeasure")
        XCTAssertEqual(control.frame, owner.contentBox())
        // The control fills the content box: a border-box minimum also holds
        // the padding and border around it; a content-box one does not.
        p.apply(wireBatch([["op": "style", "id": 10,
                            "style": ["box_sizing": "border-box", "padding_top": 6.0, "padding_bottom": 4.0]],
                           contentOp(10, 300, Double(size.height), padding: [6, 0, 4, 0])]))
        drain()
        XCTAssertEqual((reports.last ?? nil)?.height, size.height + 10)
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0,
                            "w": 300.0, "h": Double(size.height + 10)],
                           contentOp(10, 300, Double(size.height + 10), padding: [6, 0, 4, 0])]))
        XCTAssertEqual(control.frame, owner.contentBox())
        XCTAssertEqual(control.frame.height, size.height, "the padding does not squash the control")
        p.apply(wireBatch([["op": "style", "id": 10,
                            "style": ["box_sizing": "content-box", "padding_top": 6.0, "padding_bottom": 4.0]]]))
        drain()
        XCTAssertEqual((reports.last ?? nil)?.height, size.height)
        // Measured at zero width too: tabs replaced there still reserve it.
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0],
                           ["op": "children", "id": 10, "ids": [12, 11]]]))
        drain()
        XCTAssertEqual((reports.last ?? nil)?.height, size.height, "a zero-width tablist keeps its native minimum")
        let replaced = try XCTUnwrap(p.segments.control(of: 10))
        p.apply(wireBatch([["op": "props", "id": 10,
                            "set": ["accessibilityRole": "tablist", "accessibilityOrientation": "vertical"]]]))
        drain()
        XCTAssertNil(reports.last ?? nil, "leaving the projection clears its native minimum")
        XCTAssertNil(control.superview)
        XCTAssertNil(replaced.superview)
        XCTAssertFalse(try XCTUnwrap(p.views[11]).isHidden)
    }
}
#endif
