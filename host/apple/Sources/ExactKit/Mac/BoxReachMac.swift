// LLP 1123 D13 on AppKit: a box that does not scroll reaches past its laid-out border box into the area that
// reaches it, as an AppKit view's background fills a window to its edge under the title bar: its view is outset on
// the pixel grid and its bounds origin shifted by as much, so its children stay where layout put them while its
// background, border and clicks fill the reached frame. Its layout, its observations, its geometry and its
// accessibility frame keep the laid-out box. A scroller reaches with its scroll view instead, which grows past the box
// keeping CSS `scrollTop` and `scrollLeft`, the box leaving the clip to it.
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

    /// Draws the box past its laid-out frame by `reach`, top right bottom left (`Kernel::box_reach`,
    /// `Kernel::scroll_terms` for a scroller), or at it again at zero; a scroller's scroll view reaches instead.
    package func setReach(_ reach: NSEdgeInsets) {
        let given = BoxReach.reaches.object(forKey: self)
        guard given.map({ !NSEdgeInsetsEqual($0.edges, reach) }) ?? !NSEdgeInsetsEqual(reach, NSEdgeInsetsZero) else { return }
        if NSEdgeInsetsEqual(reach, NSEdgeInsetsZero) { BoxReach.reaches.removeObject(forKey: self) } else {
            BoxReach.reaches.setObject(BoxReach(reach), forKey: self)
        }
        if scroll != nil { return reachScroll() }
        guard let layout = grid?.layout else { return }
        presenter?.applyFrame(self, layout)
    }

    /// The scroll view's frame: the box's bounds outset by its reach, so a scroller draws past its box into the area
    /// that reaches it.
    package var scrollFrame: CGRect {
        let r = reach, b = bounds
        return CGRect(x: b.minX - r.left, y: b.minY - r.top, width: b.width + r.left + r.right, height: b.height + r.top + r.bottom)
    }

    /// A scroller's scroll view moved to its reach (`scrollFrame`), keeping CSS `scrollTop` and `scrollLeft` through
    /// D14's next write (`ScrollInsetsMac.moving`), the box's own clip left to the scroll view while it reaches
    /// (`clipsItself`).
    func reachScroll() {
        guard let sv = scroll else { return }
        if surface?.clippedSelf != true, clipsToBounds != clipsItself { clipsToBounds = clipsItself }
        ScrollInsetsMac.moving(sv) {
            sv.frame = scrollFrame
            fitScroll()
        }
        syncScrollCorner()
    }

    /// A click at `point`, in the superview's coordinates, in a scroller's reached strip outside its box: its scroll
    /// view's, which AppKit's own hit test, stopping at the box's frame, does not find.
    func scrollReachHit(_ point: NSPoint) -> NSView? {
        guard let sv = scroll, !NSEdgeInsetsEqual(reach, NSEdgeInsetsZero) else { return nil }
        let local = convert(point, from: superview)
        return sv.frame.contains(local) ? sv.hitTest(local) : nil
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
