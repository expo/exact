// @ref LLP 1055 D4, D7 — an `svg`'s scene as Core Animation layers, and CSS
// animations as `CAKeyframeAnimation`s. The Rust host sends the whole scene
// (host/apple/src/svg.rs): paths already flattened, paint resolved, dashes
// already scaled by `pathLength`, animations already lowered to key times,
// values and one cubic per segment. This file only builds layers from it, so
// iOS and macOS share it and no host parses SVG.
import CoreGraphics
import CoreText
import Foundation
import QuartzCore
import ExactKit
import ExactEffects

/// No implicit animations: a model change is immediate, as in CSS.
private final class Still: NSObject, CAAction {
    static let shared = Still()
    func run(forKey event: String, object anObject: Any, arguments dict: [AnyHashable: Any]?) {}
}
private final class StillDelegate: NSObject, CALayerDelegate {
    static let shared = StillDelegate()
    func action(for layer: CALayer, forKey event: String) -> CAAction? { Still.shared }
}

private func still<L: CALayer>(_ layer: L) -> L { layer.delegate = StillDelegate.shared; return layer }

private func num(_ v: Any?) -> Double { (v as? NSNumber)?.doubleValue ?? 0 }
private func nums(_ v: Any?) -> [Double] { (v as? [Any])?.map(num) ?? [] }

/// A colour the host sent (`paintColor`).
private func color(_ v: Any?, dark: Bool, tint: PlatformColor? = nil) -> CGColor? { paintColor(v, dark: dark, tint: tint) }
private func path(_ v: Any?) -> CGPath { svgPath(v) }

private func affine(_ v: Any?) -> CGAffineTransform? {
    let t = nums(v)
    return t.count == 6 ? CGAffineTransform(a: t[0], b: t[1], c: t[2], d: t[3], tx: t[4], ty: t[5]) : nil
}

/// One `svg` view's scene: a content-box layer whose sublayer transform is
/// the view box, a `CAShapeLayer` per shape, a `CALayer` per `g` and nested
/// `svg`, and a pair of layers around an element with a transform (LLP
/// 1055.000 D5): the outer takes the individual properties, the inner the
/// `transform` list, both about the transform origin. Every container's
/// anchor is its bounds' origin, so a sublayer transform applies from there.
final class SvgScene {
    let root = still(CALayer())
    #if os(macOS)
    // The flipped bounds and scene transform belong to the layer rendered
    // by an offscreen island, as they do on the displayed SVG.
    private var content: CALayer { root }
    #else
    private let content = still(CALayer())
    #endif
    #if os(iOS) || os(tvOS)
    private let blended = still(CALayer())
    private var blendKey: Int?
    private var unownedBlendMotion = false
    let blends = SvgBlend()
    private var synchronousFilters = false
    #endif
    /// An island caller renders the layers itself, without a second bitmap.
    var rasterizing = false
    private var layers: [Int: CALayer] = [:]
    /// The transform pair around an element, by id.
    private var wrappers: [Int: (outer: CALayer, inner: CALayer)] = [:]
    /// The transform animations each wrapper plays, for the agent clock's re-seek.
    private var wrapSpecs: [Int: [CssAnimation]] = [:]
    private var wrapInstalled: [Int: [String: Int]] = [:]
    private var installed: [Int: [String: Int]] = [:]
    /// Each masked element's mask island, by the digest of what drew it.
    private var islands: [Int: (key: Int, layer: CALayer)] = [:]
    /// Each filtered element's picture, by the digest of what drew it.
    private var pictures: [Int: (key: Int, layer: CALayer)] = [:]
    private var specs: [Int: [CssAnimation]] = [:]
    /// The view's device pixels per unit, for the parts drawn as pixels
    /// (`SvgHost.pixels`).
    var scale: CGFloat = 2
    /// The presenter's fonts, for SVG text (LLP 1055.000 D11).
    var fonts: SvgText.Fonts?
    /// The view's tint, which a paint naming it resolves by (`AccentColor`
    /// under an authored `accent-color`, LLP 1116.001 D2); nil on macOS.
    var tint: PlatformColor?
    /// For hit testing (LLP 1055.000 D17): each element layer's node and
    /// parent element, whether it takes presses, whether hits pass it by.
    private var node: [ObjectIdentifier: (uid: Int, node: UInt32)] = [:]
    private var parentOf: [Int: Int] = [:]
    private var nodeOf: [Int: UInt32] = [:]
    private var pressable: Set<Int> = []
    private var passes: Set<Int> = []
    private var painted: [Int: (fill: Bool, stroke: Bool)] = [:]
    /// A filter's unpainted content keeps its geometry and element ancestry
    /// for hits; the picture's pixels and region do not affect them (D17).
    private var filterShapes: [Int: CALayer] = [:]
    /// A system font where no presenter's fonts are known.
    static let systemFonts: SvgText.Fonts = { size, _, _, _ in CTFontCreateUIFontForLanguage(.system, size, nil) ?? CTFontCreateWithName("Helvetica" as CFString, size, nil) }

