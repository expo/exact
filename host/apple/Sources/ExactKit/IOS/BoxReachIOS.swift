// LLP 1123 D13: a box that does not scroll reaches past its laid-out border box into the area that
// reaches it, as a UIKit view's background colour fills it to the screen's edge: its native view is
// outset on the pixel grid and its bounds origin shifted by as much, so its children stay where
// layout put them while its background, border and touches fill the reached frame. Its layout, its
// observations, its geometry and its accessibility frame keep the laid-out box. A scroller's reach
// grows its scroll view instead (`PageEdgesIOS.swift`).
#if os(iOS) || os(tvOS)
import UIKit

extension NodeView {
    /// Its laid-out border box in its own coordinates: where reach draws it past its frame on the
    /// pixel grid, the box at its origin, as the grid drew it; else its bounds.
    package var laidOutBox: CGRect {
        guard scrollView == nil, reach != .zero, let place = grid else { return bounds }
        return CGRect(origin: .zero, size: place.drawn.size)
    }

    /// A box that does not scroll drawn past its laid-out frame by `reach` (`setReach`).
    func reachBox(_ reach: UIEdgeInsets) {
        guard reach != self.reach || !reachGiven else { return }
        more.reachGiven = true
        more.reach = reach
        guard let place = grid else { return }
        moveOrPlace {
            let r = reached(place)
            guard r != nil || bounds.origin != .zero else { return }
            let t = transform
            transform = .identity
            frame = r?.frame ?? place.drawn
            bounds.origin = r?.origin ?? .zero
            transform = t
            placeExtension()
        }
    }

    /// Its frame drawn past `place` by its reach on the grid it is placed on, and the bounds origin
    /// that keeps its children where layout put them. `nil` where it draws no reach: a scroller,
    /// whose scroll view reaches; a row a grouped list carries into its cell, which the cell places;
    /// a box a surface paints at its own size; and one that reaches nothing.
    func reached(_ place: GridPlace) -> (frame: CGRect, origin: CGPoint)? {
        guard scrollView == nil, reach != .zero, surface == nil, let presenter else { return nil }
        if presenter.groupedLists?.carriesRows == true, presenter.groupedLists?.cellFrame(of: self, at: place) != nil { return nil }
        let r = reach, origin = presenter.gridOrigin(above: self)
        let outset = UIEdgeInsets(top: -r.top, left: -r.left, bottom: -r.bottom, right: -r.right)
        let drawn = PixelGrid.place(place.layout.inset(by: outset), in: origin, scale: presenter.gridScale(under: origin)).drawn
        return (drawn, CGPoint(x: drawn.minX - place.drawn.minX, y: drawn.minY - place.drawn.minY))
    }
}

extension Presenter {
    /// Gives the box `id` its reach (LLP 1123 D13), each edge's distance past its laid-out border box: a
    /// scroller's grows its scroll view, another's its own view, a flat box first taking a view to be
    /// drawn past its box. Activation supplies it from `Kernel::scroll_terms` and `Kernel::box_reach`
    /// (stage 5), and tests set it directly.
    package func setReach(_ id: UInt32, _ reach: UIEdgeInsets) {
        if flats.isFlat(id), reach != .zero {
            flats.promote(id)
            flats.flush()
        }
        views[id]?.setReach(reach)
    }
}
#endif
