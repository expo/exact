import XCTest
@testable import ExactKit

class SvgHitTests: XCTestCase {
    private let identity = [1, 0, 0, 1, 0, 0]
    private let red = [239, 68, 68, 255]

    private func rect(_ x: Int = 10, _ y: Int = 5, _ w: Int = 100, _ h: Int = 50) -> [Int] {
        [0, x, y, 1, x + w, y, 1, x + w, y + h, 1, x, y + h, 3]
    }

    private func shape(_ changes: [String: Any] = [:]) -> [String: Any] {
        ["id": 1, "n": 7, "h": 1, "o": 1, "p": rect(), "f": red,
         "w": 12, "ml": 4].merging(changes) { _, new in new }
    }

    private func gradient(radial: Bool = false) -> [String: Any] {
        var g: [String: Any] = ["t": identity, "st": [[0, red], [1, [255, 255, 0, 255]]]]
        if radial { g["rg"] = [0, 0, 0, 0, 0, 25] } else { g["lg"] = [10, 5, 110, 55] }
        return g
    }

    private func filtered(_ content: [String: Any], shadow: Bool = false, region: [Int] = [0, 0, 120, 60]) -> [String: Any] {
        let program = shadow
            ? region + [1, 6, -1, -3] + region + [0, 1, 1, 2, 2, 0, 0, 0, 1]
            : region + [1, 0, -1, -3] + region + [0, 1, 1]
        return ["id": 2, "n": 8, "g": 1, "o": 1, "c": [Any](),
                "fl": ["r": region, "k": 1, "p": program, "c": [content]]]
    }

    private func apply(_ elements: [[String: Any]], to svg: SvgScene) {
        svg.apply(["box": [0, 0, 120, 60], "t": identity, "els": elements], dark: false, clock: nil)
    }

    func testPaintServersMasksAndFiltersKeepTheShapesPress() {
        let host = CALayer(), svg = SvgScene()
        host.addSublayer(svg.root)
        let tile = shape(["p": rect(0, 0, 4, 4)])
        let pattern: [String: Any] = ["pt": [0, 0, 8, 8], "t": identity, "o": 1, "c": [tile]]
        let mask: [String: Any] = ["r": [0, 0, 120, 60], "k": 1, "t": identity, "l": 1,
                                   "c": [shape(["p": rect(0, 0, 120, 60), "f": [255, 255, 255, 255]])]]
        let circle = PreparedPath([0, 25, 0, 2, 25, 13.807, 13.807, 25, 0, 25,
                                   2, -13.807, 25, -25, 13.807, -25, 0,
                                   2, -25, -13.807, -13.807, -25, 0, -25,
                                   2, 13.807, -25, 25, -13.807, 25, 0, 3])
        let cases: [(String, [String: Any], CGPoint)] = [
            ("solid", shape(), CGPoint(x: 60, y: 30)),
            ("pattern", shape(["f": pattern]), CGPoint(x: 60, y: 30)),
            ("gradient", shape(["f": gradient()]), CGPoint(x: 60, y: 30)),
            ("disc", shape(["p": circle, "pos": [60, 30], "f": gradient(radial: true)]), CGPoint(x: 60, y: 30)),
            ("stroke", shape(["p": rect(20, 15, 80, 30), "f": NSNull(), "s": gradient()]), CGPoint(x: 20, y: 30)),
            ("masked", shape(["mk": mask]), CGPoint(x: 60, y: 30)),
            ("blurred", filtered(shape()), CGPoint(x: 60, y: 30)),
            ("clipped", shape(["cl": ["s": [[rect(30, 10, 60, 40), 0]]]]), CGPoint(x: 60, y: 30)),
            ("shadowed", filtered(shape(), shadow: true), CGPoint(x: 60, y: 30))
        ]
        for (name, element, point) in cases {
            apply([element], to: svg)
            XCTAssertEqual(svg.target(at: point), 7, name)
            XCTAssertNil(svg.target(at: CGPoint(x: 2, y: 2)), "\(name): outside the shape")
        }
        svg.reset()
    }

    func testServerPaintUsesTheFillRuleAndDashedStrokeOutline() {
        let host = CALayer(), svg = SvgScene()
        host.addSublayer(svg.root)
        let hole = shape(["p": rect() + rect(40, 20, 40, 20), "rule": 1, "f": gradient()])
        apply([hole], to: svg)
        XCTAssertEqual(svg.target(at: CGPoint(x: 20, y: 30)), 7)
        XCTAssertNil(svg.target(at: CGPoint(x: 60, y: 30)))

        let stroke = shape(["p": [0, 10, 30, 1, 110, 30], "f": NSNull(), "s": gradient(),
                            "w": 10, "dash": [10, 10], "cap": 0])
        apply([stroke], to: svg)
        XCTAssertEqual(svg.target(at: CGPoint(x: 15, y: 34)), 7)
        XCTAssertNil(svg.target(at: CGPoint(x: 25, y: 30)), "a dash gap")
        XCTAssertNil(svg.target(at: CGPoint(x: 15, y: 36)), "outside the stroke")
        XCTAssertNil(svg.target(at: CGPoint(x: 8, y: 30)), "a butt cap")
        svg.reset()
    }

