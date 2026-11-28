// @ref LLP 1123 — what draws an image's background extension: the image's box
// with its picture, and on each side where the box meets the route's safe
// area the box mirrored about that edge, blurred the more the further from
// it, as UIKit's `UIBackgroundExtensionView` draws it.
//
// Every layer here is placed from the box's geometry in the transaction that
// places the image, so a column's animation moves them with one animation:
// Core Animation interpolates each from where it was with the same progress,
// and as a mirror is the box reflected about its edge, a function linear in
// the box's position and size, it stays the box's reflection at every frame
// and meets the box at its edge. UIKit's view moves its mirror on SwiftUI's
// clock, from the main thread, about two frames behind the render server's
// animation of the box, and rounds the mirror's position to the pixel grid
// apart from the box's: the page showed through the gap, or the mirror
// covered the picture, by up to 80 points while a sidebar moved, and the edge
// that stays wobbled by half a pixel (measured on the iPhone Duo, iOS 27.1).
// How far each side reaches is the extension's own (`NodeView.setExtension`),
// decided from the box's layout.
//
// Where the extension reaches further past the box than the box's own size
// (a box beside the iPhone Duo's fold, with a sidebar as wide as the
// screen's half), the mirror is stretched to the extension's edge, as UIKit
// stretches its own (`Mirror.stretch`). Its pieces are placed by their
// frames, whose edges are linear in the box's position and size and in the
// extension's, so the one animation keeps the mirror at the box's edge and at
// the extension's at every frame.
#if os(iOS) || os(tvOS)
import CoreImage
import UIKit

/// An image's background extension (`NodeView.placeMirror`): the image's box, and its mirrors in the extension's reach.
final class ExtensionView: UIView {
    /// The image's box, which draws its picture and clips it to the image's corners.
    let content = UIView()
    private let box = ExtensionLayer(owner: nil), picture = ExtensionLayer(owner: nil)
    private var mirrors: [Mirror] = []
    /// Where the box, its content box and its picture were last placed.
    private var placed: Placement?
    /// A later `prune` is scheduled: one at a time, however often the box is placed meanwhile.
    private var pruneScheduled = false
    /// What the levels draw besides the blurs: the picture, for the sharp level, and its tint
    private var drawing: (pixels: CGImage, tint: CGColor?)?

    init() {
        super.init(frame: .zero)
        clipsToBounds = true
        isUserInteractionEnabled = false
        accessibilityElementsHidden = true
        content.clipsToBounds = true
        box.owner = content
        picture.owner = content
        box.masksToBounds = true
        box.addSublayer(picture)
        content.layer.addSublayer(box)
        addSubview(content)
    }
    required init?(coder: NSCoder) { nil }

    /// UIKit's blur, measured against the picture on the iPhone Duo (iOS 27.1): the mirror is sharp to about 5 points
    /// from the edge, then its sharpness falls as a Gaussian blur of about 0.15 times the distance would leave it, the
    /// picture's own sharpness under such blurs read from the same pixels. Each level is the picture blurred by `sigma`
    /// points, which shows fully from `full` points out, over the level before it; the first from `sharp` points.
    private static let levels: [(sigma: CGFloat, full: CGFloat)] = [(1, 8), (2.5, 18), (6, 42), (15, 100)]
    fileprivate static let sharp: CGFloat = 2
    /// The box at `shown` in this view, its content box `inner` and the picture at `drawn` in it, with `pixels`, a
    /// `tint` through their alpha, and `corners` the box's clip; a mirror on each side of `shown` short of this view's
    /// edges, reaching them. Called inside the animation that moves the image, each layer takes it.
    func place(_ shown: CGRect, inner: CGRect, drawn: CGRect, pixels: CGImage, load: Int, blurs store: MirrorBlurStore, tint: CGColor?, corners: CGPath?) {
        content.frame = shown
        corner(content.layer, corners, shown.size)
        box.frame = inner
        picture.frame = drawn
        show(pixels, tint: tint, in: picture, owner: content)
        // A side the box comes to has a mirror placed first where the box was, to move from there; one it leaves
        // keeps its mirror while the box still moves, as what it fills narrows to nothing (`prune`).
        let sides = Mirror.Side.all.filter { side in side.extends(shown, in: bounds) || mirrors.contains { $0.side == side } }
        let now = Placement(shown: shown, inner: inner, drawn: drawn, bounds: bounds), before = placed
        placed = now
        defer { prune() }
        let masks = Self.levels.map(\.full)
        if mirrors.map(\.side) != sides {
            mirrors = sides.map { side in
                if let kept = mirrors.first(where: { $0.side == side }) { return kept }
                let made = Mirror(side, owner: self, levels: Self.levels.count)
                layer.insertSublayer(made.container, below: content.layer)
                if let before { UIView.performWithoutAnimation { made.place(before, from: nil, masks: masks, corners: corners) } }
                return made
            }
        }
        guard !mirrors.isEmpty else { return }
        drawing = (pixels, tint)
        let perPoint = drawn.width > 0 ? CGFloat(pixels.width) / drawn.width : 1
        store.landed = { [weak self] images in self?.showLevels(images) }
        let images = store.images(MirrorBlurs.Key(source: pixels, load: load, sigmas: Self.levels.map { $0.sigma * perPoint }))
        for mirror in mirrors { mirror.place(now, from: before, masks: masks, corners: corners) }
        showLevels(images)
    }

