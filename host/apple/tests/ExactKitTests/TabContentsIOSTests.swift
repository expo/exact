#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation
@testable import ExactNativeViews

/// LLP 1075.003 §3.6: a tab container the app owns sends Exact its intents
/// (`ExactTabContents.back`, `open`), which Exact applies as the platform and
/// the router do. Over the native fixture's segmented container
/// (`EXACT_FIXTURE_CONTAINER=app`), whose `goBack` and `open(_:)` send them.
final class TabContentsIOSTests: XCTestCase {
    private var window: UIWindow?
    private var sessions: [ExactSession] = []
    private var animationsWereEnabled = true

    override func setUp() {
        super.setUp()
        animationsWereEnabled = UIView.areAnimationsEnabled
        UIView.setAnimationsEnabled(false)
        setenv("EXACT_FIXTURE_CONTAINER", "app", 1)
    }

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        unsetenv("EXACT_FIXTURE_CONTAINER")
        NativeViews.uninstallTable()
        UIView.setAnimationsEnabled(animationsWereEnabled)
        super.tearDown()
    }

    /// The container's Back goes back from the selected route as the
    /// platform's Back does, and the stack follows the router; at the stack's
    /// root there is nothing to go back to, and Exact says so when the
    /// queued Back runs.
    func testTheAppsContainerGoesBackAsThePlatformsBackDoes() throws {
        let session = try fixture("contents-back")
        let navigation = session.presenter.navigationHost
        let owner = try XCTUnwrap(navigation.tabOwner)
        let home = try XCTUnwrap(navigation.tabStack(0) as? UINavigationController)
        try pushDetail(session, home)
        goBack(owner)
        waitFor("the stack followed the router back") {
            home.viewControllers.count == 1 && !navigation.inTransition && !session.presenter.views.values.contains { $0.props["testId"] == "route-detail" }
        }
        XCTAssertEqual(journal(session, "tabs: back() refused"), 0)
        goBack(owner)
        waitFor("refused as it runs: nothing to go back to at the root") { self.journal(session, "tabs: back() refused") == 1 }
        XCTAssertEqual(home.viewControllers.count, 1)
    }

    /// The container's Back is the Back of the stack in front: under a sheet
    /// whose own stack shows its root, nothing is beneath it, so the Back is
    /// refused and the sheet stays.
    func testTheAppsContainersBackIsRefusedUnderASheet() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "a presentation completes only in a scene's window") }
        let session = try fixture("contents-back-sheet")
        let navigation = session.presenter.navigationHost
        let owner = try XCTUnwrap(navigation.tabOwner)
        try tapNode(session, "detail")
        waitFor("Detail is pushed") { navigation.navigation?.viewControllers.count == 2 && !navigation.inTransition }
        // Pressed as its handler is: the button is Home's, under Detail
        session.presenter.press(try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "open-note" }).id)
        waitFor("Note is presented") {
            !navigation.presentedNavigations.isEmpty && !navigation.inTransition && !session.presenter.modalHost.inTransition
        }
        let home = try XCTUnwrap(navigation.tabStack(0) as? UINavigationController)
        goBack(owner)
        waitFor("refused as it runs: nothing beneath the sheet's root") {
            self.journal(session, nothingBeneath) == 1
        }
        XCTAssertFalse(navigation.presentedNavigations.isEmpty, "the sheet stays")
        XCTAssertEqual(home.viewControllers.count, 2, "and Detail under it")
    }

    /// The container opens a URL as one that arrived from the system: the
    /// router goes there, and the container shows the tab it chose; empty
    /// text is not taken, and the container's refusal is journaled.
    func testTheAppsContainerOpensAURLAsOneThatArrived() throws {
        let session = try fixture("contents-open")
        let navigation = session.presenter.navigationHost
        let owner = try XCTUnwrap(navigation.tabOwner)
        let second = try XCTUnwrap(navigation.tabStack(1))
        let control = try segmentedControl(owner)
        owner.perform(Selector(("open:")), with: "/second")
        XCTAssertEqual(owner.value(forKey: "intentTaken") as? Bool, true, "queued")
        waitFor("the router chose the second tab, which the container shows") {
            navigation.primaryNavigation === second && !navigation.inTransition && control.selectedSegmentIndex == 1
        }
        XCTAssertEqual(journal(session, "tabs: open(/second) refused"), 0)
        owner.perform(Selector(("open:")), with: "")
        XCTAssertEqual(owner.value(forKey: "intentTaken") as? Bool, false, "empty text")
        XCTAssertEqual(journal(session, "tabs: open() refused"), 1)
    }

    /// A URL over 64 KB is refused at the asking and not queued. The
    /// refusals name it without its query and fragment, where tokens usually
    /// travel; a path of 64 KB as it is, and a longer one by the whole URL's
    /// length.
    func testTheAppsContainerRefusesAURLOver64KBAndJournalsItWithoutItsQuery() throws {
        let session = try fixture("contents-open-long")
        let owner = try XCTUnwrap(session.presenter.navigationHost.tabOwner)
        let url = "/second?token=" + String(repeating: "a", count: hatchTextBytes) + "#part"
        owner.perform(Selector(("open:")), with: url)
        XCTAssertEqual(owner.value(forKey: "intentTaken") as? Bool, false, "over 64 KB")
        XCTAssertEqual(session.presenter.elements.inFlight, 0, "not queued")
        XCTAssertEqual(journal(session, "hatch tabs: open(/second) refused: \(url.utf8.count) bytes is over 64 KB"), 1, "Exact says why")
        XCTAssertEqual(journal(session, "open(/second) refused"), 2, "and the container's call is refused, by the same name")
        XCTAssertEqual(journal(session, "token="), 0)
        XCTAssertEqual(journal(session, "#part"), 0)
        let path = "/" + String(repeating: "b", count: hatchTextBytes) + "?key=k"
        owner.perform(Selector(("open:")), with: path)
        XCTAssertEqual(owner.value(forKey: "intentTaken") as? Bool, false, "over 64 KB")
        XCTAssertEqual(journal(session, "hatch tabs: open(\(path.utf8.count) bytes) refused: \(path.utf8.count) bytes is over 64 KB"), 1, "Exact names it by its length")
        XCTAssertEqual(journal(session, "open(\(path.utf8.count) bytes) refused"), 2, "and so does the container")
        XCTAssertEqual(journal(session, "bbbbbbbbbbbbbbbb"), 0)
        XCTAssertEqual(journal(session, "key="), 0)
        // A path of exactly 64 KB is named as it is on both sides, which holds the module's copy of the bound to the host's
        let edge = "/" + String(repeating: "d", count: hatchTextBytes - 1)
        owner.perform(Selector(("open:")), with: edge + "?key=k")
        XCTAssertEqual(owner.value(forKey: "intentTaken") as? Bool, false, "over 64 KB")
        XCTAssertEqual(journal(session, "tabs: open(\(edge)) refused"), 2, "both refusals name the path")
    }

    /// A URL of 64 KB is taken. With the act queue full, a URL is refused,
    /// and the queue names it without its query and fragment, and by its `?`
    /// or `#` when it begins with one.
    func testTheAppsContainerTakesA64KBURLAndAFullQueueRefusesOneWithoutItsQuery() throws {
        let session = try fixture("contents-open-full")
        let owner = try XCTUnwrap(session.presenter.navigationHost.tabOwner)
        let elements = session.presenter.elements
        let bound = "/second?" + String(repeating: "c", count: hatchTextBytes - 8)
        XCTAssertEqual(bound.utf8.count, hatchTextBytes)
        owner.perform(Selector(("open:")), with: bound)
        XCTAssertEqual(owner.value(forKey: "intentTaken") as? Bool, true, "64 KB is within the bound")
        for _ in elements.inFlight..<ElementHatches.maxQueued { owner.perform(Selector(("open:")), with: "/second") }
        XCTAssertEqual(elements.inFlight, ElementHatches.maxQueued)
        owner.perform(Selector(("open:")), with: "/second?token=t#part")
        XCTAssertEqual(owner.value(forKey: "intentTaken") as? Bool, false, "the queue is full")
        XCTAssertEqual(journal(session, "hatch tabs: open(/second) refused: the act queue is full"), 1)
        owner.perform(Selector(("open:")), with: "?token=t#part")
        XCTAssertEqual(owner.value(forKey: "intentTaken") as? Bool, false, "the queue is full")
        XCTAssertEqual(journal(session, "hatch tabs: open(?) refused: the act queue is full"), 1, "named by its `?`, not as an empty URL")
        XCTAssertEqual(journal(session, "tabs: open(?) refused"), 2, "and so is the container's call")
        owner.perform(Selector(("open:")), with: "#part")
        XCTAssertEqual(journal(session, "tabs: open(#) refused"), 2, "one that begins with its fragment by its `#`")
        XCTAssertEqual(journal(session, "token="), 0)
        XCTAssertEqual(journal(session, "#part"), 0)
    }

    /// An intent sent from inside the container's `onSelect`, which Exact
    /// calls while it applies the router's choice, moves nothing inside the
    /// callback, and the router and the container end at the tab it opens.
    func testAnIntentSentFromInsideOnSelectLeavesTheRouterAndTheContainerAgreeing() throws {
        let session = try fixture("contents-reentry")
        let navigation = session.presenter.navigationHost
        let owner = try XCTUnwrap(navigation.tabOwner)
        let home = try XCTUnwrap(navigation.tabStack(0))
        let control = try segmentedControl(owner)
        owner.setValue("/", forKey: "openOnNextSelect")
        owner.perform(Selector(("open:")), with: "/second")
        waitFor("the router and the container at Home again") {
            navigation.routerTab == 0 && navigation.primaryNavigation === home && control.selectedSegmentIndex == 0
                && !navigation.inTransition && owner.value(forKey: "openOnNextSelect") == nil
        }
        XCTAssertEqual(owner.value(forKey: "selectedJustAfterOpen") as? Int, 1, "nothing moved inside onSelect")
        XCTAssertEqual(journal(session, "refused"), 0)
    }

    /// A Back the container sends from inside its `onSelect`, which Exact
    /// calls while it applies the router's choice of another tab, is the Back
    /// of the tab chosen: it runs once that choice is applied, so the tab
    /// left keeps its stack, and the chosen tab's root has nothing beneath.
    func testABackSentFromInsideOnSelectIsTheChosenTabsBack() throws {
        let session = try fixture("contents-back-reentry")
        let navigation = session.presenter.navigationHost
        let owner = try XCTUnwrap(navigation.tabOwner)
        let home = try XCTUnwrap(navigation.tabStack(0) as? UINavigationController)
        let second = try XCTUnwrap(navigation.tabStack(1))
        let control = try segmentedControl(owner)
        try pushDetail(session, home)
        owner.setValue(true, forKey: "backOnNextSelect")
        // The control's action, as a tap sends it: a bare test run has no application to send it through
        control.selectedSegmentIndex = 1
        owner.perform(Selector(("chose")))
        waitFor("refused as it runs: the second tab's root has nothing beneath") {
            self.journal(session, nothingBeneath) == 1
        }
        XCTAssertTrue(navigation.primaryNavigation === second, "the second tab in front")
        XCTAssertEqual(home.viewControllers.count, 2, "Home keeps Detail")
    }

    /// Intents queued before their container's term ended do not run in a
    /// later term, though a successor holds the tabs when they come to run:
    /// the containers after the sender did not send them.
    func testAnIntentQueuedBeforeItsContainerRetiredIsRefused() throws {
        let session = try fixture("contents-retired")
        let navigation = session.presenter.navigationHost
        let owner = try XCTUnwrap(navigation.tabOwner)
        let home = try XCTUnwrap(navigation.tabStack(0) as? UINavigationController)
        try pushDetail(session, home)
        goBack(owner)
        let control = try segmentedControl(owner)
        control.selectedSegmentIndex = 1
        owner.perform(Selector(("chose")))
        owner.perform(Selector(("open:")), with: "/second")
        let refused = "hatch tabs: an intent refused: the container that sent it retired"
        XCTAssertEqual(journal(session, refused), 0, "nothing ran within the turn")
        XCTAssertEqual(navigation.routerTab, 0, "the router is at Home still")
        XCTAssertEqual(home.viewControllers.count, 2, "with Detail")
        // The tabs' containers built again, as a change to the tabs builds them (`reshape`)
        navigation.retireTabs()
        navigation.sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        let successor = try XCTUnwrap(navigation.tabOwner, "a successor holds the tabs")
        XCTAssertFalse(successor === owner)
        waitFor("all refused as they run") { self.journal(session, refused) == 3 }
        XCTAssertTrue(navigation.tabOwner === successor)
        // Retiring the tabs empties the old stacks; the successor's Home stack holds the routes the router keeps
        XCTAssertEqual((navigation.tabStack(0) as? UINavigationController)?.viewControllers.count, 2, "Home keeps Detail")
    }

    /// An intent queued before its container left the tabs is refused, though
    /// the same container holds them again when it runs, as an app that keeps
    /// its container returns it from `tabContainer` once more.
    func testAnIntentQueuedBeforeItsContainerTookTheTabsAgainIsRefused() throws {
        let session = try fixture("contents-readopted")
        let navigation = session.presenter.navigationHost
        let owner = try XCTUnwrap(navigation.tabOwner)
        goBack(owner)
        // The container leaves the tabs and takes them again within the turn, as retiring and adopting it set them
        navigation.tabOwner = nil
        navigation.tabOwner = owner
        waitFor("refused as it runs") { self.journal(session, "hatch tabs: an intent refused: the container that sent it retired") == 1 }
        XCTAssertEqual(journal(session, nothingBeneath), 0, "the Back did not run at the root")
        XCTAssertTrue(navigation.tabOwner === owner)
    }

    private let nothingBeneath = "tabs: back() refused: nothing is beneath the top of the stack in front"

    /// Detail pushed on Home's stack, and the push settled.
    private func pushDetail(_ session: ExactSession, _ home: UINavigationController) throws {
        try tapNode(session, "detail")
        waitFor("Detail is pushed") { home.viewControllers.count == 2 && !session.presenter.navigationHost.inTransition }
    }

    /// The container's Back, and that Exact queued it.
    private func goBack(_ owner: UIViewController, file: StaticString = #filePath, line: UInt = #line) {
        owner.perform(Selector(("goBack")))
        XCTAssertEqual(owner.value(forKey: "intentTaken") as? Bool, true, "queued", file: file, line: line)
    }

    /// The fixture container's segmented control.
    private func segmentedControl(_ owner: UIViewController) throws -> UISegmentedControl {
        try XCTUnwrap(owner.view.subviews.compactMap { $0 as? UISegmentedControl }.first)
    }

    private func fixture(_ label: String) throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env["EXACT_FIXTURE_PLAN"], "build.mjs --test --ios compiles EXACT_FIXTURE_PLAN")))
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: frame)
        window.frame = frame
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: frame.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        session.nativeViews!.installArtifact(try XCTUnwrap(env["EXACT_FIXTURE_MODULE"], "build.mjs --test --ios builds the fixture's module"))
        waitFor("the app's container holds the tabs") {
            let navigation = session.presenter.navigationHost
            return navigation.tabController == nil && navigation.tabOwner?.viewIfLoaded?.window != nil
                && navigation.primaryNavigation?.viewIfLoaded?.window != nil && !Agent(session: session).nativeInFlight()
        }
        return session
    }

    /// How many lines of the session's journal hold `line`.
    private func journal(_ session: ExactSession, _ line: String) -> Int {
        session.agent(#"{"op":"logs","since":0}"#).components(separatedBy: line).count - 1
    }
}
#endif
