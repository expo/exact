#if os(macOS)
import AppKit
import ImageIO
import XCTest
import CExact
@testable import ExactKit
@testable import ExactVideo
@testable import ExactImages

/// A box Core Animation can paint keeps no bitmap on AppKit, as on UIKit
/// (`BoxLayerIOSTests`): background, one radius over a mask of corners, a
/// uniform border, a gradient and an image's pixels are layer properties
/// and sublayers, and the view answers `wantsUpdateLayer`, so AppKit never
/// gives it a backing store. Only what the layer cannot say still draws.
final class BoxLayerMacTests: XCTestCase {
    func testPercentageRadiusClipsMaterialAndOverflowAfterResize() throws {
        var style: NodeStyle = ["overflow_x": "hidden", "overflow_y": "hidden"]
        for corner in ["top_left", "top_right", "bottom_right", "bottom_left"] {
            style["border_radius_" + corner] = ["pct": 50]
        }
        let n = node(style, size: CGSize(width: 160, height: 80))
        n.props["backgroundMaterial"] = "ultra-thin"
        n.updateMaterial()
        n.applyBoxLayer()
        let material = try XCTUnwrap(n.materialView)
        material.wantsLayer = true
        let materialLayer = try XCTUnwrap(material.layer)
        let nodeLayer = try XCTUnwrap(n.layer)
        let materialMask = try XCTUnwrap(materialLayer.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(materialMask.path).contains(CGPoint(x: 20, y: 5)))
        XCTAssertTrue(try XCTUnwrap(materialMask.path).contains(CGPoint(x: 80, y: 5)))
        let clip = try XCTUnwrap(nodeLayer.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(clip.path).contains(CGPoint(x: 20, y: 5)))
        n.frame.size = CGSize(width: 80, height: 160)
        n.layoutSubtreeIfNeeded()
        let resized = try XCTUnwrap(materialLayer.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(resized.path).contains(CGPoint(x: 5, y: 20)))
        XCTAssertTrue(try XCTUnwrap(resized.path).contains(CGPoint(x: 5, y: 80)))
        n.applyStyle([:])
        n.updateMaterial()
        n.applyBoxLayer()
        XCTAssertNil(materialLayer.mask, "a later square style removes the elliptical clip")
        XCTAssertNil(nodeLayer.mask)
    }

    func testVideoPercentageRadiusUsesTheContentEdgeAndClears() throws {
        var style: NodeStyle = ["padding_left": 10, "padding_top": 10, "padding_right": 10, "padding_bottom": 10]
        for corner in ["top_left", "top_right", "bottom_right", "bottom_left"] {
            style["border_radius_" + corner] = ["pct": 50]
        }
        let n = node(style, size: CGSize(width: 160, height: 80))
        let media = MediaPlatformView()
        n.applyContentBox(padding: [10, 10, 10, 10])
        VideoView.layout(media, in: n)
        let mediaLayer = try XCTUnwrap(media.layer)
        XCTAssertEqual(media.frame, CGRect(x: 10, y: 10, width: 140, height: 60))
        let mask = try XCTUnwrap(mediaLayer.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(mask.path).contains(CGPoint(x: 10, y: 5)))
        XCTAssertTrue(try XCTUnwrap(mask.path).contains(CGPoint(x: 70, y: 5)))
        n.frame.size = CGSize(width: 80, height: 160)
        n.applyContentBox(padding: [10, 10, 10, 10])
        VideoView.layout(media, in: n)
        XCTAssertEqual(media.frame, CGRect(x: 10, y: 10, width: 60, height: 140))
        let resized = try XCTUnwrap(mediaLayer.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(resized.path).contains(CGPoint(x: 5, y: 10)))
        n.applyStyle([:])
        VideoView.layout(media, in: n)
        XCTAssertNil(mediaLayer.mask)
    }

    func testPercentageOverflowClipComposesWithAuthoredClipPath() throws {
        var style: NodeStyle = ["overflow_x": "hidden", "overflow_y": "hidden",
            "clip_path": ["commands": [["M", [0, 0]], ["L", [80, 0]], ["L", [80, 80]], ["L", [0, 80]], ["Z", []]]]]
        for corner in ["top_left", "top_right", "bottom_right", "bottom_left"] {
            style["border_radius_" + corner] = ["pct": 50]
        }
        let n = node(style, size: CGSize(width: 160, height: 80))
        n.applyBoxLayer()
        let mask = try XCTUnwrap(n.layer!.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(mask.path).contains(CGPoint(x: 20, y: 5)))
        let authored = try XCTUnwrap(mask.mask as? CAShapeLayer)
        XCTAssertTrue(try XCTUnwrap(authored.path).contains(CGPoint(x: 60, y: 40)))
        XCTAssertFalse(try XCTUnwrap(authored.path).contains(CGPoint(x: 100, y: 40)))
        let layer = try XCTUnwrap(n.layer)
        func alpha(_ x: Int, _ y: Int) -> UInt8 {
            let width = Int(n.bounds.width), height = Int(n.bounds.height)
            let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
            // An opaque descendant proves both masks clip real rendered content.
            let child = CALayer()
            child.frame = n.bounds
            child.backgroundColor = CGColor(gray: 1, alpha: 1)
            layer.addSublayer(child)
            defer { child.removeFromSuperlayer() }
            layer.render(in: context)
            let row = layer.isGeometryFlipped ? height - 1 - y : y
            return context.data!.assumingMemoryBound(to: UInt8.self)[(row * width + x) * 4 + 3]
        }
        XCTAssertEqual(alpha(20, 5), 0, "outside ellipse")
        XCTAssertEqual(alpha(60, 40), 255, "inside both masks")
        XCTAssertEqual(alpha(100, 40), 0, "outside authored clip")
        n.frame.size = CGSize(width: 80, height: 160)
        n.applyBoxLayer()
        XCTAssertEqual(alpha(5, 20), 0, "resized ellipse")
        XCTAssertEqual(alpha(40, 40), 255)
        XCTAssertEqual(alpha(40, 120), 0, "authored clip survives resize")
        n.applyStyle([:])
        n.applyBoxLayer()
        XCTAssertNil(layer.mask)
        XCTAssertEqual(alpha(5, 5), 255, "removing the style clears both masks")
    }

