import XCTest
import ImageIO
@testable import ExactKit
@testable import ExactEffects
@testable import ExactSvg

/// SVG islands retain their coverage and phase when tiled or cropped
/// (LLP 1055.000 D7, D10, D14), within `FilterExtent.cap` pixels.
class SvgIslandTests: XCTestCase {
    #if os(iOS) || os(tvOS)
    func testBlendedLeafKeepsItsOpacityAndClip() throws {
        let rect = CGRect(x: 0, y: 0, width: 20, height: 20)
        let paper: [String: Any] = ["id": 1, "o": 1,
            "p": [0, 0, 0, 1, 20, 0, 1, 20, 20, 1, 0, 20, 3], "f": [52, 241, 236, 255]]
        for clipped in [false, true] {
            var ink = paper
            ink["id"] = 2; ink["bl"] = 1; ink["f"] = [253, 47, 124, 255]
            ink["o"] = clipped ? 1.0 : 0.5
            if clipped { ink["cl"] = ["s": [[[0, 0, 0, 1, 10, 0, 1, 10, 20, 1, 0, 20, 3], 0]]] }
            let ctx = try XCTUnwrap(SvgIsland.render([paper, ink], rect: rect, w: 20, h: 20, k: 1,
                transform: .identity, flip: true, dark: false, fonts: nil))
            let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
            for x in [5, 15] {
                let expected = clipped ? (x < 10 ? [52, 44, 115, 255] : [52, 241, 236, 255]) : [52, 143, 175, 255]
                for channel in 0..<4 {
                    XCTAssertEqual(Int(bytes[10 * ctx.bytesPerRow + x * 4 + channel]), expected[channel], accuracy: 2)
                }
            }
        }
    }

    func testOverflowVisibleBlendKeepsDiagonalSquareCaps() throws {
        let scene = SvgScene()
        scene.scale = 1
        scene.root.bounds = CGRect(x: 0, y: 0, width: 80, height: 80)
        let stroke: [String: Any] = ["id": 1, "o": 1, "bl": 1,
            "p": [0, 10, 10, 1, 30, 30], "s": [253, 47, 124, 255], "w": 20, "cap": 2, "ml": 1]
        scene.apply(["box": [0, 0, 40, 40], "t": [1, 0, 0, 1, 20, 20], "els": [stroke]], dark: false, clock: nil)
        let ctx = try XCTUnwrap(CGContext(data: nil, width: 80, height: 80, bitsPerComponent: 8, bytesPerRow: 320,
            space: XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB)), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.translateBy(x: 0, y: 80); ctx.scaleBy(x: 1, y: -1)
        scene.root.render(in: ctx)
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        for (x, y) in [(18, 30), (30, 18), (62, 50), (50, 62)] {
            XCTAssertGreaterThan(bytes[y * ctx.bytesPerRow + x * 4 + 3], 240, "square cap at \(x),\(y)")
        }
    }

    func testBlendRefusesAnAnimationItCannotSample() throws {
        let layer = CAShapeLayer()
        layer.path = CGPath(rect: CGRect(x: 0, y: 0, width: 20, height: 20), transform: nil)
        let animation = CABasicAnimation(keyPath: "opacity")
        animation.fromValue = 1; animation.toValue = 0; animation.duration = 1; animation.repeatCount = .infinity
        layer.add(animation, forKey: "fade")
        let renderer = SvgBlend()
        renderer.styles[ObjectIdentifier(layer)] = SvgBlend.Style(mode: 1, isolate: false)
        XCTAssertNil(renderer.bitmap(layer, rect: CGRect(x: 0, y: 0, width: 20, height: 20), w: 20, h: 20))
        XCTAssertNotNil(layer.animation(forKey: "fade"), "fallback leaves the original animation owned by Core Animation")
    }