    /// The content box when the `svg` clips to it: an island over the cap
    /// renders only what can show there (`FilterExtent.extent`).
    private var clipped: CGRect?

    /// Drawn by a `CARenderer` (a live filter picture's sub-scene): the
    /// instant its next render draws at, which its held animations are placed
    /// from, so each draws exactly where it is held (LLP 1003.001 D4).
    var offscreen: CFTimeInterval?

    init() {
        root.masksToBounds = false; root.anchorPoint = .zero
        #if !os(macOS)
        content.anchorPoint = .zero
        root.addSublayer(content)
        #endif
        #if os(iOS) || os(tvOS)
        blended.anchorPoint = .zero
        #endif
    }

    func render(in ctx: CGContext) -> Bool {
        #if os(iOS) || os(tvOS)
        if blends.contains(content) { return !unownedBlendMotion && blends.canCapture(content) && blends.draw(content, in: ctx) }
        #endif
        renderUnblended(in: ctx)
        return true
    }

    func renderUnblended(in ctx: CGContext) {
        content.render(in: ctx)
    }

    #if os(iOS) || os(tvOS)
    /// Filtered pictures that follow a changing input on the GPU, by element.
    private var live: [Int: SvgFilterLive] = [:]
    #endif

    /// A shown filtered picture whose input changed: on iOS, the same layer
    /// redrawn on the GPU after this commit (`SvgFilterLive`); `nil` for a
    /// first picture, or a chain or host the GPU path does not take.
    private func follow(_ id: Int, _ fl: [String: Any], k: CGFloat, dark: Bool, clock: Double?) -> CALayer? {
        #if os(iOS) || os(tvOS)
        guard !synchronousFilters else { forget(id); return nil }
        // A picture whose content animates follows it from its first frame:
        // its animations play in its sub-scene (`svg_lower::in_picture`).
        let animated = SvgFilterLive.animated(fl["c"] as Any)
        guard pictures[id] != nil || animated, SvgFilterLive.takes(fl),
              let e = SvgIsland.filterExtent(fl, k: k, seen: seen(fl)), e.gpu else { return nil }
        let shown = pictures[id]?.layer ?? {
            let l = CALayer()
            l.actions = ["contents": NSNull(), "bounds": NSNull(), "position": NSNull(), "contentsRect": NSNull()]
            l.anchorPoint = .zero
            return l
        }()
        let picture = live[id].flatMap { $0.layer === shown ? $0 : nil } ?? SvgFilterLive(layer: shown)
        live[id] = picture
        picture.update(els: fl["c"] as? [Any] ?? [], rect: e.rect, w: e.w, h: e.h, k: e.k, program: e.program,
                       dark: dark, fonts: fonts ?? SvgScene.systemFonts, clock: clock, now: pictures[id] == nil)
        return shown
        #else
        return nil
        #endif
    }

    private func forget(_ id: Int) {
        #if os(iOS) || os(tvOS)
        live.removeValue(forKey: id)?.stop()
        #endif
    }

    /// Lone drop shadows by element: the region's clip and the layer
    /// casting the shadow, which holds the element.
    private var shadows: [Int: (clip: CALayer, cast: CALayer)] = [:]

    /// A filter that is one `feDropShadow` (or CSS `drop-shadow()`) of the
    /// element, as Core Animation's shadow says it: `shadowRadius` is the
    /// Gaussian's σ, in the element's user space as the offset is. Its
    /// colour space (`color-interpolation-filters`) changes only how the
    /// element's antialiased edge blends over its own shadow.
    static func dropShadow(_ fl: [String: Any]) -> (region: CGRect, sigma: CGFloat, offset: CGSize, color: CGColor, opacity: Float)? {
        FilterProgram.dropShadow(program: nums(fl["p"]))
    }
    /// Each shape's last drawn spec, hashed (with the appearance and the
    /// scale its parts are drawn at): a scene applied again redraws only the
    /// shapes that changed. A live filter picture's sub-scene is applied on
    /// every frame of the animation inside it, and redrawing its unchanged
    /// gradient card was most of that (467 ms/s on the M1 iPad Pro).
    private var drawn: [Int: Int] = [:]