    func testPercentageOverflowClipSurvivesSurfaceRoundTrip() throws {
        for shadow in [false, true] {
            var style: NodeStyle = ["overflow_x": "hidden", "overflow_y": "hidden"]
            for corner in ["top_left", "top_right", "bottom_right", "bottom_left"] {
                style["border_radius_" + corner] = ["pct": 50]
            }
            if shadow {
                style["box_shadow"] = [["o": [0, 0], "b": 4, "s": 0, "c": [0, 0, 0, 128]]]
            }
            let n = node(style, size: CGSize(width: 160, height: 80))
            n.applyBoxLayer()
            if shadow { XCTAssertNotNil(n.clipBox) }
            let target = try XCTUnwrap(n.clipBox?.layer ?? n.layer)
            n.layoutResize = CGSize(width: -80, height: -40)
            n.applySurface()
            XCTAssertTrue(target.mask === n.surface?.clip)
            n.layoutResize = .zero
            n.applySurface()
            n.display()
            XCTAssertNil(n.surface)
            let mask = try XCTUnwrap(target.mask as? CAShapeLayer, "ellipse restored, shadow=\(shadow)")
            XCTAssertFalse(try XCTUnwrap(mask.path).contains(CGPoint(x: 20, y: 5)))
            XCTAssertTrue(try XCTUnwrap(mask.path).contains(CGPoint(x: 80, y: 5)))
        }
    }

    func testPercentageClipOnlyKindsUseAnEllipseWithoutACircularClip() throws {
        for kind in ["text", "canvas", "iframe"] {
            var style: NodeStyle = ["overflow_x": "hidden", "overflow_y": "hidden"]
            for corner in ["top_left", "top_right", "bottom_right", "bottom_left"] {
                style["border_radius_" + corner] = ["pct": 50]
            }
            let n = node(style, kind: kind, size: CGSize(width: 160, height: 80))
            n.applyBoxLayer()
            XCTAssertEqual(n.layer?.cornerRadius, 0, kind)
            let mask = try XCTUnwrap(n.layer?.mask as? CAShapeLayer)
            XCTAssertFalse(try XCTUnwrap(mask.path).contains(CGPoint(x: 20, y: 5)))
            XCTAssertTrue(try XCTUnwrap(mask.path).contains(CGPoint(x: 80, y: 5)))
        }
    }

    func testVideoStyleChangesRefreshGeometryWithoutPropChanges() throws {
        let n = node([:], size: CGSize(width: 160, height: 80))
        let video = VideoView(owner: n)
        n.video = video
        guard let media = n.subviews.first else {
            throw missing("module", "the video module is not beside this test runner (EXACT_VIDEO_DYLIB)")
        }
        video.update()
        var style: NodeStyle = ["padding_left": 10, "padding_top": 10, "padding_right": 10, "padding_bottom": 10]
        for corner in ["top_left", "top_right", "bottom_right", "bottom_left"] {
            style["border_radius_" + corner] = ["pct": 50]
        }
        n.applyStyle(style)
        n.applyContentBox(padding: [10, 10, 10, 10])
        XCTAssertEqual(media.frame, CGRect(x: 10, y: 10, width: 140, height: 60))
        let mask = try XCTUnwrap(media.layer?.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(mask.path).contains(CGPoint(x: 10, y: 5)))
        n.applyStyle([:])
        n.applyFieldContent(["rect": NSNull()])
        XCTAssertEqual(media.frame, n.bounds)
        XCTAssertNil(media.layer?.mask)
        n.applyStyle(style)
        XCTAssertNotNil(media.layer?.mask)
    }

    private let white: BatchValue = [255, 255, 255, 255]
    private let blue: BatchValue = [0, 136, 255, 255]

    func testPercentageRadiusUsesBothAxesAndFollowsResize() {
        var style: NodeStyle = ["background_color": [36, 104, 172, 255]]
        for corner in ["top_left", "top_right", "bottom_right", "bottom_left"] {
            style["border_radius_" + corner] = ["pct": 50]
        }
        let n = node(style, size: CGSize(width: 160, height: 80))
        XCTAssertEqual(n.cornerSizes(in: n.bounds), Array(repeating: CGSize(width: 80, height: 40), count: 4))
        XCTAssertFalse(n.roundedPath(in: n.bounds).cgPath.contains(CGPoint(x: 20, y: 5)))
        XCTAssertTrue(n.roundedPath(in: n.bounds).cgPath.contains(CGPoint(x: 80, y: 5)))
        n.frame.size = CGSize(width: 80, height: 160)
        XCTAssertEqual(n.cornerSizes(in: n.bounds), Array(repeating: CGSize(width: 40, height: 80), count: 4))
        XCTAssertNil(FlatPaint(style), "percentage geometry cannot use a fixed-radius flat leaf")
    }

