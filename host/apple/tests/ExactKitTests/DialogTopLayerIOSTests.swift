#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1035.001 D11 (#109): a `dialog` whose author painted its surface is
/// the author's box in HTML's top layer, not the platform's sheet or alert:
/// presented full screen over whatever is presented, a sheet included, with
/// no transition of UIKit's, laid out as written and centred in the window,
/// over UIKit's dimming; `closedby="any"` closes it on a tap outside its
/// box; a removal with an exit keeps it shown until the exit ends.
/// UIKit, so a simulator runs it: bun host/apple/build.mjs --test --ios
final class DialogTopLayerIOSTests: XCTestCase {
    /// A touch at a point of a view.
    private final class At: UITouch {
        let at: CGPoint, on: UIView
        init(_ at: CGPoint, on: UIView) { self.at = at; self.on = on; super.init() }
        override var view: UIView? { on }
        override func location(in view: UIView?) -> CGPoint { on.convert(at, to: view) }
    }
    private var window: UIWindow?
    private var kept: Presenter?
    private var session: ExactSession?

    override func tearDown() {
        if let p = kept, let controller = window?.rootViewController {
            spin { !p.menus.inTransition }
            p.menus.reset()
            spin { controller.presentedViewController == nil && !p.menus.inTransition }
        }
        session?.destroy()
        session = nil
        window?.isHidden = true
        window = nil
        kept = nil
        super.tearDown()
    }

