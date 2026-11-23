#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// A route whose first child is a `header` holding exactly one heading
/// (LLP 1035.001 D9's header-shaped route). The bar shows the heading as
/// its title — large for a level-1 heading, inline for any other — and the
/// header's buttons as its items, those before the heading leading and
/// those after it trailing, each pressing its authored button. The stack's
/// Back control is UIKit's back button, never an item. A header with no
/// heading that holds only what the bar places is a bar with no title,
/// its items trailing (`exact_kernel::bars`). iOS does not paint the
/// header; the web and the agent do (LLP 1021 D4's one presentation).
struct HeaderShape: Equatable {
    struct Item: Equatable {
        let id: UInt32, title: String, symbol: String?, label: String?, disabled: Bool, accessibilityDisabled: Bool
        /// The button's `testId`: the item's `accessibilityIdentifier`, so
        /// XCUITest and Maestro address it as any other node (LLP 1116 D2).
        let testId: String?
        let toolbarGroup: UInt32?
        /// A face drawn from the button's one filled box (an avatar or a
        /// badge: a shape holding a text or a symbol), when the button has
        /// one: the item's image, in the box's own colours and corners.
        let badge: BadgeFace?
        #if os(tvOS)
        let dot: BarDotFace?
        #endif
        let hasPaintedFace: Bool
        /// Whether the button opens a menu-shaped popover or a confirmation
        /// (`MenuHost.pullsDown`): the item's menu, its rows read when it
        /// opens, as LLP 1021 D3's pull-down. Any other popover opens as
        /// itself, from the item (`MenuHost.openFromBar`).
        let menu: Bool
        /// The popover's `aria-label`: the menu's title, as an in-page
        /// button's is (MenusIOS: "Open location in", "Sort by").
        let menuTitle: String
        /// The menu button's `menuOrder` (`MenuHost.order`).
        let menuOrder: String
        /// A native button's prominent style: a prominent bar item (iOS 26),
        /// as LLP 1069.011.000 D2 maps `NSToolbarItem`.
        let prominent: Bool
        /// A native button's `glass` or `clear-glass` style: a glass capsule
        /// of its own, not the one iOS 26 shares among adjacent items
        /// (`sharesBackground`). A bar item has no clear variant.
        let ownGlass: Bool
        /// The colour the author gave the item's face, light and dark: a
        /// symbol's tint, else a `color` written on its title's text, else
        /// one written on the button itself. Nil
        /// (UIKit's tint, the platform default) for a colour the button
        /// only inherits or the compiler implied.
        let tint: [[Double]]?
        /// Whether that colour is the tint itself: the item's tint then, its node's `ownAccent` or the bar's
        let followsTint: Bool
        /// A native button's title font where its face's rows give a size or
        /// a weight (LLP 1069.011.001 D4), as its in-page control draws it:
        /// the item's title in it, so its glass fits those words. Nil is
        /// UIKit's bar title font.
        let font: UIFont?
        let active: NativeButtonState
        let activeTitleColor: BatchValue?
        var supportsActiveColor: Bool { symbol == nil && !hasPaintedFace && !hosted }
        /// A readout (LLP 1141.004 D1): an item nothing presses, whose
        /// custom view holds the element's own views (BarHostedIOS.swift).
        let hosted: Bool
        /// The font UIKit draws a plain bar item's title in, which it does
        /// not publish: the system font at 17 pt, medium in the 26 design and
        /// bold under Bold Text (both measured on iOS 27,
        /// `testTheUnsaidTitleFontIsUIKits`); regular before it, semibold
        /// under Bold Text. A row its author left unsaid keeps this one's
        /// size or weight; the kernel has stepped the weights it sends.
        static func titleFont(bold: Bool) -> UIFont {
            if #available(iOS 26.0, *), LinkedDesign.liquidGlass { return .systemFont(ofSize: 17, weight: bold ? .bold : .medium) }
            return .systemFont(ofSize: 17, weight: bold ? .semibold : .regular)
        }
        init(_ button: NodeView, hosted: Bool = false) {
            var symbol: String?, text = "", ink: (NodeView, String)?
            func walk(_ node: NodeView) {
                for child in Self.shown(node) {
                    if child.kind == "image", !child.cssVisibilityHidden, let name = child.faceSymbol {
                        if symbol == nil { symbol = name; ink = (child, "text_color") }
                    } else if child.isParagraph, child.accessibilityExposed, text.isEmpty {
                        text = child.accessibleText; ink = ink ?? (child, "text_color")
                    }
                    else { walk(child) }
                }
            }
            // A native button's children are its face, not views (LLP 1069.011.000 D1).
            let face = button.isNativeButton ? button.face : nil
            if let face {
                symbol = face.symbol
                text = face.title ?? ""
            } else {
                walk(button)
            }
            // Up the authored tree: a hosted element the bar lifted keeps its group.
            var grouped: UInt32?
            if let presenter = button.presenter {
                for case let node as NodeView in presenter.logicalAncestry(of: button).dropFirst() {
                    if node.props["semanticTag"] == "header" { break }
                    if node.props["accessibilityRole"] == "group" { grouped = node.id; break }
                }
            }
            toolbarGroup = grouped
            id = button.id
            testId = button.props["testId"]
            self.hosted = hosted
            badge = button.isNativeButton || hosted ? nil : BadgeFace(button)
            #if os(tvOS)
            dot = badge == nil && symbol == nil && text.isEmpty ? BarDotFace(button) : nil
            hasPaintedFace = badge != nil || dot != nil
            #else
            hasPaintedFace = badge != nil
            #endif
            let invocation = button.presenter?.invocation(of: button)
            menu = invocation.map { $0.opensPopover && button.presenter?.menus.pullsDown($0.target) == true } == true
            menuTitle = menu ? invocation?.target.props["accessibilityLabel"] ?? "" : ""
            menuOrder = menu ? button.props["menuOrder"] ?? "" : ""
            self.symbol = symbol
            title = text
            label = hosted ? nil : button.props["accessibilityLabel"] ?? face?.label
            // An inert button is no enabled item, as in the page.
            disabled = button.disabled || button.inert
            accessibilityDisabled = button.ariaUnavailable
            prominent = ["filled", "bordered-prominent", "prominent-glass", "prominent-clear-glass"].contains(face?.style ?? "")
            // A badge (an avatar) on a button that asks for glass sits in its own capsule, as a glass button does.
            ownGlass = ["glass", "clear-glass"].contains(face?.style ?? "")
                || (badge != nil && Materials.glass(button.props["backgroundMaterial"]))
            #if os(tvOS)
            // tvOS's bar title font is not measured: its own stays.
            font = nil
            #else
            let rows = face?.rows.title ?? [:]
            font = rows["font_size"] != nil || rows["font_weight"] != nil
                ? ButtonConfigurationIOS.font(Self.titleFont(bold: button.traitCollection.legibilityWeight == .bold), rows: rows) : nil
            #endif
            // The platform's tint unless the author said otherwise (LLP
            // 1115.000 D2): a `color` written on the symbol, else on the title's
            // text, else on the button, a platform colour as much as a literal.
            // One the button only inherits, or a sheet wrote, is unsaid.
            // Read from the node it is written on, the tint in it as a value (`carriedTint`)
            active = NativeButtonState(button)
            let ownership = face.map { NativeButtonState.colorOwnership($0.rows) }
            let ownsTitle = ownership?.title ?? (ink?.0.wrote("text_color") == true)
            activeTitleColor = symbol == nil && !hasPaintedFace && !hosted && !ownsTitle
                && active.style.keys.allSatisfy { $0 == "press_color" } ? active.style["press_color"] : nil
            let source = ink.flatMap { $0.0.ownColor("text_color") != nil ? $0.0 : nil } ?? (button.ownColor("text_color") != nil ? button : nil)
            let faceInk: BatchValue? = face.flatMap { face in
                if face.symbol != nil, ownership?.image == true,
                   let ink = face.rows.symbol["tint_color"] ?? face.rows.symbol["text_color"] { return ink }
                return ownsTitle ? face.rows.title["text_color"] : nil
            }
            followsTint = (faceInk ?? source?.style["text_color"])?.tintAlpha == 1
            if followsTint {
                tint = nil
            } else if let faceInk {
                let pair = [false, true].compactMap { dark in
                    faceInk.channels(dark: dark, contrast: button.drawsHighContrast, elevated: button.drawsElevated, tint: faceInk.namesTint ? button.carriedTint : nil)
                }
                tint = pair.count == 2 ? pair : nil
            } else if let source, let light = source.carriedChannels("text_color", dark: false), let dark = source.carriedChannels("text_color", dark: true) {
                tint = [light, dark]
            } else {
                tint = nil
            }
        }
        /// `node`'s children that show (none `display: none`).
        static func shown(_ node: NodeView) -> [NodeView] {
            NodeView.committed(node.container.subviews).filter { $0.style["display"]?.string != "none" }
        }
        /// Everything a bar item is made from. A hosted element is not: its
        /// views follow it in place (`BarHosted.show`).
        var source: String {
            if hosted { return "hosted:\(toolbarGroup.map(String.init) ?? ""):\(id):\(testId ?? "")" }
            return "\(face):\(disabled):\(accessibilityDisabled)"
        }
        /// `source` but for whether the item is enabled.
        var face: String {
            if hosted { return source }
            #if os(tvOS)
            let paintedFace = badge?.source ?? dot?.source ?? ""
            #else
            let paintedFace = badge?.source ?? ""
            #endif
            return "\(toolbarGroup.map(String.init) ?? ""):\(id):\(title):\(symbol ?? ""):\(label ?? ""):\(prominent):\(ownGlass):\(paintedFace):\(menu):\(menuTitle):\(menuOrder):\(tint ?? []):\(followsTint):\(font.map { "\($0.fontName) \($0.pointSize) \(TextEngine.controlWeight($0))" } ?? ""):\(testId ?? ""):\(activeSignature)"
        }
        func hasSameImage(as other: Item) -> Bool {
            #if os(tvOS)
            guard dot == other.dot else { return false }
            #endif
            return symbol == other.symbol && badge == other.badge
        }
        func image(dimmed: Bool) -> UIImage? {
            #if os(tvOS)
            if let dot { return dot.image(dimmed: dimmed) }
            #endif
            return badge?.image(dimmed: dimmed) ?? symbol.flatMap { FaceSymbol.image($0, points: FaceSymbol.barItemPoints) }
        }
        private var activeSignature: String {
            let values = active.style.mapValues(\.any)
            guard !values.isEmpty, let data = try? JSONSerialization.data(withJSONObject: values, options: [.sortedKeys]) else { return "" }
            return String(decoding: data, as: UTF8.self) + String(activeTitleColor != nil)
        }
    }
    let header: NodeView
    let heading: NodeView?
    package let title: String
    let level: Int
    let leading: [Item], trailing: [Item]
    /// The header's `input type="search"`, if it has one: the item's
    /// search controller (LLP 1075.003 §9.6).
    let search: NodeView?
    /// The header's tablist of text tabs, if it has one: the item's title
    /// view, a segmented control (LLP 1075.003 §9.8).
    let segments: NodeView?
    /// What the heading's group holds besides it (an avatar, a subtitle) and
    /// whether pressing it does something: the item's title, richer (§9.10).
    let group: HeaderTitle?
    /// The header's nodes the bar places (`presents`): the heading, every
    /// item's button (the Back control's too), the search field, the
    /// tablist when it has tabs for the segmented title, and what the title
    /// shows of the heading's group.
    let placed: [NodeView]
    /// The readouts the bar shows in views of their own (LLP 1141.004 D1),
    /// lifted out of the header rather than hidden with it.
    let hosted: [NodeView]
    /// Explicit opt-outs stay in the authored header with their descendants.
    let authored: [NodeView]
    let authoredBack: Bool

