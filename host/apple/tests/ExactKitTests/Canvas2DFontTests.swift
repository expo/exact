import XCTest
import CoreGraphics
import CoreText
import QuartzCore
@testable import ExactKit
@testable import ExactSurfaces

final class Canvas2DFontTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() }

    private func list(_ records: [(Canvas2DOp, [Double])]) -> Data {
        var data = Data()
        func u32(_ value: UInt32) { withUnsafeBytes(of: value.littleEndian) { data.append(contentsOf: $0) } }
        u32(0x4432_4345); u32(1)
        for (op, numbers) in records {
            u32(op.rawValue); u32(UInt32(numbers.count))
            for value in numbers { withUnsafeBytes(of: value.bitPattern.littleEndian) { data.append(contentsOf: $0) } }
        }
        return data
    }

    private func font(_ size: Double) -> (Canvas2DOp, [Double]) {
        (.font, [size, 400, 0, 100, 0, 0, 0, 0, 0] + "system-ui".unicodeScalars.map { Double($0.value) })
    }

    private let cover: [(Canvas2DOp, [Double])] = [(.fillColor, [255, 255, 255, 1]), (.fillRect, [0, 0, 150, 100])]
    private let text: [(Canvas2DOp, [Double])] = [(.fillColor, [0, 0, 0, 1]), (.fillText, [10, 50, 1, 0, 83, 99, 111, 114, 101])]

    private func apply(_ host: Canvas2DHost, _ parent: CALayer, _ records: [(Canvas2DOp, [Double])],
                       fresh: Bool = false, animating: Bool = true) {
        host.apply(7, ["lifetime": 1, "generation": 0, "fresh": fresh, "w": 150, "h": 100, "scale": 1,
                       "animating": animating, "lists": [list(records)]], layer: parent)
        waitFor("the canvas replay") { host.loadingCount == 0 }
    }

    private func pixels(_ parent: CALayer) throws -> Data {
        let layer = try XCTUnwrap(parent.sublayers?.first)
        let context = try XCTUnwrap(Canvas2DSpace.srgb8.context(width: 150, height: 100))
        if let recorder = layer.sublayers?.first as? Canvas2DRecordLayer, !recorder.isHidden {
            recorder.draw(in: context)
        } else {
            let image = try XCTUnwrap(layer.contents.map { $0 as! CGImage })
            context.draw(image, in: CGRect(x: 0, y: 0, width: 150, height: 100))
        }
        let bytes = try XCTUnwrap(context.data).assumingMemoryBound(to: UInt8.self)
        var data = Data(), hasInk = false
        for y in 0..<context.height {
            data.append(bytes.advanced(by: y * context.bytesPerRow), count: context.width * 4)
            for x in 0..<context.width {
                let offset = y * context.bytesPerRow + x * 4
                if bytes[offset] < 32 && bytes[offset + 1] < 32 && bytes[offset + 2] < 32 && bytes[offset + 3] == 255 { hasInk = true }
            }
        }
        XCTAssertTrue(hasInk, "the black text must draw over the white cover")
        XCTAssertTrue(data.contains(255), "the cover must draw")
        return data
    }

    func testPulsingTextKeepsOnlyTheRecordingFontsAndABoundedCache() throws {
        let host = Canvas2DHost(), parent = CALayer(), engine = TextEngine(resolve: { _ in nil })
        host.textEngine = { engine }
        host.place(7, in: CGRect(x: 0, y: 0, width: 150, height: 100), radii: [])
        apply(host, parent, cover + [font(40), (.save, [])] + text, fresh: true)
        let saved = try pixels(parent)
        for frame in 0..<512 {
            autoreleasepool {
                let size = 24 + 6 * sin(Double(frame) / 300)
                apply(host, parent, cover + [font(size)] + text)
                var f = Canvas2DFont(); f.size = size; f.families = ["system-ui"]
                _ = engine.canvasText.measure(f, "Score", rtl: false)
            }
        }
        let layer = try XCTUnwrap(parent.sublayers?.first?.sublayers?.first as? Canvas2DRecordLayer)
        let frame = try XCTUnwrap(Mirror(reflecting: layer).descendant("frame_") as? Canvas2DRecordLayer.Frame)
        let snapshot = try XCTUnwrap(frame.env as? Canvas2DSnapshot)
        XCTAssertLessThanOrEqual(snapshot.fonts.count, 1, "a covered recording needs only this frame's font")
        let cache = try XCTUnwrap(Mirror(reflecting: engine.canvasText).descendant("fonts") as? [String: CTFont])
        XCTAssertLessThanOrEqual(cache.count, 256, "animated sizes must not accumulate in the measurement cache")
        apply(host, parent, cover + [(.restore, [])] + text)
        XCTAssertEqual(try pixels(parent), saved, "a saved font still draws after cache eviction")
        apply(host, parent, cover + text, animating: false)
        XCTAssertEqual(try pixels(parent), saved, "the bitmap inherits the recording's font")
        host.forget(7)
    }

    func testASpaceTakesWordSpacingBesideLetterSpacing() {
        let engine = TextEngine(resolve: { _ in nil })
        func width(_ text: String, size: Double, letter: Double, word: Double, kerning: Int = 0) -> Double {
            var f = Canvas2DFont()
            f.size = size; f.families = ["system-ui"]; f.letterSpacing = letter; f.wordSpacing = word; f.kerning = kerning
            return engine.canvasText.measure(f, text, rtl: false)[0]
        }
        // Chrome: 164.570, 204.570, 174.570 and 214.570 for these four.
        let plain = width("AVAVA To", size: 40, letter: 0, word: 0)
        XCTAssertEqual(width("AVAVA To", size: 40, letter: 0, word: 10) - plain, 10, accuracy: 0.01)
        let tracked = width("AVAVA To", size: 40, letter: 5, word: 0)
        XCTAssertEqual(tracked - plain, 40, accuracy: 0.01)
        XCTAssertEqual(width("AVAVA To", size: 40, letter: 5, word: 10) - tracked, 10, accuracy: 0.01)
        XCTAssertEqual(width("AVAVA To", size: 40, letter: 5, word: 10, kerning: 2)
                       - width("AVAVA To", size: 40, letter: 5, word: 0, kerning: 2), 10, accuracy: 0.01, "without kerning")
        XCTAssertEqual(width("a b c d", size: 20, letter: 2, word: 8) - width("a b c d", size: 20, letter: 2, word: 0),
                       24, accuracy: 0.01, "three spaces")
    }

    func testKeptListsRetainTheirFontsUntilCoveredAndResetDropsSavedFonts() throws {
        let host = Canvas2DHost(), parent = CALayer(), engine = TextEngine(resolve: { _ in nil })
        host.textEngine = { engine }
        host.place(7, in: CGRect(x: 0, y: 0, width: 150, height: 100), radii: [])
        apply(host, parent, cover + [font(30), (.save, [])] + text, fresh: true)
        let smaller = [font(12), (.fillText, [10, 80, 1, 0, 83, 99, 111, 114, 101])]
        apply(host, parent, smaller)
        let both = try pixels(parent)
        apply(host, parent, [])
        XCTAssertEqual(try pixels(parent), both, "a recording replays earlier font records")
        apply(host, parent, [], animating: false)
        XCTAssertEqual(try pixels(parent), both, "the bitmap replays the same kept lists")
        apply(host, parent, [(.reset, [])] + cover + [font(12)] + text)
        let reset = try pixels(parent)
        apply(host, parent, cover + [(.restore, [])] + text)
        XCTAssertEqual(try pixels(parent), reset, "reset discards the saved 30px font")
        host.forget(7)
    }
}
