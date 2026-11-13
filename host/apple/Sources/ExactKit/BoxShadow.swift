// CSS `box-shadow` on both Apple platforms (LLP 1064 D2, LLP 1077 D4): a
// list of outer and inset shadows with spread, the first on top. An outer
// shadow is a layer with no contents casting the border box's outline,
// grown by the spread, through `shadowPath` (no offscreen pass to find a
// shape); they share one container masked to what lies outside the border
// box, since CSS clips an outer shadow there, so it never shows through a
// translucent background, and a lone one is the container's own. The container is the node's lowest sublayer,
// under its children, as CSS paints it. An inset shadow casts a frame
// whose hole is the padding box shrunk by the spread, inward; they share a
// container masked to the padding box, over the background and under the
// border and children. A node that clips its overflow clips its children
// in a box of their own instead (`clipBox`), since clipping the node's
// layer would clip the shadow too. Core Animation's `shadowRadius` is a
// Gaussian's standard deviation; CSS's blur radius is twice that.
import CoreGraphics
import QuartzCore
#if os(iOS) || os(tvOS)
import UIKit
#else
import AppKit
#endif

/// One shadow of a `box-shadow` list, resolved for an appearance.
package struct BoxShadowSpec: Equatable {
    package var color: CGColor
    package var offset: CGSize
    package var blur: CGFloat
    package var spread: CGFloat
    package var inset: Bool

    /// The row's list (`style.rs`), the colours for `dark`; transparent
    /// shadows left out.
    static func list(_ value: BatchValue?, dark: Bool, contrast: Bool? = nil, elevated: Bool = false, tint: PlatformColor? = nil) -> [BoxShadowSpec] {
        (value?.array ?? []).compactMap { item in
            guard case .object(let o) = item, let off = o["o"]?.numbers, off.count == 2 else { return nil }
            guard let color = o["c"]?.cgColor(dark: dark, contrast: contrast, elevated: elevated, tint: tint), color.alpha > 0 else { return nil }
            return BoxShadowSpec(color: color,
                                 offset: CGSize(width: off[0], height: off[1]), blur: max(0, CGFloat(o["b"]?.number ?? 0)),
                                 spread: CGFloat(o["s"]?.number ?? 0), inset: o["i"] != nil)
        }
    }

    /// How far past the shape it reaches: three deviations, the offset and
    /// the spread.
    package var reach: CGFloat { 1.5 * blur + max(abs(offset.width), abs(offset.height)) + abs(spread) + 1 }
}

