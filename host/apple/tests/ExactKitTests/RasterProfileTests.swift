import XCTest
import ImageIO
import CoreGraphics
@testable import ExactKit
@testable import ExactImages

final class RasterProfileTests: XCTestCase {
    private let dir = Bundle.module.resourceURL!.appendingPathComponent("ColorFixtures")

    func testTheShopProfileUsesNamedSRGBWithoutChangingPixelsOrStorage() throws {
        let (thumbnail, plan) = try thumbnail("shop-c2ci.jpg")
        XCTAssertEqual(thumbnail.colorSpace.map(colorSpaceName), "icc")
        let stored = try XCTUnwrap(RasterImage.normalized(thumbnail, plan: plan))
        XCTAssertEqual(stored.colorSpace?.name, CGColorSpace.sRGB)
        XCTAssertEqual(stored.colorSpace.map(colorSpaceName), "srgb", "the agent's decode fact")
        XCTAssertEqual(stored.width, thumbnail.width)
        XCTAssertEqual(stored.height, thumbnail.height)
        XCTAssertEqual(stored.bytesPerRow, thumbnail.bytesPerRow)
        XCTAssertEqual(stored.bitmapInfo, thumbnail.bitmapInfo)
        XCTAssertEqual(stored.dataProvider?.data as Data?, thumbnail.dataProvider?.data as Data?)
        for space in [CGColorSpace.sRGB, CGColorSpace.displayP3, CGColorSpace.adobeRGB1998,
                      CGColorSpace.linearSRGB, CGColorSpace.itur_2020] {
            XCTAssertEqual(try pixels(stored, in: space), try pixels(thumbnail, in: space), "same displayed colours in \(space)")
        }
        let redrawn = try XCTUnwrap(RasterImage.normalized(thumbnail, plan: plan, adopt: false))
        XCTAssertEqual(redrawn.colorSpace?.name, CGColorSpace.sRGB, "a required redraw uses the named space too")
        XCTAssertEqual(try pixels(redrawn, in: CGColorSpace.displayP3), try pixels(thumbnail, in: CGColorSpace.displayP3))
    }

    func testDifferentAndLookupTableProfilesKeepTheirOwnSpaceAndPixels() throws {
        let (shop, plan) = try thumbnail("shop-c2ci.jpg")
        let base = try XCTUnwrap(shop.colorSpace?.copyICCData() as Data?)
        let cache = RasterProfile()
        XCTAssertEqual(cache.namedSpace(try XCTUnwrap(shop.colorSpace)).name, CGColorSpace.sRGB,
                       "the matrix used to build the LUT profile must qualify on its own")
        let lut = try Data(contentsOf: dir.appendingPathComponent("srgb-lut.icc"))
        let payload = lut[try tagRange("A2B0", in: lut)]
        for tag in ["A2B0", "A2B1", "A2B2", "B2A0", "B2A1", "B2A2",
                    "D2B0", "D2B1", "D2B2", "D2B3", "B2D0", "B2D1", "B2D2", "B2D3"] {
            let profile = addingLookup(tag, payload: payload, to: base)
            let lookup = try XCTUnwrap(CGColorSpace(iccData: profile as CFData), tag)
            XCTAssertTrue(cache.namedSpace(lookup) === lookup, tag)
            let picture = try XCTUnwrap(shop.copy(colorSpace: lookup))
            let stored = try XCTUnwrap(RasterImage.normalized(picture, plan: plan))
            XCTAssertTrue(stored === picture, tag)
            XCTAssertEqual(try pixels(stored, in: CGColorSpace.displayP3), try pixels(picture, in: CGColorSpace.displayP3), tag)
        }
        let (adobe, adobePlan) = try thumbnail("adobe-rgb.jpg")
        let source = try XCTUnwrap(adobe.colorSpace)
        XCTAssertTrue(cache.namedSpace(source) === source)
        let stored = try XCTUnwrap(RasterImage.normalized(adobe, plan: adobePlan))
        XCTAssertEqual(stored.colorSpace, source)
        XCTAssertEqual(try pixels(stored, in: CGColorSpace.displayP3), try pixels(adobe, in: CGColorSpace.displayP3))
    }

