// A grouped Liquid Glass on macOS takes its box's 2D transform by geometry.
// Inside an `NSGlassEffectContainerView` the glass is not drawn by the glass
// view's layer: the container draws every member's shape itself, placed by
// AppKit's geometry (the member's frame, through each bounds, into the
// container), so a layer transform on the box or an ancestor moves its
// content and leaves the glass where layout put it. A presented translate,
// scale or layout transition offset (LLP 1063) would snap the glass to its
// end while its content moved, and a box shrunk to nothing would still show
// its glass at full size.
//
// So such a glass is framed where the transform draws the box, and its
// content view keeps the box as its bounds, so AppKit scales the content the
// same: the node's layer then carries none of it. A press still scales the
// layer, conjugated by the geometry so it scales about the same origin. The
// glass view sizes its content view to its frame, never to its bounds, so
// the scale sits on the content view, not on the glass (`FrameScaleMac.swift`
// holds an iframe's the same way).
//
// Its translate and scale animations play on a follower layer the node
// holds (`GlassFollower`), sampled each frame while one runs, so the glass is
// framed where the presented transform draws the box.
//
// Only a glass whose node holds its children in it, turned in no direction,
// not mirrored (view geometry cannot draw a negative scale), not flying and
// with no shadow (the node's layer draws one) goes this way; any other keeps
// the layer transform. An ancestor's transform between the glass and its
// group is not seen.
#if os(macOS)
import AppKit

/// Where a grouped glass's translate and scale animations play
/// (`BoxAnimating.setTransformProxy`): a layer that draws nothing, held by
/// the node's layer so Core Animation presents it, and read each frame while
/// one runs, so the glass's geometry follows the presented transform.
///
/// Its display link retains it, so the link stops when no animation moves,
/// when it leaves the node's layer and when the node leaves its window or
/// goes; the node's `viewDidMoveToWindow` starts it again.
final class GlassFollower: CALayer {
    weak var node: NodeView?
    private var link: CADisplayLink?
    private var modelTranslate = CGPoint.zero
    private var modelScale = CGPoint(x: 1, y: 1)

    override func action(forKey event: String) -> (any CAAction)? { nil }
    override func add(_ anim: CAAnimation, forKey key: String?) { super.add(anim, forKey: key); follow() }
    override func removeAnimation(forKey key: String) { super.removeAnimation(forKey: key); follow() }
    override func removeAllAnimations() { super.removeAllAnimations(); follow() }
    override func removeFromSuperlayer() { stop(); super.removeFromSuperlayer() }

    /// The presented translate and scale, or nil where no animation shows,
    /// so an ended one leaves the model's, with no offset left over.
    /// Held animations (`speed == 0`, e.g. agent clock seeks) are evaluated
    /// synchronously from the track rather than `presentation()`, which reflects
    /// the previously committed frame until the next render pass.
    var presented: (translate: CGPoint, scale: CGPoint)? {
        guard showsAnimation else { return nil }
        let now = convertTime(CACurrentMediaTime(), from: nil)
        let shown = presentation()
        var tx = modelTranslate.x, ty = modelTranslate.y, sx = modelScale.x, sy = modelScale.y
        var any = false
        for anim in animations {
            guard let keyPath = (anim as? CAPropertyAnimation)?.keyPath else { continue }
            let value: CGFloat?
            if anim.speed == 0 {
                value = Self.sample(anim, at: anim.timeOffset)
            } else {
                let end = Self.end(of: anim)
                if now < anim.beginTime && anim.fillMode != .backwards && anim.fillMode != .both { continue }
                if now >= end && anim.fillMode != .forwards && anim.fillMode != .both { continue }
                value = (shown?.value(forKeyPath: keyPath) as? NSNumber).map { CGFloat($0.doubleValue) }
                    ?? Self.sample(anim, at: now - anim.beginTime)
            }
            guard let value else { continue }
            any = true
            switch keyPath {
            case "transform.translation.x": tx = value
            case "transform.translation.y": ty = value
            case "transform.scale.x": sx = value
            case "transform.scale.y": sy = value
            default: break
            }
        }
        return any ? (CGPoint(x: tx, y: ty), CGPoint(x: sx, y: sy)) : nil
    }

