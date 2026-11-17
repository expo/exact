// A zoomable scroll container (@ref LLP 1057.001.000): what both Apple hosts
// read of it. iOS zooms with UIScrollView's zoom (`ScrollZoomIOS.swift`),
// macOS with NSScrollView's magnification (`ScrollZoomMac.swift`).
import CoreGraphics

extension NodeView {
    /// D1's range, when either row is written.
    var zoomRange: ClosedRange<CGFloat>? {
        let low = style["min_zoom"]?.number, high = style["max_zoom"]?.number
        guard low != nil || high != nil else { return nil }
        let valid = { (v: Double?) in v.flatMap { $0.isFinite && $0 > 0 ? CGFloat($0) : nil } ?? 1 }
        let lo = valid(low)
        return lo...max(lo, valid(high))
    }
    /// What D3 puts after the zoomed child's scaled border box, unscaled: its
    /// end margin and this scroller's end padding.
    func zoomTrail(_ child: NodeView) -> CGSize {
        let box = contentBox(), border = number("border_width")
        return CGSize(width: max(0, layoutBounds.maxX - number("border_width_right", border) - box.maxX) + child.number("margin_right"),
                      height: max(0, layoutBounds.maxY - number("border_width_bottom", border) - box.maxY) + child.number("margin_bottom"))
    }
    /// D3 on both axes at zoom `z` in a port of `port`, for a child laid out
    /// at `f` in the scroller's content: how far the child's scaled box
    /// stands from where layout put it (half the slack), and how far the
    /// content reaches.
    func zoomPlace(_ z: CGFloat, child: NodeView, frame f: CGRect, port: CGSize) -> (shift: CGPoint, used: CGSize) {
        let trail = zoomTrail(child)
        let used = CGSize(width: f.minX + f.width * z + trail.width, height: f.minY + f.height * z + trail.height)
        return (CGPoint(x: max(0, port.width - used.width) / 2, y: max(0, port.height - used.height) / 2), used)
    }
}