    func testHitsFollowThePaintsTheSceneSaysTakeThem() {
        let host = CALayer(), svg = SvgScene()
        host.addSublayer(svg.root)
        // An empty gradient: the fill draws nothing but is set.
        apply([shape(["f": NSNull(), "hp": [1, 0]])], to: svg)
        XCTAssertEqual(svg.target(at: CGPoint(x: 60, y: 30)), 7, "a set fill that draws nothing")
        apply([shape(["hp": [0, 1]])], to: svg)
        XCTAssertNil(svg.target(at: CGPoint(x: 60, y: 30)), "visibleStroke: not the fill")
        XCTAssertEqual(svg.target(at: CGPoint(x: 12, y: 30)), 7, "visibleStroke: the stroke, unpainted")
        apply([shape(["hp": [0, 0]])], to: svg)
        XCTAssertNil(svg.target(at: CGPoint(x: 60, y: 30)), "hidden")
        svg.reset()
    }

    func testMasksIgnoreCoverageButKeepIntersectingClips() {
        let host = CALayer(), svg = SvgScene()
        host.addSublayer(svg.root)
        let mask: [String: Any] = ["r": [0, 0, 1, 1], "k": 1, "t": identity, "l": 1, "c": [Any]()]
        let clip: [String: Any] = ["s": [[rect(30, 0, 60, 60), 0]],
                                   "n": ["s": [[rect(0, 20, 120, 20), 0]]]]
        apply([shape(["mk": mask, "cl": clip, "o": 0])], to: svg)
        XCTAssertEqual(svg.target(at: CGPoint(x: 60, y: 30)), 7, "neither mask coverage nor opacity limits hits")
        XCTAssertNil(svg.target(at: CGPoint(x: 20, y: 30)))
        XCTAssertNil(svg.target(at: CGPoint(x: 60, y: 10)))
        svg.reset()
    }

    func testFiltersKeepDescendantTargetsAndIgnoreTheirPaintRegion() {
        let host = CALayer(), svg = SvgScene()
        host.addSublayer(svg.root)
        let back = shape(["id": 3, "n": 9])
        let front = shape(["f": gradient(), "p": rect(0, 0, 20, 20)])
        let group: [String: Any] = ["id": 4, "n": 10, "h": 1, "g": 1, "o": 0,
                                    "tf": ["o": [0, 0], "i": identity, "m": [1, 0, 0, 1, 40, 20]], "c": [front]]
        for shadow in [false, true] {
            apply([back, filtered(group, shadow: shadow, region: [0, 0, 5, 5])], to: svg)
            XCTAssertEqual(svg.target(at: CGPoint(x: 50, y: 30)), 7, "the child takes its press outside the filter region")
            XCTAssertEqual(svg.target(at: CGPoint(x: 20, y: 30)), 9, "outside the filtered geometry")
            var bubbling = group
            var child = front
            child.removeValue(forKey: "h")
            bubbling["c"] = [child]
            apply([back, filtered(bubbling, shadow: shadow)], to: svg)
            XCTAssertEqual(svg.target(at: CGPoint(x: 50, y: 30)), 10, "the press reaches its nearest handler")
            child["pn"] = 1
            bubbling["c"] = [child]
            apply([back, filtered(bubbling, shadow: shadow)], to: svg)
            XCTAssertEqual(svg.target(at: CGPoint(x: 50, y: 30)), 9, "pointer-events none passes through")
        }
        apply([back], to: svg)
        XCTAssertEqual(svg.target(at: CGPoint(x: 50, y: 30)), 9, "removed filter content takes no press")
        svg.reset()
        XCTAssertNil(svg.target(at: CGPoint(x: 50, y: 30)))
    }

    func testNestedFiltersKeepClipsAndRemovingAFilterRestoresServerPaint() throws {
        let host = CALayer(), svg = SvgScene()
        host.addSublayer(svg.root)
        let front = shape(["f": gradient()])
        var inner = filtered(front)
        inner["id"] = 5
        inner["cl"] = ["s": [[rect(30, 10, 60, 40), 0]]]
        inner["mk"] = ["r": [0, 0, 1, 1], "k": 1, "t": identity, "c": [Any]()]
        apply([filtered(inner)], to: svg)
        XCTAssertEqual(svg.target(at: CGPoint(x: 60, y: 30)), 7)
        XCTAssertNil(svg.target(at: CGPoint(x: 20, y: 30)))

        apply([front], to: svg)
        XCTAssertEqual(svg.target(at: CGPoint(x: 20, y: 30)), 7)
        let ctx = try XCTUnwrap(CGContext(data: nil, width: 120, height: 60, bitsPerComponent: 8,
                                          bytesPerRow: 480, space: CGColorSpaceCreateDeviceRGB(),
                                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        svg.root.render(in: ctx)
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        XCTAssertEqual(bytes[30 * 480 + 60 * 4 + 3], 255, "the unfiltered gradient paints again")
        svg.reset()
    }
}
