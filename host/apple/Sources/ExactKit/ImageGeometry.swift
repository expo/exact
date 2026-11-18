// Placing a bitmap in its box (`object-fit`, `image-rendering`), the tiles a
// pixelated one is drawn in, and a bitmap's HDR headroom: the core's, which
// text, icons and images all use (LLP 1047.001 D10; the images family is
// `ImageLink`'s).
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A PQ or HLG transfer (LLP 1100 D4).
package func isHDRSpace(_ space: CGColorSpace) -> Bool {
    #if os(tvOS)
    // The PQ/HLG-specific queries have no tvOS availability annotation.
    // This query covers both BT.2100 transfers and is available on tvOS.
    return CGColorSpaceUsesITUR_2100TF(space)
    #else
    return CGColorSpaceIsPQBased(space) || CGColorSpaceIsHLGBased(space)
    #endif
}

package enum ImageHeadroom {
    /// An image's headroom, where the system says one (iOS 18 / macOS 15);
    /// before that, PQ's and HLG's default (1000 / 203 cd/m²).
    package static func of(_ image: CGImage) -> Float {
        if #available(iOS 18, macOS 15, tvOS 18, *) { return of(reported: image.contentHeadroom, space: image.colorSpace) }
        return of(reported: 0, space: image.colorSpace)
    }

    /// Zero is unknown, not SDR. Preserve a known tag; otherwise BT.2100
    /// supplies its reference default, and other spaces remain untagged.
    package static func of(reported: Float, space: CGColorSpace?) -> Float {
        if reported > 0 { return reported }
        return space.map(isHDRSpace) == true ? 1000 / 203 : 0
    }
}

/// A one-pixel halo keeps the smooth pass continuous across bounded tiles.
package enum RasterTiles {
    package static func enlargement(_ image: CGImage, rect: CGRect, scale: CGSize) -> CGSize {
        CGSize(width: max(1, (rect.width * scale.width / CGFloat(image.width)).rounded()),
               height: max(1, (rect.height * scale.height / CGFloat(image.height)).rounded()))
    }

    package static func region(_ image: CGImage, rect: CGRect, visible: CGRect, enlargement n: CGSize) -> CGRect {
        let full = CGSize(width: CGFloat(image.width) * n.width, height: CGFloat(image.height) * n.height)
        guard full.width.isFinite, full.height.isFinite, rect.width > 0, rect.height > 0 else { return .null }
        let sx = rect.width / full.width, sy = rect.height / full.height
        let clip = visible.intersection(rect)
        guard !clip.isEmpty, !clip.isNull else { return .null }
        let left = floor((clip.minX - rect.minX) / sx / 256) * 256
        let bottom = floor((clip.minY - rect.minY) / sy / 256) * 256
        let right = min(full.width, ceil((clip.maxX - rect.minX) / sx / 256) * 256)
        let top = min(full.height, ceil((clip.maxY - rect.minY) / sy / 256) * 256)
        return CGRect(x: left, y: bottom, width: right - left, height: top - bottom)
    }

    package static func each(_ image: CGImage, rect: CGRect, visible: CGRect, enlargement n: CGSize, tint: CGColor?,
                     draw: (CGImage, CGRect, CGRect) -> Void) {
        let full = CGSize(width: CGFloat(image.width) * n.width, height: CGFloat(image.height) * n.height)
        let covered = region(image, rect: rect, visible: visible, enlargement: n)
        guard !covered.isNull, !covered.isEmpty else { return }
        let sx = rect.width / full.width, sy = rect.height / full.height
        func placed(_ r: CGRect) -> CGRect {
            CGRect(x: rect.minX + r.minX * sx, y: rect.minY + r.minY * sy, width: r.width * sx, height: r.height * sy)
        }
        for y in stride(from: covered.minY, to: covered.maxY, by: 256) {
            for x in stride(from: covered.minX, to: covered.maxX, by: 256) {
                autoreleasepool {
                    let core = CGRect(x: x, y: y, width: min(256, full.width - x), height: min(256, full.height - y))
                    let halo = core.insetBy(dx: -1, dy: -1).intersection(CGRect(origin: .zero, size: full))
                    guard let ctx = CGContext(data: nil, width: Int(halo.width), height: Int(halo.height),
                        bitsPerComponent: image.bitsPerComponent, bytesPerRow: 0,
                        space: image.colorSpace ?? CGColorSpaceCreateDeviceRGB(), bitmapInfo: image.bitmapInfo.rawValue) else { return }
                    RasterGeometry.draw(ctx, image, in: CGRect(x: -halo.minX, y: -halo.minY, width: full.width, height: full.height), tint: tint, rendering: "crisp-edges")
                    guard let pixels = ctx.makeImage() else { return }
                    draw(pixels, placed(core), placed(halo))
                }
            }
        }
    }
}

