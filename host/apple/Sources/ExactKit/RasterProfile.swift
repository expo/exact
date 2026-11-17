import Foundation
import CoreGraphics

/// Core Animation converts unnamed profiles at commit. Retag only when Core
/// Graphics produces exactly the named space's conversions (LLP 1171 D19).
/// Raster workers share the result by ICC bytes, including failed identities.
final class RasterProfile: @unchecked Sendable {
    static let shared = RasterProfile()
    private enum Match { case original, srgb, p3 }
    private let lock = NSLock()
    private var matches: [Data: Match] = [:]
    #if DEBUG
    private var classifications = 0
    var testClassifications: Int { lock.withLock { classifications } }
    #endif

    func namedSpace(_ source: CGColorSpace) -> CGColorSpace {
        guard source.model == .rgb,
              source.name != CGColorSpace.sRGB, source.name != CGColorSpace.displayP3,
              let data = source.copyICCData() as Data? else { return source }
        return lock.withLock {
            let match: Match
            if let cached = matches[data] { match = cached }
            else {
                #if DEBUG
                classifications += 1
                #endif
                if Self.isMatrix(data), let image = Self.probe(in: source) {
                    if Self.srgb?.matches(image) == true { match = .srgb }
                    else if Self.p3?.matches(image) == true { match = .p3 }
                    else { match = .original }
                } else { match = .original }
                matches[data] = match
            }
            switch match {
            case .original: return source
            case .srgb: return CGColorSpace(name: CGColorSpace.sRGB)!
            case .p3: return CGColorSpace(name: CGColorSpace.displayP3)!
            }
        }
    }

    private static func isMatrix(_ data: Data) -> Bool {
        let b = [UInt8](data)
        func word(_ at: Int) -> Int { b[at..<at + 4].reduce(0) { $0 << 8 | Int($1) } }
        guard b.count >= 132, word(0) == b.count,
              b[16..<20].elementsEqual("RGB ".utf8),
              b[20..<24].elementsEqual("XYZ ".utf8),
              b[36..<40].elementsEqual("acsp".utf8) else { return false }
        let count = word(128)
        guard count <= (b.count - 132) / 12 else { return false }
        var tags: [String: String] = [:]
        for i in 0..<count {
            let at = 132 + i * 12
            guard let tag = String(bytes: b[at..<at + 4], encoding: .ascii), tags[tag] == nil else { return false }
            // A lookup can override the matrix at any rendering intent.
            if ["A2B", "B2A", "D2B", "B2D"].contains(where: { tag.hasPrefix($0) }) { return false }
            let offset = word(at + 4), size = word(at + 8)
            guard offset >= 132 + count * 12, size >= 8,
                  offset <= b.count, size <= b.count - offset,
                  let type = String(bytes: b[offset..<offset + 4], encoding: .ascii) else { return false }
            tags[tag] = type
        }
        return ["rXYZ", "gXYZ", "bXYZ"].allSatisfy { tags[$0] == "XYZ " }
            && ["rTRC", "gTRC", "bTRC"].allSatisfy { tags[$0] == "curv" || tags[$0] == "para" }
    }

    private static func probe(in space: CGColorSpace) -> CGImage? {
        // The same 64³ lattice of 8-bit inputs is used for both source tags.
        var bytes = [UInt8](repeating: 255, count: 64 * 64 * 64 * 4)
        var i = 0
        for r in 0..<64 {
            for g in 0..<64 {
                for b in 0..<64 {
                    bytes[i] = UInt8(r * 255 / 63)
                    bytes[i + 1] = UInt8(g * 255 / 63)
                    bytes[i + 2] = UInt8(b * 255 / 63)
                    i += 4
                }
            }
        }
        guard let provider = CGDataProvider(data: Data(bytes) as CFData) else { return nil }
        return CGImage(width: 64 * 64, height: 64, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: 64 * 64 * 4, space: space,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    }

    private static let srgb = Reference(CGColorSpace.sRGB, other: CGColorSpace.displayP3)
    private static let p3 = Reference(CGColorSpace.displayP3, other: CGColorSpace.sRGB)

    private struct Reference {
        let space: CGColorSpace
        let destinations: [(space: CGColorSpace, float: Bool)]

        init?(_ name: CFString, other: CFString) {
            guard let space = CGColorSpace(name: name) else { return nil }
            var destinations: [(CGColorSpace, Bool)] = []
            // Named-to-named is a pass-through: compare that destination at
            // 8 bits. Compare cross-space conversions before 8-bit rounding,
            // which can hide a colour change at another display's boundary.
            for (name, float) in [(name, false), (other, true), (CGColorSpace.adobeRGB1998, true),
                                  (CGColorSpace.linearSRGB, true), (CGColorSpace.itur_2020, true)] {
                guard let destination = CGColorSpace(name: name) else { return nil }
                destinations.append((destination, float))
            }
            self.space = space
            self.destinations = destinations
        }

        func matches(_ source: CGImage) -> Bool {
            guard let image = source.copy(colorSpace: space) else { return false }
            return destinations.allSatisfy {
                guard let original = Self.draw(source, into: $0.space, float: $0.float),
                      let named = Self.draw(image, into: $0.space, float: $0.float) else { return false }
                return original == named
            }
        }

        private static func draw(_ image: CGImage, into space: CGColorSpace, float: Bool) -> Data? {
            let stride = image.width * (float ? 16 : 4)
            let bitmap = CGImageAlphaInfo.premultipliedLast.rawValue
                | (float ? CGBitmapInfo.floatComponents.rawValue | CGBitmapInfo.byteOrder32Little.rawValue : 0)
            guard let context = CGContext(data: nil, width: image.width, height: image.height,
                bitsPerComponent: float ? 32 : 8, bytesPerRow: stride, space: space, bitmapInfo: bitmap),
                  let pixels = context.data else { return nil }
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
            return Data(bytes: pixels, count: stride * image.height)
        }
    }
}
