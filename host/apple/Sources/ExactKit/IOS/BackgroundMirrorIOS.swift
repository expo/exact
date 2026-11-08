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
// Which sides reach the route's edge is decided from the box's layout, not
// from a safe area UIKit gives before that layout lands
// (`NodeView.extensionFrame`).
#if os(iOS) || os(tvOS)
import CoreImage
import UIKit

/// An image's background extension (`NodeView.syncBackgroundExtension`): the image's box, and its mirrors in the
/// extension's reach.
final class ExtensionView: UIView {
    /// The image's box, which draws its picture and clips it to the image's corners.
    let content = UIView()
    private let box = ExtensionLayer(owner: nil), picture = ExtensionLayer(owner: nil)
    private var mirrors: [Mirror] = []
    /// Where the box, its content box and its picture were last placed.
    private var placed: (shown: CGRect, inner: CGRect, drawn: CGRect)?
    /// A later `prune` is scheduled: one at a time, however often the box is placed meanwhile.
    private var pruneScheduled = false
    /// The sides the extension reaches past the box, and the box's layout they were judged on
    /// (`NodeView.syncBackgroundExtension`).
    var sides: UIRectEdge = []
    var laidOut: CGRect?
    /// The blurred pictures the mirrors draw: their source and how blurred each is, in its pixels.
    private var blurred: (source: CGImage, sigmas: [CGFloat], images: [CGImage])?

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
    /// edges. Called inside the animation that moves the image, each layer takes it.
    func place(_ shown: CGRect, inner: CGRect, drawn: CGRect, pixels: CGImage, tint: CGColor?, corners: CGPath?) {
        content.frame = shown
        corner(content.layer, corners, shown.size)
        box.frame = inner
        picture.frame = drawn
        show(pixels, tint: tint, in: picture, owner: content)
        // A side the box comes to has a mirror placed first where the box was, to move from there; one it leaves
        // keeps its mirror while the box still moves, as what it fills narrows to nothing (`prune`).
        let sides = Mirror.Side.all.filter { side in side.extends(shown, in: bounds) || mirrors.contains { $0.side == side } }
        let before = placed
        placed = (shown, inner, drawn)
        defer { prune() }
        if mirrors.map(\.side) != sides {
            mirrors = sides.map { side in
                if let kept = mirrors.first(where: { $0.side == side }) { return kept }
                let made = Mirror(side, owner: self, levels: Self.levels.count)
                layer.insertSublayer(made.container, below: content.layer)
                if let before { UIView.performWithoutAnimation { made.place(before.shown, inner: before.inner, drawn: before.drawn, masks: Self.levels.map(\.full)) } }
                return made
            }
        }
        guard !mirrors.isEmpty else { return }
        let images = blurs(of: pixels, drawn: drawn)
        for mirror in mirrors {
            mirror.place(shown, inner: inner, drawn: drawn, masks: Self.levels.map(\.full))
            corner(mirror.container, corners, shown.size)
            for level in mirror.levels {
                show(level.index == 0 ? pixels : images[level.index - 1], tint: tint, in: level.picture, owner: self)
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

    /// The picture blurred to each level, kept while its source and its scale on screen hold: a blur of a quarter more
    /// or less than the level's, once the picture is shown at another size, is made again.
    private func blurs(of pixels: CGImage, drawn: CGRect) -> [CGImage] {
        let perPoint = drawn.width > 0 ? CGFloat(pixels.width) / drawn.width : 1
        let sigmas = Self.levels.map { $0.sigma * perPoint }
        if let kept = blurred, kept.source === pixels, zip(kept.sigmas, sigmas).allSatisfy({ abs($0 - $1) <= 0.25 * $1 }) { return kept.images }
        let images = sigmas.map { Self.blur(pixels, sigma: $0) ?? pixels }
        blurred = (pixels, sigmas, images)
        return images
    }

    /// Blurs the pixels' encoded values, as UIKit's blur does: blurred in linear light, a detailed picture's mirror came
    /// out lighter than the picture
    private static let context = CIContext(options: [.cacheIntermediates: false, .workingColorSpace: NSNull(), .outputColorSpace: NSNull()])

    /// `image` blurred by `sigma` of its pixels, made no larger than that blur needs: at most four of its pixels to
    /// each of the blur's sigma, which the layer draws at the picture's size.
    static func blur(_ image: CGImage, sigma: CGFloat) -> CGImage? {
        let scale = min(1, 4 / max(sigma, 0.001))
        var input = CIImage(cgImage: image)
        if scale < 1 { input = input.transformed(by: CGAffineTransform(scaleX: scale, y: scale)) }
        let extent = input.extent.integral
        let output = input.clampedToExtent().applyingGaussianBlur(sigma: Double(sigma * scale)).cropped(to: extent)
        // Tagged with the picture's own colour space, its values untouched
        return context.createCGImage(output, from: extent, format: .RGBA8, colorSpace: image.colorSpace ?? CGColorSpace(name: CGColorSpace.sRGB)!)
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

    /// The box's corners on a layer the box's size, or none: a path is not animated, as the box's own is not.
    private func corner(_ layer: CALayer, _ path: CGPath?, _ size: CGSize) {
        guard let path else { layer.mask = nil; return }
        let mask = layer.mask as? CAShapeLayer ?? CAShapeLayer()
        CATransaction.begin(); CATransaction.setDisableActions(true)
        mask.path = path
        mask.frame = CGRect(origin: .zero, size: size)
        CATransaction.commit()
        if layer.mask !== mask { layer.mask = mask }
    }
}

/// The box reflected about one edge, or about a corner where two meet: a copy of the box's layers in a layer the
/// box's size, flipped, its levels each masked by how far it is from the edge.
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
    /// A level: 0 the sharp mirror, then each blur of `ExtensionView.levels`
    struct Level { let index: Int, holder: ExtensionLayer, box: ExtensionLayer, picture: ExtensionLayer, mask: ExtensionGradient? }

    let side: Side
    let container: ExtensionLayer
    let levels: [Level]

    /// A corner shows the most blurred level alone; a side, every level over the sharp mirror.
    init(_ side: Side, owner: UIView, levels count: Int) {
        self.side = side
        let container = ExtensionLayer(owner: owner)
        container.transform = CATransform3DMakeScale(side.x == 0 ? 1 : -1, side.y == 0 ? 1 : -1, 1)
        self.container = container
        let corner = side.x != 0 && side.y != 0
        levels = (corner ? [count] : Array(0...count)).map { i in
            let holder = ExtensionLayer(owner: owner), box = ExtensionLayer(owner: owner), picture = ExtensionLayer(owner: owner)
            box.masksToBounds = true
            box.addSublayer(picture)
            holder.addSublayer(box)
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
            return Level(index: i, holder: holder, box: box, picture: picture, mask: mask)
        }
    }

    /// The copy at the box's reflection: its centre reflected about the edge (twice the edge less the centre), the
    /// box's own size and layers inside it, the flip doing the rest. Each level's mask runs from the edge, at the
    /// copy's near side in its own coordinates, with the level clear to where the level before it is full and full
    /// at its own `masks` distance, and reaches only as far as the next level is full, past which that level covers
    /// it: a gradient draws its stops at a resolution along its length, and as one mask as long as any mirror the
    /// first level's 7 points showed it a third full at the edge.
    func place(_ shown: CGRect, inner: CGRect, drawn: CGRect, masks: [CGFloat]) {
        let x = side.x < 0 ? 2 * shown.minX - shown.midX : side.x > 0 ? 2 * shown.maxX - shown.midX : shown.midX
        let y = side.y < 0 ? 2 * shown.minY - shown.midY : side.y > 0 ? 2 * shown.maxY - shown.midY : shown.midY
        container.bounds = CGRect(origin: .zero, size: shown.size)
        container.position = CGPoint(x: x, y: y)
        let r = Self.reach
        for level in levels {
            level.holder.frame = container.bounds
            level.box.frame = inner
            level.picture.frame = drawn
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
