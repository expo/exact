#if os(iOS)
import UIKit
import ImageIO
import CExact
import XCTest
@testable import ExactKit
@testable import ExactVideo
@testable import ExactEffects
@testable import ExactSvg
@testable import ExactImages

/// A box Core Animation can paint keeps no bitmap: background, one radius
/// over a mask of corners, and a uniform border are layer properties; a
/// node with no paint (a collection's spacer) has no contents at all. Only
/// what the layer cannot say still draws. UIKit, so a simulator runs it:
///   bun host/apple/build.mjs --test --ios
final class BoxLayerIOSTests: XCTestCase {
    func testAnUnevenImageBackgroundStaysBehindItsDrawnPixels() throws {
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let png = try XCTUnwrap(UIGraphicsImageRenderer(size: CGSize(width: 80, height: 40), format: format).image { context in
            UIColor.red.setFill(); context.fill(CGRect(x: 0, y: 0, width: 40, height: 40))
        }.pngData())
        let p = Presenter(), loader = RasterLoader(), resolver = AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()))
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 100, height: 100))
        defer { loader.shutdown(); window.isHidden = true }
        p.viewport.frame = window.bounds; window.addSubview(p.viewport); window.makeKeyAndVisible()
        let image = NodeView(id: 1, kind: "image", presenter: p)
        p.views[1] = image; p.viewport.addSubview(image)
        image.frame = CGRect(x: 0, y: 0, width: 80, height: 40)
        image.loadGeneration = 1
        loader.load(image, source: "data:image/png;base64," + png.base64EncodedString(), resolver: resolver)
        let deadline = Date(timeIntervalSinceNow: 5)
        while image.raster == nil, Date() < deadline { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.02)) }
        XCTAssertNotNil(image.raster)
        for alpha in [255, 128] {
            image.applyStyle(["background_color": [0, 0, 255, .number(Double(alpha))],
                "border_radius_top_left": 18, "border_radius_top_right": 18,
                "border_radius_bottom_left": 18, "border_radius_bottom_right": 4])
            image.layer.displayIfNeeded()
            let context = try XCTUnwrap(CGContext(data: nil, width: 80, height: 40, bitsPerComponent: 8, bytesPerRow: 320,
                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            image.layer.render(in: context)
            let bytes = try XCTUnwrap(context.data).assumingMemoryBound(to: UInt8.self)
            func pixel(_ x: Int, _ y: Int) -> [UInt8] { Array(UnsafeBufferPointer(start: bytes + y * 320 + x * 4, count: 4)) }
            XCTAssertEqual(pixel(20, 20), [255, 0, 0, 255], "the background must not cover or tint the opaque image")
            XCTAssertEqual(pixel(60, 20), [0, 0, UInt8(alpha), UInt8(alpha)], "the transparent image leaves the background visible")
            XCTAssertEqual(pixel(0, 0), [0, 0, 0, 0], "the corner remains transparent")
        }
    }

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

    /// The radius retains its viewport and resolves percentages per axis.
    func testACalculatedRadiusReadsTheKernelOnEachAxis() throws {
        let radius = try XCTUnwrap(CalculatedLength([
            "calc": .string("min(50%, 10vw)"),
            "env": [0, 0, 0, 0, 300, 600, 1, 1, 0, 0, 16]
        ]))
        var style: NodeStyle = ["background_color": [36, 104, 172, 255]]
        for corner in ["top_left", "top_right", "bottom_right", "bottom_left"] {
            style["border_radius_" + corner] = .calculation(radius)
        }
        let n = node(style, size: CGSize(width: 160, height: 40))
        XCTAssertEqual(n.cornerSizes(in: n.bounds), Array(repeating: CGSize(width: 30, height: 20), count: 4))
        XCTAssertNil(FlatPaint(style), "a calculated radius needs the final box")
        // Cancellation precedes the final conversion to layout precision.
        let steep = BatchValue.calculation(try XCTUnwrap(CalculatedLength([
            "calc": .string("calc(1e9 * (min(0.1%, 1px) - 0.2px) + 10px)"),
            "env": [0, 0, 0, 0, 300, 600, 1, 1, 0, 0, 16]
        ])))
        XCTAssertEqual(steep.length(basis: 200), 10)
    }

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
        let materialMask = try XCTUnwrap(material.layer.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(materialMask.path).contains(CGPoint(x: 20, y: 5)))
        XCTAssertTrue(try XCTUnwrap(materialMask.path).contains(CGPoint(x: 80, y: 5)))
        let clip = try XCTUnwrap(n.layer.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(clip.path).contains(CGPoint(x: 20, y: 5)))
        n.frame.size = CGSize(width: 80, height: 160)
        n.setNeedsLayout(); n.layoutIfNeeded()
        let resized = try XCTUnwrap(material.layer.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(resized.path).contains(CGPoint(x: 5, y: 20)))
        XCTAssertTrue(try XCTUnwrap(resized.path).contains(CGPoint(x: 5, y: 80)))
        n.applyStyle([:])
        n.updateMaterial()
        n.applyBoxLayer()
        XCTAssertNil(material.layer.mask, "a later square style removes the elliptical clip")
        XCTAssertNil(n.layer.mask)
    }

    /// Corners that differ (a chat bubble's 4 where it joins its run, 18
    /// elsewhere) clip its children through their outline: one Core
    /// Animation radius cannot say them, and the children overflowed square.
    func testOverflowWithCornersThatDifferClipsThroughTheirOutline() throws {
        var style: NodeStyle = ["overflow_x": "hidden", "overflow_y": "hidden", "background_color": [233, 233, 233, 255]]
        style["border_radius_top_left"] = 4.0
        for corner in ["top_right", "bottom_right", "bottom_left"] { style["border_radius_" + corner] = 18.0 }
        let n = node(style, size: CGSize(width: 264, height: 200))
        n.applyBoxLayer()
        n.applyBoxMask()
        let clip = try XCTUnwrap((n.clipBox?.layer.mask ?? n.layer.mask) as? CAShapeLayer, "the children are clipped by a shaped mask")
        let path = try XCTUnwrap(clip.path)
        XCTAssertFalse(path.contains(CGPoint(x: 1, y: 199)), "the 18-point bottom-left corner is cut")
        XCTAssertTrue(path.contains(CGPoint(x: 1.5, y: 1.5)), "the 4-point corner keeps nearly all")
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
        XCTAssertEqual(media.frame, CGRect(x: 10, y: 10, width: 140, height: 60))
        let mask = try XCTUnwrap(media.layer.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(mask.path).contains(CGPoint(x: 10, y: 5)))
        XCTAssertTrue(try XCTUnwrap(mask.path).contains(CGPoint(x: 70, y: 5)))
        n.frame.size = CGSize(width: 80, height: 160)
        n.applyContentBox(padding: [10, 10, 10, 10])
        VideoView.layout(media, in: n)
        XCTAssertEqual(media.frame, CGRect(x: 10, y: 10, width: 60, height: 140))
        let resized = try XCTUnwrap(media.layer.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(resized.path).contains(CGPoint(x: 5, y: 10)))
        n.applyStyle([:])
        VideoView.layout(media, in: n)
        XCTAssertNil(media.layer.mask)
    }

    func testPercentageOverflowClipComposesWithAuthoredClipPath() throws {
        var style: NodeStyle = ["overflow_x": "hidden", "overflow_y": "hidden",
            "clip_path": ["commands": [["M", [0, 0]], ["L", [80, 0]], ["L", [80, 80]], ["L", [0, 80]], ["Z", []]]]]
        for corner in ["top_left", "top_right", "bottom_right", "bottom_left"] {
            style["border_radius_" + corner] = ["pct": 50]
        }
        let n = node(style, size: CGSize(width: 160, height: 80))
        n.applyBoxLayer()
        let mask = try XCTUnwrap(n.layer.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(mask.path).contains(CGPoint(x: 20, y: 5)))
        let authored = try XCTUnwrap(mask.mask as? CAShapeLayer)
        XCTAssertTrue(try XCTUnwrap(authored.path).contains(CGPoint(x: 60, y: 40)))
        XCTAssertFalse(try XCTUnwrap(authored.path).contains(CGPoint(x: 100, y: 40)))
        let layer = n.layer
        func renderedAlpha(_ target: CALayer, _ x: Int, _ y: Int) -> UInt8 {
            let width = Int(target.bounds.width), height = Int(target.bounds.height)
            let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
            target.render(in: context)
            // Raw CGContext bitmap rows run bottom-up, independently of the
            // UIKit layer's isGeometryFlipped flag. Check an unmasked reference.
            return context.data!.assumingMemoryBound(to: UInt8.self)[((height - 1 - y) * width + x) * 4 + 3]
        }
        let reference = CALayer()
        reference.frame = CGRect(x: 0, y: 0, width: 160, height: 80)
        let top = CALayer()
        top.frame = CGRect(x: 0, y: 0, width: 160, height: 20)
        top.backgroundColor = CGColor(gray: 1, alpha: 1)
        reference.addSublayer(top)
        XCTAssertEqual(renderedAlpha(reference, 40, 10), 255)
        XCTAssertEqual(renderedAlpha(reference, 40, 60), 0)
        func alpha(_ x: Int, _ y: Int) -> UInt8 {
            // An opaque descendant proves both masks clip real rendered content.
            let child = CALayer()
            child.frame = n.bounds
            child.backgroundColor = CGColor(gray: 1, alpha: 1)
            layer.addSublayer(child)
            defer { child.removeFromSuperlayer() }
            return renderedAlpha(layer, x, y)
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
        let mask = try XCTUnwrap(media.layer.mask as? CAShapeLayer)
        XCTAssertFalse(try XCTUnwrap(mask.path).contains(CGPoint(x: 10, y: 5)))
        n.applyStyle([:])
        n.applyFieldContent(["rect": NSNull()])
        XCTAssertEqual(media.frame, n.bounds)
        XCTAssertNil(media.layer.mask)
        n.applyStyle(style)
        XCTAssertNotNil(media.layer.mask)
    }

    private let white: BatchValue = [255, 255, 255, 255]
    private let blue: BatchValue = [0, 136, 255, 255]

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

    /// A layout transition's size reaches a material as well as the painted
    /// surface: the material is a view the node holds, so without it a glass
    /// box jumps to its laid-out size while its border animates. It takes the
    /// laid-out bounds back when the transition ends.
    func testLayoutResizeSizesTheMaterialAndGivesItBack() throws {
        for kind in ["glass", "ultra-thin"] {
            let n = node([:], size: CGSize(width: 300, height: 48))
            n.props["backgroundMaterial"] = kind
            n.updateMaterial()
            n.applyBoxLayer()
            let material = try XCTUnwrap(n.glassSlot ?? n.materialView, kind)
            XCTAssertEqual(material.frame, n.bounds, kind)
            n.layoutResize = CGSize(width: 120, height: -8)
            n.applySurface()
            XCTAssertEqual(material.frame, CGRect(x: 0, y: 0, width: 420, height: 40), "\(kind) takes the shown size")
            n.layoutResize = CGSize(width: 60, height: 0)
            n.applySurface()
            XCTAssertEqual(material.frame, CGRect(x: 0, y: 0, width: 360, height: 48), "\(kind) follows each frame")
            n.layoutResize = .zero
            n.applySurface()
            XCTAssertNil(n.surface, kind)
            XCTAssertEqual(material.frame, n.bounds, "\(kind) gives the size back")
        }
    }

    /// A material's corners resolve at the shown size: a `50%` radius on a
    /// box that is not square is an elliptical mask, which follows the box a
    /// layout transition resizes and returns to `bounds` when it ends.
    func testLayoutResizeReshapesTheMaterialsCorners() throws {
        var style: NodeStyle = [:]
        for corner in ["top_left", "top_right", "bottom_right", "bottom_left"] { style["border_radius_" + corner] = ["pct": 50] }
        for kind in ["glass", "ultra-thin"] {
            let n = node(style, size: CGSize(width: 300, height: 48))
            n.props["backgroundMaterial"] = kind
            n.updateMaterial()
            let material = try XCTUnwrap(n.materialView, kind)
            let outline = { (material.layer.mask as? CAShapeLayer)?.path?.boundingBoxOfPath }
            XCTAssertEqual(outline(), n.bounds, kind)
            n.layoutResize = CGSize(width: 120, height: -8)
            n.applySurface()
            XCTAssertEqual(outline(), CGRect(x: 0, y: 0, width: 420, height: 40), "\(kind) is clipped to the shown box")
            n.layoutResize = .zero
            n.applySurface()
            XCTAssertEqual(outline(), n.bounds, "\(kind) is clipped to its bounds again")
        }
    }

    private func node(_ style: NodeStyle, size: CGSize = CGSize(width: 300, height: 120)) -> NodeView {
        let p = Presenter()
        let n = NodeView(id: 1, kind: "view", presenter: p)
        p.views[n.id] = n
        n.frame = CGRect(origin: .zero, size: size)
        n.applyStyle(style)
        n.layer.displayIfNeeded()
        return n
    }

    /// CSS `filter` (LLP 1055.000 D14) arriving in a later style, and
    /// leaving in one after: the box is masked while filtered and its
    /// picture is drawn with each batch; a node that never had one is
    /// neither.
    func testAFilterGainedAfterCreationIsAppliedAndOneLostIsRemoved() throws {
        let sub: [Double] = [-10, -10, 20, 20]
        // One Gaussian blur over the box's own picture (`Filter::encode`).
        let program: [Double] = sub + [1] + [0, -1, -3] + sub + [0, 2, 2]
        guard SvgFilterGPU.runs(program.map(Float.init)) else { throw missing("metal", "no GPU filter path on this host") }
        let filter: BatchValue = .object(["p": .array(program.map { .number($0) })])
        let plain: NodeStyle = ["background_color": white]
        let p = Presenter()
        let n = NodeView(id: 1, kind: "view", presenter: p)
        p.views[n.id] = n
        n.frame = CGRect(x: 0, y: 0, width: 300, height: 120)
        p.viewport.addSubview(n)
        n.applyStyle(plain)
        XCTAssertTrue(p.boxFilters!.isEmpty)
        XCTAssertNil(n.layer.mask)
        var filtered = plain; filtered["filter"] = filter
        n.applyStyle(filtered)
        XCTAssertFalse(p.boxFilters!.isEmpty, "the style that brings a filter makes the box's picture")
        XCTAssertNotNil(n.layer.mask, "the box itself is hidden behind its picture")
        n.applyStyle(plain)
        XCTAssertTrue(p.boxFilters!.isEmpty)
        XCTAssertNil(n.layer.mask)
    }

    func testAnUnpaintedNodeHasNoBitmapHoweverTall() {
        let spacer = node([:], size: CGSize(width: 402, height: 865_678))
        XCTAssertNil(spacer.layer.contents)
        XCTAssertNil(spacer.layer.backgroundColor)
    }

    func testBackgroundAndMaskedRadiusAreLayerProperties() {
        let card = node(["background_color": white, "border_radius_top_left": 24, "border_radius_top_right": 24])
        XCTAssertNil(card.layer.contents)
        XCTAssertEqual(card.layer.backgroundColor?.alpha, 1)
        XCTAssertEqual(card.layer.cornerRadius, 24)
        XCTAssertEqual(card.layer.maskedCorners, [.layerMinXMinYCorner, .layerMaxXMinYCorner])
        XCTAssertFalse(card.layer.masksToBounds, "rounding clips nothing while the overflow is visible")
    }

    func testAUniformBorderStaysUnderTheChildrenUnlessTheyAreClipped() {
        let style: NodeStyle = ["border_width": 2, "border_color_top": blue, "border_radius_top_left": 11,
            "border_radius_top_right": 11, "border_radius_bottom_right": 11, "border_radius_bottom_left": 11]
        let open = node(style, size: CGSize(width: 22, height: 22))
        XCTAssertNil(open.layer.contents)
        XCTAssertEqual(open.layer.borderWidth, 0)
        let under = open.layer.sublayers?.first
        XCTAssertEqual(under?.borderWidth, 2)
        XCTAssertEqual(under?.cornerRadius, 11)
        var clipped = style; clipped["overflow_x"] = "hidden"; clipped["overflow_y"] = "hidden"
        let closed = node(clipped, size: CGSize(width: 22, height: 22))
        XCTAssertNil(closed.layer.contents)
        XCTAssertEqual(closed.layer.borderWidth, 2, "a clipping box's border covers what it clips")
        XCTAssertTrue(closed.layer.masksToBounds)
        XCTAssertNil(closed.boxBorder)
    }

    /// A row's separator (`border-bottom`) is a rectangle on a shape layer
    /// under the children: no backing store of the row's size for a hairline.
    func testSidesInOneColourAreAShapeLayer() throws {
        let row = node(["background_color": white, "border_width_bottom": 0.5, "border_width_left": 2, "border_color_top": blue])
        XCTAssertNil(row.layer.contents, "sides in one colour keep no bitmap")
        XCTAssertEqual(row.layer.backgroundColor?.alpha, 1)
        let edges = try XCTUnwrap(row.boxBorder as? CAShapeLayer)
        XCTAssertTrue(row.layer.sublayers?.first === edges, "under the children")
        XCTAssertEqual(edges.fillColor?.components, row.color("border_color_top", .clear).cgColor.components)
        let path = try XCTUnwrap(edges.path)
        XCTAssertTrue(path.contains(CGPoint(x: 150, y: 119.75)), "the bottom side")
        XCTAssertTrue(path.contains(CGPoint(x: 1, y: 60)), "the left side")
        XCTAssertFalse(path.contains(CGPoint(x: 150, y: 60)), "nothing inside")
        XCTAssertFalse(path.contains(CGPoint(x: 299, y: 60)), "no right side")
    }

    func testWhatTheLayerCannotSayStillDraws() {
        let sides = node(["background_color": white, "border_width_bottom": 1, "border_width_top": 1,
            "border_color_top": blue, "border_color_bottom": white])
        XCTAssertNotNil(sides.layer.contents, "sides in two colours draw")
        XCTAssertNil(sides.layer.backgroundColor, "and so does its background, once")
        let corners = node(["background_color": white, "border_radius_top_left": 8, "border_radius_bottom_right": 20])
        XCTAssertNil(corners.layer.contents, "unequal radii use a shape mask")
        XCTAssertEqual(corners.layer.cornerRadius, 0)
    }

    /// An image's decoded pixels are a sublayer's contents: the visible part
    /// of the fitted image (`contentsRect`), carrying the radius, with no
    /// bitmap painted for the view. An image whose clip the sublayer cannot
    /// say (padding under a radius) draws as before.
    func testAnImagesPixelsAreASublayersContents() throws {
        let root = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("exact-image-layer-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        for (name, width, height) in [("wide.png", 200, 100), ("skewed.png", 4000, 1001)] {
            let context = try XCTUnwrap(CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            context.setFillColor(red: 1, green: 0, blue: 0, alpha: 1); context.fill(CGRect(x: 0, y: 0, width: width, height: height))
            let destination = try XCTUnwrap(CGImageDestinationCreateWithURL(root.appendingPathComponent(name) as CFURL, "public.png" as CFString, 1, nil))
            CGImageDestinationAddImage(destination, try XCTUnwrap(context.makeImage()), nil)
            XCTAssertTrue(CGImageDestinationFinalize(destination))
        }

        let p = Presenter(), loader = RasterLoader(), resolver = AssetResolver(root: root)
        defer { loader.shutdown(); withExtendedLifetime(resolver) {} }
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 300, height: 300))
        p.viewport.frame = window.bounds; window.addSubview(p.viewport); window.makeKeyAndVisible()
        func image(_ id: UInt32, _ style: NodeStyle, _ source: String = "wide.png") -> NodeView {
            let n = NodeView(id: id, kind: "image", presenter: p)
            p.views[id] = n; p.viewport.addSubview(n)
            n.frame = CGRect(x: 0, y: 0, width: 40, height: 40)
            n.applyStyle(style)
            n.loadGeneration = 1
            loader.load(n, source: source, resolver: resolver)
            return n
        }
        let avatar = image(1, ["object_fit": .string("cover"), "border_radius_top_left": 20, "border_radius_top_right": 20,
            "border_radius_bottom_right": 20, "border_radius_bottom_left": 20])
        let padded = image(2, ["object_fit": .string("cover"), "padding_left": 4, "border_radius_top_left": 20,
            "border_radius_top_right": 20, "border_radius_bottom_right": 20, "border_radius_bottom_left": 20])
        padded.applyContentBox(padding: [0, 0, 0, 4])
        let skewed = image(3, ["object_fit": .string("cover")], "skewed.png")
        let letterboxed = image(4, ["object_fit": .string("contain"), "border_radius_top_left": 8, "border_radius_top_right": 8,
            "border_radius_bottom_right": 8, "border_radius_bottom_left": 8])
        let end = Date(timeIntervalSinceNow: 5)
        while (avatar.raster == nil || padded.raster == nil || skewed.raster == nil || letterboxed.raster == nil) && Date() < end { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01)) }
        avatar.layer.displayIfNeeded(); padded.layer.displayIfNeeded(); skewed.layer.displayIfNeeded(); letterboxed.layer.displayIfNeeded()

        let sub = try XCTUnwrap(avatar.imageLayer)
        let whole = try XCTUnwrap(sub.sublayers?.first)
        XCTAssertTrue((whole.contents as AnyObject?) === avatar.raster?.image.image)
        XCTAssertNil(avatar.layer.contents, "no bitmap of the view's size")
        XCTAssertEqual(sub.frame, avatar.bounds)
        XCTAssertEqual(whole.frame, CGRect(x: -20, y: 0, width: 80, height: 40))
        XCTAssertEqual(whole.contentsGravity, .resize)
        XCTAssertEqual(sub.cornerRadius, 20); XCTAssertTrue(sub.masksToBounds)

        XCTAssertNil(padded.imageLayer)
        XCTAssertNotNil(padded.layer.contents, "a radius over a padded content box draws")

        // A rounded `contain` image that leaves part of its box empty is
        // clipped by the box's corners, not drawn.
        let boxed = try XCTUnwrap(letterboxed.imageLayer)
        XCTAssertNil(letterboxed.layer.contents)
        XCTAssertEqual(boxed.frame, letterboxed.bounds)
        XCTAssertEqual(boxed.sublayers?.first?.frame, CGRect(x: 0, y: 10, width: 40, height: 20))
        XCTAssertEqual(boxed.cornerRadius, 8); XCTAssertTrue(boxed.masksToBounds)

        // The decoded aspect differs; the fitted rectangle retains the natural one.
        let reduced = try XCTUnwrap(skewed.raster?.image.image)
        XCTAssertLessThan(reduced.width, 4000, "decoded at the box's size")
        XCTAssertNotEqual(Double(reduced.width) / Double(reduced.height), 4000.0 / 1001, accuracy: 0.01)
        let skewedSub = try XCTUnwrap(skewed.imageLayer)
        let skewedWhole = try XCTUnwrap(skewedSub.sublayers?.first)
        XCTAssertEqual(skewedWhole.contentsGravity, .resize)
        let fitted = 40 * 4000.0 / 1001
        XCTAssertEqual(skewedWhole.bounds.width, fitted, accuracy: 0.0001)
        XCTAssertEqual(skewedWhole.frame.midX, 20, accuracy: 0.0001)

        window.layoutIfNeeded(); CATransaction.flush()
        // UIKit's animated bounds and the pixels must cover the same box throughout a resize.
        UIView.animate(withDuration: 0.8) {
            avatar.frame.size = CGSize(width: 160, height: 80)
            avatar.applyImageLayer()
        }
        let viewBounds = try XCTUnwrap((avatar.layer.animation(forKey: "bounds") ?? avatar.layer.animation(forKey: "bounds.size")) as? CABasicAnimation)
        let pixelsBounds = try XCTUnwrap((sub.animation(forKey: "bounds") ?? sub.animation(forKey: "bounds.size")) as? CABasicAnimation)
        XCTAssertEqual(pixelsBounds.duration, viewBounds.duration, accuracy: 0.001)
        XCTAssertEqual(pixelsBounds.keyPath, viewBounds.keyPath)
        XCTAssertEqual(pixelsBounds.fromValue as? NSValue, viewBounds.fromValue as? NSValue)
        XCTAssertEqual(sub.bounds.size, avatar.bounds.size)
        XCTAssertEqual(sub.animation(forKey: "position")?.duration ?? 0, viewBounds.duration, accuracy: 0.001, "its centre moves with the box")
        XCTAssertEqual(whole.bounds.width / whole.bounds.height, 2, accuracy: 0.0001)
        XCTAssertFalse(sub.animationKeys()?.contains { $0.hasPrefix("contentsRect") } ?? false)
        UIView.animate(withDuration: 0.8) {
            skewed.frame.size = CGSize(width: 160, height: 80)
            skewed.applyImageLayer()
        }
        func halfwayBounds(_ layer: CALayer) throws -> CGSize {
            let animation = try XCTUnwrap((layer.animation(forKey: "bounds") ?? layer.animation(forKey: "bounds.size")) as? CABasicAnimation)
            let from = try XCTUnwrap(animation.fromValue as? NSValue)
            let size = animation.keyPath == "bounds.size" ? from.cgSizeValue : from.cgRectValue.size
            return CGSize(width: (size.width + layer.bounds.width) / 2, height: (size.height + layer.bounds.height) / 2)
        }
        let fittedHalfway: CGSize
        if let whole = skewedSub.sublayers?.first, whole.contents != nil {
            fittedHalfway = try halfwayBounds(whole)
        } else {
            let visible = try halfwayBounds(skewedSub), crop = skewedSub.contentsRect
            fittedHalfway = CGSize(width: visible.width / crop.width, height: visible.height / crop.height)
        }
        XCTAssertEqual(fittedHalfway.width / fittedHalfway.height, 4000.0 / 1001, accuracy: 0.0001,
                       "a decoded cover keeps its natural aspect halfway through UIKit's resize")
        avatar.layer.removeAllAnimations(); sub.removeAllAnimations()
        UIView.performWithoutAnimation {
            avatar.frame.size = CGSize(width: 80, height: 80)
            avatar.applyImageLayer()
        }
        XCTAssertNil(sub.animationKeys(), "an ordinary layout must not introduce an implicit animation")

        avatar.raster = nil
        XCTAssertNil(avatar.imageLayer, "no pixels outlive the lease")
        XCTAssertNil(sub.superlayer)
        padded.raster = nil; skewed.raster = nil; letterboxed.raster = nil
    }

    /// A `-exact-tint-color` draws the bitmap as a template from the same decoded
    /// pixels, through `draw(_:)` (a canvas capture drops a mask layer): the
    /// opaque half takes the tint, the transparent half stays clear, and a
    /// `light-dark()` tint follows a live appearance change. Without the row
    /// the pixels are the sublayer's contents again.
    /// A bordered rounded image's pixels end at the border's inner curve
    /// (CSS Backgrounds 3 §5.3), as `BoxLayerMacTests` checks on AppKit.
    func testABorderedRoundedImageIsClippedToTheBordersInnerCurve() throws {
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let png = try XCTUnwrap(UIGraphicsImageRenderer(size: CGSize(width: 100, height: 100), format: format).image { context in
            UIColor.red.setFill(); context.fill(CGRect(x: 0, y: 0, width: 100, height: 100))
        }.pngData())
        let p = Presenter(), loader = RasterLoader(), resolver = AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()))
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 200, height: 200))
        defer { loader.shutdown(); window.isHidden = true }
        p.viewport.frame = window.bounds; window.addSubview(p.viewport); window.makeKeyAndVisible()
        let image = NodeView(id: 1, kind: "image", presenter: p)
        defer { image.raster = nil }
        p.views[1] = image; p.viewport.addSubview(image)
        image.frame = CGRect(x: 0, y: 0, width: 100, height: 100)
        image.applyStyle(["border_width": 10, "border_radius_top_left": 30, "border_radius_top_right": 30,
            "border_radius_bottom_left": 30, "border_radius_bottom_right": 30])
        image.loadGeneration = 1
        loader.load(image, source: "data:image/png;base64," + png.base64EncodedString(), resolver: resolver)
        let deadline = Date(timeIntervalSinceNow: 5)
        while image.raster == nil, Date() < deadline { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.02)) }
        XCTAssertNotNil(image.raster)
        image.applyFieldContent(["rect": [10.0, 10.0, 80.0, 80.0]])
        image.layer.displayIfNeeded()
        let context = try XCTUnwrap(CGContext(data: nil, width: 100, height: 100, bitsPerComponent: 8, bytesPerRow: 400,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        image.layer.render(in: context)
        let bytes = try XCTUnwrap(context.data).assumingMemoryBound(to: UInt8.self)
        func alpha(_ x: Int, _ y: Int) -> UInt8 { bytes[y * 400 + x * 4 + 3] }
        for (x, y) in [(12, 12), (88, 12), (12, 88), (88, 88)] {
            XCTAssertEqual(alpha(x, y), 0, "(\(x), \(y)) is inside the border box's curve but outside the content edge's")
        }
        for (x, y) in [(20, 20), (80, 80), (50, 11)] {
            XCTAssertEqual(alpha(x, y), 255, "(\(x), \(y)) is inside the content edge's curve")
        }
    }

    func testATintedImageIsItsAlphaInTheTint() throws {
        let root = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("exact-image-tint-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let context = try XCTUnwrap(CGContext(data: nil, width: 200, height: 100, bitsPerComponent: 8, bytesPerRow: 800,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.setFillColor(red: 0, green: 0, blue: 0, alpha: 1); context.fill(CGRect(x: 0, y: 0, width: 100, height: 100))
        let destination = try XCTUnwrap(CGImageDestinationCreateWithURL(root.appendingPathComponent("mark.png") as CFURL, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, try XCTUnwrap(context.makeImage()), nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))

        let p = Presenter(), loader = RasterLoader(), resolver = AssetResolver(root: root)
        defer { loader.shutdown(); withExtendedLifetime(resolver) {} }
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 300, height: 300))
        p.viewport.frame = window.bounds; window.addSubview(p.viewport); window.makeKeyAndVisible()
        let tint: BatchValue = [[0, 0, 0, 255], [255, 255, 255, 255]]
        let mark = NodeView(id: 1, kind: "image", presenter: p)
        p.views[1] = mark; p.viewport.addSubview(mark)
        mark.frame = CGRect(x: 0, y: 0, width: 80, height: 40)
        mark.applyStyle(["object_fit": .string("contain"), "tint_color": tint])
        mark.loadGeneration = 1
        loader.load(mark, source: "mark.png", resolver: resolver)
        let end = Date(timeIntervalSinceNow: 5)
        while mark.raster == nil && Date() < end { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01)) }
        window.overrideUserInterfaceStyle = .light
        window.layoutIfNeeded(); mark.layer.displayIfNeeded()

        // The view's paint inside the opaque left half and the transparent right half.
        func samples() throws -> [UInt8] {
            let ctx = try XCTUnwrap(CGContext(data: nil, width: 80, height: 40, bitsPerComponent: 8, bytesPerRow: 320,
                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            mark.layer.render(in: ctx)
            let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
            return [20, 60].flatMap { x in (0..<4).map { bytes[20 * 320 + x * 4 + $0] } }
        }
        XCTAssertNil(mark.imageLayer, "the template is drawn")
        XCTAssertTrue(mark.drawsPaint)
        XCTAssertEqual(try samples(), [0, 0, 0, 255, 0, 0, 0, 0])

        window.overrideUserInterfaceStyle = .dark
        window.layoutIfNeeded(); mark.layer.displayIfNeeded()
        XCTAssertEqual(try samples(), [255, 255, 255, 255, 0, 0, 0, 0], "light-dark() follows the appearance")

        mark.applyStyle(["object_fit": .string("contain")])
        mark.layer.displayIfNeeded()
        let sub = try XCTUnwrap(mark.imageLayer)
        XCTAssertTrue((sub.sublayers?.first?.contents as AnyObject?) === mark.raster?.image.image)
        mark.raster = nil
    }

    /// A scroller's children are its scroll view's, so its radius reaches them as the scroll view's corner
    /// (expo/exact#1442): one radius over the corners that have one, on Apple's continuous curve where the style
    /// asks, following the style, the box's size and a reach past the box. Under a border the clip is the padding
    /// edge's, a mask that stays over the box as the scroll view scrolls and comes back after a layout transition.
    /// A zoomed scroller keeps its zoom. Corners that differ leave the scroll view square, and so does a box whose
    /// own layer clips.
    func testAScrollersRadiusClipsItsContent() throws {
        let p = Presenter(), window = UIWindow(frame: CGRect(x: 0, y: 0, width: 200, height: 200))
        defer { window.isHidden = true }
        p.viewport.frame = window.bounds; window.addSubview(p.viewport); window.makeKeyAndVisible()
        let all: CACornerMask = [.layerMinXMinYCorner, .layerMaxXMinYCorner, .layerMaxXMaxYCorner, .layerMinXMaxYCorner]
        let red: [UInt8] = [255, 0, 0, 255], blue: [UInt8] = [0, 0, 255, 255], clear: [UInt8] = [0, 0, 0, 0]
        func look(_ rows: [String: Any], x: String = "auto") -> [String: Any] {
            ["op": "style", "id": 2, "style": rows.merging(["overflow_x": x, "overflow_y": "auto", "min_zoom": 1, "max_zoom": 4,
                                                             "background_color": [0, 0, 255, 255]]) { a, _ in a }]
        }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"], ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            look(["border_radius": 24.0]),
            ["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 200], ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 120, "h": 120],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 120, "h": 300], ["op": "content", "id": 2, "w": 120, "h": 300],
            ["op": "props", "id": 2, "set": ["scrollZoom": "2"]],
        ]))
        let node = try XCTUnwrap(p.views[2]), child = try XCTUnwrap(p.views[3]), scroll = try XCTUnwrap(node.scroll)
        /// The scroller as its layers draw it, 120 points square at 1x: the pixel at a point. The child covers the
        /// whole box, over its blue ground.
        func shot() throws -> (Int, Int) -> [UInt8] {
            window.layoutIfNeeded()
            node.layer.displayIfNeeded(); child.layer.displayIfNeeded()
            let context = try XCTUnwrap(CGContext(data: nil, width: 120, height: 120, bitsPerComponent: 8, bytesPerRow: 480,
                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            // A bitmap's rows run top down and its context's y bottom up: the layer is drawn top down into it.
            context.translateBy(x: 0, y: 120); context.scaleBy(x: 1, y: -1)
            node.layer.render(in: context)
            let bytes = Array(UnsafeBufferPointer(start: try XCTUnwrap(context.data).assumingMemoryBound(to: UInt8.self), count: 120 * 480))
            return { x, y in Array(bytes[(y * 480 + x * 4)..<(y * 480 + x * 4 + 4)]) }
        }
        var at = try shot()
        XCTAssertFalse(node.clipsToBounds, "a box that scrolls both axes leaves the clip to its scroll view")
        XCTAssertEqual(scroll.layer.cornerRadius, 24)
        XCTAssertEqual(scroll.layer.maskedCorners, all)
        XCTAssertEqual(scroll.layer.cornerCurve, .circular)
        XCTAssertTrue(scroll.clipsToBounds)
        XCTAssertNil(scroll.layer.mask, "with no border the layer's radius is the padding edge")
        XCTAssertEqual(at(60, 60), red, "the child fills the scroller")
        XCTAssertEqual(at(3, 3), clear, "and stops at the curve")
        XCTAssertEqual(scroll.zoomScale, 2)

        // An 8 point border: the padding edge's corner has radius 16 round the box's centre, (24, 24). (10, 10) is
        // 19.8 from it, inside the box's curve and outside the padding edge's; (14, 14) is 14.1 from it, inside both.
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 8.0, "border_color_top": [0, 255, 0, 128]])]))
        /// Half green over the blue ground, with none of the child's red.
        func borderOverGround(_ ink: [UInt8], _ message: String) {
            XCTAssertEqual(ink[0], 0, message)
            XCTAssertEqual(Double(ink[1]), 128, accuracy: 2, message)
            XCTAssertEqual(Double(ink[2]), 127, accuracy: 2, message)
        }
        for offset: CGFloat in [0, 40] {
            scroll.contentOffset.y = offset
            at = try shot()
            XCTAssertEqual(scroll.contentOffset.y, offset)
            XCTAssertEqual(scroll.layer.mask?.position, scroll.bounds.origin, "the mask is over the box, scrolled by \(offset)")
            XCTAssertNil(scroll.layer.mask?.animationKeys(), "and is there at once")
            XCTAssertEqual(scroll.layer.cornerRadius, 0, "no radius of a layer the size of the border box is the padding edge")
            XCTAssertEqual(at(60, 60), red)
            XCTAssertEqual(at(14, 14), red, "the child reaches the padding edge's curve, scrolled by \(offset)")
            borderOverGround(at(10, 10), "and stops there, so a translucent border shows the ground in its corner, scrolled by \(offset)")
            borderOverGround(at(60, 4), "and along its side, scrolled by \(offset)")
            XCTAssertEqual(at(3, 3), clear)
        }
        scroll.contentOffset.y = 0
        let edge = try XCTUnwrap(scroll.layer.mask)
        node.layoutResize = CGSize(width: 0, height: 20); node.applyTransform(); node.applySurface()
        XCTAssertFalse(scroll.layer.mask === edge, "a layout transition's surface masks the scroll view at its own size")
        node.layoutResize = .zero; node.applyTransform(); node.applySurface()
        XCTAssertTrue(scroll.layer.mask === edge, "and the padding edge is back when it ends")
        borderOverGround(try shot()(10, 10), "after a layout transition")

        // Borders of two widths make the padding edge's corner an ellipse, 8 by 20 round (24, 24).
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 4.0, "border_width_left": 16.0, "border_color_top": [0, 0, 0, 0]])]))
        at = try shot()
        XCTAssertEqual(at(18, 6), blue, "outside the ellipse, inside the padding box and the box's curve")
        XCTAssertEqual(at(22, 12), red, "inside it")
        // The top and bottom borders differ: the edge is the node's, top down.
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 2.0, "border_width_top": 20.0, "border_color_top": [0, 0, 0, 0]])]))
        at = try shot()
        XCTAssertEqual(at(60, 12), blue, "under the top border")
        XCTAssertEqual(at(60, 24), red)
        XCTAssertEqual(at(60, 108), red, "to the bottom border, which is thin")
        XCTAssertEqual(at(60, 119), blue)
        // A border wider than the radius leaves the padding edge square.
        p.apply(wireBatch([look(["border_radius": 6.0, "border_width": 8.0, "border_color_top": [0, 0, 0, 0]])]))
        at = try shot()
        XCTAssertEqual(at(9, 9), red)
        XCTAssertEqual(at(6, 6), blue)

        p.apply(wireBatch([look(["border_radius": 8.0, "corner_shape": ["apple", "apple", "apple", "apple"]])]))
        _ = try shot()
        XCTAssertNil(scroll.layer.mask, "the border gone, the layer's radius again")
        XCTAssertEqual(scroll.layer.cornerRadius, 8, "a new radius")
        XCTAssertEqual(scroll.layer.cornerCurve, .continuous)
        XCTAssertEqual(scroll.zoomScale, 2, "the zoom is kept")

        p.apply(wireBatch([look(["border_radius_top_left": 24.0, "border_radius_top_right": 24.0])]))
        _ = try shot()
        XCTAssertEqual(scroll.layer.cornerRadius, 24)
        XCTAssertEqual(scroll.layer.maskedCorners, [.layerMinXMinYCorner, .layerMaxXMinYCorner], "only the corners that have it")
        XCTAssertEqual(scroll.layer.cornerCurve, .circular)

        p.apply(wireBatch([look(["border_radius": 24.0]), ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 30, "h": 120]]))
        _ = try shot()
        XCTAssertEqual(scroll.layer.cornerRadius, 15, "reduced with the box")
        XCTAssertEqual(scroll.layer.maskedCorners, all)
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 0, "y": 0, "w": 120, "h": 120]]))
        _ = try shot()
        XCTAssertEqual(scroll.layer.cornerRadius, 24)

        node.setReach(UIEdgeInsets(top: 20, left: 0, bottom: 0, right: 0))
        XCTAssertEqual(scroll.layer.cornerRadius, 0, "a scroll view drawn past the box has its corners elsewhere")
        node.setReach(.zero)
        XCTAssertEqual(scroll.layer.cornerRadius, 24)

        p.apply(wireBatch([look(["border_radius_top_left": 18.0, "border_radius_top_right": 4.0])]))
        _ = try shot()
        XCTAssertEqual(scroll.layer.cornerRadius, 0, "corners that differ need a path")
        p.apply(wireBatch([look(["border_radius": 24.0], x: "hidden")]))
        _ = try shot()
        XCTAssertTrue(node.clipsToBounds, "one axis hidden: the box's own layer clips")
        XCTAssertEqual(node.layer.cornerRadius, 24)
        XCTAssertEqual(try XCTUnwrap(node.scroll).layer.cornerRadius, 0, "and its scroll view needs no corner")
    }

    /// The padding edge's mask cuts UIKit's scroll indicators too, which lie at the scroll view's edge, over the
    /// border (expo/exact#1442). They keep inside the edge by the border's widths, on both axes, over whatever
    /// insets they had, and go back when the border goes; D14's writer, which writes the insets whole, adds the
    /// widths. So under a transparent border each indicator's ink shows, inside the padding edge.
    func testAScrollersIndicatorsKeepInsideItsPaddingEdge() throws {
        // Below the screen's top band, whose depth UIKit would add to the vertical indicator's insets.
        let p = Presenter(), window = UIWindow(frame: CGRect(x: 0, y: 200, width: 200, height: 200))
        defer { window.isHidden = true }
        p.viewport.frame = window.bounds; window.addSubview(p.viewport); window.makeKeyAndVisible()
        func look(_ rows: [String: Any]) -> [String: Any] {
            ["op": "style", "id": 2, "style": rows.merging(["overflow_x": "auto", "overflow_y": "auto", "border_color_top": [0, 0, 0, 0],
                                                             "background_color": [0, 0, 255, 255]]) { a, _ in a }]
        }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"], ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            look(["border_radius": 24.0]),
            ["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 200], ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 120, "h": 120],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 400, "h": 400], ["op": "content", "id": 2, "w": 400, "h": 400],
        ]))
        let node = try XCTUnwrap(p.views[2]), child = try XCTUnwrap(p.views[3]), scroll = try XCTUnwrap(node.scroll)
        // Insets it already has, as a keyboard or a page band gives them.
        let had = (vertical: UIEdgeInsets(top: 5, left: 0, bottom: 7, right: 0), horizontal: UIEdgeInsets(top: 0, left: 3, bottom: 0, right: 9))
        scroll.verticalScrollIndicatorInsets = had.vertical
        scroll.horizontalScrollIndicatorInsets = had.horizontal
        func insets(_ border: UIEdgeInsets, _ message: String) {
            window.layoutIfNeeded()
            XCTAssertEqual(scroll.verticalScrollIndicatorInsets, had.vertical.adding(border), message)
            XCTAssertEqual(scroll.horizontalScrollIndicatorInsets, had.horizontal.adding(border), message)
        }
        /// UIKit's two indicators, shown, as rects of the node: the taller one is the vertical.
        func indicators() throws -> (vertical: CGRect, horizontal: CGRect) {
            scroll.flashScrollIndicators()
            window.layoutIfNeeded()
            let bars = scroll.subviews.filter { String(describing: type(of: $0)) == "_UIScrollViewScrollIndicator" }
            XCTAssertEqual(bars.count, 2, "\(scroll.subviews)")
            for bar in bars { bar.layer.removeAllAnimations(); bar.alpha = 1 }
            let rects = bars.map { node.convert($0.bounds, from: $0) }
            return (try XCTUnwrap(rects.first { $0.height > $0.width }), try XCTUnwrap(rects.first { $0.width > $0.height }))
        }
        /// The node as its layers draw it, 120 points square at 1x: the pixel at a point.
        func shot() throws -> (CGPoint) -> [UInt8] {
            node.layer.displayIfNeeded(); child.layer.displayIfNeeded()
            let context = try XCTUnwrap(CGContext(data: nil, width: 120, height: 120, bitsPerComponent: 8, bytesPerRow: 480,
                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            // A bitmap's rows run top down and its context's y bottom up: the layer is drawn top down into it.
            context.translateBy(x: 0, y: 120); context.scaleBy(x: 1, y: -1)
            node.layer.render(in: context)
            let bytes = Array(UnsafeBufferPointer(start: try XCTUnwrap(context.data).assumingMemoryBound(to: UInt8.self), count: 120 * 480))
            return { at in Array(bytes[(Int(at.y) * 480 + Int(at.x) * 4)..<(Int(at.y) * 480 + Int(at.x) * 4 + 4)]) }
        }
        let red: [UInt8] = [255, 0, 0, 255], blue: [UInt8] = [0, 0, 255, 255]
        insets(.zero, "no border: as they were")
        // Mid-range on both axes, where each indicator is clear of the corners' curves.
        scroll.contentOffset = CGPoint(x: 140, y: 140)
        var bars = try indicators()
        XCTAssertGreaterThan(bars.vertical.maxX, 112, "with no border the vertical indicator is at the box's edge")
        XCTAssertGreaterThan(bars.horizontal.maxY, 112, "and the horizontal one at its foot")

        // An 8 point transparent border: the padding box is 104 points square from (8, 8).
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 8.0])]))
        insets(UIEdgeInsets(top: 8, left: 8, bottom: 8, right: 8), "the border's widths more, on both")
        let edge = try XCTUnwrap((scroll.layer.mask as? CAShapeLayer)?.path), padding = CGRect(x: 8, y: 8, width: 104, height: 104)
        for offset in [CGPoint.zero, CGPoint(x: 140, y: 140), CGPoint(x: 280, y: 280)] {
            scroll.contentOffset = offset
            bars = try indicators()
            XCTAssertTrue(padding.contains(bars.vertical), "the vertical indicator is inside the padding box at \(offset): \(bars.vertical)")
            XCTAssertTrue(padding.contains(bars.horizontal), "the horizontal indicator is inside the padding box at \(offset): \(bars.horizontal)")
        }
        scroll.contentOffset = CGPoint(x: 140, y: 140)
        bars = try indicators()
        let at = try shot()
        for (name, bar) in [("vertical", bars.vertical), ("horizontal", bars.horizontal)] {
            for corner in [CGPoint(x: bar.minX, y: bar.minY), CGPoint(x: bar.maxX, y: bar.minY), CGPoint(x: bar.maxX, y: bar.maxY), CGPoint(x: bar.minX, y: bar.maxY)] {
                XCTAssertTrue(edge.contains(corner), "the \(name) indicator is inside the padding edge's mask: \(bar)")
            }
            let ink = at(CGPoint(x: bar.midX, y: bar.midY))
            XCTAssertNotEqual(ink, red, "the \(name) indicator's ink shows over the child at \(bar)")
            XCTAssertNotEqual(ink, blue, "and is not cut to the ground")
        }
        XCTAssertEqual(at(CGPoint(x: 60, y: 60)), red, "the child shows where no indicator is")
        XCTAssertEqual(at(CGPoint(x: 116, y: 60)), blue, "and the transparent border shows the ground, with no indicator in it")
        XCTAssertEqual(at(CGPoint(x: 60, y: 116)), blue)

        // A keyboard's term moves by its difference, over the border's.
        let keyboard = KeyboardInset()
        keyboard.apply(30, to: scroll)
        XCTAssertEqual(scroll.verticalScrollIndicatorInsets.bottom, had.vertical.bottom + 8 + 30)
        keyboard.apply(0, to: scroll)
        insets(UIEdgeInsets(top: 8, left: 8, bottom: 8, right: 8), "the keyboard gone")

        // Borders of other widths, and a layout transition, which masks at the same edge.
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 4.0, "border_width_left": 16.0])]))
        insets(UIEdgeInsets(top: 4, left: 16, bottom: 4, right: 4), "the new widths")
        node.layoutResize = CGSize(width: 0, height: 20); node.applyTransform(); node.applySurface()
        insets(UIEdgeInsets(top: 4, left: 16, bottom: 4, right: 4), "kept through a layout transition")
        let surfaceMask = try XCTUnwrap(scroll.layer.mask)
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 2.0, "border_width_top": 12.0, "border_width_right": 6.0])]))
        insets(UIEdgeInsets(top: 12, left: 2, bottom: 2, right: 6), "new widths while the surface owns the mask")
        XCTAssertTrue(scroll.layer.mask === surfaceMask)
        p.apply(wireBatch([look(["border_radius": 24.0])]))
        insets(.zero, "the border removed while the surface owns the mask")
        var during = ScrollInsetsIOS.Rows()
        during.verticalIndicator = [5, 0, 7, 0]; during.horizontalIndicator = [0, 9, 0, 3]
        ScrollInsetsIOS.write(scroll, in: window, terms: .init(), rows: during)
        insets(.zero, "the whole-inset writer sees no removed border during the surface")
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 4.0, "border_width_left": 16.0])]))
        ScrollInsetsIOS.write(scroll, in: window, terms: .init(), rows: during)
        insets(UIEdgeInsets(top: 4, left: 16, bottom: 4, right: 4), "the whole-inset writer adds the current border once during the surface")
        XCTAssertTrue(scroll.layer.mask === surfaceMask)
        node.layoutResize = .zero; node.applyTransform(); node.applySurface()
        insets(UIEdgeInsets(top: 4, left: 16, bottom: 4, right: 4), "and after it")

        // D14's writer writes the insets whole: the same rows, with the border and without.
        p.apply(wireBatch([look(["border_radius": 24.0])]))
        insets(.zero, "the border gone: as they were")
        XCTAssertNil(scroll.layer.mask)
        var rows = ScrollInsetsIOS.Rows()
        rows.verticalIndicator = [5, 0, 7, 0]; rows.horizontalIndicator = [0, 9, 0, 3]
        ScrollInsetsIOS.write(scroll, in: window, terms: .init(), rows: rows)
        let written = (vertical: scroll.verticalScrollIndicatorInsets, horizontal: scroll.horizontalScrollIndicatorInsets)
        p.apply(wireBatch([look(["border_radius": 24.0, "border_width": 8.0])]))
        ScrollInsetsIOS.write(scroll, in: window, terms: .init(), rows: rows)
        let border = UIEdgeInsets(top: 8, left: 8, bottom: 8, right: 8)
        XCTAssertEqual(scroll.verticalScrollIndicatorInsets, written.vertical.adding(border), "the writer adds the border's widths")
        XCTAssertEqual(scroll.horizontalScrollIndicatorInsets, written.horizontal.adding(border))
        ScrollInsetsIOS.write(scroll, in: window, terms: .init(), rows: rows)
        XCTAssertEqual(scroll.verticalScrollIndicatorInsets, written.vertical.adding(border), "once, however often it writes")
        p.apply(wireBatch([look(["border_radius": 24.0])]))
        XCTAssertEqual(scroll.verticalScrollIndicatorInsets, written.vertical, "and the border gone leaves what it wrote")
        XCTAssertEqual(scroll.horizontalScrollIndicatorInsets, written.horizontal)
        bars = try indicators()
        XCTAssertGreaterThan(bars.vertical.maxX, 112, "the indicators are back at the box's edge")
        XCTAssertGreaterThan(bars.horizontal.maxY, 112)
    }
}
#endif
