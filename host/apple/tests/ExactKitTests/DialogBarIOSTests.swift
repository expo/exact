#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactDialogs
@testable import ExactNavigation

/// LLP 1141.004 D6: a `dialog` that is UIKit's sheet and whose first child is
/// a `header` of a heading and its buttons shows that header as the sheet's
/// navigation bar, as a hand-built form sheet does: the heading the title,
/// the closing button leading, the prominent action trailing. A header the
/// bar cannot show whole stays in the page. UIKit, so a simulator runs it:
/// bun host/apple/build.mjs --test --ios --only DialogBarIOSTests
final class DialogBarIOSTests: XCTestCase {
    private var window: UIWindow?
    private var kept: Presenter?

    override func tearDown() {
        if let p = kept, let controller = window?.rootViewController {
            p.menus.reset()
            spin { controller.presentedViewController == nil && !p.menus.inTransition }
        }
        window?.isHidden = true
        window = nil
        kept = nil
        super.tearDown()
    }

    private func spin(_ seconds: TimeInterval = 5, until done: () -> Bool) {
        let end = Date().addingTimeInterval(seconds)
        while !done(), Date() < end { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
    }

    private func native(_ id: Int, _ props: [String: String], press: Bool) -> [[String: Any]] {
        [["op": "create", "id": id, "kind": "control", "handlers": press ? ["press"] : [],
          "props": ["type": "button", "accessibilityRole": "button"].merging(props) { $1 }, "style": ["appearance": "auto"]],
         ["op": "frame", "id": id, "x": 0.0, "y": 0.0, "w": 80.0, "h": 40.0]]
    }

    /// #1316's form: a page (1) and the dialog `invite` (2), whose header (30)
    /// holds Cancel (31, `command="close"` alone), the heading (32) and Invite
    /// (33, filled, pressed), then a field (3). `extra` adds a field to the
    /// header, which the bar has no object for.
    private func page(extra: Bool = false, in scene: UIWindowScene) throws -> (Presenter, UIViewController) {
        let p = Presenter()
        kept = p
        p.buttonFace = { id in
            var f = ButtonFace()
            f.title = [31: "Cancel", 33: "Invite"][id]
            if id == 33 { f.style = "filled" }
            return f
        }
        let controller = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 400, height: 800)
        window.rootViewController = controller
        window.makeKeyAndVisible()
        p.viewport.frame = controller.view.bounds
        controller.view.addSubview(p.viewport)
        self.window = window
        var ops: [[String: Any]] = [
            ["op": "create", "id": 1, "kind": "view", "props": ["id": "page"], "handlers": []],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0],
            ["op": "create", "id": 2, "kind": "view", "handlers": [],
             "props": ["id": "invite", "semanticTag": "dialog", "accessibilityRole": "dialog", "accessibilityLabelledBy": "invite-title"]],
            ["op": "frame", "id": 2, "x": 0.0, "y": 300.0, "w": 400.0, "h": 160.0],
            ["op": "create", "id": 30, "kind": "view", "props": ["semanticTag": "header"], "handlers": []],
            ["op": "frame", "id": 30, "x": 0.0, "y": 0.0, "w": 400.0, "h": 44.0],
            ["op": "create", "id": 32, "kind": "text", "props": ["text": "Invite people", "id": "invite-title", "accessibilityRole": "heading"], "handlers": []],
            ["op": "frame", "id": 32, "x": 100.0, "y": 10.0, "w": 200.0, "h": 24.0],
            ["op": "create", "id": 3, "kind": "input", "props": ["id": "email"], "handlers": ["input"]],
            ["op": "frame", "id": 3, "x": 20.0, "y": 60.0, "w": 360.0, "h": 40.0],
            ["op": "create", "id": 34, "kind": "input", "props": ["id": "filter"], "handlers": ["input"]],
            ["op": "frame", "id": 34, "x": 20.0, "y": 0.0, "w": 60.0, "h": 40.0],
        ]
        ops += native(31, ["commandfor": "invite", "command": "close"], press: false)
        ops += native(33, [:], press: true)
        ops += [["op": "children", "id": 30, "ids": extra ? [31, 32, 34, 33] : [31, 32, 33]],
                ["op": "children", "id": 2, "ids": [30, 3]],
                ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]]]
        p.apply(wireBatch(ops))
        return (p, controller)
    }

    private static var scene: UIWindowScene? { UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first }

    func testTheHeaderIsTheSheetsBarWithCancelLeadingAndTheActionTrailing() throws {
        guard let scene = Self.scene else { throw missing("scene", "a sheet presents only in a scene's window") }
        let (p, controller) = try page(in: scene)
        p.dialogCommand("showModal", "invite")
        spin { controller.presentedViewController?.viewIfLoaded?.window != nil && !p.menus.inTransition }
        let nav = try XCTUnwrap(controller.presentedViewController as? UINavigationController, "the sheet is a navigation controller")
        let sheet = try XCTUnwrap(nav.topViewController as? DialogSheetController)
        XCTAssertFalse(nav.isNavigationBarHidden, "its bar shows")
        XCTAssertNotNil(nav.sheetPresentationController, "still UIKit's sheet")
        XCTAssertTrue(nav.view.accessibilityViewIsModal, "VoiceOver stays in the sheet, its bar included")
        let item = sheet.navigationItem
        XCTAssertEqual(item.title, "Invite people")
        XCTAssertEqual(item.largeTitleDisplayMode, .never, "a heading with no level is an inline title")
        XCTAssertEqual(item.leftBarButtonItems?.map(\.title), ["Cancel"])
        XCTAssertEqual(item.rightBarButtonItems?.map(\.title), ["Invite"])
        if #available(iOS 26.0, *) { XCTAssertEqual(item.rightBarButtonItems?.first?.style, .prominent, "the filled button is the prominent item") }
        let header = try XCTUnwrap(p.views[30])
        XCTAssertTrue(header.isHidden, "the header the bar stands for is not in the page")
        XCTAssertEqual((p.navigation as? NavigationHost)?.dialogBarCovers[30], .bar, "and takes no room")
        XCTAssertGreaterThan(sheet.scroller.adjustedContentInset.top, 0, "the dialog is laid out under the bar")
        XCTAssertEqual(try XCTUnwrap(p.views[2]).authoredLabel, "Invite people", "the hidden heading still names the dialog")

        // Cancel, which only closes the dialog, closes it from the bar.
        let cancel = try XCTUnwrap(item.leftBarButtonItems?.first)
        _ = (cancel.target as? NSObject)?.perform(cancel.action)
        spin { controller.presentedViewController == nil && !p.menus.inTransition }
        XCTAssertNil(controller.presentedViewController, "Cancel closed the sheet")
        XCTAssertFalse(header.isHidden, "the header is back in the closed dialog")
        XCTAssertNil((p.navigation as? NavigationHost)?.dialogBarCovers[30])
    }

    /// An `accent-color` written on a header's button is its item's own tint, a prominent item's
    /// fill (LLP 1069.011.000 D2), in light and dark. One written on the dialog is its bar's tint,
    /// and the items under only that one set none. The button carries the dialog's row as a
    /// control's inherited row, unwritten, which is not its own.
    func testAnAccentWrittenOnAButtonIsItsItemsTint() throws {
        guard let scene = Self.scene else { throw missing("scene", "a sheet presents only in a scene's window") }
        let (p, controller) = try page(in: scene)
        let pink: [String: Any] = ["sys": "systemPinkColor", "c": [[255, 45, 85, 255], [255, 55, 95, 255]]]
        let teal: [String: Any] = ["sys": "systemTealColor", "c": [[48, 176, 199, 255], [64, 200, 224, 255]]]
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["accent_color": teal, "written": ["accent_color"]]],
                           ["op": "style", "id": 31, "style": ["appearance": "auto", "accent_color": teal]],
                           ["op": "style", "id": 33, "style": ["appearance": "auto", "accent_color": pink, "written": ["accent_color"]]]]))
        p.dialogCommand("showModal", "invite")
        spin { controller.presentedViewController?.viewIfLoaded?.window != nil && !p.menus.inTransition }
        let nav = try XCTUnwrap(controller.presentedViewController as? UINavigationController)
        let item = try XCTUnwrap(nav.topViewController?.navigationItem)
        let invite = try XCTUnwrap(item.rightBarButtonItems?.first), cancel = try XCTUnwrap(item.leftBarButtonItems?.first)
        if #available(iOS 26.0, *) { XCTAssertEqual(invite.style, .prominent) }
        for style in [UIUserInterfaceStyle.light, .dark] {
            let traits = UITraitCollection(userInterfaceStyle: style)
            let rgba = { (c: UIColor?) -> [Int] in
                var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
                _ = c?.resolvedColor(with: traits).getRed(&r, green: &g, blue: &b, alpha: &a)
                return [r, g, b, a].map { Int(($0 * 255).rounded()) }
            }
            XCTAssertEqual(rgba(invite.tintColor), rgba(.systemPink), "the prominent item's fill is its accent (\(style.rawValue))")
            XCTAssertEqual(rgba(nav.navigationBar.tintColor), rgba(.systemTeal), "the bar's tint is the dialog's accent (\(style.rawValue))")
        }
        XCTAssertNil(cancel.tintColor, "Cancel, under only the dialog's accent, takes the bar's tint")
        p.apply(wireBatch([["op": "style", "id": 33, "style": ["appearance": "auto", "accent_color": teal]]]))
        spin { item.rightBarButtonItems?.first?.tintColor == nil }
        XCTAssertNotNil(item.rightBarButtonItems?.first, "Invite is still in the bar")
        XCTAssertNil(item.rightBarButtonItems?.first?.tintColor, "with its own accent gone, the item takes the bar's")
    }

    func testAHeaderTheBarCannotShowWholeStaysInThePage() throws {
        guard let scene = Self.scene else { throw missing("scene", "a sheet presents only in a scene's window") }
        let (p, controller) = try page(extra: true, in: scene)
        p.dialogCommand("showModal", "invite")
        spin { controller.presentedViewController?.viewIfLoaded?.window != nil && !p.menus.inTransition }
        XCTAssertTrue(controller.presentedViewController is DialogSheetController, "no navigation controller: the dialog's sheet as before")
        XCTAssertFalse(try XCTUnwrap(p.views[30]).isHidden, "the header with its field is the dialog's content")
    }
}
#endif
