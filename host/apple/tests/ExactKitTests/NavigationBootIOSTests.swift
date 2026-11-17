#if os(iOS)
import UIKit
import XCTest
import ObjectiveC
@testable import ExactKit

final class NavigationBootIOSTests: XCTestCase {
    private var session: ExactSession?
    private var window: UIWindow?
    private var update: AnyObject?
    private var view: ExactView?

    override func setUpWithError() throws {
        try super.setUpWithError()
        guard #available(iOS 27.0, *) else { throw XCTSkip("deferred ownership requires iOS 27") }
    }

    override func tearDown() {
        if #available(iOS 18.0, *) { (update as? UIUpdateLink)?.isEnabled = false }
        update = nil
        session?.destroy()
        session = nil
        window?.isHidden = true
        window = nil
        view = nil
        super.tearDown()
    }

    private func fixture(_ key: String, mounted: Bool = true, rotatable: Bool = false, ownsWindow: Bool = true, batchEdit: ((inout Batch) -> Void)? = nil, beforeBoot: (ExactView) -> Void = { _ in }) throws -> ExactSession {
        let scene = try XCTUnwrap(hostedWindowScene())
        waitFor("the test scene is active") { scene.activationState == .foregroundActive }
        let session = ExactApp.shared.makeSession(label: "boot-owner-\(UUID().uuidString)")
        self.session = session
        session.hatchesOwnWindow = ownsWindow
        let view = ExactView(session: session), host = BootWindowController()
        host.orientations = rotatable ? .all : .portrait
        self.view = view
        let window = UIWindow(windowScene: scene)
        window.frame = window.screen.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        self.window = window
        if mounted { host.view.addSubview(view) }
        view.frame = host.view.bounds
        beforeBoot(view)
        if batchEdit != nil {
            let candidate = try XCTUnwrap(session.prepare(try fixturePlan(key), resolver: ExactApp.shared.resolver, size: window.bounds.size))
            var batch = try XCTUnwrap(session.commit(candidate))
            batchEdit?(&batch)
            session.presentCommitted(batch, label: "boot-fixture")
        } else { XCTAssertNil(session.boot(plan: try fixturePlan(key), size: window.bounds.size).error) }
        view.layoutIfNeeded()
        return session
    }

    private func geometry(_ session: ExactSession) -> [UInt32: [CGFloat]] {
        let view = session.view!
        func inset(_ e: UIEdgeInsets) -> [CGFloat] { [e.top, e.left, e.bottom, e.right] }
        return Dictionary(uniqueKeysWithValues: session.presenter.views.values.filter { $0.window != nil }.map { node in
            let f = node.convert(node.bounds, to: view)
            var values = [f.minX, f.minY, f.width, f.height] + inset(node.safeAreaInsets)
            if let scroll = node.scrollView {
                let frame = scroll.convert(scroll.bounds, to: view)
                values += [frame.minX, frame.minY, frame.width, frame.height] + inset(scroll.safeAreaInsets) + inset(node.pageBand)
                values += inset(scroll.contentInset) + inset(scroll.adjustedContentInset)
                values += [scroll.contentOffset.x, scroll.contentOffset.y, scroll.contentSize.width, scroll.contentSize.height]
            }
            return (node.id, values)
        })
    }

    private func accessibilityOrder(_ object: NSObject) -> [String] {
        if let view = object as? UIView, view.isHidden || view.alpha == 0 || view.accessibilityElementsHidden { return [] }
        if object.isAccessibilityElement { return [object.accessibilityLabel ?? (object as? UIView)?.accessibilityIdentifier ?? "unnamed"] }
        if let elements = object.accessibilityElements, !elements.isEmpty {
            return elements.compactMap { $0 as? NSObject }.flatMap(accessibilityOrder)
        }
        let count = object.accessibilityElementCount()
        if count != NSNotFound, count > 0 {
            return (0..<count).compactMap { object.accessibilityElement(at: $0) as? NSObject }.flatMap(accessibilityOrder)
        }
        return (object as? UIView)?.subviews.flatMap(accessibilityOrder) ?? []
    }