    /// Whether shape `id` must be drawn from `e`: it changed, or is new.
    private func drew(_ id: Int, _ e: [String: Any], dark: Bool, hitOnly: Bool) -> Bool {
        var h = Hasher()
        for k in e.keys.sorted() where k != "a" { h.combine(k); CssAnimations.digest(e[k], into: &h) }
        h.combine(dark); h.combine(scale); h.combine(tint.map { "\($0)" }); h.combine(hitOnly)
        let d = h.finalize()
        if drawn[id] == d { return false }
        drawn[id] = d
        return true
    }

    /// What of an island's user space can show: the clipping content box
    /// through the inverse of the island's `m` (user space to that box).
    private func seen(_ spec: [String: Any]) -> CGRect? {
        let m = nums(spec["m"])
        guard let clipped, m.count == 6, m[0] * m[3] - m[1] * m[2] != 0 else { return nil }
        return clipped.applying(CGAffineTransform(a: m[0], b: m[1], c: m[2], d: m[3], tx: m[4], ty: m[5]).inverted())
    }

    /// Build or update the layers from a scene; unchanged animations keep
    /// running. The root stays where `SvgHost.place` put it.
    func apply(_ scene: [String: Any], dark: Bool, clock: Double?) {
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        let box = nums(scene["box"])
        clipped = scene["clip"] != nil && box.count == 4 ? CGRect(x: 0, y: 0, width: box[2], height: box[3]) : nil
        let t = nums(scene["t"])
        root.isHidden = t.count != 6
        if t.count == 6 { content.sublayerTransform = CATransform3DMakeAffineTransform(CGAffineTransform(a: t[0], b: t[1], c: t[2], d: t[3], tx: t[4], ty: t[5])) }
        #if os(iOS) || os(tvOS)
        content.opacity = 1
        unownedBlendMotion = SvgFilterLive.animated(scene)
        synchronousFilters = rasterizing || (SvgBlend.requested(scene) && !unownedBlendMotion)
        blends.styles = [:]
        #endif
        var alive: Set<Int> = []
        attach(scene["els"] as? [Any] ?? [], to: content, dark: dark, clock: clock, alive: &alive)
        for (id, layer) in layers where !alive.contains(id) {
            layer.removeAllAnimations(); layer.removeFromSuperlayer()
            layers.removeValue(forKey: id); installed.removeValue(forKey: id); specs.removeValue(forKey: id); islands.removeValue(forKey: id); pictures.removeValue(forKey: id); forget(id); drawn.removeValue(forKey: id)
            if let pair = shadows.removeValue(forKey: id) { pair.clip.removeFromSuperlayer() }
            if let pair = wrappers.removeValue(forKey: id) { pair.outer.removeFromSuperlayer() }
            wrapSpecs.removeValue(forKey: id); wrapInstalled.removeValue(forKey: id)
            node.removeValue(forKey: ObjectIdentifier(layer)); parentOf.removeValue(forKey: id)
            nodeOf.removeValue(forKey: id); pressable.remove(id); passes.remove(id)
            painted.removeValue(forKey: id)
            filterShapes.removeValue(forKey: id)?.removeFromSuperlayer()
        }
        #if os(iOS) || os(tvOS)
        if !rasterizing, blends.contains(content) {
            var h = Hasher()
            CssAnimations.digest(scene, into: &h)
            h.combine(scale); h.combine(dark); h.combine(tint.map { "\($0)" })
            let key = h.finalize()
            if blendKey != key {
                blended.contents = nil; blendKey = nil
                let area = clipped ?? blends.bounds(content).integral
                if !unownedBlendMotion, blends.canCapture(content),
                   let e = FilterExtent.extent(area, k: scale, seen: clipped),
                   let image = blends.bitmap(content, rect: e.rect, w: e.w, h: e.h)?.makeImage() {
                    blended.bounds = CGRect(origin: .zero, size: e.rect.size)
                    blended.position = e.rect.origin
                    blended.contents = image; blendKey = key
                }
            }
            if blended.superlayer !== root { root.addSublayer(blended) }
            if blended.contents != nil { content.opacity = 0 } // Keep hit geometry.
            else { SvgBlend.unavailable() }
        } else {
            blended.removeFromSuperlayer(); blended.contents = nil; blendKey = nil
        }
        #endif
    }

