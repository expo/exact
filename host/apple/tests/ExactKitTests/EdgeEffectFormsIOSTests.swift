#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1123 D14 on iOS: a scroller's edge effect per edge, a style and a visibility each, as UIKit holds them, from
/// `-exact-scroll-edge-effect-style` and `-exact-scroll-edge-effect-hidden`; until activation the retired
/// `-exact-scroll-edge-effect` gives each aspect the new forms do not set (§"What it replaces"). A hidden edge has
/// none of LLP 1131's control bars, and the retired form's `none` hides both edges, so neither has any.
final class EdgeEffectFormsIOSTests: XCTestCase {
    private let soft = "scroll_edge_effect_soft", hard = "scroll_edge_effect_hard", hidden = "scroll_edge_effect_hidden"
    // The `<edges>` bits (D13, D14)
    private let top = 1, right = 2, bottom = 4, left = 8, start = 16, end = 32

    /// A 300-by-400 scroller with `style`, which holds a form's rows only while it is set, as the Apple host sends them.
    private func scroller(_ style: [String: Any], kind: String = "scroll", overflow: String = "auto") -> (Presenter, NodeView) {
        let p = scrollers([style], kind: kind, overflow: overflow)
        return (p, p.views[2]!)
    }

    /// One presenter holding a scroller per style, ids 2 up, side by side.
    private func scrollers(_ styles: [[String: Any]], kind: String = "scroll", overflow: String = "auto") -> Presenter {
        let p = Presenter()
        let ids = styles.indices.map { $0 + 2 }
        p.apply(wireBatch([["op": "create", "id": 1, "kind": "view"]]
            + zip(ids, styles).flatMap { id, style -> [[String: Any]] in
                [["op": "create", "id": id, "kind": kind, "props": ["testId": "scroller-\(id)"], "style": scrolling(style, overflow)],
                 ["op": "frame", "id": id, "x": 300 * (id - 2), "y": 0, "w": 300, "h": 400]]
            }
            + [["op": "children", "id": 1, "ids": ids], ["op": "roots", "ids": [1]],
               ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300 * styles.count, "h": 400]]))
        return p
    }

    private func scrolling(_ style: [String: Any], _ overflow: String) -> [String: Any] {
        style.merging(["overflow_y": overflow]) { a, _ in a }
    }

    /// Each edge's effect, top, left, bottom and right, as UIKit holds it: its style, after `hidden` where it is hidden.
    @available(iOS 26, *)
    private func effects(_ v: NodeView, file: StaticString = #filePath, line: UInt = #line) throws -> [String] {
        let sv = try XCTUnwrap(v.scrollView, "a scroll view", file: file, line: line)
        return [sv.topEdgeEffect, sv.leftEdgeEffect, sv.bottomEdgeEffect, sv.rightEdgeEffect].map { e in
            let style = e.style == .soft ? "soft" : e.style == .hard ? "hard" : "automatic"
            return e.isHidden ? "hidden " + style : style
        }
    }

    /// Each edge takes the style its item names and its own visibility, whatever the retired form says, and an edge the
    /// style form leaves out is `automatic`; an aspect the new forms do not set follows the retired form, whose word
    /// styles every edge and whose `none` hides every edge and leaves them `automatic`. `start` and `end` follow the
    /// scroller's direction: an edge one set names by `start` or `end` and the other by `left` or `right` takes the
    /// style of the set that names it by `start` or `end`, in either direction and whichever set that is; an edge both
    /// sets name alike, physically or by `start` or `end`, is hard.
    func testEachEdgeTakesItsStyleAndVisibilityFromTheFormThatSetsIt() throws {
        guard #available(iOS 26, *) else { throw XCTSkip("UIScrollEdgeEffect needs iOS 26") }
        let logical: [String: Any] = [soft: start, hard: left, hidden: end]
        let cases: [(style: [String: Any], effects: [String])] = [
            (["scroll_edge_effect": "none", soft: top, hard: bottom, hidden: 0], ["soft", "automatic", "hard", "automatic"]),
            (["scroll_edge_effect": "soft", soft: 0, hard: left | right, hidden: top], ["hidden automatic", "hard", "automatic", "hard"]),
            (["scroll_edge_effect": "soft", hidden: top], ["hidden soft", "soft", "soft", "soft"]),
            (["scroll_edge_effect": "none", soft: left, hard: 0], ["hidden automatic", "hidden soft", "hidden automatic", "hidden automatic"]),
            (["scroll_edge_effect": "hard"], ["hard", "hard", "hard", "hard"]),
            ([:], ["automatic", "automatic", "automatic", "automatic"]),
            (logical, ["automatic", "soft", "automatic", "hidden automatic"]),
            (logical.merging(["direction": "rtl"]) { a, _ in a }, ["automatic", "hidden hard", "automatic", "soft"]),
            ([soft: left, hard: start], ["automatic", "hard", "automatic", "automatic"]),
            ([soft: start, hard: right, "direction": "rtl"], ["automatic", "automatic", "automatic", "soft"]),
            ([soft: top, hard: top], ["hard", "automatic", "automatic", "automatic"]),
            ([soft: start, hard: start], ["automatic", "hard", "automatic", "automatic"]),
        ]
        let p = scrollers(cases.map(\.style))
        for (i, c) in cases.enumerated() {
            XCTAssertEqual(try effects(try XCTUnwrap(p.views[UInt32(i + 2)])), c.effects, "case \(i): \(c.style)")
        }
    }

    /// A restyle moves each edge, a cleared form giving its aspect back to the retired one, and a box that comes to scroll
    /// takes its effects with its new scroll view, as does a swipe's scroller, whose scroll view waits until it is needed.
    func testARestyleMovesEachEdgeAndANewScrollViewTakesThem() throws {
        guard #available(iOS 26, *) else { throw XCTSkip("UIScrollEdgeEffect needs iOS 26") }
        let (p, v) = scroller(["scroll_edge_effect": "none", hidden: bottom])
        XCTAssertEqual(try effects(v), ["automatic", "automatic", "hidden automatic", "automatic"])
        p.apply(wireBatch([["op": "style", "id": 2, "style": scrolling(["scroll_edge_effect": "none"], "auto")]]))
        XCTAssertEqual(try effects(v), ["hidden automatic", "hidden automatic", "hidden automatic", "hidden automatic"])
        let (q, w) = scroller([soft: top, hidden: right], kind: "view", overflow: "visible")
        XCTAssertNil(w.scrollView, "no scroll view while it does not scroll")
        q.apply(wireBatch([["op": "style", "id": 2, "style": scrolling([soft: top, hidden: right], "auto")]]))
        XCTAssertEqual(try effects(w), ["soft", "automatic", "automatic", "hidden automatic"])
        let (r, d) = scroller([:], kind: "view", overflow: "visible")
        d.swipeOwner = true
        r.apply(wireBatch([["op": "style", "id": 2, "style": scrolling([hidden: top, hard: bottom], "auto")]]))
        XCTAssertTrue(d.scrollDormant, "its scroll view waits")
        d.needScroll()
        XCTAssertEqual(try effects(d), ["hidden automatic", "automatic", "hard", "automatic"])
    }

    /// An edge whose effect the scroller hides has no control bars (LLP 1131 D2), and the other edge's are still found;
    /// whether that edge is banded is the bands' rule (`PageEdgesIOSTests`). The retired `none` hides both edges, so
    /// neither has any.
    func testAHiddenEdgeHasNoControlBars() {
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "button"],
            ["op": "create", "id": 4, "kind": "scroll", "props": ["flexGrows": "true"]],
            ["op": "create", "id": 5, "kind": "view"],
            ["op": "create", "id": 6, "kind": "button"],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 5, "ids": [6]],
            ["op": "children", "id": 1, "ids": [2, 4, 5]],
            ["op": "roots", "ids": [1]]
        ]))
        let route = p.views[1]!, head = p.views[2]!, list = p.views[4]!, foot = p.views[5]!
        route.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        head.frame = CGRect(x: 0, y: 0, width: 402, height: 50)
        list.frame = CGRect(x: 0, y: 50, width: 402, height: 774)
        foot.frame = CGRect(x: 0, y: 824, width: 402, height: 50)
        let bars = { (top: Bool) in ControlBars(p).bars(of: list, in: route, top: top).map { $0.map(\.id) } }
        XCTAssertEqual(bars(true), [head.id])
        XCTAssertEqual(bars(false), [foot.id])
        list.style[hidden] = .number(Double(top))
        XCTAssertNil(bars(true), "the hidden top has none")
        XCTAssertEqual(bars(false), [foot.id], "the bottom's bar is still found")
        list.style[hidden] = .number(Double(bottom))
        XCTAssertEqual(bars(true), [head.id])
        XCTAssertNil(bars(false))
        list.style[hidden] = nil
        list.style["scroll_edge_effect"] = .string("none")
        XCTAssertNil(bars(true))
        XCTAssertNil(bars(false))
    }
}
#endif
