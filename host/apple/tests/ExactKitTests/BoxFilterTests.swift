import XCTest
import ImageIO
import CoreText
#if os(macOS)
import AppKit
#else
import UIKit
#endif
@testable import ExactKit
@testable import ExactSurfaces

final class BoxFilterTests: XCTestCase {
    private var p: Presenter!
    #if os(macOS)
    private var window: NSWindow!
    #else
    private var window: UIWindow!
    #endif

    override func setUp() {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        p = Presenter()
        #if os(macOS)
        window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 200, height: 200), styleMask: [], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        p.viewport.wantsLayer = true
        p.root.wantsLayer = true
        p.root.frame = CGRect(x: 0, y: 0, width: 200, height: 200)
        window.orderFront(nil)
        window.displayIfNeeded()
        #else
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 200, height: 200))
        let controller = UIViewController()
        controller.view = p.viewport
        window.rootViewController = controller
        window.isHidden = false
        #endif
    }

    override func tearDown() {
        #if os(macOS)
        window.close()
        #else
        window.isHidden = true
        window.rootViewController = nil
        #endif
        window = nil; p = nil
    }

    private func node(_ id: UInt32, kind: String = "view", in parent: NodeView? = nil) -> NodeView {
        let node = NodeView(id: id, kind: kind, presenter: p)
        p.views[id] = node
        node.frame = CGRect(x: 0, y: 0, width: 40, height: 40)
        if let parent { parent.addSubview(node) } else { p.root.addSubview(node) }
        return node
    }

    private func filter() throws -> BatchValue {
        ["p": .array(try program().map(BatchValue.number))]
    }

    /// A colour matrix scaling red by `red` (identity by default), with a
    /// subregion covering the whole box.
    private func program(red: Double = 1) throws -> [Double] {
        let matrix: [Double] = [red,0,0,0,0, 0,1,0,0,0, 0,0,1,0,0, 0,0,0,1,0]
        let program: [Double] = [0,0,0,0,1, 5,-1,-3,0,0,10000,10000,0] + matrix
        guard SvgFilterGPU.runs(program.map(Float.init)) else { throw missing("metal", "no GPU filter path") }
        return program
    }

    /// `drop-shadow(dx dy σ rgba)` as `box_filter` encodes it: the region
    /// how far past the box it reaches, the subregion the same.
    private func dropShadow(dx: Double, dy: Double, sigma: Double, rgba: [Double], darkRGBA: [Double]? = nil) -> BatchValue {
        func program(_ c: [Double]) -> [Double] {
            let reach = 3 * sigma
            let region = [-reach + min(dx, 0), -reach + min(dy, 0), 2 * reach + abs(dx), 2 * reach + abs(dy)]
            return region + [1, 6, -1, -3] + region + [0, sigma, sigma, dx, dy] + c
        }
        var o: [String: BatchValue] = ["p": .array(program(rgba).map(BatchValue.number))]
        if let darkRGBA { o["pd"] = .array(program(darkRGBA).map(BatchValue.number)) }
        return .object(o)
    }

    #if os(iOS) || os(tvOS)
    /// A lone `drop-shadow()` is the box's own layer shadow: no picture,
    /// no mask hiding the box, nothing to draw again after a batch. A box
    /// that clips, which would clip its layer's shadow, and a chain of more
    /// than the shadow are pictured.
    func testALoneDropShadowIsTheLayersShadow() throws {
        let box = node(1)
        box.applyStyle(["filter": dropShadow(dx: 0, dy: 2, sigma: 2, rgba: [0, 0, 0, 0.6])])
        p.boxFilters.render()
        XCTAssertNotNil(box.boxFilter)
        XCTAssertNil(box.boxFilter?.picture.superlayer, "no picture stands in for the box")
        XCTAssertNil(box.layer.mask, "the box itself shows")
        XCTAssertEqual(box.layer.shadowRadius, 2, "Core Animation's radius is the σ")
        XCTAssertEqual(box.layer.shadowOffset, CGSize(width: 0, height: 2))
        XCTAssertEqual(box.layer.shadowOpacity, 0.6, accuracy: 0.001)
        XCTAssertEqual(box.layer.shadowColor?.components?.prefix(3).map { Double($0) }, [0, 0, 0])

        box.overrideUserInterfaceStyle = .dark
        box.updateTraitsIfNeeded()
        XCTAssertTrue(box.drawsDark)
        box.applyStyle(["filter": dropShadow(dx: 0, dy: 0, sigma: 1, rgba: [0, 0, 0, 1], darkRGBA: [1, 1, 1, 1])])
        XCTAssertEqual(box.layer.shadowColor?.components?.prefix(3).map { Double($0) }, [1, 1, 1], "the dark appearance's colour")
        box.overrideUserInterfaceStyle = .unspecified
        box.updateTraitsIfNeeded()

        box.applyStyle(["filter": try filter()])
        XCTAssertNotNil(box.boxFilter?.picture.superlayer, "a chain of more than the shadow is pictured")
        XCTAssertEqual(box.layer.shadowOpacity, 0, "and the layer casts none")
        XCTAssertTrue(box.layer.mask === box.boxFilter?.hide)

        let shadow = dropShadow(dx: 0, dy: 2, sigma: 2, rgba: [0, 0, 0, 0.6])
        box.applyStyle(["filter": shadow, "overflow_x": "hidden", "overflow_y": "hidden"])
        p.boxFilters.render()
        XCTAssertTrue(box.layer.masksToBounds)
        XCTAssertNotNil(box.boxFilter?.picture.superlayer, "a box that clips is pictured, its shadow past its bounds")
        XCTAssertEqual(box.layer.shadowOpacity, 0)

        box.applyStyle(["filter": shadow])
        p.boxFilters.render()
        XCTAssertNil(box.boxFilter?.picture.superlayer)
        XCTAssertEqual(box.layer.shadowOpacity, 0.6, accuracy: 0.001)
        box.applyStyle([:])
        XCTAssertTrue(p.boxFilters.isEmpty)
        XCTAssertEqual(box.layer.shadowOpacity, 0, "an unfiltered box casts nothing")
        XCTAssertNil(box.layer.mask)
    }
    #endif

    private func picture(_ node: NodeView) throws -> CGImage {
        try XCTUnwrap(node.boxFilter?.picture.sublayers?.first?.contents) as! CGImage
    }

    private func pixel(_ image: CGImage, x: Int, y: Int) throws -> [UInt8] {
        let ctx = try XCTUnwrap(CGContext(data: nil, width: 40, height: 40, bitsPerComponent: 8,
            bytesPerRow: 160, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: 40, height: 40))
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        return Array(UnsafeBufferPointer(start: bytes + (y * 40 + x) * 4, count: 4))
    }

    #if os(macOS)
    func testAFilteredGradientKeepsSquareCornersAsItsRadiiChange() throws {
        let box = node(1), effect = try filter()
        let corners = ["top_left", "top_right", "bottom_right", "bottom_left"]
        let gradient: BatchValue = ["linear": 90, "stops": [0,255,0,0,255, 1,0,0,255,255]]
        for selected in Array(1..<16) + [0] {
            var style: NodeStyle = ["background_image": gradient, "filter": effect]
            for (i, corner) in corners.enumerated() { style["border_radius_" + corner] = .number(selected & (1 << i) == 0 ? 0 : 16) }
            box.applyStyle(style)
            window.displayIfNeeded()
            box.renderFilter(); settle()
            let image = try picture(box)
            let alpha = try [(1,1), (38,1), (38,38), (1,38)].map { try pixel(image, x: $0.0, y: $0.1)[3] }
            XCTAssertEqual(alpha, (0..<4).map { selected & (1 << $0) == 0 ? 255 : 0 }, "corners \(selected)")
            XCTAssertEqual(try pixel(image, x: 20, y: 20)[3], 255)
        }
    }

    func testOversizedBorderRadiiReduceBeforeTheInnerEdgeIsInset() throws {
        let box = node(1), effect = try filter()
        for filtered in [false, true] {
            func render(radius: Double) throws -> CGImage {
                var style: NodeStyle = ["background_color": [255,0,0,255], "border_width": 4,
                    "border_color_top": [0,0,255,255], "border_radius_top_left": .number(radius),
                    "border_radius_top_right": .number(radius)]
                if filtered { style["filter"] = effect }
                box.applyStyle(style)
                window.displayIfNeeded()
                if filtered { box.renderFilter(); settle(); return try picture(box) }
                return try XCTUnwrap(Capture.picture(of: XCTUnwrap(box.layer), size: box.bounds.size, scale: 1))
            }
            let reduced = try render(radius: 20), oversized = try render(radius: 999)
            XCTAssertEqual(try pixel(reduced, x: 20, y: 20), [255,0,0,255])
            XCTAssertEqual(try pixel(reduced, x: 20, y: 1), [0,0,255,255])
            var mismatches = 0
            for y in [1,5,10,20,30,35,38] {
                for x in [1,5,10,20,30,35,38] {
                    if try pixel(reduced, x: x, y: y) != pixel(oversized, x: x, y: y) { mismatches += 1 }
                }
            }
            XCTAssertEqual(mismatches, 0, "999-point top corners must paint like 20-point corners; filtered: \(filtered)")
        }
    }

    func testAFilteredBoxKeepsEachRoundedAndSquareCorner() throws {
        let box = node(1), effect = try filter()
        let corners = ["top_left", "top_right", "bottom_right", "bottom_left"]
        for bordered in [false, true] {
            for clipped in [false, true] {
                for selected in 1..<16 {
                    var style: NodeStyle = ["background_color": [255,0,0,255], "filter": effect]
                    if bordered { style["border_width"] = 4; style["border_color_top"] = [0,0,255,255] }
                    if clipped { style["overflow_x"] = "hidden"; style["overflow_y"] = "hidden" }
                    for (i, corner) in corners.enumerated() { style["border_radius_" + corner] = .number(selected & (1 << i) == 0 ? 0 : 16) }
                    box.applyStyle(style)
                    #if os(macOS)
                    window.displayIfNeeded()
                    #endif
                    box.renderFilter(); settle()
                    let image = try picture(box)
                    let alpha = try [(1,1), (38,1), (38,38), (1,38)].map { try pixel(image, x: $0.0, y: $0.1)[3] }
                    XCTAssertEqual(alpha, (0..<4).map { selected & (1 << $0) == 0 ? 255 : 0 }, "corners \(selected), border \(bordered), clip \(clipped)")
                    XCTAssertEqual(try pixel(image, x: 20, y: 20), [255,0,0,255])
                    if bordered { XCTAssertEqual(try pixel(image, x: 20, y: 1), [0,0,255,255]) }
                }
            }
        }
    }

    #endif

    /// A batch draws a box's picture again only when it touched the box or
    /// something in it, or the box's size changed; otherwise the picture
    /// follows the box. A picture after the first is drawn off the main
    /// thread, the old one showing until it lands.
    func testABoxIsDrawnAgainOnlyWhenWhatItShowsChanged() throws {
        let box = node(1)
        _ = node(2, in: box); _ = node(3)
        box.applyStyle(["background_color": [255,0,0,255], "filter": try filter()])
        #if os(macOS)
        window.displayIfNeeded()
        box.renderFilter()
        #endif
        settle()
        let first = try picture(box)
        p.touched(3)
        p.boxFilters.render()
        XCTAssertFalse(p.boxFilters.drawing, "a batch that touched nothing in it draws nothing")
        p.touched(2)
        p.boxFilters.render()
        XCTAssertFalse(p.boxFilters.drawing, "nor does one that touched it and changed nothing it draws")
        box.frame.origin = CGPoint(x: 10, y: 20)
        p.boxFilters.render()
        XCTAssertFalse(p.boxFilters.drawing)
        XCTAssertTrue(try picture(box) === first)
        let moved: CALayer? = box.layer
        XCTAssertEqual(box.boxFilter?.picture.position, moved?.position, "a box that moved is followed")
        let inner: CALayer? = p.views[2]?.layer
        inner?.backgroundColor = CGColor(srgbRed: 1, green: 0, blue: 0, alpha: 1)
        p.touched(2)
        p.boxFilters.render()
        XCTAssertTrue(p.boxFilters.drawing, "a change inside it draws it again, off the main thread")
        XCTAssertTrue(try picture(box) === first, "the old picture shows until the new one lands")
        settle()
        let second = try picture(box)
        XCTAssertFalse(second === first)
        XCTAssertEqual(try pixel(second, x: 20, y: 20), [255,0,0,255])
        box.frame.size = CGSize(width: 50, height: 40)
        p.boxFilters.render()
        settle()
        XCTAssertFalse(try picture(box) === second, "so does a new size")
    }

    // A box's own motion, opacity and `filter` draw nothing of its subtree
    // again; anything else that changed in the subtree since its last
    // drawing, an op's or not, is drawn when a batch next touches the box.
    // The batches below that change a box's `filter` alone stand for a
    // scroll-linked filter, which sends one every frame.

    /// `ops` applied as one batch, and every picture it asks for drawn and
    /// landed (on the Mac a view has its layer from its first display, and
    /// the next batch pictures it).
    private func show(_ ops: [[String: Any]]) {
        p.apply(wireBatch(ops))
        #if os(macOS)
        window.displayIfNeeded()
        p.apply(wireBatch([]))
        #endif
        settle()
    }

    /// A batch changing box `id`'s `filter` alone (`style`: the rest of its
    /// style): its last unfiltered drawing after.
    private func refilter(_ id: UInt32, _ style: [String: Any] = [:], red: Double) throws -> CGImage {
        var style = style
        style["filter"] = ["p": try program(red: red)]
        show([["op": "style", "id": id, "style": style]])
        return try XCTUnwrap(p.views[id]?.boxFilter?.lastDrawing)
    }

    private func drawing(_ id: UInt32) throws -> CGImage { try XCTUnwrap(p.views[id]?.boxFilter?.lastDrawing) }

    private func bytes(_ image: CGImage) throws -> Data { try XCTUnwrap(image.dataProvider?.data as Data?) }

    /// Filtered box 1 at (10, 10), `size` points square, holding box 2
    /// (`inner`'s kind and style) filling it, inside root view 3.
    private func card(_ style: [String: Any] = [:], inner kind: String = "view", _ inner: [String: Any] = [:],
                      props: [String: String] = [:], size: Double = 40) throws {
        var style = style
        style["filter"] = ["p": try program()]
        show([
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "create", "id": 1, "kind": "view", "style": style],
            ["op": "create", "id": 2, "kind": kind, "style": inner, "props": props],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 3, "ids": [1]], ["op": "roots", "ids": [3]],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 200.0, "h": 200.0],
            ["op": "frame", "id": 1, "x": 10.0, "y": 10.0, "w": size, "h": size],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": size, "h": size],
        ])
    }

    func testTheBoxsOwnMotionOpacityAndFilterDrawNothingOfItsSubtreeAgain() throws {
        let red: [String: Any] = ["background_color": [255, 0, 0, 255]]
        try card(red)
        let box = try XCTUnwrap(p.views[1]), first = try drawing(1)
        show([
            ["op": "present", "id": 1, "property": "translate", "x": 40.0, "y": 0.0, "w": 0.0, "h": 0.0],
            ["op": "present", "id": 1, "property": "scale", "x": 0.8, "y": 0.0, "w": 0.0, "h": 0.0],
            ["op": "present", "id": 1, "property": "rotate", "x": 10.0, "y": 0.0, "w": 0.0, "h": 0.0],
            ["op": "present", "id": 1, "property": "opacity", "x": 0.4, "y": 0.0, "w": 0.0, "h": 0.0],
        ])
        XCTAssertTrue(try drawing(1) === first, "its own motion and opacity draw nothing")
        let shown = try XCTUnwrap(box.boxFilter?.picture), moved: CALayer = try XCTUnwrap(box.layer)
        XCTAssertFalse(CATransform3DIsIdentity(shown.transform), "the picture moved")
        XCTAssertTrue(CATransform3DEqualToTransform(shown.transform, moved.transform), "where the box went")
        XCTAssertEqual(shown.opacity, 0.4, accuracy: 0.001)

        XCTAssertTrue(try refilter(1, red, red: 0.5) === first, "nor does a new chain")
        let chained = try pixel(picture(box), x: 20, y: 20)
        XCTAssertEqual(chained[1...], [0, 0, 255], "the new chain over the last drawing")
        XCTAssertTrue((126...129).contains(chained[0]), "red halved: \(chained)")

        show([["op": "style", "id": 2, "style": ["background_color": [0, 255, 0, 255]]]])
        XCTAssertFalse(try drawing(1) === first, "a change inside it draws it again")
        XCTAssertEqual(try pixel(picture(box), x: 20, y: 20), [0, 255, 0, 255])
    }

    /// An animated image's frames change with no batch: each shows in the
    /// picture the next time the box's chain changes. The agent's clock
    /// picks the frame (`AnimatedRasters`).
    func testAnAnimatedImagesNewFrameIsDrawnWhenTheChainNextChanges() throws {
        try card(inner: "image")
        let image = try XCTUnwrap(p.views[2])
        let data = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(data, "com.compuserve.gif" as CFString, 2, nil))
        CGImageDestinationSetProperties(destination, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFLoopCount: 0]] as CFDictionary)
        for color in [CGColor(srgbRed: 1, green: 0, blue: 0, alpha: 1), CGColor(srgbRed: 0, green: 0, blue: 1, alpha: 1)] {
            let ctx = try XCTUnwrap(CGContext(data: nil, width: 4, height: 4, bitsPerComponent: 8, bytesPerRow: 16,
                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            ctx.setFillColor(color)
            ctx.fill(CGRect(x: 0, y: 0, width: 4, height: 4))
            CGImageDestinationAddImage(destination, try XCTUnwrap(ctx.makeImage()),
                [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: 0.1]] as CFDictionary)
        }
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        let session = ExactApp.shared.makeSession(label: "box-filter-animated")
        session.clock = 0
        p.session = session
        let loader = RasterLoader(), resolver = AssetResolver(root: FileManager.default.temporaryDirectory)
        defer { loader.shutdown(); image.raster = nil; p.session = nil; session.destroy() }
        image.loadGeneration = 1
        XCTAssertTrue(loader.load(image, source: "data:image/gif;base64," + (data as Data).base64EncodedString(), resolver: resolver))
        let end = Date(timeIntervalSinceNow: 20)
        while image.raster == nil && Date() < end { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.02)) }
        XCTAssertNotNil(image.raster)
        AnimatedRasters.shared.attach(image)
        var red = 1.0
        for (clock, rgb) in [(50.0, [255, 0, 0]), (150, [0, 0, 255]), (250, [255, 0, 0]), (350, [0, 0, 255])] as [(Double, [UInt8])] {
            session.clock = clock
            AnimatedRasters.shared.evaluate()
            #if os(macOS)
            image.layer?.displayIfNeeded() // a Mac image view takes its frame as its layer displays
            #endif
            red = red == 1 ? 0.99 : 1
            XCTAssertEqual(try pixel(refilter(1, red: red), x: 20, y: 20), rgb + [255], "the frame at \(clock) ms")
        }
    }

    /// A recorded canvas (an animating one, off the GPU) draws each frame
    /// from its lists, which no layer property holds: each frame shows in the
    /// picture the next time the box's chain changes. (A canvas node's Metal
    /// layer leaves its tree with no inputs, so it is drawn every time.)
    func testARecordedCanvasesNewFrameIsDrawnWhenTheChainNextChanges() throws {
        ExactSurfaces.install()
        try card(inner: "canvas")
        let parent: CALayer = try XCTUnwrap(p.views[2]?.layer), host = Canvas2DHost()
        host.place(2, in: CGRect(x: 0, y: 0, width: 40, height: 40), radii: [])
        func list(_ rgb: [Double]) -> Data {
            var d = Data()
            func u32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { d.append(contentsOf: $0) } }
            func f64(_ v: Double) { withUnsafeBytes(of: v.bitPattern.littleEndian) { d.append(contentsOf: $0) } }
            u32(0x4432_4345); u32(1)
            for (op, n) in [(Canvas2DOp.fillColor, rgb + [1]), (.fillRect, [0, 0, 40, 40])] {
                u32(op.rawValue); u32(UInt32(n.count)); n.forEach(f64)
            }
            return d
        }
        func frame(_ rgb: [Double], fresh: Bool = false) {
            host.apply(2, ["lifetime": 1, "generation": 0, "fresh": fresh, "w": 80, "h": 80, "scale": 2.0,
                           "stretch": false, "animating": true, "lists": [list(rgb)]], layer: parent)
            host.waitForReplays(timeout: 5)
        }
        frame([255, 0, 0], fresh: true)
        for _ in 0..<3 { frame([255, 0, 0]) }
        let canvas = try XCTUnwrap(parent.sublayers?.first { $0.sublayers?.contains { $0 is Canvas2DRecordLayer } == true })
        XCTAssertNil(canvas.contents, "recorded, not a bitmap")
        XCTAssertEqual(try pixel(refilter(1, red: 0.99), x: 20, y: 20), [255, 0, 0, 255])
        for (rgb, red) in [([0.0, 0, 255], 1.0), ([0, 255, 0], 0.99), ([255, 0, 0], 1)] {
            frame(rgb)
            XCTAssertEqual(try pixel(refilter(1, red: red), x: 20, y: 20), rgb.map { UInt8($0) } + [255], "the canvas's frame")
        }
    }

    /// A layer that draws through its own `draw(in:)` draws from state no
    /// layer property holds: what it draws now is drawn whenever a batch
    /// touches the box, also before Core Animation has displayed it.
    func testALayerThatDrawsItselfIsDrawnAgain() throws {
        final class Painted: CALayer {
            var color = CGColor(srgbRed: 1, green: 0, blue: 0, alpha: 1)
            override func draw(in ctx: CGContext) { ctx.setFillColor(color); ctx.fill(bounds) }
        }
        try card()
        let painted = Painted()
        painted.frame = CGRect(x: 0, y: 0, width: 40, height: 40)
        try XCTUnwrap(p.views[1]?.layer).addSublayer(painted)
        painted.setNeedsDisplay()
        func batch(_ red: Double) throws {
            p.apply(wireBatch([["op": "style", "id": 1, "style": ["filter": ["p": try program(red: red)]]]]))
        }
        try batch(0.99)
        XCTAssertEqual(try pixel(drawing(1), x: 20, y: 20), [255, 0, 0, 255])
        painted.color = CGColor(srgbRed: 0, green: 0, blue: 1, alpha: 1)
        painted.setNeedsDisplay()
        try batch(1)
        XCTAssertEqual(try pixel(drawing(1), x: 20, y: 20), [0, 0, 255, 255], "what it draws now, in the same turn")
        settle()
        XCTAssertEqual(try pixel(picture(XCTUnwrap(p.views[1])), x: 20, y: 20), [0, 0, 255, 255])
    }

    /// A box in a layout-size transition draws a dashed border through a
    /// closure, which no layer property holds: a new colour shows, also when
    /// it comes before Core Animation has displayed the surface.
    func testADrawnSurfacesNewColourIsDrawn() throws {
        func dashed(_ rgba: [Double], red: Double) throws -> [String: Any] {
            ["border_width": 4, "border_style_top": "dashed", "border_style_right": "dashed", "border_style_bottom": "dashed",
             "border_style_left": "dashed", "border_color_top": rgba, "filter": ["p": try program(red: red)]]
        }
        try card(dashed([255, 0, 0, 255], red: 1))
        p.apply(wireBatch([["op": "present", "id": 1, "property": "layout", "x": 0.0, "y": 0.0, "w": -2.0, "h": 0.0]]))
        XCTAssertNotNil(p.views[1]?.surface, "a surface draws the box")
        p.apply(wireBatch([["op": "style", "id": 1, "style": try dashed([0, 0, 255, 255], red: 0.99)]]))
        show([])
        let image = try drawing(1), w = image.width, h = image.height
        let ctx = try XCTUnwrap(CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
        let px = UnsafeBufferPointer(start: try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self), count: w * h * 4)
        let red = stride(from: 0, to: px.count, by: 4).filter { px[$0] > 200 && px[$0 + 2] < 50 }.count
        let blue = stride(from: 0, to: px.count, by: 4).filter { px[$0 + 2] > 200 && px[$0] < 50 }.count
        XCTAssertEqual(red, 0, "no red border left")
        XCTAssertGreaterThan(blue, 0, "the border's new colour")
    }

    /// Typing and an input method's marked text in a field change its
    /// pixels with no batch: each is drawn the next time the box's chain changes.
    func testTypingAndMarkedTextInAFieldAreDrawnWhenTheChainNextChanges() throws {
        try card(inner: "input", props: ["value": "hello"], size: 120)
        let node = try XCTUnwrap(p.views[2])
        #if os(macOS)
        let field = try XCTUnwrap(node.field)
        window.makeKeyAndOrderFront(nil)
        XCTAssertTrue(window.makeFirstResponder(field))
        let editor = try XCTUnwrap(field.currentEditor() as? NSTextView)
        let type = { (text: String) in editor.insertText(text, replacementRange: editor.selectedRange()) }
        let mark = { (text: String) in
            editor.setMarkedText(text, selectedRange: NSRange(location: (text as NSString).length, length: 0),
                                 replacementRange: NSRange(location: NSNotFound, length: 0))
        }
        #else
        let field = try XCTUnwrap(node.field)
        window.makeKeyAndVisible()
        XCTAssertTrue(field.becomeFirstResponder())
        let type = { (text: String) in field.insertText(text); node.fieldChanged() }
        let mark = { (text: String) in
            field.setMarkedText(text, selectedRange: NSRange(location: (text as NSString).length, length: 0)); node.fieldChanged()
        }
        #endif
        let focused = try refilter(1, red: 0.99)
        type(" world")
        let typed = try refilter(1, red: 1)
        XCTAssertNotEqual(try bytes(typed), try bytes(focused), "the words typed")
        mark("にほん")
        let marked = try refilter(1, red: 0.99)
        XCTAssertNotEqual(try bytes(marked), try bytes(typed), "the marked text")
    }

    /// A text area scrolls its own text with no batch: the scrolled text
    /// is drawn the next time the box's chain changes.
    func testATextAreasOwnScrollIsDrawnWhenTheChainNextChanges() throws {
        let lines = (1...60).map(String.init).joined(separator: "\n")
        try card(inner: "textarea", props: ["value": lines], size: 120)
        let node = try XCTUnwrap(p.views[2])
        let top = try refilter(1, red: 0.99)
        #if os(macOS)
        let scroll = try XCTUnwrap(node.textAreaScroll)
        scroll.contentView.scroll(to: CGPoint(x: 0, y: 300))
        scroll.reflectScrolledClipView(scroll.contentView)
        #else
        try XCTUnwrap(node.textArea).contentOffset = CGPoint(x: 0, y: 300)
        #endif
        let scrolled = try refilter(1, red: 1)
        XCTAssertNotEqual(try bytes(scrolled), try bytes(top), "the text scrolled")
    }

    /// A `background-attachment: fixed` gradient on a filtered box in a
    /// scroller, or on a box in it, is re-aimed as the scroller scrolls,
    /// with no batch: the gradient where it now meets the box is drawn the
    /// next time the box's chain changes, as a scroll-linked filter's does
    /// every frame. (The Mac re-aims a fixed gradient when its own box
    /// paints, which a filtered box does every batch, and not yet as an
    /// inner scroller moves: `NodeView.fixedGradientPort`.)
    func testAFixedGradientUnderAScrollLinkedFilterIsDrawnWhenTheChainNextChanges() throws {
        let gradient: [String: Any] = ["background_image": ["linear": 180, "stops": [0, 255, 0, 0, 255, 1, 0, 0, 255, 255]],
                                       "background_attachment": "fixed"]
        #if os(macOS)
        let placements = [true]
        #else
        let placements = [true, false]
        #endif
        for own in placements {
            show([
                ["op": "create", "id": 3, "kind": "view", "style": ["overflow_y": "scroll"]],
                ["op": "create", "id": 1, "kind": "view", "style": (own ? gradient : [:]).merging(["filter": ["p": try program()]]) { $1 }],
                ["op": "create", "id": 2, "kind": "view", "style": own ? [:] : gradient],
                ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 3, "ids": [1]], ["op": "roots", "ids": [3]],
                ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 200.0, "h": 200.0],
                ["op": "content", "id": 3, "w": 200.0, "h": 1000.0],
                ["op": "frame", "id": 1, "x": 0.0, "y": 100.0, "w": 40.0, "h": 40.0],
                ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 20.0, "h": 40.0],
            ])
            let style = own ? gradient : [:]
            let top = try refilter(1, style, red: 0.99)
            let scroller = try XCTUnwrap(p.views[3])
            #if os(macOS)
            let scroll = try XCTUnwrap(scroller.scroll)
            scroll.contentView.scroll(to: CGPoint(x: 0, y: 100))
            scroll.reflectScrolledClipView(scroll.contentView)
            #else
            try XCTUnwrap(scroller.scroll).contentOffset = CGPoint(x: 0, y: 100)
            #endif
            let scrolled = try refilter(1, style, red: 1)
            XCTAssertFalse(scrolled === top, "drawn again, on \(own ? "the box" : "a box in it")")
            XCTAssertLessThan(try pixel(scrolled, x: 10, y: 20)[2], try pixel(top, x: 10, y: 20)[2],
                              "scrolled up the viewport's gradient, less blue")
            show([["op": "destroy", "id": 2], ["op": "destroy", "id": 1], ["op": "destroy", "id": 3], ["op": "roots", "ids": []]])
        }
    }

    func testAnImageDecodeUpdatesItsFilteredAncestorWithoutABatch() throws {
        let outer = node(3), box = node(1, in: outer), image = node(2, kind: "image", in: box)
        outer.applyStyle(["filter": try filter()])
        box.applyStyle(["filter": try filter()])
        #if os(macOS)
        window.displayIfNeeded()
        #endif
        p.boxFilters.render()
        let before = try picture(box)
        let ctx = try XCTUnwrap(CGContext(data: nil, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.setFillColor(CGColor(srgbRed: 1, green: 0, blue: 0, alpha: 1))
        ctx.fill(CGRect(x: 0, y: 0, width: 1, height: 1))
        let data = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(data, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, try XCTUnwrap(ctx.makeImage()), nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        let source = "data:image/png;base64," + (data as Data).base64EncodedString()
        let loader = RasterLoader(), resolver = AssetResolver(root: FileManager.default.temporaryDirectory)
        defer { loader.shutdown(); image.raster = nil }
        image.loadGeneration = 1
        XCTAssertTrue(loader.load(image, source: source, resolver: resolver))
        let end = Date(timeIntervalSinceNow: 20)
        while image.raster == nil && Date() < end { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.05)) }
        XCTAssertNotNil(image.raster)
        settle()
        let after = try picture(box)
        XCTAssertFalse(before === after)
        XCTAssertEqual(try pixel(after, x: 20, y: 20), [255,0,0,255])
        XCTAssertEqual(try pixel(picture(outer), x: 20, y: 20), [255,0,0,255], "inner pictures publish before their ancestors")
    }
    /// Every picture drawn and landed: a box's first on screen is drawn
    /// in place, any later one off the main thread.
    private func settle() {
        let end = Date(timeIntervalSinceNow: 20)
        repeat { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01)) } while p.boxFilters.drawing && Date() < end
        XCTAssertFalse(p.boxFilters.drawing)
    }

    func testALateTextRasterUpdatesItsFilteredAncestorWithoutABatch() throws {
        let box = node(1), text = node(2, kind: "text", in: box)
        box.frame.size = CGSize(width: 160, height: 120); text.frame = box.bounds
        #if os(macOS)
        text.wantsLayer = true
        window.displayIfNeeded()
        let font = NSFont.systemFont(ofSize: 24), ink = NSColor.red
        #else
        let font = UIFont.systemFont(ofSize: 24), ink = UIColor.red
        #endif
        box.applyStyle(["filter": try filter()])
        box.renderFilter()
        let before = try picture(box)
        let key = TextRasterKey(spec: text.paragraphSpec(), size: text.bounds.size, box: text.bounds, scale: 2)
        let source = NSAttributedString(string: "Late", attributes: [.font: font, .foregroundColor: ink])
        let job = TextRasterJob(source: source, ranges: [CFRange(location: 0, length: 4)], baselines: [28],
            flush: 0, box: text.bounds, size: text.bounds.size, scale: 2)
        let result = try XCTUnwrap(job.render())
        text.textRasterKey = key; text.textRasterReady = false
        #if os(macOS)
        text.showTextRaster(result.surface, for: key, frame: result.frame)
        #else
        text.showTextRaster(result, for: key)
        #endif
        settle()
        let after = try picture(box)
        XCTAssertFalse(before === after)
        let bytes = try XCTUnwrap(after.dataProvider?.data)
        let data = try XCTUnwrap(CFDataGetBytePtr(bytes))
        XCTAssertTrue((0..<after.height).contains { y in
            (0..<after.width).contains { x in data[y * after.bytesPerRow + x * 4 + 3] > 0 }
        }, "the published glyphs are in the filtered picture")
    }

}