    /// Evaluates a scalar keyframe or basic animation track at `localTime` seconds.
    private static func sample(_ anim: CAAnimation, at localTime: CFTimeInterval) -> CGFloat? {
        let duration = anim.duration
        let repeatCount = anim.repeatCount == 0 ? 1.0 : Double(anim.repeatCount)
        let total = repeatCount.isInfinite ? Double.infinity : duration * repeatCount
        let progress: Double
        if localTime < 0 {
            guard anim.fillMode == .backwards || anim.fillMode == .both else { return nil }
            progress = 0
        } else if localTime >= total {
            guard anim.fillMode == .forwards || anim.fillMode == .both else { return nil }
            progress = anim.autoreverses && !repeatCount.isInfinite && Int(repeatCount) % 2 == 0 ? 0 : 1
        } else if duration <= 0 {
            progress = 1
        } else {
            let cycles = localTime / duration
            let index = floor(cycles)
            var frac = cycles - index
            if frac == 0 && localTime > 0 && localTime >= total - 1e-9 { frac = 1 }
            progress = anim.autoreverses && Int(index) % 2 == 1 ? 1 - frac : frac
        }
        if let kf = anim as? CAKeyframeAnimation, let values = kf.values as? [NSNumber], !values.isEmpty {
            if values.count == 1 { return CGFloat(values[0].doubleValue) }
            let times = kf.keyTimes?.map(\.doubleValue) ?? (0..<values.count).map { Double($0) / Double(values.count - 1) }
            guard times.count == values.count else { return nil }
            if progress <= times[0] { return CGFloat(values[0].doubleValue) }
            if progress >= times[times.count - 1] { return CGFloat(values[values.count - 1].doubleValue) }
            for i in 0..<(times.count - 1) where progress <= times[i + 1] {
                let span = times[i + 1] - times[i]
                var u = span > 0 ? (progress - times[i]) / span : 1
                if let fns = kf.timingFunctions, i < fns.count { u = ease(fns[i], at: u) }
                let v0 = values[i].doubleValue, v1 = values[i + 1].doubleValue
                return CGFloat(v0 + (v1 - v0) * u)
            }
            return CGFloat(values[values.count - 1].doubleValue)
        }
        if let basic = anim as? CABasicAnimation, let from = (basic.fromValue as? NSNumber)?.doubleValue,
           let to = (basic.toValue as? NSNumber)?.doubleValue {
            let u = basic.timingFunction.map { ease($0, at: progress) } ?? progress
            return CGFloat(from + (to - from) * u)
        }
        return nil
    }

    private static func ease(_ fn: CAMediaTimingFunction, at x: Double) -> Double {
        if x <= 0 { return 0 }
        if x >= 1 { return 1 }
        var c1: [Float] = [0, 0], c2: [Float] = [0, 0]
        fn.getControlPoint(at: 1, values: &c1)
        fn.getControlPoint(at: 2, values: &c2)
        let x1 = Double(c1[0]), y1 = Double(c1[1]), x2 = Double(c2[0]), y2 = Double(c2[1])
        if x1 == y1 && x2 == y2 { return x }
        let cx = 3 * x1, bx = 3 * (x2 - x1) - cx, ax = 1 - cx - bx
        let cy = 3 * y1, by = 3 * (y2 - y1) - cy, ay = 1 - cy - by
        var lo = 0.0, hi = 1.0, t = x
        for _ in 0..<12 {
            let bxT = ((ax * t + bx) * t + cx) * t - x
            if abs(bxT) < 1e-7 { break }
            if bxT < 0 { lo = t } else { hi = t }
            let d = (3 * ax * t + 2 * bx) * t + cx
            let next = abs(d) > 1e-6 ? t - bxT / d : 0.5 * (lo + hi)
            t = next > lo && next < hi ? next : 0.5 * (lo + hi)
        }
        return ((ay * t + by) * t + cy) * t
    }

    /// The model the animations replace: the node's own translate and scale.
    func setModel(translate: CGPoint, scale: CGPoint) {
        modelTranslate = translate
        modelScale = scale
        var t = CATransform3DMakeTranslation(translate.x, translate.y, 0)
        t = CATransform3DScale(t, scale.x, scale.y, 1)
        if !CATransform3DEqualToTransform(transform, t) { transform = t }
    }

    /// Whether an animation still moves: a held one (an agent's clock) shows one instant, sampled as it is set.
    private var running: Bool {
        let now = convertTime(CACurrentMediaTime(), from: nil)
        return animations.contains { $0.speed != 0 && now < Self.end(of: $0) }
    }

    /// Whether an animation shows: one that moves or is held, or one ended with a forwards fill.
    private var showsAnimation: Bool {
        let now = convertTime(CACurrentMediaTime(), from: nil)
        return animations.contains { $0.speed == 0 || now < Self.end(of: $0) || $0.fillMode == .forwards || $0.fillMode == .both }
    }

    private var animations: [CAAnimation] { (animationKeys() ?? []).compactMap { animation(forKey: $0) } }

    private static func end(of animation: CAAnimation) -> CFTimeInterval {
        animation.beginTime + animation.duration * (animation.repeatCount == 0 ? 1 : Double(animation.repeatCount))
    }

