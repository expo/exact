#if os(iOS)
import XCTest
import UIKit
@testable import ExactKit
@testable import ExactNavigation
@testable import ExactNativeViews

extension NavigationBarIOSTests {
    func testAChangedItemSizeAppliesAfterTheHatchClearsItsFont() throws {
        let session = try fixture("header-family-cleared-font", module: false, plan: "HEADER_FAMILY")
        let presenter = session.presenter
        let nav = try XCTUnwrap(presenter.navigationHost.primaryNavigation)
        presenter.press(try node(session, "open").id)
        until("the Notes route is visible") {
            nav.topViewController?.navigationItem.title == "Notes" && nav.transitionCoordinator == nil
        }
        let save = try node(session, "save")
        let readFace = presenter.buttonFace
        var face = ButtonFace()
        face.title = "Save"
        presenter.buttonFace = { $0 == save.id ? face : readFace?($0) ?? ButtonFace() }
        func prepare() {
            presenter.navigationHost.prepareRoutes(nav.viewControllers.compactMap { $0 as? RouteController }, in: nav)
        }
        func size(_ points: Double) {
            face.rows.title = ["font_size": .number(points)]
            presenter.apply(facesBatch())
            prepare()
        }
        size(20)
        let item = try XCTUnwrap(nav.topViewController?.navigationItem.rightBarButtonItems?.first { $0.title == "Save" })
        XCTAssertEqual((item.titleTextAttributes(for: .normal)?[.font] as? UIFont)?.pointSize, 20)
        let native = try XCTUnwrap(UIFont(name: "Menlo", size: 23))
        item.setTitleTextAttributes([.font: native], for: .normal)
        prepare()
        let normal = try XCTUnwrap(item.titleTextAttributes(for: .normal)?[.font] as? UIFont)
        XCTAssertEqual(normal.familyName, "Georgia")
        XCTAssertEqual(normal.pointSize, 20)
        item.setTitleTextAttributes(nil, for: .normal)
        prepare()
        size(22)
        for state: UIControl.State in [.normal, .highlighted, .disabled, .focused] {
            let font = try XCTUnwrap(item.titleTextAttributes(for: state)?[.font] as? UIFont)
            XCTAssertEqual(font.pointSize, 22)
            XCTAssertEqual(font.familyName, "Georgia")
        }
    }

    func testRemovingTheFamilyReturnsTheItemToItsRouteHatch() throws {
        let session = try fixture("family-native-item", module: true, plan: "HEADER_FAMILY")
        let natives = try XCTUnwrap(session.nativeViews)
        until("the native route hatch is connected") { natives.hatchCalls != nil }
        natives.hatchCalls?.route = { _, event, controller, _, _, _, _ in
            guard event != RouteHatchEvent.ended.rawValue, let controller else { return }
            let route = Unmanaged<UIViewController>.fromOpaque(controller).takeUnretainedValue()
            guard let save = route.navigationItem.rightBarButtonItems?.first(where: { $0.title == "Save" }) else { return }
            save.setTitleTextAttributes([.font: UIFont(name: "Menlo", size: 23)!, .kern: 2], for: .normal)
        }
        let host = session.presenter.navigationHost
        let nav = try XCTUnwrap(host.primaryNavigation)
        session.presenter.press(try node(session, "open").id)
        until("the Notes route is visible") {
            nav.topViewController?.navigationItem.title == "Notes" && nav.transitionCoordinator == nil
        }
        let route = try XCTUnwrap(nav.topViewController as? RouteController)
        let kept = try XCTUnwrap(route.navigationItem.rightBarButtonItems?.first { $0.title == "Save" })
        XCTAssertEqual(kept.titleTextAttributes(for: .normal)?[.kern] as? Int, 2)
        let appearance = try XCTUnwrap(route.navigationItem.standardAppearance)
        let effective = kept.titleTextAttributes(for: .normal)?[.font] ?? appearance.buttonAppearance.normal.titleTextAttributes[.font]
        XCTAssertEqual((effective as? UIFont)?.familyName, "Georgia")
        let header = try node(session, "header")
        session.presenter.apply(wireBatch([["op": "style", "id": header.id, "style": ["written": []]]]))
        host.prepareRoutes(nav.viewControllers.compactMap { $0 as? RouteController }, in: nav)
        XCTAssertEqual(kept.titleTextAttributes(for: .normal)?[.font] as? UIFont, UIFont(name: "Menlo", size: 23))
    }

