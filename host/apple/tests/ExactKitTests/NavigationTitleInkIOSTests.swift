#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// A bar title's authored colour and typography, over the native fixture:
/// each is the route's own item appearance, and a heading with none leaves
/// UIKit's title alone.
extension NavigationBarIOSTests {
    func testAuthoredClearTitleInkAndAbsenceUseDifferentAppearances() throws {
        let session = try fixture("title-ink", module: false)
        let heading = try node(session, "title-home")
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
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
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
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

    func testHeaderFamilyFollowsTheVisibleRoute() throws {
        let session = try fixture("header-family", module: false, plan: "HEADER_FAMILY")
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        let home = try XCTUnwrap(nav.topViewController)
        func fonts(_ text: String, in view: UIView) -> [UIFont] {
            guard !view.isHidden, view.alpha > 0 else { return [] }
            if let label = view as? UILabel, label.text == text { return [label.font] }
            return view.subviews.flatMap { fonts(text, in: $0) }
        }
        func drawn(_ text: String, in view: UIView) throws -> UIFont {
            nav.view.layoutIfNeeded()
            return try XCTUnwrap(fonts(text, in: view).max { $0.pointSize < $1.pointSize }, text)
        }
        let stock = try drawn("Open", in: nav.navigationBar)
        let stockBack = nav.navigationBar.standardAppearance.backButtonAppearance.normal.titleTextAttributes[.font] as? UIFont
        XCTAssertNil(home.navigationItem.standardAppearance)
        session.presenter.press(try node(session, "open").id)
        until("the pushed route is visible") {
            nav.topViewController?.navigationItem.title == "Notes" && nav.transitionCoordinator == nil
                && !session.presenter.navigationHost.transitioning
        }
        let item = try XCTUnwrap(nav.topViewController?.navigationItem)
        let georgia = Int(try node(session, "header").number("font_family"))
        let inline = try drawn("Notes", in: nav.navigationBar)
        XCTAssertEqual(inline.familyName, "Georgia")
        let save = try drawn("Save", in: nav.navigationBar)
        XCTAssertEqual(save.familyName, "Georgia")
        XCTAssertEqual(save.pointSize, stock.pointSize)
        XCTAssertEqual(home.navigationItem.backButtonTitle, "Shelf")
        let back = try XCTUnwrap(item.standardAppearance?.backButtonAppearance.normal.titleTextAttributes[.font] as? UIFont)
        XCTAssertEqual(try drawn("Shelf", in: nav.navigationBar), back)
        XCTAssertEqual(back.familyName, "Georgia")
        XCTAssertTrue(inline.fontDescriptor.symbolicTraits.contains(.traitBold))
        XCTAssertNil(item.scrollEdgeAppearance)
        XCTAssertEqual(nav.navigationBar.standardAppearance.backButtonAppearance.normal.titleTextAttributes[.font] as? UIFont, stockBack)

        let window = try XCTUnwrap(self.window)
        window.traitOverrides.preferredContentSizeCategory = .accessibilityExtraExtraExtraLarge
        until("Dynamic Type scales the title and item to UIKit's inline limit") {
            nav.view.layoutIfNeeded()
            return ["Notes", "Save"].allSatisfy { text in
                fonts(text, in: nav.navigationBar).contains { $0.pointSize == 21 && $0.familyName == "Georgia" }
            }
        }
        XCTAssertEqual((item.standardAppearance?.backButtonAppearance.normal.titleTextAttributes[.font] as? UIFont)?.pointSize, 21)
        window.traitOverrides.preferredContentSizeCategory = .large
        until("the item returns to the default size") {
            nav.view.layoutIfNeeded()
            return fonts("Save", in: nav.navigationBar).contains(save)
        }

        session.presenter.press(try node(session, "save").id)
        until("UIKit draws Notes as a large title") {
            nav.view.layoutIfNeeded()
            return fonts("Notes", in: nav.view).contains { $0.pointSize > inline.pointSize && $0.familyName == "Georgia" }
        }
        let large = try drawn("Notes", in: nav.view)
        window.traitOverrides.preferredContentSizeCategory = .accessibilityExtraExtraExtraLarge
        until("the expanded title follows the large-title text style") {
            nav.view.layoutIfNeeded()
            let expected = UIFont.preferredFont(forTextStyle: .largeTitle, compatibleWith: window.traitCollection)
            return expected.pointSize > large.pointSize && fonts("Notes", in: nav.view).contains { $0.pointSize == expected.pointSize && $0.familyName == "Georgia" }
        }
        window.traitOverrides.preferredContentSizeCategory = .large
        until("the expanded title returns to its stock size") {
            nav.view.layoutIfNeeded()
            return fonts("Notes", in: nav.view).contains(large)
        }
        let prominent = try drawn("Save", in: nav.navigationBar)
        XCTAssertEqual(prominent.familyName, "Georgia")
        if #available(iOS 26, *) { XCTAssertTrue(prominent.fontDescriptor.symbolicTraits.contains(.traitBold)) }
        session.presenter.press(try node(session, "save").id)
        XCTAssertEqual(try drawn("Notes", in: nav.view).familyName, "Menlo")
        XCTAssertEqual(try drawn("Save", in: nav.navigationBar), prominent)
        XCTAssertEqual(try drawn("Shelf", in: nav.navigationBar), back)
        session.presenter.press(try node(session, "save").id)
        until("an unavailable family falls back to the system") {
            nav.view.layoutIfNeeded()
            return fonts("Save", in: nav.navigationBar).first?.familyName == stock.familyName
        }
        let roles = ["Notes", "Save", "Shelf"]
        let fallback = try roles.map { try drawn($0, in: nav.navigationBar) }
        let header = try node(session, "header")
        let projected = try XCTUnwrap((item.standardAppearance ?? nav.navigationBar.standardAppearance).copy() as? UINavigationBarAppearance)
        projected.backgroundColor = .purple
        item.standardAppearance = projected
        session.presenter.apply(wireBatch([["op": "style", "id": header.id, "style": ["written": []]]]))
        XCTAssertEqual(item.standardAppearance?.backgroundColor, .purple)
        XCTAssertEqual(item.standardAppearance?.titleTextAttributes[.font] as? UIFont, nav.navigationBar.standardAppearance.titleTextAttributes[.font] as? UIFont)
        XCTAssertEqual(try drawn("Save", in: nav.navigationBar), stock)

        for (name, font) in zip(roles, fallback) {
            let system = try drawn(name, in: nav.navigationBar)
            XCTAssertEqual(font.pointSize, system.pointSize, name)
            XCTAssertEqual(TextEngine.controlWeight(font), TextEngine.controlWeight(system), name)
        }

        let compact = UINavigationBarAppearance()
        compact.titleTextAttributes[.font] = UIFont.boldSystemFont(ofSize: 24)
        item.compactAppearance = compact
        let route = try XCTUnwrap(nav.topViewController as? RouteController)
        let opaque = UINavigationBarAppearance()
        opaque.configureWithOpaqueBackground()
        opaque.backgroundColor = .red
        item.scrollEdgeAppearance = opaque
        let kept = try XCTUnwrap(item.rightBarButtonItems?.first { $0.title == "Save" })
        let custom = UIFont.boldSystemFont(ofSize: 20)
        kept.setTitleTextAttributes([.font: custom], for: .highlighted)
        let base = try XCTUnwrap(item.standardAppearance)
        base.backButtonAppearance.normal.titleTextAttributes[.font] = custom
        item.standardAppearance = base
        session.presenter.apply(wireBatch([["op": "style", "id": header.id, "style": ["font_family": georgia, "written": ["font_family"]]]]))
        let branded = try XCTUnwrap(item.standardAppearance)
        for (state, appearance): (UIControl.State, UIBarButtonItemStateAppearance) in [
            (.normal, branded.buttonAppearance.normal), (.highlighted, branded.buttonAppearance.highlighted),
            (.disabled, branded.buttonAppearance.disabled), (.focused, branded.buttonAppearance.focused)
        ] {
            let effective = kept.titleTextAttributes(for: state)?[.font] ?? appearance.titleTextAttributes[.font]
            let font = try XCTUnwrap(effective as? UIFont)
            XCTAssertEqual(font.familyName, "Georgia")
            XCTAssertEqual(font.pointSize, stock.pointSize)
        }
        let compactTitle = try XCTUnwrap(item.compactAppearance?.titleTextAttributes[.font] as? UIFont)
        XCTAssertEqual(compactTitle, inline)
        XCTAssertEqual(branded.backButtonAppearance.normal.titleTextAttributes[.font] as? UIFont, back)
        session.presenter.apply(wireBatch([["op": "props", "id": route.node.id, "set": ["navigationBar": "transparent"]]]))
        let clear = try XCTUnwrap(item.scrollEdgeAppearance?.copy() as? UINavigationBarAppearance)
        clear.backButtonAppearance.normal.titlePositionAdjustment = UIOffset(horizontal: 3, vertical: 1)
        item.scrollEdgeAppearance = clear
        session.presenter.apply(wireBatch([["op": "props", "id": route.node.id, "set": ["navigationBar": "auto"]]]))
        XCTAssertEqual(item.scrollEdgeAppearance?.backgroundColor, .red)
        XCTAssertEqual(item.scrollEdgeAppearance?.backButtonAppearance.normal.titlePositionAdjustment.horizontal, 3)
        session.presenter.apply(wireBatch([["op": "style", "id": header.id, "style": ["written": []]]]))
        kept.setTitleTextAttributes([.font: custom], for: .highlighted)
        session.presenter.navigationHost.prepareRoutes(nav.viewControllers.compactMap { $0 as? RouteController }, in: nav)
        XCTAssertEqual(kept.titleTextAttributes(for: .highlighted)?[.font] as? UIFont, custom)
        XCTAssertEqual(item.standardAppearance?.backButtonAppearance.normal.titleTextAttributes[.font] as? UIFont, custom)
        func cascadeAfterPress() throws -> [UIFontDescriptor] {
            session.presenter.press(try node(session, "save").id)
            let font = try drawn("Save", in: nav.navigationBar)
            XCTAssertEqual(font.familyName, "Georgia")
            return try XCTUnwrap(font.fontDescriptor.object(forKey: .cascadeList) as? [UIFontDescriptor])
        }
        let first = try cascadeAfterPress()
        XCTAssertNotEqual(try cascadeAfterPress(), first)
        session.presenter.press(try node(session, "back").id)
        until("the original bar is visible") { nav.topViewController === home && nav.transitionCoordinator == nil }
        XCTAssertEqual(try drawn("Open", in: nav.navigationBar), stock)
        XCTAssertEqual(nav.navigationBar.standardAppearance.backButtonAppearance.normal.titleTextAttributes[.font] as? UIFont, stockBack)
        XCTAssertNil(home.navigationItem.standardAppearance)
    }

    func testTransparentTitleSurvivesRepeatedRoutePreparation() throws {
        let session = try fixture("transparent-title-ink", module: false)
        let p = session.presenter
        let heading = try node(session, "title-home")
        let route = try node(session, "route-home")
        let item = try XCTUnwrap(p.navigationHost.primaryNavigation?.topViewController?.navigationItem)
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

extension NavigationBasicsIOSTests {
    func testAHeaderFamilyKeepsARichTitlesOwnTextSize() throws {
        let session = try fixture("rich-header-family")
        let (_, _, route) = try chat(session)
        let title = try XCTUnwrap(route.navigationItem.titleView as? HeaderTitleView)
        let shape = try XCTUnwrap(HeaderShape.bar(of: route.node))
        let heading = try XCTUnwrap(shape.heading)
        session.presenter.apply(wireBatch([["op": "style", "id": heading.id, "style": ["written": []]]]))
        title.traitOverrides.preferredContentSizeCategory = .accessibilityExtraExtraExtraLarge
        title.updateTraitsIfNeeded()
        let size = title.title.font.pointSize
        XCTAssertGreaterThan(size, 21)
        session.presenter.apply(wireBatch([["op": "style", "id": shape.header.id, "style": ["font_family": 5, "written": ["font_family"]]]]))
        XCTAssertEqual(title.title.font.pointSize, size)
        XCTAssertTrue(title.title.font.fontDescriptor.symbolicTraits.contains(.traitMonoSpace))
    }
}
#endif