    /// The node a press at `point` (the host view's coordinates) goes to:
    /// the topmost element painted there (its fill, or its stroke's
    /// outline), then up to the nearest one with a press handler
    /// (LLP 1055.000 D17). `nil` when none takes it.
    func target(at point: CGPoint) -> UInt32? {
        guard let host = root.superlayer, !pressable.isEmpty else { return nil }
        var hit = self.hit(root, root.convert(point, from: host))
        while let uid = hit {
            if pressable.contains(uid) { return nodeOf[uid] }
            hit = parentOf[uid] ?? nil
        }
        return nil
    }

    /// The topmost element layer under `p` (in `layer`'s coordinates).
    private func hit(_ layer: CALayer, _ p: CGPoint) -> Int? {
        // Opacity does not change a hit (SVG 2 pointer-events); hiding does.
        if layer.isHidden { return nil }
        let mine = node[ObjectIdentifier(layer)]
        let island = mine.flatMap { islands[$0.uid]?.layer }
        if let mask = layer.mask, !SvgScene.covers(mask, mask.convert(p, from: layer), ignoring: island) { return nil }
        if layer.masksToBounds, !layer.bounds.contains(p) { return nil }
        if let mine, passes.contains(mine.uid) { return nil }
        if let mine, let content = filterShapes[mine.uid] ?? shadows[mine.uid]?.cast {
            return hit(content, content.convert(p, from: layer))
        }
        if let shape = layer as? CAShapeLayer, let mine, let paint = painted[mine.uid] {
            return SvgScene.paints(shape, at: p, fill: paint.fill, stroke: paint.stroke) ? mine.uid : nil
        }
        for sub in (layer.sublayers ?? []).reversed() where sub !== layer.mask {
            // A text run's glyph layers hit as their element.
            if let found = hit(sub, sub.convert(p, from: layer)) { return found == -1 ? (mine?.uid ?? -1) : found }
        }
        if let shape = layer as? CAShapeLayer, SvgScene.paints(shape, at: p, fill: shape.fillColor != nil, stroke: shape.strokeColor != nil) {
            return mine?.uid ?? -1
        }
        return nil
    }

    /// Whether a shape layer paints `p`: its fill, or its stroke's outline
    /// (caps, joins and dashes as Core Graphics strokes them).
    private static func paints(_ shape: CAShapeLayer, at p: CGPoint, fill: Bool, stroke: Bool) -> Bool {
        guard let path = shape.path else { return false }
        if fill, path.contains(p, using: shape.fillRule == .evenOdd ? .evenOdd : .winding) { return true }
        guard stroke, shape.lineWidth > 0 else { return false }
        var line = path
        if let dash = shape.lineDashPattern, !dash.isEmpty {
            line = path.copy(dashingWithPhase: shape.lineDashPhase, lengths: dash.map { CGFloat($0.doubleValue) })
        }
        let cap: CGLineCap = shape.lineCap == .round ? .round : shape.lineCap == .square ? .square : .butt
        let join: CGLineJoin = shape.lineJoin == .round ? .round : shape.lineJoin == .bevel ? .bevel : .miter
        return line.copy(strokingWithWidth: shape.lineWidth, lineCap: cap, lineJoin: join, miterLimit: shape.miterLimit).contains(p)
    }

    /// Clip paths intersect; a mask island at the end of the chain does
    /// not restrict hits, even where its coverage is zero.
    private static func covers(_ mask: CALayer, _ p: CGPoint, ignoring island: CALayer?) -> Bool {
        if mask === island { return true }
        if let inner = mask.mask, !covers(inner, inner.convert(p, from: mask), ignoring: island) { return false }
        return (mask.sublayers ?? []).contains { sub in
            guard let s = sub as? CAShapeLayer, let path = s.path else { return false }
            return path.contains(sub.convert(p, from: mask), using: s.fillRule == .evenOdd ? .evenOdd : .winding)
        }
    }

