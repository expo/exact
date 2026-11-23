// LLP 1077 §5 on UIKit: what iOS draws that CSS has no name for. Smart
// Invert leaving a node's pixels alone (D18), a scroll container's edge
// effect (D16, iOS 26), the iPad pointer's effect over a node (D17), and
// a text's numerals rolling as they change (D15).
#if os(iOS) || os(tvOS)
import ObjectiveC
import UIKit

extension NodeView {
    /// Everything here a style change can move.
    func applyAffordances() {
        // D18: images, video and canvases are left alone under `auto`.
        let invert = style["smart_invert"]?.string ?? "auto"
        let ignores = invert == "ignore" || ["image", "video", "canvas"].contains(kind)
        if accessibilityIgnoresInvertColors != ignores { accessibilityIgnoresInvertColors = ignores }
        if let sv = scrollView { applyScrollEdge(sv) }
        applyHoverEffect()
    }

    /// D16, LLP 1123 D14: the edge effect where content meets a bar, a style
    /// and a visibility per edge, as UIKit holds them.
    func applyScrollEdge(_ sv: UIScrollView) {
        guard #available(iOS 26.0, tvOS 26.0, *) else { return }
        for (effect, edge) in [(sv.topEdgeEffect, UIRectEdge.top), (sv.leftEdgeEffect, .left),
                               (sv.bottomEdgeEffect, .bottom), (sv.rightEdgeEffect, .right)] {
            let hidden = edgeEffectHidden(edge), style = edgeEffectStyle(edge)
            if effect.isHidden != hidden { effect.isHidden = hidden }
            if effect.style != style { effect.style = style }
        }
    }

    /// Whether the scroller hides its edge effect on the physical `edge`
    /// (LLP 1123 D14). Where `-exact-scroll-edge-effect-hidden` is set, the
    /// edges it names are hidden, physically or by `start` or `end`; elsewhere
    /// the retired `-exact-scroll-edge-effect` decides until activation, its
    /// `none` hiding every edge (§"What it replaces"). The Apple style record
    /// sends a form only while the kernel holds it set, so a form cleared with
    /// `unset` gives its aspect back to the retired one.
    package func edgeEffectHidden(_ edge: UIRectEdge) -> Bool {
        guard style["scroll_edge_effect_hidden"] != nil else { return style["scroll_edge_effect"]?.string == "none" }
        return edgeName("scroll_edge_effect_hidden", edge) != nil
    }

    /// The scroller's edge effect style on the physical `edge` (D14). Where
    /// `-exact-scroll-edge-effect-style` is set, an edge in its soft set is
    /// soft, one in its hard set hard, and one in neither `automatic`; an edge
    /// one set names by `start` or `end` and the other by `left` or `right`
    /// takes the first set's style, and one in both alike is hard (the
    /// schema's rows). Elsewhere the retired form's word styles every edge,
    /// its `none` leaving them `automatic`.
    @available(iOS 26.0, tvOS 26.0, *)
    private func edgeEffectStyle(_ edge: UIRectEdge) -> UIScrollEdgeEffect.Style {
        guard style["scroll_edge_effect_soft"] != nil || style["scroll_edge_effect_hard"] != nil else {
            switch style["scroll_edge_effect"]?.string {
            case "soft": return .soft
            case "hard": return .hard
            default: return .automatic
            }
        }
        let soft = edgeName("scroll_edge_effect_soft", edge), hard = edgeName("scroll_edge_effect_hard", edge)
        if soft == .logical, hard != .logical { return .soft }
        return hard != nil ? .hard : soft != nil ? .soft : .automatic
    }

    /// Whether the box sets any aspect of its edge effect, by the retired form
    /// or by either of D14's.
    package var setsEdgeEffect: Bool {
        ["scroll_edge_effect", "scroll_edge_effect_soft", "scroll_edge_effect_hard", "scroll_edge_effect_hidden"]
            .contains { style[$0] != nil }
    }

    /// How the `<edges>` row `key` names the physical `edge` (D13, D14: bits
    /// for top, right, bottom, left, start and end): by `start` or `end`,
    /// resolved against the box's direction, physically, or not at all.
    private func edgeName(_ key: String, _ edge: UIRectEdge) -> EdgeName? {
        let bits = Int(style[key]?.number ?? 0)
        let rtl = (style["direction"]?.string ?? presenter?.documentDirection) == "rtl"
        let start = rtl ? UIRectEdge.right : .left, end = rtl ? UIRectEdge.left : .right
        if edge == start && bits & 16 != 0 || edge == end && bits & 32 != 0 { return .logical }
        let physical = edge == .top ? 1 : edge == .right ? 2 : edge == .bottom ? 4 : edge == .left ? 8 : 0
        return bits & physical != 0 ? .physical : nil
    }

