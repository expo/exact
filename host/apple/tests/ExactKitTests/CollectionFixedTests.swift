import XCTest
import ExactCore
@testable import ExactKit
#if os(macOS)
import AppKit
#else
import UIKit
#endif

final class CollectionFixedTests: XCTestCase {
    private func node(_ p: Presenter, _ name: String) throws -> NodeView {
        try XCTUnwrap(p.views.values.first { $0.props["testId"] == name })
    }
    private func settle(_ p: Presenter) {
        p.settlePump()
        p.collections.settle()
    }
    private func move(_ p: Presenter, _ list: NodeView, to offset: CGFloat) {
        #if os(macOS)
        let clip = list.scroll!.contentView
        let maximum = max(0, (clip.documentView?.frame.height ?? 0) - clip.bounds.height)
        clip.scroll(to: CGPoint(x: 0, y: min(maximum, max(0, offset))))
        list.scroll!.reflectScrolledClipView(list.scroll!.contentView)
        #else
        let scroll = list.scroll!
        let maximum = max(-scroll.adjustedContentInset.top, scroll.contentSize.height + scroll.adjustedContentInset.bottom - scroll.bounds.height)
        scroll.setContentOffset(CGPoint(x: 0, y: min(maximum, max(-scroll.adjustedContentInset.top, offset))), animated: false)
        #endif
        p.collections.changed(list.id, user: true)
        settle(p)
    }
    func testFixedContentMatchesEagerFlowAndFollowsAGrowingChatTrail() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let session = ExactApp.shared.makeSession(label: "fixed-chat")
        let eager = ExactApp.shared.makeSession(label: "eager-chat")
        defer { session.destroy(); eager.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("COLLECTION_FIXED"), size: CGSize(width: 400, height: 600)).error)
        XCTAssertNil(eager.boot(plan: try fixturePlan("COLLECTION_FIXED_EAGER"), size: CGSize(width: 400, height: 600)).error)
        let p = session.presenter, oracle = eager.presenter
        settle(p); settle(oracle)
        let list = try node(p, "chat"), eagerList = try node(oracle, "chat")
        let initial = try XCTUnwrap(p.collections.entries[list.id]?.snapshot)
        XCTAssertEqual(initial.fixed.count, 2)
        XCTAssertTrue(initial.rows.isEmpty, "the tall lead fills the initial data window")
        XCTAssertTrue(p.collections.covers(list.id), "fixed content covers the port without a data row")
        XCTAssertEqual(try node(p, "trail").props["accessibilityElementsHidden"], "true")
        XCTAssertEqual(try node(p, "trail").props["inert"], "true")
        #if os(macOS)
        XCTAssertTrue(try node(p, "reply").authoredInert)
        #endif
        #if os(iOS) || os(tvOS)
        XCTAssertTrue(try node(p, "trail").accessibilityElementsHidden)
        #endif
        for name in ["lead", "trail"] {
            XCTAssertEqual(try node(p, name).laidOutFrame, try node(oracle, name).laidOutFrame)
        }
        move(p, list, to: 100_000)
        let before = try XCTUnwrap(p.collections.geometry(list.id)).offset
        XCTAssertTrue(p.collections.covers(list.id), "the tall trail covers the end with no mounted data")
        XCTAssertNotEqual(try node(p, "trail").props["accessibilityElementsHidden"], "true")
        XCTAssertNotEqual(try node(p, "trail").props["inert"], "true")
        #if os(macOS)
        XCTAssertFalse(try node(p, "reply").authoredInert)
        #endif
        #if os(iOS) || os(tvOS)
        XCTAssertFalse(try node(p, "trail").accessibilityElementsHidden)
        #endif
        p.press(try node(p, "grow").id); settle(p)
        XCTAssertEqual(try XCTUnwrap(p.collections.geometry(list.id)).offset, before + 200, accuracy: 0.5)
        XCTAssertEqual(try node(p, "trail").laidOutFrame.height, 1000)
        // Leave the followed end and read within the trail; older history keeps that place.
        move(p, list, to: CGFloat(before - 100))
        let reading = try XCTUnwrap(p.collections.geometry(list.id)).offset
        p.press(try node(p, "prepend").id); settle(p)
        XCTAssertEqual(try XCTUnwrap(p.collections.geometry(list.id)).offset, reading + 40, accuracy: 0.5)
        XCTAssertEqual(list.content.height - list.layoutBounds.height, CGFloat(before + 240), accuracy: 0.5)
        XCTAssertEqual(eagerList.layoutBounds.height, 200)
        p.press(try node(p, "clear").id); settle(p)
        let empty = try XCTUnwrap(p.collections.entries[list.id]?.snapshot)
        XCTAssertEqual(empty.count, 0)
        XCTAssertTrue(empty.rows.isEmpty)
        XCTAssertEqual(empty.extent, 1620, accuracy: 0.5)
        XCTAssertTrue(p.collections.covers(list.id))
        XCTAssertNotEqual(try node(p, "trail").props["accessibilityElementsHidden"], "true")
    }
    func testARejectedFixedMeasurementIsRetriedAfterANewerSnapshot() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let p = Presenter()
        defer { p.collections.reset() }
        func snapshot(_ revision: Int, _ sizes: [Double] = [0, 0]) -> Batch {
            let fixed: [[String: Any]] = [
                ["view": 10, "roots": [2], "epoch": 1, "size": sizes[0]],
                ["view": 11, "roots": [3], "epoch": 1, "size": sizes[1]],
            ]
            let item: [String: Any] = ["view": 1, "revision": revision, "scrollSequence": 0,
                "count": 100, "totalExtent": 4000 + sizes.reduce(0, +), "rows": [], "fixed": fixed]
            return wireBatch([["op": "collections", "items": [item]]])
        }
        p.collections.beginBatch(snapshot(1)); p.collections.endBatch()
        let entry = try XCTUnwrap(p.collections.entries[1])
        entry.lastFacts = CollectionFacts(offset: 0, portMain: 200, portCross: 300, cross: 300,
            measurements: [.init(view: 10, epoch: 1, size: 600), .init(view: 11, epoch: 1, size: 800)],
            focus: nil, interaction: nil)
        p.collections.beginBatch(snapshot(2)); p.collections.endBatch()
        XCTAssertTrue(entry.owed, "the newer index has not taken either fixed measurement")
        entry.owed = false
        p.collections.beginBatch(snapshot(3, [600, 800])); p.collections.endBatch()
        XCTAssertFalse(entry.owed, "a snapshot that took both measurements needs no retry")
    }

    func testUnavailableKernelGeometryDoesNotAcknowledgeSnapshotEstimates() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let session = ExactApp.shared.makeSession(label: "fixed-measurement-retry")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("COLLECTION_FIXED"), size: CGSize(width: 400, height: 600)).error)
        let p = session.presenter
        settle(p)
        let list = try node(p, "chat")
        let measured = p.collections.fixedMeasurements(list.id)
        XCTAssertEqual(measured.map(\.size), [620, 800])
        Owner.shared.sync {
            Owner.shared.callMain {
                XCTAssertNil(session.runtime.collectionFixedSizes(list.id, lead: 1, trail: 1))
                XCTAssertTrue(p.collections.fixedMeasurements(list.id).isEmpty)
                // A report now would lack the fixed sizes the runner requires.
                p.collections.dataReady()
                p.collections.settle()
                XCTAssertEqual(p.collections.entries[list.id]?.owed, true, "an unmeasured fixed list stays owed")
            }
        }
        XCTAssertEqual(p.collections.fixedMeasurements(list.id), measured, "a later owner read supplies real measurements")
        settle(p)
        XCTAssertEqual(p.collections.entries[list.id]?.owed, false, "the next turn delivers the owed report")
        XCTAssertEqual(session.state, .ready, "the runner survived the unmeasured turn")
        p.press(try node(p, "grow").id); settle(p)
        XCTAssertEqual(p.collections.fixedMeasurements(list.id).map(\.size), [620, 1000])
    }

    #if os(macOS)
    func testSelectAllThenShiftClickOrArrowKeepsLogicalAndFixedEndpoints() throws {
        _ = NSApplication.shared
        for (leadHeight, textTrail, count) in [(0, true, 3), (20, true, 3), (1200, true, 3), (20, false, 80)] {
            let fixed = leadHeight > 0
            let source = """
            component App
              view
                list virtualized=true appearance="none" height=200 width=320 estimated-item-height=40 testId="list"
            \(fixed ? "      text \"Header\" height=\(leadHeight) testId=\"lead\"" : "")
                  for n in range(\(count)) key=n
                    text `Row ${n}` height=40 testId=`row-${n}`
            \(fixed ? (textTrail ? "      text \"Footer\" testId=\"trail\"" : "      view height=1200 testId=\"trail\"") : "")
            """
            let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            defer { try? FileManager.default.removeItem(at: dir) }
            try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
            let compiler = Process()
            compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
            compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
            try compiler.run(); compiler.waitUntilExit()
            XCTAssertEqual(compiler.terminationStatus, 0)
            let session = ExactApp.shared.makeSession(label: "select-all-range")
            defer { session.destroy() }
            XCTAssertNil(session.boot(plan: try Data(contentsOf: dir.appendingPathComponent("app.plan")), size: CGSize(width: 400, height: 300)).error)
            let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
            window.isReleasedWhenClosed = false
            defer { window.makeFirstResponder(nil); window.close() }
            window.contentView = ExactView(session: session)
            let p = session.presenter
            window.contentView?.layoutSubtreeIfNeeded(); settle(p)
            let list = try node(p, "list")
            if leadHeight == 1200 {
                XCTAssertTrue(try XCTUnwrap(p.collections.entries[list.id]?.snapshot).rows.isEmpty)
                XCTAssertEqual(try node(p, "trail").props["inert"], "true")
            }
            let initial = fixed ? try node(p, "lead") : try node(p, "row-0")
            window.makeFirstResponder(initial)
            p.selection.selectAll(in: window)
            XCTAssertTrue(p.selection.modify(NSSelectorFromString("moveLeftAndModifySelection:")))
            let edge = fixed && textTrail ? try node(p, "trail") : try node(p, "row-\(count - 1)")
            let parts = (fixed ? ["Header"] : []) + (0..<count).map { "Row \($0)" } + (fixed && textTrail ? ["Footer"] : [])
            XCTAssertEqual(p.selection.range(edge), NSRange(location: 0, length: (parts.last! as NSString).length - 1))
            XCTAssertEqual(p.selection.selectedText(), String(parts.joined(separator: "\n\n").dropLast()))
            move(p, list, to: CGFloat(leadHeight))
            let first = try node(p, "row-0"), last = try node(p, "row-2")
            p.selection.selectAll(in: window)
            let point = first.convert(NSPoint(x: 0, y: -1), to: nil)
            let event = try XCTUnwrap(NSEvent.mouseEvent(with: .leftMouseDown, location: point, modifierFlags: .shift,
                timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber, context: nil,
                eventNumber: 1, clickCount: 1, pressure: 1))
            p.selection.begin(first, event: event)
            XCTAssertEqual(p.selection.selectedText(), parts.dropFirst(fixed ? 1 : 0).joined(separator: "\n\n"))
            XCTAssertEqual(p.selection.range(last), NSRange(location: 0, length: 5))
        }
    }

    /// Shift-Command-Up after Select All collapses at the data's logical
    /// start, not at the first row still mounted, and Shift-Command-Down
    /// from there selects through the logical end.
    func testDocumentEdgeAfterSelectAllUsesLogicalEdges() throws {
        _ = NSApplication.shared
        let source = """
        component App
          view
            list virtualized=true appearance="none" height=200 width=320 estimated-item-height=40 testId="list"
              for n in range(80) key=n
                text `Row ${n}` height=40 testId=`row-${n}`
        """
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
        compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        let session = ExactApp.shared.makeSession(label: "document-edge")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try Data(contentsOf: dir.appendingPathComponent("app.plan")), size: CGSize(width: 400, height: 300)).error)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.makeFirstResponder(nil); window.close() }
        window.contentView = ExactView(session: session)
        let p = session.presenter
        window.contentView?.layoutSubtreeIfNeeded(); settle(p)
        let list = try node(p, "list")
        move(p, list, to: 400)
        XCTAssertNil(p.views.values.first { $0.props["testId"] == "row-0" }, "row 0 retired")
        window.makeFirstResponder(try node(p, "row-12"))
        p.selection.selectAll(in: window)
        XCTAssertTrue(p.selection.modify(NSSelectorFromString("moveToBeginningOfDocumentAndModifySelection:")))
        XCTAssertEqual(p.selection.selectedText(), "")
        XCTAssertTrue(p.selection.modify(NSSelectorFromString("moveToEndOfDocumentAndModifySelection:")))
        XCTAssertEqual(p.selection.selectedText(), (0..<80).map { "Row \($0)" }.joined(separator: "\n\n"))
    }

    func testSelectAllCopiesLogicalRowsBetweenFixedParagraphsWithoutMountedRows() {
        _ = NSApplication.shared
        let p = Presenter()
        defer { p.reset() }
        p.onListText = { _, first, last in
            XCTAssertNil(first); XCTAssertNil(last)
            return "Header\n\nLogical row\n\nFooter"
        }
        p.apply(wireBatch([
            ["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0,
                "count": 100, "totalExtent": 5400, "rows": [], "fixed": [
                    ["view": 10, "roots": [2], "epoch": 1, "size": 600],
                    ["view": 11, "roots": [3], "epoch": 1, "size": 800]]]]],
            ["op": "create", "id": 1, "kind": "list"],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "Header"]],
            ["op": "create", "id": 3, "kind": "text", "props": ["text": "Footer"]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
        ]))
        p.selection.selectAll()
        XCTAssertEqual(p.selection.selectedText(), "Header\n\nLogical row\n\nFooter")
        XCTAssertEqual(p.selection.range(p.views[2]!), NSRange(location: 0, length: 6))
        XCTAssertEqual(p.selection.range(p.views[3]!), NSRange(location: 0, length: 6))
    }
    func testOuterSelectAllDoesNotHighlightNestedFixedParagraphs() {
        _ = NSApplication.shared
        let p = Presenter()
        defer { p.reset() }
        p.apply(wireBatch([
            ["op": "collections", "items": [
                ["view": 1, "revision": 1, "scrollSequence": 0, "count": 1, "totalExtent": 120,
                 "rows": [["view": 4, "root": 4, "index": 0, "start": 20, "size": 80, "epoch": 1]],
                 "fixed": [["view": 10, "roots": [2], "epoch": 1, "size": 20], ["view": 11, "roots": [3], "epoch": 1, "size": 20]]],
                ["view": 5, "revision": 1, "scrollSequence": 0, "count": 0, "totalExtent": 40, "rows": [],
                 "fixed": [["view": 12, "roots": [6], "epoch": 1, "size": 20], ["view": 13, "roots": [7], "epoch": 1, "size": 20]]],
            ]],
            ["op": "create", "id": 1, "kind": "list"],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "Header"]],
            ["op": "create", "id": 3, "kind": "text", "props": ["text": "Footer"]],
            ["op": "create", "id": 4, "kind": "view", "props": ["listItemKey": "n:0", "accessibilityPosInSet": "1"]],
            ["op": "create", "id": 5, "kind": "list"],
            ["op": "create", "id": 6, "kind": "text", "props": ["text": "Inner header"]],
            ["op": "create", "id": 7, "kind": "text", "props": ["text": "Inner footer"]],
            ["op": "create", "id": 8, "kind": "text", "props": ["text": "Outer row"]],
            ["op": "create", "id": 9, "kind": "text", "props": ["text": "After inner"]],
            ["op": "children", "id": 5, "ids": [6, 7]], ["op": "children", "id": 4, "ids": [8, 5, 9]],
            ["op": "children", "id": 1, "ids": [2, 4, 3]], ["op": "roots", "ids": [1]],
        ]))
        p.selection.set(p.views[2]!, FieldSelection(start: 0, end: 1))
        p.selection.selectAll()
        for id: UInt32 in [2, 3, 8, 9] { XCTAssertNotNil(p.selection.range(p.views[id]!)) }
        for id: UInt32 in [6, 7] { XCTAssertNil(p.selection.range(p.views[id]!)) }
        p.onListText = { _, first, last in
            XCTAssertEqual(first?.1, 1); XCTAssertEqual(last?.1, 1)
            return "Aft"
        }
        p.selection.set(p.views[9]!, FieldSelection(start: 0, end: 3))
        XCTAssertEqual(p.selection.selectedText(), "Aft")
    }
    #endif

}