    func testAnUnknownFamilyFollowsLiveBarChanges() throws {
        let session = try fixture("family-live-bar", module: false, plan: "HEADER_FAMILY")
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        session.presenter.press(try node(session, "open").id)
        until("the Notes route is visible") {
            nav.topViewController?.navigationItem.title == "Notes" && nav.transitionCoordinator == nil
        }
        let route = try XCTUnwrap(nav.topViewController)
        for _ in 0..<3 { session.presenter.press(try node(session, "save").id) }
        let blue = try XCTUnwrap(nav.navigationBar.standardAppearance.copy() as? UINavigationBarAppearance)
        blue.backgroundColor = .blue
        nav.navigationBar.standardAppearance = blue
        nav.view.layoutIfNeeded()
        let effective = route.navigationItem.standardAppearance ?? nav.navigationBar.standardAppearance
        XCTAssertEqual(effective.backgroundColor, .blue, "an unavailable family keeps the live bar appearance")
    }

    func testKeptItemDropsAnEarlierSizeWhileTheHeaderHasAFamily() throws {
        let session = try fixture("header-family-item-size", module: false, plan: "HEADER_FAMILY")
        let presenter = session.presenter
        let nav = try XCTUnwrap(presenter.navigationHost.primaryNavigation)
        presenter.press(try node(session, "open").id)
        until("the Notes route is visible") {
            nav.topViewController?.navigationItem.title == "Notes" && nav.transitionCoordinator == nil
        }
        let route = try XCTUnwrap(nav.topViewController as? RouteController)
        let header = try node(session, "header"), save = try node(session, "save")
        let family = Int(header.number("font_family"))
        let readFace = presenter.buttonFace
        var face = ButtonFace()
        face.title = "Save"
        face.rows.title = ["font_size": .number(12)]
        presenter.buttonFace = { $0 == save.id ? face : readFace?($0) ?? ButtonFace() }
        func prepare(_ hasFamily: Bool) {
            let rows: [String: Any] = hasFamily ? ["font_family": family, "written": ["font_family"]] : ["written": []]
            presenter.apply(facesBatch())
            presenter.apply(wireBatch([["op": "style", "id": header.id, "style": rows]]))
            presenter.navigationHost.prepareRoutes(nav.viewControllers.compactMap { $0 as? RouteController }, in: nav)
        }
        prepare(false)
        let kept = try XCTUnwrap(route.navigationItem.rightBarButtonItems?.first { $0.title == "Save" })
        XCTAssertEqual((kept.titleTextAttributes(for: .normal)?[.font] as? UIFont)?.pointSize, 12)
        prepare(true)
        XCTAssertEqual((kept.titleTextAttributes(for: .normal)?[.font] as? UIFont)?.familyName, "Georgia")
        face.rows.title = [:]
        prepare(true)
        XCTAssertNil(kept.titleTextAttributes(for: .normal)?[.font], "the item inherits the bar's font after its written size is removed")
    }

    func testAHeaderWithoutAFamilyLeavesTheAppearancesInPlace() throws {
        let session = try fixture("stock-header-appearances", module: false, plan: "HEADER_FAMILY")
        let host = session.presenter.navigationHost
        let nav = try XCTUnwrap(host.primaryNavigation)
        let route = try XCTUnwrap(nav.topViewController as? RouteController)
        let item = route.navigationItem
        let slots: [ReferenceWritableKeyPath<UINavigationItem, UINavigationBarAppearance?>] = [
            \.standardAppearance, \.compactAppearance, \.scrollEdgeAppearance, \.compactScrollEdgeAppearance
        ]
        XCTAssertTrue(slots.allSatisfy { item[keyPath: $0] == nil })
        let bar = nav.navigationBar.standardAppearance
        let before = try XCTUnwrap(bar.copy() as? UINavigationBarAppearance)
        let appearance = UINavigationBarAppearance()
        appearance.titleTextAttributes = [.font: UIFont.italicSystemFont(ofSize: 23), .foregroundColor: UIColor.orange]
        appearance.buttonAppearance.highlighted.titleTextAttributes = [.kern: 2]
        let action = try XCTUnwrap(item.rightBarButtonItems?.first)
        for slot in slots { item[keyPath: slot] = appearance }
        let originals = slots.map { item[keyPath: $0] }
        host.prepareRoutes([route], in: nav)
        for (slot, original) in zip(slots, originals) {
            let current = try XCTUnwrap(item[keyPath: slot])
            XCTAssertTrue(current === original)
            XCTAssertEqual(current, appearance)
            XCTAssertEqual(current.titleTextAttributes as NSDictionary, appearance.titleTextAttributes as NSDictionary)
            XCTAssertEqual(current.buttonAppearance.highlighted.titleTextAttributes as NSDictionary,
                           appearance.buttonAppearance.highlighted.titleTextAttributes as NSDictionary)
        }
        XCTAssertTrue(nav.navigationBar.standardAppearance === bar)
        XCTAssertEqual(bar, before)
        XCTAssertNil(action.titleTextAttributes(for: .normal)?[.font])
    }

