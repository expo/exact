// A layout transition's size (LLP 1063) is the box's surface alone, as a
// UIKit or SwiftUI frame animation shows it: the background, border, corner
// radius, shadow and the clip it puts on its children take the size the
// engine shows, from the box's top-left corner, while the view keeps its
// laid-out frame, so its content and children stay at their final geometry
// and move only by their own transitions. A growing card reveals its title;
// it never squashes it.
//
// While the size differs from the laid-out one the node paints no surface of
// its own (`surface != nil` turns its box, gradient and shadow off); a
// `SurfaceLayer` under everything the node holds paints it at the shown size
// — Core Animation's own properties when it can say the box (one border, one
// radius, no gradient), else drawn, as the node's `draw(_:)` draws it — and a
// node that clips its children clips them to the shown box through a mask.
#if os(iOS) || os(tvOS)
import UIKit
#else
import AppKit
#endif
#if os(macOS)
// AppKit hides a placed canvas child with alpha 0, so its layer's mask is its box's.
private extension CALayer { var boxMask: CALayer? { mask } }
#endif

final class SurfaceLayer: CALayer {
    /// A box Core Animation cannot say, drawn at the layer's size.
    var drawn: ((CGContext, CGRect) -> Void)?
    let caster = ShadowCaster()
    /// The children's clip at the shown size, and the layer it masks.
    let clip = CAShapeLayer()
    let ownClip = CAShapeLayer()
    weak var clipped: CALayer?
    /// The node's own clip, turned off while this one stands in for it.
    var clippedSelf = false
    var madeClipBox = false
    var clippedToBounds = false

    override init() {
        super.init()
        needsDisplayOnBoundsChange = true
        addSublayer(caster)
    }
    override init(layer: Any) { super.init(layer: layer) }
    required init?(coder: NSCoder) { nil }

    override func draw(in ctx: CGContext) {
        // The node's painters draw from the top-left corner.
        if ctx.ctm.d > 0 { ctx.translateBy(x: 0, y: bounds.height); ctx.scaleBy(x: 1, y: -1) }
        drawn?(ctx, bounds)
    }
}

extension NodeView {
    private var hostLayer: CALayer? { layer }

    /// The surface at the laid-out size plus `layoutResize`; none at zero.
    func applySurface() {
        defer { overflowClipGeometryChanged() }
        guard layoutResize != .zero, let host = hostLayer else { return endSurface() }
        let started = surface == nil
        let s = surface ?? SurfaceLayer()
        surface = s
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        if host.sublayers?.first !== s { host.insertSublayer(s, at: 0) }
        // From the size layout gave, not the pixel-grid one: a box closing to zero shows zero.
        let laid = layoutBounds.size
        let box = CGRect(x: 0, y: 0, width: max(0, laid.width + layoutResize.width), height: max(0, laid.height + layoutResize.height))
        if s.frame != box { s.frame = box }
        #if os(iOS) || os(tvOS)
        s.contentsScale = traitCollection.displayScale
        #else
        s.contentsScale = pixelScale ?? 2
        #endif
        paintSurface(s, box)
        clipSurface(s, box)
        if started { surfaceChanged() }
    }

    /// The node paints its own surface again at its laid-out size.
    func endSurface() {
        guard let s = surface else { return }
        surface = nil
        CATransaction.begin(); CATransaction.setDisableActions(true)
        if s.clipped?.boxMask === s.clip { s.clipped?.mask = nil }
        if hostLayer?.boxMask === s.ownClip { hostLayer?.mask = nil }
        s.clipped?.masksToBounds = s.clippedToBounds
        if s.madeClipBox { syncClipBox(false) }
        if s.clippedSelf { clipsToBounds = overflowClips && clipBox == nil }
        s.removeFromSuperlayer()
        CATransaction.commit()
        surfaceChanged()
    }

    private func surfaceChanged() {
        applyShadow(outline: roundedPath(in: bounds).cgPath)
        #if os(iOS) || os(tvOS)
        applyBoxLayer()
        applyGradientLayer()
        setNeedsDisplay()
        #else
        syncEllipticalClip()
        needsDisplay = true
        #endif
    }