    /// Each mirror's levels draw the picture, the sharp one, or its blurs
    private func showLevels(_ images: [CGImage]) {
        guard let (pixels, tint) = drawing else { return }
        for mirror in mirrors {
            for level in mirror.levels {
                for piece in level.pieces { show(level.index == 0 ? pixels : images[level.index - 1], tint: tint, in: piece.picture, owner: self) }
            }
        }
    }

    /// The mirrors of sides the box no longer meets go once nothing of the extension moves: by the model the box is
    /// at its end at once, while the animation that takes it there is still showing the mirror.
    private func prune() {
        guard let placed, mirrors.contains(where: { !$0.side.extends(placed.shown, in: bounds) }) else { return }
        let moving = sequence(first: layer as CALayer?, next: { $0?.superlayer }).prefix(while: { $0 != nil }).compactMap { $0 }
            .contains { !($0.animationKeys() ?? []).isEmpty } || !(content.layer.animationKeys() ?? []).isEmpty
        guard !moving else {
            guard !pruneScheduled else { return }
            pruneScheduled = true
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self] in
                self?.pruneScheduled = false
                self?.prune()
            }
            return
        }
        for mirror in mirrors where !mirror.side.extends(placed.shown, in: bounds) { mirror.container.removeFromSuperlayer() }
        mirrors.removeAll { !$0.side.extends(placed.shown, in: bounds) }
    }

    /// A picture's pixels, or with a tint the tint through their alpha (`RasterGeometry.draw`).
    private func show(_ pixels: CGImage, tint: CGColor?, in layer: CALayer, owner: UIView) {
        if let tint {
            let alpha = layer.mask ?? ExtensionLayer(owner: owner)
            alpha.frame = layer.bounds
            if (alpha.contents as AnyObject?) !== pixels { alpha.contents = pixels }
            layer.mask = alpha
            layer.contents = nil
            layer.backgroundColor = tint
        } else {
            layer.mask = nil
            layer.backgroundColor = nil
            if (layer.contents as AnyObject?) !== pixels { layer.contents = pixels }
        }
    }

}

/// An image's mirror blurs, kept with the image while it holds its picture (`NodeExtras.mirrorBlurs`): its extension
/// view goes whenever the box stops extending, as it does for a moment while a fold takes UIKit's columns through their
/// sizes, and the one made when it extends again draws the blurs already made. Those made stand while the pixels are
/// the same and each level is within a quarter of the blur asked for. The same picture shown at another size or decoded
/// again keeps drawing its blurs, at the picture's size, while those for now are made off the main thread, as UIKit's
/// own mirror leaves the main thread free during a fold. Only a picture with none yet, or another picture, is blurred
/// at once, so a mirror never draws another picture's blur, nor none.
final class MirrorBlurStore {
    private var made: MirrorBlurs?
    /// What was last asked for, which blurs made off the main thread are checked against as they land
    private var wanted: MirrorBlurs.Key?
    /// Blurs are being made off the main thread: one at a time, the latest `wanted` made when they land
    private var making = false
    /// Shows blurs that land: the extension view that draws them now
    var landed: (([CGImage]) -> Void)?