package enum RasterGeometry {
    package static func rect(natural: CGSize, content: CGRect, fit: String) -> CGRect {
        guard natural.width > 0, natural.height > 0 else { return .zero }
        if fit == "fill" { return content }
        let x = content.width / natural.width, y = content.height / natural.height
        let scale: CGFloat
        switch fit {
        case "contain": scale = min(x, y)
        case "cover": scale = max(x, y)
        case "scale-down": scale = min(1, min(x, y))
        case "none": scale = 1
        default: return content
        }
        let size = CGSize(width: natural.width * scale, height: natural.height * scale)
        return CGRect(x: content.midX - size.width / 2, y: content.midY - size.height / 2, width: size.width, height: size.height)
    }

    /// The bitmap into `rect`, or with a `-exact-tint-color` as a template: its
    /// alpha masks the tint (LLP 1011 §4). The same decoded pixels either way.
    /// A layer composited source-in, not `clip(to:mask:)`, which reads an
    /// image with alpha wrongly (a transparent pixel came back half covered).
    static func draw(_ ctx: CGContext, _ image: CGImage, in rect: CGRect, tint: CGColor?, rendering: String? = nil) {
        ctx.saveGState()
        defer { ctx.restoreGState() }
        if rendering == "crisp-edges" { ctx.interpolationQuality = .none }
        func picture() {
            if rendering == "pixelated" { pixelated(ctx, image, in: rect) }
            else { ctx.draw(image, in: rect) }
        }
        guard let tint else { picture(); return }
        ctx.saveGState()
        ctx.beginTransparencyLayer(in: rect, auxiliaryInfo: nil)
        picture()
        ctx.setBlendMode(.sourceIn)
        ctx.setFillColor(tint)
        ctx.fill(rect)
        ctx.endTransparencyLayer()
        ctx.restoreGState()
    }

    package static func filter(_ layer: CALayer, rendering: String?) {
        let filter: CALayerContentsFilter = rendering == "crisp-edges" ? .nearest : .linear
        layer.magnificationFilter = filter
        layer.minificationFilter = filter
    }

    private static func pixelated(_ ctx: CGContext, _ image: CGImage, in rect: CGRect) {
        let t = ctx.ctm
        let n = RasterTiles.enlargement(image, rect: rect, scale: CGSize(width: hypot(t.a, t.b), height: hypot(t.c, t.d)))
        ctx.interpolationQuality = .low
        if n == CGSize(width: 1, height: 1) { ctx.draw(image, in: rect); return }
        RasterTiles.each(image, rect: rect, visible: ctx.boundingBoxOfClipPath, enlargement: n, tint: nil) { pixels, core, halo in
            ctx.saveGState()
            ctx.setShouldAntialias(false)
            ctx.clip(to: core)
            ctx.draw(pixels, in: halo)
            ctx.restoreGState()
        }
    }

}

/// Zeroed memory for a bitmap that lives only while `body` runs. A large
/// one is pages returned to the system as it ends: a freed large malloc
/// block can stay dirty in the allocator's cache, charged to the process,
/// until it is reused. A small one is not worth the system calls.
package func withScratch<T>(_ count: Int, _ body: (UnsafeMutableRawPointer) throws -> T?) rethrows -> T? {
    guard count > 0 else { return nil }
    if count < 128 * 1024 {
        guard let small = calloc(count, 1) else { return nil }
        defer { free(small) }
        return try body(small)
    }
    guard let pages = mmap(nil, count, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0),
          pages != MAP_FAILED else { return nil }
    defer { munmap(pages, count) }
    return try body(pages)
}
