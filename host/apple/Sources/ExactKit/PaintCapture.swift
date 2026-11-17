// An ordered copy for Core Graphics, which ignores a layer's zPosition.
// Live views and layers stay in logical order for input and accessibility.
// On the Mac, draw a node's AppKit backing stores again into owned pixels:
// sharing them can blank the live view even when the copy is never committed.
// Other contents (images and surfaces) may be shared.
#if os(macOS)
import AppKit
#else
import UIKit
#endif

extension Capture {
    /// `branchProbe` counts membership lookups so tests can bound work without timing.
    static func paintOrderLayer(of view: PaintView, root: CALayer, branchProbe: (() -> Void)? = nil) -> CALayer {
        var owners: [ObjectIdentifier: PaintView] = [:]
        func index(_ v: PaintView) {
            #if os(macOS)
            if let layer = v.layer { owners[ObjectIdentifier(layer)] = v }
            #else
            owners[ObjectIdentifier(v.layer)] = v
            #endif
            for child in v.subviews { index(child) }
        }
        index(view)
        #if os(macOS)
        // Route each view branch through its live intermediate layers, once.
        // They carry geometry and clipping even though no view owns them.
        var branches: [ObjectIdentifier: [CALayer]] = [:]
        var seen: [ObjectIdentifier: Set<ObjectIdentifier>] = [:]
        for owner in owners.values {
            guard let parent = owner.layer else { continue }
            for child in NodeView.hitOrder(owner.subviews).reversed().compactMap(\.layer) {
                var path = [child]
                while let next = path.last?.superlayer, next !== parent { path.append(next) }
                if path.last?.superlayer !== parent { path = [child] }
                var container = parent
                for branch in path.reversed() {
                    let key = ObjectIdentifier(container)
                    branchProbe?()
                    if seen[key, default: []].insert(ObjectIdentifier(branch)).inserted {
                        branches[key, default: []].append(branch)
                    }
                    container = branch
                }
            }
        }
        func picture(_ image: CGImage, in bounds: CGRect) -> CALayer {
            let layer = CALayer()
            layer.frame = bounds
            layer.contents = image
            return layer
        }
        #endif
        func overlay(_ animation: CAAnimation, from presentation: CALayer, onto layer: CALayer) {
            if let group = animation as? CAAnimationGroup {
                for child in group.animations ?? [] { overlay(child, from: presentation, onto: layer) }
            } else if let property = animation as? CAPropertyAnimation, let keyPath = property.keyPath {
                // Rank and topology stay model-owned. The animation's storage
                // key is arbitrary; keyPath names what it actually animates,
                // including components such as transform.translation.x.
                let key = String(keyPath.prefix { $0 != "." })
                guard !["zPosition", "mask", "sublayers"].contains(key) else { return }
                layer.setValue(presentation.value(forKeyPath: keyPath), forKeyPath: keyPath)
            }
        }
        func copy(_ source: CALayer, inheritedFlip: Bool = false, inheritedScale: CGFloat = 2) -> CALayer {
            source.displayIfNeeded()
            // Start with the model: presentation does not yet contain writes
            // in the current transaction, even on a layer animating another key.
            let values = source
            let layer = values.valueCopy()
            #if os(macOS)
            let flipped = source.contentsAreFlipped()
            // Contents inherit their orientation through the live ancestors;
            // the copied branch must preserve it in its new ancestry.
            layer.isGeometryFlipped = flipped != inheritedFlip
            #else
            let flipped = false
            let scale = inheritedScale
            layer.isGeometryFlipped = values.isGeometryFlipped
            #endif
            layer.isHidden = values.isHidden
            if let keys = source.animationKeys(), !keys.isEmpty, let presentation = source.presentation() {
                for key in keys {
                    if let animation = source.animation(forKey: key) { overlay(animation, from: presentation, onto: layer) }
                }
            }
            #if os(macOS)
            let scale = owners[ObjectIdentifier(source)]?.pixelScale ?? inheritedScale
            // A node's own drawing, its backing store and the content layers
            // AppKit adds for ink past its bounds, drawn again: a copy that
            // shares them, even uncommitted, can leave the live view showing
            // nothing at a fractional device scale (a page zoom) until it is
            // laid out again, however often it draws.
            let redrawn: (image: CGImage, box: CGRect)? = {
                guard let node = owners[ObjectIdentifier(source)] as? NodeView else { return nil }
                let stores = (source.sublayers ?? []).filter(Capture.borrowsBackingStore)
                guard Capture.borrowsBackingStore(source) || !stores.isEmpty else { return nil }
                // Out to whole device pixels, so the picture is not resampled into its box.
                let ink = stores.reduce(node.bounds) { $0.union($1.frame) }
                let pixels = CGRect(x: ink.minX * scale, y: ink.minY * scale, width: ink.width * scale, height: ink.height * scale)
                    .insetBy(dx: 0.001, dy: 0.001).integral
                let box = CGRect(x: pixels.minX / scale, y: pixels.minY / scale, width: pixels.width / scale, height: pixels.height / scale)
                return Capture.drawing(of: node, in: box, scale: scale).map { ($0, box) }
            }()
            let ownStore = redrawn != nil && Capture.borrowsBackingStore(source)
            var drawn = redrawn.map { picture($0.image, in: $0.box) }
            if let shadow = Capture.shadowPicture(of: layer, flipped: flipped, scale: scale) {
                layer.contents = shadow
                layer.shadowOpacity = 0
            } else if let shadow = Capture.contentShadowPicture(of: layer, redrawn: ownStore ? redrawn : nil, flipped: flipped, scale: scale) {
                layer.contents = nil
                layer.shadowOpacity = 0
                // The picture holds the ink the shadow is cast from: the
                // drawing made again, or the contents with the box's paint.
                if ownStore { drawn = nil } else { layer.backgroundColor = nil; layer.borderWidth = 0 }
                layer.addSublayer(shadow)
            }
            if ownStore { layer.contents = nil }
            if let drawn { layer.addSublayer(drawn) }
            #endif
            layer.mask = source.mask.map { copy($0, inheritedFlip: flipped, inheritedScale: scale) }
            // Order is explicit: negative ranks still follow the parent's
            // own fill/gradient layers. Authored 3D transforms stay intact.
            let owner = owners[ObjectIdentifier(source)]
            #if os(macOS)
            var children = branches[ObjectIdentifier(source)] ?? []
            // The CPU renderer cannot replay glass content portals. Paint
            // its public content view above the copied effect layers.
            if #available(macOS 26, *), let glass = owner as? NSGlassEffectView, let content = glass.contentView?.layer {
                var branch = content
                while let parent = branch.superlayer, parent !== source { branch = parent }
                if branch.superlayer === source, let at = children.firstIndex(where: { $0 === branch }) {
                    children.remove(at: at); children.append(branch)
                }
            }
            #else
            // Flat leaves have no UIView: merge their tree slots with the
            // view branches before sorting, so they cover negative ranks.
            let viewLayers = Set((owner?.subviews ?? []).map { ObjectIdentifier($0.layer) })
            let children = (source.sublayers ?? []).enumerated().filter {
                viewLayers.contains(ObjectIdentifier($0.element)) || $0.element.flatHit != nil
            }.sorted {
                let a = $0.element.zPosition, b = $1.element.zPosition
                return a != b ? a < b : $0.offset < $1.offset
            }.map(\.element)
            #endif
            let childSet = Set(children.map(ObjectIdentifier.init))
            let replacements = Set(children.flatMap { child -> [ObjectIdentifier] in
                guard let node = owners[ObjectIdentifier(child)] as? NodeView else { return [] }
                #if os(macOS)
                let mirrors = [node.boxFilter?.picture, node.shadowStand]
                #else
                let mirrors = [node.boxFilter?.picture]
                #endif
                return mirrors.compactMap { mirror in
                    mirror.flatMap { $0.superlayer === source ? ObjectIdentifier($0) : nil }
                }
            })
            // Own contents and decorations first: shadow casters, fills,
            // text rasters, Canvas 2D pictures. Each view branch is below.
            #if os(macOS)
            let redrawnStore = { (sub: CALayer) in redrawn != nil && Capture.borrowsBackingStore(sub) }
            #else
            let redrawnStore = { (_: CALayer) in false }
            #endif
            let own = (source.sublayers ?? []).filter {
                !childSet.contains(ObjectIdentifier($0)) && !replacements.contains(ObjectIdentifier($0)) && !redrawnStore($0)
            }
            #if os(macOS)
            // A decoration the Mac raises over the node's own views (a
            // press's wash, PressLookMac.swift) is painted after them.
            let raised = own.filter { $0.zPosition >= 1 }
            #else
            let raised: [CALayer] = []
            #endif
            for sub in own where !raised.contains(where: { $0 === sub }) {
                layer.addSublayer(copy(sub, inheritedFlip: flipped, inheritedScale: scale))
            }
            #if os(macOS)
            if let node = owner as? NodeView {
                // Metal and remote WebKit layers do not render into a CPU
                // context. Keep their existing readback/snapshot providers.
                if node.kind == "canvas", let image = node.canvases?.readback(view: node)?.cgImage {
                    layer.addSublayer(picture(image, in: node.bounds))
                }
                if !node.cssVisibilityHidden, let image = web[ObjectIdentifier(node)]?.cgImage(forProposedRect: nil, context: nil, hints: nil) {
                    layer.addSublayer(picture(image, in: node.bounds))
                }
            }
            #endif
            for child in children {
                #if os(macOS)
                // A backdrop box's shadow stands beside it to escape the
                // backdrop clip. It keeps the box's place among siblings.
                if let node = owners[ObjectIdentifier(child)] as? NodeView,
                   let stand = node.shadowStand, stand.superlayer === source {
                    layer.addSublayer(copy(stand, inheritedFlip: flipped, inheritedScale: scale))
                }
                #endif
                // A filtered node's picture already includes its subtree.
                // Drawing the masked source too would duplicate it.
                if let node = owners[ObjectIdentifier(child)] as? NodeView, let filter = node.boxFilter,
                   filter.picture.superlayer === source {
                    layer.addSublayer(copy(filter.picture, inheritedFlip: flipped, inheritedScale: scale))
                } else { layer.addSublayer(copy(child, inheritedFlip: flipped, inheritedScale: scale)) }
            }
            for sub in raised { layer.addSublayer(copy(sub, inheritedFlip: flipped, inheritedScale: scale)) }
            return layer
        }
        #if os(macOS)
        return copy(root, inheritedScale: view.pixelScale ?? 2)
        #else
        return copy(root)
        #endif
    }

    #if os(macOS)
    /// Whether a layer shows a backing store AppKit drew a view into, not a
    /// picture of Exact's (an image, a surface).
    static func borrowsBackingStore(_ layer: CALayer) -> Bool {
        guard let contents = layer.contents else { return false }
        let type = CFGetTypeID(contents as CFTypeRef)
        return type != CGImage.typeID && type != IOSurfaceGetTypeID() && !(contents is NSImage)
    }

    /// What a node's `draw(_:)` paints in `box` (its own coordinates, whole
    /// device pixels), at `scale` device pixels per point, its first row at
    /// the top. AppKit's context for a bitmap of `box.size` points keeps a
    /// shadow's offset and blur in points, as a view's backing store does;
    /// a bare `CGContext` takes them in device pixels.
    static func drawing(of node: NodeView, in box: CGRect, scale: CGFloat) -> CGImage? {
        let width = Int((box.width * scale).rounded()), height = Int((box.height * scale).rounded())
        guard width > 0, height > 0,
              let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8,
                                         samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                         bytesPerRow: width * 4, bitsPerPixel: 32)?.retagging(with: .sRGB) else { return nil }
        rep.size = box.size
        guard let bitmap = NSGraphicsContext(bitmapImageRep: rep) else { return nil }
        let ctx = bitmap.cgContext
        if node.isFlipped { ctx.translateBy(x: 0, y: box.height); ctx.scaleBy(x: 1, y: -1) }
        ctx.translateBy(x: -box.minX, y: -box.minY)
        let redrawing = redrawingBackingStore
        redrawingBackingStore = true
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: node.isFlipped)
        // AppKit draws a view under its effective appearance; outside its
        // display pass the current one is the app's, so a dynamic colour
        // would resolve for the app rather than the window.
        node.effectiveAppearance.performAsCurrentDrawingAppearance { node.draw(box) }
        NSGraphicsContext.restoreGraphicsState()
        redrawingBackingStore = redrawing
        bitmap.flushGraphics()
        return rep.cgImage
    }
    #endif
}