    func images(_ key: MirrorBlurs.Key) -> [CGImage] {
        wanted = key
        if let made, made.serves(key) { return made.images }
        if let made, made.key.load == key.load { makeWanted(); return made.images }
        let now = MirrorBlurs(key)
        made = now
        return now.images
    }

    /// Makes the blurs last asked for off the main thread, and shows them when they land. A call while some are being
    /// made loses nothing: `wanted` keeps what it asked, and the landing makes it next.
    private func makeWanted() {
        guard !making, let job = wanted else { return }
        making = true
        MirrorBlurs.queue.async { [weak self] in
            let blurs = MirrorBlurs(job)
            DispatchQueue.main.async {
                guard let self else { return }
                self.making = false
                guard let now = self.wanted, let kept = self.made, !kept.serves(now) else { return }
                // A picture blurred at once meanwhile is another picture: these are not its blurs
                if job.load == kept.key.load {
                    self.made = blurs
                    self.landed?(blurs.images)
                    if blurs.serves(now) { return }
                }
                self.makeWanted()
            }
        }
    }
}

/// A picture's blurs, one a level, made on any thread.
struct MirrorBlurs {
    /// The pixels, the image's load they came from, and each level's blur in their pixels
    struct Key { let source: CGImage, load: Int, sigmas: [CGFloat] }
    let key: Key, images: [CGImage]

    static let queue = DispatchQueue(label: "exact.mirror-blurs", qos: .userInitiated)

    /// The same pixels, each level within a quarter of the blur asked for
    func serves(_ other: Key) -> Bool {
        key.source === other.source && key.sigmas.count == other.sigmas.count
            && zip(key.sigmas, other.sigmas).allSatisfy { abs($0 - $1) <= 0.25 * $1 }
    }

    /// Each level from the one before, made smaller, as Gaussian blurs compose (one by `a`, then one by `b`, blur by
    /// √(a² + b²)), so only the first reads the whole picture. Each is at most four of its pixels to its blur's sigma,
    /// which the layer draws at the picture's size.
    init(_ key: Key) {
        var images: [CGImage] = [], from = key.source, fromScale: CGFloat = 1, done: CGFloat = 0
        let space = key.source.colorSpace ?? CGColorSpace(name: CGColorSpace.sRGB)!
        for sigma in key.sigmas {
            let scale = min(fromScale, 4 / max(sigma, 0.001))
            let rest = max(0, sigma * sigma - done * done).squareRoot()
            guard let image = Self.blur(from, by: rest * scale, scaledBy: scale / fromScale, space: space) else {
                images.append(images.last ?? key.source); continue
            }
            images.append(image)
            (from, fromScale, done) = (image, scale, sigma)
        }
        self.key = key
        self.images = images
    }

    /// Blurs the pixels' encoded values, as UIKit's blur does: blurred in linear light, a detailed picture's mirror came
    /// out lighter than the picture. A context is safe to share across threads.
    private static let context = CIContext(options: [.cacheIntermediates: false, .workingColorSpace: NSNull(), .outputColorSpace: NSNull()])

    /// `image` made `scale` times its size, then blurred by `sigma` of the new pixels, tagged with the picture's own
    /// colour space, its values untouched
    private static func blur(_ image: CGImage, by sigma: CGFloat, scaledBy scale: CGFloat, space: CGColorSpace) -> CGImage? {
        var input = CIImage(cgImage: image)
        if scale < 1 { input = input.transformed(by: CGAffineTransform(scaleX: scale, y: scale)) }
        let extent = input.extent.integral
        let output = input.clampedToExtent().applyingGaussianBlur(sigma: Double(sigma)).cropped(to: extent)
        return context.createCGImage(output, from: extent, format: .RGBA8, colorSpace: space)
    }
}

/// Where the box, its content box and its picture are placed in the extension (`ExtensionView.place`), and the
/// extension's bounds then, which say how far it reaches past the box.
private struct Placement {
    let shown: CGRect, inner: CGRect, drawn: CGRect, bounds: CGRect
}

/// The box's corners, `path`, on a layer `size` big, or none: a path is not animated, as the box's own is not.
private func corner(_ layer: CALayer, _ path: CGPath?, _ size: CGSize) {
    guard let path else { layer.mask = nil; return }
    let mask = layer.mask as? CAShapeLayer ?? CAShapeLayer()
    CATransaction.begin(); CATransaction.setDisableActions(true)
    mask.path = path
    mask.frame = CGRect(origin: .zero, size: size)
    CATransaction.commit()
    if layer.mask !== mask { layer.mask = mask }
}