    /// `back` names the stack's Back control, left to UIKit's back button
    /// when there is one (`backIsUIKits`): the root of a presented stack
    /// has none, so there it is an item like the others.
    init?(route: NodeView, back: String?, backIsUIKits: Bool = true, projected: Bool = false) {
        // What is header-shaped is the kernel's rule, read over this route's views (`HeaderRule`, LLP 1115.001 D11).
        guard let rule = HeaderRule(route: route, back: back, backIsPlatforms: backIsUIKits, projected: projected) else { return nil }
        // Every role the header rule assigns can remain authored. In
        // particular the title's tap target is not one of its bar items.
        let roles = rule.items + rule.hosted + [rule.heading, rule.tap, rule.search, rule.segments].compactMap { $0 }
        let keeps = { (node: NodeView) in node.wrote("appearance") && node.style["appearance"]?.string == "none" }
        // Any other element its author keeps (a text, a box) stays in the page
        // too, and is not the title's: the outermost such in the header,
        // outside the bar's own parts (a tab that keeps its look is still
        // the tablist's).
        let parts = rule.items + rule.hosted + [rule.heading, rule.search, rule.segments].compactMap { $0 }
        func kept(in node: NodeView) -> [NodeView] {
            NodeView.committed(node.container.subviews).flatMap { child -> [NodeView] in
                if parts.contains(where: { $0 === child }) { return [] }
                return keeps(child) ? [child] : kept(in: child)
            }
        }
        let authored = projected ? roles.filter(keeps) + kept(in: rule.header).filter { node in !roles.contains { $0 === node } } : []
        self.authored = authored
        // In the authored tree: an element the bar lifted is still inside what keeps it.
        let projects = { (node: NodeView) in !authored.contains { Self.inside(node, $0) } }
        authoredBack = back != nil && roles.contains { $0.props["id"] == back && !projects($0) }
        let heading = rule.heading.flatMap { projects($0) ? $0 : nil }, titleProjects = heading != nil
        self.heading = heading
        let search = rule.search.flatMap { projects($0) ? $0 : nil }
        let segments = rule.segments.flatMap { projects($0) ? $0 : nil }
        let tap = rule.tap.flatMap { projects($0) ? $0 : nil }
        header = rule.header
        // A heading of only white space names nothing (accname trims it);
        // UIKit would take it as a title and show a pair of quotes
        let text = heading?.accessibleText ?? ""
        title = text.unicodeScalars.allSatisfy(\.properties.isWhitespace) ? "" : text
        level = titleProjects ? rule.level : 0
        // The title decides first, with the buttons, readouts, search field
        // and tablist apart from it. What the author keeps in the page is not
        // the title's to read either.
        let readouts = rule.readouts.filter(projects)
        let apart = rule.items + rule.readouts + [rule.search, rule.segments].compactMap { $0 } + authored
        let group = heading.flatMap { HeaderTitle(header: rule.header, heading: $0, tap: tap, apart: apart, kept: authored) }
        #if os(tvOS)
        // tvOS's bar has no item that focus passes by undimmed: none hosted.
        let hosted: [NodeView] = []
        #else
        // The readouts are the bar's hosted items (D1); every button is
        // UIKit's own item, drawn from its symbol and words.
        let hosted = readouts
        #endif
        self.hosted = hosted
        let item = { (node: NodeView) -> Item? in
            if hosted.contains(where: { $0 === node }) { return Item(node, hosted: true) }
            return rule.hosted.contains(where: { $0 === node }) ? nil : Item(node)
        }
        leading = rule.leading.filter(projects).compactMap(item)
        trailing = rule.trailing.filter(projects).compactMap(item)
        self.search = search
        self.segments = segments
        self.group = group
        let tablist = segments.flatMap { HeaderShape.titleTabs($0).isEmpty ? nil : $0 }
        // The title's pressable wrapper goes with the title it holds, unless
        // it holds an opted-out node, which stays where it is.
        let wrapper = titleProjects ? tap.flatMap { tap in authored.contains { Self.inside($0, tap) } ? nil : tap } : nil
        placed = ([rule.heading].compactMap { $0 } + rule.items.filter { item in !hosted.contains { $0 === item } } + [search, tablist, wrapper].compactMap { $0 }
                  + (HeaderShape.presented(group, segments: segments)?.placed ?? [])).filter(projects)
    }

    /// Whether `node` is `box` or inside it in the authored tree, where an
    /// element the bar lifted is still in its parent (LLP 1141.004 D3).
    static func inside(_ node: NodeView, _ box: NodeView) -> Bool {
        node.presenter?.logicalAncestry(of: node).contains { $0 === box } ?? node.isDescendant(of: box)
    }

    /// The tabs of a header's tablist the segmented title shows (§9.8).
    static func titleTabs(_ list: NodeView) -> [NodeView] {
        list.container.subviews.compactMap { $0 as? NodeView }.filter {
            $0.isButton && $0.props["accessibilityRole"] == "tab" && $0.handlers.contains("press")
        }
    }

    static func == (a: HeaderShape, b: HeaderShape) -> Bool {
        a.header === b.header && a.authoredBack == b.authoredBack && a.title == b.title && a.level == b.level && a.leading == b.leading && a.trailing == b.trailing && a.search === b.search && a.segments === b.segments && a.group == b.group && a.authored.map(\.id) == b.authored.map(\.id) && a.hosted.map(\.id) == b.hosted.map(\.id)
    }
}

#endif
