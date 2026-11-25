#if os(macOS)
import AppKit
import ExactCore

/// D14's writer on AppKit (LLP 1123): a scroller's content and scroller insets from the kernel's
/// terms (`ScrollTerms`), its authored rows and its held terms, with its owner's band. AppKit makes
/// no adjustment of its own once the writer turns `automaticallyAdjustsContentInsets` off, so *P* is
/// 0: the content inset on each edge is the authored and held terms and the occupied depth, the
/// larger of the container and keyboard terms, less what a native layout clears itself there. AppKit
/// insets both scrollers by the content inset and one `scrollerInsets` beyond it, so the writer writes
/// there what puts each scroller where the indicator rule does: on each edge the indicator insets of
/// the scroller nearest it, the top and the vertical scroller's side the vertical scroller's, the
/// bottom and the other side the horizontal scroller's, with the occupied depth on every edge, as
/// AppKit's own adjustment moves its scrollers with the content. The clip view's own accommodation of
/// rulers and headers is AppKit's, neither written nor counted. Activation (stage 5) writes every
/// scroller through it; until then only its tests do.
package struct ScrollInsetsMac: Equatable {
    /// `contentInsets` and `scrollerInsets`, each top right bottom left.
    package var content: [CGFloat]
    package var scrollers: [CGFloat]

    /// What an author wrote on the scroller (D14's rows), each length top right bottom left.
    package struct Rows: Equatable {
        package var authored: [CGFloat] = [0, 0, 0, 0]
        package var verticalIndicator: [CGFloat] = [0, 0, 0, 0]
        package var horizontalIndicator: [CGFloat] = [0, 0, 0, 0]
        /// Whether each indicator inset adds the occupied depth (`-exact-scroll-indicator-adjustment: auto`).
        package var indicatorsFollow = true

        package init() {}

        /// The rows as the host's style holds them: lengths in points, and
        /// `-exact-scroll-indicator-adjustment` by its word; an unset row is its initial value.
        /// `-exact-scroll-inset-adjustment` chooses UIKit's adjustment, which AppKit has none of.
        package init(style: NodeStyle) {
            let lengths = { (row: String) in ["top", "right", "bottom", "left"].map { style["\(row)_\($0)"]?.length(basis: 0) ?? 0 } }
            authored = lengths("scroll_content_inset")
            verticalIndicator = lengths("scroll_vertical_indicator_insets")
            horizontalIndicator = lengths("scroll_horizontal_indicator_insets")
            indicatorsFollow = style["scroll_indicator_adjustment"]?.string != "none"
        }
    }

    /// D14's insets from the terms, the rows and the held terms, with the scroller's `band` and the
    /// depth a native layout clears itself, `consumed`, each top right bottom left; `verticalSide` is
    /// the edge the vertical scroller lies along, 1 for the right or 3 for the left.
    package static func insets(terms: ScrollTerms, rows: Rows, held: [CGFloat] = [0, 0, 0, 0], band: [CGFloat],
                               consumed: [CGFloat] = [0, 0, 0, 0], verticalSide: Int = 1) -> ScrollInsetsMac {
        let containers = terms.container(band: band)
        let occupied = (0..<4).map { e in max(containers[e], terms.keyboard[e], 0) }
        let content = (0..<4).map { e in rows.authored[e] + held[e] + occupied[e] - (terms.container[e] ? consumed[e] : 0) }
        let scrollers = (0..<4).map { e in
            let authored = (e == 0 || e == verticalSide ? rows.verticalIndicator : rows.horizontalIndicator)[e]
            return authored + (rows.indicatorsFollow ? occupied[e] : 0) - content[e]
        }
        return ScrollInsetsMac(content: content, scrollers: scrollers)
    }

    /// Write D14's insets to `scroll`, a scroller of the owner whose view is `owner`: AppKit's
    /// adjustment off, then its content and scroller insets, each skipped within half a device pixel.
    /// It keeps CSS `scrollTop` and `scrollLeft` as they were before the writes, or the end of a list
    /// that keeps its end (`scrollStart="end"`, `scrollFollowEnd`) and rested there. Out of a window,
    /// or out of `owner`, the band is unavailable (D9) and nothing is written.
    package static func write(_ scroll: NSScrollView, in owner: NSView, terms: ScrollTerms, rows: Rows,
                              held: [CGFloat] = [0, 0, 0, 0], consumed: [CGFloat] = [0, 0, 0, 0]) {
        guard scroll.window != nil, scroll.isDescendant(of: owner) else { return }
        let clip = scroll.contentView, before = scroll.contentInsets, origin = clip.bounds.origin
        let top = origin.y + before.top, left = origin.x + before.left
        let atEnd = keepsEnd(scroll) && origin.y >= end(of: scroll, under: before) - 1
        if scroll.automaticallyAdjustsContentInsets { scroll.automaticallyAdjustsContentInsets = false }
        let side = (scroll.verticalScroller?.frame.midX ?? .infinity) < scroll.bounds.midX ? 3 : 1
        let next = insets(terms: terms, rows: rows, held: held, band: band(of: scroll, in: owner), consumed: consumed, verticalSide: side)
        let half = 0.5 / max(scroll.window?.backingScaleFactor ?? 1, 1)
        if !near(scroll.contentInsets, next.content, half) { scroll.contentInsets = edgeInsets(next.content) }
        if !near(scroll.scrollerInsets, next.scrollers, half) { scroll.scrollerInsets = edgeInsets(next.scrollers) }
        let now = scroll.contentInsets
        let target = NSPoint(x: left - now.left, y: atEnd ? end(of: scroll, under: now) : top - now.top)
        let kept = clip.constrainBoundsRect(NSRect(origin: target, size: clip.bounds.size)).origin, at = clip.bounds.origin
        if abs(at.x - kept.x) > half || abs(at.y - kept.y) > half {
            clip.scroll(to: kept)
            scroll.reflectScrolledClipView(clip)
        }
    }

    /// D14's `band` for `scroll` on AppKit, top right bottom left: its frame's overlap with the area
    /// outside its owner view's safe area (`NSView.safeAreaRect`), at most that area's depth on each
    /// edge (D9), in the owner's orientation whether or not it is flipped.
    package static func band(of scroll: NSView, in owner: NSView) -> [CGFloat] {
        let frame = scroll.convert(scroll.bounds, to: owner), safe = owner.safeAreaRect, area = owner.safeAreaInsets
        let depth = { (value: CGFloat, most: CGFloat) in min(max(0, value), most) }
        let above = owner.isFlipped ? safe.minY - frame.minY : frame.maxY - safe.maxY
        let below = owner.isFlipped ? frame.maxY - safe.maxY : safe.minY - frame.minY
        return [depth(above, area.top), depth(frame.maxX - safe.maxX, area.right), depth(below, area.bottom), depth(safe.minX - frame.minX, area.left)]
    }

    /// Whether the node whose scroll view `scroll` is keeps its end as its insets move: it opens at
    /// its end or follows it.
    private static func keepsEnd(_ scroll: NSScrollView) -> Bool {
        guard let node = (scroll as? ChainingScrollView)?.pointerOwner else { return false }
        return node.props["scrollStart"] == "end" || node.props["scrollFollowEnd"] == "true"
    }

    /// The clip view's origin at which `scroll`'s content's end rests under `inset`.
    private static func end(of scroll: NSScrollView, under inset: NSEdgeInsets) -> CGFloat {
        let height = scroll.documentView?.frame.height ?? 0
        return max(-inset.top, height + inset.bottom - scroll.contentView.bounds.height)
    }

    private static func near(_ a: NSEdgeInsets, _ b: [CGFloat], _ tolerance: CGFloat) -> Bool {
        zip([a.top, a.right, a.bottom, a.left], b).allSatisfy { abs($0 - $1) <= tolerance }
    }

    private static func edgeInsets(_ edges: [CGFloat]) -> NSEdgeInsets {
        NSEdgeInsets(top: edges[0], left: edges[3], bottom: edges[2], right: edges[1])
    }
}
#endif
