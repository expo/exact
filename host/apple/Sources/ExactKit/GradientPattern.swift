// `repeating-linear-gradient()` backgrounds as patterns Core Animation tiles
// (#661), shared by UIKit and AppKit. Each gradient's period is rasterized
// once, axis-aligned, as a pattern's cell, and tiled along the gradient line
// at any angle. Core Animation and Core Graphics both anchor a layer's
// pattern background at the layer's own coordinate origin: a sublayer whose
// origin is where a period starts puts the phase where CSS puts it, relative
// to the view, and a resize only moves that origin. A drawn texture was
// painted again at every size, every frame of a sheet's detent drag or a
// Mac window's live resize.
//
// The sublayer is turned to the gradient's angle and the pattern's matrix
// only scales. On iOS, `render(in:)` (a capture, a filter's input) mirrors a
// pattern's y axis about the layer's origin where the screen does not; a
// cell that is the same all along its y axis is unchanged by that, so the
// matrix must not turn it.
import CoreGraphics
import QuartzCore

#if os(iOS) || os(tvOS)
/// Placed in the animation context of the view it is in (`BoxPartLayer`).
typealias GradientPatternBase = BoxPartLayer
#else
typealias GradientPatternBase = CALayer
#endif

/// A box's repeating linear gradients over the view's bounds, one sublayer
/// each, the first on top. Its owner clips it to the box's corners.
final class GradientPatternLayer: GradientPatternBase {
    /// `gradients` placed in `box` over `bounds` (both the view's), at
    /// `scale` device pixels a point.
    func show(_ gradients: [Gradient], bounds: CGRect, box: CGRect, dark: Bool, scale: CGFloat) {
        if frame != bounds { frame = bounds }
        var stripes = sublayers?.compactMap { $0 as? GradientStripesLayer } ?? []
        while stripes.count < gradients.count {
            let layer = GradientStripesLayer()
            addSublayer(layer)
            stripes.append(layer)
        }
        while stripes.count > gradients.count { stripes.removeLast().removeFromSuperlayer() }
        // The last layer at the bottom (LLP 1077 D5).
        for (layer, gradient) in zip(stripes, gradients.reversed()) {
            layer.show(gradient, bounds: bounds, box: box, dark: dark, scale: scale)
        }
    }
}

/// One gradient's pattern, a square about the start of a period turned to
/// the gradient line. Its coordinate origin, where the pattern is anchored,
/// is that start.
final class GradientStripesLayer: GradientPatternBase {
    func show(_ gradient: Gradient, bounds: CGRect, box: CGRect, dark: Bool, scale: CGFloat) {
        guard let cycle = gradient.cycle(in: box, dark: dark) else {
            backgroundColor = nil
            return
        }
        let (start, end) = (cycle.start, cycle.end)
        let length = hypot(end.x - start.x, end.y - start.y)
        guard length > 0, let color = GradientPatterns.color(cycle.gradient, period: length, dark: dark, scale: scale) else {
            backgroundColor = nil
            return
        }
        let turn: CGFloat
        if case .angle(let deg) = cycle.gradient.shape {
            turn = (deg - 90) * .pi / 180
        } else {
            turn = atan2(end.y - start.y, end.x - start.x)
        }
        // A square about the period's start, wide enough to cover the
        // container at any turn; the container clips it.
        let anchor = CGPoint(x: start.x - bounds.minX, y: start.y - bounds.minY)
        let half = [CGPoint(x: 0, y: 0), CGPoint(x: bounds.width, y: 0), CGPoint(x: 0, y: bounds.height), CGPoint(x: bounds.width, y: bounds.height)]
            .map { hypot($0.x - anchor.x, $0.y - anchor.y) }.max()!.rounded(.up) + 1
        let place = CGRect(x: -half, y: -half, width: 2 * half, height: 2 * half)
        if self.bounds != place { self.bounds = place }
        if position != anchor { position = anchor }
        let transform = CATransform3DMakeRotation(turn, 0, 0, 1)
        if !CATransform3DEqualToTransform(self.transform, transform) { self.transform = transform }
        if backgroundColor !== color { backgroundColor = color }
    }
}

