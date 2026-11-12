#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class NativeBarDetailsIOSTests: XCTestCase {
    func testAPressableTitleFollowsItsHeadingsAuthoredFont() throws {
        let session = ExactApp.shared.makeSession(label: "title-font-\(UUID().uuidString)")
        defer { session.destroy() }
        let p = session.presenter
        for symbols in [false, true] {
            let header = NodeView(id: 1, kind: "view", presenter: p)
            let group = NodeView(id: 2, kind: "view", presenter: p)
            let heading = NodeView(id: 3, kind: "text", presenter: p)
            let seal = NodeView(id: 4, kind: "image", presenter: p)
            group.handlers = ["press"]
            group.applyStyle(["display": "flex", "flex_direction": "row"])
            heading.props = ["text": "Chat", "accessibilityRole": "heading"]
            header.addSubview(group); group.addSubview(heading)
            if symbols {
                seal.props = ["imageSource": "symbol:checkmark.seal.fill", "symbolName": "checkmark.seal.fill"]
                group.addSubview(seal)
            }
            let engine = try XCTUnwrap(heading.text)
            let view = HeaderTitleView(host: p.navigation)
            let base = UIFont.preferredFont(forTextStyle: .headline, compatibleWith: view.traitCollection)
            var previous = try XCTUnwrap(HeaderTitle(header: header, heading: heading, tap: group, apart: []))
            view.update(previous, title: "Chat")
            XCTAssertEqual(view.title.font, base)
            for (family, size, weight, italic) in [(5, 29, 800, true), (2, 29, 800, true),
                                                  (2, 23, 800, true), (2, 23, 300, true), (2, 23, 300, false)] {
                heading.applyStyle(["font_family": .number(Double(family)), "font_size": .number(Double(size)), "font_weight": .number(Double(weight)),
                    "font_style": italic ? "italic" : "normal", "written": ["font_family", "font_size", "font_weight", "font_style"]])
                let next = try XCTUnwrap(HeaderTitle(header: header, heading: heading, tap: group, apart: []))
                XCTAssertNotEqual(next, previous, "a font-only change invalidates the title")
                XCTAssertNotEqual(next.source, previous.source, "the route's projection signature includes the font")
                view.update(next, title: "Chat")
                let expected = engine.font(size: CGFloat(size), weight: weight, family: family, italic: italic)
                XCTAssertEqual(view.title.font, expected)
                if symbols {
                    let line = try XCTUnwrap(view.title.attributedText)
                    XCTAssertEqual(line.attribute(.font, at: 0, effectiveRange: nil) as? UIFont, expected)
                }
                previous = next
            }
            heading.applyStyle(["font_family": 5, "written": ["font_family"]])
            view.update(try XCTUnwrap(HeaderTitle(header: header, heading: heading, tap: group, apart: [])), title: "Chat")
            XCTAssertEqual(view.title.font, engine.font(size: base.pointSize, weight: TextEngine.cssWeight(of: base) ?? 400, family: 5, italic: false),
                           "a family alone keeps the platform's size and weight")
            heading.applyStyle(["font_family": 5, "font_size": 29, "font_weight": 800, "font_style": "italic"])
            view.update(try XCTUnwrap(HeaderTitle(header: header, heading: heading, tap: group, apart: [])), title: "Chat")
            XCTAssertEqual(view.title.font, base, "inherited typography leaves the platform's headline")
        }
    }

    func testAPressableTitlesUnspecifiedSizeFollowsDynamicType() throws {
        let session = ExactApp.shared.makeSession(label: "title-size-\(UUID().uuidString)")
        defer { session.destroy() }
        let p = session.presenter
        let header = NodeView(id: 1, kind: "view", presenter: p)
        let heading = NodeView(id: 2, kind: "text", presenter: p)
        header.addSubview(heading)
        let view = HeaderTitleView(host: p.navigation)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        defer { window.isHidden = true }
        window.traitOverrides.preferredContentSizeCategory = .large
        window.addSubview(header); window.addSubview(view)
        window.makeKeyAndVisible()
        let engine = try XCTUnwrap(heading.text)
        for authoredFamily in [false, true] {
            heading.applyStyle(authoredFamily ? ["font_family": 5, "written": ["font_family"]] : [:])
            window.traitOverrides.preferredContentSizeCategory = .large
            window.updateTraitsIfNeeded(); view.updateTraitsIfNeeded()
            view.update(try XCTUnwrap(HeaderTitle(header: header, heading: heading, tap: header, apart: [])), title: "Chat")
            let before = view.title.font.pointSize
            window.traitOverrides.preferredContentSizeCategory = .accessibilityExtraLarge
            window.updateTraitsIfNeeded(); view.updateTraitsIfNeeded()
            let base = UIFont.preferredFont(forTextStyle: .headline, compatibleWith: view.traitCollection)
            let expected = authoredFamily ? engine.font(size: base.pointSize, weight: TextEngine.cssWeight(of: base) ?? 400, family: 5, italic: false) : base
            XCTAssertEqual(view.title.font, expected)
            XCTAssertGreaterThan(view.title.font.pointSize, before)
        }
        heading.applyStyle(["font_size": 29, "written": ["font_size"]])
        view.update(try XCTUnwrap(HeaderTitle(header: header, heading: heading, tap: header, apart: [])), title: "Chat")
        window.traitOverrides.preferredContentSizeCategory = .large
        window.updateTraitsIfNeeded(); view.updateTraitsIfNeeded()
        XCTAssertEqual(view.title.font.pointSize, 29, "an authored absolute size is already resolved by the kernel")
    }

    func testTheSelectedTabsOwnTintOverridesTheTablistAccent() throws {
        let p = Presenter(), list = NodeView(id: 1, kind: "view", presenter: p)
        let first = NodeView(id: 2, kind: "button", presenter: p)
        let second = NodeView(id: 3, kind: "button", presenter: p)
        list.applyStyle(["accent_color": [0, 0, 255, 255], "written": ["accent_color"]])
        first.applyStyle(["tint_color": [[255, 0, 0, 255], [0, 255, 0, 255]], "written": ["tint_color"]])
        list.addSubview(first); list.addSubview(second)
        let nav = p.navigation, controller = UITabBarController()
        nav.tabController = controller
        nav.tabItemsSource = NavigationTabs(tablist: list, tabs: [first, second], panels: [])
        nav.routerTab = 0
        nav.tint(list)
        XCTAssertEqual(controller.tabBar.tintColor.resolvedColor(with: UITraitCollection(userInterfaceStyle: .light)), .red)
        XCTAssertEqual(controller.tabBar.tintColor.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark)), .green)
        nav.routerTab = 1
        nav.tint(list)
        XCTAssertEqual(controller.tabBar.tintColor.resolvedColor(with: UITraitCollection(userInterfaceStyle: .light)), .blue)
        nav.routerTab = 0
        first.applyStyle(["tint_color": [255, 0, 0, 255]])
        nav.tint(list)
        XCTAssertEqual(controller.tabBar.tintColor.resolvedColor(with: UITraitCollection(userInterfaceStyle: .light)), .blue,
                       "an inherited tint leaves the tablist's accent")
    }

    /// The selected tab's own tint reaches the tab bar as written while UIKit dims tints: UIKit dims
    /// the bar itself, and keeps the tint once the dimming ends
    func testTheSelectedTabsOwnTintIsAsWrittenWhileDimmed() throws {
        let p = Presenter(), list = NodeView(id: 1, kind: "view", presenter: p)
        let tab = NodeView(id: 2, kind: "button", presenter: p)
        tab.applyStyle(["tint_color": [[255, 0, 0, 255], [0, 255, 0, 255]], "written": ["tint_color"]])
        list.addSubview(tab)
        let nav = p.navigation, controller = UITabBarController()
        nav.tabController = controller
        nav.tabItemsSource = NavigationTabs(tablist: list, tabs: [tab], panels: [])
        nav.routerTab = 0
        list.tintAdjustmentMode = .dimmed
        nav.tint(list)
        XCTAssertEqual(controller.tabBar.tintColor.resolvedColor(with: UITraitCollection(userInterfaceStyle: .light)), .red)
        XCTAssertEqual(controller.tabBar.tintColor.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark)), .green)
        // A tint of its own that names the tint takes the tablist's accent at rest, each appearance its side
        list.applyStyle(["accent_color": [[0, 0, 255, 255], [255, 255, 0, 255]], "written": ["accent_color"]])
        tab.applyStyle(["tint_color": ["sys": "@tint", "c": [0, 0, 255, 255]], "written": ["tint_color"]])
        nav.tint(list)
        XCTAssertEqual(controller.tabBar.tintColor.resolvedColor(with: UITraitCollection(userInterfaceStyle: .light)), .blue, "the accent's light side")
        XCTAssertEqual(controller.tabBar.tintColor.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark)), .yellow, "the accent's dark side")
    }

    func testAnOwnTintEqualToTheAccentReplacesItsLiveResolver() throws {
        let p = Presenter(), list = NodeView(id: 1, kind: "view", presenter: p)
        let tab = NodeView(id: 2, kind: "button", presenter: p)
        list.applyStyle(["accent_color": [0, 0, 255, 255], "written": ["accent_color"]])
        list.addSubview(tab)
        let nav = p.navigation, controller = UITabBarController()
        nav.tabController = controller
        nav.tabItemsSource = NavigationTabs(tablist: list, tabs: [tab], panels: [])
        nav.routerTab = 0
        nav.tint(list)
        tab.applyStyle(["tint_color": [0, 0, 255, 255], "written": ["tint_color"]])
        nav.tint(list)
        list.applyStyle(["accent_color": [255, 0, 0, 255], "written": ["accent_color"]])
        XCTAssertEqual(controller.tabBar.tintColor.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark)), .blue,
                       "the selected tab owns the colour even when its channels matched the old accent")
        list.applyStyle(["accent_color": [0, 0, 255, 255], "written": ["accent_color"]])
        tab.applyStyle([:])
        nav.tint(list)
        list.applyStyle(["accent_color": [255, 0, 0, 255], "written": ["accent_color"]])
        XCTAssertEqual(controller.tabBar.tintColor.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark)), .red,
                       "clearing an own tint reinstalls the live accent resolver even at equal channels")
    }

    /// A seal beside a pressable title is drawn in its colour as written while UIKit dims tints: the
    /// title's text keeps it once the dimming ends
    func testAHeadingRowsSealBesideAPressableTitleIsAsWrittenWhileDimmed() throws {
        let p = Presenter()
        let header = NodeView(id: 1, kind: "view", presenter: p)
        let row = NodeView(id: 2, kind: "view", presenter: p)
        let heading = NodeView(id: 3, kind: "text", presenter: p)
        let seal = NodeView(id: 4, kind: "image", presenter: p)
        header.handlers = ["press"]
        row.applyStyle(["display": "flex", "flex_direction": "row"])
        heading.props = ["text": "Verified"]
        seal.props = ["imageSource": "symbol:checkmark.seal.fill", "symbolName": "checkmark.seal.fill"]
        seal.applyStyle(["tint_color": [[255, 0, 0, 255], [0, 255, 0, 255]], "written": ["tint_color"]])
        header.addSubview(row); row.addSubview(heading); row.addSubview(seal)
        header.tintAdjustmentMode = .dimmed
        let group = try XCTUnwrap(HeaderTitle(header: header, heading: heading, tap: nil, apart: []))
        let view = HeaderTitleView(host: p.navigation)
        view.title.textColor = .label
        view.update(group, title: "Verified")
        let title = try XCTUnwrap(view.title.attributedText)
        var count = 0
        title.enumerateAttribute(.attachment, in: NSRange(location: 0, length: title.length)) { value, range, _ in
            guard value is NSTextAttachment else { return }
            count += 1
            let ink = title.attribute(.foregroundColor, at: range.location, effectiveRange: nil) as? UIColor
            XCTAssertEqual(ink?.resolvedColor(with: UITraitCollection(userInterfaceStyle: .light)), .red)
            XCTAssertEqual(ink?.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark)), .green)
        }
        XCTAssertEqual(count, 1)
    }

    func testAHeadingRowsSealIsDrawnBesideTheTitle() throws {
        let p = Presenter()
        let header = NodeView(id: 1, kind: "view", presenter: p)
        let row = NodeView(id: 2, kind: "view", presenter: p)
        let heading = NodeView(id: 3, kind: "text", presenter: p)
        let seal = NodeView(id: 4, kind: "image", presenter: p)
        row.applyStyle(["display": "flex", "flex_direction": "row"])
        heading.props = ["text": "Verified"]
        seal.props = ["imageSource": "symbol:checkmark.seal.fill", "symbolName": "checkmark.seal.fill"]
        seal.applyStyle(["tint_color": [[255, 0, 0, 255], [0, 255, 0, 255]], "written": ["tint_color"]])
        header.addSubview(row); row.addSubview(heading); row.addSubview(seal)
        let group = try XCTUnwrap(HeaderTitle(header: header, heading: heading, tap: nil, apart: []))
        XCTAssertTrue(group.drawn)
        XCTAssertTrue(group.placed.contains { $0 === seal })
        let view = HeaderTitleView(host: p.navigation)
        view.title.textColor = .label
        view.update(group, title: "Verified")
        let title = try XCTUnwrap(view.title.attributedText)
        var count = 0
        title.enumerateAttribute(.attachment, in: NSRange(location: 0, length: title.length)) { value, range, _ in
            if let attachment = value as? NSTextAttachment {
                count += 1
                XCTAssertEqual(attachment.image?.renderingMode, .alwaysTemplate)
                let ink = title.attribute(.foregroundColor, at: range.location, effectiveRange: nil) as? UIColor
                XCTAssertEqual(ink?.resolvedColor(with: UITraitCollection(userInterfaceStyle: .light)), .red)
                XCTAssertEqual(ink?.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark)), .green)
            }
        }
        XCTAssertEqual(count, 1)
        let headingInk = title.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? UIColor
        XCTAssertEqual(headingInk?.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark)),
                       UIColor.label.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark)))
        seal.applyStyle([:])
        let untinted = try XCTUnwrap(HeaderTitle(header: header, heading: heading, tap: nil, apart: []))
        view.update(untinted, title: "Verified")
        let inherited = try XCTUnwrap(view.title.attributedText)
        inherited.enumerateAttribute(.attachment, in: NSRange(location: 0, length: inherited.length)) { value, range, _ in
            guard value is NSTextAttachment else { return }
            let ink = inherited.attribute(.foregroundColor, at: range.location, effectiveRange: nil) as? UIColor
            XCTAssertEqual(ink?.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark)),
                           UIColor.label.resolvedColor(with: UITraitCollection(userInterfaceStyle: .dark)))
        }
        view.title.textColor = .red
        view.update(untinted, title: "Verified")
        let changed = try XCTUnwrap(view.title.attributedText)
        XCTAssertEqual(changed.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? UIColor, .red,
                       "an unchanged title still follows a changed heading colour")
        XCTAssertTrue(title.string.hasPrefix("Verified"))
        XCTAssertEqual(view.accessibilityLabel, "Verified")
        XCTAssertTrue(view.subtitle.isHidden)
    }
}
#endif
