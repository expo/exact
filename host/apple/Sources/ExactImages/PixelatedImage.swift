#if os(macOS)
import AppKit
#else
import UIKit
#endif
import ExactKit

/// The pixelated images whose tiles are installed. Their tiles cover only what
/// the clipping ancestors show, so any ancestor's geometry change refreshes them.
enum PixelatedImages {
    nonisolated(unsafe) static let nodes = NSHashTable<NodeView>.weakObjects()
}

/// Integer-expanded samples stay in layer contents, so Core Animation's
/// final transform performs only the smooth pass, without scaling a view's ink.
final class PixelatedImageLayer: CALayer {
    private var source: CGImage?
    private var enlargement = CGSize.zero
    private var drawn = CGRect.null
    private var visible = CGRect.null
    private var tint: CGColor?

    private func imagePlane(_ r: CGRect, in rect: CGRect) -> CGRect {
        #if os(macOS)
        return r
        #else
        // UIKit's layer plane has y down; the Quartz tiles have y up.
        return CGRect(x: r.minX, y: rect.minY + rect.maxY - r.maxY, width: r.width, height: r.height)
        #endif
    }

    func update(_ image: CGImage, rect: CGRect, visible: CGRect, scale: CGSize, tint: CGColor?) {
        let n = RasterTiles.enlargement(image, rect: rect, scale: scale)
        let visible = imagePlane(visible, in: rect)
        let tiles = RasterTiles.region(image, rect: rect, visible: visible, enlargement: n)
        guard source !== image || enlargement != n || drawn != rect || self.visible != tiles || self.tint != tint else { return }
        source = image; enlargement = n; drawn = rect; self.visible = tiles; self.tint = tint
        sublayers?.forEach { $0.removeFromSuperlayer() }
        RasterTiles.each(image, rect: rect, visible: visible, enlargement: n, tint: tint) { pixels, core, halo in
            let core = imagePlane(core, in: rect), halo = imagePlane(halo, in: rect)
            let tile = CALayer(), picture = CALayer()
            tile.frame = core; tile.masksToBounds = true
            picture.frame = halo.offsetBy(dx: -core.minX, dy: -core.minY)
            picture.contents = pixels
            RasterGeometry.filter(picture, rendering: "auto")
            tile.addSublayer(picture); addSublayer(tile)
        }
    }
}

/// `image-rendering: pixelated` (`ImageLink.pixelated`).
final class PixelatedSampling: PixelatedImaging {
    func apply(_ view: NodeView) -> Bool { view.applyPixelatedSampling() }
    func owns(_ layer: CALayer?) -> Bool { layer is PixelatedImageLayer }
    func refresh(under view: NodeView) { view.refreshPixelatedTiles() }
    func refresh(in presenter: Presenter) { presenter.refreshPixelatedTiles() }
}

extension NodeView {
    /// Returns true when this image's two-pass sampling owns its image layer.
    fileprivate func applyPixelatedSampling() -> Bool {
        guard style["image_rendering"]?.string == "pixelated", kind == "image" else { PixelatedImages.nodes.remove(self); return false }
        #if os(macOS)
        guard let host = layer, !hasSymbolLeaf else { return false }
        let displayScale = window?.backingScaleFactor ?? 1
        #else
        let host = layer
        guard symbolView == nil else { return false }
        let displayScale = traitCollection.displayScale
        #endif
        guard !cssVisibilityHidden, let bitmap = raster?.image ?? flightLook?.stand?.image else {
            imageLayer?.removeFromSuperlayer(); imageLayer = nil; PixelatedImages.nodes.remove(self); return true
        }
        PixelatedImages.nodes.add(self)
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        let pixels = AnimatedRasters.shared.frame(for: self) ?? bitmap.image
        let content = contentBox()
        let rect = flightLook?.image ?? RasterGeometry.rect(natural: bitmap.naturalSize, content: content, fit: style["object_fit"]?.string ?? "fill")
        var root = host
        while let parent = root.superlayer { root = parent }
        let origin = host.convert(CGPoint.zero, to: root)
        let x = host.convert(CGPoint(x: 1, y: 0), to: root), y = host.convert(CGPoint(x: 0, y: 1), to: root)
        let scale = CGSize(width: hypot(x.x - origin.x, x.y - origin.y) * displayScale,
                           height: hypot(y.x - origin.x, y.y - origin.y) * displayScale)
        var visible = bounds.intersection(content)
        var ancestor = superview
        while let view = ancestor {
            #if os(macOS)
            let clips = view is NSClipView || view.clipsToBounds || view.layer?.masksToBounds == true
            let parentLayer = view.layer
            #else
            let clips = view.clipsToBounds
            let parentLayer: CALayer? = view.layer
            #endif
            if (clips || view.superview == nil), let parentLayer { visible = visible.intersection(host.convert(view.bounds, from: parentLayer)) }
            ancestor = view.superview
        }
        let sampling = imageLayer as? PixelatedImageLayer ?? PixelatedImageLayer()
        if imageLayer !== sampling { imageLayer?.removeFromSuperlayer(); imageLayer = sampling; insertBoxSublayer(sampling) }
        sampling.frame = bounds
        let radii = BorderPaint.contentRadii(cornerSizes(in: layoutBounds), outer: layoutBounds, inner: content)
        let mask = sampling.mask as? CAShapeLayer ?? CAShapeLayer()
        mask.frame = bounds
        mask.path = BorderPaint.roundedRect(content, radii, shape: CornerShape(style["corner_shape"]))
        sampling.mask = mask
        sampling.update(pixels, rect: rect, visible: visible, scale: scale, tint: channels("tint_color").map { TextEngine.color($0).cgColor })
        sampling.applyDynamicRange(hdr: bitmap.isHDR, headroom: bitmap.headroom, limit: style["dynamic_range_limit"]?.string)
        return true
    }

    /// This view's own and its descendants' pixelated tiles, after its transform changed.
    fileprivate func refreshPixelatedTiles() {
        guard PixelatedImages.nodes.count > 0 else { return }
        for node in PixelatedImages.nodes.allObjects where node === self || node.isDescendant(of: self) { node.applyImageLayer() }
    }
}

extension Presenter {
    /// Every pixelated image's tiles, after a batch or a scroll moved or resized what clips it.
    fileprivate func refreshPixelatedTiles() {
        guard PixelatedImages.nodes.count > 0 else { return }
        for node in PixelatedImages.nodes.allObjects where node.presenter === self { node.applyImageLayer() }
    }
}
