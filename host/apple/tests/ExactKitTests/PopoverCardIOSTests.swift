#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1021 §5.4: while a card lies over the keys, they stand down behind a
/// picture of themselves taken from the window UIKit draws them in, which is
/// known by what UIKit says of it, not by its class.
final class PopoverCardIOSTests: XCTestCase {
    /// The keys' window is the one visible window no scene lists in its
    /// `windows`, above the app's, holding the keys' frame; a frame it does
    /// not hold finds none.
    func testTheKeysWindowIsTheVisibleWindowNoSceneLists() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        try withKeysUp("card-keys-window", in: scene) { p, window, _, sampler, _ in
            let sample = try XCTUnwrap(sampler.sample)
            let keys = try XCTUnwrap(KeyboardWindow.showing(sample.frame, over: window), "heard of as it became visible")
            XCTAssertFalse(keys.windowScene?.windows.contains(keys) ?? false, "no scene lists it")
            XCTAssertGreaterThan(keys.windowLevel.rawValue, window.windowLevel.rawValue)
            XCTAssertNil(KeyboardWindow.showing(sample.frame.offsetBy(dx: 0, dy: window.bounds.height), over: window))
            XCTAssertNotNil(p.keyboardEditor)
        }
    }

    /// The keys stand down behind their picture: the editor resigns, yet the
    /// app still sees its focus, its editor and the keys' room; they come
    /// back under a second picture laid in their own window, which goes once
    /// they show, and no blur is reported.
    func testKeysStandDownBehindTheirPictureAndComeBack() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        try withKeysUp("card-keys-back", in: scene) { p, window, field, sampler, blurs in
            let sample = try XCTUnwrap(sampler.sample)
            let keysWindow = try XCTUnwrap(KeyboardWindow.showing(sample.frame, over: window))
            let views = window.subviews.count
            let keys = try XCTUnwrap(StoodDownKeys(presenter: p))
            XCTAssertFalse(field.isFirstResponder)
            XCTAssertTrue(p.heldKeys === keys)
            XCTAssertTrue(p.keyboardEditor === field, "the editor the keys belong to")
            XCTAssertEqual(sampler.sample?.frame, sample.frame, "the keys' room is kept")
            XCTAssertEqual(window.subviews.count, views + 1, "their picture")
            XCTAssertEqual(window.subviews.last?.frame, sample.frame)
            waitFor("the real keys go") { keysWindow.isHidden }
            var covered = false
            let center = NotificationCenter.default
            // (After the stand-down's own observer, which this one precedes.)
            let shows = center.addObserver(forName: UIResponder.keyboardWillShowNotification, object: nil, queue: .main) { _ in
                DispatchQueue.main.async { covered = keysWindow.subviews.contains { $0.clipsToBounds && $0.frame == sample.frame } }
            }
            defer { center.removeObserver(shows) }
            keys.bringBack()
            waitFor("the keys are back", cap: 5) { p.heldKeys == nil }
            XCTAssertTrue(field.isFirstResponder)
            XCTAssertTrue(covered, "a picture covered them in their own window as they came")
            XCTAssertFalse(keysWindow.subviews.contains { $0.clipsToBounds && $0.frame == sample.frame }, "and went once they showed")
            XCTAssertEqual(window.subviews.count, views)
            XCTAssertEqual(blurs(), [])
        }
    }

    /// A blur while the keys are down lets them go as the card closes: the
    /// picture slides away, the room is let go and the blur is reported then.
    func testABlurWhileTheKeysAreDownLetsThemGo() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the keys show only in a scene") }
        try withKeysUp("card-keys-blur", in: scene) { p, window, field, sampler, blurs in
            let keys = try XCTUnwrap(StoodDownKeys(presenter: p))
            XCTAssertTrue(keys.blur([]))
            XCTAssertEqual(blurs(), [], "held till the card closes")
            keys.bringBack()
            waitFor("the keys are let go", cap: 5) { p.heldKeys == nil && blurs() == [2] }
            XCTAssertFalse(field.isFirstResponder)
            XCTAssertNil(sampler.held)
        }
    }

    /// With no keys up nothing stands down, and a card lies above where the keys would be.
    func testNoKeysStandsNothingDown() {
        let session = ExactApp.shared.makeSession(label: "card-no-keys")
        defer { session.destroy() }
        XCTAssertNil(StoodDownKeys(presenter: session.presenter))
    }

    /// Runs `body` in a hosted window whose Exact input (node 2, which
    /// reports its blurs) has the keys up.
    private func withKeysUp(
        _ label: String,
        in scene: UIWindowScene,
        _ body: (Presenter, UIWindow, UITextField, KeyboardSampleIOS, () -> [UInt32]) throws -> Void
    ) throws {
        let session = ExactApp.shared.makeSession(label: label)
        let window = UIWindow(windowScene: scene), host = UIViewController()
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer { window.endEditing(true); window.isHidden = true; session.destroy() }
        let p = session.presenter, size = host.view.bounds.size
        p.viewport.frame = host.view.bounds
        host.view.addSubview(p.viewport)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "input", "handlers": ["blur"], "props": ["value": ""]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": size.width, "h": size.height],
            ["op": "frame", "id": 2, "x": 20.0, "y": 120.0, "w": size.width - 40, "h": 44.0],
        ]))
        var blurs: [UInt32] = []
        p.onBlur = { blurs.append($0) }
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
        try body(p, window, field, sampler, { blurs })
    }

    private static func field(in view: UIView) -> UITextField? {
        if let field = view as? UITextField { return field }
        return view.subviews.lazy.compactMap { field(in: $0) }.first
    }
}
#endif
