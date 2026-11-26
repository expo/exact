// LLP 1123 D13 on AppKit: a box that does not scroll reaches past its laid-out border box into the area that
// reaches it, as an AppKit view's background fills a window to its edge under the title bar: its view is outset on
// the pixel grid and its bounds origin shifted by as much, so its children stay where layout put them while its
// background, border and clicks fill the reached frame. Its layout, its observations, its geometry and its
// accessibility frame keep the laid-out box. A scroller's reach is its scroll view's, not drawn here.
#if os(macOS)
import AppKit

extension NodeView {
    /// Each edge's distance past its laid-out border box that the box is drawn (`setReach`), zero for most boxes.
    package var reach: NSEdgeInsets { BoxReach.reaches.object(forKey: self)?.edges ?? NSEdgeInsetsZero }

    /// Its laid-out border box in its own coordinates: where reach draws it past its frame on the pixel grid, the box
    /// at its origin, as the grid drew it; else its bounds.
    package var laidOutBox: CGRect {
        guard scroll == nil, !NSEdgeInsetsEqual(reach, NSEdgeInsetsZero), let place = grid else { return bounds }
        return CGRect(origin: .zero, size: place.drawn.size)
    }

    /// Draws the box past its laid-out frame by `reach`, top right bottom left (`Kernel::box_reach`), or at it again at
    /// zero. A scroller keeps none of its own: its scroll view reaches instead.
    package func setReach(_ reach: NSEdgeInsets) {
        let given = BoxReach.reaches.object(forKey: self)
        guard given.map({ !NSEdgeInsetsEqual($0.edges, reach) }) ?? true else { return }
        if NSEdgeInsetsEqual(reach, NSEdgeInsetsZero) { BoxReach.reaches.removeObject(forKey: self) } else {
            BoxReach.reaches.setObject(BoxReach(reach), forKey: self)
        }
        guard let layout = grid?.layout else { return }
        presenter?.applyFrame(self, layout)
    }

    /// Its frame drawn past `place` by its reach on the grid it is placed on, and the bounds origin that keeps its
    /// children where layout put them; `nil` where it draws no reach: a scroller, whose scroll view reaches; a box a
    /// surface paints at its own size; and one that reaches nothing.
    func reached(_ place: GridPlace) -> (frame: CGRect, origin: CGPoint)? {
        let r = reach
        guard scroll == nil, surface == nil, !NSEdgeInsetsEqual(r, NSEdgeInsetsZero), let presenter else { return nil }
        let origin = presenter.gridOrigin(above: self)
        let outset = CGRect(x: place.layout.minX - r.left, y: place.layout.minY - r.top,
                            width: place.layout.width + r.left + r.right, height: place.layout.height + r.top + r.bottom)
        let drawn = PixelGrid.place(outset, in: origin, scale: presenter.gridScale(under: origin)).drawn
        return (drawn, CGPoint(x: drawn.minX - place.drawn.minX, y: drawn.minY - place.drawn.minY))
    }
}

/// A box's reach, kept beside its view while it has one (`NodeView.reach`).
final class BoxReach {
    let edges: NSEdgeInsets
    init(_ edges: NSEdgeInsets) { self.edges = edges }

    nonisolated(unsafe) static let reaches = NSMapTable<NodeView, BoxReach>.weakToStrongObjects()
}

extension Presenter {
    /// Gives the box `id` its reach (LLP 1123 D13), each edge's distance past its laid-out border box. Activation
    /// supplies it from `Kernel::box_reach` (stage 5), and tests set it directly.
    package func setReach(_ id: UInt32, _ reach: NSEdgeInsets) {
        views[id]?.setReach(reach)
    }
}
#endif
