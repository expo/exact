// @ref LLP 1056 D9 (as amended for expo/exact#188 and #504) — a capture's
// encoding: a canvas's pixels in sRGB as the type the source asked for, when
// ImageIO writes it, else as a PNG with straight alpha, which the runtime
// writes to the app file the source named (`host/apple/src/canvas_capture.rs`).
import CoreGraphics
import Foundation
import ImageIO
import IOSurface
import UniformTypeIdentifiers

/// `image` in sRGB as `type`, a media type, when ImageIO encodes it (JPEG,
/// HEIC and AVIF among them; not WebP), at `quality` (0 to 1, nil for 0.92,
/// `toBlob`'s default) where the type is lossy; else as a PNG, as
/// `HTMLCanvasElement.toBlob` falls back. ImageIO writes straight alpha from
/// a premultiplied image; a JPEG, which has none, is the image over opaque
/// black, as the HTML standard serializes a bitmap to a type without alpha.
/// A bitmap in another space (a `display-p3` canvas never reaches here: the
/// runner refuses it) is converted first.
func canvas2DEncode(_ image: CGImage, type: String, quality: Double?) -> Data? {
    let named = type.hasPrefix("image/") ? UTType(mimeType: type) : nil
    let encodable = CGImageDestinationCopyTypeIdentifiers() as? [String] ?? []
    let uti = named.flatMap { encodable.contains($0.identifier) ? $0 : nil } ?? .png
    let (w, h) = (image.width, image.height)
    var source = image
    if uti == .jpeg {
        guard let c = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0, space: canvas2DSRGB,
                                bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return nil }
        c.setFillColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
        c.fill(CGRect(x: 0, y: 0, width: w, height: h))
        c.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
        guard let opaque = c.makeImage() else { return nil }
        source = opaque
    } else if image.colorSpace?.name != CGColorSpace.sRGB {
        guard let c = Canvas2DSpace.srgb8.context(width: w, height: h) else { return nil }
        c.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
        guard let converted = c.makeImage() else { return nil }
        source = converted
    }
    let out = NSMutableData()
    guard let destination = CGImageDestinationCreateWithData(out, uti.identifier as CFString, 1, nil) else { return nil }
    let options = uti == .png ? nil : [kCGImageDestinationLossyCompressionQuality: quality ?? 0.92] as CFDictionary
    CGImageDestinationAddImage(destination, source, options)
    return CGImageDestinationFinalize(destination) ? out as Data : nil
}

/// A copy of the GPU's surface (premultiplied BGRA, `Canvas2DGpuCanvas`) as
/// an image: drawn into a bitmap of its own under a read-only lock, so no
/// image refers to the surface once it is unlocked.
func canvas2DImage(of surface: IOSurface, p3: Bool) -> CGImage? {
    surface.lock(options: .readOnly, seed: nil)
    defer { surface.unlock(options: .readOnly, seed: nil) }
    let (w, h, row) = (surface.width, surface.height, surface.bytesPerRow)
    let space = p3 ? CGColorSpace(name: CGColorSpace.displayP3)! : canvas2DSRGB
    guard let c = CGContext(data: surface.baseAddress, width: w, height: h, bitsPerComponent: 8, bytesPerRow: row,
                            space: space, bitmapInfo: canvas2DBitmapInfo),
          let shown = c.makeImage(), let own = Canvas2DSpace(p3: p3).context(width: w, height: h) else { return nil }
    own.setBlendMode(.copy)
    own.draw(shown, in: CGRect(x: 0, y: 0, width: w, height: h))
    return own.makeImage()
}
