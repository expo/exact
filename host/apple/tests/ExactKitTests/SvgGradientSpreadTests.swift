import XCTest
@testable import ExactKit

/// A repeating or reflecting gradient draws every period its shape covers
/// (SVG 2 `spreadMethod`), however many there are (LLP 1055.000 D7).
class SvgGradientSpreadTests: XCTestCase {
    private let identity = [1, 0, 0, 1, 0, 0]

    private func color(_ v: Any?, _: Bool) -> CGColor? {
        guard let c = v as? [Double], c.count == 4 else { return nil }
        return CGColor(srgbRed: c[0] / 255, green: c[1] / 255, blue: c[2] / 255, alpha: c[3] / 255)
    }

    /// The red channel along row `y` of the gradient's raster over `rect`.
    private func reds(_ g: [String: Any], rect: CGRect, y: Int = 0) throws -> [Int] {
        let layer = try XCTUnwrap(SvgPaint.gradient(g, rect: rect, scale: 1, dark: false, color: color))
        let image = try XCTUnwrap(layer.contents) as! CGImage
        let bytes = CapturePixels.bytes(image)
        let row = image.width * 4
        return (0..<image.width).map { Int(bytes[y * row + $0 * 4]) }
    }

    /// Times the red channel falls by more than half: a repeat's seams.
    private func ramps(_ reds: [Int]) -> Int {
        zip(reds, reds.dropFirst()).filter { $0 - $1 > 127 }.count
    }

    /// Times the red channel turns from rising to falling.
    private func peaks(_ reds: [Int]) -> Int {
        var count = 0, rising = false
        for (a, b) in zip(reds, reds.dropFirst()) where a != b {
            if rising, b < a { count += 1 }
            rising = b > a
        }
        return count
    }

    private func linear(period: Double, spread: Int) -> [String: Any] {
        ["lg": [0, 0, period, 0], "t": identity, "sp": spread,
         "st": [[0, [0.0, 0, 0, 255]], [1, [255.0, 0, 0, 255]]]]
    }

    func testARepeatOfMoreThan256PeriodsDrawsEach() throws {
        // 1,026 pixels: the shape's 1,024 units and a unit each side.
        let rect = CGRect(x: 0, y: 0, width: 1024, height: 40)
        for (period, expected) in [(8.0, 128), (4.2, 244), (4.0, 256), (3.0, 341)] {
            let seams = ramps(try reds(linear(period: period, spread: 2), rect: rect))
            XCTAssert(abs(seams - expected) <= 2, "period \(period): \(seams) seams, \(expected) expected")
        }
    }

    func testAReflectionOfMoreThan256PeriodsTurnsAtEach() throws {
        let rect = CGRect(x: 0, y: 0, width: 1440, height: 40)
        let turns = peaks(try reds(linear(period: 5, spread: 1), rect: rect))
        XCTAssert(abs(turns - 144) <= 2, "\(turns) turns, 144 expected")
    }

    func testAConcentricRepeatOfMoreThan256RingsDrawsEach() throws {
        let g: [String: Any] = ["rg": [0, 0, 3, 0, 0, 0], "t": identity, "sp": 2,
                                "st": [[0, [0.0, 0, 0, 255]], [1, [255.0, 0, 0, 255]]]]
        // Row 1 is y = 0, through the centre: the rings are 3 units apart.
        let rings = ramps(try reds(g, rect: CGRect(x: 0, y: 0, width: 1024, height: 40), y: 1))
        XCTAssert(abs(rings - 341) <= 2, "\(rings) rings, 341 expected")
    }
}
