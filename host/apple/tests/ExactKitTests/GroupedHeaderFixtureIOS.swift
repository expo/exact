#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

/// A grouped list with a custom section header, for the tests of custom headers and of reach.
extension GroupedListFixture {
    var headerKind: String { UICollectionView.elementKindSectionHeader }

    /// Two sections of five rows: the first's header custom when `custom`,
    /// holding a control when `parts`.
    func favorites(_ style: String, custom: Bool = true, parts: Bool = false) -> GroupedListModel {
        var first = GroupedListModel.Section(view: 2, header: "Ryu", footer: nil, rows: [10, 11, 12].map { Row(view: $0, title: "Move \($0)", pressable: true) })
        if custom { first.headerView = 40; first.headerParts = parts }
        return GroupedListModel(style: style, sections: [
            first,
            .init(view: 3, header: "Plain", footer: nil, rows: [20, 21].map { Row(view: $0, title: "Move \($0)", pressable: true) }),
        ])
    }

    /// The kernel's boxes for `header` → `row` → an avatar holding an
    /// image, then a text 8 points after it.
    func headerOps(height: Double = 44) -> [[String: Any]] {
        let view = { (id: Int, kind: String, props: [String: String]) -> [String: Any] in
            ["op": "create", "id": id, "kind": kind, "props": props, "style": ["text_color": [0, 0, 0, 255]]]
        }
        return [
            view(40, "view", ["semanticTag": "header", "testId": "header"]), view(44, "view", [:]),
            view(41, "view", ["accessibilityElementsHidden": "true"]),
            ["op": "create", "id": 43, "kind": "image", "props": ["imageSource": "symbol:person", "symbolName": "person.circle"], "style": ["font_size": 17.0]],
            view(42, "text", ["text": "Ryu"]),
            ["op": "children", "id": 2, "ids": [40, 4]], ["op": "children", "id": 40, "ids": [44]],
            ["op": "children", "id": 44, "ids": [41, 42]], ["op": "children", "id": 41, "ids": [43]],
            ["op": "frame", "id": 40, "x": 0.0, "y": 0.0, "w": 402.0, "h": height],
            ["op": "frame", "id": 44, "x": 16.0, "y": 10.0, "w": 370.0, "h": 24.0],
            ["op": "frame", "id": 41, "x": 0.0, "y": 0.0, "w": 24.0, "h": 24.0],
            ["op": "frame", "id": 43, "x": 0.0, "y": 0.0, "w": 24.0, "h": 24.0],
            ["op": "frame", "id": 42, "x": 32.0, "y": 2.0, "w": 100.0, "h": 20.0],
        ]
    }

    func header(_ p: Presenter, section: Int) throws -> GroupedHeaderView {
        let l = try list(p)
        l.collection.layoutIfNeeded()
        return try XCTUnwrap(l.collection.supplementaryView(forElementKind: headerKind, at: IndexPath(item: 0, section: section)) as? GroupedHeaderView,
                             "section \(section) shows a header")
    }
}
#endif