/// The outer shadows: a container masked to outside the border box, one
/// casting sublayer per shadow.
package final class ShadowCaster: CALayer {
    private let outside = CAShapeLayer()

    package override init() {
        super.init()
        outside.fillRule = .evenOdd
        mask = outside
    }
    override init(layer: Any) { super.init(layer: layer) }
    required init?(coder: NSCoder) { nil }

    /// One outer shadow (the GPU capture and the tests read the first).
    func cast(box: CGRect, outline: CGPath, color: CGColor, offset: CGSize, blur: CGFloat) {
        cast(box: box, outline: outline, shadows: [(outline, BoxShadowSpec(color: color, offset: offset, blur: blur, spread: 0, inset: false))])
    }

    /// `box` and the paths in the host layer's coordinates: `outline` the
    /// border box's, each shadow's shape its outline grown by its spread.
    package func cast(box: CGRect, outline: CGPath, shadows: [(shape: CGPath, spec: BoxShadowSpec)]) {
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        let reach = shadows.map(\.spec.reach).max() ?? 1
        let extent = box.insetBy(dx: -reach, dy: -reach)
        if frame != extent { frame = extent }
        var local = CGAffineTransform(translationX: -extent.minX, y: -extent.minY)
        func cast(_ layer: CALayer, _ shape: CGPath, _ spec: BoxShadowSpec) {
            layer.shadowPath = shape.copy(using: &local)
            if layer.shadowColor != spec.color { layer.shadowColor = spec.color }
            if layer.shadowOffset != spec.offset { layer.shadowOffset = spec.offset }
            if layer.shadowRadius != spec.blur / 2 { layer.shadowRadius = spec.blur / 2 }
            if layer.shadowOpacity != 1 { layer.shadowOpacity = 1 }
        }
        // One shadow (most boxes) this layer casts itself, its mask clipping
        // its own shadow as it would a sublayer's: a layer fewer a box.
        if shadows.count == 1 {
            for layer in sublayers ?? [] { layer.removeFromSuperlayer() }
            cast(self, shadows[0].shape, shadows[0].spec)
        } else {
            if shadowOpacity != 0 { shadowOpacity = 0; shadowPath = nil }
            // The first shadow on top: the last sublayer.
            var casts = sublayers ?? []
            while casts.count > shadows.count { casts.removeLast().removeFromSuperlayer() }
            while casts.count < shadows.count { let l = CALayer(); addSublayer(l); casts.append(l) }
            for (layer, (shape, spec)) in zip(casts, shadows.reversed()) {
                if layer.frame != bounds { layer.frame = bounds }
                cast(layer, shape, spec)
            }
        }
        let hole = CGMutablePath()
        hole.addRect(CGRect(origin: .zero, size: extent.size))
        if let path = outline.copy(using: &local) { hole.addPath(path) }
        outside.frame = CGRect(origin: .zero, size: extent.size)
        outside.path = hole
    }

    /// The first shadow's colour, as a single caster's was (tests, captures).
    var firstColor: CGColor? { shadowOpacity > 0 ? shadowColor : sublayers?.last?.shadowColor }
}

/// The inset shadows: a container masked to the padding box, one sublayer
/// per shadow casting a frame inward (LLP 1077 D4).
package final class InsetShadowCaster: CALayer {
    private let inside = CAShapeLayer()

    package override init() {
        super.init()
        mask = inside
    }
    override init(layer: Any) { super.init(layer: layer) }
    required init?(coder: NSCoder) { nil }

    /// `padding` the padding box's outline; each shadow's hole the padding
    /// box shrunk by its spread, all in the host layer's coordinates.
    package func cast(box: CGRect, padding: CGPath, shadows: [(hole: CGPath, spec: BoxShadowSpec)]) {
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        if frame != box { frame = box }
        var local = CGAffineTransform(translationX: -box.minX, y: -box.minY)
        inside.frame = bounds
        inside.path = padding.copy(using: &local)
        var casts = sublayers ?? []
        while casts.count > shadows.count { casts.removeLast().removeFromSuperlayer() }
        while casts.count < shadows.count { let l = CALayer(); addSublayer(l); casts.append(l) }
        for (layer, (hole, spec)) in zip(casts, shadows.reversed()) {
            if layer.frame != bounds { layer.frame = bounds }
            // A frame around the hole, far enough out that its blur never
            // shows an outer edge; Core Animation fills a shadow path by the
            // non-zero rule, so the hole runs the other way round.
            let frame = CGMutablePath()
            frame.addRect(bounds.insetBy(dx: -spec.reach - bounds.width, dy: -spec.reach - bounds.height))
            if let h = hole.copy(using: &local) { frame.addPath(BoxShadowSpec.reversed(h)) }
            layer.shadowPath = frame
            if layer.shadowColor != spec.color { layer.shadowColor = spec.color }
            if layer.shadowOffset != spec.offset { layer.shadowOffset = spec.offset }
            if layer.shadowRadius != spec.blur / 2 { layer.shadowRadius = spec.blur / 2 }
            if layer.shadowOpacity != 1 { layer.shadowOpacity = 1 }
        }
    }
}