/// The box reflected about one edge, or about a corner where two meet: a copy of the box's layers in a layer the
/// box's size, flipped, its levels each masked by how far it is from the edge, and stretched to the extension's edge
/// where that is further than the box's size.
private final class Mirror {
    struct Side: Equatable {
        let x: Int, y: Int // -1 the box's left or top, 1 its right or bottom, 0 not across that axis
        static let all = [Side(x: -1, y: 0), Side(x: 1, y: 0), Side(x: 0, y: -1), Side(x: 0, y: 1),
                          Side(x: -1, y: -1), Side(x: 1, y: -1), Side(x: -1, y: 1), Side(x: 1, y: 1)]
        /// Whether the extension reaches past the box there: across each axis the side names.
        func extends(_ box: CGRect, in view: CGRect) -> Bool {
            let across = { (s: Int, low: CGFloat, high: CGFloat, viewLow: CGFloat, viewHigh: CGFloat) in
                s == 0 || (s < 0 ? low > viewLow + 0.01 : high < viewHigh - 0.01)
            }
            return across(x, box.minX, box.maxX, view.minX, view.maxX) && across(y, box.minY, box.maxY, view.minY, view.maxY)
        }
    }
    /// A level: 0 the sharp mirror, then each blur of `ExtensionView.levels`, drawn by its pieces
    final class Level {
        let index: Int, holder: ExtensionLayer, mask: ExtensionGradient?
        var pieces: [Piece] = []
        init(index: Int, holder: ExtensionLayer, mask: ExtensionGradient?) { (self.index, self.holder, self.mask) = (index, holder, mask) }
    }
    /// A part of a level's copy of the box, stretched as UIKit stretches it there: `clip`, what it shows of the box's
    /// content box, holding the picture
    struct Piece { let clip: ExtensionLayer, picture: ExtensionLayer }

    let side: Side
    let container: ExtensionLayer
    let levels: [Level]
    private weak var owner: UIView?
    /// How many pieces each level's copy is cut into across each axis
    private var counts = (x: 0, y: 0)

    /// A corner shows the most blurred level alone; a side, every level over the sharp mirror.
    init(_ side: Side, owner: UIView, levels count: Int) {
        self.side = side
        self.owner = owner
        let container = ExtensionLayer(owner: owner)
        container.transform = CATransform3DMakeScale(side.x == 0 ? 1 : -1, side.y == 0 ? 1 : -1, 1)
        self.container = container
        let corner = side.x != 0 && side.y != 0
        levels = (corner ? [count] : Array(0...count)).map { i in
            let holder = ExtensionLayer(owner: owner)
            let mask = i > 0 && !corner ? ExtensionGradient(owner: owner) : nil
            if let mask {
                mask.colors = [UIColor.clear.cgColor, UIColor.black.cgColor]
                // Along the axis the side crosses, away from the edge, which is at the copy's near side.
                let along = side.x != 0
                let start = CGPoint(x: along ? (side.x < 0 ? 0 : 1) : 0.5, y: along ? 0.5 : (side.y < 0 ? 0 : 1))
                mask.startPoint = start
                mask.endPoint = CGPoint(x: along ? 1 - start.x : 0.5, y: along ? 0.5 : 1 - start.y)
                holder.mask = mask
            }
            container.addSublayer(holder)
            return Level(index: i, holder: holder, mask: mask)
        }
    }

    /// The mirror placed for `now`. The first time it is stretched across an axis, each level is cut into ten pieces
    /// across it, placed first for `before` without animation to move from there. The mirror keeps them for its
    /// life, so a placement while an animation still moves them never replaces them.
    func place(_ now: Placement, from before: Placement?, masks: [CGFloat], corners: CGPath?) {
        let counts = (x: max(self.counts.x, Self.excess(side.x, now, horizontal: true) > 0 ? Self.pieces : 1),
                      y: max(self.counts.y, Self.excess(side.y, now, horizontal: false) > 0 ? Self.pieces : 1))
        if counts != self.counts, let owner {
            self.counts = counts
            for level in levels {
                for piece in level.pieces { piece.clip.removeFromSuperlayer() }
                level.pieces = (0..<counts.x * counts.y).map { _ in
                    let piece = Piece(clip: ExtensionLayer(owner: owner), picture: ExtensionLayer(owner: owner))
                    piece.clip.masksToBounds = true
                    piece.clip.addSublayer(piece.picture)
                    level.holder.addSublayer(piece.clip)
                    return piece
                }
            }
            if let before { UIView.performWithoutAnimation { layout(before, masks: masks, corners: corners) } }
        }
        layout(now, masks: masks, corners: corners)
    }

