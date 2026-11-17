import Foundation
#if os(macOS)
import AppKit
#else
import UIKit
#endif

package enum InspectionValue {
    package static func r2(_ value: CGFloat) -> Double { (Double(value) * 100).rounded() / 100 }
}

package enum InspectionGeometry {
    package static func rect(_ rect: CGRect) -> [String: Any] {
        ["x": InspectionValue.r2(rect.minX), "y": InspectionValue.r2(rect.minY),
         "w": InspectionValue.r2(rect.width), "h": InspectionValue.r2(rect.height)]
    }

    #if os(macOS)
    package static func box(_ bounds: CGRect, of view: NSView, viewport: NSScrollView) -> CGRect {
        if (view as? NodeView)?.placedAncestor?.placementHidden == true { return .zero }
        let clip = viewport.contentView
        if let node = view as? NodeView, node.placedAncestor?.placement != nil || node.drawnOffFrame {
            let rect = node.drawnRect(bounds, in: clip)
            return CGRect(x: rect.origin.x - clip.bounds.origin.x, y: rect.origin.y - clip.bounds.origin.y,
                          width: rect.width, height: rect.height)
        }
        let rect = clip.convertAcross(bounds.applying(view.layer?.affineTransform() ?? .identity), from: view)
        return CGRect(x: rect.origin.x - clip.bounds.origin.x, y: rect.origin.y - clip.bounds.origin.y,
                      width: rect.width, height: rect.height)
    }
    #else
    package static func box(_ bounds: CGRect, of view: UIView, viewport: UIScrollView) -> CGRect {
        if (view as? NodeView)?.placedAncestor?.placementHidden == true { return .zero }
        let origin = viewport.pageOrigin
        if let node = view as? NodeView, node.placedAncestor?.placement != nil {
            let rect = node.drawnRect(bounds, in: viewport)
            return CGRect(x: rect.origin.x - origin.x, y: rect.origin.y - origin.y,
                          width: rect.width, height: rect.height)
        }
        let rect = view.convert(bounds, to: viewport)
        return CGRect(x: rect.origin.x - origin.x, y: rect.origin.y - origin.y,
                      width: rect.width, height: rect.height)
    }
    #endif
}

#if os(iOS) || os(tvOS)
/// A node as a contact found it, including the incarnation that prevents a
/// pooled view reused for another node from becoming the original target.
struct HeldNode {
    weak var node: NodeView?
    let id: UInt32, incarnation: UInt64
    init(_ node: NodeView) { self.node = node; id = node.id; incarnation = node.incarnation }
    func live(in presenter: Presenter) -> NodeView? {
        guard let node, presenter.views[id] === node, node.id == id, node.incarnation == incarnation else { return nil }
        return node
    }
}

/// A header button's bar item that the bar shows no control for; `overflowed`,
/// that UIKit moved it into the bar's overflow menu.
struct BarItemMissing: Error { var overflowed = false }

extension NavigationHost {
    #if os(iOS)
    /// The bar button item made for an authored header node, in the bar of the route that shows it.
    func barButtonItem(_ node: NodeView) -> (bar: UINavigationBar, item: UIBarButtonItem, of: UINavigationItem)? {
        guard !ExactEnv.authoredChrome else { return nil }
        let testId = node.props["testId"]
        for route in controllers.values {
            guard let controller = route.navigationController, controller.topViewController === route, barShows(controller),
                  route.viewIfLoaded?.window != nil else { continue }
            let items = (route.navigationItem.leftBarButtonItems ?? []) + (route.navigationItem.rightBarButtonItems ?? [])
            guard let item = items.first(where: { ($0.target as? BarPress)?.id == node.id })
                    ?? items.first(where: { $0.target == nil && testId != nil && $0.accessibilityIdentifier == testId }) else { continue }
            return (controller.navigationBar, item, route.navigationItem)
        }
        return nil
    }
    #endif

    /// The navigation bar and, in it, the frame UIKit presents from for an
    /// authored header node's item: the bar's overflow button for an item in
    /// its overflow menu.
    func barItem(_ node: NodeView) -> Result<(bar: UIView, item: CGRect), BarItemMissing>? {
        #if os(iOS)
        guard let (bar, item, _) = barButtonItem(node) else { return nil }
        guard let frame = item.frame(in: bar), frame.width > 0, frame.height > 0 else { return .failure(BarItemMissing()) }
        return .success((bar, frame))
        #else
        return nil
        #endif
    }
}

extension UIScrollView {
    /// The committed offset along the scrolling axis and whether a finger,
    /// deceleration, or overscroll currently moves it.
    var scrollSample: (Double, String) {
        let inset = adjustedContentInset
        let sideways = contentSize.width + inset.left + inset.right > bounds.width + 0.5
            && contentSize.height + inset.top + inset.bottom <= bounds.height + 0.5
        let at = sideways ? contentOffset.x : contentOffset.y
        let low = sideways ? -inset.left : -inset.top
        let high = max(low, sideways ? contentSize.width + inset.right - bounds.width : contentSize.height + inset.bottom - bounds.height)
        return (Double(at), at < low - 0.5 || at > high + 0.5 ? "b" : isTracking && isDragging ? "t" : isDecelerating ? "d" : "-")
    }
}
#endif
