// @ref LLP 1056 D9 (as amended for expo/exact#188) — a capture's encoding:
// a canvas's pixels as a PNG, straight alpha in sRGB, which the runtime
// writes to the app file the source named (`host/apple/src/canvas_capture.rs`).
import CoreGraphics
import Foundation
import ImageIO
import IOSurface
import UniformTypeIdentifiers

/// `image` as a PNG in sRGB: ImageIO writes straight alpha from a
/// premultiplied image, and a bitmap in another space (a `display-p3`
/// canvas never reaches here: the runner refuses it) is converted first.
func canvas2DPNG(_ image: CGImage) -> Data? {
    var source = image
    if image.colorSpace?.name != CGColorSpace.sRGB {
        guard let c = Canvas2DSpace.srgb8.context(width: image.width, height: image.height) else { return nil }
        c.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        guard let converted = c.makeImage() else { return nil }
        source = converted
    }
    let out = NSMutableData()
    guard let destination = CGImageDestinationCreateWithData(out, UTType.png.identifier as CFString, 1, nil) else { return nil }
    CGImageDestinationAddImage(destination, source, nil)
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