    func testHeaderFamilyPreservesTitleAndItemMetricsAcrossRouteUpdates() throws {
        let session = try fixture("header-family-route-updates", module: false, plan: "HEADER_FAMILY")
        let host = session.presenter.navigationHost
        let nav = try XCTUnwrap(host.primaryNavigation)
        session.presenter.press(try node(session, "open").id)
        until("the Notes route is visible") {
            nav.topViewController?.navigationItem.title == "Notes" && nav.transitionCoordinator == nil
        }
        let route = try XCTUnwrap(nav.topViewController as? RouteController)
        let item = route.navigationItem
        let header = try node(session, "header")
        let family = Int(header.number("font_family"))
        let heading = try XCTUnwrap(HeaderShape.bar(of: route.node)?.heading)
        func style(_ id: UInt32, _ rows: [String: Any]) {
            session.presenter.apply(wireBatch([["op": "style", "id": id, "style": rows]]))
        }
        func prepare() {
            host.prepareRoutes(nav.viewControllers.compactMap { $0 as? RouteController }, in: nav)
            nav.view.layoutIfNeeded()
        }
        func drawn(_ title: String) throws -> UIFont {
            func labels(_ view: UIView) -> [UILabel] {
                guard !view.isHidden, view.alpha > 0 else { return [] }
                if let label = view as? UILabel, label.text == title { return [label] }
                return view.subviews.flatMap(labels)
            }
            nav.view.layoutIfNeeded()
            return try XCTUnwrap(labels(nav.navigationBar).first?.font, title)
        }
        func setFamily(_ enabled: Bool) {
            style(header.id, enabled ? ["font_family": family, "written": ["font_family"]] : ["written": []])
        }
        let window = try XCTUnwrap(self.window)
        window.traitOverrides.preferredContentSizeCategory = .accessibilityExtraExtraExtraLarge
        window.updateTraitsIfNeeded()
        prepare()
        let inline = try drawn("Notes")
        style(heading.id, ["font_weight": 600, "written": ["font_weight"]])
        let weighted = try drawn("Notes")
        XCTAssertEqual(inline.pointSize, 21)
        XCTAssertEqual(weighted.familyName, "Georgia")
        XCTAssertEqual(weighted.pointSize, 53, "the parent resolves a weight-only heading against the full headline size")
        style(heading.id, ["written": []])
        window.traitOverrides.preferredContentSizeCategory = .large
        window.updateTraitsIfNeeded()
        prepare()

        let hatch = try XCTUnwrap(item.standardAppearance?.copy() as? UINavigationBarAppearance)
        hatch.titleTextAttributes[.font] = UIFont(name: "Menlo", size: 23)
        item.standardAppearance = hatch
        prepare()
        XCTAssertEqual(try drawn("Notes").familyName, "Georgia")

        setFamily(false)
        item.standardAppearance = hatch
        prepare()
        XCTAssertEqual(try drawn("Notes").familyName, "Menlo")
        item.standardAppearance = nil
        let red = UINavigationBarAppearance()
        red.configureWithOpaqueBackground()
        red.backgroundColor = .red
        red.buttonAppearance.normal.titleTextAttributes[.font] = UIFont.boldSystemFont(ofSize: 24)
        nav.navigationBar.standardAppearance = red
        window.traitOverrides.preferredContentSizeCategory = .extraLarge
        window.updateTraitsIfNeeded()
        prepare()
        let barItem = try drawn("Save")
        setFamily(true)
        let changed = try drawn("Save")
        XCTAssertEqual(changed.familyName, "Georgia")
        XCTAssertEqual(barItem.pointSize, 24)
        XCTAssertEqual(changed.pointSize, 19, "the header owns the text item at extra-large")
        let blue = try XCTUnwrap(red.copy() as? UINavigationBarAppearance)
        blue.backgroundColor = .blue
        nav.navigationBar.standardAppearance = blue
        prepare()
        XCTAssertEqual(item.standardAppearance?.backgroundColor, .blue, "the item follows the bar's current background")
    }

