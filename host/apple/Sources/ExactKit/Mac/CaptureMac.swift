// Agent and canvas pictures follow the same sibling order as input.
// Copy the layer tree, never reorder the live views (Tab and AX tree order).
#if os(macOS)
import AppKit

/// A view's subtree as pixels (LLP 1014 D3).
package enum Capture {
    /// A capture is drawing: its draws are not repaints (D4 b).
    nonisolated(unsafe) static var capturing = false
    /// Guest pictures for this turn, by the node that draws each (two
    /// sessions' ids can be the same); the remote platform views are hidden
    /// while their owning nodes draw these (@ref LLP 1020 D4/D6, `withGuests`).
    nonisolated(unsafe) static var web: [ObjectIdentifier: ExactWebImage] = [:]

    /// The subtree painted at `scale`: premultiplied RGBA, rows top-down,
    /// `pixelsWide * 4` bytes per row, transparent where nothing painted.
    package static func bitmap(of view: NSView, scale: CGFloat, prepared: Bool = false) -> NSBitmapImageRep? {
        if !prepared { prepareBackingLayers(of: view) }
        // A subtree painted through its canvas composites at alpha 0; paint
        // it opaque into the bitmap regardless.
        let alpha = view.alphaValue
        view.alphaValue = 1
        // A canvas nested under this one that is painted through its own
        // surface: its picture comes by readback (its draw), not from its
        // overlay's views, so those are hidden for the duration.
        var hidden: [NSView] = []
        func hide(_ v: NSView) {
            for s in v.subviews {
                if let n = s as? NodeView, n.placement != nil, !n.isHidden { n.isHidden = true; hidden.append(n); continue }
                if let n = s as? NodeView, let o = n.overlay, o.alphaValue == 0, !o.isHidden { o.isHidden = true; hidden.append(o); continue }
                hide(s)
            }
        }
        hide(view)
        let rep = paintOrderBitmap(of: view, scale: scale, prepared: true)
        for o in hidden { o.isHidden = false }
        view.alphaValue = alpha
        return rep
    }
}

extension Capture {
    /// AppKit attaches new backing layers at the window display boundary.
    /// Do this once per upload, before Capture.bitmap changes visibility.
    package static func prepareBackingLayers(of view: NSView) {
        view.layoutSubtreeIfNeeded()
        if view.layer == nil, (view as? NSTextView)?.isFieldEditor != true { view.wantsLayer = true }
        func unattached(_ v: NSView) -> Bool {
            // The installed editor inherits backing from its field. Opting it
            // into backing explicitly can interrupt AppKit's editing session.
            if (v as? NSTextView)?.isFieldEditor == true { return false }
            guard let layer = v.layer else { return true }
            if v.superview != nil, layer.superlayer == nil { return true }
            return v.subviews.contains(where: unattached)
        }
        if unattached(view) { view.window?.displayIfNeeded() }
    }

    static func paintOrderBitmap(of view: NSView, scale: CGFloat? = nil, prepared: Bool = false) -> NSBitmapImageRep? {
        if !prepared { prepareBackingLayers(of: view) }
        view.displayIfNeeded()
        let scale = scale ?? view.pixelScale ?? 2
        guard let root = view.layer else { return nil }
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        let tree = paintOrderLayer(of: view, root: root)
        defer { releasePaintCopy(tree) }
        guard let image = picture(of: tree, size: view.bounds.size, scale: scale) else { return nil }
        let rep = NSBitmapImageRep(cgImage: image)
        rep.size = view.bounds.size
        return rep
    }

    /// Detached copies still participate in the current transaction. Drop
    /// their borrowed backing stores before commit so AppKit retains its ink.
    static func releasePaintCopy(_ tree: CALayer) {
        tree.contents = nil
        if let mask = tree.mask { releasePaintCopy(mask) }
        for child in tree.sublayers ?? [] { releasePaintCopy(child) }
    }

