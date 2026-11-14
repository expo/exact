import XCTest
import CoreGraphics
import ImageIO
#if os(macOS)
import AppKit
#else
import UIKit
#endif
@testable import ExactKit

final class ImageRenderingTests: XCTestCase {
    private func sprite() throws -> CGImage {
        let ctx = try context(2, 1)
        ctx.setFillColor(CGColor(gray: 0, alpha: 1)); ctx.fill(CGRect(x: 0, y: 0, width: 1, height: 1))
        ctx.setFillColor(CGColor(gray: 1, alpha: 1)); ctx.fill(CGRect(x: 1, y: 0, width: 1, height: 1))
        return try XCTUnwrap(ctx.makeImage())
    }

    private func context(_ width: Int, _ height: Int) throws -> CGContext {
        try XCTUnwrap(CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
            bytesPerRow: width * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
    }

    func testIntegerAndFractionalSamplingAndTileBoundaries() throws {
        let image = try sprite()
        for width in [4, 5, 7, 9, 601] {
            for mode in ["crisp-edges", "pixelated"] {
                let ctx = try context(width, 2)
                RasterGeometry.draw(ctx, image, in: CGRect(x: 0, y: 0, width: width, height: 2), tint: nil, rendering: mode)
                let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
                for x in 0..<width {
                    let at = (Double(x) + 0.5) * 2 / Double(width)
                    let expected: Double
                    if mode == "crisp-edges" { expected = at < 1 ? 0 : 255 }
                    else {
                        let n = (Double(width) / 2).rounded()
                        let virtual = at * n - 0.5, lo = floor(virtual), fraction = virtual - lo
                        let a = min(1, max(0, floor(lo / n)))
                        let b = min(1, max(0, floor((lo + 1) / n)))
                        expected = ((1 - fraction) * a + fraction * b) * 255
                    }
                    if mode == "crisp-edges", at == floor(at) {
                        XCTAssertTrue(bytes[x * 4] == 0 || bytes[x * 4] == 255)
                        continue
                    }
                    XCTAssertEqual(Double(bytes[x * 4]), expected, accuracy: 2, "\(mode) width \(width), x \(x)")
                }
            }
        }
    }

    func testLayerTilesKeepImageOrientationAndSampleAcrossBothAxes() throws {
        let source = try context(2, 2)
        for (i, color) in [CGColor(red: 1, green: 0, blue: 0, alpha: 1), CGColor(red: 0, green: 1, blue: 0, alpha: 1),
                           CGColor(red: 0, green: 0, blue: 1, alpha: 1), CGColor(gray: 1, alpha: 1)].enumerated() {
            source.setFillColor(color); source.fill(CGRect(x: i % 2, y: i / 2, width: 1, height: 1))
        }
        let image = try XCTUnwrap(source.makeImage())
        for (width, height) in [(4, 4), (600, 6), (6, 600)] {
            let rect = CGRect(x: 0, y: 0, width: width, height: height)
            let actual = try context(width, height), expected = try context(width, height)
            actual.interpolationQuality = .low; expected.interpolationQuality = .none
            let layer = PixelatedImageLayer(), reference = CALayer()
            layer.bounds = rect; reference.bounds = rect
            layer.update(image, rect: rect, visible: rect, scale: CGSize(width: 1, height: 1), tint: nil)
            reference.contents = image
            RasterGeometry.filter(reference, rendering: "crisp-edges")
            layer.render(in: actual); reference.render(in: expected)
            let a = Data(bytes: try XCTUnwrap(actual.data), count: width * height * 4)
            let e = Data(bytes: try XCTUnwrap(expected.data), count: width * height * 4)
            let differences = a.indices.filter { a[$0] != e[$0] }
            let samples = differences.prefix(12).map { "\($0 / 4 % width),\($0 / 4 / width):\(a[$0])/\(e[$0])" }
            XCTAssertTrue(differences.isEmpty, "\(width)×\(height): \(differences.count) differing channels, \(samples)")
        }
    }

    @MainActor func testPixelatedReplacesAThumbnailWithOriginalSamples() throws {
        #if os(macOS)
        _ = NSApplication.shared
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 40, height: 40), styleMask: [], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        #else
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 40, height: 40))
        #endif
        let session = ExactApp.shared.makeSession(label: "image-rendering-decode")
        let p = session.presenter, loader = session.rasters
        p.viewport.frame = CGRect(x: 0, y: 0, width: 40, height: 40)
        #if os(macOS)
        window.contentView = p.viewport
        #else
        let controller = UIViewController(); controller.view = p.viewport; window.rootViewController = controller
        #endif
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "image", "props": [:], "handlers": []],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 8194, "h": 2],
            ["op": "roots", "ids": [1]]
        ]))
        let node = try XCTUnwrap(p.views[1])
        defer {
            session.destroy(); node.raster = nil
            #if os(macOS)
            window.close()
            #else
            window.isHidden = true
            #endif
        }
        let source = try context(4097, 1)
        let pixels = try XCTUnwrap(source.data).assumingMemoryBound(to: UInt8.self)
        for x in 0..<4097 {
            let value: UInt8 = x.isMultiple(of: 2) ? 0 : 255
            for channel in 0..<3 { pixels[x * 4 + channel] = value }
            pixels[x * 4 + 3] = 255
        }
        let encoded = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(encoded, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, try XCTUnwrap(source.makeImage()), nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        let resolver = AssetResolver(root: URL(fileURLWithPath: "/"))
        XCTAssertTrue(loader.load(node, source: "data:image/png;base64," + (encoded as Data).base64EncodedString(), resolver: resolver))
        waitFor("thumbnail decoded") { loader.reconcile(); return node.raster != nil }
        XCTAssertEqual(node.raster?.image.image.width, 4096)
        p.apply(wireBatch([["op": "style", "id": 1, "style": ["image_rendering": "pixelated"]]]))
        waitFor("source samples decoded", facts: { String(describing: loader.diagnostics) }) {
            loader.reconcile(); return node.raster?.image.image.width == 4097
        }
        let image = try XCTUnwrap(node.raster?.image.image)
        XCTAssertEqual(image.width, 4097)
        guard image.width == 4097 else { return }
        let output = try context(8194, 2)
        RasterGeometry.draw(output, image, in: CGRect(x: 0, y: 0, width: 8194, height: 2), tint: nil, rendering: "pixelated")
        let result = try XCTUnwrap(output.data).assumingMemoryBound(to: UInt8.self)
        var mismatches = 0
        for y in 0..<2 {
            for x in 0..<8194 {
                let expected: UInt8 = (x / 2).isMultiple(of: 2) ? 0 : 255
                if result[(y * 8194 + x) * 4] != expected { mismatches += 1 }
            }
        }
        XCTAssertEqual(mismatches, 0, "each source sample repeats twice")
    }

    @MainActor func testLiveImageSwitchesItsLayerFiltersAndDrawnSampling() throws {
        #if os(macOS)
        _ = NSApplication.shared
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 40, height: 40), styleMask: [], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        #else
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 40, height: 40))
        #endif
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 40, height: 40)
        #if os(macOS)
        window.contentView = p.viewport
        #else
        let controller = UIViewController(); controller.view = p.viewport; window.rootViewController = controller
        #endif
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "image", "props": [:], "handlers": []],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 5, "h": 2],
            ["op": "roots", "ids": [1]]
        ]))
        let node = try XCTUnwrap(p.views[1]), loader = RasterLoader()
        let resolver = AssetResolver(root: URL(fileURLWithPath: "/"))
        defer {
            loader.shutdown(); node.raster = nil
            #if os(macOS)
            window.close()
            #else
            window.isHidden = true
            #endif
        }
        let encoded = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(encoded, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, try sprite(), nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        XCTAssertTrue(loader.load(node, source: "data:image/png;base64," + (encoded as Data).base64EncodedString(), resolver: resolver))
        waitFor("sprite decoded") { loader.reconcile(); return node.raster != nil }
        for mode in ["crisp-edges", "pixelated", "auto", "crisp-edges"] {
            node.applyStyle(["image_rendering": .string(mode)])
            node.applyImageLayer()
            if mode == "pixelated" {
                let capture = try XCTUnwrap(Capture.bitmap(of: node, scale: 1))
                #if os(macOS)
                let picture = try XCTUnwrap(capture.cgImage)
                #else
                let picture = try XCTUnwrap(capture.context.makeImage())
                #endif
                let ctx = try context(picture.width, picture.height)
                ctx.draw(picture, in: CGRect(x: 0, y: 0, width: picture.width, height: picture.height))
                let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
                let pixels = stride(from: 0, to: picture.width * picture.height * 4, by: 4)
                XCTAssertTrue(pixels.contains { bytes[$0] == 0 && bytes[$0 + 3] == 255 })
                XCTAssertTrue(pixels.contains { bytes[$0] == 255 && bytes[$0 + 3] == 255 })
                p.apply(wireBatch([["op": "frame", "id": 1, "x": 10, "y": 10, "w": 2, "h": 1]]))
                #if os(macOS)
                let displayScale = node.pixelScale ?? 1
                #else
                let displayScale = node.traitCollection.displayScale
                #endif
                for scale in [2.0, 2.5] {
                    node.scale = scale; node.applyTransform()
                    #if os(macOS)
                    Capture.prepareBackingLayers(of: p.viewport)
                    p.viewport.displayIfNeeded()
                    let root = try XCTUnwrap(p.viewport.layer)
                    #else
                    p.viewport.layoutIfNeeded()
                    let root = p.viewport.layer
                    root.displayIfNeeded()
                    #endif
                    let size = Int(40 * displayScale)
                    let actual = try context(size, size)
                    actual.interpolationQuality = .low
                    actual.translateBy(x: 0, y: CGFloat(size)); actual.scaleBy(x: displayScale, y: -displayScale)
                    root.render(in: actual)
                    #if os(macOS)
                    let sourceLayer = try XCTUnwrap(node.layer)
                    #else
                    let sourceLayer = node.layer
                    #endif
                    let rect = root.convert(sourceLayer.bounds, from: sourceLayer)
                    let expected = try context(size, 4)
                    RasterGeometry.draw(expected, try sprite(), in: CGRect(x: rect.minX * displayScale, y: 0, width: rect.width * displayScale, height: 4), tint: nil, rendering: mode)
                    let a = try XCTUnwrap(actual.data).assumingMemoryBound(to: UInt8.self)
                    let e = try XCTUnwrap(expected.data).assumingMemoryBound(to: UInt8.self)
                    let columns = Int(ceil(rect.minX * displayScale))..<Int(floor(rect.maxX * displayScale))
                    let row = (0..<size).max { left, right in
                        func darkness(_ row: Int) -> Int { columns.reduce(0) { $0 + Int(a[(row * size + $1) * 4 + 3]) - Int(a[(row * size + $1) * 4]) } }
                        return darkness(left) < darkness(right)
                    }!
                    var mismatches = 0
                    for x in columns {
                        if abs(Int(a[(row * size + x) * 4]) - Int(e[x * 4])) > 2 { mismatches += 1 }
                    }
                    XCTAssertEqual(mismatches, 0, "the composited scale \(scale) samples the source pixels; actual \(columns.map { a[(row * size + $0) * 4] }), expected \(columns.map { e[$0 * 4] })")
                }
                node.scale = 1; node.applyTransform()
            } else {
                let layer = try XCTUnwrap(node.imageLayer)
                let pixels = layer.contents != nil ? layer : try XCTUnwrap(layer.sublayers?.first)
                XCTAssertEqual(pixels.magnificationFilter, mode == "auto" ? .linear : .nearest)
                XCTAssertEqual(pixels.minificationFilter, mode == "auto" ? .linear : .nearest)
            }
        }
    }

    @MainActor func testPixelatedTilesFollowAGrowingClippingAncestor() throws {
        #if os(macOS)
        _ = NSApplication.shared
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 600, height: 40), styleMask: [], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        #else
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 600, height: 40))
        #endif
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 600, height: 40)
        #if os(macOS)
        window.contentView = p.viewport
        #else
        let controller = UIViewController(); controller.view = p.viewport; window.rootViewController = controller
        window.isHidden = false
        #endif
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["overflow_x": "hidden", "overflow_y": "hidden"]],
            ["op": "create", "id": 2, "kind": "image", "props": [:], "handlers": [], "style": ["image_rendering": "pixelated"]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 128, "h": 16],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 1024, "h": 16],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]]
        ]))
        let node = try XCTUnwrap(p.views[2]), loader = RasterLoader()
        let resolver = AssetResolver(root: URL(fileURLWithPath: "/"))
        defer {
            loader.shutdown(); node.raster = nil
            #if os(macOS)
            window.close()
            #else
            window.isHidden = true
            #endif
        }
        let source = try context(64, 1)
        source.setFillColor(CGColor(gray: 0, alpha: 1)); source.fill(CGRect(x: 0, y: 0, width: 64, height: 1))
        let encoded = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(encoded, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, try XCTUnwrap(source.makeImage()), nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        XCTAssertTrue(loader.load(node, source: "data:image/png;base64," + (encoded as Data).base64EncodedString(), resolver: resolver))
        waitFor("strip decoded") { loader.reconcile(); return node.raster != nil }
        node.applyImageLayer()
        func covered() throws -> CGFloat {
            let tiles = try XCTUnwrap(node.imageLayer as? PixelatedImageLayer)
            return (tiles.sublayers ?? []).reduce(CGRect.null) { $0.union($1.frame) }.maxX
        }
        XCTAssertLessThan(try covered(), 512, "tiles cover what the clip shows, not the whole image")
        p.apply(wireBatch([["op": "frame", "id": 1, "x": 0, "y": 0, "w": 512, "h": 16]]))
        XCTAssertGreaterThanOrEqual(try covered().rounded(), 512, "the tiles cover what the grown clip shows")
    }
}
