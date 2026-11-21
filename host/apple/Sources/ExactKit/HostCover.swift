// What a native container covers of a box: navigation's bars, toolbars,
// sheets and status items report it, and the kernel lays out around it.
import CoreGraphics

/// What a native container covers of a box (LLP 1075.003 §3.5): bars over
/// its edges, added to its padding, or the whole box, which a native bar
/// replaces (`display: none`).
package enum HostCover: Equatable {
    package struct Edges: Equatable {
        package var top: CGFloat, right: CGFloat, bottom: CGFloat, left: CGFloat
        package init(top: CGFloat, right: CGFloat, bottom: CGFloat, left: CGFloat) {
            self.top = top; self.right = right; self.bottom = bottom; self.left = left
        }
    }
    case edges(Edges)
    case whole
    /// Replaced by a bar that shows the nodes given blocks inside the box in
    /// views of its own (LLP 1141.004 D2): `display: none`, as `whole`, but
    /// for those blocks, which are laid out and shown.
    case bar
    /// A scroller UIKit insets (a route's collapsing or bottom-edge
    /// scroller): the kernel lays its content out between these insets
    /// (LLP 1116.000 D1.2).
    case inset(top: CGFloat, bottom: CGFloat)
    /// Space occupied by native scrollers, measured after tiling.
    case gutter(right: CGFloat, bottom: CGFloat)
    /// Space the native table puts outside this row's rectangle.
    case rowSpacing(top: CGFloat, bottom: CGFloat)
}
