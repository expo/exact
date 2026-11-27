// CSS clip-path commands validated by the kernel, shared by UIKit and AppKit:
// `{"rule": "nonzero" | "evenodd", "commands": [["M", [x, y]], …]}`.
import CoreGraphics
import ExactCore
import ObjectiveC
import QuartzCore

enum ClipPath {
    static func path(_ value: BatchValue?, size: CGSize = .zero) -> CGPath? {
        ClipGeometry.path(value, size: size)
    }

    /// Which points a clip holds: CSS's `nonzero` unless it said `evenodd`.
    static func rule(_ value: BatchValue?) -> CGPathFillRule {
        field(value, "rule")?.string == "evenodd" ? .evenOdd : .winding
    }

    private static func field(_ value: BatchValue?, _ key: String) -> BatchValue? {
        if case .object(let o)? = value { return o[key] }
        return nil
    }

    static func mask(_ path: CGPath?, _ rule: CGPathFillRule = .winding) -> CALayer? {
        guard let path else { return nil }
        let mask = CAShapeLayer()
        mask.path = path
        mask.fillRule = rule == .evenOdd ? .evenOdd : .nonZero
        mask.fillColor = CGColor(gray: 1, alpha: 1)
        return mask
    }
}


extension NodeView {
    /// Four equal percentage radii can still be elliptical. Keep the declared
    /// unequal-corner overflow fallback, but carry this equal outline alongside
    /// clip-path instead of dropping it when CALayer's circular fast path fails.
    var ellipticalClip: CGPath? {
        #if os(macOS)
        let backdrop = layer?.backgroundFilters?.isEmpty == false
        #else
        let backdrop = false
        #endif
        guard clipsToBounds || clipBox != nil || backdrop else { return nil }
        let sizes = cornerSizes(in: bounds)
        guard let first = sizes.first else { return nil }
        // Core Animation's corner radius is one circular radius: an
        // elliptical corner, or corners that differ (a chat bubble's 18 and 4
        // where it joins its run), clip through this outline instead.
        guard !(first.width == first.height && sizes.allSatisfy({ $0 == first })) else { return nil }
        return BorderPaint.roundedRect(bounds, sizes)
    }

    /// The layer's mask as CSS composes it — `mask-image` over `clip-path`
    /// over the clip's outline (`BoxMaskIOS.swift`, `BoxMaskMac.swift`) — for
    /// a filtered box's picture.
    func resolvedClipMask() -> CALayer? {
        let outline = clipBox == nil ? shapedOutline() : nil
        let image = style["mask_image"]
        #if os(macOS)
        let scale = pixelScale ?? 2
        #else
        let scale = traitCollection.displayScale
        #endif
        let shown = surface?.bounds ?? bounds
        let key = BoxMaskState.FilterKey(image: image, clip: style["clip_path"], outline: outline,
                                         bounds: shown, dark: drawsDark, scale: scale, tint: gradientTintKey(for: image))
        let state = BoxMaskState.of(self)
        if state.filterKey == key { return state.filterMask }
        state.filterKey = key
        state.filterMask = composedMask(shaped: ClipPath.mask(outline))
        return state.filterMask
    }

    /// The clip's outline follows a new size or a backdrop coming or going.
    func syncEllipticalClip() { applyBoxMask() }
}

/// What a node's mask was built from and what it installed (LLP 1077 D2):
/// `applyBoxMask` builds again only when an input changed or something
/// else (a material's radius, a surface, a filter) replaced what it put on.
final class BoxMaskState {
    struct Key: Equatable {
        var image: BatchValue?, clip: BatchValue?, outline: CGPath?, size: CGSize, dark: Bool
        var scale: CGFloat, tint: [[Double]]?
        var clipBox: Bool, filter: Bool, material: Bool, materialOutline: CGPath?
    }
    struct FilterKey: Equatable {
        var image: BatchValue?, clip: BatchValue?, outline: CGPath?
        var bounds: CGRect
        var dark: Bool
        var scale: CGFloat
        var tint: [[Double]]?
    }
    var key: Key?
    var filterKey: FilterKey?
    var filterMask: CALayer?
    var layerMask: CALayer?
    var boxMask: CALayer?
    var effectMask: AnyObject?
    /// The clip's shape layer while it is the layer's own mask ([`NodeView.noteClipLayer`]).
    weak var clipLayer: CAShapeLayer?
    private static var slot = 0
    /// Its state, if `applyBoxMask` ever built one for it.
    static func existing(_ view: NodeView) -> BoxMaskState? { objc_getAssociatedObject(view, &slot) as? BoxMaskState }
    static func of(_ view: NodeView) -> BoxMaskState {
        if let s = existing(view) { return s }
        let s = BoxMaskState()
        objc_setAssociatedObject(view, &slot, s, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        return s
    }
}

package protocol ClipPathProviding: AnyObject {
    var presentedClipPath: CGPath? { get }
}

extension NodeView: ClipPathProviding {}

extension NodeView {
    /// Tell the box animations where the clip is (LLP 1003.003 D7): `mask`
    /// when the layer's own mask is the clip's shape layer, which Core
    /// Animation plays a moving clip on, else nil (no clip, or a mask that
    /// holds it, a mask's own mask, which `CALayer.h` marks unsupported).
    func noteClipLayer(_ mask: CALayer?) {
        let clip = clipPath == nil ? nil : mask as? CAShapeLayer
        let state = BoxMaskState.existing(self)
        guard state?.clipLayer !== clip else { return }
        (state ?? BoxMaskState.of(self)).clipLayer = clip
        presenter?.boxAnimations.setClip(id, clip, clock: presenter?.session?.clock)
    }

    /// The clip as it shows: the path of the clip layer's presentation copy
    /// while Core Animation plays a run on it, which `CALayer.h` calls a
    /// close approximation of what shows, else the model's, which a copy
    /// takes only at its next commit (LLP 1003.003 D7, D8).
    package var presentedClipPath: CGPath? {
        guard let clipPath else { return nil }
        guard let clip = BoxMaskState.existing(self)?.clipLayer, clip.animationKeys()?.isEmpty == false else { return clipPath }
        return clip.presentation()?.path ?? clipPath
    }
}
