// @ref LLP 1055 D7 — CSS animations lowered to Core Animation, for a box
// (`BoxAnimations`) and for an SVG scene's elements. A spec's start is on the
// runtime clock (ms / 1000 since `ExactEnv.t0`); a held one (authored
// `paused`, or any under an agent-owned clock) is `speed = 0` at its local
// time, so a screenshot and a `clock` seek are deterministic.
import ExactCore
import ExactKit
import Foundation
import QuartzCore

extension CssAnimations {
    /// Replace `layer`'s CSS animations with `specs`, keeping any whose spec
    /// is unchanged (a data tick must not restart a running pulse).
    package static func apply(_ specs: [CssAnimation], to layer: CALayer, clock: Double?, installed: inout [String: Int], offscreen: CFTimeInterval? = nil) {
        var changed = false
        defer { if changed { (layer.delegate as? NodeView)?.overflowClipGeometryChanged() } }
        var keep: Set<String> = []
        for spec in specs {
            let id = spec.id
            keep.insert(id)
            var h = Hasher()
            h.combine(spec.signature); h.combine(clock)
            let signature = h.finalize()
            if installed[id] == signature { continue }
            changed = true
            installed[id] = signature
            layer.removeAnimation(forKey: id)
            if let animation = make(spec, layer: layer, clock: clock, offscreen: offscreen) { layer.add(animation, forKey: id) }
        }
        for id in installed.keys where !keep.contains(id) {
            changed = true
            layer.removeAnimation(forKey: id)
            installed.removeValue(forKey: id)
        }
    }

    /// A spec's keyframe values, key times and timing functions: plain
    /// values, built on the owner as the batch is decoded (`SvgPrepare`).
    static func lowered(_ spec: [String: Any]) -> (values: [Any], keyTimes: [NSNumber], timing: [CAMediaTimingFunction]?) {
        let key = spec["k"] as? String ?? ""
        let colors = key == "fillColor" || key == "strokeColor" || key == "backgroundColor"
        // A colour track's values are [r,g,b,a] bytes (LLP 1055.000 D6).
        // A `path` track's values are paths, as a scene's `p` (LLP 1115.003 D4).
        let values: [Any] = colors
            ? (spec["v"] as? [Any] ?? []).compactMap { paintColor($0, dark: false) }
            : key == "path" ? (spec["v"] as? [Any] ?? []).map { svgPath($0) as Any }
            : nums(spec["v"]).map { key == "r" ? svgCircle(max(0, $0)) as Any : NSNumber(value: $0) }
        // No curves: every interval is linear, Core Animation's default.
        var timing: [CAMediaTimingFunction]?
        if let curves = spec["c"] as? [Any], !curves.isEmpty {
            timing = curves.map { c in
                let p = nums(c).map(Float.init)
                return p.count == 4 ? CAMediaTimingFunction(controlPoints: p[0], p[1], p[2], p[3]) : CAMediaTimingFunction(name: .linear)
            }
        }
        return (values, nums(spec["t"]).map { NSNumber(value: $0) }, timing)
    }

    /// `offscreen`: for a tree `CARenderer` draws (a live filter picture) at
    /// that instant, which plays running animations but not one held at
    /// `speed` 0: a held one runs from where it is held at that instant.
    static func make(_ spec: CssAnimation, layer: CALayer, clock: Double?, offscreen: CFTimeInterval? = nil) -> CAAnimation? {
        let (key, values, keyTimes, timing) = (spec.key, spec.values, spec.keyTimes, spec.timing)
        let duration = spec.duration, repeatCount = spec.repeatCount
        // A discrete track holds each value from its key time to the next,
        // the last to the end: one key time more than values (`kCAAnimationDiscrete`)
        let times = values.count + (spec.discrete ? 1 : 0)
        guard duration > 0, repeatCount != 0, keyTimes.count == times, keyTimes.count >= 2, values.count > 0 else { return nil }
        let a = CAKeyframeAnimation(keyPath: key == "r" ? "path" : key)
        a.keyTimes = keyTimes
        // Through KVC, so the shared array is kept rather than bridged anew.
        a.setValue(values, forKey: "values")
        if let timing { a.timingFunctions = timing }
        a.calculationMode = spec.discrete ? .discrete : .linear
        a.duration = duration
        a.repeatCount = repeatCount < 0 ? .infinity : Float(repeatCount)
        let fill = spec.fill
        let backwards = fill == 2 || fill == 3, forwards = fill == 1 || fill == 3
        a.fillMode = [CAMediaTimingFillMode.removed, .forwards, .backwards, .both][min(max(fill, 0), 3)]
        a.isRemovedOnCompletion = false
        let start = spec.start, delay = spec.delay
        let held = spec.held ?? clock.map { $0 / 1000 - start }
        if let local = held {
            let active = local - delay
            if active < 0 && !backwards { return nil }
            // At or past a finite end CA wraps to the next cycle's start; CSS
            // holds the last frame (with a forwards fill), so stay a hair inside.
            let total = repeatCount < 0 ? Double.infinity : duration * repeatCount
            // Ended without a forwards fill: no effect, as CSS (the property
            // shows its own value; a held last frame hid every later change).
            if active >= total && !forwards { return nil }
            let at = min(max(0, active), total - 1e-6)
            if let drawn = offscreen {
                a.beginTime = layer.convertTime(drawn, from: nil) - at
            } else {
                a.speed = 0
                a.timeOffset = at
                a.beginTime = 0
            }
        } else {
            a.beginTime = layer.convertTime(ExactEnv.t0 + start + delay, from: nil)
        }
        return a
    }
}