    func testHeaderFamilyRemovalPreservesTheInheritedScrollEdgeTitleColor() throws {
        let session = try fixture("edge-family", module: false, plan: "HEADER_FAMILY")
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        session.presenter.press(try node(session, "open").id)
        until("the Notes route is visible") {
            nav.topViewController?.navigationItem.title == "Notes" && nav.transitionCoordinator == nil
        }
        let route = try XCTUnwrap(nav.topViewController as? RouteController)
        let item = route.navigationItem
        let header = try node(session, "header"), family = Int(header.number("font_family"))
        func setFamily(_ enabled: Bool) {
            let style: [String: Any] = enabled ? ["font_family": family, "written": ["font_family"]] : ["written": []]
            session.presenter.apply(wireBatch([["op": "style", "id": header.id, "style": style]]))
        }
        func transparent(_ enabled: Bool) {
            session.presenter.apply(wireBatch([["op": "props", "id": route.node.id, "set": ["navigationBar": enabled ? "transparent" : "auto"]]]))
        }
        setFamily(false)
        let base = UINavigationBarAppearance()
        base.titleTextAttributes[.foregroundColor] = UIColor.orange
        item.standardAppearance = base
        item.scrollEdgeAppearance = nil
        nav.navigationBar.scrollEdgeAppearance = nil
        setFamily(true)
        transparent(true)
        let edge = try XCTUnwrap(item.scrollEdgeAppearance?.copy() as? UINavigationBarAppearance)
        edge.backButtonAppearance.normal.titlePositionAdjustment = UIOffset(horizontal: 3, vertical: 0)
        item.scrollEdgeAppearance = edge
        transparent(false)
        setFamily(false)
        let restored = try XCTUnwrap(item.scrollEdgeAppearance ?? nav.navigationBar.scrollEdgeAppearance ?? item.standardAppearance)
        XCTAssertEqual(restored.titleTextAttributes[.foregroundColor] as? UIColor, .orange)
        XCTAssertEqual(restored.backButtonAppearance.normal.titlePositionAdjustment.horizontal, 3)
    }

    func testUnavailableHeaderFamilyKeepsEachVisibleSystemFontWeight() throws {
        let session = try fixture("header-family-fallback-weight", module: false, plan: "HEADER_FAMILY")
        let navigation = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        session.presenter.press(try node(session, "open").id)
        until("the Notes route is visible") {
            navigation.topViewController?.navigationItem.title == "Notes" && navigation.transitionCoordinator == nil
        }
        for _ in 0..<3 { session.presenter.press(try node(session, "save").id) }
        let header = try node(session, "header")
        let missing = Int(header.number("font_family"))
        let window = try XCTUnwrap(self.window)
        func weight(_ title: String) throws -> CGFloat {
            func label(in view: UIView) -> UILabel? {
                guard !view.isHidden, view.alpha > 0 else { return nil }
                if let label = view as? UILabel, label.text == title { return label }
                return view.subviews.lazy.compactMap(label(in:)).first
            }
            navigation.view.layoutIfNeeded()
            let font = try XCTUnwrap(label(in: navigation.navigationBar)?.font, title)
            let traits = try XCTUnwrap(font.fontDescriptor.object(forKey: .traits) as? [UIFontDescriptor.TraitKey: Any])
            return try XCTUnwrap(traits[.weight] as? CGFloat, title)
        }
        let titles = ["Notes", "Save", "Shelf"]
        for legibility in [UILegibilityWeight.regular, .bold] {
            window.traitOverrides.legibilityWeight = legibility
            window.updateTraitsIfNeeded()
            func setFamily(_ style: [String: Any]) {
                session.presenter.apply(wireBatch([["op": "style", "id": header.id, "style": style]]))
            }
            setFamily(["written": []])
            let stock = try titles.map(weight)
            setFamily(["font_family": missing, "written": ["font_family"]])
            for (title, expected) in zip(titles, stock) {
                XCTAssertEqual(try weight(title), expected, accuracy: 0.001, "\(title), \(legibility == .bold ? "bold" : "regular") legibility")
            }
        }
    }
}
extension NavigationBasicsIOSTests {
    func testAnUnbrandedRichTitleKeepsItsHatchFontWhenLegibilityChanges() throws {
        let session = try fixture("plain-rich-hatch")
        let (_, _, route) = try chat(session)
        let view = try XCTUnwrap(route.navigationItem.titleView as? HeaderTitleView)
        let font = try XCTUnwrap(UIFont(name: "Menlo", size: 23))
        view.title.font = font
        view.traitOverrides.legibilityWeight = view.traitCollection.legibilityWeight == .bold ? .regular : .bold
        view.updateTraitsIfNeeded()
        XCTAssertEqual(view.title.font, font)
    }
}
#endif
