import QuartzCore

/// Paint in a layer's own coordinates, including descendants and the full
/// tracks played by Core Animation without presenter geometry callbacks.
enum OverflowPaint {
    static func bounds(_ layer: CALayer) -> CGRect {
        var rect = layer.bounds
        let stroke = (layer as? CAShapeLayer).map(strokeReach) ?? 0
        if let shape = layer as? CAShapeLayer, let path = shape.path {
            rect = rect.union(path.boundingBoxOfPath.insetBy(dx: -stroke, dy: -stroke))
        }
        for key in layer.animationKeys() ?? [] {
            if let track = layer.animation(forKey: key) as? CAKeyframeAnimation, track.keyPath == "path" {
                // Between keyframes each point moves between its positions
                // in the two, so the control points' box holds every path.
                for case let path as CGPath in track.values ?? [] {
                    rect = rect.union(path.boundingBox.insetBy(dx: -stroke, dy: -stroke))
                }
            }
        }
        for child in layer.sublayers ?? [] {
            let paint = inParent(bounds(child), of: child)
            if finite(paint) { rect = rect.union(paint) }
        }
        if layer.shadowOpacity > 0 {
            let source = layer.shadowPath?.boundingBoxOfPath ?? rect
            let blur = 3 * layer.shadowRadius
            rect = rect.union(source.insetBy(dx: -blur, dy: -blur)
                .offsetBy(dx: layer.shadowOffset.width, dy: layer.shadowOffset.height))
        }
        return rect
    }

    /// How far a stroke can paint past its path: half its widest width, as
    /// far as √2 times that at a square cap's corner, and at a miter join
    /// up to `miterLimit` times it (a miter's length over the line's width
    /// is at most the limit; SVG 2 §13.5.4 `stroke-miterlimit`). A bound,
    /// not the outline, so a long path costs no stroking.
    private static func strokeReach(_ shape: CAShapeLayer) -> CGFloat {
        var width = shape.lineWidth
        for key in shape.animationKeys() ?? [] {
            guard let track = shape.animation(forKey: key) as? CAKeyframeAnimation, track.keyPath == "lineWidth",
                  let range = range(track) else { continue }
            width = max(width, range.upperBound)
        }
        let miter = shape.lineJoin == .miter ? shape.miterLimit : 1
        let cap: CGFloat = shape.lineCap == .square ? 2.squareRoot() : 1
        return max(0, width) / 2 * max(1, miter, cap)
    }

    /// `rect` in the layer's own space with an additive `transform` ease
    /// composed before its model (the iOS press, `PressFeedback.swift`)
    /// at each extreme of its curve. The matrices are mixed entry by entry,
    /// which is Core Animation's interpolation for a scale and translation.
    private static func eased(_ rect: CGRect, of layer: CALayer) -> CGRect {
        var paint = rect
        let anchor = CGPoint(x: layer.bounds.minX + layer.anchorPoint.x * layer.bounds.width,
                             y: layer.bounds.minY + layer.anchorPoint.y * layer.bounds.height)
        for key in layer.animationKeys() ?? [] {
            guard let ease = layer.animation(forKey: key) as? CABasicAnimation, ease.isAdditive, ease.keyPath == "transform",
                  let from = (ease.fromValue as? NSValue)?.caTransform3DValue,
                  let to = (ease.toValue as? NSValue)?.caTransform3DValue else { continue }
            var low: CGFloat = 0, high: CGFloat = 1
            for control in [1, 2] {
                var point: [Float] = [0, 0]
                ease.timingFunction?.getControlPoint(at: control, values: &point)
                low = min(low, CGFloat(point[1])); high = max(high, CGFloat(point[1]))
            }
            for progress in [low, high] {
                let m = mix(from, to, progress)
                guard CATransform3DIsAffine(m) else { continue }
                let about = CGAffineTransform(translationX: -anchor.x, y: -anchor.y)
                    .concatenating(CATransform3DGetAffineTransform(m))
                    .concatenating(CGAffineTransform(translationX: anchor.x, y: anchor.y))
                paint = paint.union(rect.applying(about))
            }
        }
        return paint
    }

    private static func mix(_ a: CATransform3D, _ b: CATransform3D, _ t: CGFloat) -> CATransform3D {
        func f(_ x: CGFloat, _ y: CGFloat) -> CGFloat { x + (y - x) * t }
        return CATransform3D(m11: f(a.m11, b.m11), m12: f(a.m12, b.m12), m13: f(a.m13, b.m13), m14: f(a.m14, b.m14),
                             m21: f(a.m21, b.m21), m22: f(a.m22, b.m22), m23: f(a.m23, b.m23), m24: f(a.m24, b.m24),
                             m31: f(a.m31, b.m31), m32: f(a.m32, b.m32), m33: f(a.m33, b.m33), m34: f(a.m34, b.m34),
                             m41: f(a.m41, b.m41), m42: f(a.m42, b.m42), m43: f(a.m43, b.m43), m44: f(a.m44, b.m44))
    }

