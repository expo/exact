// LLP 1061 D7: the press's translate is added outside the node's own
// transform. Additive position animations keep layout and tracking geometry
// unchanged, and survive an authored transform or frame update while held.
import QuartzCore

package enum PressTranslation {
    package static func offset(_ style: NodeStyle, bounds: CGRect) -> [CGFloat] {
        let px = style["press_translate"]?.numbers ?? [0, 0]
        let pct = style["press_translate_percent"]?.numbers ?? [0, 0]
        guard px.count == 2, pct.count == 2 else { return [0, 0] }
        return [px[0] + pct[0] / 100 * bounds.width, px[1] + pct[1] / 100 * bounds.height]
    }

    package static func restore(_ layer: CALayer) {
        layer.removeAnimation(forKey: "pressTranslation")
    }

    private static func point(_ x: CGFloat, _ y: CGFloat) -> NSValue {
        #if os(macOS)
        NSValue(point: CGPoint(x: x, y: y))
        #else
        NSValue(cgPoint: CGPoint(x: x, y: y))
        #endif
    }

    static func apply(_ style: NodeStyle, to layer: CALayer, ease: PressFeedback, still: Bool) {
        let values = offset(style, bounds: layer.bounds)
        let settled = still || ease.settled(at: CACurrentMediaTime())
        if values == [0, 0] || settled && ease.to == 0 { restore(layer); return }
        let from = settled ? ease.to : ease.from
        let animation = CABasicAnimation(keyPath: "position")
        animation.isAdditive = true
        animation.fromValue = point(values[0] * from, values[1] * from)
        animation.toValue = point(values[0] * ease.to, values[1] * ease.to)
        animation.fillMode = .both
        animation.duration = settled ? .greatestFiniteMagnitude : PressFeedback.duration
        animation.isRemovedOnCompletion = !settled && ease.to == 0
        if !settled {
            animation.beginTime = layer.convertTime(ease.start, from: nil)
            animation.timingFunction = PressFeedback.timing
        }
        layer.add(animation, forKey: "pressTranslation")
    }
}

extension NodeView {
    func refreshPressTranslation(_ style: NodeStyle? = nil) {
        if isNativeButton { refreshNativeActivePresentation(); return }
        guard pressPaint.ease.from != 0 || pressPaint.ease.to != 0 else { return }
        let paintLayer: CALayer? = layer
        guard let paintLayer else { return }
        PressTranslation.apply(style ?? self.style, to: paintLayer, ease: pressPaint.ease, still: ExactEnv.agentFreezes)
    }
}