    /// Re-seek every animation to an agent-owned clock (or back to real time).
    func seek(clock: Double?, force: Bool = false) {
        if force { installed = [:]; wrapInstalled = [:] }
        for (id, list) in specs { if let layer = layers[id] { CssAnimations.apply(list, to: layer, clock: clock, installed: &installed[id, default: [:]], offscreen: offscreen) } }
        for (id, list) in wrapSpecs { if let outer = wrappers[id]?.outer { CssAnimations.apply(list, to: outer, clock: clock, installed: &wrapInstalled[id, default: [:]], offscreen: offscreen) } }
        #if os(iOS) || os(tvOS)
        for l in live.values { l.seek(clock) }
        #endif
    }

    /// Everything off (the view is destroyed or parked).
    func reset() {
        for layer in layers.values { layer.removeAllAnimations() }
        content.sublayers?.forEach { $0.removeFromSuperlayer() }
        #if os(iOS) || os(tvOS)
        blended.removeFromSuperlayer(); blended.contents = nil; blendKey = nil; blends.styles = [:]
        #endif
        for pair in wrappers.values { pair.outer.removeAllAnimations() }
        layers = [:]; installed = [:]; specs = [:]; wrappers = [:]; wrapSpecs = [:]; wrapInstalled = [:]; islands = [:]; pictures = [:]; drawn = [:]; shadows = [:]
        #if os(iOS) || os(tvOS)
        for l in live.values { l.stop() }
        live = [:]
        #endif
        node = [:]; parentOf = [:]; nodeOf = [:]; pressable = []; passes = []
        painted = [:]; filterShapes = [:]
    }

    /// The layer to place for an element: itself, or the outer of its
    /// transform pair, the element inside the inner.
    private func wrap(_ id: Int, _ layer: CALayer, _ tf: [String: Any]?, clock: Double?) -> CALayer {
        guard let tf else {
            if let pair = wrappers.removeValue(forKey: id) { pair.outer.removeFromSuperlayer() }
            wrapSpecs.removeValue(forKey: id); wrapInstalled.removeValue(forKey: id)
            return layer
        }
        let pair = wrappers[id] ?? { let p = (outer: still(CALayer()), inner: still(CALayer())); p.outer.addSublayer(p.inner); wrappers[id] = p; return p }()
        let o = nums(tf["o"])
        let origin = o.count == 2 ? CGPoint(x: o[0], y: o[1]) : .zero
        for l in [pair.outer, pair.inner] {
            l.bounds = CGRect(origin: origin, size: .zero)
            l.position = origin
        }
        pair.outer.setAffineTransform(affine(tf["i"]) ?? .identity)
        pair.inner.setAffineTransform(affine(tf["m"]) ?? .identity)
        if layer.superlayer !== pair.inner { layer.removeFromSuperlayer(); pair.inner.addSublayer(layer) }
        // The individual properties' lowered animations play on the outer
        // layer (LLP 1055.001, as a box's on iOS): each key path replaces
        // one component of its transform and keeps the others.
        let list = CssAnimation.list(tf["a"])
        wrapSpecs[id] = list.isEmpty ? nil : list
        CssAnimations.apply(list, to: pair.outer, clock: clock, installed: &wrapInstalled[id, default: [:]], offscreen: offscreen)
        return pair.outer
    }

