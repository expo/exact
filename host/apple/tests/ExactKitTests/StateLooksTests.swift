#if os(iOS) || os(macOS)
#if os(iOS)
import UIKit
#else
import AppKit
#endif
import XCTest
@testable import ExactKit

/// State parts on UIKit and AppKit (LLP 1121 D8, stage 8 B2): `:active`
/// arrives as the press paint (LLP 1061 D7), its opacity an absolute target
/// (`press_opacity_to`) the press eases to from what shows, with no platform
/// dim; rows it doesn't name show the base, a base the engine moves while
/// pressed included. On the Mac a `:hover` and a `:focus-visible` look stand
/// in at once while they hold (the focus's over the hover's), their opacity
/// an additive layer opacity, under the press's paint; a disabled node takes
/// none.
///   bun host/apple/build.mjs --test --ios   (and --test for the Mac)
final class StateLooksTests: XCTestCase {
    #if os(iOS)
    private var window: UIWindow!
    #else
    private var window: NSWindow!
    override func tearDown() { window?.close(); window = nil }
    #endif

    private let blue: [Double] = [37, 99, 235, 255], navy: [Double] = [29, 78, 216, 255]
    private let white: [Double] = [255, 255, 255, 255], red: [Double] = [255, 0, 0, 255], green: [Double] = [0, 255, 0, 255]
    private let yellow: [Double] = [253, 224, 71, 255]
    private func ink(_ c: [Double]) -> BatchValue { .array(c.map(BatchValue.number)) }
    private var looks: [String: Any] {
        ["hover": ["background_color": green, "opacity": 0.8], "focus_visible": ["border_color_top": green]]
    }
    private func base(_ fill: [Double], right: [Double] = [0, 0, 0, 255]) -> [String: Any] {
        ["background_color": fill, "border_width_top": 2, "border_width_right": 2, "border_color_top": [0, 0, 0, 255],
         "border_color_right": right, "looks": looks, "press_background_color": navy, "press_opacity_to": 0.6]
    }