    func testAProfileIsClassifiedOnceAcrossWorkersAndDistinctObjects() throws {
        let (shop, _) = try thumbnail("shop-c2ci.jpg")
        let profile = try XCTUnwrap(shop.colorSpace?.copyICCData())
        let cache = RasterProfile()
        DispatchQueue.concurrentPerform(iterations: 16) { _ in
            let space = CGColorSpace(iccData: profile)!
            XCTAssertEqual(cache.namedSpace(space).name, CGColorSpace.sRGB)
        }
        XCTAssertEqual(cache.testClassifications, 1, "one identity check per ICC byte sequence")
        let (adobe, _) = try thumbnail("adobe-rgb.jpg")
        let source = try XCTUnwrap(adobe.colorSpace)
        for _ in 0..<3 { XCTAssertTrue(cache.namedSpace(source) === source) }
        XCTAssertEqual(cache.testClassifications, 2, "failed identities are cached too")
    }

    func testColorantPerturbationsOfAcceptedProfilesKeepTheirOwnSpace() throws {
        let (shop, plan) = try thumbnail("shop-c2ci.jpg")
        let srgb = try XCTUnwrap(shop.colorSpace)
        let p3 = try equivalentProfile(CGColorSpace.displayP3)
        let cache = RasterProfile()
        for (base, named, values) in [(srgb, CGColorSpace.sRGB, [28576, 28577, 28579, 28580]),
                                      (p3, CGColorSpace.displayP3, [33760])] {
            XCTAssertEqual(cache.namedSpace(base).name, named, "the unmodified profile must qualify")
            let original = try XCTUnwrap(base.copyICCData() as Data?)
            let x = try tagRange("rXYZ", in: original).lowerBound + 8
            for value in values {
                var bytes = original
                putWord(value, at: x, in: &bytes)
                let space = try XCTUnwrap(CGColorSpace(iccData: bytes as CFData))
                for _ in 0..<3 { XCTAssertTrue(cache.namedSpace(space) === space, "rXYZ.X=\(value)") }
                let picture = try XCTUnwrap(shop.copy(colorSpace: space))
                let stored = try XCTUnwrap(RasterImage.normalized(picture, plan: plan))
                XCTAssertTrue(stored === picture, "rXYZ.X=\(value)")
                for destination in [CGColorSpace.sRGB, CGColorSpace.displayP3, CGColorSpace.adobeRGB1998] {
                    XCTAssertEqual(try pixels(stored, in: destination), try pixels(picture, in: destination))
                }
            }
        }
        XCTAssertEqual(cache.testClassifications, 7, "each changed profile is classified once")
    }

    func testEquivalentP3UsesNamedP3AndDeepPlansKeepTheirProfile() throws {
        let (image, plan) = try thumbnail("p3-camera.jpg")
        let source = try equivalentProfile(CGColorSpace.displayP3)
        XCTAssertNil(source.name, "a custom matrix profile, not already named P3")
        let picture = try XCTUnwrap(image.copy(colorSpace: source))
        let stored = try XCTUnwrap(RasterImage.normalized(picture, plan: plan))
        XCTAssertEqual(stored.colorSpace?.name, CGColorSpace.displayP3)
        XCTAssertEqual(stored.dataProvider?.data as Data?, picture.dataProvider?.data as Data?)
        for destination in [CGColorSpace.sRGB, CGColorSpace.displayP3, CGColorSpace.adobeRGB1998] {
            XCTAssertEqual(try pixels(stored, in: destination), try pixels(picture, in: destination))
        }
        for file in ["p3-camera.jpg", "shop-c2ci.jpg"] {
            let (image, _) = try thumbnail(file)
            let source = try XCTUnwrap(image.colorSpace)
            let metadata = try RasterMetadata.validated(width: image.width, height: image.height, orientation: 1,
                                                        encodedBytes: 1, headerBytes: 1, deep: true)
            let deep = try RasterDecodePlan(metadata: metadata, maxPixel: plan.maxPixel)
            XCTAssertTrue(RasterImage.storageSpace(source, plan: deep) === source)
        }
    }

