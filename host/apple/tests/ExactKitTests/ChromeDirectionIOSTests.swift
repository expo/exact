#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

/// The document's direction on UIKit's own chrome: the bars and sheets
/// Exact owns read the `layoutDirection` trait, which the document's
/// `semanticContentAttribute` does not set. The embedder's window and its
/// own controllers keep their direction.
final class ChromeDirectionIOSTests: XCTestCase {
    private var window: UIWindow?
    private var host: UIViewController?
    private var sessions: [ExactSession] = []

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        host = nil
        super.tearDown()
    }

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }

    /// The native fixture (its tab bar) in the embedder's controller, in a
    /// window of the test host's scene: UIKit presents a sheet only from one.
    /// Again, a second session in the same controller, as the sample host's.
    private func fixture(_ label: String) throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env["EXACT_FIXTURE_PLAN"], "build.mjs --test --ios compiles the fixture's plan")))
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        let view = ExactView(session: session)
        if host == nil {
            let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
            let window = UIWindow(windowScene: scene)
            window.frame = window.screen.bounds
            let host = UIViewController()
            window.rootViewController = host
            window.makeKeyAndVisible()
            self.window = window
            self.host = host
        }
        let host = try XCTUnwrap(host)
        host.view.addSubview(view)
        XCTAssertNil(session.boot(plan: plan, size: host.view.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        until("the tab container is in the window") { session.presenter.navigation.tabController?.viewIfLoaded?.window != nil }
        // A sheet waits for the first drawn frame, which this window never draws.
        if session.firstDrawMs == nil { session.firstDrawMs = ExactEnv.wall() }
        return session
    }

    func testExactsBarsAndSheetsTakeTheDocumentsDirectionAndTheWindowKeepsItsOwn() throws {
        guard UIApplication.shared.connectedScenes.contains(where: { $0 is UIWindowScene }) else {
            throw missing("scene", "a sheet is presented only from a scene's window")
        }
        let other = try fixture("chrome-direction-other")
        let session = try fixture("chrome-direction")
        let p = session.presenter
        let otherBar = try XCTUnwrap(other.presenter.navigation.tabController?.tabBar)
        let window = try XCTUnwrap(self.window), host = try XCTUnwrap(self.host)
        let tabs = try XCTUnwrap(p.navigation.tabController)
        let bar = try XCTUnwrap(p.navigation.primaryNavigation?.navigationBar)
        let own = host.traitCollection.layoutDirection
        XCTAssertEqual(own, .leftToRight, "the simulator's language is written left to right")

        p.apply(wireBatch([["op": "language", "lang": "ar", "dir": "rtl"]]))
        // Presented after the change, from the embedder's controller.
        session.presenter.press(try XCTUnwrap(p.views.values.first { $0.props["testId"] == "sheet" }).id)
        let sheet = try XCTUnwrap(host.presentedViewController, "the sheet is presented")
        until("the tab bar, the navigation bar and the sheet are right to left") {
            [tabs.tabBar.traitCollection, bar.traitCollection, sheet.traitCollection].allSatisfy { $0.layoutDirection == .rightToLeft }
        }
        XCTAssertEqual(tabs.tabBar.effectiveUserInterfaceLayoutDirection, .rightToLeft)
        XCTAssertFalse(window.traitOverrides.contains(UITraitLayoutDirection.self), "the window is the embedder's")
        XCTAssertEqual(host.traitCollection.layoutDirection, own, "so is its controller")
        XCTAssertEqual(window.traitCollection.layoutDirection, own)
        XCTAssertEqual(otherBar.traitCollection.layoutDirection, own, "the other session's tab bar is its own")

        p.apply(wireBatch([["op": "language", "lang": "en", "dir": "ltr"]]))
        until("English turns them back") {
            [tabs.tabBar.traitCollection, bar.traitCollection, sheet.traitCollection].allSatisfy { $0.layoutDirection == .leftToRight }
        }
        XCTAssertFalse(window.traitOverrides.contains(UITraitLayoutDirection.self))
    }
}

/// A grouped list is built again when its own direction turns, as its
/// cells keep the direction they were laid out in, and only then.
final class GroupedDirectionIOSTests: GroupedListFixture {
    private let look: [String: Any] = ["overflow_y": "scroll", "text_color": [0, 0, 0, 255]]

    func testAListWhoseDirectionTurnsIsBuiltAgainWhereItWasScrolled() throws {
        let p = presenter(height: 180, style: ["direction": "ltr"]) { self.model() }
        let first = try list(p)
        first.collection.layoutIfNeeded()
        XCTAssertEqual(first.collection.traitCollection.layoutDirection, .leftToRight)
        let c = first.collection, top = -c.adjustedContentInset.top
        let scrolled = top + (c.contentSize.height + c.adjustedContentInset.top + c.adjustedContentInset.bottom - c.bounds.height) / 2
        XCTAssertGreaterThan(scrolled, top + 10, "the list scrolls")
        c.setContentOffset(CGPoint(x: 0, y: scrolled), animated: false)

        // The kernel sends the new direction of each node that inherits the document's.
        p.apply(wireBatch([["op": "style", "id": 1, "style": look.merging(["direction": "rtl"]) { a, _ in a }],
                           ["op": "language", "lang": "ar", "dir": "rtl"]]))
        let turned = try list(p)
        XCTAssertFalse(turned === first, "built again")
        XCTAssertEqual(turned.collection.semanticContentAttribute, .forceRightToLeft)
        XCTAssertEqual(turned.collection.traitCollection.layoutDirection, .rightToLeft)
        turned.collection.layoutIfNeeded()
        XCTAssertEqual(turned.collection.contentOffset.y, scrolled, accuracy: 0.5, "where it was scrolled")

        // A list that says its own `dir` keeps it, and itself, as the document's turns.
        p.apply(wireBatch([["op": "language", "lang": "en", "dir": "ltr"]]))
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["testId": "list"]]]))
        XCTAssertTrue(try list(p) === turned, "not built again")
        XCTAssertEqual(turned.collection.traitCollection.layoutDirection, .rightToLeft)
    }
}
#endif
