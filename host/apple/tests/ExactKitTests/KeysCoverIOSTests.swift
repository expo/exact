#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1021 §5.4: a content popover opened from glass lies over the keys.
/// The keys' window, known by what UIKit says of it, is made clear and a
/// picture of the keys lies under the popover, while the editor keeps the
/// focus and the keys stay up as UIKit sees them.
final class KeysCoverIOSTests: XCTestCase {
    /// The keys' window is the one visible window its scene leaves out of
    /// `windows`, above the app's and as large, holding the keys; a frame it
    /// does not hold finds none.
    func testTheKeysWindowIsTheVisibleWindowNoSceneLists() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        try withKeysUp("cover-keys-window", glass: true, in: scene) { _, window, _, sampler in
            let sample = try XCTUnwrap(sampler.sample)
            let keys = try XCTUnwrap(KeyboardWindow.showing(sample.frame, over: window), "heard of as it became visible")
            XCTAssertNotNil(keys.windowScene)
            XCTAssertFalse(keys.windowScene?.windows.contains(keys) ?? true, "its own scene does not list it")
            XCTAssertGreaterThan(keys.windowLevel.rawValue, window.windowLevel.rawValue)
            XCTAssertNil(KeyboardWindow.showing(sample.frame.offsetBy(dx: 0, dy: window.bounds.height), over: window))
        }
    }

    /// Opened from glass with the keys up, the popover lies over them: their
    /// window is clear, their picture is under the popover where they show,
    /// and the field keeps the focus; a touch on their picture closes it, and
    /// the keys show again at once.
    func testAPopoverFromGlassLiesOverTheKeysOnTheirPicture() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        try withKeysUp("cover-keys-over", glass: true, in: scene) { p, window, field, sampler in
            let sample = try XCTUnwrap(sampler.sample)
            let keys = try XCTUnwrap(KeyboardWindow.showing(sample.frame, over: window))
            p.menus.invoke(try XCTUnwrap(p.views[5]))
            let entry = try XCTUnwrap(p.menus.agentOpen["card"]), pop = try XCTUnwrap(p.views[3])
            XCTAssertTrue(pop.convert(pop.bounds, to: nil).intersects(sample.frame), "placed over the keys")
            XCTAssertEqual(keys.alpha, 0, "the keys' window is clear")
            let picture = try XCTUnwrap(entry.layer.subviews.first)
            XCTAssertEqual(picture.convert(picture.bounds, to: nil), sample.frame, "their picture is where they show")
            XCTAssertNotNil(entry.layer.keysCover)
            XCTAssertTrue(field.isFirstResponder, "the field keeps the focus")
            XCTAssertEqual(sampler.sample?.frame, sample.frame, "the keys stay up as UIKit sees them")
            let onKeys = CGPoint(x: sample.frame.maxX - 10, y: sample.frame.maxY - 10)
            XCTAssertTrue(KeysCover.takes(picture, at: onKeys), "a touch on the picture is the app's, as the agent sees it")
            // The agent's check takes a row of the card over the cleared keys as over no keyboard, as its tap does
            let rowY = pop.convert(CGPoint(x: 0, y: sample.frame.midY), from: nil).y - 22
            p.apply(wireBatch([["op": "create", "id": 7, "kind": "button", "handlers": ["press"]], ["op": "children", "id": 3, "ids": [4, 7]],
                               ["op": "frame", "id": 7, "x": 10.0, "y": Double(rowY), "w": 200.0, "h": 44.0]]))
            let checked = try XCTUnwrap(Agent(session: try XCTUnwrap(p.session)).check([:])["items"] as? [[String: Any]])
            let row = try XCTUnwrap(checked.first { $0["id"] as? Int == 7 }, "the card's row is checked")
            XCTAssertNotEqual((row["cover"] as? [String: Any])?["kind"] as? String, "keyboard", "a card row over the cleared keys is over no keyboard")
            // A touch on their picture is outside the popover: it light-dismisses it
            p.menus.lightDismiss(picture)
            XCTAssertNil(p.menus.agentOpen["card"], "closed")
            XCTAssertEqual(keys.alpha, 1, "closed, the keys show at once")
            XCTAssertNil(picture.superview)
            XCTAssertFalse(KeysCover.takes(picture, at: onKeys))
            XCTAssertTrue(field.isFirstResponder)
        }
    }

    /// A real touch on the keys' picture is a touch outside the popover: it
    /// closes it, no key types under it, and the keys show again.
    func testATouchOnTheKeysPictureClosesThePopoverAndTypesNothing() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "a real touch reaches the app's window over clear keys")
        }
        let touch = try TouchRunner(self)
        defer { touch.connection.cancel() }
        try withKeysUp("cover-keys-touch", glass: true, in: scene) { p, window, field, sampler in
            let sample = try XCTUnwrap(sampler.sample)
            let keys = try XCTUnwrap(KeyboardWindow.showing(sample.frame, over: window))
            p.menus.invoke(try XCTUnwrap(p.views[5]))
            let pop = try XCTUnwrap(p.views[3])
            XCTAssertEqual(keys.alpha, 0)
            // A key's place on the picture, beside the popover's box
            let box = pop.convert(pop.bounds, to: nil)
            let point = CGPoint(x: box.maxX < sample.frame.maxX - 30 ? sample.frame.maxX - 20 : sample.frame.minX + 20, y: sample.frame.maxY - 30)
            XCTAssertFalse(box.contains(point))
            XCTAssertEqual(try touch.ask(TouchRunner.aim(point, in: window).merging(["op": "tap"]) { _, new in new })["done"] as? Bool, true)
            waitFor("the popover closes", cap: 5) { p.menus.agentOpen["card"] == nil }
            XCTAssertEqual(keys.alpha, 1)
            XCTAssertEqual(field.text ?? "", "", "no key typed under the picture")
            XCTAssertTrue(field.isFirstResponder)
        }
    }

    /// A laid cover lets itself go once its picture no longer shows (here an
    /// ancestor of the popover's layer is hidden): the keys show again by the next frame.
    func testACoverLetsGoOnceItsPictureNoLongerShows() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        try withKeysUp("cover-keys-hidden", glass: true, in: scene) { p, window, _, sampler in
            let sample = try XCTUnwrap(sampler.sample)
            let keys = try XCTUnwrap(KeyboardWindow.showing(sample.frame, over: window))
            p.menus.invoke(try XCTUnwrap(p.views[5]))
            let layer = try XCTUnwrap(p.menus.agentOpen["card"]?.layer)
            XCTAssertEqual(keys.alpha, 0)
            let host = try XCTUnwrap(layer.superview)
            host.isHidden = true
            defer { host.isHidden = false }
            waitFor("the cover lets go", cap: 2) { keys.alpha == 1 }
            XCTAssertFalse(layer.keysCover?.isLaid ?? false)
            p.menus.drop("card")
        }
    }

    /// Another field taking the keys ends the cover: the keys show at once,
    /// and the popover stays above them for the rest of its opening.
    func testAFocusTransferGivesTheKeysBackForTheOpening() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        try withKeysUp("cover-keys-transfer", glass: true, height: 450, in: scene) { p, window, _, sampler in
            let sample = try XCTUnwrap(sampler.sample)
            let keys = try XCTUnwrap(KeyboardWindow.showing(sample.frame, over: window))
            p.menus.invoke(try XCTUnwrap(p.views[5]))
            let layer = try XCTUnwrap(p.menus.agentOpen["card"]?.layer), pop = try XCTUnwrap(p.views[3])
            XCTAssertEqual(keys.alpha, 0)
            XCTAssertTrue(pop.convert(pop.bounds, to: nil).intersects(sample.frame), "over the keys")
            let other = try XCTUnwrap(Self.field(in: try XCTUnwrap(p.views[6])))
            XCTAssertTrue(other.becomeFirstResponder())
            waitFor("the cover lets go", cap: 2) { keys.alpha == 1 }
            XCTAssertTrue(layer.keysSpent)
            XCTAssertLessThanOrEqual(pop.convert(pop.bounds, to: nil).maxY, sample.frame.minY + 0.5, "placed again above the keys")
            p.menus.rehostPopovers()
            XCTAssertEqual(keys.alpha, 1, "no second cover this opening")
            XCTAssertNil(layer.keysCover)
            p.menus.drop("card")
        }
    }

    /// The scene's deactivation gives the keys back before another frame.
    func testDeactivationGivesTheKeysBackAtOnce() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        try withKeysUp("cover-keys-deactivate", glass: true, in: scene) { p, window, _, sampler in
            let sample = try XCTUnwrap(sampler.sample)
            let keys = try XCTUnwrap(KeyboardWindow.showing(sample.frame, over: window))
            p.menus.invoke(try XCTUnwrap(p.views[5]))
            XCTAssertEqual(keys.alpha, 0)
            NotificationCenter.default.post(name: UIScene.willDeactivateNotification, object: scene)
            XCTAssertEqual(keys.alpha, 1, "at once")
            p.menus.drop("card")
        }
    }

    /// Each keys' window is clear while one of its covers is laid and gets its
    /// alpha back when its last one goes, whichever goes first.
    func testEachKeysWindowGetsItsAlphaBackWithItsLastCover() {
        for order in [[0, 1], [1, 0]] {
            let windows = [UIWindow(frame: .zero), UIWindow(frame: .zero)], covers = [NSObject(), NSObject()]
            for (window, cover) in zip(windows, covers) { KeysClearing.hold(window, by: ObjectIdentifier(cover)) }
            XCTAssertEqual(windows.map(\.alpha), [0, 0])
            KeysClearing.release(ObjectIdentifier(windows[order[0]]), by: ObjectIdentifier(covers[order[0]]))
            XCTAssertEqual(windows[order[0]].alpha, 1, "its last cover gone")
            XCTAssertEqual(windows[order[1]].alpha, 0, "the other's still laid")
            KeysClearing.release(ObjectIdentifier(windows[order[1]]), by: ObjectIdentifier(covers[order[1]]))
            XCTAssertEqual(windows.map(\.alpha), [1, 1])
        }
        let window = UIWindow(frame: .zero), a = NSObject(), b = NSObject()
        KeysClearing.hold(window, by: ObjectIdentifier(a))
        KeysClearing.hold(window, by: ObjectIdentifier(b))
        KeysClearing.release(ObjectIdentifier(window), by: ObjectIdentifier(a))
        XCTAssertEqual(window.alpha, 0, "two covers of one window: clear while either is laid")
        KeysClearing.release(ObjectIdentifier(window), by: ObjectIdentifier(b))
        XCTAssertEqual(window.alpha, 1)
    }

    /// A laid cover retires once its layer no longer reaches over the keys
    /// one to one, or takes no touch there: the keys show again, the agent
    /// sees them as the keys, and the popover is placed again above them.
    func testACoverRetiresOnceItsLayerNoLongerReachesTheKeys() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        let changes: [(String, (UIView) -> Void, (UIView) -> Void)] = [
            ("moved aside", { $0.transform = CGAffineTransform(translationX: 40, y: 0) }, { $0.transform = .identity }),
            ("taking no touch", { $0.isUserInteractionEnabled = false }, { $0.isUserInteractionEnabled = true }),
            // A clear view above the layer, over the keys, takes their touches though nothing in the walk up changed
            ("under a sibling", { host in
                let over = UIView(frame: host.bounds)
                over.accessibilityIdentifier = "over-the-keys"
                // Above the layer as the host hit-tests its children, by depth (`NodeView.hitOrder`)
                over.layer.zPosition = (host.subviews.map(\.layer.zPosition).max() ?? 0) + 1
                host.addSubview(over)
            }, { host in host.subviews.first { $0.accessibilityIdentifier == "over-the-keys" }?.removeFromSuperview() }),
        ]
        for (name, change, undo) in changes {
            try withKeysUp("cover-keys-reach", glass: true, height: 450, in: scene) { p, window, _, sampler in
                let sample = try XCTUnwrap(sampler.sample)
                let keys = try XCTUnwrap(KeyboardWindow.showing(sample.frame, over: window))
                p.menus.invoke(try XCTUnwrap(p.views[5]))
                let layer = try XCTUnwrap(p.menus.agentOpen["card"]?.layer), pop = try XCTUnwrap(p.views[3])
                let picture = try XCTUnwrap(layer.subviews.first)
                XCTAssertEqual(keys.alpha, 0, name)
                let onKeys = CGPoint(x: sample.frame.maxX - 10, y: sample.frame.maxY - 10)
                XCTAssertTrue(KeysCover.takes(picture, at: onKeys), name)
                let host = try XCTUnwrap(layer.superview)
                change(host)
                defer { undo(host) }
                waitFor("the cover retires, \(name)", cap: 2) { keys.alpha == 1 }
                XCTAssertTrue(layer.keysSpent, name)
                XCTAssertFalse(KeysCover.takes(picture, at: onKeys), name)
                XCTAssertLessThanOrEqual(pop.convert(pop.bounds, to: nil).maxY, sample.frame.minY + 0.5, "\(name): placed again above the keys")
                p.menus.drop("card")
            }
        }
    }

    /// A card turned manual or given the menu role no longer lies over the keys: a tap on their picture light-dismisses,
    /// which neither takes, so the keys come back and the card is placed above them.
    func testACoverRetiresOnceItsCardIsNoLongerAContentPopover() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        for (name, props) in [("manual", ["popover": "manual"]), ("a menu", ["accessibilityRole": "menu"])] {
            try withKeysUp("cover-keys-reach", glass: true, height: 450, in: scene) { p, window, _, sampler in
                let sample = try XCTUnwrap(sampler.sample)
                let keys = try XCTUnwrap(KeyboardWindow.showing(sample.frame, over: window))
                p.menus.invoke(try XCTUnwrap(p.views[5]))
                let layer = try XCTUnwrap(p.menus.agentOpen["card"]?.layer)
                XCTAssertEqual(keys.alpha, 0, name)
                p.apply(wireBatch([["op": "props", "id": 3, "set": props]]))
                waitFor("the cover retires, \(name)", cap: 2) { keys.alpha == 1 }
                XCTAssertTrue(layer.keysSpent, name)
                p.menus.drop("card")
            }
        }
    }

    /// A cover that a callback of the alpha the keys get back lays keeps their window clear (`KeysClearing.release`).
    func testACoverLaidAsTheKeysComeBackKeepsThemClear() {
        final class Keys: UIWindow {
            var onShow: (() -> Void)?
            override var alpha: CGFloat { didSet { if alpha == 1, oldValue == 0, let show = onShow { onShow = nil; show() } } }
        }
        let window = Keys(frame: .zero), a = NSObject(), b = NSObject()
        KeysClearing.hold(window, by: ObjectIdentifier(a))
        window.onShow = { KeysClearing.hold(window, by: ObjectIdentifier(b)) }
        KeysClearing.release(ObjectIdentifier(window), by: ObjectIdentifier(a))
        XCTAssertEqual(window.alpha, 0, "the cover laid as the keys came back holds them clear")
        KeysClearing.release(ObjectIdentifier(window), by: ObjectIdentifier(b))
        XCTAssertEqual(window.alpha, 1)
    }

    /// An opening made while no editor holds the keys stays above them for
    /// the rest of its opening, even once a field takes them; their window
    /// shows no keys while they are down, and does again as they show.
    func testAnOpeningMadeWithoutTheKeysStaysAboveThem() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        try withKeysUp("cover-keys-later", glass: true, height: 450, in: scene) { p, window, field, sampler in
            let up = try XCTUnwrap(sampler.sample)
            XCTAssertTrue(field.resignFirstResponder())
            waitFor("the keys are down", cap: 5) { !(sampler.sample?.docked ?? false) || (sampler.sample?.frame.height ?? 0) == 0 }
            p.keyboardTop = nil
            XCTAssertNil(KeyboardWindow.showing(up.frame, over: window), "no keys while they are down")
            p.menus.invoke(try XCTUnwrap(p.views[5]))
            let layer = try XCTUnwrap(p.menus.agentOpen["card"]?.layer), pop = try XCTUnwrap(p.views[3])
            XCTAssertTrue(layer.keysSpent, "decided without the keys")
            XCTAssertTrue(field.becomeFirstResponder())
            waitFor("the keys are up again", cap: 15) { sampler.sample.map { $0.docked && $0.frame == up.frame } ?? false }
            waitFor("their window is known again", cap: 5) { KeyboardWindow.showing(up.frame, over: window) != nil }
            let keys = try XCTUnwrap(KeyboardWindow.showing(up.frame, over: window))
            p.keyboardTop = up.frame.minY
            p.menus.rehostPopovers()
            XCTAssertEqual(keys.alpha, 1, "no cover this opening")
            XCTAssertNil(layer.keysCover)
            XCTAssertLessThanOrEqual(pop.convert(pop.bounds, to: nil).maxY, up.frame.minY + 0.5, "above the keys")
            p.menus.drop("card")
        }
    }

    /// A popover whose role becomes a confirmation while it lies over the
    /// keys gives them back, and no rehost shows its painted card again.
    func testARehostDoesNotShowAPopoverThatBecameAConfirmation() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        try withKeysUp("cover-keys-confirm", glass: true, in: scene) { p, window, _, sampler in
            let sample = try XCTUnwrap(sampler.sample)
            let keys = try XCTUnwrap(KeyboardWindow.showing(sample.frame, over: window))
            p.menus.invoke(try XCTUnwrap(p.views[5]))
            let entry = try XCTUnwrap(p.menus.agentOpen["card"]), pop = try XCTUnwrap(p.views[3])
            XCTAssertEqual(keys.alpha, 0)
            p.apply(wireBatch([["op": "props", "id": 3, "set": ["accessibilityRole": "alertdialog"]]]))
            XCTAssertTrue(pop.isHidden, "the confirmation is UIKit's")
            waitFor("the cover lets go", cap: 2) { keys.alpha == 1 }
            p.menus.rehostPopovers()
            XCTAssertTrue(pop.isHidden, "not shown again")
            XCTAssertFalse(p.menus.lifts(entry))
            p.menus.drop("card")
        }
    }

    /// Opened from a control without glass, the popover stays above the keys, as UIKit's does.
    func testAPopoverFromPlainControlStaysAboveTheKeys() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        try withKeysUp("cover-keys-plain", glass: false, height: 200, in: scene) { p, window, _, sampler in
            let sample = try XCTUnwrap(sampler.sample)
            let keys = try XCTUnwrap(KeyboardWindow.showing(sample.frame, over: window))
            p.menus.invoke(try XCTUnwrap(p.views[5]))
            let entry = try XCTUnwrap(p.menus.agentOpen["card"]), pop = try XCTUnwrap(p.views[3])
            XCTAssertLessThanOrEqual(pop.convert(pop.bounds, to: nil).maxY, sample.frame.minY + 0.5, "above the keys")
            XCTAssertEqual(keys.alpha, 1)
            XCTAssertNil(entry.layer.keysCover)
            p.menus.drop("card")
        }
    }

    /// Runs `body` in a hosted window holding an Exact input (2) with the keys
    /// up, a second input (6), a button (5), drawn in glass when `glass`, opening a popover (3,
    /// "card") `height` points tall: at 600 it reaches the keys wherever it
    /// is placed when it may lie over them, at 200 it fits above them.
    private func withKeysUp(
        _ label: String,
        glass: Bool,
        height: Double = 600,
        in scene: UIWindowScene,
        _ body: (Presenter, UIWindow, UITextField, KeyboardSampleIOS) throws -> Void
    ) throws {
        let session = ExactApp.shared.makeSession(label: label)
        KeyboardWindow.watch() // as a session view starts it, before the keys show
        let window = UIWindow(windowScene: scene), host = UIViewController()
        window.frame = scene.coordinateSpace.bounds // as an app's: the scene's whole
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer { window.endEditing(true); window.isHidden = true; session.destroy() }
        let p = session.presenter, size = host.view.bounds.size
        p.viewport.frame = host.view.bounds
        host.view.addSubview(p.viewport)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "input", "props": ["value": ""]],
            ["op": "create", "id": 6, "kind": "input", "props": ["value": ""]],
            ["op": "create", "id": 5, "kind": "button", "handlers": ["press"], "style": ["appearance": "none"],
             "props": (glass ? ["backgroundMaterial": "glass"] : [:]).merging(["popovertarget": "card"]) { a, _ in a }],
            ["op": "create", "id": 3, "kind": "view", "props": ["popover": "auto", "id": "card"]],
            ["op": "create", "id": 4, "kind": "view"],
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 1, "ids": [2, 6, 5, 3]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": size.width, "h": size.height],
            ["op": "frame", "id": 2, "x": 64.0, "y": 120.0, "w": size.width - 84, "h": 44.0],
            ["op": "frame", "id": 5, "x": 16.0, "y": 122.0, "w": 40.0, "h": 40.0],
            ["op": "frame", "id": 6, "x": 64.0, "y": 180.0, "w": size.width - 84, "h": 44.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 260.0, "h": height],
            ["op": "frame", "id": 4, "x": 0.0, "y": 0.0, "w": 260.0, "h": height],
        ]))
        p.viewport.contentSize = size
        let field = try XCTUnwrap(Self.field(in: try XCTUnwrap(p.views[2])))
        let sampler = KeyboardSampleIOS.install(in: window)
        XCTAssertTrue(field.becomeFirstResponder())
        waitFor("the keys are up", cap: 15) {
            sampler.sample.map { $0.docked && $0.frame.maxY >= window.bounds.maxY - 0.5 } ?? false
        }
        waitFor("their window is heard of", cap: 5) { KeyboardWindow.showing(sampler.sample!.frame, over: window) != nil }
        // (UIKit gives a picture of the keys once it has drawn them where they rest.)
        waitFor("their window gives a picture", cap: 5) {
            KeyboardWindow.showing(sampler.sample!.frame, over: window)?.snapshotView(afterScreenUpdates: false) != nil
        }
        // (The keys' top, as `ExactView.fitKeyboard` gives it.)
        p.keyboardTop = sampler.sample!.frame.minY
        try body(p, window, field, sampler)
    }

    private static func field(in view: UIView) -> UITextField? {
        if let field = view as? UITextField { return field }
        return view.subviews.lazy.compactMap { field(in: $0) }.first
    }
}
#endif
