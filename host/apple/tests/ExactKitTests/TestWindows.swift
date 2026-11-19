#if os(iOS)
import UIKit
import XCTest

/// Takes the windows a test showed or added to a scene out of their scenes as the test ends, so that they cannot reach
/// the tests after it in the same process. Hiding a window does not do that: XCTest keeps every test case, and any
/// window it holds, until the process exits, and a hidden window stays in its scene, where UIKit still lays it out, as
/// when the keyboard moves. Each such window, after the test's `tearDown`, stops editing, hides, leaves its scene and
/// lets go of its root view controller, which takes the test's views out of it, running their work for leaving a
/// window. A presentation the test left stays with the window, out of every scene. The windows that were in a scene as
/// the test started stay, with the scene's own window and the system's. So a window a class shows in a scene in
/// `class func setUp` stays, for the class to take out in `class func tearDown`, and so does a window shown in a scene
/// between tests, as by work a test left running.
final class TestWindows: NSObject, XCTestObservation {
    private static let shared = TestWindows()
    private let shown = NSHashTable<UIWindow>.weakObjects()
    private let before = NSHashTable<UIWindow>.weakObjects()
    private var testing = false

    /// From `installTestComposition`, as the bundle loads
    static func install() {
        XCTestObservationCenter.shared.addTestObserver(shared)
        NotificationCenter.default.addObserver(forName: UIWindow.didBecomeVisibleNotification, object: nil, queue: nil) { note in
            guard shared.testing, let window = note.object as? UIWindow else { return }
            shared.shown.add(window)
        }
    }

    func testCaseWillStart(_ testCase: XCTestCase) {
        testing = true
        before.removeAllObjects()
        for window in Self.inScenes() { before.add(window) }
    }

    func testCaseDidFinish(_ testCase: XCTestCase) {
        testing = false
        for window in Self.inScenes() { shown.add(window) }
        for window in shown.allObjects where !before.contains(window) && Self.made(window) {
            window.endEditing(true)
            window.isHidden = true
            window.windowScene = nil
            window.rootViewController = nil
        }
        shown.removeAllObjects()
        before.removeAllObjects()
    }

    private static func inScenes() -> [UIWindow] {
        UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.flatMap(\.windows)
    }

    /// Whether a test or Exact made `window`: a plain `UIWindow` or one of their own classes, and neither its scene's own
    /// window, which the app keeps, nor a system framework's, which the framework may keep and show again
    private static func made(_ window: UIWindow) -> Bool {
        if let scene = window.windowScene, let own = (scene.delegate as? UIWindowSceneDelegate)?.window, own === window { return false }
        let kind = type(of: window)
        return kind == UIWindow.self || !Bundle(for: kind).bundlePath.contains("/System/Library/")
    }
}
#endif
