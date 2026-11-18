// @ref LLP 1055.000 D14 — the filter engine a box's CSS `filter` and an SVG
// group's filter share: how much of a region a picture renders, and a drop
// shadow read from a chain's numbers. The chain itself runs on the GPU
// (`SvgFilterGPU`). It belongs to box effects, and SVG uses it: a plan with
// only a CSS filter links no SVG (LLP 1047.001 D10).
import CoreGraphics
import ExactKit

package enum FilterExtent {
    /// The most pixels one island's bitmap holds (64 MB at 4 bytes each).
    package static let cap = 16_777_216
    /// The most bytes a filter island's work may hold at once: its bitmaps
    /// and the chain's working images (F1's map was 747 MB in the module's
    /// `f32` images for one drop shadow).
    package static let budget = 128 << 20

    /// The part of `rect` (an island's space) an island renders, its size
    /// in pixels and its pixels per unit, at `k` pixels per unit when that
    /// is at most `cap` pixels, as before. Over `cap` (a huge filter region
    /// or mask at device scale), `rect` is first cut to `seen`, what can
    /// show of it (in the same space), on the full bitmap's own pixel grid,
    /// so the pixels kept are the full bitmap's; then, when that is still
    /// over `cap`, it is drawn at fewer pixels per unit and the layer scales
    /// it up, as a browser draws a huge effect at a lower resolution rather
    /// than not at all. `nil` when nothing of it can show.
    package static func extent(_ rect: CGRect, k: CGFloat, seen: CGRect?, limit: Int = cap) -> (rect: CGRect, w: Int, h: Int, k: CGFloat)? {
        let cap = min(limit, Self.cap)
        let fw = (rect.width * k).rounded(.up), fh = (rect.height * k).rounded(.up)
        guard rect.width > 0, rect.height > 0, fw >= 1, fh >= 1, fw.isFinite, fh.isFinite else { return nil }
        if fw * fh <= CGFloat(cap) { return (rect, Int(fw), Int(fh), k) }
        var (r, w, h) = (rect, fw, fh)
        if let seen {
            let (sx, sy) = (fw / rect.width, fh / rect.height)
            let i0 = max(0, ((seen.minX - rect.minX) * sx).rounded(.down)), i1 = min(fw, ((seen.maxX - rect.minX) * sx).rounded(.up))
            let j0 = max(0, ((seen.minY - rect.minY) * sy).rounded(.down)), j1 = min(fh, ((seen.maxY - rect.minY) * sy).rounded(.up))
            guard i1 > i0, j1 > j0 else { return nil }
            r = CGRect(x: rect.minX + i0 / sx, y: rect.minY + j0 / sy, width: (i1 - i0) / sx, height: (j1 - j0) / sy)
            (w, h) = (i1 - i0, j1 - j0)
        }
        var scaled = k
        if w * h > CGFloat(cap) {
            let f = (CGFloat(cap) / (w * h)).squareRoot()
            (w, h) = (max(1, (w * f).rounded(.down)), max(1, (h * f).rounded(.down)))
            scaled = k * f
        }
        return (r, Int(w), Int(h), scaled)
    }
}

package enum FilterProgram {
    /// The lone `drop-shadow()` an encoded filter chain (`Filter::encode`,
    /// a box's or an SVG element's) is, if it is one.
    package static func dropShadow(program p: [Double]) -> (region: CGRect, sigma: CGFloat, offset: CGSize, color: CGColor, opacity: Float)? {
        // region (4), count, then code, in, in2, subregion (4), linear, σx, σy, dx, dy, rgba.
        guard p.count == 5 + 8 + 8, p[4] == 1, p[5] == 6, p[6] == -1, p[13] == p[14], p[13] >= 0,
              p[15].isFinite, p[16].isFinite else { return nil }
        let region = CGRect(x: p[0], y: p[1], width: p[2], height: p[3])
        let sub = CGRect(x: p[8], y: p[9], width: p[10], height: p[11])
        guard region.width > 0, region.height > 0, sub.contains(region) || sub == region else { return nil }
        return (region, CGFloat(p[13]), CGSize(width: p[15], height: p[16]),
                CGColor(srgbRed: p[17], green: p[18], blue: p[19], alpha: 1), Float(min(max(p[20], 0), 1)))
    }
}