    /// D17: a pointer interaction whose style is the node's effect.
    private func applyHoverEffect() {
        // tvOS has no pointer interactions.
        #if !os(tvOS)
        let effect = style["hover_effect"]?.string ?? "auto"
        // Only the interaction this row added: a native hatch's stays.
        let ours = interactions.compactMap { $0 as? UIPointerInteraction }.first { $0.delegate is HoverEffect }
        guard effect != "auto", effect != "none" else {
            if let ours { removeInteraction(ours) }
            return
        }
        let delegate = HoverEffect.of(self)
        delegate.effect = effect
        if ours == nil { addInteraction(UIPointerInteraction(delegate: delegate)) }
        #endif
    }
}

#if !os(tvOS)
/// The pointer's style over one node (D17), kept on the node.
final class HoverEffect: NSObject, UIPointerInteractionDelegate {
    var effect = "auto"
    private static var key = 0
    static func of(_ view: UIView) -> HoverEffect {
        if let h = objc_getAssociatedObject(view, &key) as? HoverEffect { return h }
        let h = HoverEffect()
        objc_setAssociatedObject(view, &key, h, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        return h
    }
    func pointerInteraction(_ interaction: UIPointerInteraction, styleFor region: UIPointerRegion) -> UIPointerStyle? {
        guard let view = interaction.view, view.window != nil else { return nil }
        let preview = UITargetedPreview(view: view)
        switch effect {
        case "highlight": return UIPointerStyle(effect: .highlight(preview))
        case "lift": return UIPointerStyle(effect: .lift(preview))
        case "hover": return UIPointerStyle(effect: .hover(preview))
        default: return nil
        }
    }
}
#endif

/// D15: a paragraph whose text changed under `-exact-content-transition: numeric`
/// rolls the way SwiftUI's numeric text does: a rising number comes in from
/// above, a falling one (and any `numeric-countdown`) from below. The whole
/// line rolls: the raster is one picture.
enum NumeralRoll {
    private static var key = 0
    static func forget(_ ink: CALayer) {
        objc_setAssociatedObject(ink, &key, nil, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        ink.removeAnimation(forKey: "exact.numeric")
    }
    /// Answers the roll it began, for the layer of the text's HDR shadow,
    /// which rolls with it (`CALayer.applyTextCast`).
    @discardableResult
    static func roll(_ ink: CALayer, node: NodeView) -> CATransition? {
        let mode = node.style["content_transition"]?.string ?? "none"
        let text = node.props["text"] ?? node.inlineText.map(\.text).joined()
        let last = objc_getAssociatedObject(ink, &key) as? String
        objc_setAssociatedObject(ink, &key, text, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        let ready = (ink.delegate as? CoverageImageView).map { $0.image != nil } ?? (ink.contents != nil)
        guard mode != "none", let last, last != text, ready, !UIAccessibility.isReduceMotionEnabled else { return nil }
        let roll = CATransition()
        roll.type = .push
        let down = mode == "numeric-countdown" || leadingNumber(text) < leadingNumber(last)
        roll.subtype = down ? .fromBottom : .fromTop
        roll.duration = 0.3
        roll.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        ink.add(roll, forKey: "exact.numeric")
        return roll
    }
}

#if !os(tvOS)
extension NodeView {
    /// The pointer is over an inline hover run or the rest of the paragraph,
    /// one at a time: sliding off the run onto the paragraph enters the
    /// paragraph, however the hover began.
    @objc func hovering(_ g: UIHoverGestureRecognizer) {
        switch g.state {
        case .ended, .cancelled, .failed:
            presenter?.hoverInline(nil)
            presenter?.hover(self, false)
        default:
            if let run = inlineTarget(at: g.location(in: self), handler: "hover") { presenter?.hoverInline(run.id); return }
            presenter?.hoverInline(nil)
            if presenter?.hovered !== self { presenter?.hover(self, true) }
        }
    }
}
#endif

/// How an `<edges>` row names an edge (`edgeName`).
private enum EdgeName { case logical, physical }
#endif
