#if os(iOS)
import UIKit
import XCTest
import CoreText
@testable import ExactKit

final class ProfiledCoverageIOSTests: XCTestCase {
    private let space = CGColorSpace(name: CGColorSpace.extendedLinearSRGB)!

    func testTemplateCoverageKeepsProfileAndPremultipliedAlphaAtEveryPixel() throws {
        let width = 24, height = 4
        let context = try XCTUnwrap(CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.alphaOnly.rawValue))
        let alpha: [CGFloat] = [0, 0.25, 0.5, 0.75, 1, 0.5]
        for (index, value) in alpha.enumerated() {
            context.setFillColor(gray: 1, alpha: value)
            context.fill(CGRect(x: index * 4, y: 0, width: 4, height: height))
        }
        context.setFillColor(gray: 1, alpha: 0.5)
        context.fill(CGRect(x: 20, y: 0, width: 4, height: height))
        let image = try XCTUnwrap(context.makeImage())
        let colors = [
            CGColor(gray: 0.4, alpha: 0.5),
            CGColor(colorSpace: CGColorSpaceCreateDeviceCMYK(), components: [0.2, 0.8, 0.1, 0.1, 0.5])!,
            CGColor(colorSpace: CGColorSpace(name: CGColorSpace.sRGB)!, components: [1, 0, 0, 0.5])!,
            CGColor(colorSpace: CGColorSpace(name: CGColorSpace.displayP3)!, components: [1, 0, 0, 0.5])!,
            CGColor(colorSpace: CGColorSpace(name: CGColorSpace.displayP3)!, components: [0, 1, 0, 1])!,
            CGColor(colorSpace: CGColorSpace(name: CGColorSpace.adobeRGB1998)!, components: [0, 1, 0, 1])!,
            CGColor(colorSpace: space, components: [1.225, -0.042, -0.0196, 1])!
        ]
        for color in colors {
            let coverage = TextCoverage(image, color: color)
            let view = CoverageImageView()
            view.frame = CGRect(x: 0, y: 0, width: width, height: height)
            view.show(coverage, scale: 1)
            view.layoutIfNeeded()
            view.layer.displayIfNeeded()
            XCTAssertNil(view.layer.mask)
            XCTAssertTrue(view.image?.cgImage === image)
            XCTAssertEqual(view.tintAdjustmentMode, .normal)
            let expected = try XCTUnwrap(coverage.color.converted(to: space, intent: .relativeColorimetric, options: nil)?.components)
            let pixels = try read(view.layer, width: width, height: height)
            for y in 0..<height {
                for x in 0..<width {
                    let a = x < 20 ? alpha[x / 4] : 0.75
                    let offset = (y * width + x) * 4
                    for channel in 0..<4 {
                        let want = channel == 3 ? a : expected[channel] * a
                        XCTAssertEqual(Double(pixels[offset + channel]), Double(want), accuracy: 0.008,
                            "profile \(color.colorSpace?.name as Any), pixel \(x),\(y), channel \(channel)")
                    }
                }
            }
            let baked = try XCTUnwrap(coverage.painted())
            let copied = CALayer()
            copied.bounds = view.bounds
            copied.contents = baked
            let copyPixels = try read(copied, width: width, height: height)
            XCTAssertLessThan(zip(pixels, copyPixels).map { abs($0 - $1) }.max() ?? 1, 0.008, "capture colorization matches native tint at every pixel")
        }
    }

    func testColorizedCaptureKeepsAsymmetricCoverageOrientation() throws {
        let width = 17, height = 13
        let context = try XCTUnwrap(CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.alphaOnly.rawValue))
        context.setFillColor(gray: 1, alpha: 0.5)
        context.fill(CGRect(x: 2, y: 1, width: 5, height: 3))
        context.setFillColor(gray: 1, alpha: 1)
        context.fill(CGRect(x: 11, y: 7, width: 4, height: 5))
        let color = CGColor(colorSpace: CGColorSpace(name: CGColorSpace.displayP3)!, components: [1, 0, 0, 0.5])!
        let coverage = TextCoverage(try XCTUnwrap(context.makeImage()), color: color)
        let view = CoverageImageView()
        view.bounds = CGRect(x: 0, y: 0, width: width, height: height)
        view.show(coverage, scale: 1)
        view.layoutIfNeeded()
        view.layer.displayIfNeeded()
        let live = try read(view.layer, width: width, height: height)
        let copy = CALayer()
        copy.bounds = view.bounds
        copy.contents = try XCTUnwrap(coverage.painted())
        let captured = try read(copy, width: width, height: height)
        XCTAssertLessThan(zip(live, captured).map { abs($0 - $1) }.max() ?? 1, 0.008)
        XCTAssertNotEqual(Array(live.prefix(width * 3 * 4)), Array(live.suffix(width * 3 * 4)), "the fixture is asymmetric")
    }

    func testProfileOnlyPublicationReusesCoverageAndAlphaChangesReplaceIt() throws {
        let presenter = Presenter()
        defer { presenter.reset() }
        presenter.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "text", "props": ["text": "Ink"]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 80, "h": 30]
        ]))
        let node = try XCTUnwrap(presenter.views[1])
        let key = TextRasterKey(spec: node.paragraphSpec(), size: node.bounds.size, box: node.paragraphBox(), scale: 3)
        let frame = CGRect(x: 0, y: 0, width: CGFloat(49) / 3, height: 20)
        func publish(_ color: CGColor, text: String = "HH") throws -> CGImage {
            let drawing = TextDrawing(text, color: color, baseline: 15, frame: frame, scale: 3)
            node.textRasterKey = key; node.textRasterReady = false
            node.showTextRaster(TextRasterImage(drawing, covered: node.bounds), for: key)
            return try XCTUnwrap((node.textRasterLayer?.delegate as? CoverageImageView)?.coverage?.mask)
        }
        let srgb = CGColor(colorSpace: CGColorSpace(name: CGColorSpace.sRGB)!, components: [1, 0, 0, 0.5])!
        let p3 = CGColor(colorSpace: CGColorSpace(name: CGColorSpace.displayP3)!, components: [1, 0, 0, 0.5])!
        let first = try publish(srgb)
        XCTAssertTrue(try publish(p3) === first, "profile and RGB components do not change coverage")
        let opaque = try publish(p3.copy(alpha: 1)!)
        XCTAssertFalse(opaque === first, "opacity changes each glyph's coverage")
        XCTAssertFalse(try publish(p3.copy(alpha: 1)!, text: "WW") === opaque, "glyph changes invalidate coverage")

        let line = CTLineCreateWithAttributedString(NSAttributedString(string: "H", attributes: [.font: UIFont.systemFont(ofSize: 16)]))
        let exact = TextDrawing([try XCTUnwrap(MeasuredGlyphs.Line(line, origin: 0))], at: [CGPoint(x: 0, y: 15)],
            frame: frame, scale: 3, color: p3, smooths: true, pixels: CGSize(width: 49, height: 60))
        XCTAssertEqual(try XCTUnwrap(exact.coverage()).mask.width, 49, "integer raster extents survive point conversion")
    }

    func testLaterAuthoredChildrenStayAboveTheCoverageView() throws {
        let presenter = Presenter()
        defer { presenter.reset() }
        presenter.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "text", "props": ["text": "Ink"]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 80, "h": 30]
        ]))
        let node = try XCTUnwrap(presenter.views[1])
        let key = TextRasterKey(spec: node.paragraphSpec(), size: node.bounds.size, box: node.paragraphBox(), scale: 1)
        node.textRasterKey = key; node.textRasterReady = false
        node.showTextRaster(TextRasterImage(TextDrawing("HH", color: UIColor.red.cgColor, baseline: 15, frame: node.bounds), covered: node.bounds), for: key)
        let box = try XCTUnwrap(node.specs?.rasterView)
        presenter.apply(wireBatch([
            ["op": "create", "id": 2, "kind": "view"], ["op": "create", "id": 3, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2, 3]]
        ]))
        let first = try XCTUnwrap(presenter.views[2]), second = try XCTUnwrap(presenter.views[3])
        XCTAssertTrue(node.subviews.first === box)
        let layers = try XCTUnwrap(node.layer.sublayers)
        XCTAssertLessThan(try XCTUnwrap(layers.firstIndex(of: box.layer)), try XCTUnwrap(layers.firstIndex(of: first.layer)))
        presenter.apply(wireBatch([["op": "children", "id": 1, "ids": [3, 2]]]))
        XCTAssertTrue(node.subviews.first === box)
        XCTAssertTrue(node.subviews.dropFirst().first === second)
        XCTAssertTrue(node.subviews.last === first)
    }

    func testExactHostsPlainAndDecoratedP3CoverageAsUIKitChildren() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "coverage text needs a hosted UIKit hierarchy") }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 600)
        window.makeKeyAndVisible()
        let session = ExactApp.shared.makeSession(label: "profiled-coverage")
        defer { window.isHidden = true; session.destroy() }
        let presenter = session.presenter
        presenter.viewport.frame = window.bounds
        window.rootViewController!.view.addSubview(presenter.viewport)
        let color: [String: Any] = ["cs": [["s": "display-p3", "v": [1.0, 0.0, 0.0, 0.5]]], "c": [255.0, 0.0, 0.0, 128.0]]
        presenter.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "Plain P3"], "style": ["font_size": 32.0, "text_color": color]],
            ["op": "create", "id": 3, "kind": "text", "props": ["text": "Underlined P3"], "style": ["font_size": 32.0, "text_color": color, "text_decoration_line": "underline"]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 600.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 340.0, "h": 70.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 100.0, "w": 340.0, "h": 70.0]
        ]))
        let nodes = [2, 3].compactMap { presenter.views[UInt32($0)] }
        presenter.textRasters.ensureUrgent(nodes)
        presenter.textRasters.settleVisible(nodes)
        for node in nodes {
            let view = try XCTUnwrap(node.textRasterLayer?.delegate as? CoverageImageView)
            let holder = try XCTUnwrap(node.specs?.rasterView)
            XCTAssertTrue(holder.superview === node)
            XCTAssertTrue(view.superview === holder.lines)
            XCTAssertTrue(view.layer.superlayer === holder.lines.layer)
            XCTAssertTrue(holder.layer === node.textBoxLayer)
            let coverage = try XCTUnwrap(view.coverage)
            XCTAssertEqual(coverage.mask.bitsPerPixel, 8)
            XCTAssertEqual(coverage.color.alpha, 1)
            XCTAssertEqual(coverage.color, TextEngine.color(node.paragraphSpec().color).cgColor.copy(alpha: 1))
            XCTAssertNil(view.layer.mask)
            let width = coverage.mask.width, height = coverage.mask.height
            let pixels = try read(view.layer, width: width, height: height)
            let maximumAlpha = stride(from: 3, to: pixels.count, by: 4).map { pixels[$0] }.max() ?? 0
            XCTAssertEqual(maximumAlpha, 0.5, accuracy: 0.015, "opacity is in coverage once")
            XCTAssertGreaterThan(stride(from: 0, to: pixels.count, by: 4).map { pixels[$0] }.max() ?? 0, 0.60,
                "half-opacity P3 red retains its extended-linear red component")
        }
        guard case .coverage? = presenter.views[3]?.textRaster else { return XCTFail("CoreText decorations stay in coverage") }
    }

    func testIdenticalParagraphsShareOnlyLiveCoverageAcrossBatches() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "coverage sharing needs a hosted UIKit hierarchy") }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 600)
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        for warmed in [false, true] {
            let session = ExactApp.shared.makeSession(label: "coverage-sharing")
            defer { session.presenter.viewport.removeFromSuperview(); session.destroy() }
            let presenter = session.presenter
            presenter.viewport.frame = window.bounds
            window.rootViewController!.view.addSubview(presenter.viewport)
            presenter.apply(wireBatch([
                ["op": "create", "id": 1, "kind": "view"], ["op": "roots", "ids": [1]],
                ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 600.0]
            ]))
            let style: [String: Any] = ["font_size": 16.0, "line_height": "24px", "text_color": [26, 51, 153, 128]]
            func operations(_ range: Range<Int>) -> [[String: Any]] {
                range.flatMap { index in [
                    ["op": "create", "id": index + 2, "kind": "text", "props": ["text": "Repeated label"], "style": style],
                    ["op": "frame", "id": index + 2, "x": 0.0, "y": Double(index) * 24, "w": 340.0, "h": 24.0]
                ] }
            }
            if warmed {
                TextRenderMetrics.shared.start()
                defer { TextRenderMetrics.shared.stop() }
                // Below the port, so a worker paints it: a batch paints the new
                // words that show in its own frame, on the main thread.
                let below: [String: Any] = ["op": "frame", "id": 2, "x": 0.0, "y": 900.0, "w": 340.0, "h": 24.0]
                presenter.apply(wireBatch(operations(0..<1) + [below, ["op": "children", "id": 1, "ids": [2]]]))
                presenter.textRasters.settleVisible([try XCTUnwrap(presenter.views[2])])
                TextRenderMetrics.shared.stop()
                let spans = try XCTUnwrap(TextRenderMetrics.shared.snapshot(origin: 0)["rows"] as? [[Any]])
                let paints = spans.filter { ($0[2] as? Int) == TextRenderMetrics.Kind.coveragePaint.rawValue && ($0[3] as? UInt32) == 2 }
                XCTAssertEqual(paints.count, 1)
                XCTAssertEqual(paints.first?[4] as? Bool, false, "the worker carries its painted mask through settlement; publication does not repaint it")
            }
            // The warmed paragraph moves into the port, above the rest.
            let warmedInPlace: [String: Any] = ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 340.0, "h": 24.0]
            let rest = warmed ? [warmedInPlace] + operations(1..<20) : operations(0..<20)
            presenter.apply(wireBatch(rest + [["op": "children", "id": 1, "ids": Array(2..<22)]]))
            let nodes = (2..<22).compactMap { presenter.views[UInt32($0)] }
            presenter.textRasters.ensureUrgent(nodes)
            presenter.textRasters.settleVisible(nodes)
            weak var displayed: TextCoverage?
            var cachedDrawing: TextDrawing?
            try autoreleasepool {
                let views = try nodes.map { try XCTUnwrap($0.textRasterLayer?.delegate as? CoverageImageView) }
                let first = try XCTUnwrap(views.first?.coverage)
                displayed = first
                guard case .drawn(let drawing)? = nodes.first?.textRaster else { return XCTFail("plain text keeps reusable glyph values") }
                cachedDrawing = drawing
                XCTAssertTrue(views.allSatisfy { $0.coverage?.mask === first.mask }, "identical paragraphs share in a cold batch and after priming")
                let pixels = try read(try XCTUnwrap(views.first?.layer), width: first.mask.width, height: first.mask.height)
                XCTAssertEqual(stride(from: 3, to: pixels.count, by: 4).map { pixels[$0] }.max() ?? 0, 128.0 / 255, accuracy: 0.015)
                for node in nodes { node.dropTextRaster() }
            }
            XCTAssertNotNil(cachedDrawing, "glyph values remain cached")
            XCTAssertNil(displayed, "the glyph cache does not retain a displayed coverage image")
        }
    }

    func testPublicationTakesThePreparedMaskWithoutTheRecorder() throws {
        let presenter = Presenter()
        defer { presenter.reset() }
        presenter.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "text", "props": ["text": "Prepared mask"]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 30]
        ]))
        let node = try XCTUnwrap(presenter.views[1])
        let key = TextRasterKey(spec: node.paragraphSpec(), size: node.bounds.size, box: node.paragraphBox(), scale: 3)
        let engine = TextEngine(resolve: { _ in nil })
        _ = engine.paragraph(key.spec, width: key.box.width)
        let geometry = try XCTUnwrap(engine.measuredBreaks(key.spec, width: key.box.width))
        let job = TextRasterJob(source: engine.attributed(key.spec), ranges: geometry.ranges, baselines: geometry.baselines,
            flush: 0, box: key.box, size: key.size, scale: key.scale)
        let result = try XCTUnwrap(job.renderPrepared(node: 1))
        guard case .coverage(let prepared) = result.content else { return XCTFail("the result owns its prepared mask and glyph values together") }
        node.textRasterKey = key; node.textRasterReady = false
        node.showTextRaster(result, for: key)
        XCTAssertTrue((node.textRasterLayer?.delegate as? CoverageImageView)?.coverage === prepared)
        node.textRasterReady = false
        if let drawing = result.drawing {
            let other = TextDrawing(drawing.lines, at: drawing.positions, frame: drawing.frame, scale: drawing.scale,
                color: UIColor.blue.cgColor, smooths: drawing.smooths)
            node.showTextRaster(TextRasterImage(other, covered: key.box), for: key)
            XCTAssertTrue((node.textRasterLayer?.delegate as? CoverageImageView)?.coverage?.mask === prepared.mask)
        } else { XCTFail("plain text prepares reusable glyph values") }
    }

    func testTemplateTintMatchesProfiledReferenceInWindowSnapshot() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "template tint needs a hosted window snapshot") }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.frame = scene.screen.bounds
        let root = try XCTUnwrap(window.rootViewController?.view)
        root.backgroundColor = .white
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        let colors: [[CGFloat]] = [[0.6, 0.4, 0.2], [0.4, 0.6, 0.7]]
        var centers: [CGPoint] = []
        for (index, components) in colors.enumerated() {
            for (opacity, alpha) in [CGFloat(1), 0.5].enumerated() {
                let y = CGFloat(100 + (index * 2 + opacity) * 40)
                let color = try XCTUnwrap(CGColor(colorSpace: CGColorSpace(name: CGColorSpace.displayP3)!, components: components + [alpha]))
                let context = try XCTUnwrap(CGContext(data: nil, width: 16, height: 16, bitsPerComponent: 8, bytesPerRow: 0,
                    space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.alphaOnly.rawValue))
                context.setFillColor(gray: 1, alpha: alpha)
                context.fill(CGRect(x: 0, y: 0, width: 16, height: 16))
                let template = CoverageImageView()
                template.frame = CGRect(x: 20, y: y, width: 16, height: 16)
                template.show(TextCoverage(try XCTUnwrap(context.makeImage()), color: color), scale: 1)
                let reference = UIView(frame: CGRect(x: 60, y: y, width: 16, height: 16))
                reference.backgroundColor = UIColor(cgColor: color)
                let unprofiled = UIView(frame: CGRect(x: 100, y: y, width: 16, height: 16))
                unprofiled.backgroundColor = UIColor(red: components[0], green: components[1], blue: components[2], alpha: alpha)
                for view in [template, reference, unprofiled] { root.addSubview(view) }
                centers.append(CGPoint(x: 28, y: y + 8))
            }
        }
        window.layoutIfNeeded()
        let format = UIGraphicsImageRendererFormat()
        format.preferredRange = .extended
        format.scale = 1
        for _ in 0..<20 {
            var captured = false
            let snapshot = UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { _ in
                captured = window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
            }
            XCTAssertTrue(captured)
            let image = try XCTUnwrap(snapshot.cgImage)
            for center in centers {
                let actual = try pixel(image, at: center)
                let reference = try pixel(image, at: CGPoint(x: center.x + 40, y: center.y))
                let unprofiled = try pixel(image, at: CGPoint(x: center.x + 80, y: center.y))
                XCTAssertLessThan(zip(actual, reference).map { abs($0 - $1) }.max() ?? 1, 0.008)
                XCTAssertGreaterThan(zip(reference.prefix(3), unprofiled.prefix(3)).map { abs($0 - $1) }.max() ?? 0, 0.01,
                    "the snapshot must distinguish P3 components from the same numerical sRGB components")
            }
        }
    }

    func testAuthoredP3PrimariesMatchUILabelInWindowSnapshots() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "P3 text needs a hosted window snapshot") }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.frame = scene.screen.bounds
        let root = try XCTUnwrap(window.rootViewController?.view)
        root.backgroundColor = .black
        window.makeKeyAndVisible()
        let session = ExactApp.shared.makeSession(label: "p3-window-oracle")
        defer { window.isHidden = true; session.destroy() }
        let presenter = session.presenter
        presenter.viewport.frame = window.bounds
        root.addSubview(presenter.viewport)
        var operations: [[String: Any]] = []
        for channel in 0..<3 {
            var components = [CGFloat](repeating: 0, count: 3)
            components[channel] = 1
            let color: [String: Any] = ["cs": [["s": "display-p3", "v": components + [1]]], "c": components.map { $0 * 255 } + [255]]
            operations += [
                ["op": "create", "id": channel + 1, "kind": "text", "props": ["text": "█"],
                 "style": ["font_size": 48.0, "line_height": "64px", "text_color": color]],
                ["op": "frame", "id": channel + 1, "x": 20.0, "y": Double(100 + channel * 80), "w": 64.0, "h": 64.0]
            ]
            let label = UILabel(frame: CGRect(x: 140, y: 100 + channel * 80, width: 64, height: 64))
            label.font = UIFont.systemFont(ofSize: 48)
            label.text = "█"
            label.textColor = UIColor(cgColor: CGColor(colorSpace: CGColorSpace(name: CGColorSpace.displayP3)!, components: components + [1])!)
            root.addSubview(label)
        }
        presenter.apply(wireBatch(operations + [["op": "roots", "ids": [1, 2, 3]]]))
        let nodes = (1...3).compactMap { presenter.views[UInt32($0)] }
        XCTAssertEqual(nodes.count, 3)
        presenter.textRasters.ensureUrgent(nodes)
        presenter.textRasters.settleVisible(nodes)
        for (channel, node) in nodes.enumerated() {
            let coverage = try XCTUnwrap((node.textRasterLayer?.delegate as? CoverageImageView)?.coverage)
            var components = [CGFloat](repeating: 0, count: 3)
            components[channel] = 1
            let authored = try XCTUnwrap(CGColor(colorSpace: CGColorSpace(name: CGColorSpace.displayP3)!, components: components + [1]))
            let expected = try XCTUnwrap(authored.converted(to: space, intent: .relativeColorimetric, options: nil)?.components)
            let actual = try XCTUnwrap(coverage.color.converted(to: space, intent: .relativeColorimetric, options: nil)?.components)
            for (value, reference) in zip(actual, expected) { XCTAssertEqual(value, reference, accuracy: 0.0001) }
        }
        window.layoutIfNeeded()
        let format = UIGraphicsImageRendererFormat()
        format.preferredRange = .extended
        format.scale = 1
        for _ in 0..<20 {
            var captured = false
            let snapshot = UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { _ in
                captured = window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
            }
            XCTAssertTrue(captured)
            let image = try XCTUnwrap(snapshot.cgImage)
            for channel in 0..<3 {
                let actual = try maximum(image, in: CGRect(x: 20, y: 100 + channel * 80, width: 64, height: 64), channel: channel)
                let reference = try maximum(image, in: CGRect(x: 140, y: 100 + channel * 80, width: 64, height: 64), channel: channel)
                XCTAssertGreaterThan(reference, 0.8, "the native full-block glyph must be visible")
                XCTAssertEqual(actual, reference, accuracy: 0.01, "authored P3 text matches UILabel through the same window capture")
            }
        }
    }

    private func maximum(_ image: CGImage, in rect: CGRect, channel: Int) throws -> Float {
        let sample = try XCTUnwrap(image.cropping(to: rect))
        let context = try XCTUnwrap(CGContext(data: nil, width: sample.width, height: sample.height, bitsPerComponent: 32,
            bytesPerRow: sample.width * 16, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue |
                CGBitmapInfo.floatComponents.rawValue | CGBitmapInfo.byteOrder32Little.rawValue))
        context.draw(sample, in: CGRect(x: 0, y: 0, width: sample.width, height: sample.height))
        let data = context.data!.assumingMemoryBound(to: Float.self)
        return stride(from: channel, to: sample.width * sample.height * 4, by: 4).map { data[$0] }.max() ?? 0
    }

    private func pixel(_ image: CGImage, at point: CGPoint) throws -> [Float] {
        let sample = try XCTUnwrap(image.cropping(to: CGRect(x: point.x, y: point.y, width: 1, height: 1)))
        let context = try XCTUnwrap(CGContext(data: nil, width: 1, height: 1, bitsPerComponent: 32, bytesPerRow: 16,
            space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue |
                CGBitmapInfo.floatComponents.rawValue | CGBitmapInfo.byteOrder32Little.rawValue))
        context.draw(sample, in: CGRect(x: 0, y: 0, width: 1, height: 1))
        return Array(UnsafeBufferPointer(start: context.data!.assumingMemoryBound(to: Float.self), count: 4))
    }

    private func read(_ layer: CALayer, width: Int, height: Int) throws -> [Float] {
        let context = try XCTUnwrap(CGContext(data: nil, width: width, height: height, bitsPerComponent: 32,
            bytesPerRow: width * 16, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue |
                CGBitmapInfo.floatComponents.rawValue | CGBitmapInfo.byteOrder32Little.rawValue))
        context.scaleBy(x: CGFloat(width) / layer.bounds.width, y: CGFloat(height) / layer.bounds.height)
        layer.render(in: context)
        return Array(UnsafeBufferPointer(start: context.data!.assumingMemoryBound(to: Float.self), count: width * height * 4))
    }
}
#endif
