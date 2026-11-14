#if os(iOS)
import ExactNumerals
import UIKit
import XCTest
@testable import ExactKit

/// A paragraph under `-exact-content-transition: numeric`, drawn by the
/// Numerals capability (SwiftUI's `Text`, LLP 1077 D15), against the same
/// paragraph without it, which Exact draws.
final class NumeralsIOSTests: XCTestCase {
    private var installed: NumeralCapability?
    override func setUp() { super.setUp(); installed = NumeralLink.installed; ExactNumerals.install() }
    override func tearDown() { NumeralLink.installed = installed; super.tearDown() }

    private let red: [Double] = [255, 0, 0, 255], green: [Double] = [0, 200, 0, 255]
    private typealias Pixels = (bytes: [UInt8], width: Int)

    private func hosted(_ label: String, in scene: UIWindowScene) -> (Presenter, UIWindow) {
        let session = ExactApp.shared.makeSession(label: label)
        addTeardownBlock { session.destroy() }
        let p = session.presenter
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 640)
        window.backgroundColor = .white
        p.viewport.frame = window.bounds
        window.rootViewController = UIViewController()
        window.rootViewController!.view.addSubview(p.viewport)
        window.makeKeyAndVisible()
        addTeardownBlock { window.isHidden = true }
        return (p, window)
    }

    /// "Finished " and a count on a rounded green box, both in red ink.
    private func runs(_ id: Int, _ count: String, style: [String: Any] = [:], countStyle: [String: Any] = [:]) -> [[String: Any]] {
        var plain: [String: Any] = ["font_size": 20.0, "text_color": red]
        plain.merge(style) { $1 }
        var boxed = plain
        boxed.merge(["background_color": green, "border_radius_top_left": 8.0, "border_radius_top_right": 8.0,
                     "border_radius_bottom_right": 8.0, "border_radius_bottom_left": 8.0]) { $1 }
        boxed.merge(countStyle) { $1 }
        return [["id": id + 1, "parent": id, "paint": true, "props": ["text": "Finished "], "style": plain],
                ["id": id + 2, "parent": id, "paint": true, "props": ["text": count], "style": boxed]]
    }

    private func capability(_ node: NodeView) -> Bool { node.subviews.contains { String(describing: type(of: $0)).contains("Hosting") } }

    /// Paint what the batch changed, as the end of a batch does.
    private func paint(_ p: Presenter, _ nodes: [NodeView]) {
        p.paintVisibleText()
        p.textRasters.settleVisible(nodes)
        for node in nodes { node.layer.displayIfNeeded() }
        CATransaction.flush()
    }

    /// The window's pixels, two a point, once glyphs are in every one of
    /// `areas` and the picture has not changed for a third of a second:
    /// SwiftUI has shown the text and finished rolling it (two pictures in
    /// one display frame are the same however the roll stands).
    private func still(_ window: UIWindow, inked areas: [CGRect], _ what: String = "") -> Pixels {
        var last: Pixels?, since = Date()
        waitFor("\(what): the text drawn and still", cap: 10) {
            guard let now = try? shot(window) else { return false }
            defer { last = now }
            guard now.bytes == last?.bytes, areas.allSatisfy({ !ink(now, in: $0).glyphs.isNull }) else { since = Date(); return false }
            return Date().timeIntervalSince(since) >= 0.33
        }
        return last ?? ([], 0)
    }

    /// The window's pixels, two a point.
    private func shot(_ window: UIWindow) throws -> Pixels {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 2; format.opaque = true
        // The layers as they rest: `drawHierarchy` approximates the blur
        // filter SwiftUI's numeric text keeps, and smears the area under it.
        let image = try XCTUnwrap(UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { context in
            window.layer.render(in: context.cgContext)
        }.cgImage)
        var bytes = [UInt8](repeating: 0, count: image.width * image.height * 4)
        let ctx = try XCTUnwrap(CGContext(data: &bytes, width: image.width, height: image.height, bitsPerComponent: 8, bytesPerRow: image.width * 4,
                                          space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        return (bytes, image.width)
    }

    /// The red glyphs' and the green box's extents in points from an area's
    /// top left, and how many pixels the glyphs cover.
    private struct Ink { var glyphs = CGRect.null, box = CGRect.null, inked = 0 }
    private static func isGreen(_ r: UInt8, _ g: UInt8, _ b: UInt8) -> Bool { g > 150 && Int(g) - Int(r) > 70 && Int(g) - Int(b) > 70 }
    private static func isRed(_ r: UInt8, _ g: UInt8, _ b: UInt8) -> Bool { r > 150 && Int(r) - Int(g) > 80 && Int(r) - Int(b) > 80 }
    private func isGreen(_ pixels: Pixels, _ x: CGFloat, _ y: CGFloat) -> Bool {
        let i = (Int(y * 2) * pixels.width + Int(x * 2)) * 4
        return Self.isGreen(pixels.bytes[i], pixels.bytes[i + 1], pixels.bytes[i + 2])
    }
    private func ink(_ pixels: Pixels, in area: CGRect) -> Ink {
        var ink = Ink()
        for y in Int(area.minY * 2)..<Int(area.maxY * 2) {
            for x in Int(area.minX * 2)..<Int(area.maxX * 2) {
                let i = (y * pixels.width + x) * 4
                let (r, g, b) = (pixels.bytes[i], pixels.bytes[i + 1], pixels.bytes[i + 2])
                let point = CGRect(x: CGFloat(x) / 2 - area.minX, y: CGFloat(y) / 2 - area.minY, width: 0.5, height: 0.5)
                if Self.isRed(r, g, b) { ink.glyphs = ink.glyphs.union(point); ink.inked += 1 }
                if Self.isGreen(r, g, b) { ink.box = ink.box.union(point) }
            }
        }
        return ink
    }

    /// `rolled` draws as `drawn` does: its box and its glyphs in the same
    /// places, and as many glyph pixels give or take a tenth (other words,
    /// or a whole line where the line is cut, would not be).
    private func same(_ rolled: Ink, _ drawn: Ink, _ name: String) {
        XCTAssertFalse(drawn.glyphs.isNull, name)
        XCTAssertEqual(rolled.box.isNull, drawn.box.isNull, "\(name): a box where Exact draws one")
        for (what, got, want) in [("box", rolled.box, drawn.box), ("glyphs", rolled.glyphs, drawn.glyphs)] where !want.isNull {
            XCTAssertEqual(got.minX, want.minX, accuracy: 2, "\(name): the \(what)'s left")
            XCTAssertEqual(got.maxX, want.maxX, accuracy: 2, "\(name): the \(what)'s right")
            XCTAssertEqual(got.minY, want.minY, accuracy: 2, "\(name): the \(what)'s top")
            XCTAssertEqual(got.maxY, want.maxY, accuracy: 2, "\(name): the \(what)'s bottom")
        }
        XCTAssertEqual(Double(rolled.inked), Double(drawn.inked), accuracy: Double(drawn.inked) / 10, "\(name): the glyphs' ink")
    }

    /// The rolling paragraph's digits and its run's box land where Exact
    /// lays them out: at the line's baseline in a box taller than the line,
    /// moved by a flex text's offset and by `text-indent`, and in a
    /// right-to-left line. A soft hyphen on one line draws nothing. The box
    /// keeps its radius. A line `text-overflow` or `line-clamp` cuts, and a
    /// paragraph with an image in its text, are not the capability's: SwiftUI's
    /// `Text` cuts nothing and holds no room for an image. Exact draws them.
    func testTheRollingTextAndItsRunsBoxSitWhereExactLaysThemOut() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "SwiftUI draws only in a scene's window") }
        let (p, window) = hosted("numerals-place", in: scene)
        let long = "12 volumes of the long report"
        let counted = { (count: String) in { (id: Int) in self.runs(id, count) } }
        // Each case: its name, the paragraph's style and props, its box, its runs, whether it rolls, and
        // what makes it the case it names, read off the built paragraph.
        let runs = { (n: NodeView) in n.paragraphSpec().runs }
        let cutByEllipsis = { (n: NodeView) -> Bool in
            guard let p = n.paragraphLayout(), let first = p.lines.first else { return false }
            return n.paragraphSpec().ellipsis && p.ellipsized(0, spec: n.paragraphSpec(), width: n.paragraphBox().width) !== first
        }
        let clamped = { (n: NodeView) in n.paragraphLayout()?.clampedRange != nil }
        let cases: [(String, [String: Any], [String: String], CGSize, (Int) -> [[String: Any]], Bool, (NodeView) -> Bool)] = [
            ("a box taller than its line", [:], [:], CGSize(width: 360, height: 80), counted("12 volumes"), true, { $0.bounds.height > ($0.paragraphLayout()?.height ?? 80) + 20 }),
            ("a flex text's line moved down", [:], ["textOffset": "40"], CGSize(width: 360, height: 80), counted("12 volumes"), true, { $0.paragraphBox().minY >= 40 }),
            ("text-indent", ["text_indent": 40.0], [:], CGSize(width: 360, height: 40), counted("12 volumes"), true, { $0.paragraphSpec().textIndent == 40 }),
            ("right to left", ["direction": "rtl"], [:], CGSize(width: 360, height: 40), counted("12 volumes"), true, { $0.paragraphSpec().direction == 1 }),
            // White space Exact keeps, which `Text` would place otherwise: the line is Exact's.
            ("right to left, a kept trailing space", ["direction": "rtl", "white_space": "pre"], [:], CGSize(width: 360, height: 40),
             { id in [["id": id + 1, "parent": id, "paint": true, "props": ["text": "12 "], "style": ["font_size": 20.0, "text_color": self.red]]] }, false,
             { $0.paragraphSpec().preserves && $0.paragraphSpec().direction == 1 }),
            ("right to left, a kept trailing space in a box", ["direction": "rtl", "white_space": "pre"], [:], CGSize(width: 360, height: 40),
             { id in [["id": id + 1, "parent": id, "paint": true, "props": ["text": "12 "],
                       "style": ["font_size": 20.0, "text_color": self.red, "background_color": self.green, "border_radius_top_left": 8.0,
                                 "border_radius_top_right": 8.0, "border_radius_bottom_right": 8.0, "border_radius_bottom_left": 8.0]]] }, false,
             { $0.paragraphSpec().preserves && $0.paragraphSpec().direction == 1 }),
            ("a kept trailing space, aligned to the end", ["white_space": "pre", "text_align": "end"], [:], CGSize(width: 360, height: 40),
             { id in self.runs(id, "12 volumes ") }, false, { $0.paragraphSpec().preserves && $0.paragraphSpec().align == 2 }),
            // Its box's rounded start a point left of the paragraph's box, which this view's drawing would cut.
            ("a box past the paragraph's edge", ["text_indent": -1.0], [:], CGSize(width: 360, height: 40),
             { id in [["id": id + 1, "parent": id, "paint": true, "props": ["text": "12 volumes"],
                       "style": ["font_size": 20.0, "text_color": self.red, "background_color": self.green, "border_radius_top_left": 8.0,
                                 "border_radius_top_right": 8.0, "border_radius_bottom_right": 8.0, "border_radius_bottom_left": 8.0]],
                      ["id": id + 2, "parent": id, "paint": true, "props": ["text": " today"], "style": ["font_size": 20.0, "text_color": self.red]]] }, false,
             { $0.paragraphSpec().textIndent == -1 }),
            ("a soft hyphen", [:], [:], CGSize(width: 360, height: 40), counted("12 vol\u{AD}umes"), true, { runs($0).contains { $0.text.contains("\u{AD}") } }),
            ("letter-spacing", [:], [:], CGSize(width: 360, height: 40), { id in self.runs(id, "12 volumes", style: ["letter_spacing": 3.0]) }, true, { runs($0).contains { $0.letterSpacing == 3 } }),
            ("an ellipsis", ["overflow_x": "hidden", "text_overflow": "ellipsis", "white_space": "nowrap"], [:], CGSize(width: 150, height: 40), counted("12 volumes"), false, cutByEllipsis),
            ("an ellipsis after a ligature", ["overflow_x": "hidden", "text_overflow": "ellipsis", "white_space": "nowrap"], [:], CGSize(width: 150, height: 40), counted("12 office report"), false, cutByEllipsis),
            ("line-clamp", ["line_clamp": 1.0], [:], CGSize(width: 200, height: 40), counted(long), false, clamped),
            ("line-clamp, centred", ["line_clamp": 1.0, "text_align": "center"], [:], CGSize(width: 200, height: 40), counted(long), false, clamped),
            // The first clipped character stays, with no room for "…".
            ("line-clamp with no room for its ellipsis", ["line_clamp": 1.0], [:], CGSize(width: 6, height: 40), counted(long), false, clamped),
            // The paragraph's own "…", in its 64 pt font, does not fit: the line keeps "12", with none.
            ("line-clamp whose ellipsis does not fit", ["line_clamp": 1.0, "white_space": "pre", "font_size": 64.0], [:], CGSize(width: 36, height: 80),
             { id in [["id": id + 1, "parent": id, "paint": true, "props": ["text": "12\n34"],
                       "style": ["font_size": 12.0, "text_color": self.red, "background_color": self.green]]] }, false, clamped),
            ("an image in the text", [:], [:], CGSize(width: 360, height: 40), { id in
                [["id": id + 3, "parent": id, "paint": true, "props": ["text": "\u{FFFC}", "inlineBox": "40 20 0 0"], "style": ["font_size": 20.0]]]
                    + self.runs(id, "12 volumes") }, false, { runs($0).contains { $0.box != nil } }),
        ]
        for (k, (name, style, props, size, made, rolls, named)) in cases.enumerated() {
            let one = 100 * (k + 1) + 1, two = one + 50
            var paragraph: [String: Any] = ["font_size": 20.0]
            paragraph.merge(style) { $1 }
            var rolling = paragraph
            rolling["content_transition"] = "numeric"
            p.apply(wireBatch([
                ["op": "create", "id": one, "kind": "text", "style": rolling, "props": props],
                ["op": "create", "id": two, "kind": "text", "style": paragraph, "props": props],
                ["op": "roots", "ids": [one, two]],
                ["op": "frame", "id": one, "x": 20.0, "y": 20.0, "w": Double(size.width), "h": Double(size.height)],
                ["op": "frame", "id": two, "x": 20.0, "y": 320.0, "w": Double(size.width), "h": Double(size.height)],
                ["op": "paragraph", "id": one, "runs": made(one)],
                ["op": "paragraph", "id": two, "runs": made(two)],
            ]))
            let a = try XCTUnwrap(p.views[UInt32(one)]), b = try XCTUnwrap(p.views[UInt32(two)])
            XCTAssertTrue(named(a), "\(name): the paragraph is the case it names")
            XCTAssertEqual(a.rollsNumerals, rolls, name)
            paint(p, [a, b])
            waitFor("\(name): the capability shows it or not", cap: 5) { capability(a) == rolls }
            // Past the box too: a whole line the box cuts would reach it.
            let top = CGRect(x: 0, y: 20, width: 400, height: size.height), bottom = top.offsetBy(dx: 0, dy: 300)
            let pixels = still(window, inked: [top, bottom])
            let rolled = ink(pixels, in: top), drawn = ink(pixels, in: bottom)
            same(rolled, drawn, name)
            if !rolled.box.isNull, rolled.box.width > 20 {
                // Its corner is outside its 8 pt radius; a point in from its left edge, at its middle, is inside.
                XCTAssertFalse(isGreen(pixels, rolled.box.minX + 0.5, 20 + rolled.box.minY + 0.5), "\(name): a rounded corner")
                XCTAssertTrue(isGreen(pixels, rolled.box.minX + 1, 20 + rolled.box.midY), "\(name): filled inside")
            }
            p.apply(wireBatch([["op": "destroy", "id": one], ["op": "destroy", "id": two], ["op": "roots", "ids": []]]))
        }
    }

    /// A visible run among hidden ones, in a hidden paragraph (on one line
    /// and wrapped) or in a visible one, paints its box and its glyphs and the
    /// hidden runs paint neither: the paragraph is Exact's, not the capability's.
    func testAVisibleRunInAHiddenParagraphKeepsItsBox() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "SwiftUI draws only in a scene's window") }
        let (p, window) = hosted("numerals-visibility", in: scene)
        // Several hidden runs between visible ones, in a paragraph that is not hidden.
        let several = { (id: Int) -> [[String: Any]] in
            ["Finished ", "12", " of ", "40", " volumes"].enumerated().map { i, text in
                var style: [String: Any] = ["font_size": 20.0, "text_color": self.red, "visibility": i % 2 == 0 ? "hidden" : "visible"]
                if i % 2 == 1 { style["background_color"] = self.green }
                return ["id": id + 1 + i, "parent": id, "paint": true, "props": ["text": text], "style": style]
            }
        }
        let cases: [(String, Double, Bool, (Int) -> [[String: Any]])] = [
            ("12 volumes", 360, true, { id in self.runs(id, "12 volumes", style: ["visibility": "hidden"], countStyle: ["visibility": "visible"]) }),
            ("12 volumes of the long report", 160, true, { id in self.runs(id, "12 volumes of the long report", style: ["visibility": "hidden"], countStyle: ["visibility": "visible"]) }),
            ("several hidden runs", 360, false, several),
        ]
        for (k, (count, width, hiddenParagraph, made)) in cases.enumerated() {
            let one = 100 * (k + 1) + 1, two = one + 50
            let hidden: [String: Any] = ["font_size": 20.0, "visibility": hiddenParagraph ? "hidden" : "visible"]
            var rolling = hidden
            rolling["content_transition"] = "numeric"
            p.apply(wireBatch([
                ["op": "create", "id": one, "kind": "text", "style": rolling],
                ["op": "create", "id": two, "kind": "text", "style": hidden],
                ["op": "roots", "ids": [one, two]],
                ["op": "frame", "id": one, "x": 20.0, "y": 20.0, "w": width, "h": 80.0],
                ["op": "frame", "id": two, "x": 20.0, "y": 320.0, "w": width, "h": 80.0],
                ["op": "paragraph", "id": one, "runs": made(one)],
                ["op": "paragraph", "id": two, "runs": made(two)],
            ]))
            let a = try XCTUnwrap(p.views[UInt32(one)]), b = try XCTUnwrap(p.views[UInt32(two)])
            XCTAssertTrue(a.paragraphSpec().runs.contains { $0.hidden }, "\(count): a hidden run")
            XCTAssertEqual(a.cssVisibilityHidden, hiddenParagraph, "\(count): the paragraph hidden or not")
            XCTAssertFalse(a.rollsNumerals, count)
            paint(p, [a, b])
            waitFor("\(count): no capability", cap: 5) { !capability(a) }
            paint(p, [a, b])
            let top = CGRect(x: 20, y: 20, width: 380, height: 80), bottom = top.offsetBy(dx: 0, dy: 300)
            let pixels = still(window, inked: [top, bottom])
            let rolled = ink(pixels, in: top), drawn = ink(pixels, in: bottom)
            same(rolled, drawn, count)
            XCTAssertTrue(rolled.box.insetBy(dx: -1, dy: -1).contains(rolled.glyphs), "\(count): no glyphs of the hidden run outside the box")
            p.apply(wireBatch([["op": "destroy", "id": one], ["op": "destroy", "id": two], ["op": "roots", "ids": []]]))
        }
    }

    /// Non-white pixels in `area` that are not the green box.
    private func stray(_ pixels: Pixels, in area: CGRect) -> Int {
        var n = 0
        for y in Int(area.minY * 2)..<Int(area.maxY * 2) {
            for x in Int(area.minX * 2)..<Int(area.maxX * 2) {
                let i = (y * pixels.width + x) * 4
                let (r, g, b) = (pixels.bytes[i], pixels.bytes[i + 1], pixels.bytes[i + 2])
                if !Self.isGreen(r, g, b) && Int(r) + Int(g) + Int(b) < 600 { n += 1 }
            }
        }
        return n
    }

    /// A paragraph with a hidden run is drawn by Exact, which skips the run,
    /// and does not take the capability: SwiftUI's `Text` would paint it
    /// (its stroke, the paragraph's line through it, a colour glyph's own
    /// colours). Nor does one with paint `Text` does not draw. A run in a
    /// transparent colour is not hidden: it rolls, and draws as Exact draws
    /// it (nothing, or a colour glyph in its colours), and so does a shadow.
    func testAParagraphWithAHiddenRunIsExacts() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "SwiftUI draws only in a scene's window") }
        let (p, window) = hosted("numerals-hidden-paint", in: scene)
        let clear: [Double] = [255, 0, 0, 0]
        // Each: its name, the paragraph's style, the first run's words and style, the count's own
        // style, whether it rolls, and what makes it the case it names, read off the built paragraph.
        let runs = { (n: NodeView) in n.paragraphSpec().runs }
        let hidden = { (n: NodeView) in runs(n).contains { $0.hidden } }
        let shadowed: [String: Any] = ["text_shadow": ["o": [3.0, 3.0], "b": 0.0, "c": [0.0, 0.0, 255.0, 255.0]]]
        let cases: [(String, [String: Any], String, [String: Any], [String: Any], Bool, (NodeView) -> Bool)] = [
            ("hidden words", [:], "Finished ", ["visibility": "hidden"], [:], false, hidden),
            ("a hidden stroke", [:], "Finished ", ["visibility": "hidden", "text_stroke_width": 2.0, "text_stroke_color": [0.0, 0.0, 255.0, 255.0]], [:], false, { hidden($0) && runs($0).contains { $0.stroke != nil } }),
            ("an underline under a hidden run", ["text_decoration_line": "underline"], "Finished ", ["visibility": "hidden"], [:], false, { hidden($0) && runs($0).contains { $0.hidden && $0.underlineColor != nil } }),
            ("a line through a hidden run", ["text_decoration_line": "line-through"], "Finished ", ["visibility": "hidden"], [:], false, { hidden($0) && runs($0).contains { $0.hidden && $0.strikeColor != nil } }),
            ("a hidden colour glyph", [:], "\u{1F44D} Finished ", ["visibility": "hidden"], [:], false, { runs($0).contains { $0.hidden && $0.text.contains("\u{1F44D}") } }),
            ("a collapsed run", [:], "Finished ", ["visibility": "collapse"], [:], false, hidden),
            // Paint `Text` does not draw: a fill through the glyphs, a stroke, a shadow, the
            // paragraph's (the same on every run) or one run's.
            ("a fill through transparent glyphs", ["background_clip": "text", "background_color": red], "Finished ", ["text_color": clear], [:], false, { $0.backgroundClip == "text" }),
            ("a stroke", [:], "Finished ", ["text_stroke_width": 2.0, "text_stroke_color": [0.0, 0.0, 255.0, 255.0]], [:], false, { runs($0).contains { $0.stroke != nil } }),
            ("a shared text shadow", [:], "Finished ", shadowed, shadowed, false, {
                let spec = $0.paragraphSpec()
                return spec.shadow != nil || spec.runs.allSatisfy { $0.shadow != nil }
            }),
            ("a run's own text shadow", [:], "Finished ", shadowed, [:], false, { $0.paragraphSpec().shadow == nil && runs($0).contains { $0.shadow != nil } }),
            ("transparent words", [:], "Finished ", ["text_color": clear], [:], true, { runs($0).contains { ($0.color?.last ?? 1) == 0 } }),
            ("a transparent colour glyph", [:], "\u{1F44D} Finished ", ["text_color": clear], [:], true, { runs($0).contains { ($0.color?.last ?? 1) == 0 && $0.text.contains("\u{1F44D}") } }),
        ]
        for (k, (name, paragraphStyle, words, firstStyle, countStyle, rolls, named)) in cases.enumerated() {
            let one = 100 * (k + 1) + 1, two = one + 50
            var plain: [String: Any] = ["font_size": 20.0]
            plain.merge(paragraphStyle) { $1 }
            var rolling = plain
            rolling["content_transition"] = "numeric"
            var first: [String: Any] = ["font_size": 20.0, "text_color": red]
            first.merge(firstStyle) { $1 }
            var count: [String: Any] = ["font_size": 20.0, "text_color": red, "background_color": green]
            count.merge(countStyle) { $1 }
            let made = { (id: Int) -> [[String: Any]] in
                [["id": id + 1, "parent": id, "paint": true, "props": ["text": words], "style": first],
                 ["id": id + 2, "parent": id, "paint": true, "props": ["text": "12 volumes"], "style": count]]
            }
            p.apply(wireBatch([
                ["op": "create", "id": one, "kind": "text", "style": rolling],
                ["op": "create", "id": two, "kind": "text", "style": plain],
                ["op": "roots", "ids": [one, two]],
                ["op": "frame", "id": one, "x": 20.0, "y": 20.0, "w": 360.0, "h": 40.0],
                ["op": "frame", "id": two, "x": 20.0, "y": 320.0, "w": 360.0, "h": 40.0],
                ["op": "paragraph", "id": one, "runs": made(one)],
                ["op": "paragraph", "id": two, "runs": made(two)],
            ]))
            let a = try XCTUnwrap(p.views[UInt32(one)]), b = try XCTUnwrap(p.views[UInt32(two)])
            XCTAssertTrue(named(a), "\(name): the paragraph is the case it names")
            XCTAssertEqual(a.rollsNumerals, rolls, name)
            paint(p, [a, b])
            waitFor("\(name): the capability shows it or not", cap: 5) { capability(a) == rolls }
            let top = CGRect(x: 0, y: 20, width: 400, height: 40), bottom = top.offsetBy(dx: 0, dy: 300)
            let pixels = still(window, inked: [top, bottom], name)
            same(ink(pixels, in: top), ink(pixels, in: bottom), name)
            // Left of the count's box only the first run is: it draws there as Exact draws it.
            let left = CGRect(x: 0, y: 20, width: max(1, ink(pixels, in: bottom).box.minX - 2), height: 40)
            XCTAssertEqual(Double(stray(pixels, in: left)), Double(stray(pixels, in: left.offsetBy(dx: 0, dy: 300))),
                           accuracy: Double(stray(pixels, in: left.offsetBy(dx: 0, dy: 300))) / 10 + 2, "\(name): the first run")
            p.apply(wireBatch([["op": "destroy", "id": one], ["op": "destroy", "id": two], ["op": "roots", "ids": []]]))
        }
    }

    /// A paragraph whose ink needs what only Exact's painter does is Exact's:
    /// a column fragment, whose line is clipped to its column and leaves the
    /// gap empty; a line past its box, whose raster keeps the ink beyond it;
    /// a line height tighter than its pill.
    func testAParagraphPaintedPastWhatTheCapabilityDrawsIsExacts() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "SwiftUI draws only in a scene's window") }
        let (p, window) = hosted("numerals-structure", in: scene)
        let nowrap: [String: Any] = ["white_space": "nowrap"]
        // Each: its name, the paragraph's style, its box, any op after its runs, and what makes it the case it names.
        let cases: [(String, [String: Any], CGSize, (Int) -> [[String: Any]], (NodeView) -> Bool)] = [
            ("a column fragment", nowrap, CGSize(width: 200, height: 40),
             { id in [["op": "fragments", "id": id, "fragments": [[0.0, 0.0, 80.0, 40.0, 0.0, 1.0, 0.0, 0.0]]]] }, { $0.columnRecord != nil }),
            ("a line past its box", nowrap, CGSize(width: 60, height: 40), { _ in [] }, { $0.paragraphLayout()!.width > $0.bounds.width }),
            ("a line height tighter than its pill", ["line_height": "12px"], CGSize(width: 360, height: 12), { _ in [] }, { $0.bounds.height < 20 }),
        ]
        for (k, (name, style, size, after, named)) in cases.enumerated() {
            let one = 100 * (k + 1) + 1, two = one + 50
            var plain: [String: Any] = ["font_size": 20.0]
            plain.merge(style) { $1 }
            var rolling = plain
            rolling["content_transition"] = "numeric"
            p.apply(wireBatch([
                ["op": "create", "id": one, "kind": "text", "style": rolling],
                ["op": "create", "id": two, "kind": "text", "style": plain],
                ["op": "roots", "ids": [one, two]],
                ["op": "frame", "id": one, "x": 20.0, "y": 40.0, "w": Double(size.width), "h": Double(size.height)],
                ["op": "frame", "id": two, "x": 20.0, "y": 340.0, "w": Double(size.width), "h": Double(size.height)],
                ["op": "paragraph", "id": one, "runs": runs(one, "12 volumes")],
                ["op": "paragraph", "id": two, "runs": runs(two, "12 volumes")],
            ] + after(one) + after(two)))
            let a = try XCTUnwrap(p.views[UInt32(one)]), b = try XCTUnwrap(p.views[UInt32(two)])
            XCTAssertTrue(named(a), "\(name): the paragraph is the case it names")
            XCTAssertFalse(a.rollsNumerals, name)
            paint(p, [a, b])
            waitFor("\(name): no capability", cap: 5) { !capability(a) }
            // The whole row, past the box on every side.
            let top = CGRect(x: 0, y: 20, width: 400, height: 80), bottom = top.offsetBy(dx: 0, dy: 300)
            let pixels = still(window, inked: [top, bottom], name)
            same(ink(pixels, in: top), ink(pixels, in: bottom), name)
            p.apply(wireBatch([["op": "destroy", "id": one], ["op": "destroy", "id": two], ["op": "roots", "ids": []]]))
        }
    }

    /// The issue's own sentence, its box exactly its line as layout sizes a
    /// text to its content, rolls: its glyphs and its count's box lie within.
    func testAContentSizedSentenceRolls() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "SwiftUI draws only in a scene's window") }
        let (p, _) = hosted("numerals-content-sized", in: scene)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "text", "style": ["font_size": 20.0, "content_transition": "numeric"]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 20.0, "y": 20.0, "w": 360.0, "h": 40.0],
            ["op": "paragraph", "id": 1, "runs": runs(1, "12 volumes") + [["id": 4, "parent": 1, "paint": true, "props": ["text": " today."], "style": ["font_size": 20.0]]]],
        ]))
        let node = try XCTUnwrap(p.views[1]), laid = try XCTUnwrap(node.paragraphLayout())
        p.apply(wireBatch([["op": "frame", "id": 1, "x": 20.0, "y": 20.0, "w": Double(ceil(laid.width)), "h": Double(ceil(laid.height))]]))
        XCTAssertEqual(try XCTUnwrap(node.paragraphLayout()).lines.count, 1)
        XCTAssertTrue(node.rollsNumerals, "its line fits its box: it rolls")
    }

    /// A batch that sets only a text's words shows the new words, in a
    /// paragraph that rolls and in one that does not (its line is cut).
    func testATextOnlyUpdateShowsTheNewWords() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "SwiftUI draws only in a scene's window") }
        let (p, window) = hosted("numerals-words", in: scene)
        let cut: [String: Any] = ["overflow_x": "hidden", "text_overflow": "ellipsis", "white_space": "nowrap"]
        for (k, (name, style, width, rolls)) in [("one that rolls", [String: Any](), 360.0, true), ("one cut, which does not", cut, 120.0, false)].enumerated() {
            let one = 100 * (k + 1) + 1, two = one + 50
            var plain: [String: Any] = ["font_size": 20.0, "text_color": red]
            plain.merge(style) { $1 }
            var rolling = plain
            rolling["content_transition"] = "numeric"
            let words = k == 0 ? "1234567" : "1234567 890 1234", fresh = one + 25
            // `fresh` is shown the new words from the start, `one` is given them by a batch of words alone.
            p.apply(wireBatch([
                ["op": "create", "id": one, "kind": "text", "props": ["text": "12"], "style": rolling],
                ["op": "create", "id": fresh, "kind": "text", "props": ["text": words], "style": rolling],
                ["op": "create", "id": two, "kind": "text", "props": ["text": "12"], "style": plain],
                ["op": "roots", "ids": [one, fresh, two]],
                ["op": "frame", "id": one, "x": 20.0, "y": 20.0, "w": width, "h": 40.0],
                ["op": "frame", "id": fresh, "x": 20.0, "y": 170.0, "w": width, "h": 40.0],
                ["op": "frame", "id": two, "x": 20.0, "y": 320.0, "w": width, "h": 40.0],
            ]))
            let a = try XCTUnwrap(p.views[UInt32(one)]), b = try XCTUnwrap(p.views[UInt32(two)]), c = try XCTUnwrap(p.views[UInt32(fresh)])
            paint(p, [a, b, c])
            let top = CGRect(x: 0, y: 20, width: 400, height: 40), middle = top.offsetBy(dx: 0, dy: 150), bottom = top.offsetBy(dx: 0, dy: 300)
            _ = still(window, inked: [top, middle, bottom], name)
            p.apply(wireBatch([["op": "props", "id": one, "set": ["text": words]]]))
            p.apply(wireBatch([["op": "props", "id": two, "set": ["text": words]]]))
            XCTAssertEqual(a.paragraphSpec().runs.map(\.text).joined(), words, "\(name): the spec reads the new words")
            XCTAssertEqual(a.rollsNumerals, rolls, name)
            paint(p, [a, b])
            // The roll takes its time: wait for the new words, as long as a roll.
            waitFor("\(name): the new words shown", cap: 5) {
                guard let pixels = try? shot(window) else { return false }
                let rolled = ink(pixels, in: top), drawn = ink(pixels, in: bottom)
                return !drawn.glyphs.isNull && abs(rolled.glyphs.maxX - drawn.glyphs.maxX) <= 2
            }
            let pixels = still(window, inked: [top, middle, bottom], name)
            // As the paragraph given these words from the start draws them.
            same(ink(pixels, in: top), ink(pixels, in: middle), name)
            p.apply(wireBatch([["op": "destroy", "id": one], ["op": "destroy", "id": fresh], ["op": "destroy", "id": two], ["op": "roots", "ids": []]]))
        }
    }

    /// A paragraph that breaks into lines is drawn by Exact, which wraps it;
    /// a raster rolls it whole. One that comes back to one line rolls in the
    /// capability, and the other way, with no change to the box. One that
    /// wraps and cannot raster (`background-clip: text`) is drawn, not rolled.
    func testAParagraphMovesBetweenRasterAndCapabilityAsItsLinesChange() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "SwiftUI draws only in a scene's window") }
        let (p, window) = hosted("numerals-lines", in: scene)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "text", "style": ["font_size": 20.0, "content_transition": "numeric"]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 20.0, "y": 20.0, "w": 220.0, "h": 60.0],
            ["op": "paragraph", "id": 1, "runs": runs(1, "12 volumes of the long report")],
        ]))
        let node = try XCTUnwrap(p.views[1])
        func shows(_ rolling: Bool, _ what: String) {
            paint(p, [node])
            waitFor(what, cap: 5) { capability(node) == rolling && (node.textRasterLayer == nil) == rolling }
            XCTAssertEqual(node.rollsNumerals, rolling, what)
        }
        XCTAssertGreaterThan(try XCTUnwrap(node.paragraphLayout()).lines.count, 1)
        shows(false, "two lines: the raster")
        p.apply(wireBatch([["op": "paragraph", "id": 1, "runs": runs(1, "13 volumes")]]))
        shows(true, "one line: the capability, and no raster")
        p.apply(wireBatch([["op": "paragraph", "id": 1, "runs": runs(1, "14 volumes of the long report")]]))
        shows(false, "two lines again: the raster, and no capability")
        // Drawn as the same paragraph without the transition is.
        p.apply(wireBatch([
            ["op": "style", "id": 1, "style": ["font_size": 20.0, "content_transition": "numeric", "background_clip": "text"]],
            ["op": "create", "id": 50, "kind": "text", "style": ["font_size": 20.0, "background_clip": "text"]],
            ["op": "roots", "ids": [1, 50]],
            ["op": "frame", "id": 50, "x": 20.0, "y": 320.0, "w": 220.0, "h": 60.0],
            ["op": "paragraph", "id": 50, "runs": runs(50, "14 volumes of the long report")],
        ]))
        let twin = try XCTUnwrap(p.views[50])
        XCTAssertEqual(node.backgroundClip, "text")
        XCTAssertFalse(node.rollsNumerals, "two lines that cannot raster")
        XCTAssertFalse(node.canRasterText)
        paint(p, [node, twin])
        waitFor("the capability let go", cap: 5) { !capability(node) }
        let top = CGRect(x: 0, y: 20, width: 400, height: 60), bottom = top.offsetBy(dx: 0, dy: 300)
        let pixels = still(window, inked: [top, bottom])
        same(ink(pixels, in: top), ink(pixels, in: bottom), "two lines that cannot raster: drawn")
    }
}
#endif