extension BoxShadowSpec {
    /// A closed path of lines and cubics, traced the other way round.
    static func reversed(_ path: CGPath) -> CGPath {
        // Each segment as its start, controls and end.
        var segments: [[CGPoint]] = [], start = CGPoint.zero, current = CGPoint.zero
        path.applyWithBlock { e in
            let p = e.pointee.points
            switch e.pointee.type {
            case .moveToPoint: start = p[0]; current = p[0]
            case .addLineToPoint: segments.append([current, p[0]]); current = p[0]
            case .addQuadCurveToPoint: segments.append([current, p[0], p[1]]); current = p[1]
            case .addCurveToPoint: segments.append([current, p[0], p[1], p[2]]); current = p[2]
            case .closeSubpath: if current != start { segments.append([current, start]) }; current = start
            @unknown default: break
            }
        }
        let out = CGMutablePath()
        guard let last = segments.last?.last else { return out }
        out.move(to: last)
        for s in segments.reversed() {
            switch s.count {
            case 2: out.addLine(to: s[0])
            case 3: out.addQuadCurve(to: s[0], control: s[1])
            default: out.addCurve(to: s[0], control1: s[2], control2: s[1])
            }
        }
        out.closeSubpath()
        return out
    }

    /// A corner radius grown by a spread (CSS Backgrounds 3 §7.1.1): a square
    /// corner stays square, and a radius smaller than the spread grows less.
    static func spreadRadius(_ r: CGFloat, _ spread: CGFloat) -> CGFloat {
        if r <= 0 { return 0 }
        if spread < 0 { return max(0, r + spread) }
        let ratio = r / spread
        return r + spread * (ratio >= 1 ? 1 : 1 + pow(ratio - 1, 3))
    }
}

extension NodeView {
    /// The section owns hover and focus; its card owns their shadow paint.
    var groupedShadowCard: NodeView? {
        guard props["semanticTag"] == "section", nodeSuperview?.props["listStyle"] != nil else { return nil }
        return container.subviews.compactMap { $0 as? NodeView }.first {
            !["header", "footer"].contains($0.props["semanticTag"] ?? "")
        }
    }

    /// The `box-shadow` list resolved for the view's appearance; a popover's
    /// default surface's shadow where its author wrote none (`defaultSurface`).
    package var boxShadows: [BoxShadowSpec] {
        if groupedShadowCard != nil { return [] }
        var value = style["box_shadow"]
        if let section = nodeSuperview, section.groupedShadowCard === self {
            for (state, on) in [("hover", section.pressPaint.hovered || section.holds("hover")),
                                ("focus_visible", section.pressPaint.focusShown || section.holds("focus-visible"))] where on && !section.disabled {
                if let shadow = Self.look(state, in: section.style)?["box_shadow"] { value = shadow }
            }
        }
        let list = BoxShadowSpec.list(value, dark: drawsDark, contrast: drawsHighContrast, elevated: drawsElevated, tint: ownTint(for: value))
        return list.isEmpty ? defaultSurfaceShadow.map { [$0] } ?? [] : list
    }

    /// A projected card uses the node's corners at its native size.
    package func boxShadowShape(in box: CGRect, spread: CGFloat = 0, inset: Bool = false) -> CGPath {
        let sizes = BorderPaint.reduced(BorderPaint.radii(style, in: box), in: box)
        let grown = box.insetBy(dx: inset ? spread : -spread, dy: inset ? spread : -spread)
        guard grown.width > 0, grown.height > 0 else { return CGMutablePath() }
        let radius = { (r: CGFloat) in inset ? max(0, r - spread) : BoxShadowSpec.spreadRadius(r, spread) }
        let radii = sizes.map { CGSize(width: radius($0.width), height: radius($0.height)) }
        return BorderPaint.roundedRect(grown, radii, shape: CornerShape(style["corner_shape"]))
    }

    /// The first outer shadow's colour; nil when there is none.
    var shadowColor: CGColor? { boxShadows.first { !$0.inset }?.color }

    /// The first outer shadow's offset and CSS blur radius.
    var shadowGeometry: (x: CGFloat, y: CGFloat, blur: CGFloat) {
        guard let s = boxShadows.first(where: { !$0.inset }) else { return (0, 0, 0) }
        return (s.offset.width, s.offset.height, s.blur)
    }

