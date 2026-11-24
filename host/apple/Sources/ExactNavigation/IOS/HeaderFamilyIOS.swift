#if os(iOS)
import UIKit
import ExactKit

/// A route header substitutes its family after the route hatch runs (LLP 1141.004 D1).
struct HeaderFamily {
    let id: Int
    let engine: TextEngine
    typealias Appearance = UINavigationBarAppearance
    private typealias Attributes = ReferenceWritableKeyPath<Appearance, [NSAttributedString.Key: Any]>
    private static let itemAttributes: [Attributes] = [
        \.buttonAppearance.normal.titleTextAttributes, \.buttonAppearance.highlighted.titleTextAttributes,
        \.buttonAppearance.disabled.titleTextAttributes, \.buttonAppearance.focused.titleTextAttributes,
        \.doneButtonAppearance.normal.titleTextAttributes, \.doneButtonAppearance.highlighted.titleTextAttributes,
        \.doneButtonAppearance.disabled.titleTextAttributes, \.doneButtonAppearance.focused.titleTextAttributes,
        \.backButtonAppearance.normal.titleTextAttributes, \.backButtonAppearance.highlighted.titleTextAttributes,
        \.backButtonAppearance.disabled.titleTextAttributes, \.backButtonAppearance.focused.titleTextAttributes
    ]
    private static let titleAttributes: [Attributes] = [\.titleTextAttributes, \.largeTitleTextAttributes]
    private static let stock = Appearance()

    func appearance(over base: Appearance, heading: NavigationTitleInk.Fonts?, ownTitle: Bool, traits: UITraitCollection) -> Appearance {
        let copy = base.copy() as! Appearance
        let inlineSize = TextEngine.inlineBarPointSize(traits)
        let largeSize = UIFont.preferredFont(forTextStyle: .largeTitle, compatibleWith: traits).pointSize
        let available = id < 8 || engine.catalog[id]?.isEmpty == false
        for path in Self.titleAttributes + Self.itemAttributes {
            let large = path == \Appearance.largeTitleTextAttributes
            let title = large || path == \Appearance.titleTextAttributes
            if title, let authored = large ? heading?.large : heading?.title {
                copy[keyPath: path][.font] = ownTitle ? authored : font(authored)
                continue
            }
            let stock = Self.stock[keyPath: path][.font] as? UIFont
            guard available else {
                copy[keyPath: path][.font] = stock
                continue
            }
            let size = large ? largeSize : inlineSize
            let normal = stock ?? Self.stock.buttonAppearance.normal.titleTextAttributes[.font] as? UIFont ?? HeaderShape.Item.titleFont(bold: false)
            var source = normal.withSize(size)
            if traits.legibilityWeight == .bold {
                let weight = TextEngine.controlWeight(normal)
                let heavier: UIFont.Weight = weight >= 700 ? .black : weight >= 600 ? .heavy : weight >= 500 ? .bold : .semibold
                source = .systemFont(ofSize: size, weight: heavier)
            }
            copy[keyPath: path][.font] = font(source)
        }
        return copy
    }

    /// Items without a written size or weight inherit their style's state appearances.
    /// Clearing just the font keeps colours and any other per-state attributes.
    static func applyItems(in route: RouteController, shape: HeaderShape?, family: HeaderFamily?) {
        let shapes = (shape?.leading ?? []) + (shape?.trailing ?? [])
        var authored: [ObjectIdentifier: UIFont] = [:]
        for entry in shapes {
            if let font = entry.font, let item = route.barItemsMade[entry.id]?.bar {
                authored[ObjectIdentifier(item)] = family?.font(font) ?? font
            }
        }
        let navigationItem = route.navigationItem
        for item in (navigationItem.leftBarButtonItems ?? []) + (navigationItem.rightBarButtonItems ?? [])
            where item.customView == nil && item.image == nil && item.title?.isEmpty == false {
            let font = authored[ObjectIdentifier(item)]
            for state: UIControl.State in [.normal, .highlighted, .disabled, .focused] {
                var attributes = item.titleTextAttributes(for: state) ?? [:]
                guard attributes[.font] as? UIFont != font else { continue }
                attributes[.font] = font
                item.setTitleTextAttributes(attributes.isEmpty ? nil : attributes, for: state)
            }
        }
    }

    /// Liquid Glass retains the old font on an attached item even after its
    /// navigation item's appearance changes. Reattach the same items to make
    /// UIKit resolve their inherited titles without writing a font on them.
    static func refreshItems(in item: UINavigationItem) {
        for path: ReferenceWritableKeyPath<UINavigationItem, [UIBarButtonItem]?> in [\.leftBarButtonItems, \.rightBarButtonItems] {
            guard let items = item[keyPath: path], items.contains(where: {
                $0.customView == nil && $0.image == nil && $0.title?.isEmpty == false && $0.titleTextAttributes(for: .normal)?[.font] == nil
            }) else { continue }
            item[keyPath: path] = nil
            item[keyPath: path] = items
        }
    }

