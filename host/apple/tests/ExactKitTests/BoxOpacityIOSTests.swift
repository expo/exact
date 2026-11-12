#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class BoxOpacityIOSTests: XCTestCase {
    private final class CountedLayer: CALayer {
        var renders = 0
        override func render(in ctx: CGContext) { renders += 1; super.render(in: ctx) }
    }

    private func program(_ opacity: Double) -> [Double] {
        [0, 0, 0, 0, 1, 9, -1, -3, -1e6, -1e6, 2e6, 2e6, 0, 0, 0, 0, 1, 2, 0, opacity]
    }
    private func filter(_ opacity: Double) -> [String: Any] { ["p": program(opacity)] }

    func testOpacityDoesNotSnapshotTheSubtreeWhenItChanges() throws {
        let parent = CALayer(), box = CountedLayer(), child = CALayer(), f = BoxFilter()
        box.frame = CGRect(x: 0, y: 0, width: 40, height: 40)
        child.frame = box.bounds
        parent.addSublayer(box); box.addSublayer(child)
        let value: BatchValue = ["p": .array(program(0.5).map(BatchValue.number))]
        XCTAssertTrue(f.set(value))
        for color in [UIColor.red, .blue, .green] {
            child.backgroundColor = color.cgColor
            f.render(box, clip: nil, scale: 1, dark: false, node: 1)
        }
        XCTAssertEqual(box.renders, 0, "changing content must not invoke CALayer.render(in:)")
        XCTAssertNil(f.picture.superlayer)
        XCTAssertNil(box.mask)
        XCTAssertFalse(f.drawing)
    }

    private func fixture(native: Bool = false) throws -> (Presenter, NodeView, UIWindow) {
        let p = Presenter(), window = UIWindow(frame: CGRect(x: 0, y: 0, width: 160, height: 160))
        p.buttonFace = { _ in ButtonFace() }
        let controller = UIViewController()
        controller.view = p.viewport
        window.rootViewController = controller
        window.isHidden = false
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": native ? "control" : "view", "props": native ? ["type": "button"] : [:],
             "handlers": ["press"], "style": style(0.5).merging(native ? ["appearance": "auto"] : [:]) { $1 }],
            ["op": "create", "id": 2, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0],
            ["op": "frame", "id": 2, "x": 10.0, "y": 10.0, "w": 20.0, "h": 20.0],
            ["op": "present", "id": 1, "property": "opacity", "x": 0.8],
        ]))
        return (p, try XCTUnwrap(p.views[1]), window)
    }
    private func style(_ factor: Double?) -> [String: Any] {
        var s: [String: Any] = ["background_color": [255, 0, 0, 255], "press_opacity_to": 0.4,
                              "looks": ["hover": ["opacity": 0.6]]]
        if let factor { s["filter"] = filter(factor) }
        return s
    }
    private func delta(_ box: NodeView, _ key: String) throws -> Float {
        try XCTUnwrap((box.layer.animation(forKey: key) as? CABasicAnimation)?.toValue as? Float)
    }
    private func pixels(_ layer: CALayer) throws -> [UInt8] {
        let ctx = try XCTUnwrap(CGContext(data: nil, width: 40, height: 40, bitsPerComponent: 8, bytesPerRow: 160,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        layer.render(in: ctx)
        return Array(UnsafeBufferPointer(start: try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self), count: 6400))
    }

    func testOpacityComposesWithLooksPressesAndLiveChanges() throws {
        let (p, box, window) = try fixture()
        defer { window.isHidden = true; window.rootViewController = nil }
        XCTAssertNil(box.boxFilter?.picture.superlayer)
        XCTAssertEqual(box.layer.opacity, 0.4, accuracy: 1e-6)
        box.lookState(hovered: true)
        XCTAssertEqual(try delta(box, "lookOpacity"), -0.1, accuracy: 1e-6)
        box.paintPress(true)
        XCTAssertEqual(try delta(box, "pressOpacity"), -0.1, accuracy: 1e-6)
        let start = (box.layer.animation(forKey: "pressOpacity") as? CABasicAnimation)?.beginTime
        for factor in [0.25, 0, 0.5] {
            p.apply(wireBatch([["op": "style", "id": 1, "style": style(factor)]]))
            XCTAssertEqual(box.layer.opacity, Float(0.8 * factor), accuracy: 1e-6)
            if factor != 0 {
                XCTAssertEqual(try delta(box, "lookOpacity"), Float(-0.2 * factor), accuracy: 1e-6)
                XCTAssertEqual(try delta(box, "pressOpacity"), Float(-0.2 * factor), accuracy: 1e-6)
                XCTAssertEqual((box.layer.animation(forKey: "pressOpacity") as? CABasicAnimation)?.beginTime, start)
            }
        }
        p.apply(wireBatch([["op": "present", "id": 1, "property": "opacity", "x": 0.2]]))
        XCTAssertEqual(box.layer.opacity, 0.1, accuracy: 1e-6)
        XCTAssertEqual(try delta(box, "lookOpacity"), 0.2, accuracy: 1e-6)
        p.apply(wireBatch([["op": "style", "id": 1, "style": style(nil)]]))
        XCTAssertEqual(box.layer.opacity, 0.2, accuracy: 1e-6)
        XCTAssertEqual(try delta(box, "lookOpacity"), 0.4, accuracy: 1e-6)
        XCTAssertEqual(try delta(box, "pressOpacity"), -0.2, accuracy: 1e-6)
        box.paintPress(false, instantly: true)
        box.lookState(hovered: false)
        XCTAssertNil(box.layer.animation(forKey: "pressOpacity"))
        XCTAssertNil(box.layer.animation(forKey: "lookOpacity"))
        p.apply(wireBatch([["op": "style", "id": 1, "style": style(0)]]))
        box.removeFromSuperview(); box.recycle(); box.rebind(3)
        box.applyStyle([:])
        XCTAssertEqual(box.layer.opacity, 1)
        XCTAssertTrue(box.pressPaint.atRest)
    }

    func testCSSOpacityIsClampedBeforeTheFilterAndPress() throws {
        let (p, box, window) = try fixture()
        defer { window.isHidden = true; window.rootViewController = nil }
        for value in [2.0, -0.5, 1.2, 0.8] {
            let clamped = Float(min(1, max(0, value)))
            p.apply(wireBatch([["op": "present", "id": 1, "property": "opacity", "x": value]]))
            XCTAssertEqual(box.ownOpacity, clamped)
            XCTAssertEqual(box.layer.opacity, clamped * 0.5, accuracy: 1e-6)
            XCTAssertEqual(try pixels(box.layer)[3], UInt8((clamped * 0.5 * 255).rounded()))
            box.paintPress(true, instantly: true)
            XCTAssertEqual(box.layer.opacity + (try delta(box, "pressOpacity")), 0.2, accuracy: 1e-6)
            box.paintPress(false, instantly: true)
        }
        p.apply(wireBatch([["op": "present", "id": 1, "property": "opacity", "x": 2.0]]))
        p.apply(wireBatch([["op": "style", "id": 1, "style": style(nil)]]))
        XCTAssertEqual(box.layer.opacity, 1)
        let f = BoxFilter()
        for value: Float in [-0.5, 2] {
            f.presentOpacity(value, on: box.layer)
            XCTAssertEqual(box.layer.opacity, min(1, max(0, value)), "the unfiltered branch clamps too")
        }
    }

    func testOpacityPixelsMatchGroupOpacityAndMixedChainsStillSnapshot() throws {
        let (p, box, window) = try fixture()
        defer { window.isHidden = true; window.rootViewController = nil }
        window.layoutIfNeeded(); box.layer.displayIfNeeded()
        let reference = CALayer(), child = CALayer()
        reference.frame = box.bounds
        reference.backgroundColor = UIColor.red.cgColor; reference.opacity = 0.4
        child.frame = CGRect(x: 10, y: 10, width: 20, height: 20)
        child.backgroundColor = UIColor.blue.cgColor; reference.addSublayer(child)
        let actual = try pixels(box.layer), expected = try pixels(reference)
        XCTAssertLessThanOrEqual(zip(actual, expected).map { abs(Int($0) - Int($1)) }.max() ?? 0, 1)
        box.paintPress(true, instantly: true)
        CATransaction.flush()
        waitFor("the filtered pressed opacity") { abs((box.layer.presentation()?.opacity ?? -1) - 0.2) < 1e-6 }
        let pressed = try pixels(Capture.paintOrderLayer(of: box, root: box.layer))
        reference.opacity = 0.2
        XCTAssertLessThanOrEqual(zip(pressed, try pixels(reference)).map { abs(Int($0) - Int($1)) }.max() ?? 0, 1)
        box.paintPress(false, instantly: true)
        var mixed = program(0.5)
        mixed[4] = 2
        mixed += [0, 0, -3, -1e6, -1e6, 2e6, 2e6, 0, 1, 1]
        p.apply(wireBatch([["op": "style", "id": 1, "style": ["filter": ["p": mixed]]]]))
        XCTAssertNotNil(box.boxFilter?.picture.superlayer)
        XCTAssertEqual(box.layer.opacity, 0.8, accuracy: 1e-6, "the snapshot chain applies its own opacity")
    }

    func testPlatformDimAndPressedSharesFollowFilterChanges() throws {
        let (p, box, window) = try fixture()
        defer { window.isHidden = true; window.rootViewController = nil }
        NativeButtonState(box).present(on: box, down: true)
        XCTAssertEqual(try delta(box, "nativeActiveOpacity"), -0.2, accuracy: 1e-6)
        ActivePresentation.restore(box.layer)
        func update(_ factor: Double, share: Double? = nil) {
            var s: [String: Any] = ["filter": filter(factor)]
            if let share { s["press_opacity"] = share }
            p.apply(wireBatch([["op": "style", "id": 1, "style": s]]))
        }
        update(0.5)
        box.dimPress(true)
        for factor in [0.25, 0, 0.5] {
            update(factor)
            XCTAssertEqual(try delta(box, "pressDim"), -0.8 * Float(factor) * (1 - NodeView.dimmedOpacity), accuracy: 1e-6)
        }
        box.dimPress(false)
        let release = try XCTUnwrap(box.layer.animation(forKey: "pressDim") as? CABasicAnimation)
        update(0.25)
        let changed = try XCTUnwrap(box.layer.animation(forKey: "pressDim") as? CABasicAnimation)
        XCTAssertEqual(changed.beginTime, release.beginTime, "a new factor keeps the release's clock")
        XCTAssertEqual(try XCTUnwrap(changed.fromValue as? Float), -0.2 * (1 - NodeView.dimmedOpacity), accuracy: 1e-6)
        box.dimPress(false, instantly: true)
        update(0.5, share: 0.5)
        box.paintPress(true, instantly: true)
        XCTAssertEqual(try delta(box, "pressOpacity"), -0.2, accuracy: 1e-6)
        p.apply(wireBatch([["op": "present", "id": 1, "property": "opacity", "x": 0.4]]))
        XCTAssertEqual(try delta(box, "pressOpacity"), -0.1, accuracy: 1e-6)
        update(0, share: 0.5)
        p.apply(wireBatch([["op": "present", "id": 1, "property": "opacity", "x": 0.6]]))
        update(0.5, share: 0.5)
        XCTAssertEqual(box.layer.opacity, 0.3, accuracy: 1e-6)
        XCTAssertEqual(try delta(box, "pressOpacity"), -0.15, accuracy: 1e-6)
    }

    func testAHeldNativeButtonFollowsFilterStyleBatches() throws {
        let (p, box, window) = try fixture(native: true)
        defer { window.isHidden = true; window.rootViewController = nil }
        let button = try XCTUnwrap(p.controls.controls[1] as? NativeButtonIOS)
        button.isHighlighted = true
        button.updateConfiguration()
        for share in [false, true] {
            for factor: Double? in [0.5, 0.25, 0, nil, 0.5] {
                var s = style(factor)
                s["appearance"] = "auto"
                if share { s.removeValue(forKey: "press_opacity_to"); s["press_opacity"] = 0.5 }
                p.apply(wireBatch([["op": "style", "id": 1, "style": s]]))
                let product = Float(0.8 * (factor ?? 1)), target = Float(0.4 * (factor ?? 1))
                let animation = box.layer.animation(forKey: "nativeActiveOpacity") as? CABasicAnimation
                XCTAssertEqual(box.layer.opacity, product, accuracy: 1e-6)
                let active = (animation?.toValue as? Float ?? 0) + product
                XCTAssertEqual(active, target, accuracy: 1e-6)
                CATransaction.flush()
                if abs(active - target) < 1e-6 {
                    waitFor("the native button's filtered active opacity") {
                        abs((box.layer.presentation()?.opacity ?? -1) - target) < 1e-6
                    }
                }
            }
        }
        button.isHighlighted = false
        button.updateConfiguration()
        XCTAssertNil(box.layer.animation(forKey: "nativeActiveOpacity"))
    }

    func testANativeButtonsHoverLookFollowsTheFilter() throws {
        let (p, box, window) = try fixture(native: true)
        defer { window.isHidden = true; window.rootViewController = nil }
        func shown() -> Float {
            box.layer.opacity + ((box.layer.animation(forKey: "lookOpacity") as? CABasicAnimation)?.toValue as? Float ?? 0)
        }
        box.lookState(hovered: true)
        XCTAssertEqual(shown(), 0.3, accuracy: 1e-6, "the look installs on hover")
        p.apply(wireBatch([["op": "style", "id": 1, "style": style(nil).merging(["appearance": "auto"]) { $1 }]]))
        XCTAssertEqual(shown(), 0.6, accuracy: 1e-6, "removing the filter recomputes the look")
        box.lookState(hovered: false)
        XCTAssertEqual(shown(), 0.8, accuracy: 1e-6)
        box.lookState(hovered: true)
        XCTAssertEqual(shown(), 0.6, accuracy: 1e-6, "an unfiltered look installs on hover")
    }

    func testCanvasCapturesKeepFeedbackWhileIgnoringRootCSSOpacity() throws {
        let (p, box, window) = try fixture()
        defer { window.isHidden = true; window.rootViewController = nil }
        let shadow = try XCTUnwrap(Shadow.shared)
        for factor: Double? in [0.5, 0, 0.25, nil] {
            p.apply(wireBatch([["op": "style", "id": 1, "style": style(factor)]]))
            for cssOpacity in [0.8, 0.5, 0] {
                p.apply(wireBatch([["op": "present", "id": 1, "property": "opacity", "x": cssOpacity]]))
                for state in 0..<5 {
                    let pressed = state == 2 || state == 3, hovered = state == 1 || state == 3
                    box.lookState(hovered: hovered)
                    box.paintPress(pressed, instantly: true)
                    box.dimPress(state == 4, instantly: true)
                    let shown = (pressed ? 0.4 : hovered ? 0.6 : cssOpacity) * (state == 4 ? Double(NodeView.dimmedOpacity) : 1)
                    CATransaction.flush()
                    waitFor("the captured root's opacity") {
                        let target = Float(shown * (factor ?? 1))
                        return abs((box.layer.presentation()?.opacity ?? -1) - target) < 1e-6
                    }
                    let cpu = try XCTUnwrap(Bitmap.blank(width: 40, height: 40))
                    Capture.draw(box, scale: 1, into: cpu)
                    let gpu = try XCTUnwrap(Bitmap.blank(width: 40, height: 40))
                    XCTAssertTrue(shadow.render(box, scale: 1, into: gpu))
                    for bitmap in [cpu, gpu] {
                        let bytes = try XCTUnwrap(bitmap.bytes).assumingMemoryBound(to: UInt8.self)
                        let expected = cssOpacity > 0 ? shown * (factor ?? 1) / cssOpacity : factor ?? 1
                        XCTAssertEqual(Double(bytes[(20 * 40 + 20) * 4 + 3]), min(1, expected) * 255, accuracy: 1,
                                       "filter \(String(describing: factor)), CSS \(cssOpacity), state \(state)")
                    }
                    XCTAssertEqual(box.layer.opacity, Float(cssOpacity * (factor ?? 1)), accuracy: 1e-6)
                }
                box.paintPress(false, instantly: true)
                box.lookState(hovered: false)
                box.dimPress(false, instantly: true)
            }
        }
    }

    func testAPlacedCaptureKeepsThePressAfterPlacementCommits() throws {
        let (p, box, window) = try fixture()
        defer { window.isHidden = true; window.rootViewController = nil }
        let layer = try XCTUnwrap(box.layer as? NodeLayer), shadow = try XCTUnwrap(Shadow.shared)
        for factor: Double? in [0.5, nil] {
            p.apply(wireBatch([["op": "style", "id": 1, "style": style(factor)]]))
            layer.drawnElsewhere = true
            box.paintPress(true, instantly: true)
            CATransaction.flush()
            waitFor("the placed child's committed press") {
                abs((layer.presentation()?.opacity ?? -1) - Float(1 - 0.4 * (factor ?? 1))) < 1e-6
            }
            let cpu = try XCTUnwrap(Bitmap.blank(width: 40, height: 40))
            Capture.draw(box, scale: 1, into: cpu)
            let gpu = try XCTUnwrap(Bitmap.blank(width: 40, height: 40))
            XCTAssertTrue(shadow.render(box, scale: 1, into: gpu))
            for bitmap in [cpu, gpu] {
                let bytes = try XCTUnwrap(bitmap.bytes).assumingMemoryBound(to: UInt8.self)
                XCTAssertEqual(Double(bytes[(20 * 40 + 20) * 4 + 3]), (factor ?? 1) * 0.5 * 255, accuracy: 1)
            }
            box.paintPress(false, instantly: true)
            layer.drawnElsewhere = false
        }
    }
}
#endif
