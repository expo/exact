#if os(iOS) || os(tvOS)
import UIKit

extension Presenter {
    /// Where the bar draws `source`, in `layer`: its bar item, or, for the
    /// pressable group a title stands for, the bar's title; nil when no bar
    /// draws it. A painted popover from it opens there.
    func barAnchor(of source: NodeView, in layer: UIView) -> CGRect? {
        if case .success(let (bar, item))? = navigation.barItem(source) { return bar.convert(item, to: layer) }
        #if os(iOS)
        if let title = navigation.titleSurface(for: source) { return title.convert(title.bounds, to: layer) }
        #endif
        return nil
    }
}
#endif
