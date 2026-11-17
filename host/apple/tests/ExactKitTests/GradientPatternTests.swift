import CoreGraphics
import QuartzCore
import XCTest
@testable import ExactKit
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// #661: a repeating linear gradient background is one period, rasterized
/// once and tiled by its pattern at any angle, anchored where CSS puts the
/// period's start in the view; a resize moves the anchor and draws nothing.
final class GradientPatternTests: XCTestCase {
    private static let hatch = "repeating-linear-gradient(-45deg, transparent 0 4.243px, #e6e6e6 4.243px 5.657px)"

    /// Pixels `w`×`h` at `scale` over white, y down as a view's layer.
    private func picture(_ w: CGFloat, _ h: CGFloat, scale: CGFloat, _ paint: (CGContext) -> Void) -> [UInt8] {
        let (pw, ph) = (Int(w * scale), Int(h * scale))
        let ctx = CGContext(data: nil, width: pw, height: ph, bitsPerComponent: 8, bytesPerRow: pw * 4,
                            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        ctx.setFillColor(CGColor(gray: 1, alpha: 1)); ctx.fill(CGRect(x: 0, y: 0, width: pw, height: ph))
        ctx.translateBy(x: 0, y: CGFloat(ph)); ctx.scaleBy(x: scale, y: -scale)
        paint(ctx)
        return BorderParity.bytes(ctx)
    }

    /// Mean channel difference (0–255), the share of pixels beyond 48 and
    /// the largest difference.
    private func difference(_ a: [UInt8], _ b: [UInt8]) -> (mean: Double, over: Double, most: Int) {
        var (sum, over, most) = (0, 0, 0)
        for i in stride(from: 0, to: a.count, by: 4) {
            let d = (0..<3).map { abs(Int(a[i + $0]) - Int(b[i + $0])) }
            sum += d.reduce(0, +); over += d.max()! > 48 ? 1 : 0; most = max(most, d.max()!)
        }
        let pixels = Double(a.count / 4)
        return (Double(sum) / (3 * pixels), 100 * Double(over) / pixels, most)
    }

    private func layers(_ css: [String]) -> [Gradient] {
        css.compactMap { Gradient(["css": .string($0)] as BatchValue) }
    }

    /// The pattern as Core Graphics renders the layer (a capture, `render(in:)`)
    /// against the drawn painter Chrome's parity page holds, at every angle.
    func testThePatternPaintsWhatTheDrawnGradientPaints() {
        let cases: [[String]] = [
            [Self.hatch],
            [Self.hatch, "repeating-linear-gradient(45deg, transparent 0 4.243px, #e6e6e6 4.243px 5.657px)"],
            ["repeating-linear-gradient(30deg, #1e88e5 0 6px, #fdd835 6px 10px)"],
            ["repeating-linear-gradient(0deg, #e53935 0 3px, transparent 3px 9.5px)"],
            ["repeating-linear-gradient(90deg, #000 0 2px, #fff 2px 7px)"],
            ["repeating-linear-gradient(to top right, #43a047 0 5px, #1e88e580 5px 12px)"],
            ["repeating-linear-gradient(120deg, red 10%, blue 30%)"],
            ["repeating-linear-gradient(to right, red 0px, blue 0px)"],
        ]
        for css in cases {
            let gradients = layers(css)
            XCTAssertEqual(gradients.count, css.count)
            for scale: CGFloat in [1, 2, 3] {
                for (w, h) in [(100.0, 70.0), (100.0, 233.0)] as [(CGFloat, CGFloat)] {
                    let box = CGRect(x: 0, y: 0, width: w, height: h)
                    let drawn = picture(w, h, scale: scale) { ctx in
                        for g in gradients.reversed() { g.paint(ctx, clip: CGPath(rect: box, transform: nil), box: box, dark: false) }
                    }
                    let layer = GradientPatternLayer()
                    layer.show(gradients, bounds: box, box: box, dark: false, scale: scale)
                    let tiled = picture(w, h, scale: scale) { layer.render(in: $0) }
                    let (mean, over, most) = difference(drawn, tiled)
                    print("gradient pattern \(css) \(w)x\(h) @\(scale)x: mean \(String(format: "%.2f", mean))/255, \(String(format: "%.2f", over))% beyond 48, most \(most)")
                    // Core Graphics draws the cell as an image, resampled where it
                    // falls: half a texel (an eighth of a device pixel) back from
                    // the period's start, and turned. Where a period is whole
                    // pixels, a stripe's edge pixel takes up to an eighth of the
                    // next colour; elsewhere an edge is shaded by coverage where
                    // the drawn painter samples each pixel's centre.
                    let whole = css.allSatisfy { $0.contains("90deg") } || (css[0].contains("(0deg") && scale == 2)
                    if whole {
                        XCTAssert(most <= 32, "\(css) \(w)x\(h) @\(scale)x: \(most) apart, the drawn pixels where a period is whole pixels")
                    } else {
                        XCTAssert(mean < 7 && over < 10, "\(css) \(w)x\(h) @\(scale)x: mean \(mean), \(over)% beyond 48")
                    }
                }
            }
        }
    }

    /// A resize keeps the pattern, and so its cell: only the anchor moves.
    func testAResizeKeepsThePattern() throws {
        let gradients = layers([Self.hatch])
        let layer = GradientPatternLayer()
        var colors: [CGColor] = []
        var anchors: [CGPoint] = []
        for h: CGFloat in [874, 600, 873.5, 300] {
            let box = CGRect(x: 0, y: 0, width: 402, height: h)
            layer.show(gradients, bounds: box, box: box, dark: false, scale: 3)
            let stripes = try XCTUnwrap(layer.sublayers?.first)
            colors.append(try XCTUnwrap(stripes.backgroundColor))
            anchors.append(stripes.position)
        }
        XCTAssert(colors.allSatisfy { $0 === colors[0] }, "one pattern for every size")
        XCTAssertEqual(Set(anchors.map(\.y)).count, anchors.count, "the anchor follows the corner the period starts from")
    }

    /// A node shows the pattern and keeps no bitmap; a resize keeps the
    /// pattern; a box that draws for another reason draws its gradient too.
    func testANodeTilesItsTextureAndDrawsOnlyWhatElseDraws() throws {
        let p = Presenter()
        let node = NodeView(id: 1, kind: "view", presenter: p)
        p.views[1] = node
        #if os(macOS)
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 402, height: 874), styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.root
        defer { window.close() }
        p.root.addSubview(node)
        func show(_ size: CGSize) { node.frame = NSRect(origin: .zero, size: size); node.display() }
        #else
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.addSubview(node)
        func show(_ size: CGSize) { node.frame = CGRect(origin: .zero, size: size); node.layer.setNeedsDisplay(); node.layer.displayIfNeeded() }
        #endif
        var style: NodeStyle = ["background_color": .array([252, 252, 252, 255].map { .number($0) }),
                                "background_image": .array([Self.hatch, Self.hatch.replacingOccurrences(of: "-45deg", with: "45deg")].map { .object(["css": .string($0)]) })]
        node.applyStyle(style)
        show(CGSize(width: 402, height: 874))
        XCTAssertFalse(node.drawsPaint, "the texture is the pattern's, not draw(_:)'s")
        let pattern = try XCTUnwrap(node.boxPattern)
        let stripes = try XCTUnwrap(pattern.sublayers)
        XCTAssertEqual(stripes.count, 2)
        let colors = stripes.map(\.backgroundColor)
        show(CGSize(width: 402, height: 437))
        XCTAssertTrue(node.boxPattern === pattern)
        XCTAssert(zip(colors, pattern.sublayers!.map(\.backgroundColor)).allSatisfy { $0 === $1 }, "a resize draws no new pattern")
        XCTAssertEqual(pattern.frame, pattern.superlayer?.bounds)
        for side in ["top", "right", "bottom", "left"] {
            style["border_width_" + side] = .number(2)
            style["border_color_" + side] = .array([0, 0, 0, 255].map { .number($0) })
            style["border_style_" + side] = .string("dashed")
        }
        node.applyStyle(style)
        show(CGSize(width: 402, height: 437))
        XCTAssertTrue(node.drawsPaint, "a dashed border draws, and the gradients with it")
        XCTAssertNil(node.boxPattern)
    }
}
