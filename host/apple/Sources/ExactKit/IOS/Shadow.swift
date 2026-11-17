// The GPU capture (LLP 1014 D3 on a phone; LLP 1008 §9): a shadow tree of
// plain CALayers mirrors the overlay's layer tree — geometry, opacity,
// clipping, and the same `contents` objects, the views' backing stores,
// shared rather than copied — and Core Animation renders that tree on the
// GPU (`CARenderer`) into a Metal texture. The overlay itself stays in the
// window's tree, where UIKit needs it (touches, the keyboard,
// accessibility): a layer is in one tree only, so the tree that is rendered
// is this one. Rasterizing the app on the CPU (`layer.render(in:)`) cost
// 20–25 ms a scroll frame on an iPhone 17 Pro Max; this is what replaces it.
// A nested canvas contributes its readback picture as one layer's contents,
// as it does through `draw` in the CPU capture, which remains the fallback
// where there is no Metal device, and inside an open transaction, where the
// shadow tree cannot be committed (`mirror(_:in:width:height:scale:)`).
import Metal
import QuartzCore
#if os(iOS) || os(tvOS)
import UIKit

package final class Shadow {
    /// The one shadow renderer, or nil where Metal is absent.
    nonisolated(unsafe) static var active: Shadow?
    nonisolated(unsafe) package static let shared: Shadow? = {
        let shadow = Shadow()
        active = shadow
        return shadow
    }()

    /// Update an existing mirror immediately; creation takes the same setter.
    func rank(_ layer: CALayer, _ value: CGFloat) {
        for pass in [handed, readback].compactMap({ $0 }) {
            if let mirror = pass.mirrors[ObjectIdentifier(layer)], mirror.zPosition != value { mirror.zPosition = value }
        }
    }

    let device: MTLDevice
    let queue: MTLCommandQueue

    /// A renderer, the texture it draws, and the tree it draws: one root,
    /// its sublayers the mirror of whatever this pass captured last
    /// (rebuilt per capture; layers kept by the source layer's identity so
    /// their state carries between frames). Each renderer has a root of
    /// its own: with one root under both, the hand-over drew none of the
    /// page once a deck's children had been read back through the other.
    private final class Pass {
        let texture: MTLTexture
        let renderer: CARenderer
        let root = CALayer()
        var mirrors: [ObjectIdentifier: CALayer] = [:]
        var used: Set<ObjectIdentifier> = []
        /// How many times the tree was mirrored: the number its root carries.
        var mirrored: CGFloat = 0
        /// Its own pixels back (`render`); none for the hand-over.
        let readback: MTLBuffer?
        init(_ texture: MTLTexture, queue: MTLCommandQueue, readback: MTLBuffer?) {
            self.texture = texture
            self.readback = readback
            // Under the renderer UIKit's contents draw upside down while the
            // geometry is upright; a flipped root mirrors the geometry too, so
            // the whole picture is upside down — and the top layer's transform
            // mirrors it back (`mirror(_:width:height:scale:)`).
            root.isGeometryFlipped = true
            // The queue matters: the readback blit and the hand-over's wait are
            // ordered after the renderer's work by being on the same one.
            renderer = CARenderer(mtlTexture: texture, options: [Shadow.queueOption: queue])
            renderer.layer = root
        }
    }
    /// The pass of the zero-copy hand-over (`renderTexture`), whose one
    /// texture the module imports once: a second one in turn would make the
    /// module re-import and re-bind every capture, and a surface takes a
    /// new children view as a fresh set — the glass crossfaded on every
    /// capture, a flicker with the alpha in between. The module's reads of
    /// it are complete before it is drawn into again (`gpu_sync`, called by
    /// the capture).
    private var handed: Pass?
    /// The pass read back into a bitmap (`render`), at the last size asked.
    private var readback: Pass?
    /// The pass being mirrored into.
    private var pass: Pass!

    /// The renderer's option for the command queue it renders on — the
    /// constant `kCARendererMetalCommandQueue`, whose Swift name the iOS SDK
    /// does not export: its value read from the framework at run time, so
    /// the renderer's work is on the presenter's queue and a wait on that
    /// queue means what it says. (The option's key was first passed as a
    /// guessed string; the renderer then used its own queue and the module
    /// sampled half-drawn textures — a flicker, and alpha gone thin.)
    static let queueOption: String = {
        if let sym = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "kCARendererMetalCommandQueue") {
            let value = sym.assumingMemoryBound(to: Unmanaged<NSString>.self).pointee.takeUnretainedValue()
            return value as String
        }
        return "kCARendererMetalCommandQueue"
    }()

    private init?() {
        guard let d = MTLCreateSystemDefaultDevice(), let q = d.makeCommandQueue() else { return nil }
        device = d
        queue = q
    }

    /// The view's subtree rendered at `scale` into a Metal texture the module
    /// imports as it is (`gpu_texture_metal`) — no readback, no upload. The
    /// texture stays the presenter's, alive as long as this object, the same
    /// one every time at a size. The renderer's work is complete when this
    /// returns (a wait on the presenter's queue), since the module samples
    /// on its own queue. Nil, with nothing drawn, when the tree could not be
    /// committed (`mirror(_:in:width:height:scale:)`).
    package func renderTexture(_ view: UIView, scale: CGFloat) -> MTLTexture? {
        let w = Int((view.bounds.width * scale).rounded()), h = Int((view.bounds.height * scale).rounded())
        guard w > 0, h > 0 else { return nil }
        if handed?.texture.width != w || handed?.texture.height != h {
            guard let t = texture(width: w, height: h) else { return nil }
            handed = Pass(t, queue: queue, readback: nil)
        }
        guard let handed, mirror(view, in: handed, width: w, height: h, scale: scale) else { return nil }
        draw(handed, width: w, height: h)
        guard let cb = queue.makeCommandBuffer() else { return nil }
        cb.commit()
        cb.waitUntilCompleted()
        return handed.texture
    }

    /// The view's subtree rendered at `scale`: premultiplied RGBA, rows
    /// top-down, into `bitmap` — the same bytes the CPU capture makes.
    /// False, with `bitmap` untouched, when the tree could not be committed
    /// (`mirror(_:in:width:height:scale:)`).
    func render(_ view: UIView, scale: CGFloat, into bitmap: Bitmap) -> Bool {
        let w = bitmap.width, h = bitmap.height
        if readback?.texture.width != w || readback?.texture.height != h {
            guard let t = texture(width: w, height: h), let b = device.makeBuffer(length: w * h * 4, options: .storageModeShared) else { return false }
            readback = Pass(t, queue: queue, readback: b)
        }
        guard let bytes = bitmap.bytes, let pass = readback, let target = pass.readback,
              mirror(view, in: pass, width: w, height: h, scale: scale) else { return false }
        draw(pass, width: w, height: h)
        // The pixels back: a blit on the same queue after the renderer's
        // work, waited for (the per-child textures of a deck, and any host
        // without the zero-copy export).
        guard let cb = queue.makeCommandBuffer(), let blit = cb.makeBlitCommandEncoder() else { return false }
        blit.copy(from: pass.texture, sourceSlice: 0, sourceLevel: 0, sourceOrigin: MTLOrigin(x: 0, y: 0, z: 0), sourceSize: MTLSize(width: w, height: h, depth: 1), to: target, destinationOffset: 0, destinationBytesPerRow: w * 4, destinationBytesPerImage: w * h * 4)
        blit.endEncoding()
        cb.commit()
        cb.waitUntilCompleted()
        memcpy(bytes, target.contents(), w * h * 4)
        return true
    }

    /// The shadow tree brought up to date with the view's layer tree, and
    /// whether Core Animation committed it. The renderer draws the tree as
    /// last committed, and a commit waits until every explicit transaction
    /// open on the thread has ended
    /// (https://developer.apple.com/documentation/quartzcore/catransaction/flush()).
    /// Inside one (a UIKit animation block, a host's transaction around a
    /// batch, another capture's walk) the renderer would draw the tree of an
    /// earlier capture: nothing in a new pass, else the last child of that
    /// size. The root carries this mirror's number, and its presentation
    /// layer, which holds committed values
    /// (https://developer.apple.com/documentation/quartzcore/calayer/presentation()),
    /// has that number only once this tree is the renderer's.
    private func mirror(_ view: UIView, in pass: Pass, width w: Int, height h: Int, scale: CGFloat) -> Bool {
        self.pass = pass
        let root = pass.root
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        pass.used.removeAll(keepingCapacity: true)
        // The renderer maps the root's bounds onto the texture one to one, so
        // the root is in pixels and the mirrored tree (points) is scaled by
        // its own transform, about its centre, to fill it.
        let rootBounds = CGRect(x: 0, y: 0, width: w, height: h)
        if root.bounds != rootBounds { root.bounds = rootBounds; root.position = CGPoint(x: CGFloat(w) / 2, y: CGFloat(h) / 2) }
        // A filtered box shows as its picture (`BoxFilter`), a placed canvas child's too.
        let top = mirror((view as? NodeView)?.filterPicture ?? view.layer, opaque: true)
        let centre = CGPoint(x: CGFloat(w) / 2, y: CGFloat(h) / 2)
        if top.position != centre { top.position = centre }
        // Scaled to the texture and mirrored vertically: the flipped root
        // renders the whole picture upside down, and this mirror — of the
        // whole subtree, contents included — rights it in the texture
        // itself, for a module that samples it as it is.
        let fill = CATransform3DMakeScale(scale, -scale, 1)
        if !CATransform3DEqualToTransform(top.transform, fill) { top.transform = fill }
        if root.sublayers?.first !== top || root.sublayers?.count != 1 { root.sublayers?.forEach { $0.removeFromSuperlayer() }; root.addSublayer(top) }
        // What was not seen this time is dropped from the cache.
        for key in pass.mirrors.keys where !pass.used.contains(key) { pass.mirrors.removeValue(forKey: key) }
        // A property that draws nothing while the root is not rasterized.
        pass.mirrored += 1
        root.rasterizationScale = pass.mirrored
        CATransaction.commit()
        CATransaction.flush()
        return root.presentation()?.rasterizationScale == pass.mirrored
    }

    /// One frame of the renderer into `target`, cleared first (the renderer
    /// composites over what the texture holds — the previous frame,
    /// otherwise, seen as ghosts of every text).
    private func draw(_ pass: Pass, width w: Int, height h: Int) {
        let renderer = pass.renderer
        if let cb = queue.makeCommandBuffer() {
            let clear = MTLRenderPassDescriptor()
            clear.colorAttachments[0].texture = pass.texture
            clear.colorAttachments[0].loadAction = .clear
            clear.colorAttachments[0].storeAction = .store
            clear.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 0)
            cb.makeRenderCommandEncoder(descriptor: clear)?.endEncoding()
            cb.commit()
        }
        renderer.bounds = CGRect(x: 0, y: 0, width: w, height: h)
        renderer.beginFrame(atTime: CACurrentMediaTime(), timeStamp: nil)
        renderer.addUpdate(renderer.bounds)
        renderer.render()
        renderer.endFrame()
    }

    /// A texture the renderer draws into and the module samples.
    private func texture(width w: Int, height h: Int) -> MTLTexture? {
        let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: w, height: h, mipmapped: false)
        d.usage = [.renderTarget, .shaderRead]
        d.storageMode = .private
        return device.makeTexture(descriptor: d)
    }

    /// The shadow of `layer`, its state copied, its sublayers mirrored —
    /// except what the capture does not draw: a Metal layer (a nested
    /// canvas's picture comes by readback instead), and the overlay of a
    /// canvas painted through its surface (its children are in that
    /// surface's texture, not here). `opaque`: the captured root composites
    /// at alpha 0 in the window; here it is painted.
    private func mirror(_ layer: CALayer, opaque: Bool = false) -> CALayer {
        let key = ObjectIdentifier(layer)
        pass.used.insert(key)
        // What is captured is what Core Animation has: a view the batch just
        // invalidated draws now rather than at the display pass, so the
        // capture is not one frame behind it (the logo, as it loaded).
        let redrew = layer.needsDisplay()
        if redrew { layer.displayIfNeeded() }
        let shadow: CALayer
        if let s = pass.mirrors[key], type(of: s) == shadowClass(for: layer) { shadow = s } else { shadow = shadowClass(for: layer).init(); pass.mirrors[key] = shadow }
        // A layer redrawn in place keeps its backing-store object: the copy
        // below compares contents by identity and would leave the shadow
        // holding the same object — which the renderer last snapshotted
        // before the redraw (the mark under Weird Castle's sky: box captured,
        // picture not). Through nil, the set is a real one.
        if redrew { shadow.contents = nil }
        // A layer Core Animation is animating (an SVG draw-in, a pulse, a
        // colour: LLP 1055.000) is captured as it shows, not as its model.
        let shown = layer.animationKeys()?.isEmpty == false ? (layer.presentation() ?? layer) : layer
        copy(shown, to: shadow)
        if let node = layer.delegate as? NodeView { node.setPaintPosition(node.paintZPosition) }
        else { rank(layer, shown.zPosition) }
        // An SVG part's mask (a gradient under its shape) comes along; a
        // placed child's own, not the empty one that hides it on screen.
        if let mask = layer.boxMask {
            let m = mirror(mask)
            if shadow.mask !== m { shadow.mask = m }
        } else if shadow.mask != nil {
            shadow.mask = nil
        }
        if opaque {
            let opacity = Capture.rootOpacity(of: layer)
            if shadow.opacity != opacity { shadow.opacity = opacity }
            if shadow.isHidden { shadow.isHidden = false }
        }
        // A placed child under the captured root is its surface's to draw.
        else if (layer as? NodeLayer)?.drawnElsewhere == true, !shadow.isHidden { shadow.isHidden = true }
        let view = layer.delegate as? UIView
        var children: [CALayer] = []
        if let node = view as? NodeView, node.kind == "canvas" {
            // The canvas's own picture, read back from the module — the last
            // one while nothing on it changed.
            let picture = node.canvases?.picture(of: node)
            if (shadow.contents as AnyObject?) !== picture { shadow.contents = picture }
            if shadow.contentsGravity != .resize { shadow.contentsGravity = .resize }
            for sub in layer.sublayers ?? [] {
                guard let subView = sub.delegate as? UIView, subView !== node.metal else { continue }
                if subView === node.overlay, subView.alpha == 0 { continue }
                children.append(mirror(sub))
            }
        } else {
            for sub in layer.sublayers ?? [] where !(sub is CAMetalLayer) { children.append(mirror(sub)) }
        }
        // The sublayers, re-attached only when the list changed: during a
        // scroll nothing here does.
        let current = shadow.sublayers ?? []
        if current.count != children.count || !zip(current, children).allSatisfy({ $0 === $1 }) {
            current.forEach { $0.removeFromSuperlayer() }
            children.forEach { shadow.addSublayer($0) }
        }
        return shadow
    }

    private func shadowClass(for layer: CALayer) -> CALayer.Type {
        switch layer {
        case is CAShapeLayer: return CAShapeLayer.self
        case is CATextLayer: return CATextLayer.self
        case is CAGradientLayer: return CAGradientLayer.self
        default: return CALayer.self
        }
    }

    /// Everything a plain layer shows, copied where it differs (a set on a
    /// layer costs more than a read; during a scroll only the scroll
    /// container's bounds move); a shape, text, or gradient layer's own
    /// state besides.
    private func copy(_ a: CALayer, to b: CALayer) {
        if b.bounds != a.bounds { b.bounds = a.bounds }
        if b.position != a.position { b.position = a.position }
        if b.anchorPoint != a.anchorPoint { b.anchorPoint = a.anchorPoint }
        if b.anchorPointZ != a.anchorPointZ { b.anchorPointZ = a.anchorPointZ }
        if !CATransform3DEqualToTransform(b.transform, a.transform) { b.transform = a.transform }
        if !CATransform3DEqualToTransform(b.sublayerTransform, a.sublayerTransform) { b.sublayerTransform = a.sublayerTransform }
        if b.isGeometryFlipped != a.isGeometryFlipped { b.isGeometryFlipped = a.isGeometryFlipped }
        if b.isDoubleSided != a.isDoubleSided { b.isDoubleSided = a.isDoubleSided }
        if b.opacity != a.opacity { b.opacity = a.opacity }
        if b.isHidden != a.isHidden { b.isHidden = a.isHidden }
        if b.masksToBounds != a.masksToBounds { b.masksToBounds = a.masksToBounds }
        if b.cornerRadius != a.cornerRadius { b.cornerRadius = a.cornerRadius }
        // A new layer's curve is not the box's: on iOS 27 it is continuous.
        if b.cornerCurve != a.cornerCurve { b.cornerCurve = a.cornerCurve }
        if b.maskedCorners != a.maskedCorners { b.maskedCorners = a.maskedCorners }
        if b.backgroundColor != a.backgroundColor { b.backgroundColor = a.backgroundColor }
        if b.borderWidth != a.borderWidth { b.borderWidth = a.borderWidth }
        if b.borderColor != a.borderColor { b.borderColor = a.borderColor }
        if (b.contents as AnyObject?) !== (a.contents as AnyObject?) { b.contents = a.contents }
        if b.contentsRect != a.contentsRect { b.contentsRect = a.contentsRect }
        if b.contentsCenter != a.contentsCenter { b.contentsCenter = a.contentsCenter }
        if b.contentsScale != a.contentsScale { b.contentsScale = a.contentsScale }
        if b.contentsGravity != a.contentsGravity { b.contentsGravity = a.contentsGravity }
        if b.isOpaque != a.isOpaque { b.isOpaque = a.isOpaque }
        // A shadow: a `box-shadow`'s (`BoxShadow.swift`), a lone
        // `drop-shadow()`'s on its box (`BoxFilter.swift`), a `text-shadow`'s
        // on its ink; and the mask that keeps a `box-shadow` outside the box.
        if a is ShadowCaster || a.shadowOpacity > 0 || b.shadowOpacity > 0 {
            b.shadowPath = a.shadowPath; b.shadowColor = a.shadowColor; b.shadowOffset = a.shadowOffset
            b.shadowRadius = a.shadowRadius; b.shadowOpacity = a.shadowOpacity
        }
        if a is ShadowCaster {
            let mask = a.mask.map { mirror($0) }
            if b.mask !== mask { b.mask = mask }
        }
        if let s = a as? CAShapeLayer, let t = b as? CAShapeLayer {
            t.path = s.path; t.fillColor = s.fillColor; t.strokeColor = s.strokeColor; t.lineWidth = s.lineWidth
            t.lineCap = s.lineCap; t.lineJoin = s.lineJoin; t.fillRule = s.fillRule; t.strokeStart = s.strokeStart; t.strokeEnd = s.strokeEnd; t.lineDashPattern = s.lineDashPattern
            t.lineDashPhase = s.lineDashPhase; t.miterLimit = s.miterLimit
        }
        if let s = a as? CATextLayer, let t = b as? CATextLayer {
            t.string = s.string; t.font = s.font; t.fontSize = s.fontSize; t.foregroundColor = s.foregroundColor
            t.alignmentMode = s.alignmentMode; t.isWrapped = s.isWrapped; t.truncationMode = s.truncationMode; t.allowsFontSubpixelQuantization = s.allowsFontSubpixelQuantization
        }
        if let s = a as? CAGradientLayer, let t = b as? CAGradientLayer {
            t.colors = s.colors; t.locations = s.locations; t.startPoint = s.startPoint; t.endPoint = s.endPoint; t.type = s.type
        }
    }
}
#endif
