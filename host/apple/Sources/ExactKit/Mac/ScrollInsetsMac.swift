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
/// rulers and headers is AppKit's, neither written nor counted. A write claims the scroll view
/// (`Claim`): from then until its release the writer alone writes its adjustment and insets, the
/// host's other writers leave them, and a zoomed child's placement is one of its held terms.
/// Activation (stage 5)
/// writes every scroller through it; until then only its tests do.
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

    /// D14's hold on a scroll view, from the writer's first write to its release: what the view had
    /// before, which the release gives back, and the last write's inputs, which `rewrite` writes again.
    package final class Claim {
        fileprivate let adjusts: Bool, content: NSEdgeInsets, scrollers: NSEdgeInsets
        /// Whether a zoom placed the content insets when the writer first wrote: they were its placement, not the view's own.
        fileprivate let zoomed: Bool
        fileprivate weak var owner: NSView?
        fileprivate var terms = ScrollTerms(), rows = Rows(), held: [CGFloat] = [0, 0, 0, 0], consumed: [CGFloat] = [0, 0, 0, 0]

        fileprivate init(_ scroll: NSScrollView) {
            (adjusts, content, scrollers) = (scroll.automaticallyAdjustsContentInsets, scroll.contentInsets, scroll.scrollerInsets)
            zoomed = (scroll as? ChainingScrollView)?.zoom != nil
        }
    }

    nonisolated(unsafe) private static var claimKey: UInt8 = 0

    /// The writer's hold on `scroll`, while it has one.
    package static func claim(of scroll: NSScrollView) -> Claim? {
        objc_getAssociatedObject(scroll, &claimKey) as? Claim
    }

    /// Write D14's insets to `scroll`, a scroller of the owner whose view is `owner`, claiming it at the
    /// first: AppKit's adjustment off, then its content and scroller insets, each skipped within half a
    /// device pixel. It keeps CSS `scrollTop` and `scrollLeft` as they were before the writes, or the
    /// end of a list that keeps its end (`scrollStart="end"`, `scrollFollowEnd`) and rested there. Out
    /// of a window, or out of `owner`, the band is unavailable (D9) and nothing is written.
    package static func write(_ scroll: NSScrollView, in owner: NSView, terms: ScrollTerms, rows: Rows,
                              held: [CGFloat] = [0, 0, 0, 0], consumed: [CGFloat] = [0, 0, 0, 0]) {
        guard scroll.window != nil, scroll.isDescendant(of: owner) else { return }
        let claim = claim(of: scroll) ?? {
            let made = Claim(scroll)
            objc_setAssociatedObject(scroll, &claimKey, made, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
            return made
        }()
        (claim.owner, claim.terms, claim.rows, claim.held, claim.consumed) = (owner, terms, rows, held, consumed)
        let side = (scroll.verticalScroller?.frame.midX ?? .infinity) < scroll.bounds.midX ? 3 : 1
        let zoom = (scroll as? ChainingScrollView)?.zoom?.placement ?? NSEdgeInsetsZero
        let placed = [zoom.top, zoom.right, zoom.bottom, zoom.left]
        let next = insets(terms: terms, rows: rows, held: (0..<4).map { held[$0] + placed[$0] }, band: band(of: scroll, in: owner),
                          consumed: consumed, verticalSide: side)
        keepingOffset(scroll) {
            if scroll.automaticallyAdjustsContentInsets { scroll.automaticallyAdjustsContentInsets = false }
            if !near(scroll.contentInsets, next.content, $0) { scroll.contentInsets = edgeInsets(next.content) }
            if !near(scroll.scrollerInsets, next.scrollers, $0) { scroll.scrollerInsets = edgeInsets(next.scrollers) }
        }
    }

    /// Write a claimed `scroll`'s insets again from its last write's inputs, as its zoomed child's
    /// placement moves.
    package static func rewrite(_ scroll: NSScrollView) {
        guard let claim = claim(of: scroll), let owner = claim.owner else { return }
        write(scroll, in: owner, terms: claim.terms, rows: claim.rows, held: claim.held, consumed: claim.consumed)
    }

    /// End the writer's hold on `scroll`: its adjustment and its content and scroller insets as it had
    /// them before the first write, or a zoomed child's placement as it is now, none where the zoom that
    /// placed them has gone, keeping CSS `scrollTop` and `scrollLeft`; the host's other writers then
    /// write them again.
    package static func release(_ scroll: NSScrollView) {
        guard let claim = claim(of: scroll) else { return }
        objc_setAssociatedObject(scroll, &claimKey, nil, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        keepingOffset(scroll) { _ in
            scroll.automaticallyAdjustsContentInsets = claim.adjusts
            scroll.contentInsets = (scroll as? ChainingScrollView)?.zoom?.placement ?? (claim.zoomed ? NSEdgeInsetsZero : claim.content)
            scroll.scrollerInsets = claim.scrollers
        }
    }

    /// The offset `scroll` had before its reach moved it (`moving`), which AppKit clamped to the moved port's range under
    /// the old insets: CSS `scrollTop` and `scrollLeft` in the scroll view's points, whether it rested at a kept end, and
    /// the clip's origin AppKit left. The writer's next write or release keeps it and drops it; a scroll since, which
    /// moves the clip from that origin, makes it stale, so a record no write takes up keeps nothing from anyone.
    private final class Moved {
        let top: CGFloat, left: CGFloat, atEnd: Bool, at: NSPoint
        init(top: CGFloat, left: CGFloat, atEnd: Bool, at: NSPoint) { (self.top, self.left, self.atEnd, self.at) = (top, left, atEnd, at) }
    }

    nonisolated(unsafe) private static var movedKey: UInt8 = 0

    /// Make `move`, which moves `scroll`'s frame for its reach (D13), keeping CSS `scrollTop` and `scrollLeft` as far as
    /// the range under its insets allows; where AppKit clamps the offset, the writer's next write, whose insets follow
    /// the reach, keeps the offset from before `move`, clamped to the range those insets give (D14).
    package static func moving(_ scroll: NSScrollView, _ move: () -> Void) {
        let clip = scroll.contentView, inset = scroll.contentInsets, origin = clip.bounds.origin, z = max(scroll.magnification, 0.01)
        let atEnd = keepsEnd(scroll) && origin.y >= end(of: scroll, under: inset) - 1
        move()
        objc_setAssociatedObject(scroll, &movedKey, nil, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        let kept = clip.constrainBoundsRect(NSRect(origin: origin, size: clip.bounds.size)).origin
        if kept != clip.bounds.origin {
            clip.scroll(to: kept)
            scroll.reflectScrolledClipView(clip)
        }
        if clip.bounds.origin != origin {
            let moved = Moved(top: origin.y * z + inset.top, left: origin.x * z + inset.left, atEnd: atEnd, at: clip.bounds.origin)
            objc_setAssociatedObject(scroll, &movedKey, moved, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        }
    }

    /// Make `writes` to `scroll`'s insets, given half a device pixel, keeping CSS `scrollTop` and
    /// `scrollLeft`, or the end of a list that keeps its end and rested there, as they were before a reach that
    /// moved it clamped them (`moving`). The offset is kept in the scroll view's points, the insets' units: the
    /// clip's origin, in the document's, times the magnification.
    private static func keepingOffset(_ scroll: NSScrollView, _ writes: (CGFloat) -> Void) {
        let clip = scroll.contentView, before = scroll.contentInsets, origin = clip.bounds.origin, z = max(scroll.magnification, 0.01)
        let moved = (objc_getAssociatedObject(scroll, &movedKey) as? Moved).flatMap { $0.at == origin ? $0 : nil }
        objc_setAssociatedObject(scroll, &movedKey, nil, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        let top = moved?.top ?? origin.y * z + before.top, left = moved?.left ?? origin.x * z + before.left
        let atEnd = moved?.atEnd ?? (keepsEnd(scroll) && origin.y >= end(of: scroll, under: before) - 1)
        let half = 0.5 / max(scroll.window?.backingScaleFactor ?? 1, 1)
        writes(half)
        let now = scroll.contentInsets
        let target = NSPoint(x: (left - now.left) / z, y: atEnd ? end(of: scroll, under: now) : (top - now.top) / z)
        let kept = clip.constrainBoundsRect(NSRect(origin: target, size: clip.bounds.size)).origin, at = clip.bounds.origin
        if abs(at.x - kept.x) > half || abs(at.y - kept.y) > half {
            clip.scroll(to: kept)
            scroll.reflectScrolledClipView(clip)
        }
    }

    /// D14's `band` for `scroll` on AppKit, top right bottom left, in the scroll view's own coordinates,
    /// where its insets are written: its bounds' overlap with the area outside its owner view's safe
    /// area (`NSView.safeAreaRect`), at most that area's depth on each edge (D9), whether or not
    /// either is flipped.
    package static func band(of scroll: NSView, in owner: NSView) -> [CGFloat] {
        let whole = scroll.convert(owner.bounds, from: owner), safe = scroll.convert(owner.safeAreaRect, from: owner)
        let frame = scroll.bounds, flipped = scroll.isFlipped
        let depth = { (value: CGFloat, most: CGFloat) in min(max(0, value), max(0, most)) }
        let above = flipped ? depth(safe.minY - frame.minY, safe.minY - whole.minY) : depth(frame.maxY - safe.maxY, whole.maxY - safe.maxY)
        let below = flipped ? depth(frame.maxY - safe.maxY, whole.maxY - safe.maxY) : depth(safe.minY - frame.minY, safe.minY - whole.minY)
        return [above, depth(frame.maxX - safe.maxX, whole.maxX - safe.maxX), below, depth(safe.minX - frame.minX, safe.minX - whole.minX)]
    }

    /// Whether the node whose scroll view `scroll` is keeps its end as its insets move: it opens at
    /// its end or follows it.
    private static func keepsEnd(_ scroll: NSScrollView) -> Bool {
        guard let node = (scroll as? ChainingScrollView)?.pointerOwner else { return false }
        return node.props["scrollStart"] == "end" || node.props["scrollFollowEnd"] == "true"
    }

    /// The clip view's origin at which `scroll`'s content's end rests under `inset`.
    private static func end(of scroll: NSScrollView, under inset: NSEdgeInsets) -> CGFloat {
        let height = scroll.documentView?.frame.height ?? 0, z = max(scroll.magnification, 0.01)
        return max(-inset.top / z, height + inset.bottom / z - scroll.contentView.bounds.height)
    }

    private static func near(_ a: NSEdgeInsets, _ b: [CGFloat], _ tolerance: CGFloat) -> Bool {
        zip([a.top, a.right, a.bottom, a.left], b).allSatisfy { abs($0 - $1) <= tolerance }
    }

    private static func edgeInsets(_ edges: [CGFloat]) -> NSEdgeInsets {
        NSEdgeInsets(top: edges[0], left: edges[3], bottom: edges[2], right: edges[1])
    }
}
#endif