    /// The casters onto the layer: the outer ones at its bottom, cast from
    /// the border box; the inset ones over the box's paint; or gone.
    func applyShadow(outline: CGPath) {
        if let card = groupedShadowCard {
            card.syncShadowClip()
            #if os(macOS)
            card.applyClipRadius()
            card.applyShadow()
            #else
            card.applyBoxLayer()
            #endif
        }
        if cssVisibilityHidden {
            shadowCaster?.removeFromSuperlayer(); shadowCaster = nil
            insetCaster?.removeFromSuperlayer(); insetCaster = nil
            #if os(macOS)
            removeShadowStand()
            #endif
            return
        }
        #if os(iOS) || os(tvOS)
        let host: CALayer? = layer
        #else
        let host = layer
        #endif
        let all = surface == nil && bounds.width > 0 && bounds.height > 0 ? boxShadows : []
        var sizes = BorderPaint.reduced(BorderPaint.radii(style, in: bounds), in: bounds)
        // A default surface's corner where its author wrote none, its shadow cast from it.
        var outline = outline
        if let r = defaultSurfaceRadius, !writesCorner {
            sizes = BorderPaint.reduced(Array(repeating: CGSize(width: r, height: r), count: 4), in: bounds)
            outline = BorderPaint.roundedRect(bounds, sizes, shape: CornerShape(nil))
        }
        let shape = CornerShape(style["corner_shape"])
        let outer = all.filter { !$0.inset }
        #if os(iOS) || os(tvOS)
        let outerHost = outer.isEmpty ? nil : host
        #else
        // A backdrop's layer masks: they cast beside it (`BackdropShadowMac.swift`).
        let outerHost = outer.isEmpty ? nil : outerShadowHost()
        #endif
        if let outerHost {
            let caster = shadowCaster ?? ShadowCaster()
            if caster.superlayer !== outerHost { outerHost.insertSublayer(caster, at: 0) }
            shadowCaster = caster
            caster.cast(box: bounds, outline: outline, shadows: outer.map { spec in
                guard spec.spread != 0 else { return (outline, spec) }
                let grown = bounds.insetBy(dx: -spec.spread, dy: -spec.spread)
                let radii = sizes.map { CGSize(width: BoxShadowSpec.spreadRadius($0.width, spec.spread),
                                                height: BoxShadowSpec.spreadRadius($0.height, spec.spread)) }
                return (BorderPaint.roundedRect(grown, radii, shape: shape), spec)
            })
        } else {
            shadowCaster?.removeFromSuperlayer(); shadowCaster = nil
            #if os(macOS)
            removeShadowStand()
            #endif
        }
        let inner = all.filter(\.inset)
        guard let host, !inner.isEmpty else { insetCaster?.removeFromSuperlayer(); insetCaster = nil; return }
        let uniform = number("border_width")
        let w = ["top", "right", "bottom", "left"].map { number("border_width_" + $0, uniform) }
        let padding = CGRect(x: w[3], y: w[0], width: max(0, bounds.width - w[1] - w[3]), height: max(0, bounds.height - w[0] - w[2]))
        let padRadii = [CGSize(width: sizes[0].width - w[3], height: sizes[0].height - w[0]),
                        CGSize(width: sizes[1].width - w[1], height: sizes[1].height - w[0]),
                        CGSize(width: sizes[2].width - w[1], height: sizes[2].height - w[2]),
                        CGSize(width: sizes[3].width - w[3], height: sizes[3].height - w[2])]
            .map { CGSize(width: max(0, $0.width), height: max(0, $0.height)) }
        let caster = insetCaster ?? InsetShadowCaster()
        insetCaster = caster
        if caster.superlayer !== host { insertInsetCaster(caster, into: host) }
        caster.cast(box: bounds, padding: BorderPaint.roundedRect(padding, padRadii, shape: shape), shadows: inner.map { spec in
            let hole = padding.insetBy(dx: spec.spread, dy: spec.spread)
            let radii = padRadii.map { CGSize(width: max(0, $0.width - spec.spread), height: max(0, $0.height - spec.spread)) }
            return (hole.width > 0 && hole.height > 0 ? BorderPaint.roundedRect(hole, radii, shape: shape) : CGMutablePath(), spec)
        })
    }

