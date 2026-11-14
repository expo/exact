import XCTest
@testable import ExactKit
#if os(macOS)
import AppKit
#else
import UIKit
#endif

final class FragmentTests: XCTestCase {
    private var session: ExactSession!
    private var assets: URL!
    private var oldAssets: URL!
    #if os(macOS)
    private var window: NSWindow!
    /// A field beside the session's view in its window: another owner's focus.
    private var outside: NSTextField?
    #else
    private var window: UIWindow!
    #endif

    override func tearDown() {
        session?.destroy(); session = nil
        #if os(macOS)
        window?.close(); outside = nil
        KeyboardNavigation.agent = nil
        #else
        window?.isHidden = true
        #endif
        window = nil
        if let oldAssets { ExactApp.shared.assetRoot = oldAssets }
        if let assets { try? FileManager.default.removeItem(at: assets) }
        super.tearDown()
    }

    private func fixture() throws {
        #if os(macOS)
        _ = NSApplication.shared
        KeyboardNavigation.agent = true
        #endif
        assets = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: assets, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: assets.appendingPathComponent("assets"), withIntermediateDirectories: true)
        try "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"800\"><rect width=\"100\" height=\"800\"/></svg>"
            .write(to: assets.appendingPathComponent("assets/late.svg"), atomically: true, encoding: .utf8)
        oldAssets = ExactApp.shared.assetRoot
        ExactApp.shared.assetRoot = assets
        session = ExactApp.shared.makeSession(label: "fragment")
        XCTAssertTrue(session.openIncoming("/item/7#moves"))
        // Keep the real loader pending until the assertion releases the decode.
        session.rasters.testRequest = { _ in (0, 8) }
        let bytes = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_FRAGMENT_PLAN"])))
        let size = CGSize(width: 320, height: 400)
        let view = session.presenter.viewport
        view.frame = CGRect(origin: .zero, size: size)
        #if os(macOS)
        window = NSWindow(contentRect: CGRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.autorecalculatesKeyViewLoop = false
        if let outside {
            let content = NSView(frame: CGRect(origin: .zero, size: size))
            content.addSubview(view); content.addSubview(outside)
            window.contentView = content
        } else {
            window.contentView = view
        }
        #else
        window = UIWindow(frame: CGRect(origin: .zero, size: size))
        let controller = UIViewController()
        controller.view = view
        window.rootViewController = controller
        #endif
        #if os(macOS)
        window.orderFront(nil)
        #else
        window.makeKeyAndVisible()
        #endif
        XCTAssertNil(session.boot(plan: bytes, size: size).error)
        session.apply(session.runtime.dataReady())
        waitFor("the launch fragment") { self.session.presenter.fragmentArm == nil && self.offset > 100 }
        XCTAssertGreaterThan(gap("moves"), 1, "the unloaded tail leaves too little scroll range to align the target")
        XCTAssertNil(try node("picture").raster)
    }

