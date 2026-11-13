import XCTest
import CoreGraphics
import ImageIO
import QuartzCore
import CExact
@testable import ExactKit
@testable import ExactSurfaces

/// A canvas capture (LLP 1056 D9, as amended for expo/exact#188): the PNG
/// `Canvas2DHost.capture` makes is the bitmap the canvas shows, through
/// every list applied; for a canvas Core Animation records, what the
/// recording draws; and transparent for a completed draw with no operations.
/// The GPU's surface is `Canvas2DGpuStressTests`'s, which runs with the
/// module.
final class Canvas2DCaptureTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() } // LLP 1047.001 D4

    static func list(_ records: [(Canvas2DOp, [Double])]) -> Data {
        var d = Data()
        func u32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { d.append(contentsOf: $0) } }
        func f64(_ v: Double) { withUnsafeBytes(of: v.bitPattern.littleEndian) { d.append(contentsOf: $0) } }
        u32(0x4432_4345); u32(1)
        for (op, n) in records { u32(op.rawValue); u32(UInt32(n.count)); n.forEach(f64) }
        return d
    }

    /// An image's straight RGBA bytes, `w` × `h`.
    static func rgba(_ image: CGImage, _ w: Int, _ h: Int) -> [UInt8] {
        var px = [UInt8](repeating: 0, count: w * h * 4)
        let c = CGContext(data: &px, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                          space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        c.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
        return px
    }

    /// `host`'s PNG of canvas 7, decoded.
    static func capture(_ host: Canvas2DHost, w: Int, h: Int, generation: UInt32 = 0,
                        test: XCTestCase) throws -> CGImage {
        var png: Data??
        host.capture(7, lifetime: 1, generation: generation, sequence: 1, width: w, height: h) { png = .some($0) }
        test.waitFor("the capture") { png != nil }
        let data = try XCTUnwrap(png ?? nil, "the bitmap encodes")
        let source = try XCTUnwrap(CGImageSourceCreateWithData(data as CFData, nil))
        XCTAssertEqual(CGImageSourceGetType(source) as String?, "public.png")
        let image = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
        XCTAssertEqual(image.width, w); XCTAssertEqual(image.height, h)
        return image
    }

    private let red: [(Canvas2DOp, [Double])] = [(.fillColor, [220, 38, 38, 1]), (.fillRect, [0, 0, 150, 100])]
    private let blue: [(Canvas2DOp, [Double])] = [(.fillColor, [37, 99, 235, 1]), (.fillRect, [20, 20, 40, 30])]

    private func op(_ lists: [Data], fresh: Bool, animating: Bool) -> [String: Any] {
        ["lifetime": 1, "generation": 0, "seq": 1, "fresh": fresh, "w": 300, "h": 200, "scale": 2.0, "animating": animating, "lists": lists]
    }

    func testABitmapIsCapturedAsItShowsWithEveryListApplied() throws {
        let host = Canvas2DHost(), parent = CALayer()
        host.apply(7, op([Self.list(red)], fresh: true, animating: false), layer: parent)
        host.apply(7, op([Self.list(blue)], fresh: false, animating: false), layer: parent)
        let png = try Self.capture(host, w: 300, h: 200, test: self)
        host.waitForReplays(timeout: 5)
        let shown = try XCTUnwrap(parent.sublayers?.first?.contents as! CGImage?)
        let (a, b) = (Self.rgba(png, 300, 200), Self.rgba(shown, 300, 200))
        XCTAssertEqual(a, b, "the PNG is the bitmap shown")
        XCTAssertEqual(Array(a[0..<4]), [220, 38, 38, 255], "the first list's cover")
        let inBlue = (50 * 300 + 50) * 4
        XCTAssertEqual(Array(a[inBlue..<inBlue + 4]), [37, 99, 235, 255], "the second list, kept over it")
        host.forget(7)
    }

    func testARecordedCanvasIsCapturedAsItsRecordingDraws() throws {
        let fontURL = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("../../../../host/apple/tests/fonts/InterVariable.ttf").standardized
        let fontData = try Data(contentsOf: fontURL)
        let host = Canvas2DHost(), parent = CALayer(), engine = TextEngine(resolve: { _ in nil }, read: { _ in fontData })
        let family = Array("CaptureFont".utf8), source = Array("fixture.ttf".utf8)
        family.withUnsafeBufferPointer { f in
            source.withUnsafeBufferPointer { src in
                var face = ExactFontFace()
                face.family = f.baseAddress; face.family_len = f.count
                face.source = src.baseAddress; face.source_len = src.count
                face.stack = 8; face.weight = 400
                withUnsafePointer(to: &face) { row in
                    var catalog = ExactFontCatalog(); catalog.faces = row; catalog.count = 1
                    withUnsafePointer(to: &catalog) { engine.install($0) }
                }
            }
        }
        XCTAssertEqual(engine.stack(named: "CaptureFont"), 8)
        host.textEngine = { engine }
        let tile = try XCTUnwrap(Canvas2DSpace.srgb8.image([0, 255, 0, 255], width: 1, height: 1))
        let bytes = try XCTUnwrap(canvas2DPNG(tile))
        host.assetBytes = { _ in bytes }
        let loaded = expectation(description: "capture image decoded")
        host.onImage = { _, _ in loaded.fulfill() }
        host.load(["p"])
        wait(for: [loaded], timeout: 10)
        let resources: [(Canvas2DOp, [Double])] = [
            (.image, [1, 112]), (.drawImage, [1, 0, 0, 1, 1, 70, 10, 20, 20]),
            (.font, [24, 400, 0, 100, 0, 0, 0, 0, 0] + "CaptureFont".unicodeScalars.map { Double($0.value) }),
            (.fillColor, [0, 0, 0, 1]), (.fillText, [10, 80, 1, 0, 83, 99, 111, 114, 101]),
        ]
        let records = red + blue + resources
        host.apply(7, op([Self.list(records)], fresh: true, animating: true), layer: parent)
        for _ in 0..<3 { host.apply(7, op([Self.list(records)], fresh: false, animating: true), layer: parent); host.waitForReplays(timeout: 5) }
        let layer = try XCTUnwrap(parent.sublayers?.first)
        XCTAssertNil(layer.contents, "recorded: no bitmap shows")
        let whole = Canvas2DReplayer(width: 300, height: 200, scale: 2, lifetime: 1, generation: 0)
        whole.env = host
        XCTAssertTrue(whole.apply(Self.list(records)))
        let expected = Self.rgba(try XCTUnwrap(whole.image()), 300, 200)
        XCTAssertTrue(stride(from: 0, to: expected.count, by: 4).contains { Array(expected[$0..<$0 + 4]) == [0, 255, 0, 255] }, "the image draws")
        XCTAssertTrue(stride(from: 0, to: expected.count, by: 4).contains { Array(expected[$0..<$0 + 4]) == [0, 0, 0, 255] }, "the text draws")
        let png = try Self.capture(host, w: 300, h: 200, test: self)
        XCTAssertTrue(Self.rgba(png, 300, 200) == expected, "the recording capture keeps its images and text")
        host.forget(7)
    }

    func testARecordedCaptureKeepsTheFramesImageReadiness() throws {
        let host = Canvas2DHost(), parent = CALayer()
        let records = red + [(.image, [1, 112]), (.drawImage, [1, 0, 0, 1, 1, 70, 10, 20, 20])]
        // Start without animation so even a loaded GPU module leaves this canvas on Core Graphics.
        host.apply(7, op([Self.list(red)], fresh: true, animating: false), layer: parent)
        waitFor("the bitmap") { host.loadingCount == 0 }
        host.apply(7, op([Self.list(records)], fresh: false, animating: true), layer: parent)
        waitFor("the recording") { host.loadingCount == 0 }
        let layer = try XCTUnwrap(parent.sublayers?.first)
        XCTAssertNil(layer.contents)
        let recording = try XCTUnwrap(layer.sublayers?.first as? Canvas2DRecordLayer)
        recording.bounds = CGRect(x: 0, y: 0, width: 300, height: 200)
        let context = try XCTUnwrap(CGContext(data: nil, width: 300, height: 200, bitsPerComponent: 8,
                                             bytesPerRow: 1200, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                             bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        recording.draw(in: context)
        let shown = Self.rgba(try XCTUnwrap(context.makeImage()), 300, 200)
        let tile = try XCTUnwrap(Canvas2DSpace.srgb8.image([0, 255, 0, 255], width: 1, height: 1))
        let bytes = try XCTUnwrap(canvas2DPNG(tile))
        host.assetBytes = { _ in bytes }
        let loaded = expectation(description: "image decoded after the frame")
        host.onImage = { _, _ in loaded.fulfill() }
        host.load(["p"])
        wait(for: [loaded], timeout: 10)
        XCTAssertNotNil(host.canvasImage("p"))
        let captured = Self.rgba(try Self.capture(host, w: 300, h: 200, test: self), 300, 200)
        XCTAssertTrue(captured == shown, "a late image cannot change the recording's captured pixels")
        host.forget(7)
    }

    // The capture invariant in runner/src/canvas_capture.rs, with replay held behind a later resize.
    func testACaptureSeparatesWaitingReplaysAcrossAResize() throws {
        let replay = DispatchQueue(label: "test.canvas.capture")
        replay.suspend()
        let host = Canvas2DHost(replay: replay), parent = CALayer()
        host.apply(7, op([Self.list(red)], fresh: true, animating: false), layer: parent)
        var png: Data??
        host.capture(7, lifetime: 1, generation: 0, sequence: 1, width: 300, height: 200) { png = .some($0) }
        var resized = op([Self.list(blue)], fresh: true, animating: false)
        resized["generation"] = 1; resized["w"] = 400
        host.apply(7, resized, layer: parent)
        replay.resume()
        waitFor("the requested generation's capture") { png != nil }
        let data = try XCTUnwrap(png ?? nil)
        let source = try XCTUnwrap(CGImageSourceCreateWithData(data as CFData, nil))
        let image = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
        XCTAssertEqual(image.width, 300)
        XCTAssertEqual(Array(Self.rgba(image, 300, 200)[0..<4]), [220, 38, 38, 255])
        let next = try Self.capture(host, w: 400, h: 200, generation: 1, test: self)
        let bluePixel = (50 * 400 + 50) * 4
        XCTAssertEqual(Array(Self.rgba(next, 400, 200)[bluePixel..<bluePixel + 4]), [37, 99, 235, 255])
        host.forget(7)
    }

    // Rows of runner's captures_wait_for_a_completed_drawing at the replay boundary.
    func testCaptureCompletedDrawingOrderings() throws {
        for row in ["allocation", "empty", "later before snapshot", "later after snapshot"] {
            let replay = DispatchQueue(label: "test.canvas.capture")
            replay.suspend()
            let host = Canvas2DHost(replay: replay), parent = CALayer()
            var initial = op(row == "empty" || row == "allocation" ? [] : [Self.list(red)], fresh: true, animating: false)
            initial["seq"] = row == "allocation" ? 0 : 1
            host.apply(7, initial, layer: parent)
            var later = op([Self.list(blue)], fresh: false, animating: false)
            later["seq"] = 2
            if row == "later before snapshot" { host.apply(7, later, layer: parent) }
            var png: Data??
            host.capture(7, lifetime: 1, generation: 0, sequence: 1, width: 300, height: 200) { png = .some($0) }
            if row == "later after snapshot" { host.apply(7, later, layer: parent) }
            replay.resume()
            waitFor(row) { png != nil }
            if row == "allocation" {
                XCTAssertNil(png ?? nil, row)
            } else {
                let source = try XCTUnwrap(CGImageSourceCreateWithData(try XCTUnwrap(png ?? nil) as CFData, nil))
                let image = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
                let pixels = Self.rgba(image, 300, 200)
                let at = (50 * 300 + 50) * 4
                XCTAssertEqual(Array(pixels[at..<at + 4]), row == "empty" ? [0, 0, 0, 0] : row == "later before snapshot" ? [37, 99, 235, 255] : [220, 38, 38, 255], row)
            }
            host.forget(7)
        }
    }
}