    func testTransparentShadowStillIsolatesItsBlendedSource() throws {
        let region = [0.0, 0, 20, 20]
        let paper: [String: Any] = ["id": 1, "o": 1,
            "p": [0, 0, 0, 1, 20, 0, 1, 20, 20, 1, 0, 20, 3], "f": [255, 0, 0, 255]]
        let ink: [String: Any] = ["id": 3, "o": 1, "bl": 1,
            "p": [0, 5, 5, 1, 15, 5, 1, 15, 15, 1, 5, 15, 3], "f": [0, 0, 255, 255]]
        let program = region + [1, 6, -1, -3] + region + [0, 0, 0, 0, 0, 0, 0, 0, 0]
        let filter: [String: Any] = ["id": 2, "g": 1, "o": 1, "c": [Any](),
            "fl": ["r": region, "k": 1, "p": program, "c": [ink]]]
        XCTAssertEqual(try XCTUnwrap(SvgScene.dropShadow(filter["fl"] as! [String: Any])).opacity, 0)
        let ctx = try XCTUnwrap(SvgIsland.render([paper, filter], rect: CGRect(x: 0, y: 0, width: 20, height: 20),
            w: 20, h: 20, k: 1, transform: .identity, flip: true, dark: false, fonts: nil))
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        XCTAssertEqual(Array(UnsafeBufferPointer(start: bytes + 10 * ctx.bytesPerRow + 10 * 4, count: 4)), [0, 0, 255, 255])
        XCTAssertEqual(Array(UnsafeBufferPointer(start: bytes + 2 * ctx.bytesPerRow + 2 * 4, count: 4)), [255, 0, 0, 255])
    }

    func testBlendedShadowUsesDeviceAndViewBoxScale() throws {
        for zoom in [1.0, 2.0] {
            for sigma in [0.0, 2.0] {
                let region = [0.0, 0, 60, 60]
                let shape: [String: Any] = ["id": 3, "o": 1, "bl": 1,
                    "p": [0, 10, 10, 1, 20, 10, 1, 20, 20, 1, 10, 20, 3], "f": [0, 0, 0, 255]]
                let program = region + [1, 6, -1, -3] + region + [0, sigma, sigma, 0, 20, 1, 0, 0, 1]
                let filter: [String: Any] = ["id": 2, "g": 1, "o": 1, "c": [Any](),
                    "fl": ["r": region, "k": zoom, "p": program, "c": [shape]]]
                let rect = CGRect(x: 0, y: 0, width: 60 * zoom, height: 60 * zoom)
                let ctx = try XCTUnwrap(SvgIsland.render([filter], rect: rect,
                    w: Int(rect.width * 2), h: Int(rect.height * 2), k: 2,
                    transform: CGAffineTransform(scaleX: zoom, y: zoom), flip: true, dark: false, fonts: nil))
                let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
                func pixel(_ x: Int, _ y: Int, _ channel: Int) -> UInt8 {
                    bytes[Int(Double(y) * zoom * 2) * ctx.bytesPerRow + Int(Double(x) * zoom * 2) * 4 + channel]
                }
                XCTAssertGreaterThan(pixel(15, 35, 0), 200, "downward shadow at 2x, viewBox scale \(zoom)")
                XCTAssertEqual(pixel(15, 5, 0), 0, "no upward shadow")
                if sigma > 0 {
                    XCTAssertGreaterThan(pixel(23, 35, 3), 2, "blur scales with the offset")
                } else {
                    XCTAssertEqual(pixel(23, 35, 3), 0)
                }
            }
        }
    }

    func testAnEllipticalShadowFallsBackToEveryUnblendedShape() throws {
        let region = [0.0, 0, 60, 60]
        let paper: [String: Any] = ["id": 1, "o": 1,
            "p": [0, 0, 0, 1, 60, 0, 1, 60, 60, 1, 0, 60, 3], "f": [52, 241, 236, 255]]
        let ink: [String: Any] = ["id": 3, "o": 1, "bl": 1,
            "p": [0, 10, 10, 1, 20, 10, 1, 20, 20, 1, 10, 20, 3], "f": [253, 47, 124, 255]]
        let program = region + [1, 6, -1, -3] + region + [0, 2, 2, 0, 20, 1, 0, 0, 1]
        let filter: [String: Any] = ["id": 2, "g": 1, "o": 1, "c": [Any](),
            "fl": ["r": region, "k": 1, "p": program, "c": [ink]]]
        let rect = CGRect(x: 0, y: 0, width: 120, height: 60)
        let ctx = try XCTUnwrap(SvgIsland.render([paper, filter], rect: rect, w: 120, h: 60, k: 1,
            transform: CGAffineTransform(scaleX: 2, y: 1), flip: true, dark: false, fonts: nil))
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        func pixel(_ x: Int, _ y: Int) -> [UInt8] {
            Array(UnsafeBufferPointer(start: bytes + y * ctx.bytesPerRow + x * 4, count: 4))
        }
        XCTAssertEqual(pixel(30, 15), [253, 47, 124, 255], "unsupported blur leaves the ink unblended")
        XCTAssertEqual(pixel(100, 15), [52, 241, 236, 255], "the backdrop survives the failed picture")
    }

