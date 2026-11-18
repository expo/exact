// @ref LLP 1011.001 D3, D4 — an SVG file an image or an icon draws: parsed
// and resolved in Rust (`exact_svg_image_*`), its scene drawn as an island is
// (`SvgIsland.render`). `SvgLink.documents`.
import CExact.Svg
import CoreGraphics
import Foundation
import ExactKit
import ExactEffects

final class SvgFiles: SvgDocuments {
    func size(prefix: Data) -> CGSize? {
        var size = [Double](repeating: 0, count: 2)
        let isSVG = prefix.withUnsafeBytes { raw in
            exact_svg_image_header(raw.baseAddress?.assumingMemoryBound(to: UInt8.self), prefix.count, &size) == 1
        }
        return isSVG ? CGSize(width: size[0], height: size[1]) : nil
    }

    func scene(_ bytes: Data, width: Double, height: Double) -> (json: Data, flags: UInt32)? {
        var length = 0, flags: UInt32 = 0
        let buffer = bytes.withUnsafeBytes { raw in
            exact_svg_image_scene(raw.baseAddress?.assumingMemoryBound(to: UInt8.self), bytes.count, width, height, &length, &flags)
        }
        guard let buffer else { return nil }
        defer { exact_svg_image_free(buffer, length) }
        return (Data(bytes: buffer, count: length), flags)
    }

    func draw(_ elements: [Any], rect: CGRect, width: Int, height: Int, scale: CGFloat, transform: CGAffineTransform, fonts: Any??) -> CGContext? {
        SvgIsland.render(elements, rect: rect, w: width, h: height, k: scale, transform: transform, flip: true, dark: false,
                         fonts: fonts.map { SvgText.imageFonts($0) })
    }
}