    func testHiddenBarsCommitBeforeTheOwnerAndKeepGeometryBeforeTheNextEvent() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the navigation owner is installed in a window scene") }
        var before: [UInt32: [CGFloat]]?
        var committed = false, event = false, transactionOpen = true
        CATransaction.begin()
        defer { if transactionOpen { CATransaction.commit() } }
        guard #available(iOS 18.0, *) else { throw XCTSkip("post-commit update phases require iOS 18") }
        let session = try fixture("BARLESS_ROUTE", beforeBoot: { [unowned self] view in
            let link = UIUpdateLink(view: view)
            self.update = link
            link.addAction(to: .beforeCATransactionCommit) { [unowned self] _, _ in
                guard !committed, let session = self.session, session.booted else { return }
                XCTAssertNil(session.firstDrawMs, "this must be the launch's first commit, not a later UI update")
                XCTAssertNil(session.presenter.navigation.primaryOwner)
                XCTAssertNotNil(session.presenter.navigation.bootOwner)
                before = self.geometry(session)
                committed = true
                transactionOpen = false
                CATransaction.commit()
            }
            link.addAction(to: .afterUpdateComplete) { [unowned self] _, _ in
                guard committed, !event, let session = self.session else { return }
                XCTAssertNotNil(session.firstDrawMs, "the commit drew the content")
                session.afterBootOwner {
                    XCTAssertNotNil(session.presenter.navigation.primaryOwner)
                    XCTAssertNil(session.presenter.navigation.bootOwner)
                    XCTAssertEqual(before, self.geometry(session))
                    event = true
                }
            }
            link.requiresContinuousUpdates = true
            link.isEnabled = true
        })
        XCTAssertNil(session.presenter.navigation.primaryOwner, "no navigation controller in the boot apply")
        waitFor("the first commit and following event") { event }
    }

    func testRunLoopFlushReturnsBeforeHandoffInTheSameTurn() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the first commit needs a visible scene") }
        CATransaction.begin()
        var open = true
        defer { if open { CATransaction.commit() } }
        let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
        var events: [String] = [], turn = 0, commitTurn = -1, handedOff = false
        let turns = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeTimers.rawValue, true, 0) { _, _ in turn += 1 }
        CFRunLoopAddObserver(CFRunLoopGetMain(), turns, .commonModes)
        defer { CFRunLoopObserverInvalidate(turns) }
        let geometry = geometry(session)
        DispatchQueue.main.async {
            events.append("main")
            // This block was queued before the commit. It may see either owner.
            XCTAssertEqual(geometry, self.geometry(session))
            session.afterBootOwner { XCTAssertNotNil(nav.primaryOwner); events.append("held-main") }
        }
        CFRunLoopPerformBlock(CFRunLoopGetMain(), CFRunLoopMode.commonModes.rawValue) {
            commitTurn = turn
            events.append("flush")
            open = false
            CATransaction.commit()
            CATransaction.flush()
            events.append("returned")
            XCTAssertNotNil(session.firstDrawMs)
            XCTAssertNotNil(nav.bootOwner, "ownership must not delay the content flush")
            XCTAssertNil(nav.primaryOwner)
            CFRunLoopPerformBlock(CFRunLoopGetMain(), CFRunLoopMode.commonModes.rawValue) {
                XCTAssertEqual(turn, commitTurn, "handoff precedes the next beforeTimers")
                XCTAssertNil(nav.bootOwner)
                XCTAssertNotNil(nav.primaryOwner)
                XCTAssertEqual(geometry, self.geometry(session))
                events.append("handoff-checked")
                handedOff = true
            }
        }
        CFRunLoopPerformBlock(CFRunLoopGetMain(), CFRunLoopMode.commonModes.rawValue) {
            events.append("remaining")
            XCTAssertNotNil(nav.bootOwner, "a block left in this drain can observe the holder")
            XCTAssertEqual(geometry, self.geometry(session))
        }
        waitFor("the flush, handoff and held consumer") { handedOff && events.contains("held-main") }
        XCTAssertLessThan(events.firstIndex(of: "returned")!, events.firstIndex(of: "remaining")!)
        XCTAssertLessThan(events.firstIndex(of: "remaining")!, events.firstIndex(of: "handoff-checked")!)
        waitFor("activation after ownership") { session.dataGeneration == session.generation }
    }

    func testQueuedBootWorkKeepsTheHolderGeometryAndHoldsCodeLoading() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "queued boot work needs a mounted session") }
        CATransaction.begin()
        var open = true
        defer { if open { CATransaction.commit() } }
        let session = try fixture("BARLESS_ROUTE"), p = session.presenter, nav = p.navigation
        let before = geometry(session), holder = try XCTUnwrap(nav.bootOwner)
        let launch = ExactLaunch()
        addTeardownBlock { launch.link?.invalidate() }
        launch.delegateInit()
        launch.didFinishLaunching()
        launch.bootEntered(session)
        launch.treeApplied(session, path: "boot-queues")
        // Exercise the production evaluate closure while the transaction stays open.
        launch.workEnded(session)
        var serviceSawOwner = false
        launch.requestService(module: "absent-boot-test-service", when: .afterFirstPixel, config: [:]) {
            XCTAssertNotNil(nav.primaryOwner)
            serviceSawOwner = true
            return Data()
        }
        p.requestProjectionSync()
        // didMoveToWindow's fit/mounted/accessibility block and the first apply's
        // canvas/frame block are already queued by fixture, not reproduced here.
        waitForMainQueue("mount, projection, launch evaluation and the first apply's queued work")
        XCTAssertTrue(nav.bootOwner === holder)
        XCTAssertNil(nav.primaryOwner)
        XCTAssertNil(session.firstDrawMs)
        XCTAssertEqual(before, geometry(session))
        XCTAssertTrue(launch.trace.isEmpty, "the queued launch evaluation is held")
        XCTAssertGreaterThanOrEqual(nav.bootOwnerWork.count, 2, "canvas startup and launch evaluation are held")
        var consumed = false
        session.afterBootOwner {
            XCTAssertNotNil(nav.primaryOwner)
            consumed = true
        }
        open = false
        CATransaction.commit()
        CATransaction.flush()
        XCTAssertTrue(nav.bootOwner === holder)
        launch.drawn(session)
        launch.frame(Vsync(at: CACurrentMediaTime(), interval: 1.0 / 60))
        waitFor("held work after the handoff", facts: { "consumer=\(consumed) trace=\(launch.trace) service=\(serviceSawOwner)" }) { consumed && !launch.trace.isEmpty && serviceSawOwner }
        XCTAssertEqual(before, geometry(session))
        XCTAssertTrue(nav.bootOwnerWork.isEmpty)
        session.frames.motion = false
        session.frames.run(false)
    }

    func testTextWorkersAndOwnerCallbacksAreSafeBeforeHandoff() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "boot publications need the visible geometry") }
        CATransaction.begin()
        var open = true
        defer { if open { CATransaction.commit() } }
        let session = try fixture("BARLESS_ROUTE"), p = session.presenter, nav = p.navigation
        let before = geometry(session), route = try XCTUnwrap(nav.activeRoute)
        let accessible = accessibilityOrder(try XCTUnwrap(session.view))
        XCTAssertFalse(accessible.isEmpty)
        let node = try XCTUnwrap(p.views.values.first { $0.canRasterText })
        node.dropTextRaster()
        // A fresh rasterizer has no cached image: this runs the actual worker
        // and its main-queue publication against the temporary container.
        let rasters = TextRasterizer()
        XCTAssertTrue(rasters.ensure(node, urgent: false))
        XCTAssertEqual(rasters.inFlight, 1)
        var called = false
        Owner.shared.post {
            Owner.shared.callMain {
                XCTAssertNotNil(nav.bootOwner)
                XCTAssertNil(nav.primaryOwner)
                XCTAssertEqual(before, self.geometry(session))
                session.view?.setNeedsLayout()
                session.view?.layoutIfNeeded() // fit must queue while serving the owner
                nav.deliverCovers()
                p.owners.publish()
                called = true
            }
        }
        waitFor("the text worker's publication and Owner.callMain") { rasters.inFlight == 0 && called }
        waitForMainQueue("the deferred fit, cover report and owner publication")
        XCTAssertTrue(node.textRasterReady)
        XCTAssertEqual(before, geometry(session))
        // Deferred cover and ownership reports use the holder's exact geometry.
        nav.deliverCovers()
        p.owners.publish()
        p.syncAccessibility()
        XCTAssertTrue(nav.activeRoute === route)
        XCTAssertNotNil(nav.bootOwner)
        XCTAssertEqual(before, geometry(session))
        open = false
        CATransaction.commit()
        CATransaction.flush()
        waitFor("the content's real owner") { nav.primaryOwner != nil }
        XCTAssertTrue(nav.activeRoute === route)
        XCTAssertEqual(before, geometry(session))
        XCTAssertTrue(node.textRasterReady)
        XCTAssertEqual(accessible, accessibilityOrder(try XCTUnwrap(session.view)))
    }

    func testImageWorkersPublishWithTheTemporaryOwner() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "image publication needs the mounted launch views") }
        CATransaction.begin()
        var open = true
        defer { if open { CATransaction.commit() } }
        let session = try fixture("BARLESS_ROUTE"), p = session.presenter, nav = p.navigation
        let source = "data:image/png;base64," + UIGraphicsImageRenderer(size: CGSize(width: 2, height: 2)).image { context in
            UIColor.red.setFill(); context.fill(CGRect(x: 0, y: 0, width: 2, height: 2))
        }.pngData()!.base64EncodedString()
        p.apply(wireBatch([["op": "create", "id": 90004, "kind": "image", "props": ["imageSource": source]],
                          ["op": "frame", "id": 90004, "x": 0.0, "y": 0.0, "w": 16.0, "h": 16.0]]))
        let image = try XCTUnwrap(p.views[90004]), holder = try XCTUnwrap(nav.bootOwner)
        nav.activeRoute?.node.container.addSubview(image)
        let before = geometry(session)
        waitFor("the image worker's main-queue publication") { image.raster != nil }
        XCTAssertTrue(nav.bootOwner === holder)
        XCTAssertNil(session.firstDrawMs)
        XCTAssertEqual(before, geometry(session))
        open = false
        CATransaction.commit()
        CATransaction.flush()
        waitFor("image ownership after the first commit") { nav.primaryOwner != nil }
        XCTAssertNotNil(image.raster)
        XCTAssertEqual(before, geometry(session))
    }

    private var empty: Batch { wireBatch([]) }

    func testRemovingEveryRouteOrFailingProjectionRetiresTheHolder() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        for remove in [true, false] {
            let session = try fixture("BARLESS_ROUTE"), p = session.presenter, nav = p.navigation
            XCTAssertNotNil(nav.bootOwner)
            let root = try XCTUnwrap(nav.container), route = try XCTUnwrap(nav.bootOwner?.route)
            if remove { p.apply(wireBatch([["op": "children", "id": root.id, "ids": []]])) }
            else {
                p.apply(wireBatch([["op": "props", "id": route.node.id, "set": ["navigationKey": "replacement"]],
                                   ["op": "props", "id": root.id, "set": ["navigationKey": "missing"]]]))
            }
            XCTAssertNil(nav.bootOwner)
            XCTAssertNil(nav.primaryOwner)
            XCTAssertTrue(nav.controllers.isEmpty)
            session.destroy()
        }
    }

    /// Tabs whose Contract hides the bars before the first layout.
    private func deferredTabs() throws -> ExactSession {
        let session = try fixture("BOOT_TABS")
        XCTAssertNotNil(session.presenter.navigation.bootOwner, "tabs: \(session.presenter.views.values.filter { $0.props["accessibilityRole"] == "tablist" }.map { "\($0.style)" }) routes: \(session.presenter.navigation.controllers.values.map { $0.node.props })")
        return session
    }

    func testDroppingTabsBeforeCommitKeepsTheRealStackAndReshapeStable() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        let session = try deferredTabs(), p = session.presenter, nav = p.navigation
        let root = try XCTUnwrap(nav.container), route = try XCTUnwrap(nav.bootOwner?.route)
        p.apply(wireBatch([["op": "children", "id": root.id, "ids": [route.node.id]]]))
        // A structural change must complete ownership synchronously.
        XCTAssertNil(nav.bootOwner)
        let owner = try XCTUnwrap(nav.primaryNavigation)
        XCTAssertTrue(nav.tabPanels.isEmpty)
        XCTAssertNil(nav.adoptedTablist)
        XCTAssertFalse(nav.tablistAppearanceNone)
        XCTAssertTrue(nav.primaryStacks.first === owner)
        nav.sync(empty)
        XCTAssertTrue(nav.primaryNavigation === owner, "reshape must not rebuild the same stack")
        XCTAssertTrue(nav.activeRoute === route)
        XCTAssertEqual(nav.primaryRoutes.map(\.node.id), [route.node.id])
        XCTAssertFalse(nav.gestureRecognizerShouldBegin(try XCTUnwrap(owner.interactivePopGestureRecognizer)), "one route cannot pop")
    }

    func testChangingTabsBeforeCommitThenPopUsesTheNewStacks() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        let session = try deferredTabs(), p = session.presenter, nav = p.navigation
        let root = try XCTUnwrap(nav.container), tabs = try XCTUnwrap(NavigationTabs.of(root, p))
        let reversed = Array(tabs.tabs.reversed())
        p.apply(wireBatch([["op": "children", "id": tabs.tablist.id, "ids": reversed.map(\.id)]]))
        XCTAssertNil(nav.bootOwner)
        XCTAssertEqual(nav.tabPanels, tabs.panels.reversed().map(\.id))
        let owner = try XCTUnwrap(nav.primaryOwner), stack = try XCTUnwrap(nav.primaryNavigation)
        nav.sync(empty)
        XCTAssertTrue(nav.primaryOwner === owner)
        XCTAssertTrue(nav.primaryNavigation === stack)
        // The runner's push and Back exercise ownership after the hand-off.
        let open = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "open" })
        _ = Agent(session: session).tap(["id": Int(open.id)])
        waitFor("the push settles") { !nav.inTransition }
        XCTAssertEqual(nav.primaryNavigation?.viewControllers.count, 2)
        _ = Agent(session: session).back()
        waitFor("the pop settles") { !nav.inTransition }
        XCTAssertEqual(nav.primaryNavigation?.viewControllers.count, 1)
    }

    func testFirstDrawOutsideAUIUpdateHandsOffBeforeActivation() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        CATransaction.begin()
        var transactionOpen = true
        defer { if transactionOpen { CATransaction.commit() } }
        let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
        XCTAssertNil(session.firstDrawMs)
        var checked = false, activationSawOwner = false
        // Force the first draw from within the observer pass, outside a UI
        // update. A fallback armed by firstDrawn is too late for this pass.
        let commit = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeWaiting.rawValue | CFRunLoopActivity.exit.rawValue, false, -1_000_000) { _, _ in
            transactionOpen = false
            CATransaction.commit()
            CATransaction.flush()
            XCTAssertNotNil(session.firstDrawMs)
        }
        let after = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeWaiting.rawValue | CFRunLoopActivity.exit.rawValue, false, 2_000_002) { _, _ in
            XCTAssertNotNil(session.firstDrawMs)
            XCTAssertNil(nav.bootOwner, "the first observer pass must finish the hand-off")
            XCTAssertNotNil(nav.primaryOwner)
            checked = true
        }
        CFRunLoopAddObserver(CFRunLoopGetMain(), commit, .commonModes)
        CFRunLoopAddObserver(CFRunLoopGetMain(), after, .commonModes)
        defer { CFRunLoopObserverInvalidate(commit); CFRunLoopObserverInvalidate(after) }
        let note = NotificationCenter.default.addObserver(forName: ExactSession.dataActivated, object: session, queue: .main) { _ in
            XCTAssertTrue(checked)
            XCTAssertNil(nav.bootOwner)
            XCTAssertNotNil(nav.primaryOwner)
            activationSawOwner = true
        }
        defer { NotificationCenter.default.removeObserver(note) }
        waitFor("the first commit, fallback and firstDrawn's activation") { checked && activationSawOwner }
        XCTAssertNil(nav.bootOwnerFallback)
    }

    func testDrawEvaluationWaitsForTheOwnerHandoff() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        CATransaction.begin()
        var open = true
        defer { if open { CATransaction.commit() } }
        let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
        session.runtime.evaluatesFromDraw = true
        XCTAssertNotNil(nav.bootOwner)
        session.firstDrawn(generation: session.generation, token: 0)
        XCTAssertNil(session.preparedGeneration, "the draw must not post evaluation while ownership is held")
        XCTAssertFalse(session.dataPreparing)
        nav.finishBootOwner()
        XCTAssertNil(nav.bootOwner)
        XCTAssertNotNil(nav.primaryOwner)
        XCTAssertEqual(session.preparedGeneration, session.generation, "handoff releases the draw's evaluation")
        open = false
        CATransaction.commit()
        waitFor("evaluation and data activation after ownership") { session.dataGeneration == session.generation }
    }

    func testADestroyedSessionsHeldDrawEvaluatesNothing() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        CATransaction.begin()
        defer { CATransaction.commit() }
        let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
        session.runtime.evaluatesFromDraw = true
        XCTAssertNotNil(nav.bootOwner)
        session.firstDrawn(generation: session.generation, token: 0)
        XCTAssertNotNil(nav.bootOwnerReady)
        session.destroy()
        nav.finishBootOwner()
        XCTAssertNil(nav.bootOwnerReady, "destruction discharges the held draw")
        XCTAssertNil(session.preparedGeneration, "a destroyed session posts no evaluation")
        XCTAssertFalse(session.dataPreparing)
    }

    func testEmptyObserverPassKeepsWaitingForContent() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        CATransaction.begin()
        var open = true
        defer { if open { CATransaction.commit() } }
        let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
        var checked = false
        let after = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeWaiting.rawValue, false, 2_000_002) { _, _ in
            XCTAssertNil(session.firstDrawMs)
            XCTAssertNotNil(nav.bootOwner, "an empty flush cannot end the first-content wait")
            XCTAssertNil(nav.primaryOwner)
            XCTAssertNotNil(nav.bootOwnerFallback)
            checked = true
        }
        CFRunLoopAddObserver(CFRunLoopGetMain(), after, .commonModes)
        defer { CFRunLoopObserverInvalidate(after) }
        waitFor("an empty observer pass") { checked }
        open = false
        CATransaction.commit()
        CATransaction.flush()
        waitFor("content ownership") { nav.primaryOwner != nil }
    }

    func testMainQueueFlushHoldsAnAlreadyQueuedOwnerConsumer() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        CATransaction.begin()
        var open = true
        defer { if open { CATransaction.commit() } }
        let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
        var queried = false
        DispatchQueue.main.async {
            XCTAssertNil(session.firstDrawMs)
            XCTAssertNil(nav.primaryOwner)
            XCTAssertNotNil(nav.bootOwner, "the queued flush must start with the temporary owner")
            open = false
            CATransaction.commit()
            CATransaction.flush()
            XCTAssertNotNil(session.firstDrawMs)
        }
        DispatchQueue.main.async {
            session.afterBootOwner {
                XCTAssertNotNil(nav.primaryOwner)
                XCTAssertNil(nav.bootOwner)
                XCTAssertNil(nav.transferringBootOwner)
                queried = true
            }
        }
        waitFor("the queued ownership query") { queried }
        waitFor("data activation") { session.dataGeneration == session.generation }
    }

    func testHeldDrawSurvivesDetachResetAndEarlySyncReturnsExactlyOnce() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        guard #available(iOS 18.0, *) else { return }
        for path in ["detach", "reset", "parked-reset", "off-window", "destroy"] {
            var ran = false, discharged = 0, activated = 0
            let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
            let holder = try XCTUnwrap(nav.bootOwner), mounted = try XCTUnwrap(session.view)
            let note = NotificationCenter.default.addObserver(forName: ExactSession.dataActivated, object: session, queue: .main) { _ in activated += 1 }
            let link = UIUpdateLink(view: mounted)
            update = link
            link.addAction(to: .beforeCATransactionCommit) { [unowned self] link, _ in
                guard !ran else { return }
                ran = true
                session.firstDrawn(generation: session.generation, token: 0)
                XCTAssertNotNil(nav.bootOwnerReady)
                let ready = nav.bootOwnerReady
                nav.bootOwnerReady = { discharged += 1; ready?() }
                switch path {
                case "detach":
                    mounted.removeFromSuperview()
                    self.window?.rootViewController?.view.addSubview(mounted)
                    mounted.layoutIfNeeded()
                case "reset": nav.reset(clearFocus: false)
                case "parked-reset":
                    nav.transferringBootOwner = nav.bootOwner
                    nav.bootOwner = nil
                    nav.reset(clearFocus: false)
                case "off-window":
                    let offWindow = ExactView(session: session)
                    nav.finishBootOwner()
                    XCTAssertNil(offWindow.window)
                    session.view = mounted
                default: session.destroy()
                }
                nav.finishBootOwner()
                XCTAssertEqual(discharged, 1, path)
                XCTAssertNil(nav.bootOwnerReady, path)
                XCTAssertNil(nav.bootOwner, path)
                XCTAssertNil(nav.transferringBootOwner, path)
                XCTAssertNil(holder.parent, path)
                XCTAssertNil(holder.view.superview, path)
                XCTAssertEqual(self.window?.rootViewController?.children.count, nav.primaryOwner == nil ? 0 : 1, path)
                if path == "detach" { XCTAssertNotNil(nav.primaryOwner, "remount restores ownership") }
                link.isEnabled = false
            }
            link.isEnabled = true
            waitFor("the held draw and \(path)") { ran }
            if path != "destroy" {
                waitFor("activation after \(path)") { activated == 1 }
                session.firstDrawn(generation: session.generation, token: 0)
                waitForMainQueue()
                XCTAssertEqual(activated, 1, path)
            } else {
                waitForMainQueue()
                XCTAssertEqual(activated, 0)
            }
            NotificationCenter.default.removeObserver(note)
            session.destroy()
        }
    }

    func testFinishingDuringInitialContainmentCannotParkTheHolder() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        var finished = false, discharged = 0
        let probe = BootContainmentProbe { holder in
            guard !finished, let nav = holder.host, let session = nav.presenter.session else { return }
            finished = true
            XCTAssertTrue(nav.syncing, "initial containment still owns the sync guard")
            nav.bootOwnerReady = { discharged += 1; session.firstDrawn(generation: session.generation, token: 0) }
            nav.finishBootOwner()
            XCTAssertEqual(discharged, 0, "the guarded install must finish before releasing activation")
            XCTAssertTrue(nav.transferringBootOwner === holder)
            XCTAssertNotNil(holder.view.window)
        }
        defer { probe.restore() }
        let session = try fixture("BARLESS_ROUTE")
        XCTAssertTrue(finished)
        XCTAssertNotNil(session.presenter.navigation.primaryOwner)
        XCTAssertNotNil(session.presenter.navigation.activeRoute?.node.window, "the real owner holds the content in the window")
        waitFor("activation after the reentrant initial handoff") { session.dataGeneration == session.generation }
        XCTAssertEqual(discharged, 1)
        XCTAssertNil(session.presenter.navigation.transferringBootOwner)
        XCTAssertNil(session.presenter.navigation.bootOwnerFallback)
        XCTAssertTrue(session.presenter.navigation.bootOwnerNotifications.isEmpty)
    }

    func testArrivingLargeTitleMatchesSynchronousInstallation() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        for (tabs, focused) in [(false, false), (true, false), (false, true), (true, true)] {
            func addTitle(_ session: ExactSession) throws {
                let p = session.presenter
                let route = try XCTUnwrap(p.views.values.first { $0.props["testId"] == (tabs ? "route-barless" : "route") })
                p.apply(wireBatch([
                    ["op": "create", "id": 90001, "kind": "view", "props": ["semanticTag": "header"]],
                    ["op": "create", "id": 90002, "kind": "text", "props": ["accessibilityRole": "heading", "accessibilityHeadingLevel": "1", "text": "Launch title"]],
                    ["op": "children", "id": 90001, "ids": [90002]],
                    ["op": "children", "id": route.id, "ids": [90001] + route.container.subviews.compactMap { ($0 as? NodeView)?.id }],
                    ["op": "props", "id": route.id, "set": ["navigationBar": "auto"]]
                ]))
            }
            func dimensions(_ session: ExactSession) throws -> [CGFloat] {
                session.view?.layoutIfNeeded()
                let nav = session.presenter.navigation
                let owner = try XCTUnwrap(nav.primaryNavigation), route = try XCTUnwrap(nav.activeRoute)
                owner.view.layoutIfNeeded()
                XCTAssertEqual(route.navigationItem.largeTitleDisplayMode, .always)
                let scroll = try XCTUnwrap(route.collapseScrollView)
                return [owner.navigationBar.bounds.height, route.view.safeAreaInsets.top, route.view.safeAreaInsets.bottom,
                        scroll.adjustedContentInset.top, scroll.adjustedContentInset.bottom, scroll.contentOffset.y]
            }
            let deferred = try fixture(tabs ? "BOOT_TABS" : "BARLESS_ROUTE")
            XCTAssertNotNil(deferred.presenter.navigation.bootOwner)
            let route = try XCTUnwrap(deferred.presenter.navigation.bootOwner?.route)
            let field = focused ? UITextField(frame: CGRect(x: 20, y: 100, width: 180, height: 40)) : nil
            if let field { route.node.addSubview(field); XCTAssertTrue(field.becomeFirstResponder()) }
            let epoch = route.node.pressEpoch, originalWindow = route.node.window
            try addTitle(deferred)
            XCTAssertTrue(route.node.window === originalWindow)
            if let field { XCTAssertTrue(field.isFirstResponder) }
            XCTAssertEqual(route.node.pressEpoch, epoch)
            XCTAssertNotNil(deferred.presenter.navigation.primaryOwner)
            if focused { deferred.destroy(); continue }
            let arrived = try dimensions(deferred)
            XCTAssertNil(deferred.presenter.navigation.bootOwner)
            deferred.destroy()
            let control = try fixture(tabs ? "BOOT_TABS" : "BARLESS_ROUTE", mounted: false)
            try addTitle(control)
            window?.rootViewController?.view.addSubview(try XCTUnwrap(control.view))
            let synchronous = try dimensions(control)
            XCTAssertEqual(arrived, synchronous, "tabs=\(tabs)")
            XCTAssertGreaterThan(arrived[0], 44, "a root's title is expanded")
            control.destroy()
        }
    }

    func testLaunchMediaTouchAndResizeUseSynchronousOwnership() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        let rotating = try fixture("BARLESS_ROUTE", rotatable: true)
        XCTAssertNil(rotating.presenter.navigation.bootOwner, "rotation-capable launches are ineligible")
        XCTAssertNotNil(rotating.presenter.navigation.primaryOwner)
        rotating.destroy()
        let offset = try fixture("BARLESS_ROUTE", batchEdit: { batch in
            let id = batch.ops.first { $0.props["testId"] == "route" }!.id
            for i in batch.ops.indices where batch.ops[i].op == .frame && batch.ops[i].id == id {
                batch.ops[i].x = 20; batch.ops[i].w -= 20
            }
        })
        XCTAssertNil(offset.presenter.navigation.bootOwner, "offset route planes install synchronously")
        XCTAssertNotNil(offset.presenter.navigation.primaryOwner)
        offset.destroy()
        let touched = try fixture("BARLESS_ROUTE"), touchNav = touched.presenter.navigation
        let target = touched.view?.hitTest(CGPoint(x: 100, y: 100), with: BootTouchEvent())
        XCTAssertNotNil(target?.window)
        XCTAssertNotNil(touchNav.primaryOwner, "touch hit testing ends deferral before the gesture starts")
        XCTAssertNil(touchNav.bootOwner)
        XCTAssertNil(touchNav.transferringBootOwner)
        touched.destroy()
        let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
        let size = try XCTUnwrap(session.view).bounds.size
        session.view?.frame.size = CGSize(width: size.height, height: size.width)
        session.view?.layoutIfNeeded()
        XCTAssertNil(nav.bootOwner, "a launch resize ends deferral before fitting the new geometry")
        XCTAssertNotNil(nav.primaryOwner)
        session.destroy()
        for (kind, props) in [("video", [:]), ("audio", [:]), ("image", ["imageSource": "photos:launch"])] {
            let media = try fixture("BARLESS_ROUTE"), nav = media.presenter.navigation
            media.presenter.apply(wireBatch([["op": "create", "id": 90003, "kind": kind, "props": props]]))
            nav.sync(empty)
            XCTAssertNil(nav.bootOwner, "media and package pictures can load code before first pixel")
            XCTAssertNotNil(nav.primaryOwner)
            media.destroy()
        }
    }

    func testFirstTreeFrameInstallsSynchronously() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        let session = try fixture("BARLESS_ROUTE", batchEdit: { batch in
            let route = batch.ops.first { $0.props["testId"] == "route" }!.id
            let children = batch.ops.last { $0.op == .children && $0.id == route }!.ids
            batch.ops += wireBatch([
                ["op": "create", "id": 90003, "kind": "iframe"],
                ["op": "children", "id": route, "ids": children + [90003]]
            ]).ops
        }), nav = session.presenter.navigation
        XCTAssertNotNil(session.presenter.views[90003]?.window)
        XCTAssertNil(nav.bootOwner, "a guest can present a JavaScript dialog before an Exact event")
        XCTAssertNotNil(nav.primaryOwner)
    }

    func testAnAnimatedScrollContinuesThroughTheHandoff() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        guard #available(iOS 18.0, *) else { return }
        let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
        let holder = try XCTUnwrap(nav.bootOwner)
        let route = try XCTUnwrap(holder.route)
        // Hold activation and both scheduled candidates so this test chooses
        // the transfer while UIKit's scroll animation is already moving.
        session.activatedGeneration = session.generation
        let scroll = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "list" }?.scrollView)
        var moving = false, finished = false, checked = false
        scroll.setContentOffset(CGPoint(x: 0, y: 500), animated: true)
        let link = UIUpdateLink(view: session.view!)
        update = link
        link.addAction(to: .afterCADisplayLinkDispatch) { link, _ in
            guard !checked, scroll.contentOffset.y > 0, scroll.contentOffset.y < 500 else { return }
            moving = true
            let before = scroll.contentOffset, epoch = route.node.pressEpoch
            XCTAssertTrue(nav.bootOwner === holder, "the original holder must survive until the moving scroll transfers")
            XCTAssertNil(nav.primaryOwner)
            nav.finishBootOwner()
            XCTAssertNotNil(nav.primaryOwner)
            XCTAssertEqual(scroll.contentOffset, before)
            XCTAssertEqual(route.node.pressEpoch, epoch)
            XCTAssertNotNil(scroll.window)
            XCTAssertNil(nav.transferringBootOwner)
            checked = true
            link.isEnabled = false
        }
        link.isEnabled = true
        // Hold the post-commit transfer until the scroll has actually moved.
        if let observer = nav.bootOwnerFallback { CFRunLoopObserverInvalidate(observer) }
        nav.bootOwnerFallback = nil
        waitFor("the scroll in flight crosses the handoff") { checked }
        waitFor("the scroll reaches its animated destination") { finished = abs(scroll.contentOffset.y - 500) < 0.01; return finished }
        XCTAssertTrue(moving && finished)
    }

    func testBackgroundAndHiddenWindowFinishWithoutAnUpdate() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        for name in [UIScene.didEnterBackgroundNotification, UIWindow.didBecomeHiddenNotification] {
            let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
            XCTAssertNotNil(nav.bootOwner)
            if name == UIWindow.didBecomeHiddenNotification { window?.isHidden = true }
            else { NotificationCenter.default.post(name: name, object: window?.windowScene) }
            XCTAssertNil(nav.bootOwner)
            XCTAssertNotNil(nav.primaryOwner)
            XCTAssertTrue(nav.bootOwnerNotifications.isEmpty)
            session.destroy()
        }
    }

    func testAppearanceAndFirstResponderMatchSynchronousContainment() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        let probe = BootAppearanceProbe()
        defer { probe.restore() }
        let session = try fixture("BARLESS_ROUTE", beforeBoot: { $0.window?.tintColor = .systemGreen }), nav = session.presenter.navigation
        let route = try XCTUnwrap(nav.bootOwner?.route)
        XCTAssertNil(route.contentViewport, "this fixture exercises a bare node's parent move")
        XCTAssertEqual(nav.bootOwner?.view.tintColor, .systemGreen, "an unsaid tint inherits from the window")
        nav.bootOwner?.view.backgroundColor = .systemPink
        let field = UITextField(frame: CGRect(x: 20, y: 100, width: 180, height: 40))
        route.node.addSubview(field)
        XCTAssertTrue(field.becomeFirstResponder())
        XCTAssertTrue(FocusSearch.items(try XCTUnwrap(session.view), in: session.view!.bounds).contains { ($0 as? UIView) === field })
        let epoch = route.node.pressEpoch
        let animation = CABasicAnimation(keyPath: "opacity")
        animation.fromValue = 0.5; animation.toValue = 1; animation.duration = 10
        route.node.layer.add(animation, forKey: "continuity")
        var committed = false
        CATransaction.begin()
        CATransaction.setCompletionBlock {
            XCTAssertNil(nav.bootOwner)
            XCTAssertNil(nav.transferringBootOwner)
            XCTAssertNil(route.bootSurface)
            committed = true
        }
        CATransaction.commit()
        nav.finishBootOwner()
        XCTAssertFalse(committed, "the synchronous transfer returns before the transaction completion")
        CATransaction.flush()
        waitFor("the transfer's transaction completes after removing the holder") { committed }
        nav.primaryOwner?.view.layoutIfNeeded()
        waitFor("the route's first appearance ends") { (probe.events[route] ?? []).contains("viewDidAppear:") }
        XCTAssertTrue(field.isFirstResponder)
        XCTAssertTrue(FocusSearch.items(try XCTUnwrap(session.view), in: session.view!.bounds).contains { ($0 as? UIView) === field })
        XCTAssertEqual(route.view.tintColor, .systemGreen)
        XCTAssertEqual(route.view.backgroundColor, .systemPink, "the current backdrop moves with the content")
        XCTAssertEqual(route.node.pressEpoch, epoch, "the route never leaves its window")
        XCTAssertNotNil(route.node.layer.animation(forKey: "continuity"))
        XCTAssertTrue(route.node.superview === (route.contentViewport ?? route.view), "no permanent bridge in the hierarchy")
        let deferred = probe.events[route] ?? []
        session.destroy()
        // Windowless launch takes today's synchronous install path.
        let control = try fixture("BARLESS_ROUTE", mounted: false)
        window?.rootViewController?.view.addSubview(try XCTUnwrap(control.view))
        control.view?.layoutIfNeeded()
        let synchronous = try XCTUnwrap(control.presenter.navigation.activeRoute)
        waitFor("the synchronous route's appearance ends") { (probe.events[synchronous] ?? []).contains("viewDidAppear:") }
        XCTAssertEqual(deferred, probe.events[synchronous] ?? [])
        XCTAssertEqual(deferred, ["viewWillAppear:", "viewIsAppearing:", "viewDidAppear:"])
    }

    func testControlBandsAndTabScrollerMatchTheSynchronousFirstCommit() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        for key in ["BOOT_CONTROL_TOP", "BOOT_CONTROL_BOTTOM", "BOOT_CONTROL_TAB"] {
            var snapshots: [([UInt32: [CGFloat]], Data, [String], [String])] = []
            for synchronous in [false, true] {
                CATransaction.begin()
                let session = try fixture(key, rotatable: synchronous), nav = session.presenter.navigation
                let holder = nav.bootOwner
                XCTAssertEqual(holder != nil, !synchronous, key)
                let route = try XCTUnwrap(nav.activeRoute), node = route.node
                let list = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "list" })
                XCTAssertTrue(ViewPresentation.controller(for: list, layout: true) === route)
                XCTAssertTrue(ViewPresentation.controllers(for: list, layout: true).first === route)
                XCTAssertEqual(nav.routeKey(containing: list, layout: true), route.key)
                XCTAssertTrue(ViewPresentation.root(of: list) === route.surface)
                XCTAssertTrue(LiftTarget.container(of: list) === route.surface)
                XCTAssertTrue(nav.contentScrollView(of: route) === list.scrollView)
                if let holder {
                    route.ridingBars = [list]
                    route.rideBars()
                    XCTAssertEqual(list.controlRide, node.convert(CGPoint(x: 0, y: route.surface.safeAreaInsets.top), from: route.surface).y)
                    route.ridingBars = []
                    list.controlRide = 0
                    list.applyTransform()
                    for edge in [NSDirectionalRectEdge.top, .bottom] {
                        XCTAssertTrue(holder.contentScrollView(for: edge) === route.contentScrollView(for: edge), key + " physical owner edge selection")
                    }
                }
                XCTAssertTrue(route.stack?.stackRoutes.contains(where: { $0 === route }) == true)
                XCTAssertFalse(nav.isInactiveRoute(containing: list, layout: true))
                XCTAssertTrue(nav.wantsWholeView)
                let regions = LayoutRegionsIOS.insets(of: list, viewport: list.bounds)
                let surfaceFrame = list.convert(list.bounds, to: route.surface)
                XCTAssertEqual(regions, LayoutRegionsIOS.insets(of: route.surface, viewport: surfaceFrame))
                let order = accessibilityOrder(try XCTUnwrap(session.view))
                let focus = FocusSearch.items(route.surface, in: route.surface.bounds).compactMap { item -> String? in
                    guard let view = item as? UIView else { return nil }
                    return "\(type(of: view)) \(view.convert(view.bounds, to: route.surface)) \(view.accessibilityIdentifier ?? "")"
                }.sorted()
                let geometry = geometry(session)
                CATransaction.commit()
                CATransaction.flush()
                XCTAssertNotNil(session.firstDrawMs)
                if !synchronous { XCTAssertTrue(nav.bootOwner === holder); XCTAssertNil(nav.primaryOwner) }
                let format = UIGraphicsImageRendererFormat(); format.scale = 1
                let view = try XCTUnwrap(session.view)
                let pixels = UIGraphicsImageRenderer(size: view.bounds.size, format: format).image { view.layer.render(in: $0.cgContext) }.pngData()!
                snapshots.append((geometry, pixels, order, focus))
                let attachment = XCTAttachment(data: pixels, uniformTypeIdentifier: "public.png")
                attachment.name = key + (synchronous ? "-synchronous-first-commit" : "-deferred-first-commit")
                attachment.lifetime = .keepAlways
                add(attachment)
                if !synchronous {
                    XCTAssertTrue(list.pageBand.top > 0 || list.pageBand.bottom > 0, "the ordinary controls must band the scroller")
                    nav.finishBootOwner()
                    XCTAssertTrue(nav.activeRoute?.node === node)
                    XCTAssertEqual(geometry, self.geometry(session), key)
                    XCTAssertEqual(order, accessibilityOrder(view), key)
                }
                session.destroy()
            }
            XCTAssertEqual(snapshots[0].0, snapshots[1].0, key + " frames, safe areas, insets, offsets, content sizes")
            XCTAssertEqual(snapshots[0].1, snapshots[1].1, key + " first-commit pixels")
            XCTAssertEqual(snapshots[0].2, snapshots[1].2, key + " accessibility order")
            XCTAssertEqual(snapshots[0].3, snapshots[1].3, key + " UIKit focus candidates and frames")
        }
    }

    func testDrawInANonCommonModeOnlySchedulesTheHandoff() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        CATransaction.begin()
        let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
        let holder = try XCTUnwrap(nav.bootOwner)
        let mode = RunLoop.Mode("ExactBootTestMode")
        var returned = false, checked = false
        CFRunLoopPerformBlock(CFRunLoopGetMain(), mode.rawValue as CFString) {
            CATransaction.commit()
            CATransaction.flush()
            returned = true
            XCTAssertTrue(nav.bootOwner === holder, "the draw receipt cannot install inside flush, even outside common modes")
            XCTAssertNil(nav.primaryOwner)
            CFRunLoopPerformBlock(CFRunLoopGetMain(), mode.rawValue as CFString) {
                XCTAssertTrue(returned)
                XCTAssertNotNil(nav.primaryOwner)
                XCTAssertNotNil(nav.activeRoute?.node.window)
                checked = true
                CFRunLoopStop(CFRunLoopGetMain())
            }
        }
        // A source keeps this private mode runnable without enabling a CA observer in it.
        let timer = Timer(timeInterval: 60, repeats: false) { _ in }
        RunLoop.main.add(timer, forMode: mode)
        defer { timer.invalidate() }
        let deadline = Date(timeIntervalSinceNow: 10)
        while !checked && Date() < deadline { RunLoop.main.run(mode: mode, before: deadline) }
        XCTAssertTrue(checked)
    }

    func testAClosedFirstTreeAlertDefersAndOpeningItInstallsOwnership() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "boot comparisons need a visible scene") }
        let session = try fixture("BOOT_DIALOG"), nav = session.presenter.navigation
        XCTAssertNotNil(nav.bootOwner)
        session.presenter.dialogCommand("showModal", "launch-alert")
        XCTAssertNil(nav.bootOwner)
        XCTAssertNotNil(nav.primaryOwner)
        XCTAssertNotNil(nav.activeRoute?.node.window)
        waitFor("the first-tree alert opens") { nav.activeRoute?.presentedViewController is UIAlertController }
        nav.activeRoute?.presentedViewController?.dismiss(animated: false)
    }

    func testPresentationFromTheWaitingContentInstallsItsRealPresenter() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        for source in ["node", "route-view", "menus"] {
            let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
            let holder = try XCTUnwrap(nav.bootOwner), node = try XCTUnwrap(holder.route?.node)
            let target = source == "route-view" ? try XCTUnwrap(holder.route?.view) : node
            let owner = try XCTUnwrap(source == "menus" ? session.presenter.menus.controller(for: target) : ViewPresentation.presenter(for: target))
            XCTAssertNil(nav.bootOwner)
            XCTAssertTrue(owner === nav.activeRoute)
            XCTAssertNotNil(owner.view.window)
            let alert = UIAlertController(title: "Launch confirmation", message: nil, preferredStyle: .alert)
            alert.addAction(UIAlertAction(title: "OK", style: .default))
            owner.present(alert, animated: false)
            XCTAssertNotNil(alert.presentingViewController)
            XCTAssertFalse(alert.presentingViewController === holder)
            owner.dismiss(animated: false)
            session.destroy()
        }
    }

    func testInputWithoutATouchInstallsOwnershipBeforeItsHandlers() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "input needs the mounted first frame") }
        for kind in ["accessibility", "native-button", "key", "keyup", "shortcut", "focus", "native-focus", "evaluated-input", "input", "change", "checked", "value"] {
            CATransaction.begin()
            let session = try fixture("BOOT_CONTROL_TOP"), p = session.presenter, nav = p.navigation
            let holder = try XCTUnwrap(nav.bootOwner)
            let node = try XCTUnwrap(p.views.values.first { $0.handlers.contains("press") })
            var called = false
            func check() {
                XCTAssertNotNil(nav.primaryOwner, kind)
                XCTAssertNotNil(nav.activeRoute?.node.window, kind)
                called = true
            }
            p.onPress = { _ in check() }
            p.onInput = { _, _ in check() }
            p.onChange = { _, _ in check() }
            p.onChecked = { _, _ in check() }
            p.onControlValue = { _, _, _, _ in check() }
            p.onKey = { _, _ in check() }
            p.onFocus = { _ in check() }
            node.handlers.formUnion(["key", "keyup", "focus"])
            CATransaction.commit()
            CATransaction.flush()
            XCTAssertTrue(nav.bootOwner === holder, "the input must cross the post-flush wait")
            switch kind {
            case "accessibility": XCTAssertTrue(node.accessibilityActivate())
            case "native-button": node.activateNative()
            case "key": _ = p.keyDown(at: node, "x")
            case "keyup": _ = p.keyUp(at: node, "x")
            case "shortcut": p.pressShortcut(node)
            case "focus": XCTAssertTrue(p.moveFocus(backward: false))
            case "native-focus": p.focus(node.id)
            case "evaluated-input":
                func event() -> Batch { check(); return wireBatch([]) }
                session.applyInput(event())
            case "input": p.typed(node.id, "typed", input: true)
            case "change": p.change(node.id, "changed")
            case "checked": p.checked(node.id, true)
            default: p.controlValue(node.id, "2", input: true, change: false)
            }
            XCTAssertTrue(called, kind)
            XCTAssertNil(nav.bootOwner, kind)
            session.destroy()
        }
    }

    func testClipboardAndNonInputEventsEnterTheRunnerAfterOwnership() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "boot comparisons need a visible scene") }
        for kind in ["copy", "cut", "paste", "selection", "scroll", "timer", "notification", "hold", "height-drag"] {
            CATransaction.begin()
            let session = try fixture("BOOT_CLIPBOARD"), nav = session.presenter.navigation
            let holder = try XCTUnwrap(nav.bootOwner)
            CATransaction.commit()
            CATransaction.flush()
            XCTAssertTrue(nav.bootOwner === holder, kind)
            var observed = false
            session.runtime.observeBatch = { _, _ in
                Owner.shared.callMain {
                    XCTAssertNotNil(nav.primaryOwner, kind)
                    XCTAssertNotNil(nav.activeRoute?.node.window, kind)
                    observed = true
                }
            }
            let route = try XCTUnwrap(nav.activeRoute)
            switch kind {
            case "copy": route.node.copy(nil)
            case "cut": route.node.cut(nil)
            case "paste": route.node.paste(nil)
            case "selection": session.presenter.selectionChange(route.node.id, "", 0, 0)
            case "scroll": session.presenter.scroll(route.node.id, [0, 0, 0, 0, 0, 0])
            case "hold": _ = session.runtime.holdBegin(route.node.id, property: 0, now: session.now())
            case "height-drag": _ = session.runtime.heightDragBegin(0, targetKey: 0, now: session.now())
            case "timer": session.apply(session.runtime.advance(now: session.now()))
            default: session.apply(session.runtime.moduleEvent("{}"))
            }
            XCTAssertTrue(observed, kind)
            XCTAssertNil(nav.bootOwner, kind)
            session.runtime.observeBatch = nil
            session.destroy()
        }
    }

    func testAFirstScreenCSSAnimationReceivesItsTick() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "boot comparisons need a visible scene") }
        CATransaction.begin()
        let session = try fixture("BOOT_MOTION"), nav = session.presenter.navigation
        XCTAssertNotNil(nav.bootOwner)
        XCTAssertTrue(session.frames.motion)
        session.frames.run(true)
        let link = try XCTUnwrap(session.frames.link)
        CATransaction.commit()
        CATransaction.flush()
        XCTAssertNotNil(nav.bootOwner)
        var ticked = false
        session.runtime.observeBatch = { _, _ in ticked = true }
        session.frames.tick(link)
        session.drainFill()
        XCTAssertNotNil(nav.primaryOwner)
        XCTAssertNil(nav.bootOwner)
        XCTAssertTrue(ticked, "the first tick is admitted, not dropped")
        session.runtime.observeBatch = nil
    }

    func testPlainListInstallsSynchronously() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "boot comparisons need a visible scene") }
        let session = try fixture("BOOT_PLAIN"), nav = session.presenter.navigation
        XCTAssertNil(nav.bootOwner, "native row controllers install synchronously")
        XCTAssertNotNil(nav.primaryOwner)
    }


    func testQueuedTransformGeometryInstallsOwnershipBeforeItsHandler() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the regression needs the mounted first tree") }
        CATransaction.begin()
        defer { CATransaction.commit() }
        let session = try fixture("BOOT_TRANSFORM"), p = session.presenter, nav = p.navigation
        XCTAssertNotNil(nav.bootOwner)
        XCTAssertFalse(p.transformBindings.isEmpty)
        let admit = session.runtime.beforeEvent
        var admitted = false
        session.runtime.beforeEvent = {
            admit?()
            XCTAssertNotNil(nav.primaryOwner, "admission precedes the transformgeometry handler")
            XCTAssertNil(nav.bootOwner)
            admitted = true
        }
        waitForMainQueue("the geometry report queued by the boot layout")
        XCTAssertTrue(p.views.values.contains { $0.props["testId"] == "reports" && $0.props["text"] == "1" }, "the authored geometry handler ran")
        XCTAssertTrue(admitted)
        XCTAssertNil(nav.bootOwner)
        session.runtime.beforeEvent = nil
    }

    func testPaddedFirstScrollerMatchesTheSynchronousFirstCommit() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the regression needs the mounted first tree") }
        var snapshots: [([UInt32: [CGFloat]], Data, [CGFloat]?)] = []
        for synchronous in [false, true] {
            CATransaction.begin()
            let session = try fixture("BOOT_PADDED", rotatable: synchronous), nav = session.presenter.navigation
            XCTAssertEqual(nav.bootOwner != nil, !synchronous)
            let route = try XCTUnwrap(nav.activeRoute), list = try XCTUnwrap(route.collapseScroll?.scrollView)
            XCTAssertGreaterThan(list.contentInset.top, 0, "authored padding adds clearance below the status bar")
            let values = geometry(session), local = nav.localInsets.wanted[route.node.id]
            XCTAssertEqual(local, [route.surface.safeAreaInsets.top, 0, route.surface.safeAreaInsets.bottom, 0])
            CATransaction.commit()
            CATransaction.flush()
            XCTAssertNotNil(session.firstDrawMs)
            XCTAssertEqual(nav.bootOwner != nil, !synchronous)
            let view = try XCTUnwrap(session.view), format = UIGraphicsImageRendererFormat()
            format.scale = 1
            let pixels = UIGraphicsImageRenderer(size: view.bounds.size, format: format).image { view.layer.render(in: $0.cgContext) }.pngData()!
            snapshots.append((values, pixels, local))
            nav.finishBootOwner()
            XCTAssertEqual(values, geometry(session), "handoff preserves insets, adjusted insets and offsets")
            session.destroy()
        }
        XCTAssertEqual(snapshots[0].0, snapshots[1].0)
        XCTAssertEqual(snapshots[0].1, snapshots[1].1, "first-commit pixels")
        XCTAssertEqual(snapshots[0].2, snapshots[1].2, "route-local safe areas")
    }

    func testFirstTreeTipRetainsTheInstalledPresenter() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the regression needs the mounted first tree") }
        CATransaction.begin()
        defer { CATransaction.commit() }
        let recorded = TipsRecorded.install(in: self)
        let session = try fixture("BOOT_TIP"), nav = session.presenter.navigation
        XCTAssertNil(nav.bootOwner, "a first tree carrying a tip installs synchronously")
        XCTAssertNotNil(nav.primaryOwner)
        session.presenter.presentTips()
        guard case let .view(anchor, from: owner)? = recorded.tips.first(where: { $0.content.id == "boot-tip" })?.anchor else {
            return XCTFail("the first tree supplies the tip's view anchor")
        }
        XCTAssertNil(nav.bootOwner)
        XCTAssertNotNil(nav.primaryOwner)
        XCTAssertTrue(owner === nav.activeRoute)
        waitForMainQueue("TipKit's asynchronous eligibility can use the retained presenter")
        let popover = UIViewController()
        popover.modalPresentationStyle = .popover
        popover.popoverPresentationController?.sourceView = anchor
        popover.popoverPresentationController?.sourceRect = anchor.bounds
        owner.present(popover, animated: false)
        XCTAssertNotNil(popover.presentingViewController)
        XCTAssertNotNil(owner.view.window)
        XCTAssertTrue(owner === nav.activeRoute)
        owner.dismiss(animated: false)
    }

    func testAReplacementScrollerDuringTheWaitMatchesSynchronousFirstCommit() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        var snapshots: [[UInt32: [CGFloat]]] = []
        for synchronous in [false, true] {
            CATransaction.begin()
            let session = try fixture("BARLESS_ROUTE", rotatable: synchronous), p = session.presenter, nav = p.navigation
            XCTAssertEqual(nav.bootOwner != nil, !synchronous)
            let route = try XCTUnwrap(nav.activeRoute), old = try XCTUnwrap(route.collapseScroll)
            let parent = try XCTUnwrap(old.superview as? NodeView), f = old.frame
            let children = parent.container.subviews.compactMap { ($0 as? NodeView)?.id }.map { $0 == old.id ? UInt32(90005) : $0 }
            p.apply(wireBatch([
                ["op": "create", "id": 90005, "kind": "scroll", "props": ["id": "replacement"], "style": ["overflow_y": "auto"]],
                ["op": "children", "id": 90005, "ids": old.container.subviews.compactMap { ($0 as? NodeView)?.id }],
                ["op": "children", "id": parent.id, "ids": children],
                ["op": "frame", "id": 90005, "x": f.minX, "y": f.minY, "w": f.width, "h": f.height],
                ["op": "props", "id": route.node.id, "set": ["navigationScroll": "replacement"]]
            ]))
            let replacement = try XCTUnwrap(p.views[90005])
            XCTAssertTrue(route.collapseScroll === replacement)
            XCTAssertTrue(route.edgeScroll === replacement)
            let before = geometry(session)
            CATransaction.commit()
            CATransaction.flush()
            XCTAssertNotNil(session.firstDrawMs)
            XCTAssertEqual(nav.bootOwner != nil, !synchronous)
            snapshots.append(before)
            nav.finishBootOwner()
            XCTAssertEqual(before, geometry(session))
            session.destroy()
        }
        XCTAssertEqual(snapshots[0], snapshots[1])
    }

    func testRootKeyboardFocusKeepsTheHolderThroughFirstCommit() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        var focus: [Bool] = []
        for synchronous in [false, true] {
            CATransaction.begin()
            let session = try fixture("BARLESS_ROUTE", rotatable: synchronous, batchEdit: { batch in
                if let index = batch.ops.firstIndex(where: { $0.op == .create }) { batch.ops[index].handlers.insert("key") }
            }, beforeBoot: { _ in _ = FirstResponder.current?.resignFirstResponder() }), nav = session.presenter.navigation
            waitForMainQueue("the first keyboard tree's queued mount")
            let view = try XCTUnwrap(session.view)
            XCTAssertTrue(view.canBecomeFirstResponder)
            XCTAssertTrue(view.window?.isKeyWindow == true)
            XCTAssertFalse(nav.defersFocus)
            _ = FirstResponder.current?.resignFirstResponder()
            view.syncKeyboardFocus()
            XCTAssertEqual(nav.bootOwner != nil, !synchronous, "keyboard occupancy is a read")
            CATransaction.commit()
            CATransaction.flush()
            XCTAssertNotNil(session.firstDrawMs)
            XCTAssertEqual(nav.bootOwner != nil, !synchronous)
            nav.finishBootOwner()
            view.syncKeyboardFocus()
            focus.append(view.isFirstResponder)
            _ = view.resignFirstResponder()
            session.destroy()
        }
        XCTAssertEqual(focus[0], focus[1], "root focus after handoff matches synchronous installation")
    }

    func testFullWindowEmbeddedViewInstallsSynchronously() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        let session = try fixture("BARLESS_ROUTE", ownsWindow: false), nav = session.presenter.navigation
        XCTAssertNotNil(nav.primaryOwner)
        XCTAssertNil(nav.bootOwner)
    }

    func testDevMenuResolvesItsStoredPresenterAfterOwnership() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the boot owner needs a visible scene") }
        let session = try fixture("BARLESS_ROUTE"), nav = session.presenter.navigation
        XCTAssertNotNil(nav.bootOwner)
        DevMenu.session = session
        DevMenu.controller = window?.rootViewController
        defer { DevMenu.session = nil; DevMenu.controller = nil }
        let presenter = try XCTUnwrap(DevMenu.presenter)
        XCTAssertNotNil(nav.primaryOwner)
        XCTAssertNil(nav.bootOwner)
        XCTAssertTrue(presenter === window?.rootViewController)
    }

    func testNativeBarsInstallInTheBootApply() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the navigation owner is installed in a window scene") }
        let session = try fixture("BARLESS_TAB")
        XCTAssertNotNil(session.presenter.navigation.primaryOwner)
        XCTAssertNotNil(session.presenter.navigation.tabController)
        XCTAssertNil(session.presenter.navigation.bootOwner)
    }

    func testWindowlessBootInstallsOnMount() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the navigation owner is installed in a window scene") }
        let session = try fixture("BARLESS_ROUTE", mounted: false)
        XCTAssertNil(session.presenter.navigation.primaryOwner)
        XCTAssertNil(session.presenter.navigation.bootOwner)
        window?.rootViewController?.view.addSubview(try XCTUnwrap(session.view))
        session.view?.layoutIfNeeded()
        waitFor("mount installs the windowless boot's owner") { session.presenter.navigation.primaryOwner != nil }
        XCTAssertNotNil(session.presenter.navigation.primaryOwner)
        XCTAssertNil(session.presenter.navigation.bootOwner)
    }
}

