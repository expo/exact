import XCTest
@testable import ExactKit
@testable import ExactSurfaces
#if os(iOS)
import UIKit
#else
import AppKit
#endif

class OverflowClipTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() }
    private func style(_ x: Bool) -> NodeStyle {
        ["overflow_x": x ? "clip" : "visible", "overflow_y": x ? "visible" : "clip",
         "border_radius": 16, "border_width": 2, "border_color_top": [0, 0, 255, 255]]
    }

    func testOneAxisClipsPaintAndHitsWithOverlaysAndMaterials() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        for kind in ["view", "canvas", "ultra-thin", "glass"] {
            for x in [true, false] {
                let p = Presenter()
                let root = NodeView(id: 1, kind: "view", presenter: p)
                root.frame = CGRect(x: 0, y: 0, width: 200, height: 200)
                root.applyStyle(["background_color": [255, 255, 255, 255]])
                #if os(iOS)
                let window = UIWindow(frame: root.bounds)
                window.addSubview(root)
                window.makeKeyAndVisible()
                defer { window.isHidden = true }
                #else
                let window = NSWindow(contentRect: root.bounds, styleMask: [.borderless], backing: .buffered, defer: false)
                window.isReleasedWhenClosed = false
                window.contentView = root
                defer { window.close() }
                #endif
                let n = NodeView(id: 2, kind: kind == "canvas" ? "canvas" : "view", presenter: p)
                n.frame = CGRect(x: 50, y: 50, width: 40, height: 40)
                root.addSubview(n)
                if ["ultra-thin", "glass"].contains(kind) {
                    n.props["backgroundMaterial"] = kind
                    n.updateMaterial()
                }
                let child = NodeView(id: 3, kind: "view", presenter: p)
                child.frame = CGRect(x: -30, y: -30, width: 140, height: 140)
                child.applyStyle(["background_color": [255, 0, 0, 255]])
                n.container.addSubview(child)
                n.applyStyle(style(x))
                let box = try XCTUnwrap(n.clipBox, "\(kind), clip x: \(x)")
                XCTAssertFalse(n.clipsToBounds)
                XCTAssertTrue(child.isDescendant(of: box))
                if let overlay = n.overlay {
                    XCTAssertTrue(n.container === overlay)
                    XCTAssertTrue(child.canvasAbove === n)
                    XCTAssertTrue(overlay.canvasOverlayOwner === n)
                }
                if let material = n.materialView { XCTAssertFalse(child.isDescendant(of: material)) }
                for size: CGFloat in [40, 70] {
                    n.frame.size = CGSize(width: size, height: size)
                    #if os(iOS)
                    root.layoutIfNeeded()
                    #else
                    root.layoutSubtreeIfNeeded()
                    #endif
                    let visible = x ? CGPoint(x: 60, y: 30) : CGPoint(x: 30, y: 60)
                    let clipped = x ? CGPoint(x: 30, y: 60) : CGPoint(x: 60, y: 30)
                    XCTAssertTrue(hit(root, visible) === child, "\(kind), clip x: \(x)")
                    XCTAssertFalse(hit(root, clipped) === child)
                    XCTAssertTrue(hit(root, CGPoint(x: 53, y: 53)) === child, "one-axis clips have square corners")
                    XCTAssertFalse(hit(root, CGPoint(x: 51, y: 51)) === child, "the border is outside the clip")
                    XCTAssertTrue(hit(root, x ? CGPoint(x: 51, y: 70) : CGPoint(x: 70, y: 51)) === n,
                                  "the border remains the node's own hit")
                    XCTAssertEqual(x ? box.frame.width : box.frame.height, size - 4)
                    let image = try pixels(root)
                    XCTAssertEqual(image(visible), [255, 0, 0, 255], "\(kind), clip x: \(x)")
                    XCTAssertEqual(image(clipped), [255, 255, 255, 255])
                    XCTAssertEqual(image(CGPoint(x: 53, y: 53)), [255, 0, 0, 255])
                    XCTAssertEqual(image(CGPoint(x: 51, y: 51)), [255, 255, 255, 255])
                }
                if n.overlay != nil {
                    child.placement = [1, 0, 7, 0, 1, 13, 0, 0, 1, 0]
                    XCTAssertEqual(child.drawnPoint(.zero, in: root), CGPoint(x: 57, y: 63))
                    XCTAssertEqual(child.drawnRect(child.bounds, in: root), CGRect(x: 57, y: 63, width: 140, height: 140))
                    child.placement = [1, 0, -30, 0, 1, -30, 0, 0, 1, 0]
                    let visible = x ? CGPoint(x: 60, y: 30) : CGPoint(x: 30, y: 60)
                    XCTAssertTrue(hit(root, visible) === child)
                    XCTAssertFalse(hit(root, CGPoint(x: 51, y: 51)) === child)
                    let miss = x ? CGPoint(x: 60, y: 100_000) : CGPoint(x: 100_000, y: 60)
                    XCTAssertNil(hit(n, n.convert(miss, from: root)), "a canvas miss outside its box passes through")
                    XCTAssertEqual(child.local(root.convert(visible, to: nil)), child.convert(visible, from: root))
                    child.placement = nil
                }
                let frame = child.frame
                n.applyStyle(style(!x))
                XCTAssertTrue(n.clipBox === box)
                XCTAssertEqual(child.frame, frame)
                let visible = x ? CGPoint(x: 30, y: 60) : CGPoint(x: 60, y: 30)
                XCTAssertTrue(hit(root, visible) === child)
                XCTAssertEqual(try pixels(root)(visible), [255, 0, 0, 255])
                n.applyStyle([:])
                XCTAssertNil(n.clipBox)
                XCTAssertTrue(child.superview === n.container)
                XCTAssertEqual(child.frame, frame)
                if let overlay = n.overlay { XCTAssertTrue(overlay.superview === n) }
                n.applyStyle(style(x))
                XCTAssertNotNil(n.clipBox)
                XCTAssertTrue(child.superview === n.container)
                XCTAssertEqual(child.frame, frame)
                if let material = n.materialView {
                    var both = style(x)
                    both["overflow_x"] = "hidden"; both["overflow_y"] = "hidden"
                    both["box_shadow"] = [["o": [0, 2], "b": 4, "s": 0, "c": [0, 0, 0, 128]]]
                    n.applyStyle(both)
                    XCTAssertTrue(child.isDescendant(of: material), "both-axis clipping keeps native material hosting")
                    n.applyStyle(style(x))
                    XCTAssertFalse(child.isDescendant(of: material))
                    XCTAssertTrue(child.superview === n.container)
                }
            }
        }
    }

    func testNativeAnimationExtentsKeepVisibleOverflowPainted() throws {
        for x in [true, false] {
            try withClip(x) { p, root, node, child in
                XCTAssertEqual(try pixels(root)(CGPoint(x: 60, y: 60)), [255, 0, 0, 255], "static control")
                let key = x ? "transform.translation.y" : "transform.translation.x"
                p.svg.animations(child.id, ["specs": [["id": "move", "k": key, "s": 0.0, "d": 1.0,
                    "n": 1.0, "fill": 3, "t": [0.0, 1.0], "v": [0.0, 100.0]]]], layer: child.layer, clock: 0)
                let box = try XCTUnwrap(node.clipBox)
                XCTAssertGreaterThanOrEqual(x ? box.bounds.maxY : box.bounds.maxX, 120)
                for clock in [250.0, 750.0] {
                    p.svg.seek(clock: clock)
                    CATransaction.flush()
                    let end = Date(timeIntervalSinceNow: 2)
                    func position() -> Double? {
                        #if os(iOS)
                        child.layer.presentation()?.value(forKeyPath: key) as? Double
                        #else
                        child.layer?.presentation()?.value(forKeyPath: key) as? Double
                        #endif
                    }
                    while abs((position() ?? -1) - clock / 10) > 0.001, Date() < end {
                        RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.001))
                    }
                    XCTAssertEqual(try XCTUnwrap(position()), clock / 10, accuracy: 0.001)
                    let point = x ? CGPoint(x: 60, y: 60 + clock / 10) : CGPoint(x: 60 + clock / 10, y: 60)
                    XCTAssertEqual(try pixels(root)(point), [255, 0, 0, 255])
                }
                p.svg.animations(child.id, ["specs": [["id": "move", "k": key, "s": 0.0, "d": 1.0,
                    "n": 1.0, "fill": 3, "t": [0.0, 1.0], "v": [-100_000.0, 200_000.0]]]], layer: child.layer, clock: 0)
                XCTAssertLessThanOrEqual(x ? box.bounds.minY : box.bounds.minX, -100_000)
                XCTAssertGreaterThanOrEqual(x ? box.bounds.maxY : box.bounds.maxX, 200_020)
            }
        }
    }

    func testSceneOnlyUpdatesRefreshVisibleOverflow() throws {
        for x in [true, false] {
            try withClip(x) { p, root, node, child in
                child.applyStyle([:])
                let frame = child.frame
                for distance in [0, 80, -40] {
                    let dx = x ? 4 : distance, dy = x ? distance : 4
                    let shape: [String: Any] = ["id": 1, "n": 3, "o": 1,
                        "p": [0, dx, dy, 1, dx + 16, dy, 1, dx + 16, dy + 16, 1, dx, dy + 16, 3],
                        "f": [255, 0, 0, 255], "w": 0]
                    p.svg.scene(child.id, ["scene": ["box": [0, 0, 20, 20], "t": [1, 0, 0, 1, 0, 0], "els": [shape]]],
                                layer: child.layer, dark: false, clock: 0)
                    let point = CGPoint(x: 50 + dx + 8, y: 50 + dy + 8)
                    XCTAssertEqual(child.frame, frame)
                    XCTAssertTrue(try XCTUnwrap(node.clipBox).bounds.contains(node.convert(point, from: root)))
                    XCTAssertEqual(try pixels(root)(point), [255, 0, 0, 255])
                }
            }
        }
    }

    func testConsecutiveSurfaceResizesRefreshAncestorClips() throws {
        for x in [true, false] {
            try withClip(x) { _, root, node, child in
                child.frame.origin = x ? CGPoint(x: 4, y: 50) : CGPoint(x: 50, y: 4)
                // The intended 40-point surface overshoots to 45 before settling.
                for resize: CGFloat in [10, 20, 25, 15, 0] {
                    child.layoutResize = x ? CGSize(width: 0, height: resize) : CGSize(width: resize, height: 0)
                    child.applyTransform()
                    child.applySurface()
                    let point = x ? CGPoint(x: 60, y: 100 + 20 + resize - 3) : CGPoint(x: 100 + 20 + resize - 3, y: 60)
                    XCTAssertTrue(try XCTUnwrap(node.clipBox).bounds.contains(node.convert(point, from: root)))
                    XCTAssertEqual(try pixels(root)(point), [255, 0, 0, 255], "surface resize: \(resize)")
                }
            }
        }
    }

    func testDropShadowExtendsTheClipAndRefreshesWithItsFilter() throws {
        for x in [true, false] {
            try withClip(x) { _, root, node, child in
                for distance in [80.0, -40.0] {
                    let dx = x ? 0 : distance, dy = x ? distance : 0
                    let region = [-3 + min(dx, 0), -3 + min(dy, 0), 6 + abs(dx), 6 + abs(dy)]
                    let program = region + [1, 6, -1, -3, -1e6, -1e6, 2e6, 2e6, 0, 1, 1, dx, dy, 1, 0, 0, 1]
                    child.applyStyle(["background_color": [0, 0, 255, 255],
                        "filter": .object(["p": .array(program.map(BatchValue.number))])])
                    child.renderFilter()
                    let point = x ? CGPoint(x: 60, y: 60 + distance) : CGPoint(x: 60 + distance, y: 60)
                    XCTAssertTrue(try XCTUnwrap(node.clipBox).bounds.contains(node.convert(point, from: root)))
                }
                child.frame.origin = x ? CGPoint(x: 0, y: 20) : CGPoint(x: 20, y: 0)
                for sigma in [12.0, 24.0] {
                    let region = [-3 * sigma, -3 * sigma, 6 * sigma, 6 * sigma]
                    let program = region + [1, 6, -1, -3, -1e6, -1e6, 2e6, 2e6, 0, sigma, sigma, 0, 0, 1, 0, 0, 1]
                    child.applyStyle(["background_color": [0, 0, 255, 255],
                        "filter": .object(["p": .array(program.map(BatchValue.number))])])
                    child.renderFilter()
                    let point = x ? CGPoint(x: 60, y: 100) : CGPoint(x: 100, y: 60)
                    node.applyStyle([:])
                    let reference = try pixels(root)(point)
                    XCTAssertEqual(reference[0], 255)
                    XCTAssertLessThan(reference[2], 250, "the unclipped control contains a red shadow")
                    node.applyStyle(style(x))
                    XCTAssertEqual(try pixels(root)(point), reference, "clip x: \(x), blur: \(sigma)")
                }
            }
        }
    }

    func testLayoutResizeClipsOnlyTheAuthoredAxis() throws {
        for x in [true, false] {
            try withClip(x) { _, root, node, child in
                child.frame = CGRect(x: -30, y: -30, width: 140, height: 140)
                for resize: CGFloat in [20, -10] {
                    node.layoutResize = CGSize(width: resize, height: resize)
                    node.applySurface()
                    let point = x ? CGPoint(x: 60, y: 30) : CGPoint(x: 30, y: 60)
                    XCTAssertTrue(hit(root, point) === child)
                    XCTAssertEqual(try pixels(root)(point), [255, 0, 0, 255])
                    let edge = x ? CGPoint(x: 87 + resize, y: 30) : CGPoint(x: 30, y: 87 + resize)
                    XCTAssertTrue(hit(root, edge) === child)
                    XCTAssertEqual(try pixels(root)(edge), [255, 0, 0, 255])
                    let clipped = x ? CGPoint(x: 91 + resize, y: 60) : CGPoint(x: 60, y: 91 + resize)
                    XCTAssertFalse(hit(root, clipped) === child)
                    XCTAssertEqual(try pixels(root)(clipped), [255, 255, 255, 255])
                }
                node.layoutResize = .zero
                node.applySurface()
                XCTAssertEqual(try pixels(root)(x ? CGPoint(x: 60, y: 30) : CGPoint(x: 30, y: 60)), [255, 0, 0, 255])
            }
        }
    }

    func testAgentReportsOverflowClippingOnOnlyTheAuthoredAxis() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let session = ExactApp.shared.makeSession(label: "overflow-visibility")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("SURFACE_PAIRS"), size: CGSize(width: 200, height: 200)).error)
        let p = session.presenter
        #if os(iOS)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 200, height: 200))
        window.addSubview(p.viewport); window.makeKeyAndVisible()
        defer { window.isHidden = true }
        #else
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 200, height: 200), styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.contentView = p.viewport
        defer { window.close() }
        #endif
        let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "plain" })
        let child = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "grouped" })
        node.container.addSubview(child)
        node.frame = CGRect(x: 50, y: 50, width: 40, height: 40)
        let agent = Agent(session: session)
        for x in [true, false] {
            node.applyStyle(style(x))
            for outsideClippedAxis in [false, true] {
                let offset = x != outsideClippedAxis ? CGPoint(x: 10, y: 80) : CGPoint(x: 80, y: 10)
                child.frame = CGRect(origin: offset, size: CGSize(width: 20, height: 20))
                let reply = agent.layout(["id": Int(child.id)])
                let detail = try XCTUnwrap(reply["node"] as? [String: Any], "\(reply)")
                let visible = try XCTUnwrap(detail["visible"] as? [String: Any])
                XCTAssertEqual(visible["clipped"] as? Bool, outsideClippedAxis)
            }
        }
    }

    /// A child's hit slop past the node on the visible axis takes the touch;
    /// past the clipped edge it does not.
    func testHitSlopReachesPastTheNodeOnlyOnTheVisibleAxis() throws {
        for x in [true, false] {
            try withClip(x) { _, root, _, child in
                child.frame = x ? CGRect(x: 0, y: 20, width: 20, height: 20) : CGRect(x: 20, y: 0, width: 20, height: 20)
                child.applyStyle(["background_color": [255, 0, 0, 255], "hit_slop_top": 10, "hit_slop_right": 10,
                                  "hit_slop_bottom": 10, "hit_slop_left": 10])
                let visible = x ? CGPoint(x: 60, y: 95) : CGPoint(x: 95, y: 60)
                XCTAssertTrue(hit(root, visible) === child, "visible axis, clip x: \(x)")
                child.frame = x ? CGRect(x: 20, y: 0, width: 20, height: 20) : CGRect(x: 0, y: 20, width: 20, height: 20)
                let clipped = x ? CGPoint(x: 95, y: 60) : CGPoint(x: 60, y: 95)
                XCTAssertFalse(hit(root, clipped) === child, "clipped axis, clip x: \(x)")
            }
        }
    }

    /// A stroke's miter reaches past its path's bounds by up to half its
    /// width times the miter limit.
    func testStrokeMitersExtendTheVisibleAxis() throws {
        for x in [true, false] {
            for animated in [false, true] {
                try withClip(x) { p, root, node, child in
                    child.applyStyle([:])
                    child.frame = x ? CGRect(x: 0, y: 0, width: 20, height: 40) : CGRect(x: 0, y: 0, width: 40, height: 20)
                    // A spike toward +y (or +x): its vertex is at 100, its
                    // miter tip at 150, and the stroke 4 points wide at 130.
                    let spike: [Double] = x ? [0, 0, 0, 1, 10, 100, 1, 20, 0] : [0, 0, 0, 1, 100, 10, 1, 0, 20]
                    var shape: [String: Any] = ["id": 1, "n": 3, "o": 1, "p": animated ? [0, 0, 0] : spike,
                        "s": [255, 0, 0, 255], "w": 10, "ml": 20]
                    // Animated, the model path is a point and the track holds the spike.
                    if animated {
                        shape["a"] = [["id": "spike", "k": "path", "s": 0.0, "d": 1.0, "n": 1.0, "fill": 3,
                                       "t": [0.0, 1.0], "v": [spike, spike]]]
                    }
                    p.svg.scene(child.id, ["scene": ["box": [0, 0, 200, 200], "t": [1, 0, 0, 1, 0, 0], "els": [shape]]],
                                layer: child.layer, dark: false, clock: 0)
                    let tip = x ? CGPoint(x: 60, y: 50 + 130) : CGPoint(x: 50 + 130, y: 60)
                    XCTAssertTrue(try XCTUnwrap(node.clipBox).bounds.contains(node.convert(tip, from: root)),
                                  "clip x: \(x), animated: \(animated)")
                    if !animated { XCTAssertEqual(try pixels(root)(tip), [255, 0, 0, 255], "clip x: \(x)") }
                }
            }
        }
    }

    #if os(iOS)
    /// A press eases from the box's full size on the render server while
    /// the model already holds the pressed scale.
    func testPressEaseKeepsItsStartingGeometryUnclipped() throws {
        for x in [true, false] {
            try withClip(x) { _, _, node, child in
                child.frame = x ? CGRect(x: 0, y: 60, width: 20, height: 40) : CGRect(x: 60, y: 0, width: 40, height: 20)
                child.applyStyle(["background_color": [255, 0, 0, 255], "press_scale": 0.5])
                child.showPress(true)
                XCTAssertNotNil(child.layer.animation(forKey: "press"))
                let box = try XCTUnwrap(node.clipBox)
                XCTAssertGreaterThanOrEqual(x ? box.bounds.maxY : box.bounds.maxX, 100, "clip x: \(x)")
            }
        }
    }
    #endif

    private func withClip(_ x: Bool, _ body: (Presenter, NodeView, NodeView, NodeView) throws -> Void) throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let p = Presenter()
        let root = NodeView(id: 1, kind: "view", presenter: p)
        root.frame = CGRect(x: 0, y: 0, width: 200, height: 200)
        root.applyStyle(["background_color": [255, 255, 255, 255]])
        #if os(iOS)
        let window = UIWindow(frame: root.bounds)
        window.addSubview(root); window.makeKeyAndVisible()
        defer { window.isHidden = true }
        #else
        let window = NSWindow(contentRect: root.bounds, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.contentView = root
        window.orderFront(nil)
        defer { window.close() }
        #endif
        let node = NodeView(id: 2, kind: "view", presenter: p)
        node.frame = CGRect(x: 50, y: 50, width: 40, height: 40)
        root.addSubview(node)
        node.applyStyle(style(x))
        let child = NodeView(id: 3, kind: "view", presenter: p)
        child.frame = CGRect(x: 0, y: 0, width: 20, height: 20)
        child.applyStyle(["background_color": [255, 0, 0, 255]])
        node.container.addSubview(child)
        #if os(macOS)
        root.layoutSubtreeIfNeeded(); window.displayIfNeeded()
        #endif
        XCTAssertEqual(try pixels(root)(CGPoint(x: 60, y: 60)), [255, 0, 0, 255], "untransformed paint")
        try body(p, root, node, child)
    }

    #if os(iOS)
    func testFlatRunsExtendTheVisibleAxisAfterTheirFramesChange() throws {
        for x in [true, false] {
            let p = Presenter()
            let root = NodeView(id: 1, kind: "view", presenter: p)
            root.frame = CGRect(x: 0, y: 0, width: 200, height: 200)
            root.applyStyle(["background_color": [255, 255, 255, 255]])
            p.views[1] = root
            let window = UIWindow(frame: root.bounds)
            window.addSubview(root); window.makeKeyAndVisible()
            defer { window.isHidden = true }
            let n = NodeView(id: 2, kind: "view", presenter: p)
            n.frame = CGRect(x: 50, y: 50, width: 40, height: 40)
            n.applyStyle(style(x)); root.addSubview(n); p.views[2] = n
            p.apply(wireBatch([
                ["op": "create", "id": 3, "kind": "view", "style": ["width": 20, "height": 20,
                    "background_color": [255, 0, 0, 255], "written": ["background_color"]]],
                ["op": "create", "id": 4, "kind": "view", "style": ["width": 20, "height": 20,
                    "background_color": [255, 0, 0, 255], "written": ["background_color"]]],
                ["op": "children", "id": 2, "ids": [3, 4]],
                ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 20, "h": 20],
                ["op": "frame", "id": 4, "x": 0, "y": 0, "w": 20, "h": 20],
            ]))
            XCTAssertEqual(p.flats.leaves.count, 2)
            for distance: CGFloat in [100_000, -200_000] {
                p.apply(wireBatch([["op": "frame", "id": 4, "x": x ? 0 : distance,
                                   "y": x ? distance : 0, "w": 20, "h": 20]]))
                n.frame.origin = x ? CGPoint(x: 50, y: 50 - distance) : CGPoint(x: 50 - distance, y: 50)
                root.layoutIfNeeded()
                XCTAssertEqual(p.flats.leaves.count, 2)
                XCTAssertEqual(try pixels(root)(CGPoint(x: 60, y: 60)), [255, 0, 0, 255])
            }
        }
    }
    #endif

    func testVisibleOverflowHasNoFixedReachAndFollowsDescendantChanges() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        for x in [true, false] {
            let p = Presenter()
            let root = NodeView(id: 1, kind: "view", presenter: p)
            root.frame = CGRect(x: 0, y: 0, width: 200, height: 200)
            root.applyStyle(["background_color": [255, 255, 255, 255]])
            #if os(iOS)
            let window = UIWindow(frame: root.bounds)
            window.addSubview(root); window.makeKeyAndVisible()
            defer { window.isHidden = true }
            #else
            let window = NSWindow(contentRect: root.bounds, styleMask: [.borderless], backing: .buffered, defer: false)
            window.isReleasedWhenClosed = false; window.contentView = root
            defer { window.close() }
            #endif
            let n = NodeView(id: 2, kind: "view", presenter: p)
            n.frame = CGRect(x: 50, y: 50, width: 40, height: 40)
            n.applyStyle(style(x)); root.addSubview(n)
            let branch = NodeView(id: 3, kind: "view", presenter: p)
            branch.frame = n.bounds; n.container.addSubview(branch)
            let child = NodeView(id: 4, kind: "view", presenter: p)
            child.frame = n.bounds
            child.applyStyle(["background_color": [255, 0, 0, 255]])
            branch.addSubview(child)
            for distance: CGFloat in [100_000, -200_000, 300_000] {
                child.frame.origin = x ? CGPoint(x: 0, y: distance) : CGPoint(x: distance, y: 0)
                n.frame.origin = x ? CGPoint(x: 50, y: 50 - distance) : CGPoint(x: 50 - distance, y: 50)
                #if os(iOS)
                root.layoutIfNeeded()
                #else
                root.layoutSubtreeIfNeeded()
                #endif
                XCTAssertTrue(hit(root, CGPoint(x: 60, y: 60)) === child)
                let image = try pixels(root)
                XCTAssertEqual(image(CGPoint(x: 60, y: 60)), [255, 0, 0, 255], "clip x: \(x), distance: \(distance)")
                XCTAssertEqual(image(CGPoint(x: 30, y: 30)), [255, 255, 255, 255])
            }
        }
    }

    private func hit(_ root: NodeView, _ point: CGPoint) -> AnyObject? {
        #if os(iOS)
        root.hitTest(point, with: nil)
        #else
        root.hitTest(root.convert(point, to: root.superview))
        #endif
    }

    private func pixels(_ root: NodeView) throws -> (CGPoint) -> [UInt8] {
        #if os(iOS)
        let bitmap = try XCTUnwrap(Bitmap.blank(width: 200, height: 200))
        func display(_ view: UIView) { view.layer.displayIfNeeded(); view.subviews.forEach(display) }
        display(root)
        Capture.draw(root, scale: 1, into: bitmap)
        let image = try XCTUnwrap(bitmap.image?.cgImage)
        #else
        let rep = try XCTUnwrap(Capture.paintOrderBitmap(of: root, scale: 1))
        let image = try XCTUnwrap(rep.cgImage)
        #endif
        let context = try XCTUnwrap(CGContext(data: nil, width: 200, height: 200, bitsPerComponent: 8,
            bytesPerRow: 800, space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.draw(image, in: CGRect(x: 0, y: 0, width: 200, height: 200))
        let data = Data(bytes: context.data!, count: 160_000)
        return { p in Array(data[(Int(p.y) * 200 + Int(p.x)) * 4..<(Int(p.y) * 200 + Int(p.x)) * 4 + 4]) }
    }
}

#if os(iOS)
final class OverflowClipIOSTests: OverflowClipTests {}
#endif
