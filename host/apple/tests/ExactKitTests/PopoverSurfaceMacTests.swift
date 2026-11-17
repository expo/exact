#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

/// A menu the Mac paints (the driver's, LLP 1021 D4) stands on AppKit's menu
/// panel where its author wrote no surface (expo/exact#551): an `NSMenu`'s
/// glass with its 12-point corner. The compiler says which menus those are
/// (`defaultSurface`: no background, border or shadow in any state); a menu
/// it did not mark keeps its author's surface.
final class PopoverSurfaceMacTests: XCTestCase {
    private var windows: [NSWindow] = []
    private let blue: [Double] = [0, 0, 255, 255]

    /// A page with an invoker (2) and its popover (3), painted in the top
    /// layer; in the popover an invoker (5) and its popover (6), a submenu.
    /// `marked`: the compiler's mark on (3), with the corner the host sends
    /// beside it (`menu-radius`).
    private func fixture(role: String? = "menu", marked: Bool = true, radius: String = "12", style: [String: Any]? = nil) -> Presenter {
        _ = NSApplication.shared
        let p = Presenter()
        p.menus.painted = true
        p.dialogs.painted = true
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 500, height: 400),
                              styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        windows.append(window)
        var props = ["popover": "auto", "id": "m"]
        props["accessibilityRole"] = role
        if marked { props["defaultSurface"] = "true"; props["defaultSurfaceRadius"] = radius }
        var ops: [[String: Any]] = [
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "button", "props": ["popovertarget": "m"]],
            ["op": "create", "id": 3, "kind": "view", "props": props],
            // A field keeps the popover out of an `NSMenu`'s shape, so it is painted outside the driver too.
            ["op": "create", "id": 4, "kind": "input"],
            ["op": "create", "id": 5, "kind": "button", "props": ["popovertarget": "s"]],
            ["op": "create", "id": 6, "kind": "view", "props": ["popover": "auto", "id": "s", "accessibilityRole": "menu"]],
            ["op": "create", "id": 7, "kind": "input"],
            ["op": "children", "id": 1, "ids": [2, 3]],
            ["op": "children", "id": 3, "ids": [4, 5, 6]],
            ["op": "children", "id": 6, "ids": [7]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 500, "h": 400],
            ["op": "frame", "id": 2, "x": 10, "y": 10, "w": 80, "h": 30],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 160, "h": 60],
            ["op": "frame", "id": 4, "x": 0, "y": 0, "w": 160, "h": 30],
            ["op": "frame", "id": 5, "x": 0, "y": 30, "w": 160, "h": 30],
            ["op": "frame", "id": 6, "x": 0, "y": 0, "w": 120, "h": 30],
            ["op": "frame", "id": 7, "x": 0, "y": 0, "w": 120, "h": 30],
        ]
        if let style { ops.append(["op": "style", "id": 3, "style": style]) }
        p.apply(wireBatch(ops))
        return p
    }

    private func corner(_ view: NSView?) -> CGFloat? {
        if #available(macOS 26.0, *), let glass = view as? NSGlassEffectView { return glass.cornerRadius }
        return view?.layer?.cornerRadius
    }
    private func mark(_ p: Presenter, _ on: Bool) {
        p.apply(wireBatch([on ? ["op": "props", "id": 3, "set": ["defaultSurface": "true", "defaultSurfaceRadius": "12"]]
                              : ["op": "props", "id": 3, "set": [:] as [String: String], "clear": ["defaultSurface", "defaultSurfaceRadius"]]]))
    }
    /// A `dialog` (8), with a field (9), as the last child of the menu (3).
    private func addDialog(_ p: Presenter) throws -> NodeView {
        p.apply(wireBatch([
            ["op": "create", "id": 8, "kind": "view", "props": ["semanticTag": "dialog", "id": "d"]],
            ["op": "create", "id": 9, "kind": "input"],
            ["op": "children", "id": 3, "ids": [4, 5, 6, 8]],
            ["op": "children", "id": 8, "ids": [9]],
            ["op": "frame", "id": 8, "x": 0, "y": 0, "w": 200, "h": 100],
            ["op": "frame", "id": 9, "x": 10, "y": 10, "w": 160, "h": 30],
        ]))
        return try XCTUnwrap(p.views[8])
    }
    private func ids(_ view: NSView) -> [UInt32] { view.subviews.compactMap { ($0 as? NodeView)?.id } }

    func testAnUnstyledPaintedMenuStandsOnTheMenuPanelWhileItIsOpen() throws {
        let p = fixture(), pop = try XCTUnwrap(p.views[3])
        XCTAssertNil(pop.materialView, "closed: no panel")
        p.press(2)
        XCTAssertTrue(p.menus.owns(pop), "painted")
        let panel = try XCTUnwrap(pop.materialView, "AppKit's menu panel")
        if #available(macOS 26.0, *) { XCTAssertTrue(panel is NSGlassEffectView, "an NSMenu's glass") }
        XCTAssertEqual(panel.frame, pop.bounds)
        XCTAssertEqual(corner(panel), 12)
        XCTAssertTrue(p.views[4]!.isDescendant(of: pop), "its rows stay its own")
        p.menus.close(pop)
        XCTAssertNil(pop.materialView, "back in the page: no panel")
    }

    /// The compiler leaves a menu unmarked for a surface its author wrote in
    /// any state, so the panel is absent in every state and the state's own
    /// fill is what shows.
    func testASurfaceInAnyStateIsTheMenusInEveryState() throws {
        let fill = BatchValue.array(blue.map(BatchValue.number))
        let states: [(String, [String: Any], (NodeView, Bool) -> Void)] = [
            ("base", ["background_color": blue, "written": ["background_color"]], { _, _ in }),
            ("hover", ["looks": ["hover": ["background_color": blue]]], { $0.lookState(hovered: $1) }),
            ("focus-visible", ["looks": ["focus_visible": ["background_color": blue]]], { $0.lookState(focus: $1) }),
            ("active", ["press_background_color": blue, "written": ["press_background_color"]], { $0.pressed = $1 }),
        ]
        for (name, style, hold) in states {
            let p = fixture(marked: false, style: style), pop = try XCTUnwrap(p.views[3])
            p.press(2)
            XCTAssertTrue(p.menus.owns(pop), name)
            XCTAssertNil(pop.materialView, "\(name): at rest")
            hold(pop, true)
            XCTAssertNil(pop.materialView, "\(name): held")
            XCTAssertEqual(pop.style["background_color"], fill, "\(name): its fill shows")
            hold(pop, false)
            XCTAssertNil(pop.materialView, "\(name): let go")
        }
    }

    func testTheMarkDecidesWhileTheMenuIsOpen() throws {
        let p = fixture(), pop = try XCTUnwrap(p.views[3])
        p.press(2)
        XCTAssertNotNil(pop.materialView)
        mark(p, false)
        XCTAssertNil(pop.materialView, "its author wrote a surface")
        mark(p, true)
        XCTAssertNotNil(pop.materialView, "and took it away")
    }

    /// The panel's arrival and departure move the menu's children into
    /// another container: a submenu open meanwhile stays open, and goes back
    /// among the menu's children when it closes.
    func testASurfaceChangeKeepsAnOpenSubmenu() throws {
        for marked in [true, false] {
            let p = fixture(marked: marked), pop = try XCTUnwrap(p.views[3]), sub = try XCTUnwrap(p.views[6])
            p.press(2)
            p.press(5)
            XCTAssertTrue(p.menus.owns(pop) && p.menus.owns(sub), "both open")
            mark(p, !marked)
            XCTAssertEqual(pop.materialView == nil, marked, "the panel left, or came")
            XCTAssertTrue(p.menus.owns(sub), "marked \(marked): the submenu is still open")
            p.apply(wireBatch([["op": "children", "id": 3, "ids": [4, 5, 6]]]))
            XCTAssertTrue(p.menus.owns(sub), "marked \(marked): and after its parent's children are said again")
            XCTAssertTrue(p.menus.contains(pop, sub), "and still the menu's")
            p.menus.close(sub)
            XCTAssertTrue(sub.superview === pop.container, "marked \(marked): closed, it is among the menu's children")
            XCTAssertEqual(ids(pop.container), [4, 5, 6])
            p.press(5)
            XCTAssertTrue(p.menus.owns(sub), "marked \(marked): it opens again")
        }
    }

    func testAnAuthoredCornerShapesThePanelAndAMaterialReplacesIt() throws {
        let p = fixture(style: ["border_radius": 4.0, "written": ["border_radius"]]), pop = try XCTUnwrap(p.views[3])
        p.press(2)
        XCTAssertEqual(corner(pop.materialView), 4, "a corner draws no surface: the panel takes it")
        let q = fixture(marked: false), menu = try XCTUnwrap(q.views[3])
        q.apply(wireBatch([["op": "props", "id": 3, "set": ["backgroundMaterial": "thin"]]]))
        q.press(2)
        XCTAssertTrue(menu.materialView is NSVisualEffectView, "its author's material")
    }

    /// The corner is the Mac's `menu-radius`, which the host sends with the
    /// mark: a marked menu sent none has no panel.
    func testThePanelsCornerIsTheOneTheHostSends() throws {
        let p = fixture(radius: "9"), pop = try XCTUnwrap(p.views[3])
        p.press(2)
        XCTAssertEqual(corner(pop.materialView), 9)
        p.apply(wireBatch([["op": "props", "id": 3, "set": [:] as [String: String], "clear": ["defaultSurfaceRadius"]]]))
        XCTAssertNil(pop.materialView, "no corner sent: no panel")
    }

    /// A scrolling menu's rows are its scroll view's, beside the panel and
    /// not in its content: the scroll view takes the panel's corner while the
    /// panel is there, so an opaque row scrolled against an edge stays
    /// inside the corners.
    func testAScrollingMenusRowsStayInsideThePanelsCorners() throws {
        let red: [Double] = [255, 0, 0, 255]
        let p = fixture(style: ["overflow_y": "auto"]), pop = try XCTUnwrap(p.views[3])
        p.apply(wireBatch([
            ["op": "create", "id": 8, "kind": "view", "style": ["background_color": red]],
            ["op": "children", "id": 3, "ids": [8, 4, 5, 6]],
            ["op": "frame", "id": 8, "x": 0, "y": 0, "w": 160, "h": 60],
            ["op": "frame", "id": 4, "x": 0, "y": 60, "w": 160, "h": 30],
            ["op": "frame", "id": 5, "x": 0, "y": 90, "w": 160, "h": 30],
            ["op": "content", "id": 3, "w": 160, "h": 120],
        ]))
        p.press(2)
        let scroll = try XCTUnwrap(pop.scroll)
        XCTAssertNotNil(pop.materialView, "the panel")
        scroll.contentView.scroll(to: NSPoint(x: 0, y: 20))
        func isRed(_ x: Int, _ y: Int) throws -> Bool {
            let rep = try XCTUnwrap(Capture.picture(of: pop, scale: 1))
            let c = try XCTUnwrap(rep.colorAt(x: x, y: y)?.usingColorSpace(.sRGB))
            return c.redComponent > 0.8 && c.greenComponent < 0.3 && c.blueComponent < 0.3 && c.alphaComponent > 0.8
        }
        XCTAssertTrue(try isRed(80, 20), "the row, scrolled against the top edge")
        XCTAssertFalse(try isRed(1, 1), "outside the panel's 12-point corner")
        XCTAssertEqual(scroll.layer?.cornerRadius, 12)
        XCTAssertEqual(scroll.layer?.masksToBounds, true)
        p.apply(wireBatch([["op": "style", "id": 3, "style": ["overflow_y": "auto", "border_radius": 4.0, "written": ["border_radius"]]]]))
        XCTAssertEqual(scroll.layer?.cornerRadius, 4, "an authored corner shapes the panel, and the rows with it")
        p.apply(wireBatch([["op": "style", "id": 3, "style": ["overflow_y": "auto"]]]))
        mark(p, false)
        XCTAssertNil(pop.materialView)
        XCTAssertEqual(scroll.layer?.cornerRadius, 0, "no panel: the scroll view is square again")
        mark(p, true)
        XCTAssertEqual(scroll.layer?.cornerRadius, 12)
        p.menus.close(pop)
        XCTAssertEqual(scroll.layer?.cornerRadius, 0, "back in the page")
    }

    /// A material's arrival and departure replace the container a dialog
    /// was lifted out of. Open meanwhile, it stays open, and closes back
    /// among its parent's children. The material is the author's: a menu
    /// behind its own modal dialog is inert, so its panel cannot come or go.
    func testAnOpenDialogFollowsItsParentAcrossAMaterial() throws {
        for material in [true, false] {
            let p = fixture(marked: false), pop = try XCTUnwrap(p.views[3]), dialog = try addDialog(p)
            let set = { (on: Bool) in
                p.apply(wireBatch([on ? ["op": "props", "id": 3, "set": ["backgroundMaterial": "thin"]]
                                      : ["op": "props", "id": 3, "set": [:] as [String: String], "clear": ["backgroundMaterial"]]]))
            }
            if material { set(true) }
            XCTAssertEqual(pop.materialView != nil, material)
            p.dialogs.show(dialog)
            XCTAssertTrue(p.dialogs.owns(dialog), "open")
            set(!material)
            XCTAssertEqual(pop.materialView == nil, material, "the material left, or came")
            XCTAssertTrue(p.dialogs.owns(dialog), "material \(material): the dialog is still open")
            p.apply(wireBatch([["op": "children", "id": 3, "ids": [4, 5, 6, 8]]]))
            XCTAssertTrue(p.dialogs.owns(dialog), "material \(material): and after its parent's children are said again")
            XCTAssertTrue(p.dialogs.authoredParent(of: dialog) === pop.container)
            p.dialogs.close(dialog)
            XCTAssertEqual(ids(pop.container), [4, 5, 6, 8], "material \(material): closed, it is its parent's last child again")
        }
    }

    func testAPopoverThatIsNoMenuGetsNoMenuPanel() throws {
        let p = fixture(role: nil), pop = try XCTUnwrap(p.views[3])
        p.press(2)
        XCTAssertTrue(p.menus.owns(pop))
        XCTAssertNil(pop.materialView)
    }
}
#endif