    private func attach(_ elements: [Any], to parent: CALayer, dark: Bool, clock: Double?, alive: inout Set<Int>, owner: Int? = nil, hitOnly: Bool = false) {
        var order: [CALayer] = []
        for case let e as [String: Any] in elements {
            let id = Int(num(e["id"]))
            alive.insert(id)
            parentOf[id] = owner
            nodeOf[id] = UInt32(num(e["n"]))
            if e["h"] != nil { pressable.insert(id) } else { pressable.remove(id) }
            if e["pn"] != nil { passes.insert(id) } else { passes.remove(id) }
            let text = e["tx"] as? [Any]
            let group = e["g"] != nil || text != nil
            let filter = e["fl"] as? [String: Any]
            if filter == nil || hitOnly || filter.flatMap(SvgScene.dropShadow) != nil {
                filterShapes.removeValue(forKey: id)?.removeFromSuperlayer()
            }
            let layer: CALayer
            if let existing = layers[id], (existing is CAShapeLayer) != group { layer = existing } else {
                layers[id]?.removeFromSuperlayer()
                layer = group ? still(CALayer()) : still(CAShapeLayer())
                layers[id] = layer
                drawn.removeValue(forKey: id)
            }
            layer.opacity = Float(num(e["o"]))
            if let text {
                // @ref LLP 1055.000 D11 — text: a layer of glyph outlines per run.
                SvgText.build(layer, chunks: text, dark: dark, fonts: fonts ?? SvgScene.systemFonts) { [tint] in color($0, dark: $1, tint: tint) }
            } else if group {
                let vp = nums(e["vp"])
                if vp.count == 4 {
                    // A nested `svg`: its own viewport, clipped unless visible.
                    layer.anchorPoint = .zero
                    layer.bounds = CGRect(x: 0, y: 0, width: vp[2], height: vp[3])
                    layer.position = CGPoint(x: vp[0], y: vp[1])
                    layer.masksToBounds = num(e["clip"]) != 0
                    let view = affine(e["t"])
                    layer.isHidden = view == nil
                    layer.sublayerTransform = CATransform3DMakeAffineTransform(view ?? .identity)
                }
                let children = hitOnly ? filter?["c"] ?? e["c"] : e["c"]
                attach(children as? [Any] ?? [], to: layer, dark: dark, clock: clock, alive: &alive, owner: id, hitOnly: hitOnly)
                // @ref LLP 1055.000 D14 — a filtered element's picture is an island.
                if !hitOnly, let fl = filter, let drop = SvgScene.dropShadow(fl) {
                    // A lone drop shadow is Core Animation's own shadow on
                    // the element, clipped to the filter region: no pixels
                    // of ours (F1's 300 shadowed POIs held a 21 MB island
                    // and 150 MB of Core Image intermediates at launch).
                    if let old = pictures.removeValue(forKey: id) { old.layer.removeFromSuperlayer(); forget(id) }
                    let pair = shadows[id] ?? { let p = (clip: still(CALayer()), cast: still(CALayer())); p.clip.addSublayer(p.cast); p.clip.anchorPoint = .zero; p.clip.masksToBounds = true; shadows[id] = p; return p }()
                    pair.clip.bounds = drop.region
                    pair.clip.position = drop.region.origin
                    pair.cast.shadowColor = drop.color
                    pair.cast.shadowOpacity = drop.opacity
                    pair.cast.shadowRadius = drop.sigma
                    pair.cast.shadowOffset = drop.offset
                    #if os(iOS) || os(tvOS)
                    // A filter isolates its source even when its shadow is transparent.
                    blends.styles[ObjectIdentifier(pair.cast)] = SvgBlend.Style(mode: 0, isolate: true)
                    #endif
                    attach(fl["c"] as? [Any] ?? [], to: pair.cast, dark: dark, clock: clock, alive: &alive, owner: id)
                    if pair.clip.superlayer !== layer { layer.addSublayer(pair.clip) }
                } else if !hitOnly, let fl = filter {
                    if let pair = shadows.removeValue(forKey: id) { pair.clip.removeFromSuperlayer() }
                    var h = Hasher()
                    CssAnimations.digest(fl, into: &h)
                    #if os(iOS) || os(tvOS)
                    h.combine(synchronousFilters)
                    #endif
                    h.combine(scale); h.combine(dark)
                    let key = h.finalize()
                    let k = CGFloat(num(fl["k"])) * scale
                    let picture = pictures[id].flatMap { $0.key == key ? $0.layer : nil }
                        ?? follow(id, fl, k: k, dark: dark, clock: clock)
                        ?? SvgIsland.filter(fl, k: k, seen: seen(fl), dark: dark, fonts: fonts ?? SvgScene.systemFonts)
                    if pictures[id]?.layer !== picture { pictures[id]?.layer.removeFromSuperlayer() }
                    pictures[id] = (key, picture)
                    if picture.superlayer !== layer { layer.addSublayer(picture) }
                    let content = filterShapes[id] ?? still(CALayer())
                    content.opacity = 0
                    filterShapes[id] = content
                    attach(fl["c"] as? [Any] ?? [], to: content, dark: dark, clock: clock, alive: &alive, owner: id, hitOnly: true)
                    if content.superlayer !== layer { layer.addSublayer(content) }
                } else {
                    if let old = pictures.removeValue(forKey: id) { old.layer.removeFromSuperlayer(); forget(id) }
                    if let pair = shadows.removeValue(forKey: id) { pair.clip.removeFromSuperlayer() }
                }
            } else if let shape = layer as? CAShapeLayer, drew(id, e, dark: dark, hitOnly: hitOnly) {
                shape.path = path(e["p"])
                let pos = nums(e["pos"])
                shape.position = pos.count == 2 ? CGPoint(x: pos[0], y: pos[1]) : .zero
                shape.fillColor = color(e["f"], dark: dark, tint: tint)
                shape.strokeColor = color(e["s"], dark: dark, tint: tint)
                // Which paints take hits, by `pointer-events` (`hp`); a set
                // paint may draw nothing. Without it (`bounding-box`), the
                // paints drawn.
                let hp = nums(e["hp"])
                painted[id] = hp.count == 2 ? (hp[0] != 0, hp[1] != 0)
                    : (shape.fillColor != nil || SvgPaint.server(e["f"]) != nil,
                       shape.strokeColor != nil || SvgPaint.server(e["s"]) != nil)
                shape.lineWidth = CGFloat(num(e["w"]))
                shape.lineCap = [CAShapeLayerLineCap.butt, .round, .square][min(Int(num(e["cap"])), 2)]
                shape.lineJoin = [CAShapeLayerLineJoin.miter, .round, .bevel][min(Int(num(e["join"])), 2)]
                shape.miterLimit = CGFloat(num(e["ml"]))
                shape.fillRule = num(e["rule"]) == 1 ? .evenOdd : .nonZero
                let dash = nums(e["dash"])
                shape.lineDashPattern = dash.isEmpty ? nil : dash.map { NSNumber(value: $0) }
                shape.lineDashPhase = CGFloat(num(e["ph"]))
                if !hitOnly, SvgPaint.needsParts(e) {
                    // Gradients or a paint order: part layers paint it.
                    shape.fillColor = nil; shape.strokeColor = nil
                    SvgPaint.parts(shape, e, scale: scale, dark: dark, fonts: fonts ?? SvgScene.systemFonts) { [tint] in color($0, dark: $1, tint: tint) }
                } else if shape.sublayers?.isEmpty == false {
                    shape.sublayers?.forEach { $0.removeFromSuperlayer() }
                }
            }
            if let shape = layer as? CAShapeLayer {
                // A non-scaling stroke: the path is in the content box's
                // space, and the layer undoes its parents' transforms.
                shape.setAffineTransform(affine(e["inv"]) ?? .identity)
            }
            // @ref LLP 1055.000 D10 — a clip is the layer's mask; a mask
            // is an island inside it.
            layer.mask = SvgPaint.clip(e["cl"]) { path($0) }
            if !hitOnly, let mk = e["mk"] as? [String: Any] {
                var h = Hasher()
                CssAnimations.digest(mk, into: &h)
                h.combine(scale); h.combine(dark)
                let key = h.finalize()
                let m = islands[id].flatMap { $0.key == key ? $0.layer : nil }
                    ?? SvgIsland.mask(mk, k: CGFloat(num(mk["k"])) * scale, seen: seen(mk), dark: dark, fonts: fonts ?? SvgScene.systemFonts)
                islands[id] = (key, m)
                if var inner = layer.mask {
                    while let next = inner.mask { inner = next }
                    inner.mask = m
                } else {
                    layer.mask = m
                }
            } else {
                islands.removeValue(forKey: id)
            }
            node[ObjectIdentifier(layer)] = (id, UInt32(num(e["n"])))
            let list = CssAnimation.list(e["a"])
            specs[id] = list
            CssAnimations.apply(list, to: layer, clock: clock, installed: &installed[id, default: [:]], offscreen: offscreen)
            let placed = wrap(id, layer, e["tf"] as? [String: Any], clock: clock)
            #if os(macOS)
            SvgIsland.blend(placed, mode: Int(num(e["bl"])), isolate: e["iso"] != nil, scale: scale)
            #else
            blends.styles[ObjectIdentifier(placed)] = SvgBlend.Style(mode: Int(num(e["bl"])), isolate: e["iso"] != nil)
            #endif
            order.append(placed)
        }
        // Paint order is document order.
        if parent.sublayers?.map(ObjectIdentifier.init) != order.map(ObjectIdentifier.init) {
            for layer in order { layer.removeFromSuperlayer() }
            for layer in order { parent.addSublayer(layer) }
        }
    }
}

