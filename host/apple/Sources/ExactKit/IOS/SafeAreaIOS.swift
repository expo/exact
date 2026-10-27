#if os(iOS) || os(tvOS)
import UIKit

extension NodeView {
    /// UIKit's inset changed as a bar or the safe area did: a route that turns
    /// an edge off corrects it now, before the next batch's cover report.
    package func scrollViewDidChangeAdjustedContentInset(_ scrollView: UIScrollView) {
        guard scrollView === self.scrollView else { return }
        presenter?.navigation.adjustedInsetChanged(self)
    }
}
#endif
