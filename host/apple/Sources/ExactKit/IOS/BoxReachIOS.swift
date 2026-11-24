// LLP 1123 D13: a box that does not scroll reaches past its laid-out border box into the area that
// reaches it, as a UIKit view's background colour fills it to the screen's edge: its native view is
// outset on the pixel grid and its bounds origin shifted by as much, so its children stay where
// layout put them while its background, border and touches fill the reached frame. Its layout, its
// observations, its geometry and its accessibility frame keep the laid-out box. A scroller's reach
// grows its scroll view instead (`PageEdgesIOS.swift`).
#if os(iOS) || os(tvOS)
import UIKit

extension NodeView {
    /// Its laid-out border box in its own coordinates: its bounds less the reach a box that does
    /// not scroll is drawn past it by.
    package var laidOutBox: CGRect { scrollView == nil ? bounds.inset(by: reach) : bounds }

    /// A box that does not scroll drawn past its laid-out frame by `reach` (`setReach`).
    func reachBox(_ reach: UIEdgeInsets) {
        guard reach != self.reach || !reachGiven else { return }
        more.reachGiven = true
        more.reach = reach
        moveOrPlace { outsetByReach() }
    }

    /// Its frame outset by its reach on the grid it is placed on, and its bounds origin shifted so
    /// that its children keep their places, as each frame it is given is (`Presenter.applyFrame`).
    func outsetByReach() {
        guard scrollView == nil, let place = grid, let presenter else { return }
        let r = reach, origin = presenter.gridOrigin(above: self)
        let outset = UIEdgeInsets(top: -r.top, left: -r.left, bottom: -r.bottom, right: -r.right)
        let drawn = r == .zero ? place.drawn : PixelGrid.place(place.layout.inset(by: outset), in: origin, scale: presenter.gridScale(under: origin)).drawn
        let t = transform
        transform = .identity
        frame = drawn
        bounds.origin = CGPoint(x: drawn.minX - place.drawn.minX, y: drawn.minY - place.drawn.minY)
        transform = t
        placeExtension()
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