    /// UIKit's stretch of a mirror that reaches further than the box's size (the mesh on the mirror layer of
    /// `UIBackgroundExtensionView` and SwiftUI's `backgroundExtensionEffect()`, read on iOS 27.1): the point a
    /// fraction `u` of the box's size from the edge is drawn `u + k·u³` sizes from it, `k` the reach in box sizes less
    /// one, so the mirror is the box's reflection at the edge and its far side meets the extension's edge. UIKit
    /// samples it at tenths, ten straight pieces, as here. UIKit blurs the mirror after stretching it; each level here
    /// is blurred before, so far out its blur is up to `1 + 3k` times wider along the axis.
    private static func stretch(_ u: CGFloat, _ k: CGFloat) -> CGFloat { u + k * u * u * u }
    private static let pieces = 10

    /// How far the extension reaches past the box on side `s` across an axis, in box sizes less one: `k` in
    /// `stretch`, or 0 where it reaches no further than the box's size.
    private static func excess(_ s: Int, _ p: Placement, horizontal: Bool) -> CGFloat {
        let (low, high, size) = horizontal ? (p.shown.minX - p.bounds.minX, p.bounds.maxX - p.shown.maxX, p.shown.width)
                                           : (p.shown.minY - p.bounds.minY, p.bounds.maxY - p.shown.maxY, p.shown.height)
        let reach = s < 0 ? low : s > 0 ? high : 0
        return size > 0 ? max(0, reach / size - 1) : 0
    }

    /// The pieces across one axis of the copy, whose edge is at 0 when `s` is -1 and at the box's size when 1: each
    /// one's span of the box, `low` to `high` in the copy's coordinates, and the map `scale`·x + `offset` that draws
    /// it. An axis the side does not cross has one piece, unstretched.
    private static func spans(_ s: Int, _ p: Placement, horizontal: Bool, count: Int) -> [(low: CGFloat, high: CGFloat, scale: CGFloat, offset: CGFloat)] {
        let size = horizontal ? p.shown.width : p.shown.height, k = excess(s, p, horizontal: horizontal)
        return (0..<count).map { j in
            let u0 = CGFloat(j) / CGFloat(count), u1 = CGFloat(j + 1) / CGFloat(count)
            // Distances from the edge: the piece's span of the box, and where it is drawn
            let (s0, s1) = (size * u0, size * u1), (d0, d1) = (size * stretch(u0, k), size * stretch(u1, k))
            let scale = s1 > s0 ? (d1 - d0) / (s1 - s0) : 1
            let (low, high, drawnLow) = s < 0 ? (s0, s1, d0) : (size - s1, size - s0, size - d1)
            return (low, high, scale, drawnLow - scale * low)
        }
    }