    private func spin(_ seconds: TimeInterval = 3, until done: () -> Bool) {
        let end = Date().addingTimeInterval(seconds)
        while !done(), Date() < end { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
    }

    private func native(_ id: Int, _ props: [String: String], press: Bool, y: Double) -> [[String: Any]] {
        [["op": "create", "id": id, "kind": "control", "handlers": press ? ["press"] : [],
          "props": ["type": "button", "accessibilityRole": "button"].merging(props) { $1 }, "style": ["appearance": "auto"]],
         ["op": "frame", "id": id, "x": 20.0, "y": y, "w": 80.0, "h": 40.0]]
    }
    private static let painted: [String: Any] = ["background_color": [20, 23, 29, 255], "border_radius": 30.0, "written": ["background_color", "border_radius"]]

    /// A page (1) holding `form` (2), a bare dialog with a field, which is a
    /// sheet; `ask` (30), a painted dialog 300 by 180 of a text, Clear (32)
    /// and Cancel (33), the alert's shape; and `card` (40), painted, not of
    /// the alert's shape.
    /// The scene a presentation completes in, only in its window.
    private var scene: UIWindowScene? { UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first }
    private func page(_ scene: UIWindowScene, ask: [String: String] = [:]) -> (Presenter, UIViewController) {
        let p = Presenter(), controller = UIViewController()
        kept = p
        p.buttonFace = { id in var f = ButtonFace(); f.title = [32: "Clear", 33: "Cancel", 42: "Done"][id]; return f }
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 800)
        window.rootViewController = controller
        window.makeKeyAndVisible()
        p.viewport.frame = controller.view.bounds
        controller.view.addSubview(p.viewport)
        self.window = window
        let closes = ["commandfor": "ask", "command": "close"]
        var ops: [[String: Any]] = [
            ["op": "create", "id": 1, "kind": "view", "props": ["id": "page"], "handlers": []],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0],
            ["op": "create", "id": 2, "kind": "view", "handlers": [], "props": ["id": "form", "semanticTag": "dialog"]],
            ["op": "frame", "id": 2, "x": 10.0, "y": 300.0, "w": 300.0, "h": 180.0],
            ["op": "create", "id": 3, "kind": "input", "props": ["id": "name"], "handlers": ["input"]],
            ["op": "frame", "id": 3, "x": 20.0, "y": 20.0, "w": 260.0, "h": 40.0],
            ["op": "create", "id": 30, "kind": "view", "handlers": [], "style": Self.painted,
             "props": ["id": "ask", "semanticTag": "dialog", "accessibilityRole": "alertdialog", "accessibilityLabel": "Clear history?"].merging(ask) { $1 }],
            ["op": "frame", "id": 30, "x": 0.0, "y": 500.0, "w": 300.0, "h": 180.0],
            ["op": "create", "id": 31, "kind": "text", "props": ["text": "Clear history?"], "handlers": []],
            ["op": "frame", "id": 31, "x": 20.0, "y": 20.0, "w": 260.0, "h": 20.0],
            ["op": "create", "id": 40, "kind": "view", "handlers": [], "style": Self.painted, "props": ["id": "card", "semanticTag": "dialog"]],
            ["op": "frame", "id": 40, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0],
            ["op": "create", "id": 41, "kind": "input", "props": ["id": "note"], "handlers": ["input"]],
            ["op": "frame", "id": 41, "x": 20.0, "y": 20.0, "w": 260.0, "h": 40.0],
        ]
        ops += native(32, closes, press: true, y: 60) + native(33, closes, press: false, y: 110)
        ops += native(42, ["commandfor": "card", "command": "close"], press: false, y: 80)
        ops += [["op": "children", "id": 2, "ids": [3]], ["op": "children", "id": 30, "ids": [31, 32, 33]], ["op": "children", "id": 40, "ids": [41, 42]],
                ["op": "children", "id": 1, "ids": [2, 30, 40]], ["op": "roots", "ids": [1]]]
        p.apply(wireBatch(ops))
        return (p, controller)
    }

    func testAPaintedDialogIsTheTopLayerOverAPresentedSheet() throws {
        guard let scene else { throw missing("scene", "a presentation completes only in a scene's window") }
        let (p, controller) = page(scene)
        p.dialogCommand("showModal", "form")
        spin { !p.menus.inTransition }
        let form = try XCTUnwrap(controller.presentedViewController as? DialogSheetController, "a bare dialog is a sheet")
        XCTAssertFalse(form.topLayer)
        XCTAssertEqual(form.modalPresentationStyle, .pageSheet)

        p.dialogCommand("showModal", "ask")
        spin { !p.menus.inTransition }
        let top = try XCTUnwrap(form.presentedViewController as? DialogSheetController, "presented over the sheet, not as an alert")
        XCTAssertTrue(top.topLayer)
        XCTAssertEqual(top.modalPresentationStyle, .overFullScreen)
        let ask = try XCTUnwrap(p.views[30])
        XCTAssertTrue(ask.superview === top.scroller, "its own subtree, laid out as written")
        XCTAssertEqual(ask.bounds.size, CGSize(width: 300, height: 180))
        top.view.layoutIfNeeded()
        XCTAssertEqual(ask.center.x, top.scroller.bounds.midX, accuracy: 0.5, "centred across the window")
        XCTAssertEqual(ask.center.y, top.scroller.bounds.midY, accuracy: 0.5, "and down it")
        XCTAssertEqual(top.view.backgroundColor, DialogSheetController.dimming, "over UIKit's dimming")
        XCTAssertTrue(ask.accessibilityViewIsModal)
        XCTAssertEqual(p.menus.sheets.observation?["presentation"] as? String, "top-layer")
        let block = try XCTUnwrap(p.menus.sheets.containingBlocks.first { $0.0 == 30 }?.1)
        XCTAssertEqual(block, HostBlock(width: .size(top.scroller.bounds.width), height: .fit(top.view.bounds.height)), "its block is the window")

        p.dialogCommand("close", "ask")
        spin { form.presentedViewController == nil }
        XCTAssertNil(form.presentedViewController, "closed at once")
        XCTAssertTrue(controller.presentedViewController === form, "the sheet stays")
    }

    /// `closedby="any"` closes it on a tap outside its box, never inside it;
    /// without it a tap outside does nothing.
    func testATapOutsideItsBoxIsItsCloseRequestUnderClosedbyAny() throws {
        guard let scene else { throw missing("scene", "a presentation completes only in a scene's window") }
        for closedby in ["any", "closerequest"] {
            let (p, controller) = page(scene, ask: ["closedby": closedby])
            p.dialogCommand("showModal", "ask")
            spin { !p.menus.inTransition }
            let top = try XCTUnwrap(controller.presentedViewController as? DialogSheetController)
            top.view.layoutIfNeeded()
            let ask = try XCTUnwrap(p.views[30])
            let tap = UITapGestureRecognizer()
            XCTAssertEqual(top.gestureRecognizer(tap, shouldReceive: At(CGPoint(x: 5, y: 5), on: top.view)), closedby == "any", closedby)
            XCTAssertFalse(top.gestureRecognizer(tap, shouldReceive: At(ask.center, on: top.scroller)), "\(closedby): inside its box")
            tearDown()
        }
    }

    /// A painted dialog of any shape is the top layer, a field in it included.
    func testAPaintedFormIsTheTopLayerToo() throws {
        guard let scene else { throw missing("scene", "a presentation completes only in a scene's window") }
        let (p, controller) = page(scene)
        p.dialogCommand("showModal", "card")
        spin { !p.menus.inTransition }
        let top = try XCTUnwrap(controller.presentedViewController as? DialogSheetController)
        XCTAssertTrue(top.topLayer)
        XCTAssertTrue(p.views[40]?.superview === top.scroller)
    }

    /// The dialog fixture (`dialog-sheet-reads.contract`) booted in a scene's window.
    private func boot(_ scene: UIWindowScene) throws -> ExactSession {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_DIALOG_SHEET_READS_PLAN"], "build.mjs --test --ios compiles the fixture's plan")
        let session = ExactApp.shared.makeSession(label: "dialog-top-layer")
        self.session = session
        let view = ExactView(session: session), host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        spin { session.state == .ready && session.presenter.root.window === window && !session.presenter.viewport.bounds.isEmpty }
        kept = session.presenter
        return session
    }

    func testPointReadsFollowTheNativeBackdropBelowAPopover() throws {
        guard let scene else { throw missing("scene", "a modal backdrop needs a scene's window") }
        let session = try boot(scene), p = session.presenter
        p.dialogCommand("showModal", "responsive")
        spin { !p.menus.inTransition }
        p.dialogCommand("showModal", "painted-point")
        spin { !p.menus.inTransition }
        let sheet = try XCTUnwrap(window?.rootViewController?.presentedViewController?.presentedViewController as? DialogSheetController)
        let dialog = try XCTUnwrap(p.views.values.first { $0.props["id"] == "painted-point" })
        let outside = CGPoint(x: 5, y: 5)
        func read(_ point: CGPoint, hit: String, hits: String, file: StaticString = #filePath, line: UInt = #line) {
            let at = p.viewportPoints(sheet.view.convert(CGRect(origin: point, size: .zero), to: p.viewport)).origin
            let sample = PointerSample(x: 0, y: 0, buttons: 0, pressure: 0, type: "mouse", id: 1, clientX: at.x, clientY: at.y)
            session.apply(session.runtime.pointer(dialog.id, .move, sample, now: session.now()))
            let state = session.runtime.agent("{\"op\":\"state\"}")
            let slots = ((try? JSONSerialization.jsonObject(with: Data(state.utf8))) as? [String: Any])?["slots"] as? [String: Any]
            XCTAssertEqual(slots?["hit"] as? String, hit, file: file, line: line)
            XCTAssertEqual(slots?["hits"] as? String, hits, file: file, line: line)
        }
        XCTAssertTrue(sheet.view.hitTest(outside, with: nil) === sheet.scroller)
        read(outside, hit: "painted-point", hits: "painted-point")
        // The dialog refuses its own hit, but its native backdrop still accepts it.
        let center = sheet.scroller.convert(CGPoint(x: dialog.bounds.midX, y: dialog.bounds.midY), from: dialog)
        XCTAssertTrue(sheet.scroller.hitTest(center, with: nil) === sheet.scroller)
        read(sheet.view.convert(center, from: sheet.scroller), hit: "painted-point", hits: "painted-point")
        p.menus.action("showPopover", ["point-pop"])
        let pop = try XCTUnwrap(p.views.values.first { $0.props["id"] == "point-pop" })
        let popAt = pop.convert(CGPoint(x: pop.bounds.midX, y: pop.bounds.midY), to: sheet.view)
        XCTAssertFalse(dialog.convert(dialog.bounds, to: sheet.view).contains(popAt), "the popover must be over the backdrop")
        let delivered = window?.hitTest(sheet.view.convert(popAt, to: window), with: nil)
        XCTAssertTrue(delivered === pop || delivered?.isDescendant(of: pop) == true, "\(String(describing: delivered))")
        read(popAt, hit: "point-pop", hits: "point-pop,painted-point")
        p.menus.drop("point-pop")
        spin { !p.menus.inTransition }
        sheet.view.isUserInteractionEnabled = false
        XCTAssertNil(sheet.view.hitTest(outside, with: nil))
        read(outside, hit: "none", hits: "")
        sheet.view.isUserInteractionEnabled = true
        let beyond = CGPoint(x: sheet.view.bounds.maxX + 1, y: 5)
        XCTAssertNil(sheet.view.hitTest(beyond, with: nil))
        read(beyond, hit: "none", hits: "")
    }

    /// A painted dialog taller than the window stops at the window (the
    /// modal dialog's `max-height`) and scrolls to what overflows it, a flat
    /// child and the Close button below it, as the sheet does.
    func testATallPaintedDialogScrollsToItsBottom() throws {
        guard let scene else { throw missing("scene", "a presentation completes only in a scene's window") }
        let p = try boot(scene).presenter
        p.dialogCommand("showModal", "painted-tall")
        spin { !p.menus.inTransition }
        let top = try XCTUnwrap(window?.rootViewController?.presentedViewController as? DialogSheetController)
        XCTAssertTrue(top.topLayer)
        top.view.layoutIfNeeded()
        let scroller = top.scroller
        let dialog = try XCTUnwrap(p.views.values.first { $0.props["id"] == "painted-tall" })
        let close = try XCTUnwrap(p.views.values.first { $0.props["id"] == "painted-tall-close" })
        XCTAssertLessThan(dialog.bounds.height, 2000, "capped at the window")
        let reach = dialog.convert(close.frame, to: scroller).maxY
        XCTAssertGreaterThan(reach, scroller.bounds.height, "its Close button is past the window")
        XCTAssertGreaterThanOrEqual(scroller.contentSize.height, reach - 0.5, "and within the scroll range")
        scroller.setContentOffset(CGPoint(x: 0, y: scroller.contentSize.height - scroller.bounds.height), animated: false)
        XCTAssertLessThanOrEqual(dialog.convert(close.frame, to: scroller).maxY, scroller.bounds.maxY + 0.5, "scrolled to, it shows")
        // `closedby="any"`: a tap on the control below the dialog's box is the
        // dialog's, never the backdrop's; one beside the dialog is.
        let tap = UITapGestureRecognizer()
        XCTAssertFalse(top.gestureRecognizer(tap, shouldReceive: At(CGPoint(x: close.bounds.midX, y: close.bounds.midY), on: close)), "the revealed Close is no backdrop")
        XCTAssertTrue(top.gestureRecognizer(tap, shouldReceive: At(CGPoint(x: scroller.bounds.maxX - 4, y: scroller.bounds.midY), on: scroller)), "beside the dialog is")
    }

    /// A child 100 points tall whose content shows 2,000 points past it
    /// (visible overflow) reaches the scroll range too, its Close included.
    func testADescendantsVisibleOverflowIsInTheScrollRange() throws {
        guard let scene else { throw missing("scene", "a presentation completes only in a scene's window") }
        let p = try boot(scene).presenter
        p.dialogCommand("showModal", "painted-deep")
        spin { !p.menus.inTransition }
        let top = try XCTUnwrap(window?.rootViewController?.presentedViewController as? DialogSheetController)
        top.view.layoutIfNeeded()
        let scroller = top.scroller
        let close = try XCTUnwrap(p.views.values.first { $0.props["id"] == "painted-deep-close" })
        let parent = try XCTUnwrap(close.superview as? NodeView)
        XCTAssertEqual(parent.bounds.height, 100, accuracy: 0.5, "a 100-point child")
        let reach = close.convert(close.bounds, to: scroller).maxY
        XCTAssertGreaterThan(reach, 2000, "its Close shows 2,000 points below it")
        XCTAssertGreaterThanOrEqual(scroller.contentSize.height, reach - 0.5, "within the scroll range")
    }

    /// The top layer resolves its presenter's theme, not the last one to
    /// apply, and a live edit retints it and leaves its dimming.
    func testTheTopLayerTakesItsPresentersThemeAndALiveEdit() throws {
        guard let scene else { throw missing("scene", "a presentation completes only in a scene's window") }
        let (p, controller) = page(scene)
        let other = Presenter()
        defer {
            _ = ThemeRoles.apply(wireBatch([["op": "roles", "roles": [String: Any]()]]), root: p.root)
            _ = ThemeRoles.apply(wireBatch([["op": "roles", "roles": [String: Any]()]]), root: other.root)
        }
        _ = ThemeRoles.apply(wireBatch([["op": "roles", "roles": ["AccentColor": [0, 200, 0, 255]]]]), root: p.root)
        p.applyThemeTint()
        p.dialogCommand("showModal", "ask")
        spin { !p.menus.inTransition }
        let top = try XCTUnwrap(controller.presentedViewController as? DialogSheetController)
        // Another session's theme applies last.
        _ = ThemeRoles.apply(wireBatch([["op": "roles", "roles": ["AccentColor": [200, 0, 0, 255]]]]), root: other.root)
        func rgb(_ c: UIColor?) throws -> [CGFloat] {
            var (r, g, b, a): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
            XCTAssertTrue(try XCTUnwrap(c).getRed(&r, green: &g, blue: &b, alpha: &a))
            return [r, g, b]
        }
        XCTAssertTrue(ThemeRoles.adopted(by: p.root).contains { $0 === top.view }, "its presenter adopted it")
        let ask = try XCTUnwrap(p.views[30])
        for (got, want) in zip(try rgb(ThemeRoles.color("AccentColor", in: ask)), [0, 200.0 / 255, 0]) { XCTAssertEqual(got, want, accuracy: 0.01, "its presenter's accent") }
        for (got, want) in zip(try rgb(top.view.tintColor), [0, 200.0 / 255, 0]) { XCTAssertEqual(got, want, accuracy: 0.01, "and tint") }
        _ = ThemeRoles.apply(wireBatch([["op": "roles", "roles": ["AccentColor": [0, 0, 200, 255]]]]), root: p.root)
        p.applyThemeTint()
        for (got, want) in zip(try rgb(top.view.tintColor), [0, 0, 200.0 / 255]) { XCTAssertEqual(got, want, accuracy: 0.01, "a live edit retints it") }
        XCTAssertEqual(top.view.backgroundColor, DialogSheetController.dimming, "and keeps its dimming")
    }

    /// Removed with an exit, it stays in the top layer until the exit ends,
    /// so its `-exact-exit-animation` is seen; then the presentation goes.
    func testAnExitPlaysInTheTopLayer() throws {
        guard let scene else { throw missing("scene", "a presentation completes only in a scene's window") }
        let (p, controller) = page(scene)
        p.dialogCommand("showModal", "ask")
        spin { !p.menus.inTransition }
        let top = try XCTUnwrap(controller.presentedViewController as? DialogSheetController)
        let ask = try XCTUnwrap(p.views[30])
        p.apply(wireBatch([["op": "exit", "id": 30], ["op": "children", "id": 1, "ids": [2, 40]]]))
        XCTAssertTrue(p.isLeaving(ask))
        XCTAssertTrue(ask.superview === top.scroller, "it plays its exit where it is shown")
        XCTAssertTrue(controller.presentedViewController === top)
        p.apply(wireBatch([["op": "destroy", "id": 30]]))
        spin { controller.presentedViewController == nil }
        XCTAssertNil(controller.presentedViewController, "the exit ended: the top layer goes")
        XCTAssertNil(ask.superview)
    }
}
#endif