    /// An app can change one field on a copy of the item appearance. Import just
    /// its edits into the native slot, including removal of an attribute.
    static func nativeEdit(_ edit: Appearance?, after last: Appearance?, base: Appearance) -> Appearance? {
        guard let edit, let last else { return edit }
        let result = base.copy() as! Appearance
        func field<T: Equatable>(_ path: ReferenceWritableKeyPath<Appearance, T>) {
            if edit[keyPath: path] != last[keyPath: path] { result[keyPath: path] = edit[keyPath: path] }
        }
        func attributes(_ path: Attributes) {
            let before = last[keyPath: path] as NSDictionary, after = edit[keyPath: path] as NSDictionary
            guard !before.isEqual(after) else { return }
            var values = result[keyPath: path]
            for key in Set(last[keyPath: path].keys).union(edit[keyPath: path].keys) {
                if key == .font, titleAttributes.contains(path) || itemAttributes.contains(path) { continue }
                if let old = before[key] as? NSObject, old.isEqual(after[key]) { continue }
                values[key] = after[key]
            }
            result[keyPath: path] = values
        }
        field(\.backgroundEffect)
        field(\.backgroundColor)
        field(\.backgroundImage)
        field(\.backgroundImageContentMode)
        field(\.shadowImage)
        field(\.shadowColor)
        field(\.titlePositionAdjustment)
        if #available(iOS 27, *) { field(\.overrideUserInterfaceStyle) }
        if edit.backIndicatorImage != last.backIndicatorImage || edit.backIndicatorTransitionMaskImage != last.backIndicatorTransitionMaskImage {
            result.setBackIndicatorImage(edit.backIndicatorImage, transitionMaskImage: edit.backIndicatorTransitionMaskImage)
        }
        for path in titleAttributes { attributes(path) }
        if #available(iOS 26, *) { attributes(\.subtitleTextAttributes); attributes(\.largeSubtitleTextAttributes) }
        let buttons: [ReferenceWritableKeyPath<Appearance, UIBarButtonItemAppearance>] = [\.buttonAppearance, \.doneButtonAppearance, \.backButtonAppearance]
        let states: [KeyPath<UIBarButtonItemAppearance, UIBarButtonItemStateAppearance>] = [\.normal, \.highlighted, \.disabled, \.focused]
        for button in buttons {
            for state in states {
                let path = button.appending(path: state)
                attributes(path.appending(path: \.titleTextAttributes))
                field(path.appending(path: \.titlePositionAdjustment))
                field(path.appending(path: \.backgroundImage))
                field(path.appending(path: \.backgroundImagePositionAdjustment))
            }
        }
        return result
    }
}

extension NavigationHost {
    func observeHeaderTypography(in nav: UINavigationController) {
        guard let stack = stacks[ObjectIdentifier(nav)], !stack.observesHeaderFonts else { return }
        stack.observesHeaderFonts = true
        let refresh = { [weak self, weak nav, weak stack] in
            guard let self, let nav, let stack, stacks[ObjectIdentifier(nav)] === stack else { return }
            for case let route as RouteController in nav.stackRoutes where route.titleInk.hasHeaderFamily && controllers[route.node.id] === route {
                guard let shape = HeaderShape.bar(of: route.node), let id = shape.family, let engine = shape.header.text else { continue }
                let fonts = NavigationTitleInk.fonts(of: shape.heading, level: shape.level, traits: route.traitCollection)
                route.titleInk.updateFamily(HeaderFamily(id: id, engine: engine), shape: shape, route: route, bar: nav.navigationBar,
                                            color: shape.heading?.ownUIColor("text_color"), fonts: fonts, transparent: RouteGeometry.transparentBar(route.node))
            }
        }
        (nav.navigationBar as? RouteNavigationBar)?.appearanceChanged = refresh
        nav.navigationBar.registerForTraitChanges([UITraitPreferredContentSizeCategory.self, UITraitLegibilityWeight.self]) {
            (_: UINavigationBar, _: UITraitCollection) in refresh()
        }
    }
}

extension HeaderFamily {
    /// UIKit resolves a text-style descriptor back to its system face even after withFamily.
    /// Carry the resolved weight, width and slant into the requested family instead.
    func font(_ base: UIFont) -> UIFont {
        let family = id
        let generic = family < 8 ? family : engine.catalog[family]?.first?.generic
        guard generic != nil || engine.catalog[family]?.isEmpty == false else { return base }
        let selected = engine.font(size: base.pointSize, weight: TextEngine.controlWeight(base), family: family,
                            italic: base.fontDescriptor.symbolicTraits.contains(.traitItalic))
        var attributes = base.fontDescriptor.fontAttributes
        attributes.removeValue(forKey: .textStyle)
        attributes.removeValue(forKey: .name)
        attributes.removeValue(forKey: .family)
        attributes.removeValue(forKey: .cascadeList)
        var traits = (base.fontDescriptor.object(forKey: .traits) as? [UIFontDescriptor.TraitKey: Any] ?? [:])
            .filter { [.weight, .width, .slant].contains($0.key) }
        traits[.symbolic] = base.fontDescriptor.symbolicTraits.intersection([.traitBold, .traitItalic, .traitCondensed, .traitExpanded, .traitVertical]).rawValue
        attributes[.traits] = traits
        let descriptor = generic == nil ? UIFontDescriptor(fontAttributes: attributes).withFamily(selected.familyName)
            : selected.fontDescriptor.addingAttributes(attributes)
        let cascade = selected.fontDescriptor.fontAttributes[.cascadeList]
        let result = cascade.map { descriptor.addingAttributes([.cascadeList: $0]) } ?? descriptor
        return UIFont(descriptor: result, size: base.pointSize)
    }
}
#endif
