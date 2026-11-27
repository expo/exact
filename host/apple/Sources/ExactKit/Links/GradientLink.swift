// @ref LLP 1047.001 D10 — CSS gradients (`background-image`, `mask-image`;
// LLP 1066, LLP 1077 D2, D5) are a family the composition selects (the
// roster's `gradients`, a plan that binds either row). The core reaches a
// gradient only through what this file declares: `ExactGradients.install()`
// sets the slots; without it a box shows no gradient, and a plan that binds
// one is refused at boot (`Unlinked("gradients")`). Where a gradient goes in
// a box (its layer, its mask, `draw(_:)`, a fixed gradient's re-aim) is the
// core's; parsing, resolving and painting one are the family's.
import CoreGraphics
import ExactCore
import QuartzCore

/// One `background-image` or `mask-image` gradient, parsed for a view.
package protocol BoxGradient {
    /// `repeating-linear-gradient()`, whose period a pattern can tile.
    var isRepeatingLinear: Bool { get }
    /// Conic geometry and repeated hard stops need the drawn painter.
    var needsDrawing: Bool { get }
    /// Locations 0–1 and their colours under an appearance (`dense`: each
    /// stretch between two hues sampled sixteen times).
    func stops(dark: Bool, dense: Bool) -> ([CGFloat], [CGColor])
    /// Show it on `layer`, placed in `box`; `stops` already made, if any.
    func apply(_ layer: CAGradientLayer, bounds: CGRect, box: CGRect, dark: Bool, limit: String?, stops: ([CGFloat], [CGColor])?)
    /// Paint it inside `clip`, placed in `box`.
    func paint(_ ctx: CGContext, clip: CGPath, box: CGRect, dark: Bool, limit: String?)
    /// Its alpha as pixels `size` points at `scale`.
    func image(size: CGSize, scale: CGFloat, dark: Bool) -> CGImage?
}

extension BoxGradient {
    func stops(dark: Bool) -> ([CGFloat], [CGColor]) { stops(dark: dark, dense: false) }
    func apply(_ layer: CAGradientLayer, bounds: CGRect, box: CGRect, dark: Bool, limit: String? = nil) {
        apply(layer, bounds: bounds, box: box, dark: dark, limit: limit, stops: nil)
    }
    func paint(_ ctx: CGContext, clip: CGPath, box: CGRect, dark: Bool) { paint(ctx, clip: clip, box: box, dark: dark, limit: nil) }
}

/// A layer that tiles repeating linear gradients' periods (#661).
package protocol GradientPatterning: CALayer {
    func show(_ gradients: [any BoxGradient], bounds: CGRect, box: CGRect, dark: Bool, scale: CGFloat)
}

/// The slots are set before any session is made, and read on the main thread.
package enum GradientLink {
    /// A gradient row's value parsed for a view's tint; nil for none.
    package static var make: ((BatchValue?, PlatformColor?) -> (any BoxGradient)?)?
    /// A box's pattern layer.
    package static var makePattern: (() -> any GradientPatterning)?

    static func gradient(_ value: BatchValue?, tint: PlatformColor? = nil) -> (any BoxGradient)? { make?(value, tint) }

    /// A `background-image`'s layers (LLP 1077 D5): one gradient's object,
    /// or an array of them, the first on top.
    static func layers(_ value: BatchValue?, tint: PlatformColor? = nil) -> [any BoxGradient] {
        guard let make else { return [] }
        if let list = value?.array { return list.compactMap { make($0, tint) } }
        return make(value, tint).map { [$0] } ?? []
    }
}

/// A drawn `mask-image`'s pixels as a layer's mask (LLP 1077 D5).
final class GradientImageMaskLayer: CALayer {}

extension BatchValue {
    /// A gradient that carries a `light-dark()` stop: an appearance change
    /// is something to the view that shows it.
    ///
    /// Nested too: a layer list, a shadow list's or a text shadow's colour,
    /// a symbol palette (LLP 1077), and a box filter with a dark program
    /// (`pd`, LLP 1095 D9).
    var isSchemeGradient: Bool {
        switch self {
        case .object(let o): return o["css"] != nil || o["dark"] != nil || o["pd"] != nil || o.values.contains { $0.isSchemeColor || $0.isSchemeGradient }
        case .array(let a): return a.contains { $0.isSchemeColor || $0.isSchemeGradient }
        default: return false
        }
    }
}

extension NodeView {
    var gradientTint: PlatformColor? {
        #if os(iOS) || os(tvOS)
        return viewTint?.resolvedColor(with: traitCollection)
        #else
        return viewTint ?? ThemeRoles.color("AccentColor", in: self)
        #endif
    }
    /// Resolved extended-sRGB tint channels for a tint-referencing gradient row,
    /// or nil when the row does not reference the view's tint.
    func gradientTintKey(for value: BatchValue?) -> [[Double]]? {
        guard value?.namesTint == true else { return nil }
        return SystemColor.tintPair(gradientTint ?? SystemColor.appTint, extended: true)
    }
    /// `background-attachment: fixed` (LLP 1066 D7).
    var gradientFixed: Bool { style["background_attachment"]?.string == "fixed" }

    /// The padding box: CSS's gradient box under the initial `background-origin`;
    /// under `background-attachment: fixed`, the viewport, in this view's
    /// coordinates (LLP 1066 D7), so every node with it shows its part of one
    /// gradient and the gradient stays as they scroll.
    var gradientBox: CGRect {
        if gradientFixed, let port = fixedGradientPort { return port }
        let uniform = number("border_width")
        return bounds.insetBy(left: number("border_width_left", uniform), top: number("border_width_top", uniform),
                              right: number("border_width_right", uniform), bottom: number("border_width_bottom", uniform))
    }

    /// The gradient painted by `draw(_:)`, over the background and under
    /// the border, inside the border box's outline.
    func paintGradient(_ ctx: CGContext, clip: CGPath) {
        guard surface == nil, !cssVisibilityHidden else { return }
        #if os(iOS)
        // A fixed gradient is the layer's (LLP 1066 D7).
        if gradientLayered && boxGradient != nil { return }
        #endif
        // The last layer first, so the first is on top (LLP 1077 D5).
        for g in GradientLink.layers(style["background_image"], tint: gradientTint).reversed() {
            g.paint(ctx, clip: clip, box: gradientBox, dark: drawsDark)
        }
    }

    /// Whether the background's gradients need `draw(_:)`: several layers,
    /// or a conic or repeating one (LLP 1077 D5, LLP 1066). Repeating linear
    /// gradients alone are patterns instead (`gradientPatterned`).
    var gradientDraws: Bool {
        let layers = GradientLink.layers(style["background_image"], tint: gradientTint)
        if gradientPatterned(layers) { return false }
        return layers.count > 1 || layers.contains { $0.needsDrawing }
    }

    /// A background of repeating linear gradients only, not `fixed`: Core
    /// Animation tiles each one's period (`GradientPatternLayer`, #661)
    /// wherever nothing else makes the box draw.
    func gradientPatterned(_ layers: [any BoxGradient]) -> Bool {
        !gradientFixed && !layers.isEmpty && layers.allSatisfy { $0.isRepeatingLinear }
    }
}