    /// Frames the glass where the box is now drawn, and keeps doing so each
    /// frame while an animation moves and the node is on screen.
    func follow() {
        guard let node else { return stop() }
        node.sizeMaterial()
        if node.pressFactor != 1 || !CATransform3DIsIdentity(node.layer?.transform ?? CATransform3DIdentity) {
            node.applyGlassPress()
        }
        guard running, node.window != nil, superlayer != nil else { return stop() }
        guard link == nil else { return }
        let frames = node.displayLink(target: self, selector: #selector(tick))
        frames.add(to: .main, forMode: .common)
        link = frames
    }

    @objc private func tick() { follow() }

    private func stop() { link?.invalidate(); link = nil }
}

extension NodeView {
    /// The node's translate, scale about `transform-origin` and layout
    /// transition offset, from its bounds to its superview's frame space,
    /// when its glass takes them by geometry; else nil.
    var glassGeometry: CGAffineTransform? {
        guard groupedGlass, !layerOwnsTransform, layer?.anchorPoint == .zero else { return nil }
        let shown = glassFollower?.presented ?? (translate, scale)
        // Scaled to nothing, or through it as it plays, the glass's frame is empty.
        let shownScale = CGPoint(x: max(0, shown.scale.x), y: max(0, shown.scale.y))
        let lift = presenter?.reorders.lifts(id) == true ? translate : .zero
        let origin = transformOriginPoint, shift = shown.translate
        return CGAffineTransform(translationX: shift.x - lift.x + layoutOffset.x + origin.x, y: shift.y - lift.y + layoutOffset.y + origin.y)
            .scaledBy(x: shownScale.x, y: shownScale.y).translatedBy(x: -origin.x, y: -origin.y)
    }

    /// Whether transform routing (`glassFollower` presence) disagrees with current `groupedGlass` eligibility.
    var glassRoutingChanged: Bool { groupedGlass != (glassFollower != nil) }

    /// Applies press scale on the node's layer conjugated about the current drawn origin of its grouped glass.
    func applyGlassPress() {
        guard let geometry = glassGeometry, let layer else { return }
        let drawnOrigin = transformOriginPoint.applying(geometry), factor = pressFactor
        let pressed = CGAffineTransform(translationX: drawnOrigin.x, y: drawnOrigin.y).scaledBy(x: factor, y: factor)
            .translatedBy(x: -drawnOrigin.x, y: -drawnOrigin.y)
        if !CATransform3DIsAffine(layer.transform) || layer.affineTransform() != pressed { layer.setAffineTransform(pressed) }
    }

    /// A glass in a group that can take the box's transform by geometry, whatever the transform now is.
    private var groupedGlass: Bool {
        guard #available(macOS 26.0, *), let glass = materialView as? NSGlassEffectView, glassIsolation == nil,
              glass.superview === self, let content = materialContent, glass.contentView === content, baseContainer === content,
              subviews.count == 1, subviews[0] === glass,
              !isSurfaceControl, kind != "text", textRaster == nil, readerParagraph == nil, image == nil,
              web == nil, video == nil, layer?.contents == nil,
              !clipsToBounds, clipPath == nil, style["mask_image"] == nil, boxFilter == nil,
              surface == nil || (!hasBoxPaint && shadowColor == nil),
              rotate == 0, scale.x >= 0, scale.y >= 0, !standsInSpace, flightLook == nil, shadowCaster == nil, insetCaster == nil,
              nearestGlassGroup() != nil else { return false }
        return true
    }

    /// A grouped glass plays its translate and scale animations on a follower
    /// it samples, not on its layer; any other box on its layer. Decided
    /// before the layer's anchor (`applyTransform`), which follows from it.
    func routeGlassTransform() {
        let clock = presenter?.session?.clock
        guard groupedGlass, let layer else {
            guard let follower = glassFollower else { return }
            glassFollower = nil
            follower.removeFromSuperlayer()
            presenter?.boxAnimations.setTransformProxy(id, nil, clock: clock)
            return
        }
        let follower = glassFollower ?? GlassFollower()
        if glassFollower == nil {
            follower.node = self
            follower.frame = .zero
            glassFollower = follower
        }
        if follower.superlayer !== layer { layer.addSublayer(follower) }
        follower.setModel(translate: translate, scale: scale)
        presenter?.boxAnimations.setTransformProxy(id, follower, clock: clock)
    }

    /// A material is a view the node holds, not paint, so it takes the
    /// surface's size (`Surface.swift`) and, for a grouped glass, the box's
    /// transform: framed at the box as drawn, its content keeping the box as
    /// its bounds.
    func sizeMaterial() {
        guard let material = materialView else { return }
        let geometry = glassGeometry
        // Disable flexible autoresizing while geometry owns the glass frame: AppKit layout otherwise enlarges
        // sub-10 pt or collapsed (0x0) NSGlassEffectView frames and mutates contentView.bounds.
        let mask: NSView.AutoresizingMask = geometry == nil ? [.width, .height] : []
        if material.autoresizingMask != mask { material.autoresizingMask = mask }
        let box = materialBox
        let shown = geometry.map { box.applying($0) } ?? box
        if material.frame != shown { material.frame = shown }
        if let content = materialContent {
            if content.autoresizingMask != mask { content.autoresizingMask = mask }
            let frame = content.isDescendant(of: material) ? CGRect(origin: .zero, size: material.bounds.size) : box
            if content.frame != frame { content.frame = frame }
            let inner = CGRect(origin: .zero, size: box.size)
            if content.bounds != inner { content.bounds = inner }
        }
        applyMaterialRadius()
    }
}
#endif
