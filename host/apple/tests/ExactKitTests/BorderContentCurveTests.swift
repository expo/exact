import XCTest
@testable import ExactKit

/// A replaced element's content meets its border's inner curve, as Chrome
/// draws them (CSS Backgrounds 3 §5.3), drawn offscreen.
final class BorderContentCurveTests: XCTestCase {
    func testContentMeetsTheBordersInnerCurve() throws {
        // 100×100, a 10-point border and only a top-right radius of 100:
        // the inner radius 90 is reduced to the content box's 80.
        let box = CGRect(x: 0, y: 0, width: 100, height: 100)
        let content = box.insetBy(dx: 10, dy: 10)
        let radii = [CGSize.zero, CGSize(width: 100, height: 100), .zero, .zero]
        let curve = BorderPaint.contentRadii(radii, outer: box, inner: content)
        XCTAssertEqual(curve[1], CGSize(width: 80, height: 80))

        let ctx = try XCTUnwrap(CGContext(data: nil, width: 100, height: 100, bitsPerComponent: 8, bytesPerRow: 400,
                                          space: CGColorSpaceCreateDeviceRGB(),
                                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        // The view's y axis, downward.
        ctx.translateBy(x: 0, y: 100); ctx.scaleBy(x: 1, y: -1)
        let red = CGColor(red: 1, green: 0, blue: 0, alpha: 1)
        BorderPaint.paint(ctx, box: box, widths: [10, 10, 10, 10], colors: Array(repeating: red, count: 4), radii: radii)
        ctx.addPath(BorderPaint.roundedRect(content, curve))
        ctx.clip()
        ctx.setFillColor(CGColor(red: 0, green: 0, blue: 1, alpha: 1))
        ctx.fill(content)
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        let alpha = { (x: Int, y: Int) in Int(bytes[y * 400 + x * 4 + 3]) }
        XCTAssertEqual(alpha(45, 20), 255, "(45,20) is under the border or the content")
        // Inside the outer curve, a circle about (0,100), everything is
        // covered: where the two antialiased edges meet, each in part.
        var thinnest = 255
        for y in 10..<90 {
            for x in 10..<90 where hypot(CGFloat(x) + 0.5, 99.5 - CGFloat(y)) < 99 {
                thinnest = min(thinnest, alpha(x, y))
            }
        }
        XCTAssertGreaterThan(thinnest, 128, "no gap between the border and the content")
        // Chrome's content edge along rows 20, 30 and 50 (its last blue pixel).
        for (y, last) in [(20, 48), (30, 62), (50, 78)] {
            XCTAssertGreaterThan(Int(bytes[y * 400 + last * 4 + 2]), 128, "row \(y): blue at \(last)")
            XCTAssertLessThan(Int(bytes[y * 400 + (last + 2) * 4 + 2]), 128, "row \(y): none at \(last + 2)")
        }
    }
}
