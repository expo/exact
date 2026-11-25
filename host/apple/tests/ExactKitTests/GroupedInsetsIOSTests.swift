#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

/// LLP 1123 D15: the regions a grouped list's insets are measured from, its rows' content
/// margins and background insets, and where its indicators lie.
final class GroupedInsetsIOSTests: GroupedListFixture {
    func testTheListsInsetsReferencesSetItsLayoutAndSections() throws {
        let p = presenter { self.model() }
        defer { window.isHidden = true }
        let collection = try list(p).collection
        let layout = try XCTUnwrap(collection.collectionViewLayout as? UICollectionViewCompositionalLayout)
        // Margins wide enough to tell measuring from them from measuring from the list's edge
        collection.preservesSuperviewLayoutMargins = false
        collection.insetsLayoutMarginsFromSafeArea = false
        collection.directionalLayoutMargins = NSDirectionalEdgeInsets(top: 0, leading: 60, bottom: 0, trailing: 60)
        let card = { (rows: [String: Any]) throws -> CGFloat in
            p.apply(wireBatch([["op": "style", "id": 1, "style": rows.merging(["overflow_y": "scroll"]) { a, _ in a }]]))
            return try self.cell(p, 10).frame.minX
        }
        let margins = try card(["list_section_insets_reference": "layout-margins"])
        let edge = try card(["list_section_insets_reference": "none"])
        XCTAssertGreaterThanOrEqual(margins, 60, "the card is inset from the list's margins")
        XCTAssertLessThan(edge, margins, "the card is inset from the list's edge")
        // The layout's own region follows its row, and UIKit's choice comes back as it goes
        _ = try card(["list_insets_reference": "none"])
        XCTAssertEqual(layout.configuration.contentInsetsReference, .none)
        _ = try card([:])
        XCTAssertEqual(layout.configuration.contentInsetsReference, .automatic)
    }

    /// A list's indicators lie beside the side safe area its cards are inset from, as a plain scroller's do: UIKit's
    /// adjustment never insets them from the collection's side, which on the iPhone Duo's cover screen is under the
    /// button strip (`SafeIndicatorTerm`).
    func testAListsIndicatorsKeepBesideItsSideSafeArea() throws {
        let p = presenter { self.model() }
        let l = try list(p)
        let vertical = l.collection.verticalScrollIndicatorInsets, horizontal = l.collection.horizontalScrollIndicatorInsets
        l.collection.sideSafeArea = (0, 84)
        XCTAssertEqual(l.collection.verticalScrollIndicatorInsets.right - vertical.right, 84, accuracy: 0.5)
        XCTAssertEqual(l.collection.horizontalScrollIndicatorInsets.right - horizontal.right, 84, accuracy: 0.5)
        l.collection.sideSafeArea = (0, 0)
        XCTAssertEqual(l.collection.verticalScrollIndicatorInsets, vertical, "and gone with it")
    }

    func testARowsMarginsAndBackgroundInsetsReachItsCellThroughItsUpdates() throws {
        let p = presenter { self.model() }
        defer { window.isHidden = true }
        let said: [String: Any] = [
            "text_color": [0, 0, 0, 255],
            "list_cell_margins_top": 4.0, "list_cell_margins_right": 6.0, "list_cell_margins_bottom": 4.0, "list_cell_margins_left": 30.0,
            "list_background_insets_top": 0.0, "list_background_insets_right": 0.0, "list_background_insets_bottom": 0.0, "list_background_insets_left": 12.0,
        ]
        p.apply(wireBatch([["op": "style", "id": 10, "style": said]]))
        let c = try cell(p, 10)
        c.layoutIfNeeded()
        XCTAssertEqual((c.contentConfiguration as? UIListContentConfiguration)?.directionalLayoutMargins,
                       NSDirectionalEdgeInsets(top: 4, leading: 30, bottom: 4, trailing: 6))
        XCTAssertEqual(c.backgroundConfiguration?.backgroundInsets, NSDirectionalEdgeInsets(top: 0, leading: 12, bottom: 0, trailing: 0))
        // UIKit's own update for a pressed row keeps them
        c.isHighlighted = true
        c.layoutIfNeeded()
        XCTAssertEqual(c.backgroundConfiguration?.backgroundInsets.leading, 12)
        c.isHighlighted = false
        // Said no more, UIKit's again
        p.apply(wireBatch([["op": "style", "id": 10, "style": ["text_color": [0, 0, 0, 255]]]]))
        let again = try cell(p, 10)
        again.layoutIfNeeded()
        XCTAssertNotEqual((again.contentConfiguration as? UIListContentConfiguration)?.directionalLayoutMargins.leading, 30)
        XCTAssertNotEqual(again.backgroundConfiguration?.backgroundInsets.leading, 12)
    }
}
#endif
