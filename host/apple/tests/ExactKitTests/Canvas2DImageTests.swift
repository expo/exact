import XCTest
import CoreGraphics
import ImageIO
@testable import ExactSurfaces

final class Canvas2DImageTests: XCTestCase {
    private let colors: [[UInt8]] = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]]

    private func jpeg(orientation: Int?) throws -> Data {
        var bytes: [UInt8] = []
        for y in 0..<20 {
            for x in 0..<40 { bytes += colors[(y < 10 ? 0 : 2) + (x < 20 ? 0 : 1)] }
        }
        let image = try XCTUnwrap(Canvas2DSpace.srgb8.image(bytes, width: 40, height: 20))
        let data = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(data, "public.jpeg" as CFString, 1, nil))
        var properties: [CFString: Any] = [kCGImageDestinationLossyCompressionQuality: 1]
        if let orientation { properties[kCGImagePropertyOrientation] = orientation }
        CGImageDestinationAddImage(destination, image, properties as CFDictionary)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        return data as Data
    }

    private func draw(_ host: Canvas2DHost, _ image: CGImage) throws -> CGContext {
        var data = Data()
        func u32(_ value: UInt32) { withUnsafeBytes(of: value.littleEndian) { data.append(contentsOf: $0) } }
        u32(0x4432_4345); u32(1)
        let records: [(Canvas2DOp, [Double])] = [(.image, [1, 112]),
            (.drawImage, [1, 0, 0, Double(image.width), Double(image.height), 0, 0, Double(image.width), Double(image.height)])]
        for (op, numbers) in records {
            u32(op.rawValue); u32(UInt32(numbers.count))
            for value in numbers { withUnsafeBytes(of: value.bitPattern.littleEndian) { data.append(contentsOf: $0) } }
        }
        let replay = Canvas2DReplayer(width: 60, height: 60, scale: 1, lifetime: 1, generation: 0)
        replay.env = host
        XCTAssertTrue(replay.apply(data))
        return try XCTUnwrap(replay.context)
    }

    private func pixel(_ context: CGContext, x: Int, y: Int) throws -> [UInt8] {
        let bytes = try XCTUnwrap(context.data).assumingMemoryBound(to: UInt8.self)
        let offset = y * context.bytesPerRow + x * 4
        return [bytes[offset + 2], bytes[offset + 1], bytes[offset], bytes[offset + 3]]
    }

    /// Past the budget the handle drawn least recently is let go, and the
    /// runtime told; one let go while a draw in flight used it is decoded again.
    func testDecodedHandlesPastTheBudgetAreLetGoLeastRecentlyDrawnFirst() throws {
        let budget = Canvas2DHost.imageBudget, recently = Canvas2DHost.drawnRecently
        defer { Canvas2DHost.imageBudget = budget; Canvas2DHost.drawnRecently = recently }
        Canvas2DHost.drawnRecently = 0
        let host = Canvas2DHost(), bytes = try jpeg(orientation: nil)
        host.assetBytes = { _ in bytes }
        var decoded: [String] = [], released: [String] = []
        host.onRelease = { released.append($0) }
        func load(_ src: String) throws {
            let loaded = expectation(description: "\(src) decoded")
            host.onImage = { src, image in XCTAssertNotNil(image); decoded.append(src); loaded.fulfill() }
            host.load([src])
            wait(for: [loaded], timeout: 10)
        }
        try load("a")
        let one = try XCTUnwrap(host.canvasImage("a")).bytesPerRow * 20
        Canvas2DHost.imageBudget = 2 * one
        try load("b")
        XCTAssertEqual(released, [], "two fit")
        host.drew(["a"], missed: [])
        try load("c")
        XCTAssertEqual(released, ["b"], "the one drawn least recently")
        XCTAssertNotNil(host.canvasImage("a"))
        XCTAssertNil(host.canvasImage("b"))
        let again = expectation(description: "b decoded again")
        host.onImage = { src, _ in decoded.append(src); again.fulfill() }
        host.drew([], missed: ["b"])
        wait(for: [again], timeout: 10)
        XCTAssertEqual(decoded, ["a", "b", "c", "b"])
        XCTAssertNotNil(host.canvasImage("b"))
    }

    func testImageHandlesReportAndDrawAllExifOrientationsAtFullSize() throws {
        let quadrants = [[0, 1, 2, 3], [0, 1, 2, 3], [1, 0, 3, 2], [3, 2, 1, 0], [2, 3, 0, 1],
                         [0, 2, 1, 3], [2, 0, 3, 1], [3, 1, 2, 0], [1, 3, 0, 2]]
        for orientation in 0...8 {
            let host = Canvas2DHost(), bytes = try jpeg(orientation: orientation == 0 ? nil : orientation)
            host.assetBytes = { _ in bytes }
            let loaded = expectation(description: "orientation \(orientation) decoded")
            var decoded: CGImage?
            host.onImage = { _, image in decoded = image; loaded.fulfill() }
            host.load(["p"])
            wait(for: [loaded], timeout: 10)
            let image = try XCTUnwrap(decoded)
            let (w, h) = orientation >= 5 ? (20, 40) : (40, 20)
            XCTAssertEqual(image.width, w, "orientation \(orientation)")
            XCTAssertEqual(image.height, h, "orientation \(orientation)")
            let context = try draw(host, image)
            for i in 0..<4 {
                let actual = try pixel(context, x: w * (i % 2 * 2 + 1) / 4, y: h * (i / 2 * 2 + 1) / 4)
                let expected = colors[quadrants[orientation][i]]
                for channel in 0..<4 {
                    XCTAssertEqual(Int(actual[channel]), Int(expected[channel]), accuracy: 8,
                                   "orientation \(orientation), quadrant \(i), channel \(channel)")
                }
            }
            XCTAssertEqual(try pixel(context, x: w + 5, y: 5), [0, 0, 0, 0])
            XCTAssertEqual(try pixel(context, x: 5, y: h + 5), [0, 0, 0, 0])
        }
    }
}
