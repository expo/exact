// @ref LLP 1055 D7 — CSS animations lowered to Core Animation, for a box
// (`BoxAnimations`) and for an SVG scene's elements, and the colour and path
// readings their keyframes share with the scene.
import CoreGraphics
import Foundation
import QuartzCore

private func num(_ v: Any?) -> Double { (v as? NSNumber)?.doubleValue ?? 0 }
private func nums(_ v: Any?) -> [Double] { (v as? [Any])?.map(num) ?? [] }

/// A colour the host sent: `[r,g,b,a]` (0–255), a `light-dark()` pair, or a
/// colour in its own space, `{"cs": [{"s", "v"}…]}` (LLP 1100 D2).
package func paintColor(_ v: Any?, dark: Bool, tint: PlatformColor? = nil) -> CGColor? {
    // Restore the typed color, including the ICC handle kept by the payload
    // adapter, and use the same conversion/intent as every other paint path.
    func value(_ v: Any?) -> BatchValue {
        switch v {
        case let p as ProfileSpaces.Handle: return .profile(p)
        case let n as NSNumber: return .number(n.doubleValue)
        case let s as String: return .string(s)
        case let a as [Any]: return .array(a.map { value($0) })
        case let o as [String: Any]: return .object(o.mapValues { value($0) })
        default: return .null
        }
    }
    return value(v).cgColor(dark: dark, tint: tint)
}

/// `[0,x,y, 1,x,y, 2,x1,y1,x2,y2,x,y, 3]`: move, line, cubic, close.
package func svgPath(_ v: Any?) -> CGPath {
    if let prepared = v as? PreparedPath { return prepared.path }
    let n = nums(v), p = CGMutablePath()
    var i = 0
    while i < n.count {
        switch Int(n[i]) {
        case 0 where i + 2 < n.count: p.move(to: CGPoint(x: n[i + 1], y: n[i + 2])); i += 3
        case 1 where i + 2 < n.count: p.addLine(to: CGPoint(x: n[i + 1], y: n[i + 2])); i += 3
        case 2 where i + 6 < n.count:
            p.addCurve(to: CGPoint(x: n[i + 5], y: n[i + 6]), control1: CGPoint(x: n[i + 1], y: n[i + 2]), control2: CGPoint(x: n[i + 3], y: n[i + 4])); i += 7
        case 3: p.closeSubpath(); i += 1
        default: return p
        }
    }
    return p
}

/// The scene's path parser, for the owner's preparation (`SvgPrepare`).
enum SvgGeometry {
    static func path(_ v: Any?) -> CGPath { svgPath(v) }
}

/// A circle about the origin from (r, 0), toward positive y (SVG 2 §10.3),
/// the same four cubics the kernel draws, so a radius animation interpolates
/// between circles.
func svgCircle(_ r: Double) -> CGPath {
    let k = r * 0.5522848, p = CGMutablePath()
    p.move(to: CGPoint(x: r, y: 0))
    p.addCurve(to: CGPoint(x: 0, y: r), control1: CGPoint(x: r, y: k), control2: CGPoint(x: k, y: r))
    p.addCurve(to: CGPoint(x: -r, y: 0), control1: CGPoint(x: -k, y: r), control2: CGPoint(x: -r, y: k))
    p.addCurve(to: CGPoint(x: 0, y: -r), control1: CGPoint(x: -r, y: -k), control2: CGPoint(x: -k, y: -r))
    p.addCurve(to: CGPoint(x: r, y: 0), control1: CGPoint(x: k, y: -r), control2: CGPoint(x: r, y: -k))
    p.closeSubpath()
    return p
}

/// CSS animations lowered to Core Animation (LLP 1055 D7; a box's transform
/// key paths and background colour, LLP 1055.001). A spec's start is
/// on the runtime clock (ms / 1000 since `ExactEnv.t0`); a held one (authored
/// `paused`, or any under an agent-owned clock) is `speed = 0` at its local
/// time, so a screenshot and a `clock` seek are deterministic.
package enum CssAnimations {
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

    /// A spec's values, hashed: describing them as text was most of a
    /// scene's cost on a list's rows.
    /// A batch's numbers arrive as `NSNumber`: that case goes first, as a
    /// class cast, because `as Double` bridges and was most of a list
    /// row's animation cost.
    package static func digest(_ v: Any?, into h: inout Hasher) {
        switch v {
        case let n as NSNumber: h.combine(n.doubleValue.bitPattern)
        case let n as Double: h.combine(n.bitPattern)
        case let s as String: h.combine(s)
        case let p as ProfileSpaces.Handle: h.combine(p.key)
        case let a as [Any]: h.combine(a.count); for x in a { digest(x, into: &h) }
        // A prepared path hashes as the numbers it was built from.
        case let p as PreparedPath: h.combine(p.digest)
        case let d as [String: Any]:
            // A spec's prepared parts ("#") are made from the rest.
            let keys = d.keys.filter { $0 != "#" }.sorted()
            h.combine(keys.count); for k in keys { h.combine(k); digest(d[k], into: &h) }
        case .none: h.combine(0 as UInt8)
        case let .some(other): h.combine(String(describing: other))
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
        guard duration > 0, repeatCount != 0, keyTimes.count == values.count, keyTimes.count >= 2 else { return nil }
        let a = CAKeyframeAnimation(keyPath: key == "r" ? "path" : key)
        a.keyTimes = keyTimes
        // Through KVC, so the shared array is kept rather than bridged anew.
        a.setValue(values, forKey: "values")
        if let timing { a.timingFunctions = timing }
        a.calculationMode = .linear
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