private final class BootWindowController: UIViewController {
    var orientations: UIInterfaceOrientationMask = .portrait
    override var supportedInterfaceOrientations: UIInterfaceOrientationMask { orientations }
}

private final class BootTouchEvent: UIEvent {
    override var type: UIEvent.EventType { .touches }
}

private final class BootContainmentProbe {
    private let method: Method
    private let original: IMP
    private let replacement: IMP
    init(_ entered: @escaping (PlainTab) -> Void) {
        let selector = #selector(UIViewController.didMove(toParent:))
        let inherited = class_getInstanceMethod(PlainTab.self, selector)!
        class_addMethod(PlainTab.self, selector, method_getImplementation(inherited), method_getTypeEncoding(inherited))
        method = class_getInstanceMethod(PlainTab.self, selector)!
        original = method_getImplementation(method)
        let old = original
        let block: @convention(block) (PlainTab, UIViewController?) -> Void = { holder, parent in
            typealias Call = @convention(c) (UIViewController, Selector, UIViewController?) -> Void
            unsafeBitCast(old, to: Call.self)(holder, selector, parent)
            if parent != nil, holder.bootContent { entered(holder) }
        }
        replacement = imp_implementationWithBlock(block)
        method_setImplementation(method, replacement)
    }
    func restore() { method_setImplementation(method, original); imp_removeBlock(replacement) }
}

