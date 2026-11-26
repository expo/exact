#if os(macOS)
import AppKit

/// The layout regions AppKit lays out beside the safe area (LLP 1123 D10),
/// what `env(-exact-<region>-inset-<edge>)` resolves to: each region of the
/// session's view, as an inset per edge from its bounds, the viewport.
/// AppKit is the source: its corner adaptation is macOS 26's
/// `NSView.LayoutRegion`, and a region below macOS 26 is the one without
/// it. AppKit has no readable-content region: it is the margins.
enum LayoutRegionsMac {
    /// Twenty insets, five regions of top, right, bottom, left, in
    /// `Region`'s order (`kernel/src/style/env.rs`), none less than zero.
    static func insets(of view: NSView) -> [CGFloat] {
        let edges = { (rect: NSRect) in Self.edges(of: rect, in: view) }
        let safe: [CGFloat], margins = Self.margins(of: view), marginsCorners: [CGFloat]
        if #available(macOS 26.0, *) {
            func adapted(_ region: (NSView.LayoutRegion.AdaptivityAxis) -> NSView.LayoutRegion) -> [CGFloat] {
                let h = edges(view.rect(for: region(.horizontal))), v = edges(view.rect(for: region(.vertical)))
                return [v[0], h[1], v[2], h[3]]
            }
            safe = adapted { .safeArea(cornerAdaptation: $0) }
            marginsCorners = adapted { .margins(cornerAdaptation: $0) }
        } else {
            // The safe area and the margins guide, without adaptation
            safe = edges(view.safeAreaRect)
            marginsCorners = margins
        }
        return safe + margins + marginsCorners + margins + marginsCorners
    }

    /// The view's margins, top, right, bottom, left: macOS 26's margins region, or before it the margins guide; `signed`,
    /// as AppKit gives them where a negative safe area puts them past the bounds, else at least 0.
    static func margins(of view: NSView, signed: Bool = false) -> [CGFloat] {
        if #available(macOS 26.0, *) { return edges(of: view.rect(for: .margins()), in: view, signed: signed) }
        return edges(of: view.layoutMarginsGuide.frame, in: view, signed: signed)
    }

    /// How far `rect` lies inside the view's bounds at each edge, top, right, bottom, left, in the view's own
    /// orientation, at least 0 unless `signed`.
    private static func edges(of rect: NSRect, in view: NSView, signed: Bool = false) -> [CGFloat] {
        let bounds = view.bounds
        let top = view.isFlipped ? rect.minY - bounds.minY : bounds.maxY - rect.maxY
        let bottom = view.isFlipped ? bounds.maxY - rect.maxY : rect.minY - bounds.minY
        return [top, bounds.maxX - rect.maxX, bottom, rect.minX - bounds.minX].map { signed ? $0 : max(0, $0) }
    }
}
#endif
