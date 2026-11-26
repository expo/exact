#if os(macOS) || os(iOS)
import XCTest
@testable import ExactKit
#if os(macOS)
import AppKit
#else
import UIKit
#endif

private weak var observedText: NodeView?
private var draws = 0

extension NodeView {
    @objc fileprivate dynamic func countMovedTextDraw(_ rect: CGRect) {
        if self === observedText { draws += 1 }
        countMovedTextDraw(rect)
    }
}

final class MovedTextTests: XCTestCase {
    func testMovingALabelKeepsItsInkUntilAPaintInputChanges() throws {
        let session = ExactApp.shared.makeSession(label: "moving-text")
        defer { session.destroy() }
        let p = session.presenter
        let rect = CGRect(x: 0, y: 0, width: 320, height: 200)
        #if os(macOS)
        _ = NSApplication.shared
        let window = NSWindow(contentRect: rect, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.setFrameOrigin(NSPoint(x: 160, y: 240))
        window.colorSpace = .sRGB
        window.hasShadow = false
        let page = NSView(frame: rect)
        window.contentView = page
        page.addSubview(p.viewport)
        p.viewport.frame = rect
        window.orderFrontRegardless()
        defer { window.close() }
        #else
        guard let scene = hostedWindowScene() else { throw missing("scene", "the label draws in its window") }
        let window = UIWindow(windowScene: scene)
        window.frame = rect
        let controller = UIViewController()
        window.rootViewController = controller
        controller.view.addSubview(p.viewport)
        p.viewport.frame = rect
        window.isHidden = false
        defer { window.isHidden = true }
        #endif
        func frame(_ x: Double, y: Double = 10, width: Int = 56) -> [String: Any] {
            ["op": "frame", "id": 2, "x": x, "y": y, "w": width, "h": 32]
        }
        var style: [String: Any] = [
            "font_size": 12, "white_space": "nowrap", "text_color": [220, 0, 0, 255], "background_color": [255, 255, 255, 255],
            "box_shadow": [["o": [2, 4], "b": 1, "s": 0, "c": [0, 0, 255, 255]]],
        ]
        #if os(iOS)
        style["background_clip"] = "text"
        #endif
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "Overflowing ruler label"], "style": style],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 160],
            ["op": "present", "id": 1, "property": "translate", "x": 4, "y": 3], frame(5),
        ]))
        let text = try XCTUnwrap(p.views[2])
        let original = try XCTUnwrap(class_getInstanceMethod(NodeView.self, #selector(NodeView.draw(_:))))
        let counted = try XCTUnwrap(class_getInstanceMethod(NodeView.self, #selector(NodeView.countMovedTextDraw(_:))))
        method_exchangeImplementations(original, counted)
        observedText = text
        defer { observedText = nil; method_exchangeImplementations(original, counted) }
        func display() {
            #if os(macOS)
            window.displayIfNeeded()
            text.layer?.displayIfNeeded()
            #else
            window.layoutIfNeeded()
            text.layer.displayIfNeeded()
            #endif
            CATransaction.flush()
        }
        #if os(macOS)
        text.needsDisplay = true
        #else
        text.setNeedsDisplay()
        #endif
        draws = 0
        display()
        XCTAssertGreaterThan(draws, 0, "the label draws into its own layer")
        func apply(_ ops: [[String: Any]]) -> Int {
            draws = 0
            p.apply(wireBatch(ops))
            display()
            return draws
        }
        #if os(macOS)
        // Read the compositor. Rendering the layer into a new context can draw
        // the glyphs again and conceal a stale or clipped backing store.
        func pixels() throws -> NSBitmapImageRep { try XCTUnwrap(Capture.windowServerPicture(of: page)) }
        let first = try pixels()
        #endif
        XCTAssertEqual(apply([["op": "paragraph", "id": 2, "runs": []], frame(75), ["op": "style", "id": 2, "style": style]]), 0)
        XCTAssertEqual(text.frame.minX, 75)
        #if os(macOS)
        let moved = try pixels()
        let scale = CGFloat(moved.pixelsWide) / page.bounds.width
        func extent(_ picture: NSBitmapImageRep, red: Bool) -> ClosedRange<Int>? {
            var lo = picture.pixelsWide, hi = -1
            for y in 0..<picture.pixelsHigh {
                for x in 0..<picture.pixelsWide {
                    guard let c = picture.colorAt(x: x, y: y) else { continue }
                    let channel = red ? c.redComponent : c.blueComponent
                    if channel - max(c.greenComponent, red ? c.blueComponent : c.redComponent) > 0.35 {
                        lo = min(x, lo); hi = max(x, hi)
                    }
                }
            }
            return hi >= lo ? lo...hi : nil
        }
        for red in [true, false] {
            let old = try XCTUnwrap(extent(first, red: red)), new = try XCTUnwrap(extent(moved, red: red))
            let dx = Int((70 * scale).rounded())
            XCTAssertEqual(new.lowerBound, old.lowerBound + dx, accuracy: 2)
            XCTAssertEqual(new.upperBound, old.upperBound + dx, accuracy: 2)
            if red { XCTAssertGreaterThan(new.upperBound, Int((75 + 4 + 56) * scale), "ink beyond the right edge survives") }
        }
        text.labelInk = nil
        text.needsDisplay = true
        display()
        let fresh = try pixels()
        let actual = CapturePixels.bytes(moved.cgImage), reference = CapturePixels.bytes(fresh.cgImage)
        XCTAssertEqual(moved.pixelsWide, fresh.pixelsWide)
        XCTAssertEqual(moved.pixelsHigh, fresh.pixelsHigh)
        XCTAssertEqual(actual.count, reference.count)
        // WindowServer dithers a freshly drawn backing. Compare every channel,
        // including the antialiased edges of overflowing glyphs and the cast.
        let difference = zip(actual, reference).map { abs(Int($0) - Int($1)) }.max()
        XCTAssertLessThanOrEqual(try XCTUnwrap(difference), 8)
        #endif
        XCTAssertEqual(apply([["op": "present", "id": 2, "property": "translate", "x": 7]]), 0)
        XCTAssertEqual(apply([["op": "present", "id": 2, "property": "opacity", "x": 0.5]]), 0)
        style["z_index"] = 4
        XCTAssertEqual(apply([["op": "rank", "id": 2, "rank": 4], ["op": "style", "id": 2, "style": style]]), 0)
        #if os(macOS)
        _ = apply([frame(14)])
        let paragraph = try XCTUnwrap(text.paragraphLayout()), baseline = try XCTUnwrap(paragraph.baselines.first)
        let top = text.paragraphBox().minY, factor = p.gridScale
        func localBaseline(_ y: CGFloat) -> CGFloat {
            let place = p.gridPlace(text, CGRect(x: 14, y: y, width: 56, height: 32))
            return TextEngine.paintedLine(baseline, top: top + place.baselineDrop, scale: factor, lineScale: paragraph.lineScale)
        }
        let offsets = (1..<64).map { 10 + CGFloat($0) / 64 }
        let same = try XCTUnwrap(offsets.first { localBaseline($0) == localBaseline(10) })
        XCTAssertEqual(apply([frame(14, y: same)]), 0)
        if let different = offsets.first(where: { localBaseline($0) != localBaseline(10) }) {
            XCTAssertGreaterThan(apply([frame(14, y: different)]), 0)
        }
        #else
        _ = apply([["op": "props", "id": 2, "set": ["textLeft": "0.125"]]])
        style["content_transition"] = "numeric-text"
        _ = apply([["op": "style", "id": 2, "style": style], frame(0)])
        let start = text.textPlace(scale: p.gridScale)
        let origin = p.gridOrigin(above: text)
        let x = try XCTUnwrap((1..<64).map { CGFloat($0) / 64 }.first {
            let place = PixelGrid.place(CGRect(x: $0, y: 10, width: 56, height: 32), in: origin, scale: p.gridScale)
            return PixelGrid.place(text.paragraphBox(), in: place.origin, scale: p.gridScale).drawn.origin != start
        })
        XCTAssertGreaterThan(apply([frame(x)]), 0, "the numeral run snaps at its new absolute origin")
        XCTAssertNotEqual(text.textPlace(scale: p.gridScale), start)
        #endif
        XCTAssertGreaterThan(apply([frame(14), ["op": "props", "id": 2, "set": ["text": "north south east west"]]]), 0)
        style["text_color"] = [0, 150, 0, 255]
        XCTAssertGreaterThan(apply([frame(16), ["op": "style", "id": 2, "style": style]]), 0)
        style["font_size"] = 14
        style.removeValue(forKey: "white_space")
        XCTAssertGreaterThan(apply([["op": "style", "id": 2, "style": style]]), 0)
        XCTAssertGreaterThan(apply([frame(20, width: 36)]), 0, "the words wrap at the new width")
        let run: [String: Any] = ["id": 21, "parent": 2, "paint": true, "props": ["text": "inline"]]
        XCTAssertGreaterThan(apply([frame(24, width: 36), ["op": "props", "id": 2, "clear": ["text"]],
                                    ["op": "paragraph", "id": 2, "runs": [run]]]), 0)
        var painted = run
        painted["style"] = ["text_color": [0, 0, 120, 255]]
        XCTAssertGreaterThan(apply([frame(26, width: 36), ["op": "paragraph", "id": 2, "runs": [painted]]]), 0)
        #if os(macOS)
        style["background_attachment"] = "fixed"
        style["background_image"] = [["css": "linear-gradient(90deg, red, blue)"]]
        _ = apply([["op": "style", "id": 2, "style": style]])
        XCTAssertGreaterThan(apply([frame(32, width: 36)]), 0, "a fixed gradient samples the viewport at the new position")
        draws = 0
        text.viewDidChangeBackingProperties()
        display()
        XCTAssertGreaterThan(draws, 0)
        draws = 0
        text.appearance = NSAppearance(named: text.drawsDark ? .aqua : .darkAqua)
        display()
        XCTAssertGreaterThan(draws, 0)
        #else
        draws = 0
        window.overrideUserInterfaceStyle = text.traitCollection.userInterfaceStyle == .dark ? .light : .dark
        display()
        XCTAssertGreaterThan(draws, 0)
        p.root.traitOverrides.displayScale = p.gridScale == 2 ? 3 : 2
        XCTAssertGreaterThan(apply([frame(30, width: 36)]), 0)
        #endif
    }
}
#endif