    /// A layer tree drawn into a bitmap of `size` points, its first row at
    /// the top, as AppKit's `cacheDisplay` has it.
    static func picture(of tree: CALayer, size: CGSize, scale: CGFloat) -> CGImage? {
        let width = Int((size.width * scale).rounded()), height = Int((size.height * scale).rounded())
        guard width > 0, height > 0,
              let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                                  bytesPerRow: width * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        // CA renders the root in bottom-up Quartz space.
        ctx.translateBy(x: 0, y: CGFloat(height))
        ctx.scaleBy(x: scale, y: -scale)
        tree.render(in: ctx)
        return ctx.makeImage()
    }
}

extension Capture {
    /// `body` with each session's guest pictures (its iframes' and native views'
    /// snapshots) in `web` for their nodes to draw, and those live views hidden,
    /// which a capture cannot draw; put back after.
    static func withGuests<T>(of sessions: [ExactSession], _ body: () -> T) -> T {
        var hidden: [(NSView, Bool)] = []
        for session in sessions {
            let views = session.presenter.views
            for (id, image) in session.webviews.snapshots().merging(session.natives.snapshots(), uniquingKeysWith: { web, _ in web }) {
                if let node = views[id] { web[ObjectIdentifier(node)] = image }
            }
            hidden += (views.values.compactMap(\.web) + session.natives.snapshotViews).map { ($0, $0.isHidden) }
        }
        hidden.forEach { $0.0.isHidden = true }
        defer { hidden.forEach { $0.0.isHidden = $0.1 }; web = [:] }
        return body()
    }
}

extension Agent {
    /// Let the window server composite readbacks at the Metal layers' places.
    /// A CPU capture of the whole content loses backdrop filters and materials.
    static func canvasReadbacks(in window: NSWindow) -> (painted: Bool, restore: () -> Void) {
        var replacements: [(CALayer, Bool, CALayer)] = []
        CATransaction.begin(); CATransaction.setDisableActions(true)
        func walk(_ view: NSView) {
            guard !view.isHiddenOrHasHiddenAncestor else { return }
            if let node = view as? NodeView, node.kind == "canvas",
               let image = node.canvases?.readback(view: node)?.cgImage,
               let metal = node.metal?.layer, let parent = metal.superlayer {
                let picture = metal.valueCopy()
                picture.contents = image
                picture.zPosition = metal.zPosition
                picture.isHidden = metal.isHidden
                parent.insertSublayer(picture, above: metal)
                replacements.append((metal, metal.isHidden, picture))
                metal.isHidden = true
            }
            view.subviews.forEach(walk)
        }
        var seen: Set<Int> = []
        func visit(_ window: NSWindow) {
            guard seen.insert(window.windowNumber).inserted else { return }
            if let content = window.contentView { walk(content) }
            for child in ([window.attachedSheet].compactMap { $0 } + (window.childWindows ?? [])) where child.isVisible { visit(child) }
        }
        visit(window)
        CATransaction.commit()
        return (!replacements.isEmpty, {
            CATransaction.begin(); CATransaction.setDisableActions(true)
            for (metal, hidden, picture) in replacements {
                metal.isHidden = hidden
                picture.removeFromSuperlayer()
            }
            CATransaction.commit()
        })
    }

    static func drawingWindows(_ windows: [(CGRect, CGImage)], over image: CGImage, in frame: CGRect) -> CGImage {
        guard frame.width > 0,
              let context = CGContext(data: nil, width: image.width, height: image.height, bitsPerComponent: 8, bytesPerRow: 0,
                                      space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(),
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return image }
        let scale = CGFloat(image.width) / frame.width
        context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        for (child, picture) in windows {
            context.draw(picture, in: CGRect(x: (child.minX - frame.minX) * scale, y: (child.minY - frame.minY) * scale,
                                             width: child.width * scale, height: child.height * scale))
        }
        return context.makeImage() ?? image
    }
}
#endif