    static func inParent(_ own: CGRect, of layer: CALayer) -> CGRect {
        guard let parent = layer.superlayer else { return own }
        let rect = eased(own, of: layer)
        let keys = ["transform.translation.x", "transform.translation.y", "transform.scale", "transform.rotation.z", "position.x", "position.y"]
        var ranges: [String: ClosedRange<CGFloat>] = [:]
        for name in layer.animationKeys() ?? [] {
            guard let track = layer.animation(forKey: name) as? CAKeyframeAnimation,
                  let key = track.keyPath, keys.contains(key), let range = range(track) else { continue }
            if let old = ranges[key] { ranges[key] = min(old.lowerBound, range.lowerBound)...max(old.upperBound, range.upperBound) }
            else { ranges[key] = range }
        }
        guard !ranges.isEmpty else { return layer.convert(rect, to: parent) }

        // A detached pair lets Core Animation apply its anchor, flipped
        // geometry and sublayer transform without changing the live tree.
        let holder = CALayer(), probe = CALayer()
        holder.bounds = parent.bounds; holder.anchorPoint = parent.anchorPoint
        holder.isGeometryFlipped = parent.isGeometryFlipped; holder.sublayerTransform = parent.sublayerTransform
        probe.bounds = layer.bounds; probe.position = layer.position; probe.anchorPoint = layer.anchorPoint
        probe.isGeometryFlipped = layer.isGeometryFlipped; probe.transform = layer.transform
        holder.addSublayer(probe)
        var paint = rect
        if ranges["transform.scale"] != nil || ranges["transform.rotation.z"] != nil {
            let pivot = CGPoint(x: layer.bounds.minX + layer.anchorPoint.x * layer.bounds.width,
                                y: layer.bounds.minY + layer.anchorPoint.y * layer.bounds.height)
            let radius = hypot(max(abs(rect.minX - pivot.x), abs(rect.maxX - pivot.x)),
                               max(abs(rect.minY - pivot.y), abs(rect.maxY - pivot.y)))
            paint = CGRect(x: pivot.x - radius, y: pivot.y - radius, width: 2 * radius, height: 2 * radius)
            let model = max(abs(scalar(layer, "transform.scale.x")), abs(scalar(layer, "transform.scale.y")))
            let scale = ranges["transform.scale"].map { max(model, abs($0.lowerBound), abs($0.upperBound)) } ?? model
            probe.setValue(scale, forKeyPath: "transform.scale")
            probe.setValue(0, forKeyPath: "transform.rotation.z")
        }
        let axes = keys.filter { $0 != "transform.scale" && $0 != "transform.rotation.z" && ranges[$0] != nil }
        var reach = layer.convert(rect, to: parent)
        func include(_ axis: Int) {
            guard axis < axes.count else { reach = reach.union(probe.convert(paint, to: holder)); return }
            let key = axes[axis], span = ranges[key]!
            for value in [span.lowerBound, span.upperBound] {
                probe.setValue(value, forKeyPath: key)
                include(axis + 1)
            }
        }
        include(0)
        return reach
    }

    /// A cubic stays inside its control polygon, including CSS curves
    /// whose y controls overshoot the segment's endpoints.
    private static func range(_ track: CAKeyframeAnimation) -> ClosedRange<CGFloat>? {
        let values = (track.values ?? []).compactMap { ($0 as? NSNumber).map { CGFloat($0.doubleValue) } }
        guard values.count == track.values?.count, let first = values.first, values.allSatisfy(\.isFinite) else { return nil }
        var low = first, high = first
        for i in values.indices {
            low = min(low, values[i]); high = max(high, values[i])
            guard i + 1 < values.count, let curves = track.timingFunctions, i < curves.count else { continue }
            for control in [1, 2] {
                var point: [Float] = [0, 0]
                curves[i].getControlPoint(at: control, values: &point)
                let value = values[i] + CGFloat(point[1]) * (values[i + 1] - values[i])
                low = min(low, value); high = max(high, value)
            }
        }
        return low...high
    }

    private static func scalar(_ layer: CALayer, _ key: String) -> CGFloat {
        (layer.value(forKeyPath: key) as? NSNumber).map { CGFloat($0.doubleValue) } ?? 0
    }

    static func finite(_ rect: CGRect) -> Bool {
        !rect.isNull && !rect.isInfinite && rect.minX.isFinite && rect.minY.isFinite && rect.maxX.isFinite && rect.maxY.isFinite
    }
}