    /// The inset casters over the box's background and gradient, under its
    /// border, image, text and children.
    private func insertInsetCaster(_ caster: CALayer, into host: CALayer) {
        if host === layer { insertBoxSublayer(caster) } else { host.insertSublayer(caster, at: 0) }
    }
}

extension NodeView {
    #if os(iOS) || os(tvOS)
    private typealias ClipBox = PlainView
    #else
    /// The shadow at the node's current size.
    func applyShadow() {
        applyShadow(outline: roundedPath(in: bounds).cgPath)
    }

    /// A capture (`cacheDisplay`, the agent's screenshot and a canvas's
    /// surface) draws views, not sublayers: the shadow drawn as the caster
    /// shows it, before the box. The casting shape is drawn far off to the
    /// left and only its shadow lands here (`castShadow`), so a spread
    /// ring is the shadow's colour, as the live caster's is.
    func drawCapturedShadow(_ ctx: CGContext) {
        guard Capture.capturing, shadowCaster != nil || insetCaster != nil else { return }
        let outline = roundedPath(in: bounds).cgPath
        let sizes = BorderPaint.reduced(BorderPaint.radii(style, in: bounds), in: bounds)
        let shape = CornerShape(style["corner_shape"])
        // The last first, so the first is on top, as the live casters stack.
        for spec in boxShadows.reversed() where !spec.inset {
            let (x, y, blur) = (spec.offset.width, spec.offset.height, spec.blur)
            let grown = bounds.insetBy(dx: -spec.spread, dy: -spec.spread)
            let radii = sizes.map { CGSize(width: BoxShadowSpec.spreadRadius($0.width, spec.spread),
                                            height: BoxShadowSpec.spreadRadius($0.height, spec.spread)) }
            let cast = spec.spread == 0 ? outline : BorderPaint.roundedRect(grown, radii, shape: shape)
            let outside = CGMutablePath()
            outside.addRect(bounds.insetBy(dx: -spec.reach, dy: -spec.reach))
            outside.addPath(outline)
            ctx.saveGState()
            ctx.addPath(outside)
            ctx.clip(using: .evenOdd)
            ctx.castShadow(cast, offset: CGSize(width: x, height: y), blur: blur, color: spec.color, rule: .winding)
            ctx.restoreGState()
        }
    }

    /// The inset shadows into a capture, over the box's paint (LLP 1077 D4):
    /// a frame around the shrunk padding box casts inward, clipped to it.
    func drawCapturedInsetShadow(_ ctx: CGContext) {
        guard Capture.capturing, insetCaster != nil else { return }
        let uniform = number("border_width")
        let w = ["top", "right", "bottom", "left"].map { number("border_width_" + $0, uniform) }
        let padding = CGRect(x: w[3], y: w[0], width: max(0, bounds.width - w[1] - w[3]), height: max(0, bounds.height - w[0] - w[2]))
        let sizes = BorderPaint.reduced(BorderPaint.radii(style, in: bounds), in: bounds)
        let shape = CornerShape(style["corner_shape"])
        let padRadii = [CGSize(width: sizes[0].width - w[3], height: sizes[0].height - w[0]),
                        CGSize(width: sizes[1].width - w[1], height: sizes[1].height - w[0]),
                        CGSize(width: sizes[2].width - w[1], height: sizes[2].height - w[2]),
                        CGSize(width: sizes[3].width - w[3], height: sizes[3].height - w[2])]
            .map { CGSize(width: max(0, $0.width), height: max(0, $0.height)) }
        let clip = BorderPaint.roundedRect(padding, padRadii, shape: shape)
        for spec in boxShadows.reversed() where spec.inset {
            let hole = padding.insetBy(dx: spec.spread, dy: spec.spread)
            let frame = CGMutablePath()
            frame.addRect(padding.insetBy(dx: -spec.reach - padding.width, dy: -spec.reach - padding.height))
            if hole.width > 0, hole.height > 0 {
                let radii = padRadii.map { CGSize(width: max(0, $0.width - spec.spread), height: max(0, $0.height - spec.spread)) }
                frame.addPath(BorderPaint.roundedRect(hole, radii, shape: shape))
            }
            ctx.saveGState()
            ctx.addPath(clip)
            ctx.clip()
            ctx.castShadow(frame, offset: spec.offset, blur: spec.blur, color: spec.color, rule: .evenOdd)
            ctx.restoreGState()
        }
    }

