#if os(iOS) || os(tvOS)
import UIKit

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
        /// A face that says more than a title or symbol (a dot beside its
        /// label, a font of the author's), drawn as painted (`ChipFace`).
        let chip: ChipFace?
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
        /// symbol's tint, else a `color` written on the button itself. Nil
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
        var supportsActiveColor: Bool { symbol == nil && badge == nil && chip == nil && readout == nil }
        /// A readout's face (`role="status"`, LLP 1075.003 §9.15): an item
        /// no one presses, whose custom view shows the face as written and
        /// follows it in place (BarReadoutIOS.swift).
        let readout: ChipFace?
        /// The padding around a readout's face inside the glass: each side
        /// as its author wrote it, else UIKit's (`titleInset` beside words).
        let readoutPadding: UIEdgeInsets
        /// How far a `UIBarButtonItem(title:)`'s label sits inside its item
        /// on each side, which UIKit does not publish: 12 pt on iOS 26 and 27
        /// (measured), so its glass is 16 pt wider than the words each side.
        static let titleInset: CGFloat = 12
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
        init(_ button: NodeView, readout isReadout: Bool = false) {
            var symbol: String?, text = "", ink: (NodeView, String)?
            func walk(_ node: NodeView) {
                for case let child as NodeView in node.container.subviews {
                    if child.kind == "image", let name = child.faceSymbol {
                        if symbol == nil { symbol = name; ink = (child, "text_color") }
                    } else if child.isParagraph, text.isEmpty { text = child.accessibleText; ink = ink ?? (child, "text_color") }
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
            var parent = button.superview
            var grouped: UInt32?
            while let view = parent {
                if let node = view as? NodeView {
                    if node.props["semanticTag"] == "header" { break }
                    if node.props["accessibilityRole"] == "group" { grouped = node.id; break }
                }
                parent = view.superview
            }
            toolbarGroup = grouped
            id = button.id
            testId = button.props["testId"]
            readout = isReadout ? ChipFace(button, readout: true) : nil
            // A side left unsaid is UIKit's. A bare custom view hugs its
            // capsule, whose round ends then cut into its words, so a face
            // with words takes the inset a title item's label has.
            let words = readout?.parts.contains { if case .text = $0 { true } else { false } } == true
            let pad = { (side: String, platform: CGFloat) in button.wrote("padding_\(side)") ? button.number("padding_\(side)") : platform }
            let beside: CGFloat = words ? Self.titleInset : 0
            readoutPadding = isReadout ? UIEdgeInsets(top: pad("top", 0), left: pad("left", beside), bottom: pad("bottom", 0), right: pad("right", beside)) : .zero
            badge = button.isNativeButton || isReadout ? nil : BadgeFace(button)
            chip = badge == nil && !isReadout ? ChipFace(button) : nil
            let invocation = button.presenter?.invocation(of: button)
            menu = invocation.map { $0.opensPopover && button.presenter?.menus.pullsDown($0.target) == true } == true
            menuTitle = menu ? invocation?.target.props["accessibilityLabel"] ?? "" : ""
            menuOrder = menu ? button.props["menuOrder"] ?? "" : ""
            self.symbol = symbol
            title = text
            label = button.props["accessibilityLabel"] ?? face?.label ?? (isReadout ? Self.shownWords(button) : nil)
            disabled = button.disabled
            accessibilityDisabled = button.ariaUnavailable
            prominent = ["filled", "bordered-prominent", "prominent-glass", "prominent-clear-glass"].contains(face?.style ?? "")
            ownGlass = ["glass", "clear-glass"].contains(face?.style ?? "")
            #if os(tvOS)
            // tvOS's bar title font is not measured: its own stays.
            font = nil
            #else
            let rows = face?.rows.title ?? [:]
            font = rows["font_size"] != nil || rows["font_weight"] != nil
                ? ButtonConfigurationIOS.font(Self.titleFont(bold: button.traitCollection.legibilityWeight == .bold), rows: rows) : nil
            #endif
            // The platform's tint unless the author said otherwise (LLP
            // 1115.000 D2): a `color` written on the symbol, else a `color`
            // written on the button, a platform colour as much as a literal.
            // One the button only inherits, or a sheet wrote, is unsaid.
            // Read from the node it is written on, the tint in it as a value (`carriedTint`)
            active = NativeButtonState(button)
            let ownsTitle = face.map { NativeButtonState.colorOwnership($0.rows).title } ?? (ink?.0.wrote("text_color") == true)
            activeTitleColor = symbol == nil && badge == nil && chip == nil && !isReadout && !ownsTitle
                && active.style.keys.allSatisfy { $0 == "press_color" } ? active.style["press_color"] : nil
            let source = ink.flatMap { $0.0.kind == "image" && $0.0.ownColor("text_color") != nil ? $0.0 : nil } ?? (button.ownColor("text_color") != nil ? button : nil)
            followsTint = source?.style["text_color"]?.tintAlpha == 1
            if !followsTint, let source, let light = source.carriedChannels("text_color", dark: false), let dark = source.carriedChannels("text_color", dark: true) {
                tint = [light, dark]
            } else {
                tint = nil
            }
        }
        /// What a readout says with no `aria-label`: its words that show,
        /// none `display: none` or `aria-hidden` (the bar's own hiding of the
        /// header is `isHidden`, not this).
        static func shownWords(_ node: NodeView) -> String {
            guard node.style["display"]?.string != "none", node.props["accessibilityElementsHidden"] != "true" else { return "" }
            if node.isParagraph { return node.accessibleText }
            return node.container.subviews.compactMap { ($0 as? NodeView).map(shownWords) }.filter { !$0.isEmpty }.joined(separator: " ")
        }
        /// Everything a bar item is made from. A readout's face is not: its
        /// view follows it in place (`showReadouts`).
        var source: String {
            if readout != nil { return "readout:\(toolbarGroup.map(String.init) ?? ""):\(id):\(testId ?? "")" }
            return "\(face):\(disabled):\(accessibilityDisabled)"
        }
        /// `source` but for whether the item is enabled.
        var face: String {
            if readout != nil { return source }
            return "\(toolbarGroup.map(String.init) ?? ""):\(id):\(title):\(symbol ?? ""):\(label ?? ""):\(prominent):\(ownGlass):\(badge?.source ?? ""):\(chip?.source ?? ""):\(menu):\(menuTitle):\(menuOrder):\(tint ?? []):\(followsTint):\(font.map { "\($0.fontName) \($0.pointSize) \(TextEngine.controlWeight($0))" } ?? ""):\(testId ?? ""):\(activeSignature)"
        }
        private var activeSignature: String {
            let values = active.style.mapValues(\.any)
            guard !values.isEmpty, let data = try? JSONSerialization.data(withJSONObject: values, options: [.sortedKeys]) else { return "" }
            return String(decoding: data, as: UTF8.self) + String(activeTitleColor != nil)
        }
    }
    let header: NodeView
    let heading: NodeView?
    let title: String
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
        // A readout hidden as authored, itself or by a box around it in the
        // header, shows nothing (the bar's own hiding is `isHidden`, not this).
        let readouts = rule.readouts.filter { readout in
            var at: UIView? = readout
            while let view = at, view !== rule.header {
                if (view as? NodeView)?.style["display"]?.string == "none" { return false }
                at = view.superview
            }
            return true
        }
        let roles = rule.items + readouts + [rule.heading, rule.tap, rule.search, rule.segments].compactMap { $0 }
        let authored = projected ? roles.filter { $0.wrote("appearance") && $0.style["appearance"]?.string == "none" } : []
        self.authored = authored
        let projects = { (node: NodeView) in !authored.contains { node === $0 || node.isDescendant(of: $0) } }
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
        let item = { (node: NodeView) -> Item? in
            guard rule.readouts.contains(where: { $0 === node }) else { return Item(node) }
            guard readouts.contains(where: { $0 === node }) else { return nil }
            let item = Item(node, readout: true)
            return item.readout == nil ? nil : item
        }
        let before = rule.leading.filter(projects).compactMap(item), after = rule.trailing.filter(projects).compactMap(item)
        // A readout is apart from the title when it has a face to show: one
        // hidden, or with nothing to draw, is the page's, as the web has it.
        let drawn = (before + after).filter { $0.readout != nil }.map(\.id)
        let faced = readouts.filter { drawn.contains($0.id) }
        #if os(tvOS)
        // tvOS's bar has no item that focus passes by undimmed: no readout.
        leading = before.filter { $0.readout == nil }
        trailing = after.filter { $0.readout == nil }
        let shownReadouts: [NodeView] = []
        #else
        (leading, trailing) = (before, after)
        let shownReadouts = faced
        #endif
        self.search = search
        self.segments = segments
        let group = heading.flatMap { HeaderTitle(header: rule.header, heading: $0, tap: tap, apart: rule.items + faced + [rule.search, rule.segments].compactMap { $0 }) }
        self.group = group
        let tablist = segments.flatMap { HeaderShape.titleTabs($0).isEmpty ? nil : $0 }
        // The title's pressable wrapper goes with the title it holds, unless
        // it holds an opted-out node, which stays where it is.
        let wrapper = titleProjects ? tap.flatMap { tap in authored.contains { $0.isDescendant(of: tap) } ? nil : tap } : nil
        placed = ([rule.heading].compactMap { $0 } + rule.items + shownReadouts + [search, tablist, wrapper].compactMap { $0 } + (HeaderShape.presented(group, segments: segments)?.placed ?? [])).filter(projects)
    }

    /// The tabs of a header's tablist the segmented title shows (§9.8).
    static func titleTabs(_ list: NodeView) -> [NodeView] {
        list.container.subviews.compactMap { $0 as? NodeView }.filter {
            $0.isButton && $0.props["accessibilityRole"] == "tab" && $0.handlers.contains("press")
        }
    }

    static func == (a: HeaderShape, b: HeaderShape) -> Bool {
        a.header === b.header && a.authoredBack == b.authoredBack && a.title == b.title && a.level == b.level && a.leading == b.leading && a.trailing == b.trailing && a.search === b.search && a.segments === b.segments && a.group == b.group && a.authored.map(\.id) == b.authored.map(\.id)
    }
}

#endif
