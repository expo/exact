#if os(macOS)
import AppKit
import XCTest
@testable import ExactCore
@testable import ExactKit
@testable import ExactAnimations
@testable import ExactNativeViews
@testable import ExactSurfaces

/// A large batch prepared over frames and published in one turn (LLP 1072 T13).
final class StagedApplyMacTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() } // LLP 1047.001 D4
    private var window: NSWindow?
    private var minimum = 0
    override func setUp() { super.setUp(); minimum = StagePlan.minimumOps; StagePlan.minimumOps = 10 }
    override func tearDown() { StagePlan.minimumOps = minimum; window?.close(); window = nil }

    private func show(_ viewport: NSView) {
        _ = NSApplication.shared
        viewport.frame = NSRect(x: 0, y: 0, width: 400, height: 600)
        let window = NSWindow(contentRect: viewport.frame, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = viewport
        self.window?.close()
        self.window = window
    }

    /// Root 9001 holds an old block 9002 (text 9003) and boxes 9004 and 9005.
    private let tree: [[String: Any]] = [9001, 9002, 9004, 9005].map { ["op": "create", "id": $0, "kind": "view", "style": [String: Any]()] } + [
        ["op": "create", "id": 9003, "kind": "text", "style": [String: Any](), "props": ["text": "old"]],
        ["op": "children", "id": 9001, "ids": [9002, 9004, 9005]], ["op": "children", "id": 9002, "ids": [9003]], ["op": "roots", "ids": [9001]],
        ["op": "frame", "id": 9001, "x": 0.0, "y": 0.0, "w": 400.0, "h": 600.0], ["op": "frame", "id": 9002, "x": 0.0, "y": 0.0, "w": 400.0, "h": 100.0],
        ["op": "frame", "id": 9003, "x": 0.0, "y": 0.0, "w": 400.0, "h": 20.0], ["op": "frame", "id": 9004, "x": 0.0, "y": 500.0, "w": 10.0, "h": 10.0],
        ["op": "frame", "id": 9005, "x": 20.0, "y": 500.0, "w": 10.0, "h": 10.0],
    ]

    /// Replaces block 9002 with block 9100 of `rows` rows (box and text),
    /// writes box 9004's title, and samples box 9004's translate. Every box
    /// it makes stands `offset` in from its parent; `row` adds props to row 1.
    private func replacement(rows: Int = 40, offset: Double = 0, row: [String: String] = [:], title: String = "new", translate: Double = 1) -> Batch {
        let extra = row
        var ops: [[String: Any]] = [["op": "destroy", "id": 9003], ["op": "destroy", "id": 9002],
                                    ["op": "create", "id": 9100, "kind": "view", "style": [String: Any]()]]
        var kids: [UInt32] = []
        for i in 0..<rows {
            let row = UInt32(9200 + 2 * i)
            kids.append(row)
            let props = i == 0 ? ["hatch": "map"] : i == 1 ? extra : [:]
            ops += [["op": "create", "id": row, "kind": "view", "style": [String: Any](), "props": props],
                    ["op": "create", "id": row + 1, "kind": "text", "style": [String: Any](), "props": ["text": "row \(i)"]],
                    ["op": "children", "id": row, "ids": [row + 1]],
                    ["op": "frame", "id": row, "x": offset, "y": Double(i) * 10 + offset, "w": 400.0 - offset, "h": 10.0],
                    ["op": "frame", "id": row + 1, "x": offset, "y": offset, "w": 25.0 + offset, "h": 10.0 - offset]]
        }
        ops += [["op": "children", "id": 9100, "ids": kids], ["op": "children", "id": 9001, "ids": [9100, 9004, 9005]],
                ["op": "frame", "id": 9100, "x": offset, "y": offset, "w": 400.0, "h": Double(rows) * 10],
                ["op": "props", "id": 9004, "set": ["title": title]],
                ["op": "present", "id": 9004, "property": "translate", "x": translate, "y": 0.0, "w": 0.0, "h": 0.0]]
        return wireBatch(ops)
    }

    func testAStagedBatchShowsNothingUntilItsRevealAndAllOfItThen() throws {
        let p = Presenter()
        show(p.viewport)
        p.apply(wireBatch(tree))
        let plan = try XCTUnwrap(p.stagePlan(replacement()))
        p.prepare(plan, until: 0) // one slice: some of the rows
        XCTAssertGreaterThan(plan.cursor, 0)
        XCTAssertFalse(plan.preparedAll)
        let top = try XCTUnwrap(plan.made[9100])
        XCTAssertTrue(top.superview === p.views[9001]?.container, "built where it will stand")
        XCTAssertNotNil(top.offstageAlpha)
        XCTAssertEqual(top.layer?.opacity ?? 1, 0, "drawn at no opacity")
        XCTAssertNil(p.views[9100], "out of the table between slices")
        XCTAssertNil(p.root.hitTest(NSPoint(x: 5, y: 5)).flatMap { $0 as? NodeView }.flatMap { plan.made[$0.id] }, "nothing hits it")
        while !plan.preparedAll { p.prepare(plan, until: .infinity) }

        // Nothing that reads the committed tree finds it: focus, names, hatches.
        let row = try XCTUnwrap(plan.made[9201])
        XCTAssertTrue(row.held)
        XCTAssertFalse(row.acceptsFirstResponder, "Tab cannot reach it")
        XCTAssertFalse(row.isAccessibilityElement())
        let live = try XCTUnwrap(p.views[9001])
        XCTAssertEqual(live.accessibleText, "old", "its parent's name reads the old words only")
        XCTAssertEqual(plan.afterReveal.count, 1, "the hatch's build waits with the stage")
        p.apply(wireBatch([["op": "present", "id": 9005, "property": "opacity", "x": 0.5, "y": 0.0, "w": 0.0, "h": 0.0]]))
        XCTAssertEqual(plan.afterReveal.count, 1, "a batch that passes leaves it waiting")
        XCTAssertNotNil(p.views[9002], "the old block shows until the reveal")
        XCTAssertNotNil(p.views[9003])
        XCTAssertNil(p.views[9004]?.props["title"], "and so do the live boxes' old values")

        p.revealing = plan
        p.apply(plan.reveal())
        p.revealing = nil
        XCTAssertNotNil(p.views[9100])
        XCTAssertNil(top.offstageAlpha)
        XCTAssertEqual(top.alphaValue, 1)
        XCTAssertEqual(p.views.keys.filter { (9200..<9280).contains($0) }.count, 80, "every row at once")
        XCTAssertNil(p.views[9002], "the old block's identity is gone with the reveal")
        XCTAssertNil(p.views[9003])
        XCTAssertEqual(p.views[9004]?.props["title"], "new")
        XCTAssertFalse(row.held)
        XCTAssertTrue(row.acceptsFirstResponder)
        XCTAssertTrue(live.accessibleText.hasPrefix("row 0 row 1"))
        XCTAssertFalse(live.accessibleText.contains("old"), "the old words, still in the window until taken down, name nothing")
        XCTAssertTrue(plan.afterReveal.isEmpty && p.afterBatchWork.isEmpty, "the hatch's build ran with the reveal")
    }

    func testABatchLeavesAStagedOneAloneOnlyWhereItTouchesNothingThatOneTouches() throws {
        let p = Presenter()
        show(p.viewport)
        p.apply(wireBatch(tree))
        let plan = try XCTUnwrap(p.stagePlan(replacement()))
        XCTAssertTrue(plan.disjoint(wireBatch([["op": "present", "id": 9005, "property": "opacity", "x": 0.5, "y": 0.0, "w": 0.0, "h": 0.0]])), "a box it leaves alone")
        XCTAssertFalse(plan.disjoint(wireBatch([["op": "props", "id": 9004, "set": ["title": "tick"]]])), "a property it writes")
        XCTAssertTrue(plan.disjoint(wireBatch([["op": "props", "id": 9004, "set": ["lang": "fr"]]])), "another property of a box it writes")
        XCTAssertFalse(plan.disjoint(wireBatch([["op": "present", "id": 9002, "property": "opacity", "x": 0.5, "y": 0.0, "w": 0.0, "h": 0.0]])), "a box it destroys")
        XCTAssertFalse(plan.disjoint(wireBatch([["op": "frame", "id": 9001, "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0]])), "a box whose children it moves")
        let sample = wireBatch([["op": "present", "id": 9004, "property": "translate", "x": 9.0, "y": 0.0, "w": 0.0, "h": 0.0]])
        XCTAssertTrue(plan.disjoint(sample), "a later sample of motion it samples")
        XCTAssertTrue(plan.superseded.isEmpty, "asking records nothing")
        plan.passed(sample)
        XCTAssertFalse(plan.reveal().ops.contains { $0.op == .present && $0.id == 9004 }, "the reveal leaves its earlier sample out")


        // A paint sample, which Apple's runtime sends in the node's whole style, passes with the
        // batch's own values withheld; the reveal gives the newest style, the batch's values with it.
        var styling = replacement()
        styling.ops += wireBatch([paint(9005, ["opacity": 0.5, "cursor": "pointer"])]).ops
        let painted = try XCTUnwrap(p.stagePlan(styling))
        let paintSample = wireBatch([paint(9005, ["opacity": 0.6, "cursor": "pointer"])])
        XCTAssertTrue(painted.disjoint(paintSample), "a newer paint sample")
        let shown = painted.withheld(paintSample) { p.views[$0]?.style }.ops[0].style
        XCTAssertEqual(shown["opacity"]?.number, 0.6, "the sample shows at once")
        XCTAssertNil(shown["cursor"], "the batch's cursor waits for its reveal")
        painted.passed(paintSample)
        let revealed = try XCTUnwrap(painted.reveal().ops.first { $0.op == .style && $0.id == 9005 }).style
        XCTAssertEqual(revealed["opacity"]?.number, 0.6, "an older style never overwrites the newer sample")
        XCTAssertNotNil(revealed["cursor"])

        // A batch that removes a value the node shows: a later sample, without it, keeps it until the reveal.
        p.apply(wireBatch([paint(9005, ["cursor": "pointer"])]))
        var removing = replacement()
        removing.ops += wireBatch([paint(9005, ["opacity": 0.5])]).ops
        let removal = try XCTUnwrap(p.stagePlan(removing))
        let later = wireBatch([paint(9005, ["opacity": 0.6])])
        XCTAssertTrue(removal.disjoint(later))
        let kept = removal.withheld(later) { p.views[$0]?.style }.ops[0].style
        XCTAssertNotNil(kept["cursor"], "the batch's removal waits for its reveal")
        XCTAssertEqual(kept["opacity"]?.number, 0.6)
    }

    func testThroughASessionAWaitingTickLandsAfterTheRevealAndAPressOnARemovedNodeIsRefused() throws {
        let session = ExactSession(app: ExactApp.shared, label: "staged", launchPlace: LaunchPlace(environment: [:]))
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try TableFixture.plan("component App\n  view\n    text \"boot\"\n"), size: CGSize(width: 400, height: 600)).error)
        show(session.presenter.viewport)
        session.apply(wireBatch(tree))
        session.apply(replacement(rows: 400))
        let plan = try XCTUnwrap(session.stage, "the batch is staged")
        XCTAssertNotNil(session.presenter.views[9002])

        // Each frame prepares a slice, and motion on a box it leaves alone advances between them;
        // none of the batch shows.
        for x in 1...3 {
            session.stepStage()
            session.apply(wireBatch([["op": "present", "id": 9005, "property": "translate", "x": Double(x), "y": 0.0, "w": 0.0, "h": 0.0]]))
            XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, CGFloat(x))
            XCTAssertTrue(session.stage === plan)
            XCTAssertNotNil(session.presenter.views[9003])
            XCTAssertNil(session.presenter.views[9100])
            XCTAssertNil(session.presenter.views[9201])
        }
        XCTAssertFalse(plan.preparedAll, "400 rows take more than three slices")

        // A timer's batch that changes nothing passes; a write to a property it writes waits.
        session.applyUnlessEmpty(wireBatch([]))
        XCTAssertTrue(session.stage === plan, "a timer's batch does not publish it")
        session.applyUnlessEmpty(wireBatch([["op": "props", "id": 9004, "set": ["title": "tick"]]]))
        XCTAssertTrue(session.stage === plan)
        XCTAssertEqual(session.pending.count, 1)
        XCTAssertNil(session.presenter.views[9004]?.props["title"])

        // A press on a box it removes is refused, and leaves it preparing.
        XCTAssertFalse(session.admits(9003))
        XCTAssertTrue(session.stage === plan)
        session.drainFill()
        XCTAssertNotNil(session.presenter.views[9100])
        XCTAssertNil(session.presenter.views[9003])
        XCTAssertEqual(session.presenter.views.keys.filter { (9200..<10000).contains($0) }.count, 800, "every row at once")
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 3, "the reveal leaves the newer sample")
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "tick", "the waiting write lands after the batch's own")
        XCTAssertTrue(session.admits(9004), "with nothing staged a press goes on")
    }

    func testARebootDuringPreparationEndsTheStage() throws {
        let session = ExactSession(app: ExactApp.shared, label: "staged-reboot", launchPlace: LaunchPlace(environment: [:]))
        defer { session.destroy() }
        let plan0 = try TableFixture.plan("component App\n  view\n    text \"boot\"\n")
        XCTAssertNil(session.boot(plan: plan0, size: CGSize(width: 400, height: 600)).error)
        show(session.presenter.viewport)
        session.apply(wireBatch(tree))
        session.apply(replacement(rows: 400))
        let plan = try XCTUnwrap(session.stage)
        session.stepStage()
        let top = try XCTUnwrap(plan.made[9100])
        XCTAssertNotNil(top.superview)
        // A batch of the old tree waits for it: it makes a box.
        session.apply(wireBatch([["op": "create", "id": 9500, "kind": "view", "style": [String: Any]()], ["op": "children", "id": 9001, "ids": [9002, 9004, 9005, 9500]]]))
        XCTAssertEqual(session.pending.count, 1)

        XCTAssertNil(session.boot(plan: plan0, size: CGSize(width: 400, height: 600)).error)
        XCTAssertNil(session.stage)
        XCTAssertTrue(session.pending.isEmpty)
        XCTAssertNil(session.presenter.views[9500], "what waited for the old tree is dropped, not applied")
        XCTAssertNil(top.superview, "what it prepared left the window")
        XCTAssertNil(session.presenter.views[9100])
        XCTAssertTrue(session.presenter.views.keys.allSatisfy { !(9100..<10000).contains($0) }, "none of its rows joined the new tree")
        session.stepStage()
        XCTAssertNil(session.presenter.views[9100], "an old frame reveals nothing")
        XCTAssertNotNil(session.presenter.views.values.first { $0.props["text"] == "boot" })
    }

    /// A booted session with `tree` applied, shown in a window.
    private func session(_ label: String) throws -> ExactSession {
        let session = ExactSession(app: ExactApp.shared, label: label, launchPlace: LaunchPlace(environment: [:]))
        XCTAssertNil(session.boot(plan: try TableFixture.plan("component App\n  view\n    text \"boot\"\n"), size: CGSize(width: 400, height: 600)).error)
        show(session.presenter.viewport)
        session.apply(wireBatch(tree))
        return session
    }

    func testABatchMadeAfterAPublishedOneThatStagesWaitsForItsReveal() throws {
        let session = try session("staged-order")
        defer { session.destroy() }
        // The owner published the replacement; the later batch's apply lands it first, and it stages.
        session.publishTickForTests(replacement(rows: 400))
        session.apply(wireBatch([["op": "props", "id": 9004, "set": ["title": "later"]]]))
        let plan = try XCTUnwrap(session.stage, "the earlier batch staged")
        XCTAssertNil(session.presenter.views[9004]?.props["title"], "the later write waits")
        XCTAssertEqual(session.pending.count, 1)
        session.finishStage()
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "later", "and lands after the earlier one's")
    }

    func testTheRevealPublishesTooltipsAndCursorRectsOnlyWhereRequested() throws {
        let p = Presenter()
        show(p.viewport)
        p.apply(wireBatch(tree))
        var batch = replacement(row: ["title": "Open details", "href": "#details"])
        batch.ops += wireBatch([
            paint(9204, ["cursor": "crosshair"]),
            ["op": "paragraph", "id": 9207, "runs": [
                ["id": 9901, "parent": 9207, "paint": true, "props": ["text": "Help", "href": "#help"]],
            ]],
        ]).ops
        let plan = try XCTUnwrap(p.stagePlan(batch))
        while !plan.preparedAll { p.prepare(plan, until: .infinity) }
        let link = try XCTUnwrap(plan.made[9202])
        XCTAssertNotNil(link.window)
        XCTAssertNil(link.toolTip)
        XCTAssertEqual(plan.tips.map(\.id), [9202])
        XCTAssertEqual(Set(plan.cursorRects.map(\.id)), [9202, 9204, 9207])

        p.revealing = plan
        p.apply(plan.reveal())
        p.revealing = nil
        XCTAssertEqual(link.toolTip, "Open details")
        XCTAssertFalse(link.held)
    }

    func testAButtonNamedByItsContentIsNamedAtTheReveal() throws {
        let p = Presenter()
        show(p.viewport)
        p.apply(wireBatch(tree))
        let plan = try XCTUnwrap(p.stagePlan(replacement(row: ["accessibilityRole": "button"])))
        while !plan.preparedAll { p.prepare(plan, until: .infinity) }
        let button = try XCTUnwrap(plan.made[9202])
        XCTAssertTrue(plan.revealAccessibility.contains(9202), "its name waits for the reveal's pass")
        p.revealing = plan
        p.apply(plan.reveal())
        p.revealing = nil
        XCTAssertEqual(button.accessibilityLabel(), "row 1")
    }

    func testAKeyedFormPreparesItsTextAndRevealsItsNativeControlsInOrder() throws {
        let source = """
        component App
          state generation = 0
          action swap
            generation = generation + 1
          view
            column
              button "Swap" press=swap testId="swap"
              for record in [generation] key=record
                column testId="form"
                  for i in range(40) key=i
                    text `${record} before ${i}` testId=`before-${i}`
                  button `Edit ${record}` testId="edit"
                  textarea value=`Note ${record}` testId="note"
                  for i in range(40) key=i
                    text `${record} after ${i}` testId=`after-${i}`
        """
        let session = ExactApp.shared.makeSession(label: "mixed-staged-form")
        defer { session.destroy() }
        show(session.presenter.viewport)
        XCTAssertNil(session.boot(plan: try TableFixture.plan(source), size: CGSize(width: 400, height: 600)).error)
        func node(_ name: String) throws -> NodeView {
            try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == name })
        }
        let old = try node("form"), oldButton = try node("edit")
        session.presenter.onPress?(try node("swap").id)
        let plan = try XCTUnwrap(session.stage, "the text around the controls can be prepared")
        while !plan.preparedAll { session.presenter.prepare(plan, until: .infinity) }
        let rows = plan.made.values.filter { $0.props["testId"]?.hasPrefix("before-") == true || $0.props["testId"]?.hasPrefix("after-") == true }
        XCTAssertEqual(rows.count, 80)
        XCTAssertTrue(rows.allSatisfy { $0.held && $0.window != nil })
        XCTAssertTrue(try node("form") === old)
        XCTAssertTrue(try node("edit") === oldButton)

        session.finishStage()
        let form = try node("form"), button = try node("edit"), note = try node("note")
        XCTAssertFalse(form === old)
        XCTAssertFalse(button === oldButton)
        let children = form.structuralChildren.compactMap { $0.props["testId"] }
        XCTAssertEqual(children, (0..<40).map { "before-\($0)" } + ["edit", "note"] + (0..<40).map { "after-\($0)" })
        XCTAssertTrue(rows.allSatisfy { !$0.held && session.presenter.views[$0.id] === $0 })
        XCTAssertEqual((session.presenter.controls.controls[button.id] as? NativeButtonMac)?.title, "Edit 1")
        XCTAssertEqual(note.props["value"], "Note 1")
    }

    func testAPreparedChildListTakesALiveViewOnlyAtTheReveal() throws {
        for rows in [4, 40] {
            let p = Presenter()
            show(p.viewport)
            var original = wireBatch(tree)
            for i in original.ops.indices where original.ops[i].op == .children {
                if original.ops[i].id == 9001 { original.ops[i].ids.removeAll { $0 == 9004 } }
                if original.ops[i].id == 9002 { original.ops[i].ids.append(9004) }
            }
            p.apply(original)
            let live = try XCTUnwrap(p.views[9004]), retiring = try XCTUnwrap(p.views[9002])
            let oldText = try XCTUnwrap(p.views[9003])
            let native = NSView()
            oldText.addSubview(native)
            var batch = replacement(rows: rows)
            let into = try XCTUnwrap(batch.ops.firstIndex { $0.op == .children && $0.id == 9100 })
            let from = try XCTUnwrap(batch.ops.firstIndex { $0.op == .children && $0.id == 9001 })
            batch.ops[into].ids.insert(9004, at: 1)
            batch.ops[from].ids.removeAll { $0 == 9004 }

            let plan = try XCTUnwrap(p.stagePlan(batch))
            while !plan.preparedAll { p.prepare(plan, until: .infinity) }
            let top = try XCTUnwrap(plan.made[9100])
            XCTAssertTrue(live.superview === retiring.container)
            XCTAssertEqual(top.structuralChildren.count, rows)
            p.revealing = plan
            p.apply(plan.reveal())
            p.revealing = nil
            XCTAssertTrue(oldText.superview === retiring.container)
            XCTAssertTrue(live.superview === top.container)
            XCTAssertTrue(retiring.held)
            p.teardown.finish()
            XCTAssertNil(retiring.superview)
            XCTAssertNil(oldText.superview)
            XCTAssertNil(oldText.presenter)
            XCTAssertTrue(native.superview === oldText)
            XCTAssertTrue(live.superview === top.container)
            XCTAssertEqual(top.structuralChildren.map(\.id), batch.ops[into].ids)
        }
    }

    func testAPreparedSubtreeIsRevealedOnThePixelGridAsAPlainApplyDrawsIt() throws {
        let batch = replacement(offset: 0.3)
        let plain = Presenter()
        plain.viewport.frame = NSRect(x: 0, y: 0, width: 400, height: 600)
        let other = NSWindow(contentRect: plain.viewport.frame, styleMask: [.titled], backing: .buffered, defer: false)
        other.isReleasedWhenClosed = false
        other.contentView = plain.viewport
        defer { other.close() }
        plain.apply(wireBatch(tree))
        plain.apply(batch)

        let p = Presenter()
        show(p.viewport)
        p.apply(wireBatch(tree))
        let plan = try XCTUnwrap(p.stagePlan(batch))
        while !plan.preparedAll { p.prepare(plan, until: .infinity) }
        p.revealing = plan
        p.apply(plan.reveal())
        p.revealing = nil
        XCTAssertEqual(other.backingScaleFactor, window?.backingScaleFactor)
        for id: UInt32 in [9100, 9200, 9201, 9203, 9279] {
            XCTAssertEqual(p.views[id]?.frame, plain.views[id]?.frame, "#\(id)")
        }
    }

    func testWhileStagedEveryInputReachesTheRuntimeAndLeavesTheBatchPreparing() throws {
        let session = try session("staged-input")
        defer { session.destroy() }
        session.apply(replacement(rows: 400))
        let plan = try XCTUnwrap(session.stage)
        var heard: [String] = []
        session.admit(9004) { heard.append("move") }
        session.admit(9003) { heard.append("press on the old block") }
        session.admit(9004) { heard.append("release") }
        XCTAssertEqual(heard, ["move", "release"], "each reaches the runtime as it comes; one on a node the batch removes is refused")
        XCTAssertTrue(session.stage === plan)

        // A press, a key and a pan's release from the presenter: none prepares the rest of the batch in its turn.
        let before = plan.cursor
        let began = CACurrentMediaTime()
        session.presenter.onPan?(9004, 1, 0)
        session.presenter.onPanRelease?(9004, 0, 0)
        session.presenter.onPress?(9004)
        session.presenter.onKey?(9004, "a")
        let spent = CACurrentMediaTime() - began
        XCTAssertTrue(session.stage === plan)
        XCTAssertEqual(plan.cursor, before, "preparation goes on in its own slices")
        if spent > StagePlan.frameSeconds + 0.03 { latePass("input's turn", String(format: "%.1f ms", spent * 1000)) }
    }

    func testAStagedBatchArmsTheClockForItsDeadline() throws {
        let session = try session("staged-clock")
        defer { session.destroy() }
        var batch = replacement(rows: 400)
        let due = session.now() + 60_000
        batch.timerDueMs = due
        session.apply(batch)
        XCTAssertNotNil(session.stage)
        XCTAssertEqual(session.timerDue, due)
        XCTAssertEqual(session.clockTimer?.isValid, true, "its deadline is asked while it is prepared")

        // A batch that goes ahead of an older one leaves its schedule when the older one applies after it.
        var older = wireBatch([title(9004, "input")])
        older.timerDueMs = due
        session.apply(older)
        XCTAssertEqual(session.pending.count, 1)
        var newer = wireBatch([translate(9005, 1)])
        newer.timerDueMs = due + 1000
        session.apply(newer)
        XCTAssertEqual(session.timerDue, due + 1000)
        session.finishStage()
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "input")
        XCTAssertEqual(session.timerDue, due + 1000, "the newest batch's deadline")
    }

    /// Block `k` of `rows` rows (0: box 9100, rows from 9200; 1: box 20000,
    /// rows from 20100) in place of block `old` of as many (nil: `tree`'s
    /// block 9002), writing box 9004's title and translate.
    private func block(_ k: Int, replacing old: Int?, rows: Int = 400, title: String = "new", translate: Double = 1, style: [String: Any]? = nil) -> Batch {
        let base: [(box: UInt32, rows: UInt32)] = [(9100, 9200), (20000, 20100)]
        var ops: [[String: Any]] = old.map { o in [["op": "destroy", "id": base[o].box]] + (0..<(2 * rows)).map { ["op": "destroy", "id": base[o].rows + UInt32($0)] } }
            ?? [["op": "destroy", "id": 9003], ["op": "destroy", "id": 9002]]
        let (box, first) = base[k]
        ops.append(["op": "create", "id": box, "kind": "view", "style": [String: Any]()])
        var kids: [UInt32] = []
        for i in 0..<rows {
            let row = first + UInt32(2 * i)
            kids.append(row)
            ops += [["op": "create", "id": row, "kind": "view", "style": [String: Any]()],
                    ["op": "create", "id": row + 1, "kind": "text", "style": [String: Any](), "props": ["text": "block \(k) row \(i)"]],
                    ["op": "children", "id": row, "ids": [row + 1]],
                    ["op": "frame", "id": row, "x": 0.0, "y": Double(i) * 10, "w": 400.0, "h": 10.0]]
        }
        ops += [["op": "children", "id": box, "ids": kids], ["op": "children", "id": 9001, "ids": [box, 9004, 9005]],
                self.title(9004, title), self.translate(9004, translate)] + (style.map { [paint(9004, $0)] } ?? [])
        return wireBatch(ops)
    }
    private func reply(_ session: ExactSession, _ batch: Batch) {
        session.repliesInFlight += 1
        session.replySubmissions.append(0)
        session.publish(.reply, RuntimeCompletion(lease: session.runtime.completionLease, batch: batch))
    }
    private func notice(_ session: ExactSession, _ batch: Batch) {
        session.noticesInFlight += 1
        session.publish(.notice, RuntimeCompletion(lease: session.runtime.completionLease, batch: batch))
    }
    private func translate(_ id: Int, _ x: Double) -> [String: Any] { ["op": "present", "id": id, "property": "translate", "x": x, "y": 0.0, "w": 0.0, "h": 0.0] }
    private func title(_ id: Int, _ value: String) -> [String: Any] { ["op": "props", "id": id, "set": ["title": value]] }
    private func paint(_ id: Int, _ style: [String: Any]) -> [String: Any] { ["op": "style", "id": id, "style": style] }

    func testAReplyPublishedWhileStagedLandsBeforeTheTicksThatOverlapIt() throws {
        let session = try session("staged-reply")
        defer { session.destroy() }
        session.apply(replacement(rows: 400))
        let plan = try XCTUnwrap(session.stage)

        // A reply that leaves the batch alone, then a tick: both land, in that order.
        reply(session, wireBatch([translate(9005, 1)]))
        session.apply(wireBatch([translate(9005, 2)]))
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 2)
        // One that writes what the batch writes waits. A newer sample passes it, which then leaves its own.
        reply(session, wireBatch([translate(9005, 3), title(9004, "reply")]))
        session.publishTickForTests(wireBatch([translate(9005, 4)]))
        session.landPublished()
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 4)
        XCTAssertEqual(session.pending.map(\.batch.ops.count), [1])
        XCTAssertFalse(session.waits(.tick))
        // A tick that writes what the reply writes waits behind it.
        session.publishTickForTests(wireBatch([translate(9005, 5), title(9004, "tick")]))
        session.landPublished()
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 4)
        XCTAssertEqual(session.pending.map(\.batch.ops.count), [1, 2])
        XCTAssertTrue(session.waits(.tick))
        XCTAssertTrue(session.stage === plan)
        session.finishStage()
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 5, "the reveal ends on the newest sample")
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "tick")
    }

    func testABatchBlockedBehindTheStageHoldsBackEveryLaterOne() throws {
        let session = try session("staged-blocked")
        defer { session.destroy() }
        session.apply(replacement(rows: 400))
        reply(session, wireBatch([title(9004, "older"), title(9005, "older")]))
        session.apply(wireBatch([title(9005, "newer")]))
        XCTAssertNil(session.presenter.views[9005]?.props["title"], "the later write waits behind the blocked one")
        session.finishStage()
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "older")
        XCTAssertEqual(session.presenter.views[9005]?.props["title"], "newer")
    }

    func testDrainFillReturnsWithNoStageOpenAndNothingQueued() throws {
        let session = try session("staged-drain")
        defer { session.destroy() }
        session.apply(replacement(rows: 400))
        XCTAssertNotNil(session.stage)
        reply(session, block(1, replacing: 0))
        session.drainFill()
        XCTAssertNil(session.stage, "the second block, staged at the first's reveal, is published too")
        XCTAssertFalse(session.hasPublished())
        XCTAssertNotNil(session.presenter.views[20000])
        XCTAssertNotNil(session.presenter.views[20101])
        XCTAssertNil(session.presenter.views[9100])

        // A publication that stages as the drain lands it is published too.
        reply(session, block(0, replacing: 1))
        XCTAssertNil(session.stage)
        session.drainFill()
        XCTAssertNil(session.stage)
        XCTAssertNotNil(session.presenter.views[9201])
        XCTAssertNil(session.presenter.views[20000])
    }

    func testAHandlersEventResultsComeBackAtOnceThoughItsBatchWaitsOrStages() throws {
        let session = try session("staged-prevent")
        defer { session.destroy() }
        let prevent: [[String: Any]] = [["op": "command", "name": "preventDefault", "args": [Any]()], ["op": "command", "name": "stopPropagation", "args": [Any]()]]
        // A handler whose batch stages.
        var staging = replacement(rows: 400)
        staging.ops += wireBatch(prevent).ops
        session.presenter.defaultPrevented = false; session.presenter.propagationStopped = false
        session.apply(staging)
        XCTAssertNotNil(session.stage)
        XCTAssertTrue(session.presenter.defaultPrevented && session.presenter.propagationStopped)
        // One whose batch waits behind it.
        session.presenter.defaultPrevented = false; session.presenter.propagationStopped = false
        session.apply(wireBatch(prevent + [title(9004, "key")]))
        XCTAssertNotNil(session.stage)
        XCTAssertNil(session.presenter.views[9004]?.props["title"])
        XCTAssertTrue(session.presenter.defaultPrevented && session.presenter.propagationStopped)
        session.presenter.defaultPrevented = false; session.presenter.propagationStopped = false
        session.finishStage()
        XCTAssertFalse(session.presenter.defaultPrevented || session.presenter.propagationStopped, "the reveal does not set them again")
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "key")
    }

    /// Ticks, replies, notices, published ticks, inputs and frames, in a
    /// seeded random interleaving around large batches that stage one after
    /// another. Each batch writes its own number: what shows of a property
    /// never goes back to an older batch's, and what is left is the newest
    /// batch's, as applying every batch in order leaves it.
    func testWhatTheRevealMakesAppliesAfterWhatWaitedForIt() throws {
        let session = try session("staged-reveal-order")
        defer { session.destroy() }
        session.apply(replacement(rows: 400))
        let plan = try XCTUnwrap(session.stage)
        session.apply(wireBatch([title(9004, "hovered")]))
        XCTAssertEqual(session.pending.count, 1, "a write to what the batch writes waits")
        // An autofocus the reveal runs: its batch is made after the one that waited.
        var focused = false
        session.onApplyForTests = { batch in
            guard batch.sequence == plan.batch.sequence, !focused else { return }
            focused = true
            session.apply(wireBatch([self.title(9004, "focused")]))
        }
        session.finishStage()
        XCTAssertTrue(focused)
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "focused")
        XCTAssertTrue(session.pending.isEmpty)

        // With nothing waiting, its batch still applies after the reveal's, not inside it.
        session.apply(block(0, replacing: nil, rows: 400, title: "revealed"))
        let second = try XCTUnwrap(session.stage)
        focused = false
        session.onApplyForTests = { batch in
            guard batch.sequence == second.batch.sequence, !focused else { return }
            focused = true
            session.apply(wireBatch([self.title(9004, "focused again")]))
        }
        session.finishStage()
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "focused again")
    }

    func testWithNoStageOpenBatchesApplyInTheOrderTheyWereMade() throws {
        let session = try session("unstaged-order")
        defer { session.destroy() }
        var order: [UInt64] = []
        func made(_ value: String) -> Batch { var b = wireBatch([title(9004, value)]); b.sequence = session.runtime.sequence.next(); return b }
        // Publications 2 and 4 wait when 3 comes.
        let two = made("2"), three = made("3"), four = made("4")
        session.onApplyForTests = { order.append($0.sequence) }
        reply(session, two)
        notice(session, four)
        session.apply(three)
        session.drainFill()
        XCTAssertEqual(order, [two, three, four].map(\.sequence))
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "4")

        // A and B published, and landing A makes C (an autofocus): C follows B.
        order = []
        let a = made("a"), b = made("b")
        var c: UInt64 = 0
        session.onApplyForTests = { batch in
            order.append(batch.sequence)
            guard batch.sequence == a.sequence else { return }
            let callback = made("c")
            c = callback.sequence
            session.apply(callback)
        }
        reply(session, a)
        reply(session, b)
        session.landPublished()
        XCTAssertEqual(order, [a.sequence, b.sequence, c])
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "c")

        // A publication the owner made before a batch reaching the presenter lands first; what landing it
        // makes, made after that batch, follows it.
        order = []
        let early = made("early"), batch = made("batch")
        session.onApplyForTests = { applied in
            order.append(applied.sequence)
            guard applied.sequence == early.sequence else { return }
            let callback = made("callback")
            c = callback.sequence
            session.apply(callback)
        }
        // Published with no turn of its own scheduled, as when a drain already claimed it.
        session.tickInFlight = true
        _ = session.published.append((.tick, RuntimeCompletion(lease: session.runtime.completionLease, batch: early)))
        session.applyNow(batch)
        XCTAssertEqual(order, [early.sequence, batch.sequence, c])
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "callback")

        // A batch made while another applies, with nothing waiting (a callback the apply runs), applies after it.
        order = []
        let outer = made("outer")
        session.onApplyForTests = { applied in
            order.append(applied.sequence)
            guard applied.sequence == outer.sequence else { return }
            let callback = made("inner")
            c = callback.sequence
            session.apply(callback)
        }
        session.apply(outer)
        XCTAssertEqual(order, [outer.sequence, c])
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "inner")
    }

    func testAPaintSampleWhileStagedShowsAtOnceAndMotionGoesOn() throws {
        let session = try session("staged-paint")
        defer { session.destroy() }
        // The batch restyles 9005, whose paint is moving: its cursor is the batch's own.
        var staging = replacement(rows: 400)
        staging.ops += wireBatch([paint(9005, ["opacity": 0.1, "cursor": "pointer"])]).ops
        session.apply(staging)
        let plan = try XCTUnwrap(session.stage)
        for step in 2...4 {
            session.stepStage()
            // Apple's runtime samples the paint into the node's whole style, the batch's cursor with it.
            session.apply(wireBatch([paint(9005, ["opacity": 0.1 * Double(step), "cursor": "pointer"]), translate(9005, Double(step))]))
            XCTAssertTrue(session.stage === plan)
            XCTAssertEqual(session.presenter.views[9005]?.style["opacity"]?.number ?? 0, 0.1 * Double(step), accuracy: 1e-9)
            XCTAssertNil(session.presenter.views[9005]?.style["cursor"], "the batch's cursor waits for its reveal")
            XCTAssertFalse(session.waits(.tick), "nothing waits, so frames go on")
        }
        session.finishStage()
        XCTAssertEqual(session.presenter.views[9005]?.style["opacity"]?.number ?? 0, 0.4, accuracy: 1e-9, "the reveal's older style does not overwrite the sample")
        XCTAssertNotNil(session.presenter.views[9005]?.style["cursor"])
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 4)
    }

    func testAnInputWaitingForTheRevealHoldsOnlyWhatOverlapsIt() throws {
        let session = try session("staged-input-motion")
        defer { session.destroy() }
        session.apply(replacement(rows: 400))
        let plan = try XCTUnwrap(session.stage)
        // An input writes the title the staged batch writes: it waits for the reveal.
        session.admit(9004) { session.apply(wireBatch([title(9004, "typed")])) }
        XCTAssertEqual(session.pending.count, 1)
        // The bar's motion, which nothing waiting touches, keeps moving frame after frame.
        for x in 1...5 {
            XCTAssertFalse(session.waits(.tick), "frame \(x) ticks")
            session.stepStage()
            session.publishTickForTests(wireBatch([translate(9005, Double(x))]))
            session.landPublished()
            XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, CGFloat(x))
        }
        XCTAssertTrue(session.stage === plan)
        // A timer's advance that overlaps it waits too, and holds only the next advance.
        session.advanceInFlight = true
        var timer = wireBatch([title(9004, "timer")])
        timer.clockWork = .advance
        session.publish(.advance, RuntimeCompletion(lease: session.runtime.completionLease, batch: timer))
        session.landPublished()
        XCTAssertTrue(session.waits(.advance))
        XCTAssertFalse(session.waits(.tick))
        // A tick that overlaps the waiting input waits behind it.
        session.publishTickForTests(wireBatch([translate(9005, 6), title(9004, "tick")]))
        session.landPublished()
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 5)
        XCTAssertTrue(session.waits(.tick))
        session.finishStage()
        XCTAssertEqual(session.presenter.views[9004]?.props["title"], "tick", "the staged batch's, the input's, the timer's, then the tick's")
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 6)
        XCTAssertFalse(session.waits(.tick))
    }

    func testABarKeepsMovingBesideABoxWhosePaintTheStagedBatchMakesMove() throws {
        let session = try session("staged-paint-split")
        defer { session.destroy() }
        var staged = replacement(rows: 400)
        staged.ops += wireBatch([["op": "create", "id": 9400, "kind": "view", "style": [String: Any]()],
                                 ["op": "children", "id": 9001, "ids": [9100, 9004, 9005, 9400]],
                                 ["op": "frame", "id": 9400, "x": 40.0, "y": 500.0, "w": 10.0, "h": 10.0]]).ops
        session.apply(staged)
        let plan = try XCTUnwrap(session.stage)
        // Each tick samples the new box's paint, in its whole style, and the bar's motion.
        for x in 1...3 {
            session.stepStage()
            var tick = wireBatch([paint(9400, ["opacity": 0.1 * Double(x)]), translate(9005, Double(x))])
            tick.clockWork = .tick
            session.publishTickForTests(tick)
            session.landPublished()
            XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, CGFloat(x), "frame \(x)")
            XCTAssertEqual(session.pending.map(\.batch.ops.count), [1], "only the newest paint waits")
        }
        XCTAssertTrue(session.stage === plan)
        session.finishStage()
        XCTAssertEqual(session.presenter.views[9400]?.style["opacity"]?.number ?? 0, 0.3, accuracy: 1e-9)
        XCTAssertTrue(session.pending.isEmpty)
    }

    /// A native button's face as the runtime gives it (`exact_press_face`).
    private func face(_ title: String) -> ControlRead { ControlRead(face: Data(#"{"button":true,"title":"\#(title)"}"#.utf8)) }
    /// Button 9010 beside the boxes, made with face `title`. The runtime's
    /// face is "live" from then on: a read of it would show that.
    private func button(_ session: ExactSession, _ title: String) throws -> NativeButtonMac {
        var made = wireBatch([["op": "create", "id": 9010, "kind": "control", "props": ["type": "button"], "handlers": ["press"]],
                              ["op": "children", "id": 9001, "ids": [9002, 9004, 9005, 9010]],
                              ["op": "frame", "id": 9010, "x": 0.0, "y": 540.0, "w": 120.0, "h": 32.0]])
        made.controlReads = [9010: face(title)]
        session.apply(made)
        var live = ButtonFace(); live.title = "live"
        session.presenter.buttonFace = { _ in live }
        return try XCTUnwrap(session.presenter.controls.controls[9010] as? NativeButtonMac)
    }

    func testAButtonFaceShowsWithTheBatchThatCarriesIt() throws {
        let session = try session("staged-face")
        defer { session.destroy() }
        let button = try button(session, "gen 0 taps 0")
        XCTAssertEqual(button.title, "gen 0 taps 0")
        var staged = replacement(rows: 400)
        staged.controlReads = [9010: face("gen 1 taps 0")]
        session.apply(staged)
        let plan = try XCTUnwrap(session.stage)
        // A press changes the face the staged batch changes: it waits. Its frame's sample of the bar's
        // motion goes with it, and a newer sample passes it.
        var press = wireBatch([translate(9005, 0.5)])
        press.controlReads = [9010: face("gen 1 taps 1")]
        session.admit(9010) { session.apply(press) }
        XCTAssertEqual(session.pending.count, 1)
        session.publishTickForTests(wireBatch([translate(9005, 1)]))
        session.landPublished()
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 1)
        // A new frame for the button changes no face, so the runtime gives none with it: it applies, and
        // the face is still the last applied.
        var frame = wireBatch([["op": "frame", "id": 9010, "x": 0.0, "y": 541.0, "w": 120.0, "h": 32.0]])
        frame.controlReads = [:]
        session.apply(frame)
        XCTAssertEqual(session.pending.count, 1)
        XCTAssertEqual(button.title, "gen 0 taps 0")
        XCTAssertTrue(session.stage === plan)
        session.finishStage()
        XCTAssertEqual(button.title, "gen 1 taps 1")
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 1)
        XCTAssertTrue(session.pending.isEmpty)
    }

    func testEachOfTwoStagedBatchesShowsItsOwnButtonFace() throws {
        let session = try session("staged-faces")
        defer { session.destroy() }
        let button = try button(session, "before")
        var first = replacement(rows: 400, title: "first")
        first.controlReads = [9010: face("first")]
        session.apply(first)
        let plan = try XCTUnwrap(session.stage)
        // The next replaces the first's block: it waits, and stages at the reveal.
        var ops: [[String: Any]] = [["op": "destroy", "id": 9100], ["op": "create", "id": 9300, "kind": "view", "style": [String: Any]()]]
        for i in 0..<400 { ops.append(["op": "create", "id": 10000 + i, "kind": "view", "style": [String: Any]()]) }
        ops += [["op": "children", "id": 9300, "ids": (0..<400).map { 10000 + $0 }], ["op": "children", "id": 9001, "ids": [9300, 9004, 9005, 9010]]]
        var second = wireBatch(ops)
        second.controlReads = [9010: face("second")]
        session.apply(second)
        XCTAssertEqual(session.pending.count, 1)
        session.finishStage()
        let next = try XCTUnwrap(session.stage, "the second stages at the first's reveal")
        XCTAssertFalse(next === plan)
        XCTAssertNotNil(session.presenter.views[9100])
        XCTAssertEqual(button.title, "first", "the first's face, not the runtime's")
        session.finishStage()
        XCTAssertNotNil(session.presenter.views[9300])
        XCTAssertEqual(button.title, "second")
    }

    func testAFaceWaitsBehindAWaitingBatchThatCarriesTheSameFace() throws {
        let session = try session("staged-face-order")
        defer { session.destroy() }
        let button = try button(session, "before")
        session.apply(replacement(rows: 400))
        let plan = try XCTUnwrap(session.stage)
        // One writes the title the staged batch writes, and the button's face: it waits. A later
        // change of only the face waits behind it.
        var waiting = wireBatch([title(9004, "waiting")])
        waiting.controlReads = [9010: face("waiting")]
        session.apply(waiting)
        var later = wireBatch([])
        later.controlReads = [9010: face("later")]
        session.apply(later)
        XCTAssertEqual(session.pending.count, 2)
        XCTAssertEqual(button.title, "before")
        XCTAssertTrue(session.stage === plan)
        session.finishStage()
        XCTAssertEqual(button.title, "later")
    }

    func testRootTabsShowInTheOrderTheBatchesCarryingThemWereMade() throws {
        let session = try session("staged-root-tabs")
        defer { session.destroy() }
        func tabs(_ panel: UInt32) -> [RootTabs] { [RootTabs(root: 9001, tablist: 9004, tabs: [9005], panels: [panel], unsaid: true)] }
        var staged = replacement(rows: 400)
        staged.rootTabs = tabs(9100)
        session.apply(staged)
        let plan = try XCTUnwrap(session.stage)
        // Tabs the runtime said after the staged batch's replace them: they wait for its reveal.
        var later = wireBatch([])
        later.rootTabs = tabs(9002)
        session.apply(later)
        XCTAssertEqual(session.pending.count, 1)
        XCTAssertNil(session.presenter.rootTabs[9001])
        XCTAssertTrue(session.stage === plan)
        session.finishStage()
        XCTAssertEqual(session.presenter.rootTabs[9001]?.panels, [9002])
    }

    func testABatchTheRuntimeMakesCarriesTheStateOfTheControlsItMakes() throws {
        let session = ExactSession(app: ExactApp.shared, label: "controls-read", launchPlace: LaunchPlace(environment: [:]))
        defer { session.destroy() }
        let source = """
        component App
          view
            column
              button
                text "Hi"
              select value="b" aria-label="Pick"
                option "A" value="a"
                option "B" value="b"
              input type="radio" name="g" value="x" aria-label="X"
              input type="radio" name="g" value="y" aria-label="Y"

        """
        let boot = session.boot(plan: try TableFixture.plan(source), size: CGSize(width: 400, height: 600))
        XCTAssertNil(boot.error)
        let reads = try XCTUnwrap(boot.controlReads).values
        XCTAssertEqual(reads.compactMap(\.face).map { ButtonFace(json: $0).title }, ["Hi"])
        XCTAssertEqual(reads.compactMap(\.select).map { SelectMenu(json: $0) }, [SelectMenu(options: [.init(value: "a", label: "A", disabled: false), .init(value: "b", label: "B", disabled: false)], chosen: 1)])
        XCTAssertEqual(reads.compactMap(\.radio).map { RadioGroup(json: $0).group.count }, [2, 2])
    }

    func testASelectsOptionsAndARadiosGroupShowWithTheBatchThatCarriesThem() throws {
        let session = try session("staged-select")
        defer { session.destroy() }
        func menu(_ label: String) -> ControlRead { ControlRead(select: Data(#"{"options":[{"value":"v","label":"\#(label)"}],"chosen":0}"#.utf8)) }
        func group(_ ids: [UInt32]) -> ControlRead { ControlRead(radio: Data(#"{"group":\#(ids),"next":null,"previous":null}"#.utf8)) }
        var made = wireBatch([["op": "create", "id": 9011, "kind": "control", "props": ["type": "select"], "handlers": ["change"]],
                              ["op": "create", "id": 9012, "kind": "control", "props": ["type": "radio", "name": "g"], "handlers": ["change"]],
                              ["op": "children", "id": 9001, "ids": [9002, 9004, 9005, 9011, 9012]],
                              ["op": "frame", "id": 9011, "x": 0.0, "y": 540.0, "w": 120.0, "h": 24.0],
                              ["op": "frame", "id": 9012, "x": 200.0, "y": 540.0, "w": 16.0, "h": 16.0]])
        made.controlReads = [9011: menu("first"), 9012: group([9012, 9013])]
        session.apply(made)
        // The runtime holds every batch it made: a read of it would show these.
        session.presenter.selectOptions = { _ in SelectMenu(json: Data(#"{"options":[{"value":"v","label":"live"}],"chosen":0}"#.utf8)) }
        session.presenter.controls.radioGroup = { _ in RadioGroup(json: Data(#"{"group":[1,2,3]}"#.utf8)) }
        let popup = try XCTUnwrap(session.presenter.controls.controls[9011] as? NSPopUpButton)
        XCTAssertEqual(popup.itemTitles, ["first"])
        session.apply(replacement(rows: 400))
        let plan = try XCTUnwrap(session.stage)
        // One writes the title the staged batch writes, the select's options and the radio's group: it waits.
        var waiting = wireBatch([title(9004, "waiting")])
        waiting.controlReads = [9011: menu("waiting"), 9012: group([9012])]
        session.apply(waiting)
        XCTAssertEqual(session.pending.count, 1)
        // A tick passes and the controls sync: they show what the batches applied so far carried.
        var tick = wireBatch([translate(9005, 1)])
        tick.controlReads = [:]
        session.apply(tick)
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 1)
        XCTAssertEqual(popup.itemTitles, ["first"])
        XCTAssertEqual(session.presenter.controls.radioMembers(9012), [9012, 9013])
        XCTAssertTrue(session.stage === plan)
        session.finishStage()
        XCTAssertEqual(popup.itemTitles, ["waiting"])
        XCTAssertEqual(session.presenter.controls.radioMembers(9012), [9012])
    }

    func testTheRevealOfAnOlderBatchLeavesTheNewestOnesRunnerState() throws {
        let session = try session("staged-runner")
        defer { session.destroy() }
        var staged = replacement(rows: 400)
        staged.seq = (1, 10)
        session.apply(staged)
        let plan = try XCTUnwrap(session.stage)
        // A batch made after it passes it, owing a canvas draw, at commit 14.
        var newer = wireBatch([translate(9005, 1)])
        newer.seq = (1, 14)
        newer.canvasOwed = true
        session.apply(newer)
        XCTAssertEqual(session.nativeViews?.hatchClock.seq, 14)
        XCTAssertTrue(session.canvasOwed)
        XCTAssertTrue(session.stage === plan)
        session.finishStage()
        XCTAssertEqual(session.nativeViews?.hatchClock.seq, 14, "a hatch's frame never reports an older commit")
        XCTAssertTrue(session.canvasOwed, "the draw owed while it prepared is still asked")
    }

    func testABarBesideABlockAWaitingBatchReplacesKeepsMoving() throws {
        let session = try session("staged-rekey")
        defer { session.destroy() }
        session.apply(replacement(rows: 400))
        let plan = try XCTUnwrap(session.stage)
        // The next block replaces the staged one, and keeps the boxes beside it in place.
        session.apply(wireBatch([["op": "destroy", "id": 9100], ["op": "create", "id": 9300, "kind": "view", "style": [String: Any]()],
                                 ["op": "children", "id": 9001, "ids": [9300, 9004, 9005]],
                                 ["op": "frame", "id": 9300, "x": 0.0, "y": 0.0, "w": 400.0, "h": 10.0]]))
        XCTAssertEqual(session.pending.count, 1)
        for x in 1...3 {
            session.publishTickForTests(wireBatch([translate(9005, Double(x))]))
            session.landPublished()
            XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, CGFloat(x), "frame \(x)")
        }
        XCTAssertFalse(session.waits(.tick))
        XCTAssertTrue(session.stage === plan)
        session.finishStage()
        XCTAssertTrue(session.pending.isEmpty)
        XCTAssertNotNil(session.presenter.views[9300])
        XCTAssertNil(session.presenter.views[9100])
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 3)
    }

    func testABarKeepsMovingBesideAnAnimatedBoxTheStagedBatchMakes() throws {
        let session = try session("staged-tick-split")
        defer { session.destroy() }
        var staged = replacement(rows: 400)
        staged.ops += wireBatch([["op": "create", "id": 9400, "kind": "view", "style": [String: Any]()],
                                 ["op": "children", "id": 9001, "ids": [9100, 9004, 9005, 9400]],
                                 ["op": "frame", "id": 9400, "x": 40.0, "y": 500.0, "w": 10.0, "h": 10.0]]).ops
        session.apply(staged)
        let plan = try XCTUnwrap(session.stage)
        // Each frame's tick samples the new box and the bar: the bar's sample shows, and only the
        // newest of the box's waits. A tick from a display frame and one the owner published alike.
        for x in 1...3 {
            session.stepStage()
            var tick = wireBatch([translate(9400, Double(x)), translate(9005, Double(x))])
            tick.clockWork = .tick
            if x == 2 { session.apply(tick) } else { session.publishTickForTests(tick); session.landPublished() }
            XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, CGFloat(x), "frame \(x)")
            XCTAssertEqual(session.pending.map(\.batch.ops.count), [1])
        }
        XCTAssertNil(session.presenter.views[9400])
        XCTAssertTrue(session.stage === plan)
        session.finishStage()
        XCTAssertEqual(session.presenter.views[9400]?.translatePx.x, 3)
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 3)
        XCTAssertTrue(session.pending.isEmpty)
    }

    /// A press replaces block "old" with 400 rows and a box whose entrance
    /// fades it in over 100 ms.
    private static let entrancePlan: Result<Data, Error> = Result {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let rows = (0..<400).map { "          text \"Row \($0)\"" }.joined(separator: "\n")
        let source = """
        component App
          state on = false
          action swap
            on = true
          view
            column
              button "Swap" testId="swap" press=swap
              if on
                column testId="rows"
        \(rows)
                box testId="entrance" width=10 height=10 animation="enter 100ms linear"
              else
                column testId="old"
                  text "old"

        keyframes enter
          from opacity=0
          to opacity=1

        """
        let contract = directory.appendingPathComponent("app.contract"), plan = directory.appendingPathComponent("app.plan")
        try source.write(to: contract, atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
        compiler.arguments = ["build", contract.path, "-o", plan.path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: plan)
    }

    func testAnEntranceTheStagedBatchBeginsStartsAtItsReveal() throws {
        let session = ExactSession(app: ExactApp.shared, label: "staged-entrance", launchPlace: LaunchPlace(environment: [:]))
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try Self.entrancePlan.get(), size: CGSize(width: 400, height: 600)).error)
        show(session.presenter.viewport)
        func node(_ testId: String) -> NodeView? { session.presenter.views.values.first { $0.props["testId"] == testId } }
        let swap = try XCTUnwrap(node("swap"))
        var t = session.now()
        session.apply(session.runtime.press(swap.id, now: t))
        let plan = try XCTUnwrap(session.stage, "the replacement is staged")
        // Preparation outlasts the entrance, and frames go on meanwhile: none starts it.
        for _ in 0..<12 {
            t += 16
            session.stepStage()
            let tick = session.runtime.tick(now: t, frame: t)
            XCTAssertFalse(tick.ops.contains { $0.op == .animations }, "at \(t)")
            session.apply(tick)
        }
        XCTAssertTrue(session.stage === plan)
        session.finishStage()
        let entrance = try XCTUnwrap(node("entrance"))
        // The first frame after the reveal starts it there.
        t += 16
        let first = session.runtime.tick(now: t, frame: t)
        let specs = first.ops.first { $0.op == .animations && $0.id == entrance.id }?.payload["specs"] as? [CssAnimation]
        XCTAssertEqual(try XCTUnwrap(specs?.first?.start), t / 1000, accuracy: 1e-9)
    }

    func testTheRevealReportsItsCommitAfterATickThatCommitsNothing() throws {
        let session = try session("staged-commit")
        defer { session.destroy() }
        var staged = replacement(rows: 400)
        staged.seq = (1, 10)
        session.apply(staged)
        let plan = try XCTUnwrap(session.stage)
        var tick = wireBatch([translate(9005, 1)])
        tick.clockWork = .tick
        session.apply(tick)
        XCTAssertEqual(session.presenter.views[9005]?.translatePx.x, 1)
        XCTAssertNotEqual(session.nativeViews?.hatchClock.seq, 10)
        XCTAssertTrue(session.stage === plan)
        session.finishStage()
        XCTAssertEqual(session.nativeViews?.hatchClock.seq, 10)
    }

    func testADisplayFramesTickAndASynchronousAdvanceWaitAsTheirKind() throws {
        let session = try session("staged-clock-kinds")
        defer { session.destroy() }
        // Made while a batch applies, each waits for it: as a tick and an advance.
        var kinds: [(tick: Bool, advance: Bool)] = []
        session.onApplyForTests = { _ in
            session.onApplyForTests = nil
            _ = session.frames.submitMotionTick(now: session.now(), frame: nil)
            kinds.append((session.waits(.tick), session.waits(.advance)))
            session.timerDue = 0
            session.advanceIfDue()
            kinds.append((session.waits(.tick), session.waits(.advance)))
        }
        session.apply(wireBatch([title(9004, "outer")]))
        XCTAssertEqual(kinds.map(\.tick), [true, true])
        XCTAssertEqual(kinds.map(\.advance), [false, true])
        XCTAssertTrue(session.pending.isEmpty, "both applied after it")
    }

    func testACanvasTheStagedBatchLeavesAloneDrawsWhileOneItChangesWaitsForTheReveal() throws {
        let session = try session("staged-canvas")
        defer { session.destroy() }
        session.apply(wireBatch([9007, 9008].map { ["op": "create", "id": $0, "kind": "canvas2d"] } + [
            ["op": "children", "id": 9001, "ids": [9002, 9004, 9005, 9007, 9008]],
            ["op": "frame", "id": 9007, "x": 0.0, "y": 520.0, "w": 10.0, "h": 10.0],
            ["op": "frame", "id": 9008, "x": 20.0, "y": 520.0, "w": 10.0, "h": 10.0],
        ]))
        let canvases = session.presenter.canvas2d
        func draw(_ id: Int) -> [String: Any] {
            ["op": "canvas2d", "id": id, "lifetime": id, "generation": 0, "seq": 0, "fresh": true,
             "w": 10, "h": 10, "scale": 1.0, "stretch": false, "lists": [String]()]
        }
        func shown(_ id: UInt32) -> Bool {
            canvases.waitForReplays()
            return session.presenter.views[id]?.layer?.sublayers?.contains { $0.contents != nil } ?? false
        }
        // The staged batch changes canvas 9008's props, and leaves 9007 alone.
        var staged = replacement(rows: 400)
        staged.ops += wireBatch([["op": "props", "id": 9008, "set": ["accessibilityLabel": "new"]]]).ops
        session.apply(staged)
        let plan = try XCTUnwrap(session.stage)
        session.publishTickForTests(wireBatch([draw(9007)]))
        session.landPublished()
        XCTAssertTrue(shown(9007), "the canvas it leaves alone draws while it prepares")
        session.publishTickForTests(wireBatch([draw(9008)]))
        session.landPublished()
        XCTAssertEqual(session.pending.count, 1)
        XCTAssertFalse(shown(9008), "the one it changes shows nothing new before its reveal")
        XCTAssertTrue(session.stage === plan)
        session.finishStage()
        XCTAssertTrue(shown(9008))
        XCTAssertTrue(session.pending.isEmpty)
    }

    func testAFieldBeingEditedThatARevealDestroysStopsTakingKeys() throws {
        let session = try session("staged-field")
        defer { session.destroy() }
        session.apply(wireBatch([["op": "create", "id": 9006, "kind": "input", "props": ["value": "draft"]],
                                 ["op": "children", "id": 9002, "ids": [9003, 9006]],
                                 ["op": "frame", "id": 9006, "x": 0.0, "y": 30.0, "w": 200.0, "h": 24.0]]))
        let window = try XCTUnwrap(window), block = try XCTUnwrap(session.presenter.views[9002])
        let field = try XCTUnwrap(session.presenter.views[9006]?.field)
        XCTAssertTrue(window.makeFirstResponder(field))
        XCTAssertNotNil(field.currentEditor(), "it is being edited")
        var staging = replacement(rows: 400)
        staging.ops.insert(contentsOf: wireBatch([["op": "destroy", "id": 9006]]).ops, at: 0)
        session.apply(staging)
        XCTAssertNotNil(session.stage)
        session.finishStage()
        XCTAssertNil(session.presenter.views[9006])
        XCTAssertNil(field.currentEditor(), "editing ended")
        XCTAssertFalse((window.firstResponder as? NSView)?.isDescendant(of: block) ?? false, "no key reaches the field held off screen")
    }

    func testAnyInterleavingShowsEveryPropertyInOrderAndEndsAsAnInOrderApply() throws {
        for seed in UInt64(1)...8 {
            var rng = SplitMix(seed)
            let session = try session("staged-order-\(seed)")
            defer { session.destroy() }
            var newest: [String: UInt64] = [:], shown: [String: UInt64] = [:]
            var blocks = 0, cursor: UInt64 = 0, slowest = 0.0
            func number() -> UInt64 { session.runtime.sequence.next() }
            // The next block, staged at once (`apply`) or published.
            func nextBlock() -> Batch {
                let n = number()
                // Each restyles 9004 with an authored cursor and a paint sample.
                var b = block(blocks % 2, replacing: blocks == 0 ? nil : (blocks - 1) % 2, rows: 60, title: "s\(n)", translate: Double(n),
                              style: ["opacity": Double(n), "cursor": "s\(n)"])
                b.sequence = n
                blocks += 1
                cursor = n
                for key in ["title", "translate", "opacity", "cursor"] { newest["9004 \(key)"] = n }
                return b
            }
            func read() -> [String: UInt64] {
                var out: [String: UInt64] = [:]
                for id: UInt32 in [9004, 9005] {
                    guard let v = session.presenter.views[id] else { continue }
                    if let t = v.props["title"], t.hasPrefix("s") { out["\(id) title"] = UInt64(t.dropFirst()) }
                    if v.translatePx.x > 0 { out["\(id) translate"] = UInt64(v.translatePx.x) }
                    if let o = v.style["opacity"]?.number { out["\(id) opacity"] = UInt64(o) }
                    if case .string(let c) = v.style["cursor"], c.hasPrefix("s") { out["\(id) cursor"] = UInt64(c.dropFirst()) }
                }
                return out
            }
            func batch() -> Batch {
                let n = number()
                var ops: [[String: Any]] = []
                for _ in 0..<(1 + Int(rng.next() % 2)) {
                    let id = rng.next() % 2 == 0 ? 9004 : 9005
                    switch rng.next() % 3 {
                    case 0: ops.append(title(id, "s\(n)")); newest["\(id) title"] = n
                    case 1: ops.append(translate(id, Double(n))); newest["\(id) translate"] = n
                    default:
                        // A paint sample: the node's whole style, with 9004's cursor as the runtime has it.
                        ops.append(paint(id, id == 9004 && cursor > 0 ? ["opacity": Double(n), "cursor": "s\(cursor)"] : ["opacity": Double(n)]))
                        newest["\(id) opacity"] = n
                    }
                }
                var b = wireBatch(ops)
                b.sequence = n
                return b
            }
            // What a landing or the reveal runs makes a batch of its own now and then (an autofocus).
            var calling = false
            session.onApplyForTests = { _ in
                guard !calling, rng.next() % 6 == 0 else { return }
                calling = true
                session.apply(batch())
                calling = false
            }
            for step in 0..<400 {
                switch rng.next() % 10 {
                case 0: session.apply(batch())
                case 1: reply(session, batch())
                case 2: notice(session, batch())
                case 3: session.publishTickForTests(batch())
                case 4, 5:
                    // A press or a key: it reaches the runtime at once and leaves the stage preparing.
                    let open = session.stage, prepared = open?.cursor, began = CACurrentMediaTime()
                    let b = batch()
                    session.admit(9004) { session.apply(b) }
                    if let open {
                        slowest = max(slowest, CACurrentMediaTime() - began)
                        XCTAssertTrue(session.stage === open, "seed \(seed) step \(step): input published the stage")
                        XCTAssertEqual(open.cursor, prepared, "seed \(seed) step \(step): input's turn prepared the stage")
                    }
                case 6, 7: session.stepStage()
                case 8: session.landPublished()
                default:
                    guard session.stage == nil else { session.stepStage(); break }
                    if rng.next() % 2 == 0 { session.apply(nextBlock()) } else { reply(session, nextBlock()) }
                }
                // Per node: no presented value goes back in number order, whatever went ahead of what.
                for (key, value) in read() {
                    XCTAssertGreaterThanOrEqual(value, shown[key] ?? 0, "seed \(seed) step \(step): \(key) went back")
                    shown[key] = value
                }
            }
            session.onApplyForTests = nil
            session.drainFill()
            XCTAssertNil(session.stage)
            XCTAssertGreaterThan(blocks, 3, "seed \(seed): several batches staged")
            // A wall time on a shared machine is evidence, not a verdict (docs/testing.md).
            if slowest > StagePlan.frameSeconds + 0.03 { latePass("seed \(seed): input's turn", String(format: "%.1f ms", slowest * 1000)) }
            XCTAssertEqual(read(), newest, "seed \(seed)")
        }
    }
}

/// A seeded generator (SplitMix64), so a failing interleaving can be run again.
private struct SplitMix {
    private var state: UInt64
    init(_ seed: UInt64) { state = seed }
    mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        return z ^ (z >> 31)
    }
}
#endif