/// A presenter's scenes, by view id: the `svg` ops, the agent clock's
/// re-seek, and cleanup when a view goes. Box animations are the core's
/// (`BoxAnimations`).
final class SvgHost: SvgScenes {
    private var scenes: [UInt32: SvgScene] = [:]
    /// Each view's last scene: a `light-dark()` paint is resolved when a
    /// scene is applied, so an appearance change applies it again
    /// (minesweeper F11, paint F10).
    private var payloads: [UInt32: [String: Any]] = [:]
    /// The session's fonts, for SVG text.
    private var fonts: SvgText.Fonts?
    private var seeked: Double?

    func useFonts(_ session: @escaping (CGFloat, Int, Int, Bool) -> CTFont?) {
        fonts = { size, weight, family, italic in session(size, weight, family, italic) ?? SvgScene.systemFonts(size, weight, family, italic) }
    }

    func scene(_ id: UInt32, _ payload: [String: Any], layer: CALayer?, dark: Bool, clock: Double?, limit: String?, tint: PlatformColor?) {
        guard let layer else { return }
        let scene = scenes[id] ?? { let s = SvgScene(); scenes[id] = s; return s }()
        scene.root.name = SvgLink.rootName
        if scene.root.superlayer !== layer { layer.addSublayer(scene.root) }
        scene.scale = Self.pixels(layer)
        scene.fonts = fonts
        scene.tint = tint
        let spec = payload["scene"] as? [String: Any] ?? [:]
        payloads[id] = spec
        scene.apply(spec, dark: dark, clock: clock)
        if let view = layer.delegate as? NodeView { place(id, in: view.contentBox()) }
        scene.root.applyColorRange(limit: limit, deep: true)
        (layer.delegate as? NodeView)?.overflowClipGeometryChanged()
    }