    private func fixture(props: [String: String] = [:]) throws -> (Presenter, NodeView) {
        let p = Presenter()
        #if os(iOS)
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        #else
        _ = NSApplication.shared
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        #endif
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "handlers": ["press"], "props": props, "style": base(blue)],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 50.0, "y": 50.0, "w": 200.0, "h": 100.0],
        ]))
        return (p, try XCTUnwrap(p.views[1]))
    }
    private func layer(_ v: NodeView) -> CALayer? { v.layer }
    private func fade(_ v: NodeView) -> CABasicAnimation? { layer(v)?.animation(forKey: "pressOpacity") as? CABasicAnimation }
    private func lookFade(_ v: NodeView) -> Float? { (layer(v)?.animation(forKey: "lookOpacity") as? CABasicAnimation)?.toValue as? Float }

    func testActiveIsThePressPaintAndTheBaseShowsBeneath() throws {
        let (p, v) = try fixture()
        XCTAssertTrue(v.authorsPress, "the author's press")
        #if os(iOS)
        XCTAssertFalse(v.platformPressDims, "no dim on top")
        #else
        XCTAssertFalse(v.platformPressTints, "no wash on top")
        #endif
        v.pressed = true
        XCTAssertEqual(v.style["background_color"], ink(navy))
        XCTAssertEqual(v.style["border_color_right"], ink([0, 0, 0, 255]), "a row it doesn't name is the base's")
        XCTAssertEqual(try XCTUnwrap(fade(v)?.toValue as? Float), -0.4, accuracy: 1e-6, "0.6 in place of 1, not a share")
        // The engine moves the base while pressed: the look stays over it.
        p.apply(wireBatch([["op": "style", "id": 1, "style": base(white, right: red)]]))
        XCTAssertEqual(v.style["background_color"], ink(navy))
        XCTAssertEqual(v.style["border_color_right"], ink(red), "the new base beneath")
        v.press.start -= 1; v.pressPaint.at -= 1
        v.pressed = false
        XCTAssertEqual(v.style["background_color"], ink(white), "the engine's newer value")
    }

    #if os(macOS)
    func testHoverAndFocusVisibleLooksStandInUnderThePress() throws {
        let (p, v) = try fixture()
        p.setHoverPath([v])
        XCTAssertEqual(v.style["background_color"], ink(green), ":hover, though it hears no hover")
        XCTAssertEqual(try XCTUnwrap(lookFade(v)), -0.2, accuracy: 1e-6, "its opacity, 0.8, at once")
        v.focusVisible = true
        XCTAssertEqual(v.style["border_color_top"], ink(green), ":focus-visible")
        v.pressed = true
        XCTAssertEqual(v.style["background_color"], ink(navy), "the press paint over :hover")
        XCTAssertEqual(try XCTUnwrap(lookFade(v)), -0.2, accuracy: 1e-6, "the hover's 0.8 stays beneath")
        XCTAssertEqual(try XCTUnwrap(fade(v)?.toValue as? Float), -0.2, accuracy: 1e-6, "and the press takes it to 0.6")
        v.press.start -= 1; v.pressPaint.at -= 1
        v.pressed = false
        XCTAssertEqual(v.style["background_color"], ink(green), "the hover again")
        p.setHoverPath([])
        v.focusVisible = false
        XCTAssertEqual(v.style["background_color"], ink(blue))
        XCTAssertEqual(v.style["border_color_top"], ink([0, 0, 0, 255]))
        XCTAssertNil(lookFade(v))
    }

    func testADisabledNodeTakesNoLook() throws {
        let (p, v) = try fixture(props: ["disabled": "true"])
        p.setHoverPath([v])
        XCTAssertEqual(v.style["background_color"], ink(blue))
        XCTAssertNil(lookFade(v))
        // Enabled under the resting pointer: the look comes; disabled again, it goes.
        p.apply(wireBatch([["op": "props", "id": 1, "set": [:], "clear": ["disabled"]]]))
        XCTAssertEqual(v.style["background_color"], ink(green))
        XCTAssertNotNil(lookFade(v))
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["disabled": "true"], "clear": []]]))
        XCTAssertEqual(v.style["background_color"], ink(blue))
        XCTAssertNil(lookFade(v))
        // The focus's look goes and comes the same way.
        v.focusVisible = true
        XCTAssertEqual(v.style["border_color_top"], ink([0, 0, 0, 255]))
        p.apply(wireBatch([["op": "props", "id": 1, "set": [:], "clear": ["disabled"]]]))
        XCTAssertEqual(v.style["border_color_top"], ink(green))
    }

    /// A box with a `:hover` colour (1, `ink` its base), holding a text that
    /// takes its colour (2), one with its own (3), and a pressable box (4)
    /// with a press colour holding a text that takes the colour around (5).
    private func colourFixture(_ extra: [String: Any] = [:]) throws -> (Presenter, NodeView, [NodeView]) {
        let (p, v) = try fixture()
        let style: [String: Any] = ["text_color": white, "looks": ["hover": ["text_color": red]]].merging(extra) { $1 }
        p.apply(wireBatch([
            ["op": "style", "id": 1, "style": style],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "Takes"], "style": ["text_color": white]],
            ["op": "create", "id": 3, "kind": "text", "props": ["text": "Keeps"], "style": ["text_color": blue, "written": ["text_color"]]],
            ["op": "create", "id": 4, "kind": "view", "handlers": ["press"], "style": ["text_color": white, "press_color": navy]],
            ["op": "create", "id": 5, "kind": "text", "props": ["text": "Pressed"], "style": ["text_color": white]],
            ["op": "children", "id": 4, "ids": [5]],
            ["op": "children", "id": 1, "ids": [2, 3, 4]],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 200.0, "h": 20.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 20.0, "w": 200.0, "h": 20.0],
            ["op": "frame", "id": 4, "x": 0.0, "y": 40.0, "w": 200.0, "h": 40.0],
            ["op": "frame", "id": 5, "x": 0.0, "y": 0.0, "w": 200.0, "h": 20.0],
        ]))
        return (p, v, try [2, 3, 4, 5].map { try XCTUnwrap(p.views[$0]) })
    }
    private func colours(_ views: [NodeView]) -> [BatchValue?] { views.map { $0.style["text_color"] } }

    /// A `:hover` `color` is its text colour and its subtree's, as CSS's
    /// `a:hover { color }` is inherited: a text with its own keeps it, the
    /// engine's writes beneath keep it, and the pointer leaving gives the
    /// base back.
    func testAHoverColourReachesTheTextBelow() throws {
        let (p, v, below) = try colourFixture()
        p.setHoverPath([v])
        XCTAssertEqual(v.style["text_color"], ink(red))
        XCTAssertEqual(colours(below), [ink(red), ink(blue), ink(red), ink(red)], "all but the text with its own")
        // The engine writes a text beneath again: the hover's colour stays over it.
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["text_color": white]]]))
        XCTAssertEqual(below[0].style["text_color"], ink(red))
        p.setHoverPath([])
        XCTAssertEqual(v.style["text_color"], ink(white))
        XCTAssertEqual(colours(below), [ink(white), ink(blue), ink(white), ink(white)])
    }

    /// The press's colour stands over the hover's, its own and a pressed
    /// child's, and the hover's comes back with the release; the focus's
    /// stands over the hover's.
    func testAPressColourStandsOverTheHoversAndTheFocusOver() throws {
        let (p, v, below) = try colourFixture(["press_color": yellow, "looks": ["hover": ["text_color": red], "focus_visible": ["text_color": green]]])
        p.setHoverPath([v])
        v.pressed = true
        XCTAssertEqual(v.style["text_color"], ink(yellow))
        XCTAssertEqual(colours(below), [ink(yellow), ink(blue), ink(yellow), ink(yellow)])
        v.press.start -= 1; v.pressPaint.at -= 1
        v.pressed = false
        XCTAssertEqual(colours(below), [ink(red), ink(blue), ink(red), ink(red)], "the hover's again")
        // A child's own press colour over the hover's, and the hover's again after.
        let child = below[2]
        child.pressed = true
        XCTAssertEqual(colours(below), [ink(red), ink(blue), ink(navy), ink(navy)])
        child.press.start -= 1; child.pressPaint.at -= 1
        child.pressed = false
        XCTAssertEqual(colours(below), [ink(red), ink(blue), ink(red), ink(red)])
        v.focusVisible = true
        XCTAssertEqual(colours(below), [ink(green), ink(blue), ink(green), ink(green)], "the focus's over the hover's")
        p.setHoverPath([])
        v.focusVisible = false
        XCTAssertEqual(colours(below), [ink(white), ink(blue), ink(white), ink(white)])
    }

    /// A disabled node lends no look's colour; enabled under the pointer, it does.
    func testADisabledNodeLendsNoColour() throws {
        let (p, v, below) = try colourFixture()
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["disabled": "true"], "clear": []]]))
        p.setHoverPath([v])
        XCTAssertEqual(colours(below), [ink(white), ink(blue), ink(white), ink(white)])
        p.apply(wireBatch([["op": "props", "id": 1, "set": [:], "clear": ["disabled"]]]))
        XCTAssertEqual(colours(below), [ink(red), ink(blue), ink(red), ink(red)])
    }

    /// A text a batch mounts under a node while its look holds takes the
    /// look's colour, as a browser's inheritance gives it, and gives it back
    /// when the look goes.
    func testAViewMountedUnderAHoldingLookTakesItsColour() throws {
        let (p, v, _) = try colourFixture()
        p.setHoverPath([v])
        p.apply(wireBatch([
            ["op": "create", "id": 6, "kind": "text", "props": ["text": "New"], "style": ["text_color": white]],
            ["op": "children", "id": 1, "ids": [2, 3, 4, 6]],
            ["op": "frame", "id": 6, "x": 0.0, "y": 80.0, "w": 200.0, "h": 20.0],
        ]))
        let mounted = try XCTUnwrap(p.views[6])
        XCTAssertEqual(mounted.style["text_color"], ink(red))
        p.setHoverPath([])
        XCTAssertEqual(mounted.style["text_color"], ink(white))
    }

    /// A view a batch moves out from under a node while its look holds gives
    /// the look's colour back: at once, as it is no longer below, and the
    /// look ending later lends it nothing.
    func testAViewMovedOutGivesTheLookColourBack() throws {
        let (p, v, below) = try colourFixture()
        p.apply(wireBatch([
            ["op": "create", "id": 7, "kind": "view", "style": ["text_color": white]],
            ["op": "roots", "ids": [1, 7]],
            ["op": "frame", "id": 7, "x": 0.0, "y": 200.0, "w": 200.0, "h": 40.0],
        ]))
        p.setHoverPath([v])
        XCTAssertEqual(below[0].style["text_color"], ink(red))
        p.apply(wireBatch([["op": "children", "id": 1, "ids": [3, 4]], ["op": "children", "id": 7, "ids": [2]]]))
        XCTAssertEqual(below[0].style["text_color"], ink(white), "moved out: its own again")
        p.setHoverPath([])
        XCTAssertEqual(below[0].style["text_color"], ink(white))
    }

    /// A mode boundary (`-exact-mode`) writes the colour around it again in
    /// its own tuple, a value of its own: it and the views below it that
    /// take it are lent the look's colour, as on the web.
    func testAModeBoundaryIsLentTheLookColour() throws {
        let (p, v, _) = try colourFixture()
        p.apply(wireBatch([
            ["op": "create", "id": 8, "kind": "view", "style": ["text_color": green, "mode": 2, "written": ["mode"]]],
            ["op": "create", "id": 9, "kind": "text", "props": ["text": "Pinned"], "style": ["text_color": green, "mode": 2]],
            ["op": "children", "id": 8, "ids": [9]],
            ["op": "children", "id": 1, "ids": [2, 3, 4, 8]],
            ["op": "frame", "id": 8, "x": 0.0, "y": 80.0, "w": 200.0, "h": 20.0],
            ["op": "frame", "id": 9, "x": 0.0, "y": 0.0, "w": 200.0, "h": 20.0],
        ]))
        let pin = try XCTUnwrap(p.views[8]), text = try XCTUnwrap(p.views[9])
        p.setHoverPath([v])
        XCTAssertEqual([pin.style["text_color"], text.style["text_color"]], [ink(red), ink(red)])
        p.setHoverPath([])
        XCTAssertEqual([pin.style["text_color"], text.style["text_color"]], [ink(green), ink(green)])
    }

    /// A look's `color="unset"` is CSS's `inherit`: the node and the views
    /// below it that take its colour show the parent's while it holds.
    func testAnUnsetLookColourIsTheParents() throws {
        let (p, _) = try fixture()
        p.apply(wireBatch([
            ["op": "create", "id": 10, "kind": "view", "style": ["text_color": blue]],
            ["op": "create", "id": 11, "kind": "view", "style": ["text_color": white, "looks": ["hover": ["text_color": NSNull()]]]],
            ["op": "create", "id": 12, "kind": "text", "props": ["text": "Kid"], "style": ["text_color": white]],
            ["op": "children", "id": 10, "ids": [11]],
            ["op": "children", "id": 11, "ids": [12]],
            ["op": "roots", "ids": [1, 10]],
            ["op": "frame", "id": 10, "x": 0.0, "y": 200.0, "w": 200.0, "h": 60.0],
            ["op": "frame", "id": 11, "x": 0.0, "y": 0.0, "w": 200.0, "h": 40.0],
            ["op": "frame", "id": 12, "x": 0.0, "y": 0.0, "w": 200.0, "h": 20.0],
        ]))
        let node = try XCTUnwrap(p.views[11]), kid = try XCTUnwrap(p.views[12])
        p.setHoverPath([node])
        XCTAssertEqual([node.style["text_color"], kid.style["text_color"]], [ink(blue), ink(blue)])
        p.setHoverPath([])
        XCTAssertEqual([node.style["text_color"], kid.style["text_color"]], [ink(white), ink(white)])
    }

    /// A re-entrant hover (an ancestor's enter replacing the path) publishes
    /// only the newer path's looks.
    func testAReentrantHoverShowsOnlyTheNewerPathsLooks() throws {
        let (p, v) = try fixture()
        p.apply(wireBatch([
            ["op": "create", "id": 2, "kind": "view", "handlers": ["hover"], "style": ["looks": ["hover": ["background_color": red]]]],
            ["op": "children", "id": 2, "ids": [1]], ["op": "roots", "ids": [2]],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 300.0, "h": 300.0],
        ]))
        let parent = try XCTUnwrap(p.views[2])
        p.onHover = { id, over in if id == parent.id, over { p.setHoverPath([parent]) } }
        p.setHoverPath([v, parent])
        XCTAssertEqual(parent.style["background_color"], ink(red))
        XCTAssertEqual(v.style["background_color"], ink(blue), "the superseded child's look stays off")
    }

    /// A press whose depth is 0 when it starts (its target what shows)
    /// still runs its clock: a look that goes mid-press takes it the rest of
    /// the way, and a release takes it back.
    func testAPressAtNoDepthStillRunsItsClock() throws {
        let (p, v) = try fixture()
        p.apply(wireBatch([["op": "style", "id": 1, "style": base(blue).merging(["press_opacity_to": 0.8]) { $1 }]]))
        p.setHoverPath([v])
        v.pressed = true
        XCTAssertNil(fade(v), "0.8 to 0.8: nothing to fade")
        XCTAssertEqual(v.pressPaint.ease.to, 1, "but the press is down")
        p.setHoverPath([])
        XCTAssertEqual(try XCTUnwrap(fade(v)?.toValue as? Float), -0.2, accuracy: 1e-6, "1 to 0.8 once the hover goes")
        v.press.start -= 1; v.pressPaint.at -= 1
        v.pressed = false
        XCTAssertEqual(v.pressPaint.ease.to, 0)
        XCTAssertEqual(try XCTUnwrap(fade(v)?.toValue as? Float), 0, accuracy: 1e-6, "and back")
    }

    /// A hand-written pressed opacity is a share of what shows, a hover's
    /// included, as the web and Linux take it: 0.5 of 0.8.
    func testAPressedShareIsOfTheHoversOpacity() throws {
        let (p, v) = try fixture()
        p.apply(wireBatch([["op": "style", "id": 1, "style": base(blue).merging(["press_opacity": 0.5]) { $1 }.filter { $0.key != "press_opacity_to" }]]))
        p.setHoverPath([v])
        v.pressed = true
        XCTAssertEqual(try XCTUnwrap(lookFade(v)), -0.2, accuracy: 1e-6, "the hover's 0.8")
        XCTAssertEqual(try XCTUnwrap(fade(v)?.toValue as? Float), -0.4, accuracy: 1e-6, "then 0.4, half of it")
    }

    /// What shows moves mid-press: the press goes on as `-depth × progress`
    /// of the new depth on its own ease, so the opacity is the new base's at
    /// that progress. A depth of 0 at the press still moves the press's clock.
    func testALookThatGoesMidPressMovesThePressWithIt() throws {
        let (p, v) = try fixture()
        p.setHoverPath([v])
        v.pressed = true
        v.pressPaint.ease.start -= PressFeedback.duration / 2 // halfway in
        let progress = Float(v.pressPaint.ease.factor(at: CACurrentMediaTime()))
        p.setHoverPath([])
        XCTAssertNil(lookFade(v))
        // The fade is the press's own ease with the new depth: from 0 at its start to -0.4, begun when the press
        // began, so it shows -0.4 × progress now (1 less 0.4 of the way to 0.6).
        let f = try XCTUnwrap(fade(v))
        XCTAssertEqual(try XCTUnwrap(f.fromValue as? Float), 0, accuracy: 1e-6)
        XCTAssertEqual(try XCTUnwrap(f.toValue as? Float), -0.4, accuracy: 1e-6, "to 0.6")
        XCTAssertEqual(f.beginTime, try XCTUnwrap(layer(v)).convertTime(v.pressPaint.ease.start, from: nil), accuracy: 1e-6)
        XCTAssertGreaterThan(progress, 0)
    }
    #endif
}
#endif
