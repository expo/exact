#if os(iOS)
import ImageIO
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1123 D11: an image's background extension is the image's own bottom
/// subview, which draws its picture and the mirror: it grows on the sides
/// where the image meets the route's safe area, composites with the image as
/// one, goes when the image does, and keeps its mirror at the picture's edge
/// while an animation moves them.
final class BackgroundExtensionIOSTests: XCTestCase {
    private var window: UIWindow?
    private var loader: RasterLoader?

    override func tearDown() {
        loader?.shutdown()
        window?.isHidden = true
        window = nil
        loader = nil
        super.tearDown()
    }

    /// A page (1) holding a box (3) that holds an image (2) at `frame`,
    /// `source` or else red, styled `style`, which says
    /// `-exact-background-extension="auto"`, its raster loaded, in a 300 by
    /// 400 route whose safe area is `insets` in from its edges, in `scene`
    /// where one is given.
    private func mount(_ frame: CGRect, insets: UIEdgeInsets, style: [String: Any] = [:], source: Data? = nil, in scene: UIWindowScene? = nil) throws -> (Presenter, RouteController, NodeView) {
        guard #available(iOS 26.0, *) else { throw XCTSkip("the background extension is iOS 26's") }
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let png = try source ?? XCTUnwrap(UIGraphicsImageRenderer(size: CGSize(width: 30, height: 10), format: format).image { context in
            UIColor.red.setFill(); context.fill(CGRect(x: 0, y: 0, width: 30, height: 10))
        }.pngData())
        let p = Presenter(), loader = RasterLoader(), resolver = AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()))
        self.loader = loader
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "create", "id": 2, "kind": "image", "props": ["backgroundExtension": "auto"], "style": style],
            ["op": "children", "id": 1, "ids": [3]],
            ["op": "children", "id": 3, "ids": [2]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 400],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 300, "h": 400],
            ["op": "frame", "id": 2, "x": frame.minX, "y": frame.minY, "w": frame.width, "h": frame.height],
        ]))
        let page = try XCTUnwrap(p.views[1]), image = try XCTUnwrap(p.views[2])
        let route = RouteController(page), window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow()
        window.frame = CGRect(x: 0, y: 0, width: 300, height: 400)
        self.window = window
        window.rootViewController = route
        window.makeKeyAndVisible()
        let own = route.view.safeAreaInsets
        route.additionalSafeAreaInsets = UIEdgeInsets(top: insets.top - own.top, left: insets.left - own.left, bottom: insets.bottom - own.bottom, right: insets.right - own.right)
        route.view.addSubview(page)
        route.view.layoutIfNeeded()
        XCTAssertEqual(route.view.safeAreaInsets, insets)
        image.loadGeneration = 1
        loader.load(image, source: "data:\(source == nil ? "image/png" : "image/gif");base64," + png.base64EncodedString(), resolver: resolver)
        let deadline = Date(timeIntervalSinceNow: 5)
        while image.raster == nil, Date() < deadline { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.02)) }
        XCTAssertNotNil(image.raster)
        image.syncBackgroundExtension()
        return (p, route, image)
    }

    private func extensions(_ route: RouteController) -> [UIView] {
        func all(_ view: UIView) -> [UIView] { view.subviews.flatMap { ($0 is ExtensionView ? [$0] : []) + all($0) } }
        return all(route.view)
    }

    /// The window as the window server draws it: each point's red, green and blue.
    private func pixels() throws -> (CGPoint) -> [Int] {
        let window = try XCTUnwrap(self.window)
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let cg = try XCTUnwrap(UIGraphicsImageRenderer(size: window.bounds.size, format: format).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }.cgImage)
        var data = [UInt8](repeating: 0, count: cg.width * cg.height * 4)
        let context = try XCTUnwrap(CGContext(data: &data, width: cg.width, height: cg.height, bitsPerComponent: 8, bytesPerRow: cg.width * 4,
                                              space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.draw(cg, in: CGRect(x: 0, y: 0, width: cg.width, height: cg.height))
        return { point in let i = (Int(point.y) * cg.width + Int(point.x)) * 4; return [Int(data[i]), Int(data[i + 1]), Int(data[i + 2])] }
    }

    func testAnImagesExtensionGoesWithTheImage() throws {
        let (p, route, _) = try mount(CGRect(x: 0, y: 50, width: 300, height: 100), insets: UIEdgeInsets(top: 50, left: 0, bottom: 0, right: 0))
        XCTAssertEqual(extensions(route).count, 1, "the image meets the safe area's top")
        p.apply(wireBatch([["op": "destroy", "id": 2], ["op": "children", "id": 3, "ids": []]]))
        XCTAssertTrue(extensions(route).isEmpty, "the image is destroyed, and its extension with it")
    }

    /// The extension grows to the route's edge on each side where the image
    /// meets the route's safe area, and on no other: an image short of the
    /// safe area's edge, or on an edge with no safe area, has none there.
    func testTheExtensionGrowsOnTheSidesTheImageMeetsTheSafeArea() throws {
        let insets = UIEdgeInsets(top: 50, left: 40, bottom: 20, right: 30)
        let (p, route, image) = try mount(CGRect(x: 40, y: 50, width: 230, height: 100), insets: insets)
        let cases: [(CGRect, CGRect?, String)] = [
            (CGRect(x: 40, y: 50, width: 230, height: 100), CGRect(x: 0, y: 0, width: 300, height: 150), "top, left and right"),
            (CGRect(x: 40, y: 80, width: 100, height: 100), CGRect(x: 0, y: 80, width: 140, height: 100), "left"),
            (CGRect(x: 100, y: 280, width: 100, height: 100), CGRect(x: 100, y: 280, width: 100, height: 120), "bottom"),
            (CGRect(x: 41, y: 51, width: 100, height: 100), nil, "none: a point short of the top and left"),
        ]
        for (frame, grown, sides) in cases {
            p.apply(wireBatch([["op": "frame", "id": 2, "x": frame.minX, "y": frame.minY, "w": frame.width, "h": frame.height]]))
            image.syncBackgroundExtension()
            let shown = extensions(route)
            XCTAssertEqual(shown.first.map { $0.convert($0.bounds, to: route.view) }, grown, sides)
            XCTAssertLessThanOrEqual(shown.count, 1, sides)
            // The image keeps its place: the extension's content is laid out at it.
            if let content = (shown.first as? ExtensionView)?.content {
                XCTAssertEqual(content.convert(content.bounds, to: route.view), frame, sides)
            }
        }
        // A side with no safe area has nothing to fill, even where the image reaches the route's edge.
        let added = route.additionalSafeAreaInsets
        route.additionalSafeAreaInsets = UIEdgeInsets(top: added.top - insets.top, left: added.left - insets.left, bottom: added.bottom - insets.bottom, right: added.right - insets.right)
        route.view.layoutIfNeeded()
        XCTAssertEqual(route.view.safeAreaInsets, .zero)
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 0, "y": 0, "w": 300, "h": 100]]))
        image.syncBackgroundExtension()
        XCTAssertTrue(extensions(route).isEmpty)
    }

    /// Within the image's box the picture is drawn once: nothing of a square
    /// picture shows behind the image's rounded corners, and a faded image is
    /// not drawn over a copy of itself. The extension's content takes the
    /// image's corners, so what it mirrors is the image as it shows.
    func testWithinTheImagesBoxOnlyTheImageDraws() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the extension's pixels are drawn by the window server")
        }
        let corners: [String: Any] = ["border_radius_top_left": 30, "border_radius_top_right": 30, "border_radius_bottom_right": 30, "border_radius_bottom_left": 30]
        let (p, route, image) = try mount(CGRect(x: 0, y: 50, width: 300, height: 100), insets: UIEdgeInsets(top: 50, left: 0, bottom: 0, right: 0), style: corners, in: scene)
        var at = try pixels()
        let ground = at(CGPoint(x: 150, y: 300)), red = [255, 0, 0]
        XCTAssertNotEqual(ground, red)
        XCTAssertEqual(at(CGPoint(x: 150, y: 25)), red, "the extension fills the safe area above the image")
        XCTAssertEqual(at(CGPoint(x: 150, y: 100)), red, "the image")
        XCTAssertEqual(at(CGPoint(x: 3, y: 53)), ground, "the ground behind the image's corner, not a square picture")
        if let content = (extensions(route).first as? ExtensionView)?.content {
            let mask = (content.layer.mask as? CAShapeLayer)?.path
            XCTAssertEqual(mask, image.roundedPath(in: content.bounds).cgPath, "the extension mirrors the image's corners")
        }
        p.apply(wireBatch([["op": "present", "id": 2, "property": "opacity", "x": 0.5, "y": 0.0, "w": 0.0, "h": 0.0]]))
        at = try pixels()
        let half = zip(red, ground).map { ($0 + $1) / 2 }, shown = at(CGPoint(x: 150, y: 100))
        XCTAssertTrue(zip(shown, half).allSatisfy { abs($0 - $1) <= 2 }, "the image at half its opacity over the ground, once: \(shown), not \(half)")
    }

    /// The extension mirrors the pixels the image shows: with a
    /// `-exact-tint-color`, the tint through the picture's alpha, and only
    /// what the image's content box shows of its picture.
    func testTheExtensionMirrorsTheImagesTintAndClip() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the extension's pixels are drawn by the window server")
        }
        let green: [String: Any] = ["tint_color": [0, 255, 0, 255]]
        let (p, route, image) = try mount(CGRect(x: 0, y: 50, width: 300, height: 100), insets: UIEdgeInsets(top: 50, left: 0, bottom: 0, right: 0), style: green, in: scene)
        var at = try pixels()
        let ground = at(CGPoint(x: 150, y: 300))
        XCTAssertEqual(at(CGPoint(x: 150, y: 100)), [0, 255, 0], "the image, tinted")
        XCTAssertEqual(at(CGPoint(x: 150, y: 25)), [0, 255, 0], "its extension, tinted")
        // Untinted and covering a content box 40 points inside the image's
        // top and bottom, the picture overflows into the padding, where the
        // image shows the ground.
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["object_fit": "cover"]]]))
        image.applyKernelContentBox(CGRect(x: 0, y: 40, width: 300, height: 20), padding: [40, 0, 40, 0])
        image.syncBackgroundExtension()
        at = try pixels()
        XCTAssertEqual(at(CGPoint(x: 150, y: 100)), [255, 0, 0], "the content box")
        XCTAssertEqual(at(CGPoint(x: 150, y: 55)), ground, "the padding")
        let mirrored = at(CGPoint(x: 150, y: 45))
        let near = { (a: [Int], b: [Int]) in zip(a, b).map { abs($0 - $1) }.reduce(0, +) }
        XCTAssertLessThan(near(mirrored, ground), near(mirrored, [255, 0, 0]), "the extension mirrors the padding: \(mirrored)")
        XCTAssertEqual(extensions(route).count, 1)
    }

    /// An animated image's extension shows the frame the image shows.
    func testAnAnimatedImagesExtensionFollowsItsFrame() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the extension's pixels are drawn by the window server")
        }
        // Red for 50 ms, then blue for a minute.
        let gif = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(gif as CFMutableData, "com.compuserve.gif" as CFString, 2, nil))
        CGImageDestinationSetProperties(destination, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFLoopCount: 0]] as CFDictionary)
        for (color, delay) in [(UIColor.red, 0.05), (UIColor.blue, 60.0)] {
            let format = UIGraphicsImageRendererFormat(); format.scale = 1
            let frame = try XCTUnwrap(UIGraphicsImageRenderer(size: CGSize(width: 30, height: 10), format: format).image { context in
                color.setFill(); context.fill(CGRect(x: 0, y: 0, width: 30, height: 10))
            }.cgImage)
            CGImageDestinationAddImage(destination, frame, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: delay, kCGImagePropertyGIFUnclampedDelayTime: delay]] as CFDictionary)
        }
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        let (_, _, image) = try mount(CGRect(x: 0, y: 50, width: 300, height: 100), insets: UIEdgeInsets(top: 50, left: 0, bottom: 0, right: 0),
                                      source: gif as Data, in: scene)
        XCTAssertNotNil(image.imageLayer, "the image's pixels are a layer's, which its player sets")
        let deadline = Date(timeIntervalSinceNow: 10)
        while AnimatedRasters.shared.frame(for: image) == nil, Date() < deadline { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.05)) }
        XCTAssertNotNil(AnimatedRasters.shared.frame(for: image), "the second frame shows")
        let at = try pixels()
        XCTAssertEqual(at(CGPoint(x: 150, y: 100)), [0, 0, 255], "the image's second frame")
        XCTAssertEqual(at(CGPoint(x: 150, y: 25)), [0, 0, 255], "its extension's")
    }

    /// A column's animation moves the image's box, and each mirror meets the box's edge at every frame of it, as one
    /// animation moves both: the box's left edge moves from the safe area's old edge to its new one while the mirror
    /// fills what is left of it. UIKit's own extension moved its mirror on another clock, two frames behind the box,
    /// and the page showed through the gap or the mirror covered the picture by up to 80 points.
    func testTheMirrorsMeetTheBoxAtEveryFrameOfAnAnimation() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the presentation tree is the render server's, which needs a window in a scene")
        }
        let (p, route, image) = try mount(CGRect(x: 40, y: 50, width: 230, height: 100), insets: UIEdgeInsets(top: 0, left: 40, bottom: 0, right: 30), in: scene)
        let view = try XCTUnwrap(extensions(route).first as? ExtensionView)
        UIView.animate(withDuration: 0.6, delay: 0, options: [.curveEaseInOut]) {
            route.additionalSafeAreaInsets.left += 80
            route.view.layoutIfNeeded()
            p.apply(wireBatch([["op": "frame", "id": 2, "x": 120, "y": 50, "w": 150, "h": 100]]))
            image.syncBackgroundExtension()
        }
        // Each mirror's edge against the box's, on screen, as the render server draws them
        func edges() -> (box: CGRect, left: CGFloat?, right: CGFloat?) {
            let shown = { (layer: CALayer) -> CGRect in let pres = layer.presentation() ?? layer; return pres.convert(pres.bounds, to: nil) }
            let mirrors = (view.layer.sublayers ?? []).filter { $0.transform.m11 < 0 }.map(shown)
            let box = shown(view.content.layer)
            return (box, mirrors.first { $0.midX < box.midX }?.maxX, mirrors.first { $0.midX > box.midX }?.minX)
        }
        var seen: [CGFloat] = []
        let end = Date(timeIntervalSinceNow: 0.7)
        while Date() < end {
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.03))
            let (box, left, right) = edges()
            seen.append(box.minX)
            XCTAssertEqual(try XCTUnwrap(left, "a mirror on the left"), box.minX, accuracy: 0.01, "the left mirror at the box's edge, the box at \(box.minX)")
            XCTAssertEqual(try XCTUnwrap(right, "a mirror on the right"), box.maxX, accuracy: 0.01, "the right mirror at the box's edge")
        }
        XCTAssertTrue(seen.contains { $0 > 41 && $0 < 119 }, "the box was seen between its two places: \(seen)")
    }

    /// The pieces of a mirror's sharp level on screen, nearest the box's edge first: `presented`, as the render
    /// server draws them now
    private func pieces(_ view: ExtensionView, left: Bool, presented: Bool = false) throws -> [CGRect] {
        let shown = { (layer: CALayer) -> CGRect in let l = presented ? layer.presentation() ?? layer : layer; return l.convert(l.bounds, to: nil) }
        let box = shown(view.content.layer)
        let mirror = try XCTUnwrap((view.layer.sublayers ?? []).filter { $0.transform.m11 < 0 }.first { (shown($0).midX < box.midX) == left })
        let sharp = try XCTUnwrap(mirror.sublayers?.first?.sublayers)
        return sharp.map(shown).sorted { left ? $0.maxX > $1.maxX : $0.minX < $1.minX }
    }

    /// A mirror that reaches further past the box than the box's own width is stretched to the route's edge as
    /// UIKit's own is (SwiftUI's mesh, read on iOS 27.1): the point a fraction `u` of the box's width from its edge is
    /// drawn `u + k·u³` widths from it, `k` the reach in widths less one, sampled at tenths. A copy of the box alone
    /// left the page showing between it and the route's edge (beside the iPhone Duo's fold, under a sidebar).
    func testAMirrorWiderThanItsBoxIsStretchedToTheEdgeAsUIKitStretchesIt() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the extension's pixels are drawn by the window server")
        }
        let (_, route, _) = try mount(CGRect(x: 200, y: 50, width: 60, height: 100), insets: UIEdgeInsets(top: 0, left: 200, bottom: 0, right: 40), in: scene)
        let view = try XCTUnwrap(extensions(route).first as? ExtensionView)
        let left = try pieces(view, left: true), k: CGFloat = 200.0 / 60 - 1
        XCTAssertEqual(left.count, 10, "ten pieces, as UIKit's mesh has")
        for (j, piece) in left.enumerated() {
            let u0 = CGFloat(j) / 10, u1 = CGFloat(j + 1) / 10
            XCTAssertEqual(piece.maxX, 200 - 60 * (u0 + k * u0 * u0 * u0), accuracy: 0.01, "piece \(j)'s side nearer the box")
            XCTAssertEqual(piece.minX, 200 - 60 * (u1 + k * u1 * u1 * u1), accuracy: 0.01, "piece \(j)'s further side")
        }
        XCTAssertEqual(left.last?.minX ?? 200, 0, accuracy: 0.01, "the mirror's far side at the route's edge")
        XCTAssertEqual(try pieces(view, left: false).count, 1, "the right side reaches 40 points past a box 60 wide: a mirror of the box, unstretched")
        let at = try pixels(), page = (0..<200).first { at(CGPoint(x: $0, y: 100)) != [255, 0, 0] }
        XCTAssertNil(page, "the mirror from the route's edge to the box, no page between its pieces: \(page.map { "\($0): \(at(CGPoint(x: $0, y: 100)))" } ?? "")")
    }

    /// A mirror once stretched keeps its pieces: a placement while an animation still moves them, as UIKit's fold
    /// places the image more than once inside its animation, finds them where they are rather than making new ones
    /// at the end, which would jump the mirror from its stretch to none.
    func testAStretchedMirrorKeepsItsPiecesThroughAnAnimationThatEndsUnstretched() throws {
        let (p, route, image) = try mount(CGRect(x: 200, y: 50, width: 60, height: 100), insets: UIEdgeInsets(top: 0, left: 200, bottom: 0, right: 40))
        let view = try XCTUnwrap(extensions(route).first as? ExtensionView)
        let stretched = try XCTUnwrap(view.layer.sublayers?.first { $0.transform.m11 < 0 && $0.frame.midX < 200 }?.sublayers?.first?.sublayers)
        UIView.animate(withDuration: 0.6) {
            route.additionalSafeAreaInsets.left -= 160
            route.view.layoutIfNeeded()
            p.apply(wireBatch([["op": "frame", "id": 2, "x": 40, "y": 50, "w": 220, "h": 100]]))
            image.syncBackgroundExtension()
        }
        image.syncBackgroundExtension()
        let after = try XCTUnwrap(view.layer.sublayers?.first { $0.transform.m11 < 0 && $0.frame.midX < 40 }?.sublayers?.first?.sublayers)
        XCTAssertEqual(after.count, 10)
        XCTAssertTrue(zip(stretched, after).allSatisfy { $0 === $1 }, "the same pieces, still moving")
    }

    /// Folding the iPhone Duo moves the box beside the fold, where the sidebar reaches further than the box is wide:
    /// at every frame of the one animation, the stretched mirror meets the box's edge and reaches the route's.
    func testAMirrorStretchingInAnAnimationMeetsTheBoxAndTheEdgeAtEveryFrame() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the presentation tree is the render server's, which needs a window in a scene")
        }
        let (p, route, image) = try mount(CGRect(x: 40, y: 50, width: 230, height: 100), insets: UIEdgeInsets(top: 0, left: 40, bottom: 0, right: 30), in: scene)
        let view = try XCTUnwrap(extensions(route).first as? ExtensionView)
        XCTAssertEqual(try pieces(view, left: true).count, 1, "40 points past a box 230 wide: unstretched")
        UIView.animate(withDuration: 0.6, delay: 0, options: [.curveEaseInOut]) {
            route.additionalSafeAreaInsets.left += 160
            route.view.layoutIfNeeded()
            p.apply(wireBatch([["op": "frame", "id": 2, "x": 200, "y": 50, "w": 70, "h": 100]]))
            image.syncBackgroundExtension()
        }
        var seen: [CGFloat] = []
        let end = Date(timeIntervalSinceNow: 0.7)
        while Date() < end {
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.03))
            let presented = view.content.layer.presentation() ?? view.content.layer
            let box = presented.convert(presented.bounds, to: nil)
            let left = try pieces(view, left: true, presented: true)
            seen.append(box.minX)
            XCTAssertEqual(left.count, 10)
            XCTAssertEqual(try XCTUnwrap(left.first).maxX, box.minX, accuracy: 0.01, "the mirror at the box's edge, the box at \(box.minX)")
            XCTAssertLessThanOrEqual(try XCTUnwrap(left.map(\.minX).min()), 0.01, "the mirror reaching the route's edge, the box at \(box.minX)")
        }
        XCTAssertTrue(seen.contains { $0 > 41 && $0 < 199 }, "the box was seen between its two places: \(seen)")
        XCTAssertEqual(try XCTUnwrap(pieces(view, left: true).map(\.minX).min()), 0, accuracy: 0.01, "at the end, the mirror's far side at the route's edge")
    }

    /// UIKit gives a route its new safe area before the kernel lays the image out for it (folding the iPhone Duo):
    /// until then the extension keeps reaching the route's edge on the sides it reached, so nothing between the old
    /// box and the edge goes blank, to slide back in when the layout lands. The box's new layout decides its sides.
    func testTheExtensionKeepsItsSidesUntilTheBoxIsLaidOutAgain() throws {
        let (p, route, image) = try mount(CGRect(x: 0, y: 50, width: 270, height: 100), insets: UIEdgeInsets(top: 0, left: 0, bottom: 0, right: 30))
        let reach = { self.extensions(route).first.map { $0.convert($0.bounds, to: route.view) } }
        XCTAssertEqual(reach(), CGRect(x: 0, y: 50, width: 300, height: 100), "the image meets the safe area's right edge, its extension the route's")
        route.additionalSafeAreaInsets.right += 20
        route.view.layoutIfNeeded()
        image.syncBackgroundExtension()
        XCTAssertEqual(reach(), CGRect(x: 0, y: 50, width: 300, height: 100), "laid out for the old safe area, the image keeps its extension to the edge")
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 0, "y": 50, "w": 250, "h": 100]]))
        image.syncBackgroundExtension()
        XCTAssertEqual(reach(), CGRect(x: 0, y: 50, width: 300, height: 100), "laid out for the new one, it meets it")
        p.apply(wireBatch([["op": "frame", "id": 2, "x": 0, "y": 50, "w": 200, "h": 100]]))
        image.syncBackgroundExtension()
        XCTAssertTrue(extensions(route).isEmpty, "laid out short of the safe area, it has none")
    }

    /// The extension is in the image's subtree, so it composites with the
    /// image as one: faded with the image and its ancestors, its mirror at the
    /// image's opacity, drawn once; an ancestor that is `display: none` hides
    /// it with the image.
    func testTheExtensionCompositesWithTheImageAsOne() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the extension's pixels are drawn by the window server")
        }
        let (p, route, image) = try mount(CGRect(x: 0, y: 50, width: 300, height: 100), insets: UIEdgeInsets(top: 50, left: 0, bottom: 0, right: 0), in: scene)
        let view = try XCTUnwrap(extensions(route).first)
        XCTAssertTrue(view.isDescendant(of: image), "the image's own subview")
        XCTAssertTrue(image.imageLayer.map { $0.isHidden } ?? true, "the extension draws the picture; the image's layer does not draw it again")
        func present(_ id: Int, _ opacity: Double) {
            p.apply(wireBatch([["op": "present", "id": id, "property": "opacity", "x": opacity, "y": 0.0, "w": 0.0, "h": 0.0]]))
        }
        var at = try pixels()
        let ground = at(CGPoint(x: 150, y: 300)), red = [255, 0, 0]
        present(3, 0.5)
        at = try pixels()
        let half = zip(red, ground).map { ($0 + $1) / 2 }
        for (point, what) in [(CGPoint(x: 150, y: 25), "the mirror"), (CGPoint(x: 150, y: 100), "the picture")] {
            let shown = at(point)
            XCTAssertTrue(zip(shown, half).allSatisfy { abs($0 - $1) <= 2 }, "\(what) at its box's opacity, once: \(shown), not \(half)")
        }
        present(3, 1)
        p.apply(wireBatch([["op": "style", "id": 3, "style": ["display": "none"]]]))
        XCTAssertTrue(sequence(first: view, next: \.superview).contains { $0.isHidden } || view.window == nil, "the box is not displayed")
        p.apply(wireBatch([["op": "style", "id": 3, "style": [:]]]))
        XCTAssertTrue(extensions(route).first.map { ext in !sequence(first: ext, next: \.superview).contains { $0.isHidden } } ?? false, "the box is displayed again")
    }
}
#endif
