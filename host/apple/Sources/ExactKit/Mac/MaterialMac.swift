// A node's material on macOS (LLP 1001 §1): a glass or a blur behind its
// content, which AppKit draws and adapts. Its frame, and a grouped glass's
// transform, are `sizeMaterial`'s (`GlassGeometryMac.swift`).
#if os(macOS)
import AppKit
import CoreImage

/// A material paints, but never supplies a new hit target or focus owner.
private final class MaterialContent: NSView {
    weak var owner: NodeView?
    override func draw(_ dirtyRect: NSRect) {
        guard let owner, !owner.cssVisibilityHidden else { return }
        let path = owner.roundedPath(in: bounds)
        if owner.standsOnSheetGround || owner.standsOnSplitGround {
            if let ctx = NSGraphicsContext.current?.cgContext { owner.paintBackground(ctx, border: path.cgPath, color: false) }
        } else {
            owner.drawBackground(path, rounded: owner.cornerRadii(in: bounds).contains { $0 > 0 })
        }
        if owner.materialPaintsBorder { owner.drawBoxBorder() }
    }
    override var isFlipped: Bool { true }
    override func hitTest(_ point: NSPoint) -> NSView? {
        let hit = raisedHit(super.hitTest(point), point)
        return hit === self ? nil : hit
    }
}

@available(macOS 26.0, *)
private final class GlassBackground: NSGlassEffectView {
    weak var owner: NodeView?
    override func layout() {
        super.layout()
        guard let owner, owner.glassGeometry != nil, let content = contentView else { return }
        let frame = CGRect(origin: .zero, size: bounds.size)
        if content.frame != frame { content.frame = frame }
        if frame.width > 0, frame.height > 0 {
            let inner = CGRect(origin: .zero, size: owner.materialBox.size)
            if content.bounds != inner { content.bounds = inner }
        }
    }
    override func hitTest(_ point: NSPoint) -> NSView? {
        contentView?.hitTest(convert(point, from: superview))
    }
}

private final class BlurBackground: NSVisualEffectView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        let hit = raisedHit(super.hitTest(point), point)
        return hit === self ? nil : hit
    }
}

extension NodeView {
    // @ref LLP 1001 §1 — two semantic materials, not sampled blur constants.
    // AppKit owns accessibility/appearance adaptation, including Reduce
    // Transparency and Increase Contrast; do not freeze the effective appearance.
    var appliedMaterial: String {
        guard let materialView, materialView.superview === self || (glassIsolation != nil && materialView.superview?.superview === glassIsolation) else {
            if let filters = layer?.backgroundFilters, !filters.isEmpty {
                let effects = filters.compactMap { ($0 as? CIFilter)?.name }
                    .filter { $0 == "CIGaussianBlur" || $0 == "CIColorMatrix" }
                return "backgroundFilters(\(effects.joined(separator: ",")))"
            }
            return props["backgroundMaterial"] == nil ? "none" : "unsupported"
        }
        if #available(macOS 26.0, *), let glass = materialView as? NSGlassEffectView { return "NSGlassEffectView(.\(glass.style == .clear ? "clear" : "regular"))" }
        return "NSVisualEffectView(.\(materialKind.map { Materials.resolve($0) { _ in } } ?? "popover"))"
    }

    func updateMaterial() {
        let requested = props["backgroundMaterial"]
        defer { applyBackdrop(); syncGlassSlot(); syncGlassGroup(); syncScrollCorner() }
        let kind = requested ?? (needsSheetMaterial ? "sheet" : paintedMenuRadius != nil && !unevenCorners ? "glass" : nil)
        if materialKind != kind {
            releaseGlassIsolation()
            let children = structuralChildren
            let old = materialView
            let oldContent = materialContent
            materialView = nil
            materialContent = nil
            materialKind = kind
            if let kind {
                let content = MaterialContent(frame: bounds)
                content.owner = self
                content.autoresizingMask = [.width, .height]
                let effect: NSView
                let apple = Materials.resolve(kind) { [weak self] in self?.presenter?.session?.log($0) }
                if apple == "glass" || apple == "glassClear", #available(macOS 26.0, *) {
                    let glass = GlassBackground(frame: bounds)
                    glass.owner = self
                    glass.style = apple == "glass" ? .regular : .clear
                    effect = glass
                } else {
                    // The table's material; AppKit's stand-in where it lacks
                    // the named one (ultra-thin draws popover), LLP 1053.000 D4.
                    let blur = BlurBackground(frame: bounds)
                    blur.material = Materials.material(apple) ?? .popover
                    effect = blur
                }
                effect.autoresizingMask = [.width, .height]
                effect.setAccessibilityElement(false)
                content.setAccessibilityElement(false)
                addSubview(effect, positioned: .below, relativeTo: subviews.first)
                addSubview(content, positioned: .above, relativeTo: effect)
                materialView = effect
                materialContent = content
            }
            // Reconcile before removing the old container, keeping live child
            // identities, their frames and their native editors intact.
            for child in children where child.superview !== container { container.addSubview(child) }
            old?.removeFromSuperview()
            oldContent?.removeFromSuperview()
        }
        if let blur = materialView as? NSVisualEffectView {
            let sheetGround = requested == nil && needsSheetMaterial
            blur.blendingMode = sheetGround || blur.material == .sidebar ? .behindWindow : .withinWindow
            blur.state = .followsWindowActiveState
        }
        if #available(macOS 26.0, *), let glass = materialView as? NSGlassEffectView { glass.tintColor = glassTint.flatMap { NSColor(cgColor: $0) } }
        syncMaterialVisibility()
        sizeMaterial()
    }

    /// Visible content participates in the native effect. Hidden materials
    /// leave visible descendants outside it, including their focused editor.
    func syncMaterialVisibility() {
        guard let material = materialView, let content = materialContent else { return }
        GlassGroups.moving(in: self) {
            if cssVisibilityHidden {
                let parent = material.superview ?? self
                if content.superview !== parent {
                    if #available(macOS 26.0, *), let glass = material as? NSGlassEffectView { glass.contentView = nil }
                    parent.addSubview(content, positioned: .above, relativeTo: material)
                }
            } else if !content.isDescendant(of: material) {
                if #available(macOS 26.0, *), let glass = material as? NSGlassEffectView { glass.contentView = content }
                else { material.addSubview(content) }
            }
            material.isHidden = cssVisibilityHidden
        }
    }

    /// The material's corners: the box's radius as CSS reduces it to fit the
    /// border box (a 100 pt radius on a 28 pt pill is 14), at the current size,
    /// scaled as a grouped glass's geometry scales the box (`GlassGeometryMac.swift`).
    func applyMaterialRadius() {
        guard let materialView else { return }
        materialView.wantsLayer = true
        guard let materialLayer = materialView.layer else { return }
        let geometryScale = glassGeometry.map { CGSize(width: $0.a, height: $0.d) } ?? CGSize(width: 1, height: 1)
        let radii = effectiveCornerSizes(in: materialBox).map { CGSize(width: $0.width * geometryScale.width, height: $0.height * geometryScale.height) }
        let radius = BorderPaint.clip(materialLayer, in: materialView.bounds, radii: radii)
        if #available(macOS 26.0, *), let glass = materialView as? NSGlassEffectView {
            glass.cornerRadius = radius
        } else {
            materialView.wantsLayer = true
            materialView.layer?.cornerRadius = radius
            materialView.layer?.masksToBounds = true
        }
    }
}
#endif
