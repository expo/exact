#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// A header's bar items (LLP 1141.004 D1) over hand-built routes: every
/// button UIKit's own item, the readouts hosted, nothing else an item, what
/// the title takes, and a readout lifted into its item and back. These
/// presenters have no kernel, so nothing is laid out; the native fixture's
/// readout, pushed and laid out, is NavigationBasicsIOSTests'.
final class BarHostedIOSTests: XCTestCase {
    private var window: UIWindow!
    private var sessions: [ExactSession] = []

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    /// A route whose header holds a level-2 heading "Log" (id 3) and then
    /// `rest`, created as given; `kids` are the header's children after the
    /// heading, `more` any further `children` ops.
    private func route(_ rest: [[String: Any]], kids: [Int], more: [[String: Any]] = [], header: [Int]? = nil) -> Presenter {
        // A session's presenter: a face's texts are set by its text engine.
        let session = ExactApp.shared.makeSession(label: "hosted-\(UUID().uuidString)")
        sessions.append(session)
        let p = session.presenter
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        var ops: [[String: Any]] = [
            ["op": "create", "id": 1, "kind": "view", "props": ["navigationKey": "log"]],
            ["op": "create", "id": 2, "kind": "view", "props": ["semanticTag": "header"]],
            ["op": "create", "id": 3, "kind": "text", "props": ["text": "Log", "accessibilityRole": "heading"]],
        ]
        ops += rest + more
        ops += [["op": "children", "id": 2, "ids": header ?? [3] + kids], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]]]
        p.apply(wireBatch(ops))
        return p
    }

    private func text(_ id: Int, _ words: String, _ props: [String: String] = [:], style: [String: Any] = [:]) -> [String: Any] {
        ["op": "create", "id": id, "kind": "text", "props": ["text": words].merging(props) { $1 }, "style": style]
    }

    private func status(_ id: Int, _ props: [String: String] = [:], style: [String: Any] = [:]) -> [String: Any] {
        ["op": "create", "id": id, "kind": "view", "props": ["accessibilityRole": "status"].merging(props) { $1 }, "style": style]
    }

    private let dot: [String: Any] = ["background_color": [52, 199, 89, 255], "width": 7, "height": 7]

    private func progress(_ id: Int) -> [String: Any] { ["op": "create", "id": id, "kind": "control", "props": ["type": "progress"]] }
    private func bar(_ p: Presenter) throws -> HeaderShape { try XCTUnwrap(HeaderShape(route: try XCTUnwrap(p.views[1]), back: nil, projected: true)) }

    /// A readout hidden as authored, by a box around it, is no item.
    func testAReadoutInABoxHiddenAsAuthoredShowsNothing() throws {
        let p = route([["op": "create", "id": 4, "kind": "view", "style": ["display": "none"]], text(5, "Saved", ["accessibilityRole": "status"])],
                      kids: [4], more: [["op": "children", "id": 4, "ids": [5]]])
        let shape = try bar(p)
        XCTAssertTrue(shape.trailing.isEmpty, "hidden by the box around it")
        XCTAssertTrue(shape.hosted.isEmpty)
    }

    /// An element the bar has no object for is no item (D1): a row of a
    /// spinner and its words is one only as a readout, else the title reads
    /// its words as the subtitle; a spinner beside the subtitle is none. A
    /// status is never the subtitle, and a symbol beside it in a row is no
    /// item.
    func testAnElementTheBarHasNoObjectForIsNoItem() throws {
        for role in ["", "status"] {
            let p = route([["op": "create", "id": 4, "kind": "view", "props": role.isEmpty ? [:] : ["accessibilityRole": role]], progress(5), text(6, "Updating")],
                          kids: [4], more: [["op": "children", "id": 4, "ids": [5, 6]]])
            let shape = try bar(p)
            XCTAssertEqual(shape.trailing.map(\.id), role.isEmpty ? [] : [4], "a readout whole, else nothing: \(role)")
            XCTAssertEqual(shape.hosted.map(\.id), role.isEmpty ? [] : [4])
            // A plain row after the heading is read for its words; a readout never.
            XCTAssertEqual(shape.group?.subtitle ?? "", role.isEmpty ? "Updating" : "", "the subtitle: \(role)")
        }
        let siblings = route([progress(5), text(6, "Updating")], kids: [5, 6])
        let apart = try bar(siblings)
        XCTAssertTrue(apart.trailing.isEmpty, "the spinner is no item")
        XCTAssertEqual(apart.group?.subtitle, "Updating")
        let status = route([["op": "create", "id": 4, "kind": "view"],
                            ["op": "create", "id": 5, "kind": "image", "props": ["imageSource": "symbol:sf/checkmark", "symbolName": "checkmark"]],
                            text(6, "Saved", ["accessibilityRole": "status"]), text(7, "3 entries")],
                           kids: [4, 7], more: [["op": "children", "id": 4, "ids": [5, 6]]])
        let saved = try bar(status)
        XCTAssertEqual(saved.trailing.map(\.id), [6], "the readout alone")
        XCTAssertEqual(saved.group?.subtitle, "3 entries")
        XCTAssertTrue(saved.group?.glyphs.isEmpty ?? true)
    }

    /// A row of glyphs beside a readout is still the glyph line, the readout
    /// its own item; beside a hidden status it is the glyph line whole.
    func testAGlyphLineBesideAReadoutStaysOne() throws {
        for hidden in [false, true] {
            let p = route([["op": "create", "id": 4, "kind": "view"],
                           ["op": "create", "id": 5, "kind": "image", "props": ["imageSource": "symbol:sf/bell.slash", "symbolName": "bell.slash"]],
                           text(6, "Muted"), text(7, "Live", ["accessibilityRole": "status"], style: hidden ? ["display": "none"] : [:])],
                          kids: [4], more: [["op": "children", "id": 4, "ids": [5, 6, 7]]])
            let shape = try bar(p)
            XCTAssertEqual(shape.group?.glyphs.map(\.symbol), ["bell.slash", nil], "hidden: \(hidden)")
            XCTAssertEqual(shape.group?.subtitle, "Muted")
            XCTAssertEqual(shape.trailing.map(\.id), hidden ? [] : [7])
        }
    }

    /// A disabled button is UIKit's disabled bar item, shown dimmed, though
    /// nothing presses it (the euxy port's "● Clock" written as one); one
    /// neither pressed nor disabled is still the page's.
    func testADisabledButtonIsADisabledItemThoughNothingPressesIt() throws {
        let button = { (id: Int, props: [String: String]) -> [String: Any] in
            ["op": "create", "id": id, "kind": "control", "props": ["type": "button", "accessibilityRole": "button"].merging(props) { $1 }]
        }
        let p = route([button(4, ["disabled": "true", "accessibilityLabel": "Clock"]), button(5, ["accessibilityLabel": "Idle"]),
                       button(6, ["disabled": "false", "accessibilityLabel": "Enabled"])], kids: [4, 5, 6])
        let shape = try XCTUnwrap(HeaderShape(route: try XCTUnwrap(p.views[1]), back: nil, projected: true))
        XCTAssertEqual(shape.trailing.map(\.id), [4])
        XCTAssertTrue(shape.trailing.first?.disabled ?? false)
        XCTAssertTrue(shape.placed.contains { $0.id == 4 })
        XCTAssertFalse(shape.placed.contains { $0.id == 5 || $0.id == 6 })
        // An inert button that presses is an item, dimmed, as in the page.
        let inert = route([["op": "create", "id": 4, "kind": "control", "handlers": ["press"], "props": ["type": "button", "accessibilityRole": "button", "accessibilityLabel": "Send", "inert": "true"]]], kids: [4])
        XCTAssertEqual(try bar(inert).trailing.first?.disabled, true)
    }

    /// Every button is UIKit's own item, drawn from its symbol and words (D1):
    /// a link of one text, a pressed row of words or holding a spinner, a
    /// badge, a badge holding a spinner, a button with a dot beside its label,
    /// a switch that hears `press`. A link is an item only with an `href` to
    /// follow. A row holding a button is walked for it. The readout is the one
    /// hosted item, and what the bar replaces is covered by a bar, so the
    /// readout lifted out of it is laid out.
    func testEveryButtonIsUIKitsItemAndOnlyAReadoutIsHosted() throws {
        let fill: [String: Any] = ["background_color": [40, 100, 200, 255], "border_radius": 16]
        let p = route([["op": "create", "id": 4, "kind": "button", "props": ["href": "/history", "accessibilityLabel": "History"]], text(5, "History"),
                       ["op": "create", "id": 6, "kind": "view", "handlers": ["press"]], text(7, "Sync"),
                       ["op": "create", "id": 8, "kind": "view", "handlers": ["press"], "props": ["accessibilityLabel": "Busy"]], progress(9),
                       ["op": "create", "id": 10, "kind": "button", "handlers": ["press"], "props": ["accessibilityLabel": "Me"]],
                       ["op": "create", "id": 11, "kind": "view", "style": fill], text(12, "AB"),
                       ["op": "create", "id": 13, "kind": "button", "handlers": ["press"], "props": ["accessibilityLabel": "Live"]],
                       ["op": "create", "id": 14, "kind": "view", "style": dot], text(15, "Live"),
                       ["op": "create", "id": 16, "kind": "control", "handlers": ["press"], "props": ["type": "checkbox", "accessibilityRole": "switch", "accessibilityLabel": "Alerts"]],
                       ["op": "create", "id": 17, "kind": "button", "props": ["accessibilityRole": "link", "accessibilityLabel": "Nowhere"]],
                       ["op": "create", "id": 18, "kind": "button", "props": ["href": "", "accessibilityLabel": "Cleared"]],
                       status(19), text(20, "Live")],
                      kids: [4, 6, 8, 10, 13, 16, 17, 18, 19],
                      more: [["op": "children", "id": 4, "ids": [5]], ["op": "children", "id": 6, "ids": [7]], ["op": "children", "id": 8, "ids": [9]],
                             ["op": "children", "id": 10, "ids": [11]], ["op": "children", "id": 11, "ids": [12]], ["op": "children", "id": 13, "ids": [14, 15]],
                             ["op": "children", "id": 19, "ids": [20]]])
        let shape = try bar(p)
        XCTAssertEqual(shape.trailing.map(\.id), [4, 6, 8, 10, 13, 16, 19], "no link that follows nothing")
        XCTAssertEqual(shape.trailing.map(\.hosted), [false, false, false, false, false, false, true])
        XCTAssertEqual(Array(shape.trailing.map(\.title).prefix(5)), ["History", "Sync", "", "AB", "Live"], "drawn from its words")
        XCTAssertEqual(shape.trailing[3].badge?.text, "AB")
        XCTAssertEqual(shape.hosted.map(\.id), [19])
        let controller = RouteController(try XCTUnwrap(p.views[1]))
        XCTAssertEqual(NavigationHost.cover(of: try XCTUnwrap(p.views[19]), in: controller), .bar)
        // A row holding a button is walked: the button an item, the text
        // after the subtitle none.
        let button: [String: Any] = ["op": "create", "id": 7, "kind": "button", "handlers": ["press"], "props": ["accessibilityLabel": "Retry"]]
        let walked = route([text(8, "Inbox"), ["op": "create", "id": 4, "kind": "view"], text(6, "Offline"), button], kids: [8, 4], more: [["op": "children", "id": 4, "ids": [6, 7]]])
        XCTAssertEqual(try bar(walked).trailing.map(\.id), [7])
    }

    /// The Back control UIKit's back button stands for is no item, whatever
    /// its face, and the bar hides it with what it places.
    func testTheBackControlIsUIKitsWhateverItsFace() throws {
        let back = route([["op": "create", "id": 4, "kind": "button", "handlers": ["press"], "props": ["id": "back", "accessibilityLabel": "Back"]], progress(5),
                          text(6, "Kept", style: ["appearance": "none", "written": ["appearance"]])],
                         kids: [], more: [["op": "children", "id": 4, "ids": [5]]], header: [4, 3, 6])
        let pushed = try XCTUnwrap(HeaderShape(route: try XCTUnwrap(back.views[1]), back: "back", projected: true))
        XCTAssertTrue(pushed.hosted.isEmpty, "Back is UIKit's")
        XCTAssertTrue(pushed.leading.isEmpty)
        XCTAssertTrue(pushed.placed.contains { $0.id == 4 }, "hidden with what the bar places")
    }

    /// The title's ownership is the title presented: with segments in the
    /// title view, a glyph line after the heading is no item, and a readout
    /// in it its own, lifted; when the segments go, the title takes the line
    /// again. A text its author keeps in the page (`appearance: none`) is not
    /// the subtitle.
    func testTheTitleOwnsWhatItPresents() throws {
        let tab: (Int, String, Bool) -> [String: Any] = { id, words, on in
            ["op": "create", "id": id, "kind": "button", "handlers": ["press"], "props": ["accessibilityRole": "tab", "accessibilityLabel": words, "accessibilitySelected": on ? "true" : "false"]]
        }
        let p = route([["op": "create", "id": 4, "kind": "view"],
                       ["op": "create", "id": 5, "kind": "image", "props": ["imageSource": "symbol:sf/bell.slash", "symbolName": "bell.slash"]],
                       text(6, "Muted"),
                       ["op": "create", "id": 10, "kind": "view", "props": ["accessibilityRole": "tablist"]], tab(11, "All", true), text(13, "All"), tab(12, "Missed", false), text(14, "Missed")],
                      kids: [4, 10], more: [["op": "children", "id": 4, "ids": [5, 6]], ["op": "children", "id": 10, "ids": [11, 12]],
                                            ["op": "children", "id": 11, "ids": [13]], ["op": "children", "id": 12, "ids": [14]]])
        let segmented = try bar(p)
        XCTAssertNotNil(segmented.segments)
        XCTAssertTrue(segmented.hosted.isEmpty && segmented.trailing.isEmpty, "the glyph line, which the segments' title cannot show, is no item")
        let live = route([["op": "create", "id": 4, "kind": "view"],
                          ["op": "create", "id": 5, "kind": "image", "props": ["imageSource": "symbol:sf/bell.slash", "symbolName": "bell.slash"]],
                          text(6, "Muted"), text(7, "Live", ["accessibilityRole": "status"]),
                          ["op": "create", "id": 10, "kind": "view", "props": ["accessibilityRole": "tablist"]], tab(11, "All", true), text(13, "All"), tab(12, "Missed", false), text(14, "Missed")],
                         kids: [4, 10], more: [["op": "children", "id": 4, "ids": [5, 6, 7]], ["op": "children", "id": 10, "ids": [11, 12]],
                                               ["op": "children", "id": 11, "ids": [13]], ["op": "children", "id": 12, "ids": [14]]])
        let liveRoute = RouteController(try XCTUnwrap(live.views[1]))
        let tabbed = try bar(live)
        XCTAssertEqual(tabbed.hosted.map(\.id), [7], "the readout alone")
        _ = live.navigationHost.barItems(tabbed.trailing, liveRoute)
        XCTAssertFalse(try XCTUnwrap(live.views[7]).isDescendant(of: try XCTUnwrap(live.views[4])), "lifted")
        live.apply(wireBatch([["op": "children", "id": 2, "ids": [3, 4]]]))
        let untabbed = try bar(live)
        XCTAssertEqual(untabbed.group?.glyphs.map(\.symbol), ["bell.slash", nil], "the title's glyph line")
        XCTAssertEqual(untabbed.hosted.map(\.id), [7])
        XCTAssertEqual(untabbed.trailing.map(\.id), [7])
        // A status kept in the page inside the glyph line stays there: the
        // bar hides the glyphs the title draws, not the row that holds it.
        let optOut = route([["op": "create", "id": 4, "kind": "view"],
                            ["op": "create", "id": 5, "kind": "image", "props": ["imageSource": "symbol:sf/bell.slash", "symbolName": "bell.slash"]],
                            text(6, "Muted"), text(7, "Live", ["accessibilityRole": "status"], style: ["appearance": "none", "written": ["appearance"]])],
                           kids: [4], more: [["op": "children", "id": 4, "ids": [5, 6, 7]]])
        let partial = try bar(optOut)
        XCTAssertEqual(partial.authored.map(\.id), [7])
        XCTAssertEqual(partial.group?.subtitle, "Muted")
        XCTAssertTrue(partial.hosted.isEmpty)
        XCTAssertEqual(Set(partial.placed.map(\.id)).intersection([4, 5, 6, 7]), [5, 6], "the glyphs, not the row")
        let kept = route([text(5, "3 left", style: ["appearance": "none", "written": ["appearance"]])], kids: [5])
        let shape = try XCTUnwrap(HeaderShape(route: try XCTUnwrap(kept.views[1]), back: nil, projected: true))
        XCTAssertEqual(shape.group?.subtitle ?? "", "", "kept in the page, not the bar's subtitle")
        XCTAssertTrue(shape.hosted.isEmpty)
    }

    /// A button's colour is its item's tint: its symbol's, else its text's,
    /// else the button's own.
    func testAButtonsColourIsItsItemsTint() throws {
        let press: (Int, [String: Any]) -> [String: Any] = { ["op": "create", "id": $0, "kind": "button", "handlers": ["press"], "style": $1] }
        let red: [String: Any] = ["text_color": [255, 0, 0, 255], "written": ["text_color"]]
        let p = route([press(4, [:]), text(5, "Stop", style: red), press(6, red), text(7, "Stop")],
                      kids: [4, 6], more: [["op": "children", "id": 4, "ids": [5]], ["op": "children", "id": 6, "ids": [7]]])
        let shape = try bar(p)
        for item in shape.trailing { XCTAssertEqual(Array(item.tint?.first?.prefix(3) ?? []), [255, 0, 0], "item \(item.id)") }
    }

    /// A title wrapper that opts out after the bar lifted its readout keeps
    /// the readout in the page with it: the opt-out is read in the authored
    /// tree.
    func testAWrapperThatOptsOutKeepsItsLiftedReadout() throws {
        let p = route([["op": "create", "id": 8, "kind": "view", "handlers": ["press"]], status(6), text(7, "Live")],
                      kids: [], more: [["op": "children", "id": 6, "ids": [7]], ["op": "children", "id": 8, "ids": [3, 6]]], header: [8])
        let route = RouteController(try XCTUnwrap(p.views[1]))
        let first = try bar(p)
        XCTAssertEqual(first.hosted.map(\.id), [6], "the readout, hosted")
        XCTAssertEqual(first.group?.tap, 8)
        _ = p.navigationHost.barItems(first.trailing, route)
        XCTAssertFalse(try XCTUnwrap(p.views[6]).isDescendant(of: try XCTUnwrap(p.views[8])), "lifted")
        p.apply(wireBatch([["op": "style", "id": 8, "style": ["appearance": "none", "written": ["appearance"]]]]))
        let second = try bar(p)
        XCTAssertEqual(second.authored.map(\.id), [8])
        XCTAssertTrue(second.hosted.isEmpty, "kept in the page with its wrapper")
        XCTAssertTrue(second.trailing.isEmpty)
    }

    /// The item holds the readout's own view, lifted out of the header; the
    /// next projection finds it at its place, and a bar that stops showing it
    /// puts it back there.
    func testAHostedItemHoldsTheReadoutsOwnView() throws {
        let p = route([status(4), progress(6), text(5, "3 left")], kids: [4, 5], more: [["op": "children", "id": 4, "ids": [6]]])
        let route = RouteController(try XCTUnwrap(p.views[1]))
        let element = try XCTUnwrap(p.views[4])
        let first = try bar(p)
        let item = try XCTUnwrap(p.navigationHost.barItems(first.trailing, route).first)
        let view = try XCTUnwrap(item.customView as? HostedItemView)
        XCTAssertTrue(view.node === element)
        XCTAssertTrue(element.superview === view.stand, "lifted into the item")
        XCTAssertTrue(p.navigation.lifted?.parent(of: element) === p.views[2], "the header is still its parent")
        XCTAssertEqual(try bar(p).trailing.map(\.id), [4], "found where it was")
        XCTAssertTrue(p.navigationHost.barItems(try bar(p).trailing, route).first === item, "the same item")
        if #available(iOS 26.0, *) { XCTAssertTrue(item.hidesSharedBackground, "a readout presses nothing: no glass") }
        p.navigationHost.barHosted.show([], in: route)
        XCTAssertTrue(element.superview === p.views[2]?.container, "back in the header")
        XCTAssertEqual(p.views[2]?.container.subviews.compactMap { ($0 as? NodeView)?.id }, [3, 4, 5], "at its place")
    }

    /// A readout in the heading's box, lifted into its item, leaves the title as
    /// the first projection made it: the box holds a bar item, so it is no
    /// wrapper and the filled box in it no avatar, before the lift and after.
    func testALiftedReadoutInTheHeadingsBoxLeavesTheTitleAsItWas() throws {
        let p = route([["op": "create", "id": 8, "kind": "view"],
                       ["op": "create", "id": 4, "kind": "view", "style": ["background_color": [40, 100, 200, 255], "border_radius": 16]],
                       text(5, "AB"), status(6), text(7, "Live")],
                      kids: [], more: [["op": "children", "id": 4, "ids": [5]], ["op": "children", "id": 6, "ids": [7]],
                                       ["op": "children", "id": 8, "ids": [4, 3, 6]]], header: [8])
        let route = RouteController(try XCTUnwrap(p.views[1]))
        let first = try bar(p)
        XCTAssertEqual(first.trailing.map(\.id), [6], "the readout, hosted")
        XCTAssertNil(first.group?.avatar)
        _ = p.navigationHost.barItems(first.trailing, route)
        XCTAssertFalse(try XCTUnwrap(p.views[6]).isDescendant(of: try XCTUnwrap(p.views[8])), "lifted")
        let second = try bar(p)
        XCTAssertNil(second.group?.avatar, "still no avatar")
        XCTAssertEqual(second.trailing.map(\.id), [6])
    }

    /// A second text after the subtitle is no item; once the first goes, it
    /// is the subtitle.
    func testASecondTextIsNoItemAndTheSubtitleOnceTheFirstGoes() throws {
        let p = route([text(5, "3 left"), text(6, "synced")], kids: [5, 6])
        let first = try bar(p)
        XCTAssertEqual(first.group?.subtitle, "3 left")
        XCTAssertTrue(first.trailing.isEmpty)
        p.apply(wireBatch([["op": "children", "id": 2, "ids": [3, 6]]]))
        XCTAssertEqual(try bar(p).group?.subtitle, "synced")
    }

    /// A bar item's own colour that is the tint is UIKit's tint, which follows the bar's and dims
    /// with it; a share of the tint (`Highlight`) is a colour of the button's tint, undimmed while
    /// UIKit dims tints: the item keeps the colour it is given
    func testABarItemsTintColourIsUIKitsTintOrMadeFromItsButtons() throws {
        let p = route([["op": "create", "id": 4, "kind": "button", "handlers": ["press"], "props": ["accessibilityLabel": "Edit"],
                        "style": ["text_color": ["sys": "@tint", "c": [0, 122, 255, 255]], "written": ["text_color"]]],
                       text(5, "Edit"),
                       ["op": "create", "id": 6, "kind": "button", "handlers": ["press"], "props": ["accessibilityLabel": "Mark"],
                        "style": ["text_color": ["sys": "@tint/0.2", "c": [0, 122, 255, 51]], "written": ["text_color"]]],
                       text(7, "Mark")], kids: [4, 6], more: [["op": "children", "id": 4, "ids": [5]], ["op": "children", "id": 6, "ids": [7]]])
        window.tintColor = UIColor(red: 1, green: 0, blue: 0, alpha: 1)
        func item(_ id: UInt32) throws -> HeaderShape.Item {
            let shape = try XCTUnwrap(HeaderShape(route: try XCTUnwrap(p.views[1]), back: nil, projected: true))
            return try XCTUnwrap(shape.trailing.first { $0.id == id }, "the button is a bar item: \(shape.trailing.map(\.id))")
        }
        XCTAssertTrue(try item(4).followsTint, "the tint itself: UIKit's")
        XCTAssertNil(try item(4).tint)
        func share() throws -> [Double] { try XCTUnwrap(item(6).tint?.first, "a share of the tint is a colour of its own") }
        for (got, want) in zip(try share(), [255.0, 0, 0, 51]) { XCTAssertEqual(got, want, accuracy: 2, "a fifth of the button's red tint") }
        p.viewport.tintAdjustmentMode = .dimmed
        defer { p.viewport.tintAdjustmentMode = .automatic }
        for (got, want) in zip(try share(), [255.0, 0, 0, 51]) { XCTAssertEqual(got, want, accuracy: 2, "a fifth of the red tint, undimmed, while dimmed") }
    }

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }

    /// `bar-hosted.contract` booted in a window of a phone's size, its first route shown.
    private func session() throws -> ExactSession {
        let plan = try fixturePlan("BAR_HOSTED")
        let session = ExactApp.shared.makeSession(label: "bar-hosted-\(UUID().uuidString)")
        sessions.append(session)
        let view = ExactView(session: session), host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        XCTAssertNil(session.boot(plan: plan, size: CGSize(width: 402, height: 874)).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        until("the first route") { session.presenter.navigationHost.primaryNavigation?.topViewController is RouteController }
        return session
    }

    private func node(_ testId: String, in p: Presenter) throws -> NodeView {
        try XCTUnwrap(p.views.values.first { $0.props["testId"] == testId }, testId)
    }

    private func item(_ testId: String, on c: UIViewController) throws -> UIBarButtonItem {
        let items = (c.navigationItem.leftBarButtonItems ?? []) + (c.navigationItem.rightBarButtonItems ?? [])
        return try XCTUnwrap(items.first { $0.accessibilityIdentifier == testId }, testId)
    }

    private func tap(_ point: CGPoint, with touch: TouchRunner) throws {
        var request = try TouchRunner.aim(point, in: window)
        request["op"] = "tap"
        XCTAssertEqual(try touch.ask(request)["done"] as? Bool, true)
    }

    /// A tap on a readout's words in the bar presses the header that hears
    /// `press`, as a tap on them in the page does: the press bubbles out of
    /// the item, which UIKit holds apart from the header's box (`heldApart`).
    func testATapOnAHostedReadoutPressesItsHeader() throws {
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "a touch in the bar reaches the header through UIKit")
        }
        let touch = try TouchRunner(self)
        defer { touch.connection.cancel() }
        let session = try session(), p = session.presenter
        let words = try node("sync-text", in: p), presses = try node("header-presses", in: p)
        waitFor("the readout's words are laid out in the bar") { words.window != nil && !words.bounds.isEmpty }
        XCTAssertEqual(presses.accessibleText, "0")
        try tap(words.convert(CGPoint(x: words.bounds.midX, y: words.bounds.midY), to: window), with: touch)
        waitFor("the hosted readout's tap pressed the header", cap: 5) { presses.accessibleText == "1" }
    }

    private func stops(in object: NSObject) -> [NSObject] {
        if let view = object as? UIView, view.isHidden || view.accessibilityElementsHidden { return [] }
        if object.isAccessibilityElement { return [object] }
        let children = object.accessibilityElements ?? (object as? UIView)?.subviews ?? []
        return children.compactMap { $0 as? NSObject }.flatMap { stops(in: $0) }
    }

    /// A readout is one VoiceOver stop, static text named by its label over
    /// its item, and the label follows the readout.
    func testAHostedReadoutIsOneStopNamedByItsLabel() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else {
            throw missing("scene", "UIKit lays out the bar in the app window")
        }
        let session = try session(), p = session.presenter
        let nav = try XCTUnwrap(p.navigationHost.primaryNavigation)
        let home = try XCTUnwrap(nav.topViewController)
        let ready = try XCTUnwrap(item("sync-status", on: home).customView)
        let syncing = try XCTUnwrap(item("syncing", on: home).customView)
        waitFor("both readouts are laid out in the bar") {
            [ready, syncing].allSatisfy { $0.isDescendant(of: nav.navigationBar) && !$0.bounds.isEmpty }
        }
        nav.navigationBar.layoutIfNeeded()
        XCTAssertEqual(stops(in: ready).map(\.accessibilityLabel), ["Ready"])
        XCTAssertEqual(stops(in: ready).map(\.accessibilityTraits), [.staticText])
        let spinning = stops(in: syncing)
        XCTAssertEqual(spinning.map(\.accessibilityLabel), ["Syncing"], "its spinner is no stop of its own")
        let element = try XCTUnwrap(spinning.first)
        XCTAssertEqual(element.accessibilityFrame, UIAccessibility.convertToScreenCoordinates(syncing.bounds, in: syncing))
        p.press(try node("retry", in: p).id)
        waitFor("the accessibility element's label follows the readout", cap: 5) { self.stops(in: ready).first?.accessibilityLabel == "Retrying" }
    }

    func testAHostedReadoutAnnouncesAnUpdateOnce() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else {
            throw missing("scene", "the readout announces from the bar in the app window")
        }
        let session = try session(), p = session.presenter
        let nav = try XCTUnwrap(p.navigationHost.primaryNavigation)
        let home = try XCTUnwrap(nav.topViewController)
        let view = try XCTUnwrap(item("sync-status", on: home).customView)
        waitFor("the readout is laid out in the bar") { view.isDescendant(of: nav.navigationBar) && view.window != nil && !view.bounds.isEmpty }
        waitForActivation(session)

        var announcements: [NSAttributedString] = []
        let post = Presenter.postAnnouncement
        Presenter.postAnnouncement = { announcements.append($0) }
        defer { Presenter.postAnnouncement = post }
        p.press(try node("retry", in: p).id)
        waitFor("the readout displays the updated words", cap: 5) { self.stops(in: view).first?.accessibilityLabel == "Retrying" }
        waitForMainQueue()
        XCTAssertEqual(announcements.map(\.string), ["Retrying"])
        XCTAssertEqual(announcements.first?.attribute(.accessibilitySpeechQueueAnnouncement, at: 0, effectiveRange: nil) as? Bool, true)
        p.syncAccessibility()
        XCTAssertEqual(announcements.count, 1)
    }

    /// What a link item and a readout do (LLP 1141.004 D1, D3): a link is
    /// UIKit's item and follows its `href` under the agent's tap and under a
    /// tap on the item; a readout is hosted, its entry in UIKit's overflow
    /// menu its name, dimmed. On History a native button's `color` is its
    /// item's tint, and a filled button of one compiled text is UIKit's item.
    func testALinkItemAndAReadoutDoWhatTheElementDoes() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "a link item follows through the app, which a hosted test has") }
        let session = try session(), p = session.presenter
        func node(_ testId: String) throws -> NodeView { try self.node(testId, in: p) }
        let nav = try XCTUnwrap(p.navigationHost.primaryNavigation)
        let home = try XCTUnwrap(nav.topViewController as? RouteController)
        let syncing = try item("syncing", on: home)
        XCTAssertTrue(syncing.customView is HostedItemView)
        let entry = try XCTUnwrap(syncing.menuRepresentation as? UIAction)
        XCTAssertEqual(entry.title, "Syncing")
        XCTAssertTrue(entry.attributes.contains(.disabled), "a readout does nothing")
        let link = try node("history-link")
        XCTAssertNil(try item("history-link", on: home).customView, "UIKit's own item")
        let reply = p.navigationHost.activateChrome(link)
        XCTAssertNil(reply?["error"], "\(reply ?? [:])")
        until("the agent's tap followed the link") { nav.viewControllers.count == 2 && nav.transitionCoordinator == nil }
        let history = try XCTUnwrap(nav.topViewController)
        let red = try XCTUnwrap(item("stop", on: history).tintColor, "the button's colour").resolvedColor(with: history.traitCollection)
        var (r, g, b, a) = (CGFloat(0), CGFloat(0), CGFloat(0), CGFloat(0))
        red.getRed(&r, green: &g, blue: &b, alpha: &a)
        XCTAssertTrue(r > 0.8 && g < 0.4 && b < 0.4, "system red, not the bar's tint: \(red)")
        XCTAssertNil(try item("note-button", on: history).customView, "UIKit's own item")
        try XCTUnwrap(item("about-link", on: history).target as? BarPress).press()
        until("a tap on the item followed the link") { nav.viewControllers.count == 3 }
    }
}
#endif
