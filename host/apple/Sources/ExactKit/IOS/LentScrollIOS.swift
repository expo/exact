// A scroll view a native view lends the host (LLP 1123 D15, `ExactNativeSafeArea.ScrollView`): D14's
// writer writes its content and indicator insets as it writes Exact's own scrollers, the view's own
// layout told the container clearance it may take and its answer the writer's consumed term, and the
// five properties it borrowed are given back on release.
#if os(iOS) || os(tvOS)
import UIKit

package final class LentScrollIOS {
    package let view: UIScrollView
    private let layout: ((UIEdgeInsets) -> UIEdgeInsets)?
    /// The properties as the view had them before the host's first write, while the host holds them.
    private var borrowed: Borrowed?

    private struct Borrowed {
        let content, vertical, horizontal: UIEdgeInsets
        let adjustment: UIScrollView.ContentInsetAdjustmentBehavior
        let adjustsIndicators: Bool

        init(_ v: UIScrollView) {
            (content, vertical, horizontal) = (v.contentInset, v.verticalScrollIndicatorInsets, v.horizontalScrollIndicatorInsets)
            (adjustment, adjustsIndicators) = (v.contentInsetAdjustmentBehavior, v.automaticallyAdjustsScrollIndicatorInsets)
        }
    }

    package init(view: UIScrollView, layout: ((UIEdgeInsets) -> UIEdgeInsets)?) {
        self.view = view
        self.layout = layout
    }

    /// Write D14's insets to the lent view, a scroller of the owner whose view is `owner`, as
    /// `ScrollInsetsIOS.write` writes one, borrowing its properties first: its layout is told the
    /// container clearance it may take on each edge, in its own coordinates, and what it says it
    /// cleared, at least 0 and at most that, is the consumed term. Activation supplies the terms and
    /// rows (stage 5); tests give them.
    package func write(in owner: UIView, terms: ScrollInsetsIOS.Terms, rows: ScrollInsetsIOS.Rows) {
        guard view.window != nil, view.isDescendant(of: owner) else { return }
        if borrowed == nil { borrowed = Borrowed(view) }
        let allowed = ScrollInsetsIOS.container(terms: terms, band: ScrollInsetsIOS.band(of: view, in: owner))
        let said = layout.map { ScrollInsetsIOS.edges($0(ScrollInsetsIOS.edgeInsets(allowed))) } ?? [0, 0, 0, 0]
        let consumed = (0..<4).map { e in said[e].isFinite ? min(max(0, said[e]), allowed[e]) : 0 }
        ScrollInsetsIOS.write(view, in: owner, terms: terms, rows: rows, consumed: consumed)
    }

    /// Give back the five properties as the view had them before the host's first write.
    package func release() {
        guard let b = borrowed else { return }
        borrowed = nil
        view.contentInsetAdjustmentBehavior = b.adjustment
        view.automaticallyAdjustsScrollIndicatorInsets = b.adjustsIndicators
        view.contentInset = b.content
        view.verticalScrollIndicatorInsets = b.vertical
        view.horizontalScrollIndicatorInsets = b.horizontal
    }
}
#endif
