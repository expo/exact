#if os(iOS) || os(tvOS)
import ExactCore
import UIKit

/// A native scroll view whose own layout clears some depth of the occupied region on some edges, top right bottom
/// left, as a grouped list insets its cards from the side safe area its layout uses (LLP 1123 D9: each native scroll
/// view an owner hosts reports the band it consumes). D14's writer writes the occupied depth less that there, and the
/// indicators, which the layout does not move, the whole of it, but for the vertical indicator's sides, which take
/// none (`ScrollInsetsIOS.insets`).
package protocol BandConsumer: AnyObject {
    var consumedBand: [CGFloat] { get }
}

/// D14's one writer of a scroller's content and indicator insets (LLP 1123): the kernel's terms
/// (`Kernel::scroll_terms`), the scroller's authored rows and its held terms, with the band and
/// the adjustment UIKit gives, written as one `contentInset` and two indicator insets. On each
/// edge the occupied depth is the larger of the container term and the keyboard term. The
/// container term is `reserved` where Exact reserves; elsewhere it is the scroller's overlap with
/// its owner's band, where that band reaches it. Where the container term is the band, the writer
/// adds what UIKit's adjustment *P* leaves of the occupied depth to the authored and held terms.
/// Where Exact reserves, the band does not reach the scroller, or the scroller ignores the
/// container region, it adds the occupied depth less *P* whatever its sign, so a negative inset
/// inside the root reaches the scroller, a scroller under another takes none of the band that one
/// took, and an ignored region cancels *P*. On an edge a native scroll view's own layout consumes
/// (`BandConsumer`), it adds the occupied depth less what that layout clears, less *P*. Activation
/// (stage 5) writes every scroller through it; until then only its tests do.
package struct ScrollInsetsIOS: Equatable {
    package var content: UIEdgeInsets
    package var verticalIndicator: UIEdgeInsets
    package var horizontalIndicator: UIEdgeInsets

    /// Write D14's insets to `scroll`, a scroller of the owner whose view is `owner`, the view of
    /// the nearest controller above it: its policy, then, reading the band and the adjustment UIKit
    /// gives under it, its content inset and its indicator insets, each skipped within half a
    /// device pixel. It keeps CSS `scrollTop` and `scrollLeft` as they were before the writes, or
    /// the end of a list that keeps its end and rested there (`ScrollView.restsAtEnd`), on the pixel
    /// grid, and clamped to the new range unless the scroll view is moving itself, and records the
    /// insets it kept the offset under (`ScrollView.kept(under:)`). Exact's reactions to the insets
    /// run once the writes end (`SafeAreaSteps`). Out of a window, or out of `owner`, the band is
    /// unavailable (D9) and nothing is written; activation writes the scroller when it enters one.
    package static func write(_ scroll: UIScrollView, in owner: UIView, terms: Terms, rows: Rows,
                              held: [CGFloat] = [0, 0, 0, 0], consumed lent: [CGFloat]? = nil) {
        guard scroll.window != nil, scroll.isDescendant(of: owner) else { return }
        SafeAreaSteps.stepping {
            let before = scroll.adjustedContentInset, free = scroll.scrollsItself
            let top = scroll.contentOffset.y + before.top, left = scroll.contentOffset.x + before.left
            // A list that keeps its end and is there keeps it; the rest keep CSS `scrollTop` and `scrollLeft`
            let atEnd = (scroll as? ScrollView).flatMap { !free && $0.restsAtEnd ? $0 : nil }
            if scroll.contentInsetAdjustmentBehavior != rows.adjustment { scroll.contentInsetAdjustmentBehavior = rows.adjustment }
            if scroll.automaticallyAdjustsScrollIndicatorInsets { scroll.automaticallyAdjustsScrollIndicatorInsets = false }
            if let own = scroll as? any SafeIndicators, !own.writtenWhole { own.writtenWhole = true }
            let adjusted = scroll.adjustedContentInset, inset = scroll.contentInset
            let platform = edges(UIEdgeInsets(top: adjusted.top - inset.top, left: adjusted.left - inset.left,
                                              bottom: adjusted.bottom - inset.bottom, right: adjusted.right - inset.right))
            // What a lent scroll view's own layout cleared (`LentScrollIOS`), or a native list's
            let consumed = lent ?? (scroll as? BandConsumer)?.consumedBand ?? [0, 0, 0, 0]
            var next = insets(terms: terms, rows: rows, held: held, band: band(of: scroll, in: owner), platform: platform, consumed: consumed)
            // The indicators keep inside a padding edge that masks the scroll view (`ScrollView.clip(toPaddingEdge:inset:)`)
            if let clip = (scroll as? ScrollView)?.paddingInset, clip != .zero {
                next.verticalIndicator = next.verticalIndicator.adding(clip)
                next.horizontalIndicator = next.horizontalIndicator.adding(clip)
            }
            let half = 0.5 / max(scroll.traitCollection.displayScale, 1)
            if !near(inset, next.content, half) { scroll.contentInset = next.content }
            if !near(scroll.verticalScrollIndicatorInsets, next.verticalIndicator, half) {
                scroll.verticalScrollIndicatorInsets = next.verticalIndicator
            }
            if !near(scroll.horizontalScrollIndicatorInsets, next.horizontalIndicator, half) {
                scroll.horizontalScrollIndicatorInsets = next.horizontalIndicator
            }
            let now = scroll.adjustedContentInset
            let target = PixelGrid.offset(CGPoint(x: left - now.left, y: atEnd?.end(under: now) ?? top - now.top), in: scroll, clamped: !free)
            if !PixelGrid.same(scroll.contentOffset, target, in: scroll) { scroll.contentOffset = target }
            (scroll as? ScrollView)?.kept(under: now)
        }
    }

    /// D14's insets from the terms, the rows and the held terms, with the scroller's native `band`,
    /// UIKit's adjustment `platform` (`adjustedContentInset − contentInset`) and the depth a native
    /// list's layout clears itself, `consumed` (`BandConsumer`), each top right bottom left.
    package static func insets(terms: Terms, rows: Rows, held: [CGFloat] = [0, 0, 0, 0], band: [CGFloat],
                               platform: [CGFloat], consumed: [CGFloat] = [0, 0, 0, 0]) -> ScrollInsetsIOS {
        var content = [CGFloat](repeating: 0, count: 4), occupied = content
        let containers = terms.container(band: band)
        for e in 0..<4 {
            let reaches = terms.bandReaches[e]
            occupied[e] = max(containers[e], terms.keyboard[e], 0)
            // What the layout clears of an observed region is not the writer's, and UIKit's adjustment over it is
            // cancelled, unclamped; an inset cannot undo what it clears of an ignored one, which it is to clear none of,
            // nor what it clears past a shallower reservation, which it is to clear no more than
            let past = occupied[e] - (terms.container[e] ? consumed[e] : 0) - platform[e]
            let clamped = terms.container[e] && terms.reserved[e] == nil && reaches && consumed[e] == 0
            content[e] = rows.authored[e] + held[e] + (clamped ? max(0, past) : past)
        }
        // The vertical indicator takes no side band: it lies at the scroll view's side, as UIKit's own does, never over
        // content that keeps to the band's inner edge (`SafeIndicatorTerm`)
        let follow = { (authored: [CGFloat], sides: Bool) in
            (0..<4).map { e in authored[e] + (rows.indicatorsFollow && (sides || e % 2 == 0) ? occupied[e] : 0) }
        }
        return ScrollInsetsIOS(content: edgeInsets(content), verticalIndicator: edgeInsets(follow(rows.verticalIndicator, false)),
                               horizontalIndicator: edgeInsets(follow(rows.horizontalIndicator, true)))
    }

    /// D14's native `band` for `scroll`, top right bottom left, in the scroller's own coordinates,
    /// where its insets are written: its bounds' overlap with the unsafe area of its owner's view, as
    /// UIKit gives that view, at most that area's depth on each edge however small the scroller (D9),
    /// so a scroller scaled under its owner takes the band at its own scale. A scroll view's own `safeAreaInsets` do not serve: UIKit
    /// gives a view none of the band past its superview's box, so a scroll view that reach outsets
    /// reads less than it overlaps (LLP 1123, the evidence row "Outset scroll view"). It reads the
    /// overlap at the current offsets of the scroll views above, and it holds Exact's own writes to
    /// the owner's controller, which D11's writers are to take out.
    package static func band(of scroll: UIView, in owner: UIView) -> [CGFloat] {
        let whole = scroll.convert(owner.bounds, from: owner), frame = scroll.bounds
        let safe = scroll.convert(owner.bounds.inset(by: owner.safeAreaInsets), from: owner)
        let depth = { (value: CGFloat, most: CGFloat) in min(max(0, value), max(0, most)) }
        return [depth(safe.minY - frame.minY, safe.minY - whole.minY), depth(frame.maxX - safe.maxX, whole.maxX - safe.maxX),
                depth(frame.maxY - safe.maxY, whole.maxY - safe.maxY), depth(safe.minX - frame.minX, safe.minX - whole.minX)]
    }

    /// The kernel's terms for one scroller, shared with AppKit's writer.
    package typealias Terms = ScrollTerms

    /// What an author wrote on the scroller (D14's rows), each length top right bottom left.
    package struct Rows: Equatable {
        package var authored: [CGFloat] = [0, 0, 0, 0]
        package var verticalIndicator: [CGFloat] = [0, 0, 0, 0]
        package var horizontalIndicator: [CGFloat] = [0, 0, 0, 0]
        package var adjustment: UIScrollView.ContentInsetAdjustmentBehavior = .automatic
        /// Whether each indicator inset adds the occupied depth (`-exact-scroll-indicator-adjustment: auto`).
        package var indicatorsFollow = true

        package init() {}

        /// The rows as the host's style holds them: lengths in points, `-exact-scroll-inset-adjustment`
        /// and `-exact-scroll-indicator-adjustment` by their words; an unset row is its initial value.
        package init(style: NodeStyle) {
            let lengths = { (row: String) in ["top", "right", "bottom", "left"].map { style["\(row)_\($0)"]?.length(basis: 0) ?? 0 } }
            authored = lengths("scroll_content_inset")
            verticalIndicator = lengths("scroll_vertical_indicator_insets")
            horizontalIndicator = lengths("scroll_horizontal_indicator_insets")
            adjustment = switch style["scroll_inset_adjustment"]?.string {
            case "scrollable-axes": .scrollableAxes
            case "always": .always
            case "never": .never
            default: .automatic
            }
            indicatorsFollow = style["scroll_indicator_adjustment"]?.string != "none"
        }
    }

    private static func near(_ first: UIEdgeInsets, _ second: UIEdgeInsets, _ within: CGFloat) -> Bool {
        abs(first.top - second.top) < within && abs(first.left - second.left) < within
            && abs(first.bottom - second.bottom) < within && abs(first.right - second.right) < within
    }

    /// `insets` top right bottom left, the kernel's order.
    static func edges(_ insets: UIEdgeInsets) -> [CGFloat] { [insets.top, insets.right, insets.bottom, insets.left] }

    /// `edges`, top right bottom left, as UIKit's insets.
    static func edgeInsets(_ edges: [CGFloat]) -> UIEdgeInsets {
        UIEdgeInsets(top: edges[0], left: edges[3], bottom: edges[2], right: edges[1])
    }
}
#endif
