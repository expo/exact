#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A bar title's authored colour and typography, over the native fixture:
/// each is the route's own item appearance, and a heading with none leaves
/// UIKit's title alone.
extension NavigationBarIOSTests {
    func testAuthoredClearTitleInkAndAbsenceUseDifferentAppearances() throws {
        let session = try fixture("title-ink", module: false)
        let heading = try node(session, "title-home")
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        let item = try XCTUnwrap(nav.topViewController?.navigationItem)
        XCTAssertNil(item.standardAppearance)
        session.presenter.apply(wireBatch([["op": "style", "id": heading.id,
            "style": ["text_color": [0, 0, 0, 0], "written": ["text_color"]]]]))
        let appearance = try XCTUnwrap(item.standardAppearance)
        XCTAssertEqual((appearance.titleTextAttributes[.foregroundColor] as? UIColor)?.cgColor.alpha, 0)
        XCTAssertEqual((appearance.largeTitleTextAttributes[.foregroundColor] as? UIColor)?.cgColor.alpha, 0)
        session.presenter.apply(wireBatch([["op": "style", "id": heading.id,
            "style": ["text_color": [255, 0, 0, 255]]]]))
        XCTAssertNil(item.standardAppearance, "an inherited colour leaves UIKit's title alone")
    }

    /// LLP 1115 §2, "what the author writes wins": the heading's authored
    /// `font-family`, `font-weight` and `font-size` are the bar's title font
    /// in each of its item's appearances, and UIKit draws it. The fixture's
    /// home heading is level 1, so its size is the large title's and the
    /// collapsed inline title keeps UIKit's size. A row left unsaid stays
    /// UIKit's, and a heading with no authored typography (an inherited
    /// font is not authored) leaves UIKit's title alone.
    func testAHeadingsAuthoredTypographyIsTheBarsTitleFont() throws {
        let session = try fixture("title-font", module: false)
        let heading = try node(session, "title-home"), engine = try XCTUnwrap(heading.text)
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        let route = try XCTUnwrap(nav.topViewController), item = route.navigationItem
        func labels(_ view: UIView) -> [UILabel] { ((view as? UILabel).map { [$0] } ?? []) + view.subviews.flatMap(labels) }
        func drawnLarge() -> UIFont? {
            // Over a route's scroller, iOS 26 draws the large title in the
            // scroll content rather than in the bar.
            nav.view.layoutIfNeeded()
            return labels(nav.view).filter { $0.text == "Fixture" }.map(\.font).max { $0.pointSize < $1.pointSize }
        }
        let platform = NavigationTitleInk.platformFonts(route.traitCollection)
        XCTAssertNil(item.standardAppearance, "no authored typography: UIKit's title")
        let own = try XCTUnwrap(drawnLarge(), "UIKit draws the large title")
        XCTAssertEqual(own.pointSize, platform.large.pointSize, "the large title UIKit draws is the one an unsaid size keeps")
        XCTAssertEqual(TextEngine.cssWeight(of: own), TextEngine.cssWeight(of: platform.large), "and its weight")

        session.presenter.apply(wireBatch([["op": "style", "id": heading.id,
            "style": ["font_family": 5, "font_weight": 800, "font_size": 30, "written": ["font_family", "font_weight", "font_size"]]]]))
        let large = engine.font(size: 30, weight: 800, family: 5, italic: false)
        let inline = engine.font(size: platform.title.pointSize, weight: 800, family: 5, italic: false)
        for appearance in [item.standardAppearance, item.compactAppearance, item.scrollEdgeAppearance, item.compactScrollEdgeAppearance] {
            let appearance = try XCTUnwrap(appearance)
            XCTAssertEqual(appearance.largeTitleTextAttributes[.font] as? UIFont, large)
            XCTAssertEqual(appearance.titleTextAttributes[.font] as? UIFont, inline)
        }
        XCTAssertEqual(drawnLarge(), large, "UIKit draws the authored font")

        session.presenter.apply(wireBatch([["op": "style", "id": heading.id, "style": ["font_family": 5, "written": ["font_family"]]]]))
        let appearance = try XCTUnwrap(item.standardAppearance)
        XCTAssertEqual(appearance.largeTitleTextAttributes[.font] as? UIFont,
                       engine.font(size: platform.large.pointSize, weight: TextEngine.cssWeight(of: platform.large) ?? 0, family: 5, italic: false),
                       "a family alone keeps the large title's size and weight")
        XCTAssertEqual(appearance.titleTextAttributes[.font] as? UIFont,
                       engine.font(size: platform.title.pointSize, weight: TextEngine.cssWeight(of: platform.title) ?? 0, family: 5, italic: false),
                       "and the inline title's")

        session.presenter.apply(wireBatch([["op": "style", "id": heading.id, "style": ["font_family": 5, "font_weight": 800, "font_size": 30]]]))
        XCTAssertNil(item.standardAppearance, "an inherited font leaves UIKit's title alone")
        XCTAssertEqual(drawnLarge()?.pointSize, own.pointSize)
    }

    func testTransparentTitleSurvivesRepeatedRoutePreparation() throws {
        let session = try fixture("transparent-title-ink", module: false)
        let p = session.presenter
        let heading = try node(session, "title-home")
        let route = try node(session, "route-home")
        let item = try XCTUnwrap(p.navigation.primaryNavigation?.topViewController?.navigationItem)
        p.apply(wireBatch([
            ["op": "props", "id": route.id, "set": ["navigationBar": "transparent"]],
            ["op": "style", "id": heading.id, "style": ["text_color": [0, 0, 0, 0], "written": ["text_color"]]]
        ]))
        for _ in 0..<2 {
            let edge = try XCTUnwrap(item.scrollEdgeAppearance)
            XCTAssertNil(edge.backgroundEffect)
            XCTAssertEqual(edge.backgroundColor?.cgColor.alpha ?? 0, 0)
            XCTAssertEqual((edge.titleTextAttributes[.foregroundColor] as? UIColor)?.cgColor.alpha, 0)
            XCTAssertEqual((edge.largeTitleTextAttributes[.foregroundColor] as? UIColor)?.cgColor.alpha, 0)
            p.apply(wireBatch([["op": "props", "id": heading.id, "set": ["accessibilityLabel": "Title"]]]))
        }
        p.apply(wireBatch([["op": "style", "id": heading.id, "style": ["text_color": [255, 0, 0, 255]]]]))
        XCTAssertEqual(item.scrollEdgeAppearance, NavigationHost.clearScrollEdge)
    }
}
#endif
