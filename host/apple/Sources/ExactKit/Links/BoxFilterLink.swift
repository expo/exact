// @ref LLP 1047.001 D10 — CSS `filter` on a box is a family the composition
// selects (the roster's `Effects`): a box's picture, the presenter's
// filtered boxes, and the filter engine (`FilterExtent`, `FilterProgram`,
// `SvgFilterGPU`), which SVG's filters use too, so the SVG product depends on
// this one. The core reaches it only through what this file declares.
// `ExactEffects.install()` sets its slots; without it a box's `filter`
// draws nothing, and a plan with one is refused at boot
// (`Unlinked("effects")`).
// `clip-path` stays in the core: its parser is small and its mask is the
// core's rounded-outline mask (`ClipPath.mask`).
import CoreGraphics
import QuartzCore

/// What the core asks of one box's CSS `filter`.
package protocol BoxFilterEffect: AnyObject {
    /// The layer standing where the box stands, holding the filtered picture.
    var picture: CALayer { get }
    /// The empty mask that hides the box behind its picture (AppKit).
    var hide: CALayer { get }
    /// The factor of a lone `opacity()` the layer plays itself, if that is all the chain is.
    var opacity: Float? { get }
    /// The box's own opacity while a lone `opacity()` fades it.
    var unfilteredOpacity: Float? { get }
    /// Whether the chain is a lone `drop-shadow()` the layer casts itself.
    var casts: Bool { get }
    /// Whether a picture is being drawn.
    var drawing: Bool { get }
    /// Whether the box shows as a picture, not as a layer effect.
    var snapshots: Bool { get }
    /// The `filter` value; whether the box has a chain to draw.
    func set(_ value: BatchValue?) -> Bool
    /// The box's opacity, which a lone `opacity()` multiplies.
    func presentOpacity(_ value: Float, on box: CALayer)
    /// Draw `box`'s picture. `node`: the box's node, a new one in a reused
    /// view. `shown`: whether the box is on screen, where its first picture
    /// is waited for. `landed`: new filtered pixels or a layer effect showed.
    func render(_ box: CALayer, clip: CALayer?, scale: CGFloat, dark: Bool, node: UInt32, fresh: Bool, shown: Bool, landed: @escaping () -> Void)
    /// The box no longer has a filter, or left the tree.
    func remove()
}

/// What the core asks of a presenter's filtered boxes: which to draw again
/// after a batch, and after pixels under them were published.
package protocol BoxFilterSet: AnyObject {
    var isEmpty: Bool { get }
    /// Whether a picture is queued or being drawn.
    var drawing: Bool { get }
    func add(_ view: NodeView, render: @escaping (_ fresh: Bool) -> Void)
    func remove(_ view: NodeView)
    /// A batch touched `view` or a box in it.
    func touched(_ view: PlatformView)
    /// Pixels under `view` were published between batches.
    func changed(_ view: PlatformView)
    /// `view`'s picture showed.
    func landed(_ view: NodeView)
    /// The batch ended: draw what it touched.
    func render()
}

/// The slots are set before any session is made, and read on the main thread.
package enum BoxFilterLink {
    /// One box's filter.
    package static var make: (() -> any BoxFilterEffect)?
    /// A presenter's filtered boxes.
    package static var makeSet: (() -> any BoxFilterSet)?
    /// Make the GPU filter pipelines now, off the main thread, for a plan
    /// that can show a filter, before its first picture.
    package static var prewarm: (() -> Void)?
}