/// Pattern colours by what they show, so a resize, or another view with the
/// same gradient at the same angle, reuses the cell already rasterized.
enum GradientPatterns {
    private struct Key: Hashable {
        let stops: [CGFloat]
        let wide: Bool
        let texels: Int
        let step: CGFloat
    }
    private static var made: [Key: CGColor] = [:]
    /// How far a cell reaches along the stripes, points: one texel row,
    /// which the stripes do not change along. Core Graphics, drawing a
    /// capture, can leave a row of pixels unpainted or painted twice
    /// between two cells; one cell this long, centred on the period's
    /// start, spans a box to 2,048 points from it.
    private static let reach: CGFloat = 4096

    /// What a cell paints: the period from its left edge to its right.
    private final class Cell {
        let cycle: Gradient, texels: Int, dark: Bool
        init(_ cycle: Gradient, texels: Int, dark: Bool) { (self.cycle, self.texels, self.dark) = (cycle, texels, dark) }
    }

    /// A pattern of `cycle`'s period, `period` points long along its x
    /// axis and constant along its y, with a period's start at the origin.
    /// Its cell is painted, not an image, so it is rasterized at the
    /// resolution it is drawn at. Core Animation rasterizes it once at a
    /// texel a quarter of a device pixel wide, so the turned cell samples
    /// close to each pixel's centre, as Chrome shades it.
    static func color(_ cycle: Gradient, period length: CGFloat, dark: Bool, scale: CGFloat) -> CGColor? {
        guard length > 0, scale > 0 else { return nil }
        // The kernel's single-precision line differs in its last bits from
        // one box size to the next; read exactly, every size would be a new
        // pattern. A 2⁻¹⁶ pt period drifts under a thousandth of a point
        // across a phone's screen.
        let period = (length * 65536).rounded() / 65536
        let fine = min(4, 4096 / (period * scale))
        let texels = max(1, Int((period * scale * fine).rounded()))
        let step = period / CGFloat(texels)
        let (locations, colors) = cycle.stops(dark: dark)
        // The stops' positions carry the same noise; a cell is at most 4,096
        // texels wide, so a 2⁻¹⁶ step is the same cell.
        let at = locations.map { ($0 * 65536).rounded() / 65536 }
        let key = Key(stops: at + colors.flatMap { $0.components ?? [] }, wide: cycle.linear(dark: dark), texels: texels, step: step)
        if let color = made[key] { return color }
        var callbacks = CGPatternCallbacks(version: 0, drawPattern: { info, ctx in
            guard let info else { return }
            let cell = Unmanaged<Cell>.fromOpaque(info).takeUnretainedValue()
            // Core Graphics draws the cells one by one, edge to edge:
            // antialiased, each would half cover the pixels on a shared edge
            // and leave a seam.
            ctx.setShouldAntialias(false)
            // Texel i's centre is the period's position i, so a pixel whose
            // centre falls on a stop takes the colour after it, as in
            // Chrome; the half texel before the first is the period's end.
            let n = CGFloat(cell.texels)
            for (left, right, start) in [(0.5, n, 0.5), (0, 0.5, 0.5 - n)] as [(CGFloat, CGFloat, CGFloat)] {
                ctx.saveGState()
                ctx.clip(to: CGRect(x: left, y: -0.5, width: right - left, height: 1))
                cell.cycle.paintAxial(ctx, start: CGPoint(x: start, y: 0), end: CGPoint(x: start + n, y: 0), dark: cell.dark)
                ctx.restoreGState()
            }
        }, releaseInfo: { info in
            if let info { Unmanaged<Cell>.fromOpaque(info).release() }
        })
        // A texel each along x, texel 0's centre at the origin; one row
        // along y.
        let matrix = CGAffineTransform(a: step, b: 0, c: 0, d: reach, tx: -step / 2, ty: 0)
        let info = Unmanaged.passRetained(Cell(cycle, texels: texels, dark: dark)).toOpaque()
        guard let pattern = CGPattern(info: info, bounds: CGRect(x: 0, y: -0.5, width: CGFloat(texels), height: 1), matrix: matrix,
                                      xStep: CGFloat(texels), yStep: 1, tiling: .noDistortion, isColored: true, callbacks: &callbacks),
              let space = CGColorSpace(patternBaseSpace: nil) else {
            Unmanaged<Cell>.fromOpaque(info).release()
            return nil
        }
        var alpha: CGFloat = 1
        guard let color = CGColor(patternSpace: space, pattern: pattern, components: &alpha) else { return nil }
        if made.count >= 64 { made.removeAll() }
        made[key] = color
        return color
    }
}
