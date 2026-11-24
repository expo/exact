#if os(iOS) || os(tvOS)
import UIKit
private typealias CanvasView = UIView
#else
import AppKit
private typealias CanvasView = NSView
#endif

extension CanvasView {
    /// A canvas keeps its overlay's children directly in the overlay, even
    /// when a one-axis overflow clip wraps it.
    var canvasOverlayOwner: NodeView? {
        if let node = superview as? NodeView, node.overlay === self { return node }
        if let box = superview, let node = box.superview as? NodeView,
           node.clipBox === box, node.overlay === self { return node }
        return nil
    }
}

#if os(macOS)
extension NSView {
    /// Its subviews: the Mac adds no extension view to a box, as iOS does (LLP 1123 D13, `heldSubviews` on iOS).
    package var heldSubviews: [NSView] { subviews }
}
#endif

extension NodeView {
    #if os(iOS) || os(tvOS)
    static let canvasOverlays = NSHashTable<PlainView>.weakObjects()
    #else
    static let canvasOverlays = NSHashTable<FlippedView>.weakObjects()
    #endif

    static let placedViews = NSHashTable<NodeView>(options: [.weakMemory, .objectPointerPersonality])

    /// The node whose surface placement maps this subtree.
    package var placedAncestor: NodeView? {
        guard Self.placedViews.count > 0 else { return nil }
        var view: CanvasView? = self
        while let current = view {
            if let node = current as? NodeView, node.placement != nil || node.placementHidden { return node }
            if current.superview?.canvasOverlayOwner != nil { return nil }
            view = current.superview
        }
        return nil
    }

    /// The nearest canvas whose overlay holds this node. With no live
    /// overlays, text and input need no ancestor walk.
    package var canvasAbove: NodeView? {
        guard Self.canvasOverlays.count > 0 else { return nil }
        var view = superview
        while let current = view {
            if let canvas = current.canvasOverlayOwner { return canvas }
            view = current.superview
        }
        return nil
    }

    #if os(iOS) || os(tvOS)
    /// Something under a canvas repainted outside a batch (LLP 1014 D4 b, c).
    package func repaintThrough() {
        guard !Capture.capturing, let canvas = canvasAbove, !canvas.paintedThisTurn else { return }
        canvas.needsCapture = true
        canvases?.scheduleCapture()
    }
    #endif
}