    /// A content box layout sends with no frame or style change (a
    /// percentage padding whose containing block widened) repaints an
    /// image by the new one: a radius over a padded content box draws, so
    /// the next display draws it rather than keeping the sublayer's choice.
    func testAnImageFollowsAContentBoxSentAlone() throws {
        _ = NSApplication.shared
        let root = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("exact-content-box-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let context = try XCTUnwrap(CGContext(data: nil, width: 200, height: 100, bitsPerComponent: 8, bytesPerRow: 800,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.setFillColor(red: 1, green: 0, blue: 0, alpha: 1); context.fill(CGRect(x: 0, y: 0, width: 200, height: 100))
        let destination = try XCTUnwrap(CGImageDestinationCreateWithURL(root.appendingPathComponent("wide.png") as CFURL, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, try XCTUnwrap(context.makeImage()), nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        let p = Presenter(), loader = RasterLoader(), resolver = AssetResolver(root: root)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        let n = NodeView(id: 1, kind: "image", presenter: p)
        defer { loader.shutdown(); n.raster = nil; window.close(); withExtendedLifetime(resolver) {} }
        p.views[1] = n; p.viewport.addSubview(n)
        n.frame = NSRect(x: 0, y: 0, width: 40, height: 40)
        var style = round(20); style["object_fit"] = .string("cover"); style["padding_left"] = .object(["pct": .number(10)])
        n.applyStyle(style)
        n.loadGeneration = 1
        loader.load(n, source: "wide.png", resolver: resolver)
        let end = Date(timeIntervalSinceNow: 300)
        while n.raster == nil && Date() < end { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01)) }
        n.applyFieldContent(["rect": [0.0, 0.0, 40.0, 40.0]])
        n.display()
        XCTAssertNotNil(n.imageLayer, "an unpadded rounded image is a sublayer")
        XCTAssertNil(n.layer?.contents)
        n.applyFieldContent(["rect": [4.0, 0.0, 36.0, 40.0]])
        n.display()
        XCTAssertNil(n.imageLayer)
        XCTAssertNotNil(n.layer?.contents, "a radius over a padded content box draws")
    }

    /// A bordered rounded image's pixels end at the border's inner curve
    /// (CSS Backgrounds 3 §5.3), as Chrome draws an `<img>` with
    /// `border: 10px; border-radius: 30px`: a 20 px radius over the content
    /// box, not the border box's 30 px curve cut by the content box's
    /// square corners.
    func testABorderedRoundedImageIsClippedToTheBordersInnerCurve() throws {
        _ = NSApplication.shared
        let ctx = try XCTUnwrap(CGContext(data: nil, width: 100, height: 100, bitsPerComponent: 8, bytesPerRow: 400,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.setFillColor(red: 1, green: 0, blue: 0, alpha: 1); ctx.fill(CGRect(x: 0, y: 0, width: 100, height: 100))
        let png = try XCTUnwrap(NSBitmapImageRep(cgImage: try XCTUnwrap(ctx.makeImage())).representation(using: .png, properties: [:]))
        let p = Presenter(), loader = RasterLoader(), resolver = AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 200, height: 200), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        let n = NodeView(id: 1, kind: "image", presenter: p)
        defer { loader.shutdown(); n.raster = nil; window.close(); withExtendedLifetime(resolver) {} }
        p.views[1] = n; p.viewport.addSubview(n)
        n.frame = NSRect(x: 0, y: 0, width: 100, height: 100)
        var style = round(30); style["border_width"] = 10
        n.applyStyle(style)
        n.loadGeneration = 1
        loader.load(n, source: "data:image/png;base64," + png.base64EncodedString(), resolver: resolver)
        let end = Date(timeIntervalSinceNow: 5)
        while n.raster == nil && Date() < end { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01)) }
        XCTAssertNotNil(n.raster)
        n.applyFieldContent(["rect": [10.0, 10.0, 80.0, 80.0]])
        let rep = try XCTUnwrap(n.bitmapImageRepForCachingDisplay(in: n.bounds))
        n.cacheDisplay(in: n.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / 100
        func alpha(_ x: CGFloat, _ y: CGFloat) -> CGFloat { rep.colorAt(x: Int(x * scale), y: Int(y * scale))?.alphaComponent ?? -1 }
        for (x, y) in [(12.0, 12.0), (88.0, 12.0), (12.0, 88.0), (88.0, 88.0)] {
            XCTAssertEqual(alpha(x, y), 0, "(\(x), \(y)) is inside the border box's curve but outside the content edge's")
        }
        for (x, y) in [(20.0, 20.0), (80.0, 80.0), (50.0, 11.0)] {
            XCTAssertEqual(alpha(x, y), 1, "(\(x), \(y)) is inside the content edge's curve")
        }
    }

    /// A rounded image that does not clip and whose picture `draw(_:)`
    /// paints (here, inside a padding) keeps its `background-color` under
    /// the picture, as CSS paints a background: a fill sublayer would
    /// composite over the layer's contents and hide the picture.
    func testARoundedPaddedImageDrawsItsPictureOverItsBackground() throws {
        _ = NSApplication.shared
        let ctx = try XCTUnwrap(CGContext(data: nil, width: 100, height: 100, bitsPerComponent: 8, bytesPerRow: 400,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.setFillColor(red: 1, green: 0, blue: 0, alpha: 1); ctx.fill(CGRect(x: 0, y: 0, width: 100, height: 100))
        let png = try XCTUnwrap(NSBitmapImageRep(cgImage: try XCTUnwrap(ctx.makeImage())).representation(using: .png, properties: [:]))
        let p = Presenter(), loader = RasterLoader(), resolver = AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 200, height: 200), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        let n = NodeView(id: 1, kind: "image", presenter: p)
        defer { loader.shutdown(); n.raster = nil; window.close(); withExtendedLifetime(resolver) {} }
        p.views[1] = n; p.viewport.addSubview(n)
        n.frame = NSRect(x: 0, y: 0, width: 100, height: 100)
        var style = round(12); style["background_color"] = [128, 128, 128, 255]
        n.applyStyle(style)
        n.loadGeneration = 1
        loader.load(n, source: "data:image/png;base64," + png.base64EncodedString(), resolver: resolver)
        let end = Date(timeIntervalSinceNow: 5)
        while n.raster == nil && Date() < end { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01)) }
        XCTAssertNotNil(n.raster)
        // An 8 px padding: the content box layout sends.
        n.applyFieldContent(["rect": [8.0, 8.0, 84.0, 84.0]])
        n.display()
        // The layer tree as the window composites it.
        let layer = try XCTUnwrap(n.layer)
        let shot = try XCTUnwrap(CGContext(data: nil, width: 100, height: 100, bitsPerComponent: 8, bytesPerRow: 400,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        layer.render(in: shot)
        let pixels = try XCTUnwrap(shot.data).assumingMemoryBound(to: UInt8.self)
        func rgb(_ x: Int, _ y: Int) -> [UInt8] { let i = (y * 100 + x) * 4; return [pixels[i], pixels[i + 1], pixels[i + 2]] }
        XCTAssertEqual(rgb(50, 50), [255, 0, 0], "the picture")
        XCTAssertEqual(rgb(4, 50), [128, 128, 128], "the background in the padding")
    }

    /// LLP 1116: a padding that resolves at layout (`min(10%, 20px)`
    /// arrives as a calculation) reads the content box the kernel published,
    /// for the box and for each side; without one, the box's bounds.
    func testAPaddingResolvedAtLayoutReadsTheKernelsContentBox() throws {
        let padding = try XCTUnwrap(CalculatedLength([
            "calc": .string("min(10%, 20px)"),
            "env": [0, 0, 0, 0, 300, 600, 1, 1, 0, 0, 16]
        ]))
        let knots: NodeStyle = ["padding_top": .calculation(padding)]
        var style = knots
        style["padding_left"] = 6; style["border_width"] = 2
        let n = node(style, size: CGSize(width: 300, height: 120))
        n.applyContentBox(padding: [20, 0, 0, 6], border: [2, 2, 2, 2])
        XCTAssertEqual(n.contentBox(), CGRect(x: 8, y: 22, width: 290, height: 96))
        XCTAssertEqual(["top", "right", "bottom", "left"].map { n.padding($0) }, [20, 0, 0, 6])
        n.applyFieldContent(["rect": NSNull()])
        XCTAssertEqual(n.contentBox(), CGRect(x: 0, y: 0, width: 300, height: 120), "a box sent none is its own content box")
    }

    private func node(_ style: NodeStyle, kind: String = "view", size: CGSize = CGSize(width: 300, height: 120)) -> NodeView {
        _ = NSApplication.shared
        let p = Presenter()
        let n = NodeView(id: 1, kind: kind, presenter: p)
        n.frame = NSRect(origin: .zero, size: size)
        p.root.addSubview(n); p.views[1] = n
        n.applyStyle(style)
        n.display()
        return n
    }
    private func round(_ r: Double, _ corners: [String] = ["top_left", "top_right", "bottom_right", "bottom_left"]) -> NodeStyle {
        var s: NodeStyle = [:]
        for c in corners { s["border_radius_" + c] = .number(r) }
        return s
    }

    func testBackgroundAndMaskedRadiusAreLayerProperties() {
        let square = node(["background_color": white])
        XCTAssertTrue(square.wantsUpdateLayer)
        XCTAssertNil(square.layer?.contents)
        XCTAssertEqual(square.layer?.backgroundColor?.alpha, 1)
        var style = round(24, ["top_left", "top_right"]); style["background_color"] = white
        let card = node(style)
        XCTAssertTrue(card.wantsUpdateLayer)
        XCTAssertNil(card.layer?.contents)
        // AppKit makes a backing layer's radius clip, so a rounded box that
        // does not clip fills a sublayer under its children.
        XCTAssertNil(card.layer?.backgroundColor)
        XCTAssertEqual(card.layer?.cornerRadius, 0)
        XCTAssertEqual(card.layer?.masksToBounds, false, "rounding clips nothing while the overflow is visible")
        XCTAssertEqual((card.boxFill as? CAShapeLayer)?.fillColor?.alpha, 1)
        XCTAssertEqual(card.boxFill?.cornerRadius, 0)
        XCTAssertNotNil((card.boxFill as? CAShapeLayer)?.path)
        // The layer is flipped as the view is: minimum y is the top.
        XCTAssertEqual(card.boxFill?.maskedCorners, [.layerMinXMinYCorner, .layerMaxXMinYCorner])
        style["overflow_x"] = "hidden"; style["overflow_y"] = "hidden"
        let clipped = node(style)
        XCTAssertNotNil(clipped.layer?.mask, "a clipping box keeps its explicit corner outline")
        XCTAssertEqual(clipped.layer?.cornerRadius, 0)
        XCTAssertNotNil(clipped.boxFill as? CAShapeLayer)
    }

    /// The live row's waveform bar: a 3 × h pill of one colour.
    func testAPillPastHalfItsWidthIsReducedNotDrawn() {
        var style = round(1.5); style["background_color"] = blue
        let bar = node(style, size: CGSize(width: 3, height: 20))
        XCTAssertTrue(bar.wantsUpdateLayer)
        XCTAssertEqual(bar.boxFill?.cornerRadius ?? 0, 1.5, accuracy: 0.001)
        var wide = round(100); wide["background_color"] = blue
        XCTAssertEqual(node(wide, size: CGSize(width: 56, height: 28)).boxFill?.cornerRadius ?? 0, 14, accuracy: 0.001)
    }

    func testAUniformBorderStaysUnderTheChildrenUnlessTheyAreClipped() {
        var style = round(11); style["border_width"] = 2; style["border_color_top"] = blue
        let open = node(style, size: CGSize(width: 22, height: 22))
        XCTAssertTrue(open.wantsUpdateLayer)
        XCTAssertEqual(open.layer?.borderWidth, 0)
        XCTAssertEqual(open.boxBorder?.borderWidth, 2)
        XCTAssertEqual(open.boxBorder?.cornerRadius ?? 0, 11, accuracy: 0.001)
        style["overflow_x"] = "hidden"; style["overflow_y"] = "hidden"
        let closed = node(style, size: CGSize(width: 22, height: 22))
        XCTAssertTrue(closed.wantsUpdateLayer)
        XCTAssertEqual(closed.layer?.borderWidth, 2, "a clipping box's border covers what it clips")
        XCTAssertEqual(closed.layer?.masksToBounds, true)
        XCTAssertNil(closed.boxBorder)
    }

    /// A row's separator (`border-bottom`): a rectangle on a shape layer.
    func testSidesInOneColourAreAShapeLayer() {
        let row = node(["border_width_bottom": 0.5, "border_color_bottom": white, "border_color_top": white])
        XCTAssertTrue(row.wantsUpdateLayer)
        let shape = row.boxBorder as? CAShapeLayer
        XCTAssertNotNil(shape)
        XCTAssertEqual(shape?.path?.boundingBox.maxY, 120, "the bottom side, y down")
        XCTAssertGreaterThanOrEqual(shape?.path?.boundingBox.height ?? 0, 0.5, "at least one device pixel")
    }

    func testWhatTheLayerCannotSayStillDraws() {
        var corners = round(8, ["top_left"]); corners["background_color"] = white
        corners["border_radius_top_right"] = 4
        XCTAssertFalse(node(corners).wantsUpdateLayer, "radii that differ by corner draw")
        XCTAssertFalse(node(["background_color": white], kind: "canvas").wantsUpdateLayer)
        XCTAssertFalse(node(["background_color": white], kind: "iframe").wantsUpdateLayer)
    }

    func testAGradientIsASublayerWithTheBoxRadius() {
        let gradient: BatchValue = .object(["linear": .number(90), "stops": .array([0, 255, 0, 0, 255, 1, 0, 0, 255, 255].map { .number($0) })])
        var style = round(12); style["background_image"] = gradient
        let n = node(style)
        guard n.boxGradient != nil else { return XCTFail("no gradient layer for \(n.style)") }
        XCTAssertTrue(n.wantsUpdateLayer)
        XCTAssertEqual(n.boxGradient?.cornerRadius ?? 0, 12, accuracy: 0.001)
        XCTAssertEqual(n.boxGradient?.frame, n.bounds)
    }

    /// A scroller's children are its scroll view's document's, so its radius reaches them as the scroll view's
    /// corner (expo/exact#1442), following the style and the box's size. Under a border the clip is the padding
    /// edge's: a mask on the scroll view's layer, which stays its node's own subview at the border box, with its
    /// scroll position as it was and its scrollers inside the edge. Corners that differ, or that only some have,
    /// leave the scroll view square, and so does a box whose own layer clips.
    func testAScrollersRadiusClipsItsContent() throws {
        _ = NSApplication.shared
        let p = Presenter()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 200, height: 200), styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        defer { window.close() }
        func look(_ rows: [String: Any], x: String = "auto") -> [String: Any] {
            ["op": "style", "id": 2, "style": rows.merging(["overflow_x": x, "overflow_y": "auto", "background_color": [0, 0, 255, 255]]) { a, _ in a }]
        }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"], ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            look(["border_radius": 24.0]),
            ["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 200], ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 120, "h": 120],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 120, "h": 300], ["op": "content", "id": 2, "w": 120, "h": 300],
        ]))
        let node = try XCTUnwrap(p.views[2]), scroll = try XCTUnwrap(node.scroll)
        /// What a capture of the scroller painted at a point: "none", the red child, the blue ground under it, or the
        /// half green border over that ground. The child covers the whole box, so it shows wherever nothing clips it.
        func ink(_ x: Int, _ y: Int) throws -> String {
            let c = try XCTUnwrap(try XCTUnwrap(Capture.picture(of: node, scale: 1)).colorAt(x: x, y: y)?.usingColorSpace(.sRGB))
            guard c.alphaComponent > 0.5 else { return "none" }
            return c.redComponent > 0.3 ? "child" : c.greenComponent > 0.45 ? "border" : "ground"
        }
        XCTAssertFalse(node.clipsToBounds, "a box that scrolls both axes leaves the clip to its scroll view")
        XCTAssertEqual(scroll.layer?.cornerRadius, 24)
        XCTAssertEqual(scroll.layer?.masksToBounds, true)
        XCTAssertNil(scroll.layer?.mask, "with no border the scroll view's bounds are the padding edge")
        XCTAssertEqual(try ink(60, 60), "child", "the child fills the scroller")
        XCTAssertEqual(try ink(3, 3), "none", "and stops at the curve")