    func testDuplicateEXIFUsesImageIOOrientationAndStillDecodes() throws {
        var bytes = try Data(contentsOf: dir.appendingPathComponent("p3-orient6.jpg"))
        let tiff = try XCTUnwrap(bytes.range(of: Data("Exif\0\0".utf8))).upperBound
        let start = tiff - 10
        let length = Int(bytes[start + 2]) << 8 | Int(bytes[start + 3])
        var second = bytes.subdata(in: start..<start + 2 + length)
        second[28] = 0
        second[29] = 1
        bytes.insert(contentsOf: second, at: start + 2 + length)
        let source = try XCTUnwrap(CGImageSourceCreateWithData(bytes as CFData, nil))
        let properties = try XCTUnwrap(CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any])
        let metadata = try RasterMetadata.read(prefix: bytes, encodedBytes: bytes.count)
        XCTAssertEqual(metadata.orientation, (properties[kCGImagePropertyOrientation] as? NSNumber)?.intValue)
        let plan = try RasterDecodePlan(metadata: metadata, maxPixel: 840)
        let expected = try XCTUnwrap(CGImageSourceCreateThumbnailAtIndex(source, 0, RasterImage.thumbnailOptions(plan)))
        let decoded = try RasterImage.decode(bytes, metadata: metadata, plan: plan, charge: Charge())
        XCTAssertEqual(decoded.image.width, expected.width)
        XCTAssertEqual(decoded.image.height, expected.height)
        XCTAssertEqual(try pixels(decoded.image, in: CGColorSpace.displayP3), try pixels(expected, in: CGColorSpace.displayP3))
    }

    private func equivalentProfile(_ name: CFString) throws -> CGColorSpace {
        var bytes = try XCTUnwrap(CGColorSpace(name: name)?.copyICCData() as Data?)
        let description = try tagRange("desc", in: bytes).lowerBound
        let ascii = bytes[description..<description + 4].elementsEqual("desc".utf8)
        bytes[description + (ascii ? 12 : 29)] = 81
        let space = try XCTUnwrap(CGColorSpace(iccData: bytes as CFData))
        XCTAssertNil(space.name, "only the description differs from the named reference")
        return space
    }

    private func word(_ bytes: Data, _ at: Int) -> Int {
        bytes[at..<at + 4].reduce(0) { $0 << 8 | Int($1) }
    }

    private func putWord(_ value: Int, at: Int, in bytes: inout Data) {
        for i in 0..<4 { bytes[at + i] = UInt8((value >> (8 * (3 - i))) & 255) }
    }

    private func tagRange(_ name: String, in bytes: Data) throws -> Range<Int> {
        let tag = try XCTUnwrap((0..<word(bytes, 128)).map { 132 + 12 * $0 }.first {
            bytes[$0..<$0 + 4].elementsEqual(name.utf8)
        })
        let offset = word(bytes, tag + 4)
        return offset..<offset + word(bytes, tag + 8)
    }

    private func addingLookup(_ tag: String, payload: Data.SubSequence, to base: Data) -> Data {
        var bytes = base
        let count = word(base, 128), entry = 132 + count * 12
        bytes.insert(contentsOf: repeatElement(UInt8(0), count: 12), at: entry)
        for i in 0..<count {
            let at = 132 + i * 12 + 4
            putWord(word(base, at) + 12, at: at, in: &bytes)
        }
        bytes.replaceSubrange(entry..<entry + 4, with: tag.utf8)
        putWord(bytes.count, at: entry + 4, in: &bytes)
        putWord(payload.count, at: entry + 8, in: &bytes)
        bytes.append(payload)
        putWord(bytes.count, at: 0, in: &bytes)
        putWord(count + 1, at: 128, in: &bytes)
        return bytes
    }

    private final class Charge: RasterBackingCharge, @unchecked Sendable {}

    private func thumbnail(_ file: String) throws -> (CGImage, RasterDecodePlan) {
        let bytes = try Data(contentsOf: dir.appendingPathComponent(file))
        let metadata = try RasterMetadata.read(prefix: bytes, encodedBytes: bytes.count)
        let plan = try RasterDecodePlan(metadata: metadata, maxPixel: 840)
        let source = try XCTUnwrap(CGImageSourceCreateWithData(bytes as CFData, nil))
        return (try XCTUnwrap(CGImageSourceCreateThumbnailAtIndex(source, 0, RasterImage.thumbnailOptions(plan))), plan)
    }

    private func pixels(_ image: CGImage, in name: CFString) throws -> Data {
        let context = try XCTUnwrap(CGContext(data: nil, width: image.width, height: image.height, bitsPerComponent: 8,
            bytesPerRow: image.width * 4, space: CGColorSpace(name: name)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        return Data(bytes: try XCTUnwrap(context.data), count: image.width * image.height * 4)
    }
}