    private func node(_ name: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["id"] == name || $0.props["testId"] == name })
    }

    private var offset: CGFloat {
        let port = try! node("port")
        #if os(macOS)
        return port.scroll!.contentView.bounds.minY
        #else
        return port.scrollView!.contentOffset.y
        #endif
    }

    private func gap(_ name: String) -> CGFloat {
        let target = try! node(name), port = try! node("port")
        #if os(macOS)
        return target.convert(target.bounds, to: port.scroll!.contentView).minY - offset
        #else
        return target.convert(target.bounds, to: port.scrollView!).minY - offset
        #endif
    }

    private func releasePicture() throws {
        session.rasters.testRequest = nil
        session.rasters.reconcile()
        waitFor("the picture and its layout") {
            let raster = self.session.rasters.diagnostics
            return (try? self.node("picture").raster) != nil && (try? self.node("tail").raster) != nil
                && raster["pending"] as? UInt64 == 0 && raster["sessionRunning"] as? UInt64 == 0
                && raster["ready"] as? UInt64 == 0
                && self.session.rasters.loadingOnScreen == 0 && !self.session.ownerInFlight
        }
        var idle = false
        session.whenIdle { idle = true }
        waitFor("the image layout's finalization") { idle }
        waitForMainQueue()
    }

    func testFragmentStaysAlignedUntilItsPicturesLand() throws {
        try fixture()
        let before = offset
        try releasePicture()
        XCTAssertGreaterThan(offset, before + 100, "the picture's natural height moves the target")
        XCTAssertEqual(gap("moves"), 0, accuracy: 1)
        XCTAssertNil(session.presenter.fragmentHeld, "finished pictures release the alignment")
    }

    func testFailedPicturesReleaseTheAlignment() throws {
        try fixture()
        try node("picture").loadImage("assets/missing.svg")
        try node("tail").loadImage("assets/missing.svg")
        waitFor("both image failures") { self.session.rasters.testAnnounced.filter { $0.failure != nil }.count == 2 }
        waitForMainQueue()
        XCTAssertNil(session.presenter.fragmentHeld, "an error with no authored handler also ends the hold")
    }

    func testFragmentResumesAfterAnUnchangedFill() throws {
        try fixture()
        try releasePicture()
        let p = session.presenter
        p.scrollElementIntoView(["other", "start", "nearest", "instant"])
        let chosen = offset
        let visit = try XCTUnwrap(Int(try XCTUnwrap(node("port").props["navigationKey"])))
        session.holdFillForTests()
        p.scrollToFragment([visit, "moves", 2])
        waitForMainQueue("the fragment's retry while a fill is pending")
        XCTAssertNotNil(p.fragmentHeld)
        XCTAssertEqual(offset, chosen, accuracy: 1)
        let applied = session.appliedBatches
        session.publishFillForTests(try node("port").id, wireBatch([]), generation: session.generation)
        session.landFill()
        XCTAssertFalse(session.fillInFlight)
        XCTAssertEqual(session.appliedBatches, applied, "an unchanged fill skips the presenter apply pass")
        waitForMainQueue("the fill completion's queued work")
        XCTAssertEqual(gap("moves"), 0, accuracy: 1, "the fill completion resumes the fragment without another batch")
    }

    func testPersonCancelsThePictureHold() throws {
        try fixture()
        let p = session.presenter
        #if os(macOS)
        let port = try XCTUnwrap(node("port").scroll)
        port.contentView.scroll(to: CGPoint(x: 0, y: offset - 120))
        try node("port").liveScrollStarted(Notification(name: NSScrollView.willStartLiveScrollNotification, object: port))
        #else
        let port = try XCTUnwrap(node("port").scrollView)
        p.elementScrolls.userScrolled(port)
        port.setContentOffset(CGPoint(x: 0, y: offset - 120), animated: false)
        #endif
        try releasePicture()
        XCTAssertGreaterThan(abs(gap("moves")), 100, "the person keeps the place they chose")
    }

    func testNavigationWithoutAFragmentCancelsThePictureHold() throws {
        try fixture()
        XCTAssertTrue(session.navigate("/item/7"))
        let chosen = offset
        try releasePicture()
        XCTAssertEqual(offset, chosen, accuracy: 1)
    }

    func testANewerFragmentReplacesThePictureHold() throws {
        try fixture()
        XCTAssertTrue(session.navigate("/item/7#other"))
        XCTAssertEqual(gap("other"), 0, accuracy: 1)
        try releasePicture()
        XCTAssertEqual(gap("other"), 0, accuracy: 1)
    }

    func testAnActivationAlreadyTakenOrSupersededScrollsNothing() throws {
        try fixture()
        let p = session.presenter, held = try XCTUnwrap(p.fragmentHeld)
        let visit = try XCTUnwrap(Int(try XCTUnwrap(node("port").props["navigationKey"])))
        XCTAssertTrue(session.navigate("/item/7#other"))
        let current = try XCTUnwrap(p.fragmentHeld)
        XCTAssertFalse(current === held)
        XCTAssertGreaterThan(p.fragmentSeen, 1, "an older activation to replay")
        for activation in [p.fragmentSeen, p.fragmentSeen - 1] {
            p.scrollToFragment([visit, "moves", activation])
            XCTAssertEqual(gap("other"), 0, accuracy: 1, "activation \(activation): still at #other")
            XCTAssertTrue(p.fragmentHeld === current, "activation \(activation): the current hold stays")
        }
    }

    func testAuthoredScrollCancelsThePictureHold() throws {
        try fixture()
        let p = session.presenter
        p.scrollElementIntoView(["other", "start", "nearest", "instant"])
        let chosen = offset
        try releasePicture()
        XCTAssertEqual(offset, chosen, accuracy: 1)
    }

    func testAuthoredOffsetCancelsThePictureHold() throws {
        try fixture()
        session.presenter.apply(wireBatch([["op": "props", "id": try node("port").id, "set": ["scrollTop": "0"]]]))
        XCTAssertEqual(offset, 0, accuracy: 1)
        try releasePicture()
        XCTAssertEqual(offset, 0, accuracy: 1)
    }

    #if os(macOS)
    private func pageScrollCancelsThePictureHold(wheel: Bool) throws {
        try fixture()
        let p = session.presenter, page = p.viewport
        page.documentView?.setFrameSize(CGSize(width: 320, height: 1600))
        page.contentView.scroll(to: CGPoint(x: 0, y: 300))
        if wheel {
            let cg = try XCTUnwrap(CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1,
                                          wheel1: 80, wheel2: 0, wheel3: 0))
            page.scrollWheel(with: try XCTUnwrap(NSEvent(cgEvent: cg)))
        } else {
            NotificationCenter.default.post(name: NSScrollView.willStartLiveScrollNotification, object: page)
            page.contentView.scroll(to: CGPoint(x: 0, y: 200))
        }
        p.flushFragment()
        XCTAssertNil(p.fragmentHeld, "taking the page scroller cancels the fragment")
        try releasePicture()
        XCTAssertGreaterThan(abs(gap("moves")), 100, "the picture cannot pull the page back to the fragment")
    }

    func testPageWheelCancelsThePictureHold() throws {
        try pageScrollCancelsThePictureHold(wheel: true)
    }

    func testPageLiveScrollCancelsThePictureHold() throws {
        try pageScrollCancelsThePictureHold(wheel: false)
    }

    private func tab(back: Bool = false) -> Bool {
        let key = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: back ? .shift : [], timestamp: 0,
                                   windowNumber: window.windowNumber, context: nil, characters: "\t",
                                   charactersIgnoringModifiers: "\t", isARepeat: false, keyCode: 48)!
        return session.presenter.routeKey(key, focused: true, in: window)
    }

    func testTabBeginsAtTheFragmentTarget() throws {
        try fixture()
        let p = session.presenter
        XCTAssertTrue(tab())
        XCTAssertEqual(p.focusedNode?.props["id"], "after")
        XCTAssertTrue(session.navigate("/item/7#moves"))
        XCTAssertTrue(tab(back: true))
        XCTAssertEqual(p.focusedNode?.props["id"], "before")
    }

    func testAFragmentLeavesFocusOutsideItsSessionAlone() throws {
        outside = NSTextField(frame: CGRect(x: 0, y: 0, width: 100, height: 24))
        try fixture()
        XCTAssertTrue(window.makeFirstResponder(outside))
        let editor = try XCTUnwrap(outside?.currentEditor())
        XCTAssertTrue(session.navigate("/item/7#moves"))
        waitForMainQueue()
        XCTAssertTrue(window.firstResponder === editor, "the other owner's editing goes on")
    }

    func testANavigationFromABlurHandlerKeepsItsTabStart() throws {
        try fixture()
        let p = session.presenter
        let leaver = try node("leaver")
        XCTAssertTrue(window.makeFirstResponder(p.keyView(of: leaver)))
        XCTAssertTrue(p.focusedNode === leaver)
        XCTAssertTrue(session.navigate("/item/7#moves"))
        waitForMainQueue()
        XCTAssertEqual(gap("other"), 0, accuracy: 1, "the blur's navigation supersedes")
        XCTAssertTrue(tab())
        XCTAssertEqual(p.focusedNode?.props["id"], "before", "Tab goes on from the superseding fragment")
    }

    func testKeyboardScrollingCancelsThePictureHold() throws {
        try fixture()
        XCTAssertTrue(session.presenter.keyScroll("moveToBeginningOfDocument:", from: try node("moves")))
        XCTAssertEqual(offset, 0, accuracy: 1)
        try releasePicture()
        XCTAssertEqual(offset, 0, accuracy: 1)
    }
    #else
    func testFragmentResumesWhenItsTargetMountsAfterTheQueuedRetry() throws {
        let p = Presenter()
        p.viewport.frame = CGRect(x: 0, y: 0, width: 320, height: 400)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "props": ["navigationKey": "7"], "style": ["overflow_y": "scroll"]],
            ["op": "create", "id": 2, "kind": "view", "props": ["id": "moves"]],
            ["op": "create", "id": 3, "kind": "view"],
            ["op": "children", "id": 1, "ids": [2, 3]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 320, "h": 400],
            ["op": "frame", "id": 2, "x": 0, "y": 600, "w": 100, "h": 40],
            ["op": "frame", "id": 3, "x": 0, "y": 1000, "w": 100, "h": 400],
            ["op": "content", "id": 1, "w": 320, "h": 1400],
        ]))
        p.scrollToFragment([7, "moves", 1])
        waitForMainQueue("the unmounted fragment's retry")
        XCTAssertTrue(p.elementScrolls.names.isEmpty)
        XCTAssertNotNil(p.fragmentHeld)
        let scroll = try XCTUnwrap(p.views[1]?.scrollView)
        XCTAssertEqual(scroll.contentSize.height, 1400, accuracy: 1)
        XCTAssertEqual(scroll.contentOffset.y, 0, accuracy: 1)
        window = UIWindow(frame: p.viewport.frame)
        window.addSubview(p.viewport)
        XCTAssertEqual(scroll.contentOffset.y, 600, accuracy: 1, "mounting resumes the fragment without another batch")
        withExtendedLifetime(p) {}
    }

    func testAStatusBarTapCancelsThePictureHold() throws {
        try fixture()
        let port = try XCTUnwrap(node("port").scrollView)
        XCTAssertTrue(try node("port").scrollViewShouldScrollToTop(port))
        port.setContentOffset(CGPoint(x: 0, y: -port.adjustedContentInset.top), animated: false)
        let top = offset
        try releasePicture()
        XCTAssertEqual(offset, top, accuracy: 1, "the tap's scroll to the top stands")
    }

    /// A route projected from a navigation root that scrolls keeps the root's
    /// scrolling in its page viewport (`RouteController.fitContentViewport`).
    func testAFragmentScrollsThePageProjectedFromANavigationRoot() throws {
        session = ExactApp.shared.makeSession(label: "fragment-navigation-root")
        // The batches below stand in for the runner's: no plan boots over them.
        XCTAssertNotNil(session.boot(plan: Data(), size: CGSize(width: 200, height: 400)).error)
        let p = session.presenter, host = UIViewController(), view = ExactView(session: session)
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 200, height: 400))
        window.rootViewController = host
        window.isHidden = false
        host.view.addSubview(view)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        session.rasters.setPaused(true)
        p.apply(wireBatch([
            ["op": "create", "id": 9, "kind": "view", "props": ["navigationKey": "7", "navigationBack": "back"], "style": ["overflow_y": "auto", "overflow_x": "hidden"]],
            ["op": "create", "id": 1, "kind": "view", "props": ["navigationKey": "7"]],
            ["op": "create", "id": 2, "kind": "image", "props": ["imageSource": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGMIqFjQAAAEbQHpbOjxOAAAAABJRU5ErkJggg=="]],
            ["op": "create", "id": 3, "kind": "view", "props": ["id": "moves"]],
            ["op": "create", "id": 4, "kind": "view", "props": ["id": "east"]],
            ["op": "children", "id": 1, "ids": [2, 3, 4]],
            ["op": "children", "id": 9, "ids": [1]],
            ["op": "roots", "ids": [9]],
            ["op": "frame", "id": 9, "x": 0, "y": 0, "w": 200, "h": 400],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 800, "h": 1600],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 100, "h": 10],
            ["op": "frame", "id": 3, "x": 0, "y": 700, "w": 100, "h": 50],
            ["op": "frame", "id": 4, "x": 600, "y": 1000, "w": 100, "h": 50],
            ["op": "content", "id": 9, "w": 200, "h": 1600],
            ["op": "content", "id": 1, "w": 800, "h": 1600],
        ]))
        window.layoutIfNeeded()
        p.navigation.sync(wireBatch([]))
        let route = try XCTUnwrap(p.navigation.activeRoute)
        let target = try XCTUnwrap(p.views[3])
        XCTAssertEqual(p.keyboardPolicy, "resizes-content")
        let scroll = try XCTUnwrap(route.contentViewport, "the projected route keeps the root's scrolling")
        XCTAssertFalse(scroll.scrollsX)
        XCTAssertTrue(scroll.scrollsY)
        XCTAssertEqual(scroll.contentSize.width, 800, accuracy: 0.5, "hidden overflow still scrolls programmatically")
        scroll.layoutIfNeeded()
        XCTAssertFalse(scroll.showsHorizontalScrollIndicator, "nor does it show the person a horizontal range")
        p.apply(wireBatch([["op": "style", "id": 9, "style": ["overflow_x": "auto", "overflow_y": "hidden"]]]))
        XCTAssertTrue(scroll.scrollsX)
        XCTAssertFalse(scroll.scrollsY)
        XCTAssertEqual(scroll.contentSize.height, 1600, accuracy: 0.5)
        p.apply(wireBatch([["op": "style", "id": 9, "style": ["overflow_x": "hidden", "overflow_y": "auto"]]]))

        // CSS `overflow: hidden` permits a programmatic scroll on that axis.
        p.armFragment(1)
        p.scrollToFragment([7, "east", 1])
        let east = try XCTUnwrap(p.views[4]).convert(try XCTUnwrap(p.views[4]).bounds, to: scroll)
        XCTAssertTrue(scroll.bounds.contains(east), "the fragment reveals a target past the hidden axis: \(east) in \(scroll.bounds)")
        // The person's drag moves only the axis the root scrolls.
        let start = scroll.contentOffset
        route.scrollViewWillBeginDragging(scroll)
        scroll.contentOffset = CGPoint(x: start.x - 300, y: start.y - 200)
        route.scrollViewDidEndDragging(scroll, willDecelerate: false)
        XCTAssertEqual(scroll.contentOffset.x, start.x, accuracy: 0.5, "a drag cannot pan the hidden axis")
        XCTAssertEqual(scroll.contentOffset.y, start.y - 200, accuracy: 0.5)

        route.additionalSafeAreaInsets.top = 24
        route.view.layoutIfNeeded()
        let clearance = scroll.safeAreaInsets.top
        XCTAssertGreaterThan(clearance, 0, "native chrome covers the page's top")
        p.armFragment(2)
        p.scrollToFragment([7, "moves", 2])
        XCTAssertEqual(scroll.contentOffset.y, 700 - clearance, accuracy: 0.5)
        XCTAssertTrue(session.rasters.loading(inside: route.node))
        XCTAssertEqual(target.convert(target.bounds, to: scroll).minY, scroll.bounds.minY + clearance, accuracy: 0.5)
        p.apply(wireBatch([
            ["op": "style", "id": 3, "style": ["margin_top": 900]],
            ["op": "frame", "id": 3, "x": 0, "y": 900, "w": 100, "h": 50],
        ]))
        XCTAssertTrue(route.contentViewport === scroll)
        XCTAssertEqual(scroll.contentOffset.y, 900 - clearance, accuracy: 0.5, "the page holds alignment while its picture loads")
        p.apply(wireBatch([["op": "style", "id": 1, "style": ["safe_area": 1]]]))
        XCTAssertEqual(scroll.contentOffset.y, 900, accuracy: 0.5, "authored clearance can align under the chrome")
        p.apply(wireBatch([["op": "style", "id": 1, "style": ["safe_area": 0]]]))
        XCTAssertEqual(scroll.contentOffset.y, 900 - clearance, accuracy: 0.5)
        XCTAssertTrue(route.scrollViewShouldScrollToTop(scroll))
        scroll.setContentOffset(.zero, animated: false)
        p.flushFragment()
        XCTAssertEqual(scroll.contentOffset.y, 0, accuracy: 0.5, "the page's status-bar scroll ends alignment")
    }
    #endif
}
