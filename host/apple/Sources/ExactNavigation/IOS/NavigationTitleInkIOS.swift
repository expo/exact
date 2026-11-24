#if os(iOS) || os(tvOS)
import UIKit
import ExactKit

/// Item appearances keep an authored title colour and font local to its route.
final class NavigationTitleInk {
    /// The fonts a heading's authored typography gives the bar's inline and
    /// large titles; nil where it gives that title none (`fonts(of:level:traits:)`).
    struct Fonts: Equatable {
        var title: UIFont?, large: UIFont?
    }

    private var saved: [UINavigationBarAppearance?]?
    private var projected: [UINavigationBarAppearance?]?
    private var ink: UIColor?
    private var fonts: Fonts?
    private var transparent = false

    #if os(iOS)
    /// UIKit's own title fonts, which a row the author left unsaid keeps: the
    /// inline title's `headline` (17-point semibold at the default size) and
    /// the large title's bold `largeTitle` (34 points), each in `traits`
    /// (Dynamic Type, Bold Text).
    static func platformFonts(_ traits: UITraitCollection) -> (title: UIFont, large: UIFont) {
        let large = UIFont.preferredFont(forTextStyle: .largeTitle, compatibleWith: traits)
        let bold = large.fontDescriptor.withSymbolicTraits(.traitBold).map { UIFont(descriptor: $0, size: 0) } ?? large
        return (UIFont.preferredFont(forTextStyle: .headline, compatibleWith: traits), bold)
    }

    /// What `heading`'s authored typography gives the bar's titles. Its size
    /// is the title's it is shown as: the large title's for a level-1 heading
    /// (its collapsed inline title keeps UIKit's size), else the inline one's.
    /// Nil when its author wrote none of it, which leaves UIKit's fonts.
    static func fonts(of heading: NodeView?, level: Int, traits: UITraitCollection) -> Fonts? {
        // Read on every projection: most headings write none, and are answered before UIKit's fonts are made.
        guard let heading, ["font_family", "font_weight", "font_style", "font_size"].contains(where: heading.wrote) else { return nil }
        let platform = platformFonts(traits)
        let fonts = Fonts(title: heading.authoredTitleFont(over: platform.title, sized: level != 1),
                          large: heading.authoredTitleFont(over: platform.large, sized: level == 1))
        return fonts.title == nil && fonts.large == nil ? nil : fonts
    }
    #endif

    func update(_ color: UIColor?, fonts: Fonts? = nil, item: UINavigationItem, bar: UINavigationBar?, transparent: Bool) {
        let current = [item.standardAppearance, item.compactAppearance, item.scrollEdgeAppearance, item.compactScrollEdgeAppearance]
        if color == ink, fonts == self.fonts, transparent == self.transparent, current == projected { return }
        ink = color
        self.fonts = fonts
        self.transparent = transparent
        let styled = color != nil || fonts != nil
        if saved == nil, styled || transparent { saved = current }
        guard let saved else { return }
        let next = appearances(saved, color: color, fonts: fonts, bar: bar, transparent: transparent)
        item.standardAppearance = next[0]
        item.compactAppearance = next[1]
        item.scrollEdgeAppearance = next[2]
        item.compactScrollEdgeAppearance = next[3]
        projected = next
        if !styled, !transparent { self.saved = nil }
    }