        // An 8 point border: the padding edge's corner has radius 16 round the box's centre, (24, 24). (10, 10) is
        // 19.8 from it, inside the box's curve and outside the padding edge's; (14, 14) is 14.1 from it, inside both.
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 8.0, "border_color_top": [0, 255, 0, 128]])]))
        for offset: CGFloat in [0, 40] {
            scroll.contentView.scroll(to: NSPoint(x: 0, y: offset))
            scroll.reflectScrolledClipView(scroll.contentView)
            XCTAssertEqual(scroll.contentView.bounds.origin.y, offset)
            XCTAssertEqual(try ink(60, 60), "child")
            XCTAssertEqual(try ink(14, 14), "child", "the child reaches the padding edge's curve, scrolled by \(offset)")
            XCTAssertEqual(try ink(10, 10), "border", "and stops there, so a translucent border shows the ground in its corner, scrolled by \(offset)")
            XCTAssertEqual(try ink(60, 4), "border", "and along its side, scrolled by \(offset)")
            XCTAssertEqual(try ink(3, 3), "none")
        }
        scroll.contentView.scroll(to: .zero)
        let edge = try XCTUnwrap(scroll.layer?.mask), padding = NSRect(x: 8, y: 8, width: 104, height: 104)
        XCTAssertTrue(scroll.superview === node, "the scroll view is still its node's own subview")
        XCTAssertEqual(scroll.frame, node.bounds, "at the border box")
        XCTAssertEqual(scroll.layer?.cornerRadius, 0, "no radius of a layer the size of the border box is the padding edge")
        let child = try XCTUnwrap(p.views[3])
        XCTAssertTrue(node.hitTest(NSPoint(x: 60, y: 60)) === child, "the child takes a point inside the padding edge")
        XCTAssertTrue(node.hitTest(NSPoint(x: 14, y: 14)) === child)
        XCTAssertTrue(node.hitTest(NSPoint(x: 4, y: 60)) === scroll, "the edge cuts the child away under the border: the point, and its wheel, are the scroll view's")
        XCTAssertTrue(node.hitTest(NSPoint(x: 10, y: 10)) === scroll, "and in the corner the edge rounds")
        // The scrollers, which the mask would cut where they lie over the border, keep inside it.
        for style in [NSScroller.Style.overlay, .legacy] {
            scroll.scrollerStyle = style
            scroll.tile()
            let bar = try XCTUnwrap(scroll.verticalScroller)
            XCTAssertFalse(bar.isHidden, "\(style.rawValue)")
            XCTAssertTrue(padding.contains(node.convert(bar.bounds, from: bar)), "\(style.rawValue): \(node.convert(bar.bounds, from: bar))")
            XCTAssertTrue(scroll.layer?.mask === edge, "\(style.rawValue): tiling leaves the mask")
        }
        scroll.scrollerStyle = .overlay
        node.layoutResize = CGSize(width: 0, height: 20); node.applyTransform(); node.applySurface()
        XCTAssertFalse(scroll.layer?.mask === edge, "a layout transition's surface masks the scroll view at its own size")
        let surfaceMask = try XCTUnwrap(scroll.layer?.mask)
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 2.0, "border_width_top": 12.0, "border_width_right": 6.0])]))
        XCTAssertTrue(NSEdgeInsetsEqual(scroll.clipInsets, NSEdgeInsets(top: 12, left: 2, bottom: 2, right: 6)), "new widths while the surface owns the mask")
        XCTAssertTrue(NSEdgeInsetsEqual(scroll.scrollerInsets, scroll.clipInsets))
        XCTAssertTrue(scroll.layer?.mask === surfaceMask)
        p.apply(wireBatch([look(["border_radius": 24.0])]))
        XCTAssertTrue(NSEdgeInsetsEqual(scroll.clipInsets, NSEdgeInsetsZero), "the border removed while the surface owns the mask")
        XCTAssertTrue(NSEdgeInsetsEqual(scroll.scrollerInsets, NSEdgeInsetsZero))
        XCTAssertTrue(scroll.layer?.mask === surfaceMask)
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 8.0, "border_color_top": [0, 255, 0, 128]])]))
        XCTAssertTrue(NSEdgeInsetsEqual(scroll.scrollerInsets, NSEdgeInsets(top: 8, left: 8, bottom: 8, right: 8)))
        node.layoutResize = .zero; node.applyTransform(); node.applySurface()
        XCTAssertTrue(scroll.layer?.mask === edge, "and the padding edge is back when it ends")
        XCTAssertEqual(try ink(10, 10), "border")

        // Borders of two widths make the padding edge's corner an ellipse, 8 by 20 round (24, 24).
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 4.0, "border_width_left": 16.0, "border_color_top": [0, 0, 0, 0]])]))
        XCTAssertEqual(try ink(18, 6), "ground", "outside the ellipse, inside the padding box and the box's curve")
        XCTAssertEqual(try ink(22, 12), "child", "inside it")
        XCTAssertEqual(scroll.scrollerInsets.left, 16)
        XCTAssertEqual(scroll.scrollerInsets.right, 4)
        // The top and bottom borders differ: the edge is the node's, top down, as the scroll view's layer is.
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 2.0, "border_width_top": 20.0, "border_color_top": [0, 0, 0, 0]])]))
        XCTAssertEqual(try ink(60, 12), "ground", "under the top border")
        XCTAssertEqual(try ink(60, 24), "child")
        XCTAssertEqual(try ink(60, 108), "child", "to the bottom border, which is thin")
        XCTAssertEqual(try ink(60, 119), "ground")
        // A border wider than the radius leaves the padding edge square.
        p.apply(wireBatch([look(["border_radius": 6.0, "border_width": 8.0, "border_color_top": [0, 0, 0, 0]])]))
        XCTAssertEqual(try ink(9, 9), "child")
        XCTAssertEqual(try ink(6, 6), "ground")

        p.apply(wireBatch([look(["border_radius": 8.0, "corner_shape": ["apple", "apple", "apple", "apple"]])]))
        XCTAssertNil(scroll.layer?.mask, "the border gone, the layer's radius again")
        XCTAssertTrue(node.hitTest(NSPoint(x: 4, y: 60)) === child, "and the child's again where the border was")
        XCTAssertTrue(scroll.superview === node)
        XCTAssertEqual(scroll.frame, node.bounds)
        XCTAssertEqual(scroll.scrollerInsets.right, 0)
        XCTAssertEqual(scroll.layer?.cornerRadius, 8, "a new radius")
        XCTAssertEqual(scroll.layer?.cornerCurve, .continuous)
        p.apply(wireBatch([look(["border_radius": 24.0]), ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 30, "h": 120]]))
        XCTAssertEqual(scroll.layer?.cornerRadius, 15, "reduced with the box")
        XCTAssertEqual(scroll.layer?.cornerCurve, .circular)
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 0, "y": 0, "w": 120, "h": 120]]))
        XCTAssertEqual(scroll.layer?.cornerRadius, 24)

        p.apply(wireBatch([look(["border_radius_top_left": 24.0, "border_radius_top_right": 24.0])]))
        XCTAssertEqual(scroll.layer?.cornerRadius, 0, "a capture's render has no corner mask")
        p.apply(wireBatch([look(["border_radius_top_left": 18.0, "border_radius_top_right": 4.0])]))
        XCTAssertEqual(scroll.layer?.cornerRadius, 0, "corners that differ need a path")
        p.apply(wireBatch([look(["border_radius": 24.0], x: "hidden")]))
        XCTAssertTrue(node.clipsToBounds, "one axis hidden: the box's own layer clips")
        XCTAssertEqual(node.layer?.cornerRadius, 24)
        XCTAssertEqual(try XCTUnwrap(node.scroll).layer?.cornerRadius, 0, "and its scroll view needs no corner")
    }

    /// A scroller clipped at its padding edge is driven as one that is not (expo/exact#1442): its scroll view finds
    /// its node by `superview`, for a trackpad drag's `scrolldragstart` and `scrolldragend`, the snap at its lift,
    /// the snap after a wheel's notch, and the wheel that takes a smooth scroll over. A rounded bordered scroller
    /// and a square bordered one, which has no clip of its own, take the same events to the same ends.
    func testAScrollerClippedAtItsPaddingEdgeKeepsItsDragItsSnapAndItsWheel() throws {
        guard !ExactEnv.agentFreezes else { throw XCTSkip("platform timing") }
        _ = NSApplication.shared
        let saved = ChainingScrollView.appliesAtOnce, savedLatch = ChainingScrollView.latch
        ChainingScrollView.appliesAtOnce = true // the handler applies each event itself, synchronously
        defer { ChainingScrollView.appliesAtOnce = saved; ChainingScrollView.latch = savedLatch }
        let p = Presenter()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 200), styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        defer { window.close() }
        var said: [String] = []
        p.onScrollDrag = { id, end in said.append(end.map { "\(id) end \($0.velocity.y)" } ?? "\(id) start") }
        func look(_ id: Int, _ rows: [String: Any]) -> [String: Any] {
            ["op": "style", "id": id, "style": rows.merging(["overflow_x": "auto", "overflow_y": "auto", "border_width": 8.0,
                                                               "border_color_top": [0, 255, 0, 128]]) { a, _ in a }]
        }
        // Two scrollers of eight cards 100 points high, 2 rounded and 3 square, each 120 points square.
        var ops: [[String: Any]] = [["op": "create", "id": 1, "kind": "view"]]
        for (id, x) in [(2, 0.0), (3, 150.0)] {
            ops.append(["op": "create", "id": id, "kind": "view"])
            for i in 0..<8 {
                ops.append(["op": "create", "id": id * 10 + i, "kind": "view", "style": ["scroll_snap_align": "start"]])
                ops.append(["op": "frame", "id": id * 10 + i, "x": 8.0, "y": 8.0 + Double(i) * 100, "w": 104.0, "h": 100.0])
            }
            ops += [["op": "children", "id": id, "ids": (0..<8).map { id * 10 + $0 }],
                    ["op": "frame", "id": id, "x": x, "y": 0.0, "w": 120.0, "h": 120.0], ["op": "content", "id": id, "w": 120.0, "h": 816.0]]
        }
        ops += [look(2, ["border_radius": 24.0, "scroll_snap_type": "y mandatory"]), look(3, ["scroll_snap_type": "y mandatory"]),
                ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]], ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 200.0]]
        p.apply(wireBatch(ops))
        p.viewport.layoutSubtreeIfNeeded()
        /// CGScrollPhase: 1 began, 2 changed, 4 ended; none is a mouse wheel's notch. A negative delta moves the
        /// content up, so the offset grows.
        func wheel(_ sv: ChainingScrollView, _ dy: Int32, phase: Int64, ms: Double) throws {
            let cg = try XCTUnwrap(CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2, wheel1: dy, wheel2: 0, wheel3: 0))
            cg.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
            if phase != 0 { cg.setIntegerValueField(.scrollWheelEventScrollPhase, value: phase) }
            cg.timestamp = CGEventTimestamp(ms * 1_000_000)
            sv.scrollWheel(with: try XCTUnwrap(NSEvent(cgEvent: cg)))
        }
        func spin(until done: () -> Bool) {
            let end = Date().addingTimeInterval(3)
            while !done(), Date() < end { RunLoop.main.run(until: Date().addingTimeInterval(0.02)) }
        }
        /// What one scroller did with a drag, a notch and a wheel over a smooth scroll.
        struct Drive: Equatable { var dragged: CGFloat, lifted: CGFloat, notched: CGFloat, said: [String], taken: Bool, wheels: Int }
        func drive(_ id: UInt32) throws -> Drive {
            let node = try XCTUnwrap(p.views[id]), sv = try XCTUnwrap(node.scroll), clip = sv.contentView
            XCTAssertTrue(sv.superview === node, "\(id): its node's own subview")
            node.handlers = ["scrolldragstart", "scrolldragend"]
            said = []
            let wheels = p.personScrolls
            // Four moves of 10 points 10 ms apart, lifted 10 ms on: a point a millisecond, which projects past four cards.
            try wheel(sv, 0, phase: 1, ms: 1000)
            for i in 1...4 { try wheel(sv, -10, phase: 2, ms: 1000 + Double(i) * 10) }
            let dragged = clip.bounds.minY
            try wheel(sv, 0, phase: 4, ms: 1050)
            XCTAssertTrue(sv.snapSettling, "\(id): the lift eases to a snap position")
            spin { !sv.snapSettling }
            let lifted = clip.bounds.minY
            // A notch moves a mandatory snap one position its way, at once under the agent's wheel.
            try wheel(sv, -30, phase: 0, ms: 2000)
            let notched = clip.bounds.minY
            // A smooth `scrollTop` owns the scroller until it lands, or until the person's wheel takes it.
            p.apply(wireBatch([look(Int(id), ["border_radius": id == 2 ? 24.0 : 0.0, "scroll_behavior": "smooth"])]))
            p.apply(wireBatch([["op": "props", "id": Int(id), "set": ["scrollTop": "0"]]]))
            XCTAssertNotNil(node.scrollEnd?.elementAnimation, "\(id): the write owns the scroller")
            try wheel(sv, -30, phase: 0, ms: 3000)
            return Drive(dragged: dragged, lifted: lifted, notched: notched, said: said.map { String($0.dropFirst("\(id) ".count)) },
                         taken: node.scrollEnd?.elementAnimation == nil, wheels: p.personScrolls - wheels)
        }
        XCTAssertNotNil(p.views[2]?.scroll?.layer?.mask, "the rounded one is masked at its padding edge")
        XCTAssertNil(p.views[3]?.scroll?.layer?.mask, "the square one is not")
        let rounded = try drive(2), square = try drive(3)
        XCTAssertEqual(rounded.dragged, 40, "the fingers moved it")
        XCTAssertEqual(rounded.said.first, "start", "the drag's first move said it began")
        XCTAssertEqual(rounded.said.count, 2, "\(rounded.said)")
        XCTAssertEqual(Double(rounded.said.last?.dropFirst("end ".count) ?? "") ?? 0, 1, accuracy: 1e-6, "and its lift its velocity, a point a millisecond")
        // A point a millisecond carries it 499 points on from 40: the lift rests on the card nearest 539.
        XCTAssertEqual(rounded.lifted, 539, accuracy: 50, "the lift snapped to the card its momentum projects to")
        XCTAssertNotEqual(rounded.lifted, rounded.dragged)
        XCTAssertEqual(rounded.notched - rounded.lifted, 100, accuracy: 0.01, "and a notch to the next card")
        XCTAssertTrue(rounded.taken, "a wheel takes the scroller from a smooth scroll")
        XCTAssertEqual(rounded.wheels, 8, "and each wheel event counts as the person's")
        XCTAssertEqual(rounded, square, "as a scroller with no clip of its own does")
    }
}
#endif