private final class BootAppearanceProbe {
    // Retain each observed controller so another cannot reuse its address as a key.
    var events: [UIViewController: [String]] = [:]
    private var originals: [(Method, IMP, IMP)] = []
    init() {
        for name in ["viewWillAppear:", "viewIsAppearing:", "viewDidAppear:", "viewWillDisappear:", "viewDidDisappear:"] {
            let selector = NSSelectorFromString(name)
            let inherited = class_getInstanceMethod(RouteController.self, selector)!
            class_addMethod(RouteController.self, selector, method_getImplementation(inherited), method_getTypeEncoding(inherited))
            let method = class_getInstanceMethod(RouteController.self, selector)!
            let old = method_getImplementation(method)
            let block: @convention(block) (UIViewController, Bool) -> Void = { [weak self] controller, animated in
                self?.events[controller, default: []].append(name)
                typealias Call = @convention(c) (UIViewController, Selector, Bool) -> Void
                unsafeBitCast(old, to: Call.self)(controller, selector, animated)
            }
            let replacement = imp_implementationWithBlock(block)
            originals.append((method, old, replacement))
            method_setImplementation(method, replacement)
        }
    }
    func restore() {
        for (method, old, replacement) in originals { method_setImplementation(method, old); imp_removeBlock(replacement) }
        originals = []
    }
}
#endif