    private func appearances(_ saved: [UINavigationBarAppearance?], color: UIColor?, fonts: Fonts?,
                             bar: UINavigationBar?, transparent: Bool) -> [UINavigationBarAppearance?] {
        let styled = color != nil || fonts != nil
        let base = bar?.standardAppearance ?? UINavigationBarAppearance()
        func painted(_ appearance: UINavigationBarAppearance?) -> UINavigationBarAppearance? {
            guard styled else { return appearance }
            let copy = (appearance ?? base).copy() as! UINavigationBarAppearance
            if let color { copy.titleTextAttributes[.foregroundColor] = color }
            if let font = fonts?.title { copy.titleTextAttributes[.font] = font }
            #if !os(tvOS)
            if let color { copy.largeTitleTextAttributes[.foregroundColor] = color }
            if let font = fonts?.large { copy.largeTitleTextAttributes[.font] = font }
            #endif
            return copy
        }
        return [
            painted(saved[0]),
            painted(styled ? saved[1] ?? bar?.compactAppearance : saved[1]),
            painted(transparent ? NavigationHost.clearScrollEdge : styled ? saved[2] ?? bar?.scrollEdgeAppearance : saved[2]),
            painted(transparent ? NavigationHost.clearScrollEdge : styled ? saved[3] ?? bar?.compactScrollEdgeAppearance : saved[3])
        ]
    }

    #if os(iOS)
    private(set) var hasHeaderFamily = false

    func updateFamily(_ family: HeaderFamily?, shape: HeaderShape?, route: RouteController, bar: UINavigationBar?,
                      color: UIColor?, fonts: Fonts?, transparent: Bool, afterHatch: Bool = false) {
        let item = route.navigationItem
        let current = [item.standardAppearance, item.compactAppearance, item.scrollEdgeAppearance, item.compactScrollEdgeAppearance]
        let standard = bar?.standardAppearance ?? UINavigationBarAppearance()
        let defaults = [standard, bar?.compactAppearance, bar?.scrollEdgeAppearance, bar?.compactScrollEdgeAppearance]
        // The route hatch sees the prepared colours and edges. Its return changes only fonts.
        var next = current
        if !afterHatch {
            var saved = self.saved ?? current
            if hasHeaderFamily, let projected {
                for i in current.indices where current[i] != projected[i] {
                    let inherited = defaults[i] ?? (i == 3 ? saved[2] ?? defaults[2] ?? saved[1] ?? defaults[1] : nil) ?? saved[0] ?? standard
                    let base = saved[i] ?? inherited
                    let edited = HeaderFamily.nativeEdit(current[i], after: projected[i], base: base)
                    if edited != base || current[i] == nil { saved[i] = edited }
                }
            }
            self.saved = saved
            self.ink = color
            self.fonts = fonts
            self.transparent = transparent
            hasHeaderFamily = family != nil
            next = appearances(saved, color: color, fonts: fonts, bar: bar, transparent: transparent)
        }
        var bases = next
        if let family {
            for i in next.indices {
                guard let base = next[i] ?? defaults[i] else { continue }
                let branded = family.appearance(over: base, heading: fonts, ownTitle: shape?.heading?.authoredFontFamily != nil,
                                                traits: bar?.traitCollection ?? route.traitCollection)
                if branded != base {
                    bases[i] = base.copy()
                    next[i] = branded
                }
            }
            route.titleView?.applyHeaderFont(family)
        }
        HeaderFamily.applyItems(in: route, shape: shape, family: family)
        if next != current {
            item.standardAppearance = next[0]
            item.compactAppearance = next[1]
            item.scrollEdgeAppearance = next[2]
            item.compactScrollEdgeAppearance = next[3]
            if #available(iOS 26, *), bar?.topItem === item, zip(current, next).contains(where: {
                $0?.buttonAppearance != $1?.buttonAppearance || $0?.doneButtonAppearance != $1?.doneButtonAppearance
            }) {
                if let transition = route.transitionCoordinator {
                    transition.animate(alongsideTransition: nil) { [weak bar, weak item] _ in
                        if let item, bar?.topItem === item { HeaderFamily.refreshItems(in: item) }
                    }
                } else {
                    HeaderFamily.refreshItems(in: item)
                }
            }
        }
        // Retain the underlying appearance for non-font edits and for the return to the parent path.
        if !afterHatch { projected = bases }
        if family == nil, color == nil, fonts == nil, !transparent { self.saved = nil }
    }
    #endif
}
#endif
