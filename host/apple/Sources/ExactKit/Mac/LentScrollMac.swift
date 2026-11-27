// A scroll view a native view lends the host on AppKit (LLP 1123 D15, `ExactNativeSafeArea.ScrollView`): D14's writer
// writes its content and scroller insets as it writes Exact's own scrollers, claiming it (`ScrollInsetsMac.Claim`), the
// view's own layout told the container clearance it may take and its answer the writer's consumed term; the release
// gives back what the claim borrowed. AppKit has no list or cell control of UIKit's kind to set.
#if os(macOS)
import AppKit
import ExactCore

package final class LentScrollMac {
    package let view: NSScrollView
    private let layout: ((NSEdgeInsets) -> NSEdgeInsets)?

    package init(view: NSScrollView, layout: ((NSEdgeInsets) -> NSEdgeInsets)?) {
        self.view = view
        self.layout = layout
    }

    /// Write D14's insets to the lent view, a scroller of the owner whose view is `owner`, as `ScrollInsetsMac.write`
    /// writes one: its layout is told the container clearance it may take on each edge, in its own coordinates, and what
    /// it says it cleared, at least 0 and at most that, is the consumed term. Activation supplies the terms and rows
    /// (stage 5); tests give them.
    package func write(in owner: NSView, terms: ScrollTerms, rows: ScrollInsetsMac.Rows) {
        guard view.window != nil, view.isDescendant(of: owner) else { return }
        let allowed = terms.container(band: ScrollInsetsMac.band(of: view, in: owner))
        let said = layout.map { layout -> [CGFloat] in
            let i = layout(NSEdgeInsets(top: allowed[0], left: allowed[3], bottom: allowed[2], right: allowed[1]))
            return [i.top, i.right, i.bottom, i.left]
        } ?? [0, 0, 0, 0]
        let consumed = (0..<4).map { e in said[e].isFinite ? min(max(0, said[e]), allowed[e]) : 0 }
        ScrollInsetsMac.write(view, in: owner, terms: terms, rows: rows, consumed: consumed)
    }

    /// Give back what the writer borrowed: the view's adjustment and insets as it had them.
    package func release() { ScrollInsetsMac.release(view) }
}
#endif
