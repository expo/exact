#if os(iOS)
import ExactTips
import TipKit
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1035.006.000 over `scripts/fixtures/tips.contract`: a tip is TipKit's
/// popover, over the bar item UIKit draws for a header's button and over a
/// button in the page; one shows at a time, in document order; pressing the
/// button (`tipDone`) retires its tip for good, and a session made again
/// shows neither.
final class TipsIOSTests: XCTestCase {
    private var installed: TipCapability?
    private var window: UIWindow?
    private var sessions: [ExactSession] = []

    override func setUp() {
        super.setUp()
        installed = TipLink.installed
        // What an earlier run retired is in TipKit's store until it is reset,
        // which TipKit allows only before it is configured.
        try? Tips.resetDatastore()
        ExactTips.install()
    }

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        TipLink.installed = installed
        super.tearDown()
    }

    private func boot(_ label: String, in scene: UIWindowScene) throws -> ExactSession {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_TIPS_PLAN"], "build.mjs --test --ios compiles the tips fixture")
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = scene.screen.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window?.isHidden = true
        self.window = window
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    /// The tip popover showing in `window`, and what it points at.
    private func shownTip() -> (TipUIPopoverViewController, UIPopoverPresentationControllerSourceItem?)? {
        var controller = window?.rootViewController
        while let c = controller {
            if let tip = c as? TipUIPopoverViewController, !tip.isBeingDismissed { return (tip, tip.popoverPresentationController?.sourceItem) }
            controller = c.presentedViewController ?? c.children.first { $0.presentedViewController != nil }
        }
        return nil
    }

    private func tips(_ session: ExactSession) -> [String: [String: Any]] {
        let rows = TipLink.installed?.state(owner: session.presenter) ?? []
        return Dictionary(uniqueKeysWithValues: rows.map { ($0["id"] as? String ?? "", $0) })
    }

    /// A session destroyed with its tip showing dismisses the popover and
    /// leaves nothing of itself in the process's Tips host.
    func testADestroyedSessionClosesItsTipAndKeepsNothing() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a popover is presented in a window scene") }
        let session = try boot("tips-destroyed", in: scene)
        let presenter = session.presenter
        waitFor("the Filter tip shows") { self.shownTip() != nil }
        session.destroy()
        sessions.removeAll { $0 === session }
        XCTAssertTrue(TipLink.installed?.state(owner: presenter).isEmpty == true, "no tip kept for the destroyed session")
        waitFor("its popover dismissed") { self.shownTip() == nil }
    }

    func testATipShowsOverItsBarItemThenItsElementAndRetiresWhenDone() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a popover is presented in a window scene") }
        let session = try boot("tips", in: scene)
        let p = session.presenter
        let filter = try node(session, "filter"), add = try node(session, "add")
        let item = try XCTUnwrap(p.navigation.tipItem(filter)?.0, "the header's button is a bar item")

        waitFor("the Filter tip over its bar item") { (self.shownTip()?.1 as AnyObject?) === item }
        XCTAssertEqual(tips(session)["filter-tip"]?["shows"] as? Bool, true)
        XCTAssertEqual(tips(session)["filter-tip"]?["over"] as? String, "bar item")
        XCTAssertEqual(tips(session)["add-tip"]?["shows"] as? Bool, false, "one tip at a time, in document order")

        // Pressing Filter is what its tip teaches: it is retired, and Add's shows.
        _ = Agent(session: session).tap(["id": Int(filter.id)])
        waitFor("the Add tip over its button") { (self.shownTip()?.1 as AnyObject?) === add }
        // TipKit records the invalidation in its own time; the next tip shows at once.
        waitFor("TipKit retires the Filter tip") { self.tips(session)["filter-tip"]?["status"] as? String == "invalidated" }
        XCTAssertEqual(tips(session)["filter-tip"]?["reason"] as? String, "actionPerformed")
        XCTAssertEqual(tips(session)["add-tip"]?["over"] as? String, "element")

        _ = Agent(session: session).tap(["id": Int(add.id)])
        waitFor("no tip shows once both are done") { self.shownTip() == nil && self.tips(session)["add-tip"]?["status"] as? String == "invalidated" }

        // TipKit remembers: the app made again shows neither.
        let again = try boot("tips-again", in: scene)
        waitFor("the tips read back retired") { self.tips(again).values.allSatisfy { $0["status"] as? String == "invalidated" } && self.tips(again).count == 2 }
        waitForMainQueue()
        XCTAssertNil(shownTip(), "a retired tip does not show again")
    }
}
#endif
