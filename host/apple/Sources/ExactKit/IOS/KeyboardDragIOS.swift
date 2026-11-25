#if os(iOS) || os(tvOS)
import ObjectiveC
import UIKit

extension Presenter {
    /// A finger drags a scroller in an interactive dismiss mode, the keyboard following it: the session's page, a
    /// node's scroller or a primary route's page.
    var interactiveKeyboardDrag: Bool {
        let drags = { (sv: UIScrollView) in
            [.interactive, .interactiveWithAccessory].contains(sv.keyboardDismissMode) && (sv.isTracking || sv.isDragging)
        }
        return drags(viewport) || scrollers.contains { views[$0]?.scrollView.map(drags) ?? false }
            || navigation.primaryRouteSurfaces.contains { $0.contentViewport.map(drags) ?? false }
    }
}
#endif

#if os(iOS)

/// A scroller's keyboard drag (LLP 1123 D7). While a finger drags the scroll view in an interactive dismiss mode, the
/// drag holds its window's dismiss padding (`KeyboardSampleIOS.hold`): its node's `-exact-keyboard-dismiss-padding` in
/// the window's points, read again as the finger moves. It lets the padding go as the drag ends, is cancelled or
/// fails, which UIKit reports too for a scroll view taken out of its window mid-drag. The scroll view's own pan target
/// was added first and hears the end first, so the release leaves UIKit's choice to dismiss the keyboard or bring it
/// back as it was (§"Evidence: measured on expo/exact", "Keyboard dismissal").
package final class KeyboardDrag: NSObject {
    private weak var scroll: UIScrollView?
    private var node: () -> NodeView?
    private weak var holding: KeyboardSampleIOS?

    nonisolated(unsafe) private static var key: UInt8 = 0

    private init(_ scroll: UIScrollView, node: @escaping () -> NodeView?) {
        self.scroll = scroll
        self.node = node
        super.init()
    }

    /// Watches `scroll`'s drags, whose padding is `node`'s: once per scroll view, a later call naming its node again.
    package static func watch(_ scroll: UIScrollView, node: @escaping () -> NodeView?) {
        if let drag = objc_getAssociatedObject(scroll, &key) as? KeyboardDrag {
            drag.node = node
            return
        }
        let drag = KeyboardDrag(scroll, node: node)
        objc_setAssociatedObject(scroll, &key, drag, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        scroll.panGestureRecognizer.addTarget(drag, action: #selector(panned(_:)))
    }

    @objc private func panned(_ pan: UIPanGestureRecognizer) {
        guard let scroll else { return }
        switch pan.state {
        case .began, .changed:
            let interactive = scroll.keyboardDismissMode == .interactive || scroll.keyboardDismissMode == .interactiveWithAccessory
            guard interactive, let window = scroll.window, let sampler = KeyboardSampleIOS.existing(in: window) else { return release() }
            if holding !== sampler { release() }
            holding = sampler
            let padding = max(0, CGFloat(node()?.style["keyboard_dismiss_padding"]?.number ?? 0))
            sampler.hold(scroll.convert(CGRect(x: 0, y: 0, width: 0, height: padding), to: window).height, for: scroll)
        default:
            release()
            node()?.presenter?.keyboardDragEnded()
        }
    }

    private func release() {
        holding?.release(scroll)
        holding = nil
    }
}
#endif