    /// Flipped, and never a hit target of its own.
    private final class ClipBox: NSView {
        override var isFlipped: Bool { true }
        override func hitTest(_ point: NSPoint) -> NSView? {
            let hit = raisedHit(super.hitTest(point), point)
            return hit === self ? nil : hit
        }
    }
    #endif

    /// Whether the overflow clips one axis and leaves the other visible
    /// (`clip` beside `visible`, CSS Overflow 3 §3.1). The children's clip
    /// box then reaches `ClipReach.far` past the node on the visible axis; a
    /// node that can have no clip box (an overlay, a material) clips both.
    var clipsOneAxis: Bool {
        guard kind != "text" else { return false }
        let pair = [style["overflow_x"]?.string ?? "visible", style["overflow_y"]?.string ?? "visible"]
        return pair == ["clip", "visible"] || pair == ["visible", "clip"]
    }

    /// Keep overflow clipped on the children while an outer shadow reaches past the card.
    func syncShadowClip() {
        #if os(macOS)
        let canHoldChildren = materialContent == nil
        #else
        let canHoldChildren = materialKind != "glass"
        #endif
        syncClipBox(overflowClips && kind != "text" && (shadowColor != nil || clipsOneAxis)
                    && scroll == nil && overlay == nil && canHoldChildren)
        let clipped = overflowClips && clipBox == nil
        if clipsToBounds != clipped { clipsToBounds = clipped }
    }

    /// The children into a clipping box of their own, or back out of it, as
    /// a scroll view takes them.
    func syncClipBox(_ wanted: Bool) {
        let scrolls = ["overflow_x", "overflow_y"].contains { ["auto", "scroll"].contains(style[$0]?.string ?? "") }
        if scrolls { surface?.madeClipBox = false }
        let wanted = wanted || surface?.madeClipBox == true
        #if os(iOS) || os(tvOS)
        let previous = clipBox
        #endif
        defer {
            placeClipBox()
            #if os(iOS) || os(tvOS)
            if previous !== clipBox { presenter?.flats.containerChanged(id) }
            #endif
        }
        if wanted, clipBox == nil {
            let box = ClipBox(frame: bounds)
            #if os(iOS) || os(tvOS)
            box.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            #else
            box.autoresizingMask = [.width, .height]
            box.wantsLayer = true
            #endif
            box.clipsToBounds = true
            GlassGroups.moving(in: self) {
                // The box is in the window before the children move into it,
                // so they never leave it: UIKit cancels the touches of a
                // view that does, and AppKit its first responder.
                let children = subviews.filter {
                    #if os(iOS)
                    $0 is NodeView || $0 === glassGroupView || $0 === textArea || $0 === symbolView || $0 === field || $0 === searchChrome || $0 === stepper
                    #elseif os(tvOS)
                    $0 is NodeView || $0 === glassGroupView || $0 === textArea || $0 === symbolView || $0 === field
                    #else
                    $0 is NodeView || $0 === glassGroupView || $0 === textAreaScroll || $0 === symbolClip || $0 === field || $0 === stepper
                    #endif
                }
                addSubview(box)
                for child in children { box.addSubview(child) }
            }
            clipBox = box
        } else if !wanted, let box = clipBox {
            GlassGroups.moving(in: self) {
                for child in box.subviews { addSubview(child) }
                box.removeFromSuperview()
            }
            clipBox = nil
        }
    }

