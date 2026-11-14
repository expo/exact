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

extension NodeView {
    #if os(iOS) || os(tvOS)
    static let canvasOverlays = NSHashTable<PlainView>.weakObjects()
    static let placedViews = NSHashTable<NodeView>.weakObjects()
    #else
    static let canvasOverlays = NSHashTable<FlippedView>.weakObjects()
    #endif

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
    func repaintThrough() {
        guard !Capture.capturing, let canvas = canvasAbove, !canvas.paintedThisTurn else { return }
        canvas.needsCapture = true
        canvases?.scheduleCapture()
    }
    #endif
}