    /// Places `id`'s scene at its view's content box, which a sheet may hold:
    /// the one place a scene's root moves.
    func place(_ id: UInt32, in box: CGRect) {
        guard let root = scenes[id]?.root, root.frame != box else { return }
        CATransaction.begin(); CATransaction.setDisableActions(true)
        root.bounds = CGRect(origin: .zero, size: box.size)
        root.position = box.origin
        CATransaction.commit()
    }

    /// `id`'s view changed appearance: its scene is applied again in the
    /// new one. Only what the appearance reaches redraws (`drew`, and the
    /// islands' and pictures' keys, hash it); animations keep running.
    func reappear(_ id: UInt32, dark: Bool, clock: Double?, tint: PlatformColor?) {
        guard let scene = scenes[id], let spec = payloads[id] else { return }
        scene.tint = tint
        scene.apply(spec, dark: dark, clock: clock)
    }

    /// Device pixels per unit of the view `layer` backs. On the Mac a page
    /// zoom magnifies the view without raising its layer's `contentsScale`,
    /// so the scale is the view's `pixelScale` (LLP 1021 §5.3.1).
    private static func pixels(_ layer: CALayer) -> CGFloat {
        #if os(macOS)
        if let scale = (layer.delegate as? NodeView)?.pixelScale { return max(1, scale) }
        #endif
        return max(1, layer.contentsScale)
    }

    /// The page zoom or the display's scale changed: each scene whose view
    /// has another scale is applied again at it, which redraws only its
    /// pixel-drawn parts (their keys hash the scale).
    func rescale(clock: Double?) {
        for (id, scene) in scenes {
            guard let layer = scene.root.superlayer, let spec = payloads[id] else { continue }
            let scale = Self.pixels(layer)
            guard scale != scene.scale else { continue }
            scene.scale = scale
            scene.apply(spec, dark: (layer.delegate as? NodeView)?.drawsDark ?? false, clock: clock)
        }
    }

    /// An agent moved the clock: every held animation shows the new instant.
    func seek(clock: Double?) {
        guard clock != seeked else { return }
        seeked = clock
        for scene in scenes.values { scene.seek(clock: clock) }
    }

    /// The SVG element a press at `point` (in `id`'s view) goes to, if any
    /// takes it (LLP 1055.000 D17).
    func target(_ id: UInt32, at point: CGPoint) -> UInt32? { scenes[id]?.target(at: point) }

    func forget(_ id: UInt32) {
        if let scene = scenes.removeValue(forKey: id) { scene.reset(); scene.root.removeFromSuperlayer() }
        payloads.removeValue(forKey: id)
    }
}