    /// The clip box over the node's bounds, or for a one-axis clip that far
    /// past them on the visible axis, with its bounds' origin where its
    /// frame's is so the children keep the node's coordinates. Autoresizing
    /// keeps the margins as the node resizes.
    private func placeClipBox() {
        guard let box = clipBox else { return }
        let far = clipsOneAxis ? ClipReach.far : 0
        let x = (style["overflow_x"]?.string ?? "visible") == "visible" ? far : 0
        let y = (style["overflow_y"]?.string ?? "visible") == "visible" ? far : 0
        let reach = bounds.insetBy(dx: -x, dy: -y)
        guard box.frame != reach || box.bounds.origin != reach.origin else { return }
        box.frame = reach
        #if os(iOS) || os(tvOS)
        box.bounds.origin = reach.origin
        #else
        box.setBoundsOrigin(reach.origin)
        #endif
    }
}

/// How far a one-axis clip's box reaches past the node on its visible axis;
/// overflow beyond it is clipped. A power of two, so that positions in the
/// box stay exact in single precision.
enum ClipReach {
    static let far: CGFloat = 65_536
}

#if os(macOS)
extension Capture {
    /// A capture renders a box's sublayers over what its `draw(_:)` paints,
    /// so a box with an inset shadow — drawn in `draw(_:)` over its fill —
    /// has its fill and gradient sublayers hidden for the capture (its
    /// `draw(_:)` paints both), and so does an image whose pixels are a
    /// sublayer: while capturing, `draw(_:)` paints its bitmap (sRGB, as the
    /// shot is), and a translucent image composited twice comes out darker.
    /// Its fill and gradient sublayers go too, as `draw(_:)` paints them
    /// under the bitmap; left showing, they covered it (podcast F7: a
    /// rounded artwork with a placeholder colour shot as the colour alone).
    /// `restore` shows them again.
    static func hideBoxFills(in root: NSView) -> [CALayer] {
        var out: [CALayer] = []
        func walk(_ v: NSView) {
            if let n = v as? NodeView, n.insetCaster != nil || n.capturesPixels {
                for l in [n.boxFill, n.boxGradient, n.capturesPixels ? n.imageLayer : nil].compactMap({ $0 }) where !l.isHidden {
                    l.isHidden = true
                    out.append(l)
                }
            }
            v.subviews.forEach(walk)
        }
        CATransaction.begin(); CATransaction.setDisableActions(true)
        walk(root)
        CATransaction.commit()
        return out
    }

    static func restore(_ layers: [CALayer]) {
        CATransaction.begin(); CATransaction.setDisableActions(true)
        layers.forEach { $0.isHidden = false }
        CATransaction.commit()
    }
}

extension CGContext {
    /// Only the shadow of `path`, at CSS's offset and blur: the shape is
    /// filled far off to the left (outside any clip a caller set) and its
    /// shadow is offset back. A capture's context takes a shadow's offset
    /// and blur in its unflipped base space, in points, so `away` cancels.
    func castShadow(_ path: CGPath, offset: CGSize, blur: CGFloat, color: CGColor, rule: CGPathFillRule) {
        let box = path.boundingBoxOfPath
        let away = 2 * (box.width + box.height) + 4 * blur + 1000
        setShadow(offset: CGSize(width: offset.width + away, height: -offset.height), blur: blur, color: color)
        var shift = CGAffineTransform(translationX: -away, y: 0)
        if let far = path.copy(using: &shift) { addPath(far) }
        setFillColor(CGColor(gray: 0, alpha: 1))
        fillPath(using: rule)
    }
}
#endif
