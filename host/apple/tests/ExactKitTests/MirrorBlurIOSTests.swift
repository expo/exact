#if os(iOS)
@testable import ExactKit
import CoreGraphics
import UIKit
import XCTest

/// An image mirror's blurs (`MirrorBlurs`, `MirrorBlurStore`): each level blurs by its sigma though it is made from the
/// level before, and a picture keeps drawing its blurs while those for another size or decode are made off the main
/// thread.
final class MirrorBlurIOSTests: XCTestCase {
    /// A picture `width` pixels wide, black on its left half and white on its right
    private func edge(width: Int = 400, height: Int = 400) -> CGImage {
        let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.setFillColor(gray: 0, alpha: 1); context.fill(CGRect(x: 0, y: 0, width: width / 2, height: height))
        context.setFillColor(gray: 1, alpha: 1); context.fill(CGRect(x: width / 2, y: 0, width: width - width / 2, height: height))
        return context.makeImage()!
    }

    /// The distance over which the image's middle row rises from 10% to 90% white, in the source's pixels
    private func rise(_ image: CGImage, sourceWidth: Int) -> CGFloat {
        let width = image.width, height = image.height
        var bytes = [UInt8](repeating: 0, count: width * height * 4)
        let context = CGContext(data: &bytes, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        let row = (0..<width).map { CGFloat(bytes[((height / 2) * width + $0) * 4]) / 255 }
        // Against the row's own dark and light, which a level's partly covered last pixels dim at its ends
        let (dark, light) = (row.min() ?? 0, row.max() ?? 1)
        func crossing(_ level: CGFloat) -> CGFloat {
            guard let i = row.indices.dropFirst().first(where: { row[$0] >= level }) else { return CGFloat(width) }
            let (a, b) = (row[i - 1], row[i])
            return CGFloat(i - 1) + (b > a ? (level - a) / (b - a) : 0)
        }
        return (crossing(dark + 0.9 * (light - dark)) - crossing(dark + 0.1 * (light - dark))) * CGFloat(sourceWidth) / CGFloat(width)
    }

    func testEachLevelBlursByItsSigmaThoughMadeFromTheOneBefore() {
        let source = edge(), sigmas: [CGFloat] = [2, 5, 12, 30]
        let made = MirrorBlurs(MirrorBlurs.Key(source: source, load: 1, sigmas: sigmas))
        XCTAssertEqual(made.images.count, sigmas.count)
        for (image, sigma) in zip(made.images, sigmas) {
            // A Gaussian blur rises from 10% to 90% over 2.563 sigmas
            XCTAssertEqual(rise(image, sourceWidth: source.width), 2.563 * sigma, accuracy: 0.2 * 2.563 * sigma, "the level of sigma \(sigma)")
            XCTAssertLessThanOrEqual(CGFloat(image.width), max(CGFloat(source.width) * 4 / sigma, 1) + 1, "at most four of its pixels to its sigma")
        }
    }

    func testAPictureKeepsItsBlursWhileThoseForAnotherSizeOrDecodeAreMadeOffTheMainThread() {
        let store = MirrorBlurStore(), first = edge(), sigmas: [CGFloat] = [2, 5, 12, 30]
        var landed: [[CGImage]] = []
        store.landed = { landed.append($0) }
        let made = store.images(MirrorBlurs.Key(source: first, load: 1, sigmas: sigmas))
        XCTAssertEqual(made.count, sigmas.count, "a picture with no blurs is blurred at once")
        XCTAssertTrue(zip(store.images(MirrorBlurs.Key(source: first, load: 1, sigmas: sigmas.map { $0 * 1.2 })), made).allSatisfy { $0 === $1 },
                      "blurs within a quarter of those asked for stand")
        // Shown at half the size: the blurs stand at once, and those for the new size land
        let doubled = sigmas.map { $0 * 2 }
        XCTAssertTrue(zip(store.images(MirrorBlurs.Key(source: first, load: 1, sigmas: doubled)), made).allSatisfy { $0 === $1 })
        // Decoded again before those land: the latest is what lands last
        let again = edge(width: 300)
        XCTAssertTrue(zip(store.images(MirrorBlurs.Key(source: again, load: 1, sigmas: doubled)), made).allSatisfy { $0 === $1 })
        let start = Date()
        while store.images(MirrorBlurs.Key(source: again, load: 1, sigmas: doubled)).first === made.first, Date().timeIntervalSince(start) < 10 {
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.02))
        }
        XCTAssertFalse(landed.isEmpty, "the new decode's blurs landed")
        XCTAssertEqual(landed.last?.first?.width, MirrorBlurs(MirrorBlurs.Key(source: again, load: 1, sigmas: doubled)).images.first?.width,
                       "what landed last is the latest decode's, at its size")
        // Another load is another picture: blurred at once, never drawn from the last one's blurs
        let other = store.images(MirrorBlurs.Key(source: edge(), load: 2, sigmas: sigmas))
        XCTAssertFalse(other.contains { image in landed.last?.contains { $0 === image } == true })
    }
}
#endif