    /// The copy at the box's reflection: its centre reflected about the edge (twice the edge less the centre), the
    /// box's own size and layers inside it, the flip doing the rest. Each piece clips its span of the box's content
    /// box and holds the picture, both mapped by the piece's stretch. The box's corners clip the copy, or, once it is
    /// stretched, each piece, mapped the same way: a mask is drawn offscreen, so an unstretched mirror keeps to one.
    /// Each level's mask runs from the edge, at the copy's near side in its own coordinates, with the level clear to
    /// where the level before it is full and full at its own `masks` distance, and reaches only as far as the next
    /// level is full, past which that level covers it: a gradient draws its stops at a resolution along its length,
    /// and as one mask as long as any mirror the first level's 7 points showed it a third full at the edge.
    private func layout(_ p: Placement, masks: [CGFloat], corners: CGPath?) {
        let shown = p.shown
        let x = side.x < 0 ? 2 * shown.minX - shown.midX : side.x > 0 ? 2 * shown.maxX - shown.midX : shown.midX
        let y = side.y < 0 ? 2 * shown.minY - shown.midY : side.y > 0 ? 2 * shown.maxY - shown.midY : shown.midY
        container.bounds = CGRect(origin: .zero, size: shown.size)
        container.position = CGPoint(x: x, y: y)
        let xs = Self.spans(side.x, p, horizontal: true, count: counts.x), ys = Self.spans(side.y, p, horizontal: false, count: counts.y)
        let inner = p.inner, drawn = p.drawn.offsetBy(dx: p.inner.minX, dy: p.inner.minY)
        let whole = counts == (1, 1)
        corner(container, whole ? corners : nil, shown.size)
        let r = Self.reach
        for level in levels {
            level.holder.frame = container.bounds
            for (i, piece) in level.pieces.enumerated() {
                let a = xs[i % xs.count], b = ys[i / xs.count]
                let left = max(a.low, inner.minX), right = max(left, min(a.high, inner.maxX))
                let top = max(b.low, inner.minY), bottom = max(top, min(b.high, inner.maxY))
                let clip = CGRect(x: a.scale * left + a.offset, y: b.scale * top + b.offset, width: a.scale * (right - left), height: b.scale * (bottom - top))
                piece.clip.frame = clip
                piece.picture.frame = CGRect(x: a.scale * drawn.minX + a.offset - clip.minX, y: b.scale * drawn.minY + b.offset - clip.minY,
                                             width: a.scale * drawn.width, height: b.scale * drawn.height)
                var map = CGAffineTransform(a: a.scale, b: 0, c: 0, d: b.scale, tx: a.offset - clip.minX, ty: b.offset - clip.minY)
                corner(piece.clip, whole ? nil : corners?.copy(using: &map), clip.size)
            }
            guard let mask = level.mask else { continue }
            let i = level.index
            // The edge is the copy's side the box meets: its left (0) for a left mirror, as the flip turns it, else
            // its far side. The mask's length along the axis is fixed, so its stops stay in points as the box
            // resizes; across it, the box's size. The last level's runs past any mirror.
            let w = shown.width, h = shown.height, l = i < masks.count ? masks[i] : r
            switch (side.x, side.y) {
            case (-1, _): mask.frame = CGRect(x: 0, y: 0, width: l, height: h)
            case (1, _): mask.frame = CGRect(x: w - l, y: 0, width: l, height: h)
            case (_, -1): mask.frame = CGRect(x: 0, y: 0, width: w, height: l)
            default: mask.frame = CGRect(x: 0, y: h - l, width: w, height: l)
            }
            let from = i > 1 ? masks[i - 2] : ExtensionView.sharp
            mask.locations = [NSNumber(value: Double(from / l)), NSNumber(value: Double(masks[i - 1] / l))]
        }
    }

    /// How far a level's mask runs from the edge: past any mirror on a phone or a tablet.
    private static let reach: CGFloat = 2048
}

/// A layer of an image's extension that moves and resizes in the animation
/// its owner view's frame is set in, as that view does; its other changes
/// show at once. Core Animation asks a view for its own layer's actions only,
/// never a sublayer's or a mask's: `owner` is asked.
final class ExtensionLayer: CALayer {
    weak var owner: UIView?
    init(owner: UIView?) {
        self.owner = owner
        super.init()
    }
    override init(layer: Any) {
        owner = (layer as? ExtensionLayer)?.owner
        super.init(layer: layer)
    }
    required init?(coder: NSCoder) { nil }
    override func action(forKey event: String) -> (any CAAction)? {
        guard event == "bounds" || event == "position", let owner else { return NSNull() }
        return owner.action(for: self, forKey: event) ?? NSNull()
    }
}

/// A level's mask, which moves in the animation its owner's frame is set in, as `ExtensionLayer` does.
final class ExtensionGradient: CAGradientLayer {
    weak var owner: UIView?
    init(owner: UIView?) {
        self.owner = owner
        super.init()
    }
    override init(layer: Any) {
        owner = (layer as? ExtensionGradient)?.owner
        super.init(layer: layer)
    }
    required init?(coder: NSCoder) { nil }
    override func action(forKey event: String) -> (any CAAction)? {
        guard event == "bounds" || event == "position", let owner else { return NSNull() }
        return owner.action(for: self, forKey: event) ?? NSNull()
    }
}
#endif