    func testMultiplyOverprintKeepsTheBackdropInsideTheSvg() throws {
        let rect = CGRect(x: 0, y: 0, width: 200, height: 120)
        let paper: [String: Any] = ["id": 1, "o": 1,
            "p": [0, 0, 0, 1, 200, 0, 1, 200, 120, 1, 0, 120, 3], "f": [246, 242, 233, 255]]
        let cyan: [String: Any] = ["id": 2, "o": 1,
            "p": [0, 20, 40, 1, 180, 40, 1, 180, 56, 1, 20, 56, 3], "f": [52, 241, 236, 255]]
        let pink: [String: Any] = ["id": 3, "n": 3, "h": 1, "o": 1, "bl": 1,
            "p": [0, 60, 0, 1, 60, 120], "s": [253, 47, 124, 255], "w": 16]
        let ctx = try XCTUnwrap(SvgIsland.render([paper, cyan, pink], rect: rect, w: 200, h: 120, k: 1,
                                               transform: .identity, flip: true, dark: false, fonts: nil))
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        let url = try XCTUnwrap(Bundle.module.url(forResource: "svg-overprint", withExtension: "png", subdirectory: "ColorFixtures"))
        let source = try XCTUnwrap(CGImageSourceCreateWithURL(url as CFURL, nil))
        let image = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
        let reference = try XCTUnwrap(CGContext(data: nil, width: 200, height: 120, bitsPerComponent: 8, bytesPerRow: 800,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        reference.draw(image, in: rect)
        let expected = try XCTUnwrap(reference.data).assumingMemoryBound(to: UInt8.self)
        var worst = 0
        for i in 0..<(200 * 120 * 4) { worst = max(worst, abs(Int(bytes[i]) - Int(expected[i]))) }
        XCTAssertLessThanOrEqual(worst, 1, "Chrome's SVG at one pixel per point, including the crossing and paper")

        let scene = SvgScene(), host = CALayer()
        scene.scale = 1
        scene.root.bounds = rect
        #if os(macOS)
        scene.root.isGeometryFlipped = true
        #endif
        host.addSublayer(scene.root)
        let payload: [String: Any] = ["box": [0, 0, 200, 120], "t": [1, 0, 0, 1, 0, 0], "clip": 1, "els": [paper, cyan, pink]]
        scene.apply(payload, dark: false, clock: nil)
        XCTAssertEqual(scene.target(at: CGPoint(x: 60, y: 48)), 3, "the rasterized stroke keeps its hit geometry")
        ctx.clear(rect)
        scene.root.render(in: ctx)
        worst = 0
        for i in 0..<(200 * 120 * 4) { worst = max(worst, abs(Int(bytes[i]) - Int(expected[i]))) }
        XCTAssertLessThanOrEqual(worst, 1, "the displayed SVG matches its offscreen island")
        var plain = pink
        plain["bl"] = 0
        var changed = payload
        changed["els"] = [paper, cyan, plain]
        scene.apply(changed, dark: false, clock: nil)
        ctx.clear(rect)
        scene.root.render(in: ctx)
        XCTAssertEqual(Array(UnsafeBufferPointer(start: bytes + 48 * ctx.bytesPerRow + 60 * 4, count: 4)), [253, 47, 124, 255], "removing the blend removes the picture")

    }

    func testOverflowVisibleBlendKeepsShadowOutsideGeometry() throws {
        let scene = SvgScene()
        scene.scale = 1
        scene.root.bounds = CGRect(x: 0, y: 0, width: 120, height: 80)
        let region = [0.0, 0, 120, 80]
        let context = try XCTUnwrap(CGContext(data: nil, width: 120, height: 80, bitsPerComponent: 8, bytesPerRow: 480,
            space: XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB)), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.translateBy(x: 0, y: 80); context.scaleBy(x: 1, y: -1)
        let bytes = try XCTUnwrap(context.data).assumingMemoryBound(to: UInt8.self)
        for sigma in [0.0, 3.0] {
            for placement in 0..<3 {
                let shape: [String: Any] = ["id": 3, "o": 1, "bl": placement == 2 ? 1 : 0,
                    "p": [0, 10, 30, 1, 30, 30, 1, 30, 50, 1, 10, 50, 3], "f": [0, 0, 0, 255]]
                let program = region + [1, 6, -1, -3] + region + [0, sigma, sigma, 40, 0, 1, 0, 0, 1]
                let filter: [String: Any] = ["id": 2, "g": 1, "o": 1, "bl": placement == 0 ? 1 : 0,
                    "c": [Any](), "fl": ["r": region, "k": 1, "p": program, "c": [shape]]]
                let sibling: [String: Any] = ["id": 4, "o": 1, "bl": placement == 1 ? 1 : 0,
                    "p": [0, 10, 30, 1, 20, 30, 1, 20, 40, 1, 10, 40, 3], "f": [0, 0, 0, 255]]
                scene.apply(["box": region, "t": [1, 0, 0, 1, 0, 0], "els": [filter, sibling]], dark: false, clock: nil)
                context.clear(scene.root.bounds)
                scene.root.render(in: context)
                let center = 40 * context.bytesPerRow + 60 * 4
                XCTAssertGreaterThan(bytes[center], 200, "shadow survives: placement \(placement), sigma \(sigma)")
                if sigma > 0 {
                    let edge = 40 * context.bytesPerRow + 72 * 4
                    XCTAssertGreaterThan(bytes[edge + 3], 2, "blur extends beyond the offset geometry")
                }
                scene.reset()
            }
        }
    }

    func testBlendedGroupKeepsItsGradientMask() throws {
        let rect = CGRect(x: 0, y: 0, width: 20, height: 20)
        let root = CALayer(), group = CALayer(), shape = CAShapeLayer(), mask = CAGradientLayer()
        shape.path = CGPath(rect: rect, transform: nil)
        shape.fillColor = CGColor(srgbRed: 1, green: 0, blue: 0, alpha: 1)
        group.addSublayer(shape); root.addSublayer(group)
        mask.frame = rect
        mask.colors = [CGColor(gray: 0, alpha: 1), CGColor(gray: 0, alpha: 0)]
        mask.startPoint = CGPoint(x: 0, y: 0.5); mask.endPoint = CGPoint(x: 1, y: 0.5)
        group.mask = mask
        let renderer = SvgBlend()
        renderer.styles[ObjectIdentifier(shape)] = SvgBlend.Style(mode: 1, isolate: false)
        let ctx = try XCTUnwrap(renderer.bitmap(root, rect: rect, w: 20, h: 20))
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        XCTAssertGreaterThan(bytes[10 * ctx.bytesPerRow + 4 + 3], 220)
        XCTAssertLessThan(bytes[10 * ctx.bytesPerRow + 18 * 4 + 3], 35)
        XCTAssertTrue(group.mask === mask, "rendering preserves the retained mask")
    }

    func testBlendedDescendantKeepsATranslatedUpperHalfMask() throws {
        let rect = CGRect(x: 0, y: 0, width: 40, height: 60)
        let root = CALayer(), group = CALayer(), shape = CAShapeLayer(), mask = CAShapeLayer()
        group.anchorPoint = .zero
        group.position = CGPoint(x: 5, y: 10)
        shape.path = CGPath(rect: CGRect(x: 0, y: 0, width: 20, height: 40), transform: nil)
        shape.fillColor = CGColor(srgbRed: 1, green: 0, blue: 0, alpha: 1)
        mask.path = CGPath(rect: CGRect(x: 0, y: 0, width: 20, height: 20), transform: nil)
        mask.fillColor = CGColor(gray: 1, alpha: 1)
        group.mask = mask
        group.addSublayer(shape); root.addSublayer(group)
        let renderer = SvgBlend()
        renderer.styles[ObjectIdentifier(shape)] = SvgBlend.Style(mode: 1, isolate: false)
        let ctx = try XCTUnwrap(renderer.bitmap(root, rect: rect, w: 40, h: 60))
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        XCTAssertEqual(bytes[15 * ctx.bytesPerRow + 10 * 4 + 3], 255, "upper half at the group's translated origin")
        XCTAssertEqual(bytes[45 * ctx.bytesPerRow + 10 * 4 + 3], 0, "lower half stays masked")
        XCTAssertTrue(group.mask === mask)
    }

    func testAChangedFilterLandsBeforeItsBlendPictureIsCached() throws {
        let scene = SvgScene()
        scene.scale = 1
        scene.root.bounds = CGRect(x: 0, y: 0, width: 60, height: 40)
        let region = [0, 0, 60, 40]
        let program = region + [1, 0, -1, -3] + region + [0, 2, 2]
        func payload(_ color: [Int], blend: Int) -> [String: Any] {
            let shape: [String: Any] = ["id": 3, "o": 1,
                "p": [0, 0, 0, 1, 40, 0, 1, 40, 40, 1, 0, 40, 3], "f": color]
            let filter: [String: Any] = ["id": 2, "g": 1, "o": 1, "c": [Any](),
                "fl": ["r": region, "k": 1, "p": program, "c": [shape]]]
            let sibling: [String: Any] = ["id": 4, "o": 1, "bl": blend,
                "p": [0, 50, 0, 1, 60, 0, 1, 60, 40, 1, 50, 40, 3], "f": [253, 47, 124, 255]]
            return ["box": region, "clip": 1, "t": [1, 0, 0, 1, 0, 0], "els": [filter, sibling]]
        }
        let context = try XCTUnwrap(CGContext(data: nil, width: 60, height: 40, bitsPerComponent: 8, bytesPerRow: 240,
            space: XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB)), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.translateBy(x: 0, y: 40); context.scaleBy(x: 1, y: -1)
        let bytes = try XCTUnwrap(context.data).assumingMemoryBound(to: UInt8.self)
        for initialBlend in [1, 0] {
            scene.apply(payload([255, 0, 0, 255], blend: initialBlend), dark: false, clock: nil)
            scene.apply(payload([0, 0, 255, 255], blend: 1), dark: false, clock: nil)
            SvgFilterLive.waitForDraws()
            XCTAssertEqual(SvgFilterLive.inFlight, 0)
            context.clear(scene.root.bounds)
            scene.root.render(in: context)
            let at = 20 * context.bytesPerRow + 20 * 4
            XCTAssertLessThan(bytes[at], 5, "the old red filter is gone")
            XCTAssertGreaterThan(bytes[at + 2], 250, "one change and settlement shows the blue filter")
            XCTAssertGreaterThan(bytes[at + 3], 250)
            scene.reset()
        }
    }
    #else
    func testMacBlendAndIsolationBelongToThePlacedLayer() throws {
        let scene = SvgScene(), backdrop = CALayer()
        backdrop.backgroundColor = CGColor(srgbRed: 1, green: 0, blue: 0, alpha: 1)
        backdrop.addSublayer(scene.root)
        var shape: [String: Any] = ["id": 1, "o": 1, "bl": 1, "iso": 1,
            "p": [0, 0, 0, 1, 20, 0, 1, 20, 20, 1, 0, 20, 3], "f": [0, 0, 255, 255]]
        func apply() {
            scene.apply(["box": [0, 0, 20, 20], "t": [1, 0, 0, 1, 0, 0], "els": [shape]], dark: false, clock: nil)
        }
        apply()
        let content = scene.root
        let layer = try XCTUnwrap(content.sublayers?.first as? CAShapeLayer)
        _ = try XCTUnwrap(layer.path)
        XCTAssertEqual(layer.compositingFilter as? String, "multiplyBlendMode")
        XCTAssertTrue(layer.shouldRasterize)
        XCTAssertEqual(content.opacity, 1, "Core Animation sees the external backdrop through the live SVG")
        XCTAssertEqual(scene.root.sublayers?.count, 1, "no flattened blend picture")
        shape["bl"] = 0; shape.removeValue(forKey: "iso")
        apply()
        XCTAssertNil(layer.compositingFilter)
        XCTAssertFalse(layer.shouldRasterize)
    }
    #endif

    func testAsymmetricIslandKeepsItsTopAndTranslatedOrigin() throws {
        let shape: [String: Any] = ["id": 1, "o": 1,
            "p": [0, 10, 20, 1, 30, 20, 1, 30, 30, 1, 10, 30, 3], "f": [255, 0, 0, 255]]
        let rect = CGRect(x: 10, y: 20, width: 20, height: 40)
        let region = [10, 20, 20, 40]
        let program = region + [1, 0, -1, -3] + region + [0, 0, 0]
        let filter: [String: Any] = ["id": 2, "g": 1, "o": 1, "c": [Any](),
            "fl": ["r": region, "k": 1, "p": program, "c": [shape]]]
        let ctx = try XCTUnwrap(SvgIsland.render([filter], rect: rect, w: 40, h: 80, k: 2,
            transform: .identity, flip: true, dark: false, fonts: nil))
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        XCTAssertEqual(bytes[10 * ctx.bytesPerRow + 20 * 4], 255, "top half draws at the cropped origin")
        XCTAssertEqual(bytes[70 * ctx.bytesPerRow + 20 * 4 + 3], 0, "bottom half stays clear")
    }

    func testRotatedPatternHasNoSeamsBetweenOpaqueTiles() throws {
        let rect = CGRect(x: 0, y: 0, width: 32, height: 32)
        let cell: [String: Any] = ["id": 1, "n": 7, "o": 1,
                                   "p": [0, 0, 0, 1, 8, 0, 1, 8, 8, 1, 0, 8, 3],
                                   "f": [51, 65, 85, 255]]
        for angle in [CGFloat.pi / 4, -.pi / 6] {
            let t = CGAffineTransform(rotationAngle: angle)
            for scale: CGFloat in [1, 2] {
                for opacity in [1.0, 0.5] {
                    let pattern: [String: Any] = ["pt": [0, 0, 8, 8], "c": [cell], "o": opacity,
                                                  "t": [t.a, t.b, t.c, t.d, 0, 0]]
                    let layer = try XCTUnwrap(SvgIsland.pattern(pattern, rect: rect, scale: scale, dark: false, fonts: nil))
                    let image = try XCTUnwrap(layer.contents) as! CGImage
                    let data = try XCTUnwrap(image.dataProvider?.data)
                    let bytes = try XCTUnwrap(CFDataGetBytePtr(data))
                    var worst = 0
                    for y in 0..<image.height {
                        for x in 0..<image.width {
                            let alpha = Int(bytes[y * image.bytesPerRow + x * 4 + 3])
                            worst = max(worst, abs(alpha - Int((255 * opacity).rounded())))
                        }
                    }
                    XCTAssertLessThanOrEqual(worst, 1, "angle \(angle), scale \(scale), opacity \(opacity)")
                }
            }
        }
    }

    func testPatternPhaseFollowsItsTileOriginAndTransformAcrossCrops() throws {
        func paint(origin: CGPoint, rect: CGRect) throws -> CGImage {
            let x = origin.x, y = origin.y
            let cell: [String: Any] = ["id": 1, "n": 7, "o": 1,
                                       "p": [0, x, y, 1, x + 4, y, 1, x + 4, y + 8, 1, x, y + 8, 3],
                                       "f": [51, 65, 85, 255]]
            let t = CGAffineTransform(rotationAngle: .pi / 4).translatedBy(x: -x, y: -y)
            let pattern: [String: Any] = ["pt": [x, y, 8, 8], "c": [cell],
                                          "t": [t.a, t.b, t.c, t.d, t.tx, t.ty]]
            let layer = try XCTUnwrap(SvgIsland.pattern(pattern, rect: rect, scale: 1, dark: false, fonts: nil))
            return try XCTUnwrap(layer.contents) as! CGImage
        }
        let rect = CGRect(x: 0, y: 0, width: 32, height: 32)
        let whole = try paint(origin: .zero, rect: rect)
        let shifted = try paint(origin: CGPoint(x: 2, y: 3), rect: rect)
        XCTAssertEqual(CapturePixels.bytes(whole), CapturePixels.bytes(shifted))
        let part = try paint(origin: .zero, rect: CGRect(x: 5, y: 7, width: 16, height: 16))
        let a = try XCTUnwrap(whole.dataProvider?.data), b = try XCTUnwrap(part.dataProvider?.data)
        XCTAssertTrue(CapturePixels.hasPaint(CapturePixels.bytes(whole)), "the reference pattern draws")
        XCTAssertTrue(CapturePixels.hasPaint(CapturePixels.bytes(part)), "the cropped pattern draws")
        let pa = try XCTUnwrap(CFDataGetBytePtr(a)), pb = try XCTUnwrap(CFDataGetBytePtr(b))
        var worst = 0
        // Compare the requested paint bounds, excluding the raster's one-unit gutter.
        for y in 1..<(part.height - 1) {
            for x in 4..<((part.width - 1) * 4) {
                worst = max(worst, abs(Int(pa[(y + 7) * whole.bytesPerRow + 5 * 4 + x]) - Int(pb[y * part.bytesPerRow + x])))
            }
        }
        XCTAssertLessThanOrEqual(worst, 1)
    }

    func testUnderTheCapIsTheWholeRegion() {
        let rect = CGRect(x: -12, y: -8, width: 144, height: 96)
        let e = FilterExtent.extent(rect, k: 3, seen: CGRect(x: 0, y: 0, width: 10, height: 10))
        XCTAssertEqual(e?.rect, rect)
        XCTAssertEqual(e?.w, 432)
        XCTAssertEqual(e?.h, 288)
        XCTAssertEqual(e?.k, 3)
    }

    /// The feature benchmark's repro: a 200% drop-shadow region on a group
    /// spanning a 1600 × 1000 view box shown with `slice` in a 1032 pt box
    /// at 2x, 24 M pixels whole.
    func testOverTheCapIsCutToWhatShowsOnTheSameGrid() throws {
        let rect = CGRect(x: -780, y: -470, width: 3120, height: 1920)
        let seen = CGRect(x: 284, y: 0, width: 1032, height: 1000).insetBy(dx: -10, dy: -10)
        let e = try XCTUnwrap(FilterExtent.extent(rect, k: 2, seen: seen))
        XCTAssertEqual(e.k, 2)
        XCTAssertEqual(e.w, 2104)
        XCTAssertEqual(e.h, 2040)
        XCTAssertEqual(e.rect, CGRect(x: 274, y: -10, width: 1052, height: 1020))
        XCTAssertLessThanOrEqual(e.w * e.h, FilterExtent.cap)
    }

    func testStillOverTheCapIsDrawnAtFewerPixels() throws {
        let rect = CGRect(x: 0, y: 0, width: 4000, height: 4000)
        let e = try XCTUnwrap(FilterExtent.extent(rect, k: 3, seen: nil))
        XCTAssertEqual(e.rect, rect)
        XCTAssertLessThanOrEqual(e.w * e.h, FilterExtent.cap)
        XCTAssertGreaterThan(e.w * e.h, FilterExtent.cap * 99 / 100)
        XCTAssertEqual(e.k, 3 * CGFloat(e.w) / 12000, accuracy: 0.001)
    }

    func testNothingShowsIsNothing() {
        let rect = CGRect(x: 0, y: 0, width: 4000, height: 4000)
        XCTAssertNil(FilterExtent.extent(rect, k: 3, seen: CGRect(x: 5000, y: 0, width: 10, height: 10)))
    }
}
