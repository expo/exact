#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

/// LLP 1116.019 D6 on UIKit: an action row's title is the cell's tint, as a
/// hand-built action cell's is, and a row that presses says so to VoiceOver;
/// a labelled text keeps its words as its value, and a symbol never speaks
/// the name UIKit gives it.
final class GroupedActionsIOSTests: GroupedListFixture {
    func testAnActionRowIsTheTintAndAPressingRowIsAButton() throws {
        let p = presenter {
            var m = self.model()
            m.sections[1].rows[0] = Row(view: 20, title: "Finish Workout", pressable: true, action: true)
            return m
        }
        defer { window.isHidden = true }
        let finish = try cell(p, 20)
        let c = try XCTUnwrap(finish.contentConfiguration as? UIListContentConfiguration)
        XCTAssertEqual(c.textProperties.color, .tintColor, "the action's title: the tint")
        let delete = try XCTUnwrap(try cell(p, 21).contentConfiguration as? UIListContentConfiguration)
        XCTAssertEqual(delete.textProperties.color, .systemRed, "a destructive row stays red")
        let profile = try XCTUnwrap(try cell(p, 10).contentConfiguration as? UIListContentConfiguration)
        XCTAssertNotEqual(profile.textProperties.color, .tintColor, "a row that pushes keeps the label's colour")
        XCTAssertTrue(finish.accessibilityTraits.contains(.button), "a `button` row is a button")
        XCTAssertFalse(try cell(p, 12).accessibilityTraits.contains(.button), "a row that presses nothing is not")
    }

    func testAStandardRowsContextMenuLiftsItsNativeCellAndFollowsReuse() throws {
        if ExactEnv.agentPaints { throw XCTSkip("the agent paints the popover (D4)") }
        let p = presenter { self.model() }
        defer { window.isHidden = true }
        let row = try XCTUnwrap(p.views[10])
        row.applyProps(set: ["contextPopover": "row-menu"], clear: [])
        p.apply(wireBatch([
            ["op": "create", "id": 40, "kind": "view", "props": ["popover": "auto", "id": "row-menu"], "style": [:], "handlers": []],
            ["op": "create", "id": 41, "kind": "button", "props": ["text": "Archive"], "style": [:], "handlers": ["press"]],
            ["op": "children", "id": 40, "ids": [41]],
            ["op": "children", "id": 1, "ids": [2, 3, 40]],
        ]))
        let native = try cell(p, 10), collection = try list(p).collection
        XCTAssertFalse(native.interactions.contains { $0 is UIContextMenuInteraction }, "no interaction of its own")
        let delegate = try XCTUnwrap(collection.delegate), path = try XCTUnwrap(collection.indexPath(for: native))
        let point = native.convert(CGPoint(x: 20, y: 20), to: collection)
        let menu = p.menus.context
        let configuration = try XCTUnwrap(delegate.collectionView?(collection, contextMenuConfigurationForItemsAt: [path], point: point))
        let owner = try XCTUnwrap(menu.open)
        XCTAssertTrue(owner.source === row)
        XCTAssertEqual(menu.menu(owner)?.children.count, 1)
        XCTAssertFalse(delegate.responds(to: #selector(UICollectionViewDelegate.collectionView(_:contextMenuConfiguration:highlightPreviewForItemAt:))),
                       "UIKit lifts the cell itself")
        delegate.collectionView?(collection, willEndContextMenuInteraction: configuration, animator: nil)
        XCTAssertNil(menu.open)
        row.applyProps(set: [:], clear: ["contextPopover"])
        try list(p).mount()
        XCTAssertNil(delegate.collectionView?(collection, contextMenuConfigurationForItemsAt: [path], point: point), "a row without its menu has none")
    }

    func testALabelledTextSpeaksItsWordsAsItsValue() {
        let p = Presenter()
        let readout = NodeView(id: 30, kind: "text", presenter: p)
        readout.applyProps(set: ["text": "1:05.32", "accessibilityLabel": "Elapsed time"], clear: [])
        XCTAssertEqual(readout.accessibilityLabel, "Elapsed time")
        XCTAssertEqual(readout.accessibilityValue, "1:05.32", "the label names the words; they are still read")
        readout.applyProps(set: ["accessibilityLabel": "1:05.32"], clear: [])
        XCTAssertNil(readout.accessibilityValue, "a label that is the words says them once")
        readout.applyProps(set: [:], clear: ["accessibilityLabel"])
        XCTAssertEqual(readout.accessibilityLabel, "1:05.32")
        XCTAssertNil(readout.accessibilityValue)
    }
}
#endif
