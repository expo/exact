#if os(iOS) || os(tvOS)
import UIKit

extension NodeView {
    /// The reduced radii; the layer fast path additionally requires circles.
    func cornerSizes(in rect: CGRect, inset: CGFloat = 0) -> [CGSize] {
        BorderPaint.reduced(BorderPaint.radii(style, in: rect, inset: inset), in: rect)
    }
    func cornerRadii(in rect: CGRect, inset: CGFloat = 0) -> [CGFloat] {
        cornerSizes(in: rect, inset: inset).map { $0.width }
    }
    func roundedPath(in rect: CGRect, inset: CGFloat = 0) -> UIBezierPath {
        UIBezierPath(cgPath: BorderPaint.roundedRect(rect, cornerSizes(in: rect, inset: inset), shape: CornerShape(style["corner_shape"])))
    }

    /// Mask only the fill: CSS's visible overflow and outer shadows extend beyond the corners.
    func applyCornerFill(_ color: CGColor?) {
        guard let color else {
            boxFill?.removeFromSuperlayer()
            boxFill = nil
            return
        }
        let fill = boxFill ?? CALayer()
        if boxFill == nil {
            boxFill = fill
            fill.mask = CAShapeLayer()
            insertBoxSublayer(fill)
        }
        if fill.frame != bounds { fill.frame = bounds }
        if fill.backgroundColor != color { fill.backgroundColor = color }
        if let mask = fill.mask as? CAShapeLayer {
            let path = roundedPath(in: bounds).cgPath
            if mask.path != path { mask.path = path }
        }
    }
}
#endif
