#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNativeViews
@testable import ExactNavigation

final class PackageScreenIOSTests: XCTestCase {
    private let appeared = "willAppear didAppear "
    private let disappeared = "willDisappear didDisappear "
    private var window: UIWindow?
    private var session: ExactSession!

    override func tearDown() {
        session?.destroy()
        session = nil
        NativeViews.uninstallTable()
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    private func boot(_ scene: UIWindowScene, plan: String = "PACKAGE_SCREEN", regular: Bool = false, embedded: Bool = false) throws {
        let window = UIWindow(windowScene: scene), host = UIViewController()
        if regular {
            window.frame = CGRect(x: 0, y: 0, width: 1100, height: 800)
            window.traitOverrides.horizontalSizeClass = .regular
        }
        if embedded {
            let stack = UINavigationController(rootViewController: UIViewController())
            stack.pushViewController(host, animated: false)
            window.rootViewController = stack
        } else { window.rootViewController = host }
        window.makeKeyAndVisible()
        self.window = window
        session = ExactApp.shared.makeSession(label: "package-screen")
        let view = ExactView(session: session)
        host.view.addSubview(view)
        view.frame = host.view.bounds
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        XCTAssertNil(session.boot(plan: try fixturePlan(plan), size: view.bounds.size).error)
        session.nativeViews!.installArtifact(try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_PACKAGE_SCREEN_MODULE"]))
        view.layoutIfNeeded()
        session.nativeViews!.activated()
        waitForActivation(session)
    }

    private func settle(_ what: String, _ ready: () -> Bool) {
        waitFor(what) { ready() && !Agent(session: self.session).nativeInFlight() }
    }

    private func screen(in route: UIViewController?) throws -> UITableViewController {
        let route = try XCTUnwrap(route)
        let screen = try XCTUnwrap(route.children.compactMap { $0 as? UITableViewController }.first)
        XCTAssertTrue(screen.parent === route)
        XCTAssertTrue(screen.view.isDescendant(of: route.view))
        let box = try XCTUnwrap(screen.view.superview as? NodeView)
        XCTAssertTrue(ViewPresentation.controller(for: box) === route)
        let expected = box.contentBox(), frame = screen.view.frame
        XCTAssertEqual(frame.minX, expected.minX, accuracy: 0.001)
        XCTAssertEqual(frame.minY, expected.minY, accuracy: 0.001)
        XCTAssertEqual(frame.width, expected.width, accuracy: 0.001)
        XCTAssertEqual(frame.height, expected.height, accuracy: 0.001)
        return screen
    }

    /// UIKit's animation can retire objects at the end of the current run-loop turn.
    private func assertReleased(_ screen: () -> UIViewController?, file: StaticString = #filePath, line: UInt = #line) {
        let loop = CFRunLoopGetMain()
        var turns = 0
        let counter = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeWaiting.rawValue, true, Int.max) { _, _ in
            turns += 1
            if turns == 2 { CFRunLoopStop(loop) } else { CFRunLoopWakeUp(loop) }
        }
        CFRunLoopAddObserver(loop, counter, .defaultMode)
        autoreleasepool { _ = CFRunLoopRunInMode(.defaultMode, 1, false) }
        CFRunLoopObserverInvalidate(counter)
        XCTAssertEqual(turns, 2, file: file, line: line)
        XCTAssertNil(screen(), file: file, line: line)
    }

    /// LLP 1123 D15: a native view's safe-area declaration crosses the module's table as it mounts, a
    /// package's through the package seam and an app's through its own, and one that declares none has none.
    func testANativeViewsSafeAreaDeclarationReachesTheHost() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "native views mount in a window") }
        try boot(scene)
        let natives = try XCTUnwrap(session.nativeViews)
        let home = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation?.topViewController)
        func mount(_ tag: String, _ id: UInt32) -> NodeView {
            let owner = NodeView(id: id, kind: "native", presenter: session.presenter)
            home.view.addSubview(owner)
            natives.create(owner: owner)
            owner.props = ["nativeViewModuleName": tag]
            natives.update(owner)
            natives.loadIfNeeded()
            waitForMainQueue()
            return owner
        }
        let plain = mount("probe-plain", 100010), clearing = mount("clearing-table", 100011), table = mount("app-table", 100012)
        defer { [plain, clearing, table].forEach { natives.destroy(id: $0.id) } }
        let declared = try XCTUnwrap(natives.safeArea(of: plain.id), "the package's declaration")
        XCTAssertFalse(declared.adjustsAdditionalSafeAreaInsets)
        let lent = try XCTUnwrap(declared.scrollViews.first)
        XCTAssertEqual(declared.scrollViews.count, 1)
        XCTAssertTrue(lent.view.superview === plain.subviews.first, "the package's own scroll view")
        XCTAssertEqual(lent.anchor, "list")
        XCTAssertEqual(lent.layout?(UIEdgeInsets(top: 0, left: 84, bottom: 0, right: 0)), UIEdgeInsets(top: 0, left: 20, bottom: 0, right: 0))
        XCTAssertEqual(declared.safeAreaRegions, 1)
        declared.release()
        XCTAssertEqual(lent.view.accessibilityHint, "released", "the package's release ran")
        XCTAssertEqual(natives.safeArea(of: clearing.id)?.adjustsAdditionalSafeAreaInsets, true, "the app's declaration")
        XCTAssertNil(natives.safeArea(of: table.id), "a view that declares nothing")
    }

    /// LLP 1123 D15: the host clears what is owed in a consenting native view through the view's own
    /// controller, beside the app's own additions, writes nothing for a view that does not consent, and
    /// gives its clearance back when the view goes, the app's additions kept.
    func testANativeViewsClearanceIsTaggedKeptApartAndGivenBack() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "native screens are contained in a window") }
        try boot(scene)
        let natives = try XCTUnwrap(session.nativeViews)
        let home = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation?.topViewController)
        func mount(_ tag: String, _ id: UInt32) throws -> UIViewController {
            let owner = NodeView(id: id, kind: "native", presenter: session.presenter)
            home.view.addSubview(owner)
            natives.create(owner: owner)
            owner.props = ["nativeViewModuleName": tag]
            natives.update(owner)
            natives.loadIfNeeded()
            waitForMainQueue()
            return try XCTUnwrap(natives.entries[id]?.screen, "\(tag)'s controller")
        }
        let clearing = try mount("clearing-table", 100020), plain = try mount("app-table", 100021)
        defer { natives.destroy(id: 100021) }
        clearing.additionalSafeAreaInsets = UIEdgeInsets(top: 0, left: 0, bottom: 10, right: 0)
        XCTAssertTrue(natives.clear(100020, owed: UIEdgeInsets(top: 30, left: 0, bottom: 0, right: 0)))
        XCTAssertEqual(clearing.additionalSafeAreaInsets, UIEdgeInsets(top: 30, left: 0, bottom: 10, right: 0), "beside the app's own")
        XCTAssertFalse(natives.clear(100021, owed: UIEdgeInsets(top: 30, left: 0, bottom: 0, right: 0)))
        XCTAssertEqual(plain.additionalSafeAreaInsets, .zero, "nothing for a view that does not consent")
        natives.destroy(id: 100020)
        XCTAssertEqual(clearing.additionalSafeAreaInsets, UIEdgeInsets(top: 0, left: 0, bottom: 10, right: 0), "given back, the app's kept")
    }

    func testPushPopTabsRotationAndSheet() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "push/pop, tabs, rotation, and a sheet") }
        try pushPopTabsRotationAndSheet(scene, appScreen: false)
    }

    func testAppPushPopTabsRotationAndSheet() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "push/pop, tabs, rotation, and a sheet") }
        try pushPopTabsRotationAndSheet(scene, appScreen: true)
    }

    private func pushPopTabsRotationAndSheet(_ scene: UIWindowScene, appScreen: Bool) throws {
        try boot(scene)
        let navigation = session.presenter.navigationHost
        let primary = try XCTUnwrap(navigation.primaryNavigation)
        let home = try XCTUnwrap(primary.topViewController)
        func push() throws -> UITableViewController {
            XCTAssertTrue(session.follow(appScreen ? "/app-detail" : "/detail"))
            settle("detail is visible") { primary.topViewController !== home && primary.topViewController?.viewIfLoaded?.window != nil }
            let table = try screen(in: primary.topViewController)
            settle("UIKit finished showing the table") { table.tableView.tableFooterView?.accessibilityHint == "visible" }
            return table
        }
        func pop(_ id: UInt32) {
            autoreleasepool {
                weak var surface = primary.topViewController?.viewIfLoaded
                XCTAssertNotNil(surface?.window)
                primary.popViewController(animated: true)
                waitFor("the popped route or its surface is gone") { session.presenter.views[id] == nil || surface?.window == nil }
            }
        }

        weak var retired: UITableViewController?, firstView: UITableView?
        let (firstID, first): (UInt32, UILabel) = try autoreleasepool {
            let table = try push()
            retired = table
            firstView = table.tableView
            return (try XCTUnwrap((table.parent as? RouteController)?.node.id), try XCTUnwrap(table.tableView.tableFooterView as? UILabel))
        }
        XCTAssertEqual(first.text, appeared)
        pop(firstID)
        assertReleased { retired }
        XCTAssertNil(session.presenter.views[firstID])
        XCTAssertNil(firstView?.superview)
        XCTAssertEqual(first.text, appeared + disappeared)
        XCTAssertEqual(first.accessibilityValue, "controller left first")

        weak var rotatedTable: UITableViewController?
        let (rotatedID, trace): (UInt32, UILabel) = try autoreleasepool {
            let table = try push()
            let detail = try XCTUnwrap(table.parent)
            let trace = try XCTUnwrap(table.tableView.tableFooterView as? UILabel)
            XCTAssertEqual(trace.text, appeared)
            let tabs = try XCTUnwrap(navigation.tabController)
            tapTab(tabs, 1)
            settle("the other tab is visible") { navigation.primaryNavigation !== primary && trace.accessibilityHint == "hidden" }
            XCTAssertTrue(try screen(in: detail) === table)
            XCTAssertEqual(trace.text, appeared + disappeared)
            let plain = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "plain" })
            XCTAssertNotNil(plain.subviews.first?.window)
            XCTAssertTrue(navigation.primaryNavigation?.topViewController?.children.isEmpty == true)

            tapTab(tabs, 0)
            settle("the detail is visible again") { detail.viewIfLoaded?.window != nil && trace.accessibilityHint == "visible" }
            XCTAssertTrue(try screen(in: primary.topViewController) === table)
            let initial = scene.interfaceOrientation
            let turn: UIInterfaceOrientation = initial.isPortrait ? .landscapeLeft : .portrait
            func rotate(_ to: UIInterfaceOrientation) {
                scene.requestGeometryUpdate(.iOS(interfaceOrientations: UIInterfaceOrientationMask(rawValue: 1 << to.rawValue))) { XCTFail("\($0)") }
                settle("scene and detail finished rotating") { scene.interfaceOrientation == to && detail.transitionCoordinator == nil }
            }
            let width = table.view.bounds.width
            rotate(turn)
            XCTAssertTrue(try screen(in: primary.topViewController) === table)
            XCTAssertNotEqual(table.view.bounds.width, width)
            rotate(initial)
            XCTAssertEqual(trace.text, appeared + disappeared + appeared)
            rotatedTable = table
            return (try XCTUnwrap((detail as? RouteController)?.node.id), trace)
        }
        pop(rotatedID)
        assertReleased { rotatedTable }
        XCTAssertNil(session.presenter.views[rotatedID])
        XCTAssertEqual(trace.text, appeared + disappeared + appeared + disappeared)
        XCTAssertEqual(trace.accessibilityValue, "controller left first")

        weak var sheetTable: UITableViewController?, sheetView: UITableView?, sheetSurface: UIView?
        let (sheetID, sheetTrace): (UInt32, UILabel) = try autoreleasepool {
            XCTAssertTrue(session.follow(appScreen ? "/app-sheet" : "/sheet"))
            settle("UIKit presented the sheet") { navigation.modalNavigation?.topViewController?.viewIfLoaded?.window != nil }
            let table = try screen(in: navigation.modalNavigation?.topViewController)
            let trace = try XCTUnwrap(table.tableView.tableFooterView as? UILabel)
            settle("the sheet finished appearing") { trace.accessibilityHint == "visible" }
            sheetSurface = try XCTUnwrap(window?.rootViewController?.presentedViewController?.viewIfLoaded)
            XCTAssertNotNil(sheetSurface?.window)
            sheetTable = table
            sheetView = table.tableView
            return (try XCTUnwrap((table.parent as? RouteController)?.node.id), trace)
        }
        XCTAssertEqual(sheetTrace.text, appeared)
        autoreleasepool {
            XCTAssertNil(Agent(session: session).back()["error"])
            waitFor("the dismissed sheet left its window") { sheetSurface?.window == nil }
        }
        assertReleased { sheetTable }
        XCTAssertNil(session.presenter.views[sheetID])
        XCTAssertNil(sheetView?.superview)
        XCTAssertEqual(sheetTrace.text, appeared + disappeared)
        XCTAssertEqual(sheetTrace.accessibilityValue, "controller left first")
    }

    func testAWaitingScreenFollowsItsAncestorIntoAnAlreadyLoadedHost() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a waiting screen moves within a window") }
        try boot(scene)
        let home = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation?.topViewController)
        let waiting = NodeView(id: 100000, kind: "column", presenter: session.presenter)
        waiting.props["navigationKey"] = "waiting"
        home.view.addSubview(waiting)
        let wrapper = NodeView(id: 100001, kind: "column", presenter: session.presenter)
        waiting.addSubview(wrapper)
        let owner = NodeView(id: 100002, kind: "native", presenter: session.presenter)
        wrapper.addSubview(owner)
        let natives = try XCTUnwrap(session.nativeViews)
        natives.create(owner: owner)
        defer { natives.destroy(id: owner.id) }
        owner.props = ["nativeViewModuleName": "app-table"]
        natives.update(owner)
        natives.loadIfNeeded()
        waitForMainQueue()
        XCTAssertTrue(owner.subviews.isEmpty)
        home.view.addSubview(wrapper)
        let table = try screen(in: home)
        XCTAssertTrue(table.view.superview === owner)
        let trace = try XCTUnwrap(table.tableView.tableFooterView as? UILabel)
        settle("the moved table appeared") { trace.accessibilityHint == "visible" }
        wrapper.movingRoute(into: waiting) { waiting.addSubview(wrapper) }
        XCTAssertEqual(trace.text, appeared + disappeared)
    }

    func testUnmountingWithTheDetailTabHiddenKeepsItsDisappearance() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the hidden tab survives a session unmount") }
        try boot(scene)
        let navigation = session.presenter.navigationHost
        XCTAssertTrue(session.follow("/detail"))
        settle("the detail is visible") { navigation.primaryNavigation?.viewControllers.count == 2 }
        let table = try screen(in: navigation.primaryNavigation?.topViewController)
        let trace = try XCTUnwrap(table.tableView.tableFooterView as? UILabel)
        settle("the table appeared") { trace.accessibilityHint == "visible" }
        tapTab(try XCTUnwrap(navigation.tabController), 1)
        settle("the detail is hidden") { trace.accessibilityHint == "hidden" }
        XCTAssertEqual(trace.text, appeared + disappeared)
        session.view?.removeFromSuperview()
        waitForMainQueue()
        XCTAssertEqual(trace.text, appeared + disappeared)
    }

    func testScreenWaitsUnderAVisibleControllerUntilItsRouteLoads() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the waiting route is already in a window") }
        try boot(scene)
        let home = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation?.topViewController)
        let route = NodeView(id: 100_000, kind: "column", presenter: session.presenter)
        route.props["navigationKey"] = "waiting"
        home.view.addSubview(route)
        let owner = NodeView(id: 100_001, kind: "native", presenter: session.presenter)
        route.addSubview(owner)
        let natives = try XCTUnwrap(session.nativeViews)
        let before = natives.made, children = home.children
        natives.create(owner: owner)
        owner.props = ["nativeViewModuleName": "reusable-table"]
        natives.update(owner)
        natives.loadIfNeeded()
        XCTAssertEqual(natives.made, before + 1)
        XCTAssertEqual(home.children, children)
        XCTAssertTrue(owner.subviews.isEmpty)
        owner.frame = CGRect(x: 0, y: 0, width: 220, height: 140)
        defer { natives.destroy(id: owner.id) }
        let controller = RouteController(route)
        controller.loadViewIfNeeded()
        let table = try screen(in: controller)
        XCTAssertNil((table.tableView.tableFooterView as? UILabel)?.text)
        let holder = PlainTab()
        holder.bootContent = true
        holder.show(controller)
        XCTAssertTrue(owner.isDescendant(of: holder.view))
        holder.releaseBootContent()
        XCTAssertTrue(try screen(in: controller) === table)

        // Place the live screen under Home, then into a second route whose
        // responder chain still reaches Home until its own controller loads.
        owner.movingRoute(into: home.view) { home.view.addSubview(owner) }
        XCTAssertTrue(try screen(in: home) === table)
        let waiting = NodeView(id: 100_002, kind: "column", presenter: session.presenter)
        waiting.props["navigationKey"] = "second"
        home.view.addSubview(waiting)
        session.presenter.views[owner.id] = owner
        defer { session.presenter.views.removeValue(forKey: owner.id) }
        let trace = try XCTUnwrap(table.tableView.tableFooterView as? UILabel)
        settle("Home shows the table") { trace.accessibilityHint == "visible" }
        XCTAssertEqual(trace.text, appeared)
        home.view.setNeedsLayout()
        session.presenter.placeChildren(waiting, [owner.id])
        XCTAssertNil(table.parent)
        XCTAssertNil(table.view.superview)
        waitFor("Home finished hiding the table") { trace.accessibilityHint == "hidden" }
        XCTAssertEqual(trace.text, appeared + disappeared)
        let second = RouteController(waiting)
        second.loadViewIfNeeded()
        XCTAssertTrue(try screen(in: second) === table)
        table.view.removeFromSuperview()
        natives.moved(owner)
        XCTAssertTrue(table.view.superview === owner)

        owner.movingRoute(into: home.view) { home.view.addSubview(owner) }
        waitFor("Home showed the returning table") { trace.accessibilityHint == "visible" }
        home.view.setNeedsLayout()
        let parked = natives.parks
        natives.destroy(id: owner.id)
        XCTAssertEqual(natives.parks, parked + 1)
        XCTAssertNil(table.parent)
        XCTAssertNil(table.view.superview)
        waitFor("the removed screen disappeared") { trace.accessibilityHint == "hidden" }
        XCTAssertEqual(trace.text, (appeared + disappeared) + (appeared + disappeared))
        XCTAssertEqual(trace.accessibilityValue, "controller left first")
    }

    func testRemovingNavigationKeepsTheScreenInTheOrdinaryViewHierarchy() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the session stays on screen after its navigation ends") }
        try boot(scene)
        let navigation = session.presenter.navigationHost
        XCTAssertTrue(session.follow("/detail"))
        settle("the detail is visible") { navigation.primaryNavigation?.viewControllers.count == 2 }
        let table = try screen(in: navigation.primaryNavigation?.topViewController)
        let trace = try XCTUnwrap(table.tableView.tableFooterView as? UILabel)
        settle("the detail appeared") { trace.accessibilityHint == "visible" }
        let root = try XCTUnwrap(navigation.container)
        root.props.removeValue(forKey: "navigationBack")
        navigation.sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        XCTAssertNil(navigation.primaryOwner)
        let host = try XCTUnwrap(window?.rootViewController)
        XCTAssertTrue(try screen(in: host) === table)
        waitFor("the host finished showing the table") { trace.text?.hasSuffix(appeared) == true }
        XCTAssertEqual(trace.text, appeared + disappeared + appeared)
        let view = try XCTUnwrap(session.view)
        view.removeFromSuperview()
        XCTAssertNil(table.parent)
        XCTAssertEqual(trace.text, appeared + disappeared + appeared + disappeared)
        let container = UIViewController()
        window?.rootViewController = container
        let left = UIViewController(), right = UIViewController()
        for child in [left, right] {
            container.addChild(child)
            container.view.addSubview(child.view)
            child.didMove(toParent: container)
        }
        left.view.addSubview(view)
        settle("the left host contains the screen") { table.parent === left && trace.accessibilityHint == "visible" }
        XCTAssertTrue(try screen(in: left) === table)
        right.view.addSubview(view)
        XCTAssertTrue(try screen(in: right) === table)
        waitFor("the right host finished showing the table") { trace.accessibilityHint == "visible" }
        XCTAssertEqual(trace.text, String(repeating: appeared + disappeared, count: 3) + appeared)

        // A controller's root view keeps that controller as its next responder across a move.
        view.removeFromSuperview()
        let owner = UIViewController()
        owner.view = view
        container.addChild(owner)
        let first = UIView(), second = UIView()
        container.view.addSubview(first)
        container.view.addSubview(second)
        first.addSubview(view)
        owner.didMove(toParent: container)
        settle("the controller shows its own root view") { table.parent === owner && trace.accessibilityHint == "visible" }
        let before = trace.text
        second.addSubview(view)
        waitForMainQueue()
        XCTAssertTrue(try screen(in: owner) === table)
        XCTAssertEqual(trace.text, before)
    }

    func testPoppingTheEmbeddingControllerFinishesTheNativeAppearance() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the enclosing navigation controller pops the session") }
        try boot(scene, embedded: true)
        XCTAssertTrue(session.follow("/detail"))
        let navigation = session.presenter.navigationHost
        settle("the detail is visible") { navigation.primaryNavigation?.viewControllers.count == 2 }
        let table = try screen(in: navigation.primaryNavigation?.topViewController)
        let trace = try XCTUnwrap(table.tableView.tableFooterView as? UILabel)
        settle("the table appeared") { trace.accessibilityHint == "visible" }
        let stack = try XCTUnwrap(window?.rootViewController as? UINavigationController)
        stack.popViewController(animated: true)
        waitFor("the embedding controller left") { stack.transitionCoordinator == nil && table.view.window == nil }
        waitFor("UIKit finished the enclosing pop") { trace.accessibilityHint == "hidden" }
        XCTAssertEqual(trace.text, appeared + disappeared)
    }

    func testUnmountingWithASheetOpenFinishesItsNativeAppearance() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit presents a sheet over the embedded session") }
        try boot(scene)
        let navigation = session.presenter.navigationHost
        XCTAssertTrue(session.follow("/detail"))
        settle("the pushed route is visible") { navigation.primaryNavigation?.viewControllers.count == 2 }
        let table = try screen(in: navigation.primaryNavigation?.topViewController)
        let detailTrace = try XCTUnwrap(table.tableView.tableFooterView as? UILabel)
        settle("the pushed table appeared") { detailTrace.accessibilityHint == "visible" }
        XCTAssertTrue(session.follow("/sheet"))
        settle("the sheet is visible") { navigation.modalNavigation?.viewIfLoaded?.window != nil }
        let sheet = try screen(in: navigation.modalNavigation?.topViewController)
        let trace = try XCTUnwrap(sheet.tableView.tableFooterView as? UILabel)
        settle("the sheet table appeared") { trace.accessibilityHint == "visible" }
        let surface = try XCTUnwrap(window?.rootViewController?.presentedViewController?.viewIfLoaded)
        let box = try XCTUnwrap(sheet.view.superview)
        let liveSubviews = surface.subviews
        let probe = WindowExitProbe()
        sheet.view.addSubview(probe)
        probe.onExit = {
            if surface.window != nil && box.window != nil && box.isDescendant(of: surface) {
                // Route dismissal freezes the card before restoring its logical children.
                let cover = surface.subviews.last
                XCTAssertTrue(cover.map { !liveSubviews.contains($0) && !$0.isHidden && $0.frame == surface.bounds } == true,
                              "the outgoing card needs its snapshot until UIKit finishes dismissal")
            }
        }
        defer { probe.onExit = nil }
        session.view?.removeFromSuperview()
        waitFor("the presented surface left the window") { surface.window == nil }
        waitFor("the sheet and underlying detail disappeared") { trace.accessibilityHint == "hidden" && detailTrace.accessibilityHint == "hidden" }
        XCTAssertEqual(trace.text, appeared + disappeared)
        XCTAssertEqual(detailTrace.text, appeared + disappeared)
    }

    func testTheDialogControlsWhenItsScreenLeavesTheWindow() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a dialog presents above an ordinary embedded view") }
        try boot(scene)
        let navigation = session.presenter.navigationHost
        XCTAssertTrue(session.follow("/dialog"))
        settle("the dialog route is visible") { navigation.primaryNavigation?.viewControllers.count == 2 }
        let root = try XCTUnwrap(navigation.container)
        root.props.removeValue(forKey: "navigationBack")
        navigation.sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
        let button = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "open-dialog" })
        session.presenter.press(button.id)
        settle("the dialog is presented") { self.window?.rootViewController?.presentedViewController?.viewIfLoaded?.window != nil }
        XCTAssertNil(navigation.primaryOwner)
        let dialog = try XCTUnwrap(session.presenter.views.values.first { $0.props["id"] == "dialog" })
        let box = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "dialog-table" })
        let sheet = try XCTUnwrap(ViewPresentation.controller(for: box))
        let table = try XCTUnwrap(sheet.children.compactMap { $0 as? UITableViewController }.first { $0.view.superview === box })
        session.presenter.menus.action("showPopover", ["choices"])
        let popupBox = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "popover-table" })
        let layer = try XCTUnwrap(PopoverLayer.containing(popupBox))
        XCTAssertTrue(layer.sheet === sheet)
        let popupHost = try XCTUnwrap(ViewPresentation.controller(for: popupBox))
        let popup = try XCTUnwrap(popupHost.children.compactMap { $0 as? UITableViewController }.first { $0.view.superview === popupBox })
        XCTAssertNotNil(popup.view.window)
        let popover = try XCTUnwrap(session.presenter.views.values.first { $0.props["id"] == "choices" })
        let popupProbe = WindowExitProbe()
        popup.view.addSubview(popupProbe)
        popupProbe.onExit = {
            if layer.sheet === sheet && !popover.isHidden && popupBox.window != nil && popupBox.isDescendant(of: layer) && self.session.presenter.menus.isOpen(popover) {
                XCTFail("the table left its lifted popover while it was still open")
            }
        }
        defer { popupProbe.onExit = nil }
        XCTAssertFalse(dialog.isHidden)
        XCTAssertNotNil(table.view.window)
        let probe = WindowExitProbe()
        table.view.addSubview(probe)
        probe.onExit = {
            if !dialog.isHidden && box.isDescendant(of: sheet.view) && box.window != nil {
                XCTFail("the table left while its dialog was still visible in the sheet")
            }
        }
        defer { probe.onExit = nil }
        session.view?.removeFromSuperview()
        waitFor("the dialog sheet left its window") { sheet.view.window == nil }
        XCTAssertTrue(dialog.isHidden)
    }

    private final class WindowExitProbe: UIView {
        var onExit: (() -> Void)?
        override func didMoveToWindow() {
            super.didMoveToWindow()
            if window == nil { onExit?() }
        }
    }

    func testSplitDetailKeepsItsPackageScreenWhenTheSessionRemounts() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the split detail remounts in a window") }
        try splitDetailRemount(scene, appScreen: false)
    }

    func testSplitDetailKeepsItsAppScreenWhenTheSessionRemounts() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the split detail remounts in a window") }
        try splitDetailRemount(scene, appScreen: true)
    }

    private func splitDetailRemount(_ scene: UIWindowScene, appScreen: Bool) throws {
        try boot(scene, plan: "PACKAGE_SCREEN_SPLIT", regular: true)
        let navigation = session.presenter.navigationHost
        settle("the list is beside its detail") { navigation.split?.controller.isCollapsed == false }
        XCTAssertTrue(session.follow(appScreen ? "/app-detail" : "/detail"))
        settle("the detail is visible") { navigation.split?.detailRoutes.last?.viewIfLoaded?.window != nil }
        let detail = try XCTUnwrap(navigation.split?.detailRoutes.last)
        let table = try screen(in: detail)
        let trace = try XCTUnwrap(table.tableView.tableFooterView as? UILabel)
        settle("UIKit finished showing the split detail") { trace.accessibilityHint == "visible" }
        XCTAssertEqual(trace.text, appeared)
        let view = try XCTUnwrap(session.view), host = try XCTUnwrap(view.superview)
        view.removeFromSuperview()
        XCTAssertNil(table.parent)
        host.addSubview(view)
        settle("the replacement route holds the table") { navigation.split?.detailRoutes.last?.children.contains(table) == true }
        let next = try XCTUnwrap(navigation.split?.detailRoutes.last)
        XCTAssertFalse(next === detail)
        XCTAssertTrue(try screen(in: next) === table)
        settle("the remounted detail finished appearing") { trace.accessibilityHint == "visible" }
        XCTAssertEqual(trace.text, appeared + disappeared + appeared)
    }
}
#endif