    /// The box as the node paints it, at `box`: paint motion's values over
    /// its rows, read afresh on every frame the size moves.
    private func paintSurface(_ s: SurfaceLayer, _ box: CGRect) {
        let sides = ["top", "right", "bottom", "left"]
        let uniform = number("border_width")
        let widths = sides.map { devicePixels(number("border_width_" + $0, uniform)) }
        let styles = sides.map { style["border_style_" + $0]?.string ?? "solid" }
        let top = color("border_color_top", .clear)
        let colors = sides.map { color("border_color_" + $0, top).cgColor }
        let sizes = BorderPaint.radii(style, in: box)
        let radii = sizes.map { $0.width }
        let authored = channels("background_color").map { CGColor(srgbRed: $0[0] / 255, green: $0[1] / 255, blue: $0[2] / 255, alpha: $0[3] / 255) }
        let fill = authored ?? fieldGround
        let gradients = Gradient.layers(style["background_image"], tint: gradientTint)
        let radius = radii.max() ?? 0
        let shape = CornerShape(style["corner_shape"])
        // A background clipped inside the border box is drawn, in the clip `paintBackground` takes (`backgroundOutline`).
        let clipped = backgroundClip != "border-box"
        let said = !clipped && !styles.contains(where: { $0 == "dotted" || $0 == "dashed" }) && gradients.isEmpty && (shape == nil || shape?.isAppleContinuous == true) && sizes.allSatisfy { $0.width == $0.height }
            && widths.allSatisfy { $0 == widths[0] } && (widths[0] == 0 || colors.allSatisfy { $0 == colors[0] })
            && radii.allSatisfy { $0 == 0 || $0 == radius } && radius <= min(box.width, box.height) / 2
        if said {
            var corners: CACornerMask = []
            let masks: [CACornerMask] = [.layerMinXMinYCorner, .layerMaxXMinYCorner, .layerMaxXMaxYCorner, .layerMinXMaxYCorner]
            for (r, mask) in zip(radii, masks) where r > 0 { corners.insert(mask) }
            // The design's Field under a field's own bezel: on the bezel's corners (BoxLayerIOS.swift).
            var radius = radius
            #if os(iOS)
            if authored == nil, fill != nil, radius == 0, !isCellField {
                radius = FieldChromeCache.textareaRadius
                corners = [.layerMinXMinYCorner, .layerMaxXMinYCorner, .layerMaxXMaxYCorner, .layerMinXMaxYCorner]
            }
            #endif
            s.drawn = nil; s.contents = nil
            s.backgroundColor = fill
            s.cornerRadius = radius
            s.cornerCurve = shape == nil ? .circular : .continuous
            if radius > 0 { s.maskedCorners = corners }
            s.borderWidth = widths[0]
            s.borderColor = colors[0]
        } else {
            s.backgroundColor = nil; s.cornerRadius = 0; s.borderWidth = 0
            let dark = drawsDark
            s.drawn = { [weak self] ctx, rect in
                guard let self else { return }
                let path = self.roundedPath(in: rect).cgPath
                if let ground = clipped ? self.backgroundOutline(path, in: rect) : path {
                    if let fill { ctx.addPath(ground); ctx.setFillColor(fill); ctx.fillPath() }
                    for gradient in gradients.reversed() {
                        gradient.paint(ctx, clip: ground, box: rect.insetBy(left: widths[3], top: widths[0], right: widths[1], bottom: widths[2]), dark: dark)
                    }
                }
                BorderPaint.paint(ctx, box: rect, widths: widths, colors: colors, radii: sizes, styles: styles, shape: shape)
            }
            s.setNeedsDisplay()
        }
        if let color = shadowColor {
            let g = shadowGeometry
            s.caster.isHidden = false
            s.caster.cast(box: box, outline: roundedPath(in: box).cgPath, color: color, offset: CGSize(width: g.x, height: g.y), blur: g.blur)
        } else {
            s.caster.isHidden = true
        }
    }

    /// A node that clips its children clips them inside the shown border: its own
    /// clip is lifted (the surface may be larger than its frame as it
    /// shrinks) and a mask stands in; a box of its own that holds them is
    /// masked instead, keeping the node's shadow unclipped.
    private func clipSurface(_ s: SurfaceLayer, _ box: CGRect) {
        if clipsOneAxis {
            if s.clipped?.boxMask === s.clip { s.clipped?.mask = nil }
            s.clipped?.masksToBounds = s.clippedToBounds
            s.clipped = nil
            if hostLayer?.boxMask === s.ownClip { hostLayer?.mask = nil }
            s.madeClipBox = false
            syncClipBox(true)
            clipsToBounds = false
            return
        }
        let uniform = number("border_width")
        let widths = ["top", "right", "bottom", "left"].map { devicePixels(number("border_width_" + $0, uniform)) }
        // A padding-edge mask must exclude the surface itself, or it cuts
        // away the border it is meant to protect from the children.
        if clipBox == nil, scroll == nil, clipsToBounds, widths.contains(where: { $0 > 0 }),
           clipPath == nil, boxFilter == nil, style["mask_image"] == nil {
            s.madeClipBox = true
            syncClipBox(true)
            clipsToBounds = false
            s.clippedSelf = true
        }
        if s.madeClipBox {
            // The node's own paint still needs the outer clip.
            s.ownClip.path = roundedPath(in: box).cgPath
            hostLayer?.mask = s.ownClip
        } else if hostLayer?.boxMask === s.ownClip { hostLayer?.mask = nil }
        let scroller: CALayer? = scroll?.layer
        let target: CALayer?
        if let clipped = clipBox?.layer { target = clipped }
        else if let scroller { target = scroller }
        else if clipsToBounds, (clipPath == nil && boxFilter == nil && style["mask_image"] == nil) || hostLayer?.boxMask === s.clip {
            clipsToBounds = false
            s.clippedSelf = true
            target = hostLayer
        } else if overflowClips, hostLayer?.boxMask === s.clip { target = hostLayer } else { target = nil }
        if s.clipped !== target {
            if s.clipped?.boxMask === s.clip { s.clipped?.mask = nil }
            s.clipped?.masksToBounds = s.clippedToBounds
            s.clipped = target
            s.clippedToBounds = target?.masksToBounds ?? false
            target?.masksToBounds = false
        }
        guard let target else { return }
        clipsToBounds = false
        s.clippedSelf = true
        if target === hostLayer {
            s.clip.path = roundedPath(in: box).cgPath
        } else {
            let inner = CGRect(x: box.minX + widths[3], y: box.minY + widths[0],
                               width: max(0, box.width - widths[1] - widths[3]),
                               height: max(0, box.height - widths[0] - widths[2]))
            let radii = BorderPaint.contentRadii(cornerSizes(in: box), outer: box, inner: inner)
            s.clip.path = BorderPaint.roundedRect(inner, radii, shape: CornerShape(style["corner_shape"]))
        }
        // In the masked layer's own coordinates: a scroll view's are scrolled.
        s.clip.frame = CGRect(origin: target === hostLayer ? .zero : CGPoint(x: -target.frame.minX + target.bounds.minX, y: -target.frame.minY + target.bounds.minY), size: bounds.size)
        if target.boxMask !== s.clip { target.mask = s.clip }
        s.clipped = target
    }
}
