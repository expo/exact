#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// Regression tests for #2055: a row mounted above a pushed route's `navigationScroll`
/// while UIKit's push animation is still running must lay out under the navigation bar
/// from its first frame, never at y = 0 under the status bar. Also covers flat-leaf
/// transparency in top/bottom scroll-edge selection (`collapse`, `topScroller`, `bottomScroller`).
final class PushedNoticeIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?

    override func tearDown() {
        session?.destroy()
        session = nil
        window?.isHidden = true
        window = nil
        UIView.setAnimationsEnabled(true)
        super.tearDown()
    }

    private func until(_ what: String, _ seconds: Double = 8, file: StaticString = #filePath, line: UInt = #line, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01)) }
        XCTAssertTrue(done(), what, file: file, line: line)
    }

    private func settle() {
        let turned = expectation(description: "main queue turn")
        DispatchQueue.main.async { turned.fulfill() }
        wait(for: [turned], timeout: 5)
        window?.layoutIfNeeded()
    }

    private func node(_ testId: String) -> NodeView? {
        session?.presenter.views.values.first { $0.props["testId"] == testId }
    }

    private func fixture(_ scene: UIWindowScene) throws -> (ExactSession, UINavigationController) {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_PUSHED_NOTICE_PLAN"], "build.mjs --test --ios compiles pushed-notice.contract")
        let session = ExactApp.shared.makeSession(label: "pushed-notice")
        self.session = session
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: CGSize(width: 402, height: 874)).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let nav = try XCTUnwrap(tabs.selectedViewController as? UINavigationController)
        until("start route settled") {
            nav.viewControllers.count == 1 && nav.transitionCoordinator == nil && !session.presenter.navigationHost.transitioning
                && nav.topViewController?.viewIfLoaded?.window != nil
        }
        settle()
        return (session, nav)
    }

    private func presentedBarBottom(of nav: UINavigationController) -> CGFloat {
        let barLayer = nav.navigationBar.layer.presentation() ?? nav.navigationBar.layer
        return barLayer.convert(barLayer.bounds, to: nil).maxY
    }

    private func press(_ session: ExactSession, _ testId: String) throws {
        let target = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "missing \(testId)")
        session.presenter.press(target.id)
    }

    private func largeTitleShows(_ title: String) -> Bool {
        guard let window else { return false }
        var found = false
        func visit(_ view: UIView, _ alpha: CGFloat, _ clip: CGRect) {
            guard !view.isHidden, !found else { return }
            let a = alpha * view.alpha
            let inside = view.clipsToBounds ? clip.intersection(view.convert(view.bounds, to: window)) : clip
            if let label = view as? UILabel, label.text == title, label.font.pointSize >= 26, a > 0.5 {
                let frame = label.convert(label.bounds, to: window), shown = frame.intersection(inside)
                if frame.height > 1, !shown.isNull, shown.height >= frame.height / 2 { found = true }
            }
            for sub in view.subviews { visit(sub, a, inside) }
        }
        visit(window, 1, window.bounds)
        return found
    }

    /// Reproduces #2055: Detail is pushed with `ready == false` (`detail-list` collapses under the bar),
    /// then `ready` flips to `true` while UIKit's animated push is still running (`changing == true`).
    /// On every sampled mid-push frame once the notice is mounted, its presentation layer must sit at
    /// the navigation bar's bottom (never at y = 0 under the status bar), with the list directly beneath it.
    func testANoticeMountedDuringAnAnimatedPushClearsTheBarFromItsFirstFrame() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "animated push transitions require a hosted window scene")
        }
        let (session, nav) = try fixture(scene)
        let navigation = session.presenter.navigationHost

        var revealedDuringPush = false
        var collapsedBeforeReveal: Bool?
        var pressFrameNoticeY: CGFloat?
        var pressFrameBarBottom: CGFloat?
        var departingCoverBeforeReveal: HostCover?
        var departingCoverAfterReveal: HostCover?
        var missedFirstNoticeFrame = false
        var preRevealFrames = 0
        var samples: [(frame: Int, noticeY: CGFloat, noticeMaxY: CGFloat, listY: CGFloat?, barBottom: CGFloat)] = []
        var frameCount = 0
        let startRouteID = try XCTUnwrap((nav.viewControllers.first as? RouteController)?.node.id)

        let sampler = FrameTicker { [weak self] in
            guard let self, let navTop = nav.topViewController as? RouteController,
                  navTop.node.props["testId"] == "route-detail",
                  let transition = nav.transitionCoordinator,
                  transition.viewController(forKey: .to) === navTop,
                  navTop.view.window != nil else { return }
            frameCount += 1
            let barBottom = self.presentedBarBottom(of: nav)
            if !revealedDuringPush {
                preRevealFrames += 1
                // Match `SessionClock` / `NativeTaskTimers`, which wait for `!ownerInFlight` so cover
                // updates apply synchronously inside the same `CATransaction` rather than via `coversAsync`.
                guard !session.ownerInFlight else { return }
                collapsedBeforeReveal = navTop.collapseScroll === self.node("detail-list")
                departingCoverBeforeReveal = navigation.covers[startRouteID]
                revealedDuringPush = true
                if let revealBtn = self.node("reveal-from-start") {
                    session.presenter.press(revealBtn.id)
                    self.window?.layoutIfNeeded()
                    departingCoverAfterReveal = navigation.covers[startRouteID]
                    if let notice = self.node("notice"), notice.window != nil {
                        pressFrameNoticeY = notice.convert(notice.bounds, to: nil).minY
                        pressFrameBarBottom = navTop.view.safeAreaInsets.top
                    }
                }
                return
            }
            guard let notice = self.node("notice"), notice.window != nil else { return }
            guard let noticeLayer = notice.layer.presentation(),
                  noticeLayer.superlayer?.model() === notice.layer.superlayer else {
                if samples.isEmpty { missedFirstNoticeFrame = true }
                return
            }
            let noticeOrigin = noticeLayer.convert(CGPoint.zero, to: nil)
            let listY: CGFloat? = self.node("detail-list").flatMap { list in
                guard list.window != nil, let listLayer = list.layer.presentation(),
                      listLayer.superlayer?.model() === list.layer.superlayer else { return nil }
                return listLayer.convert(CGPoint.zero, to: nil).y
            }
            samples.append((frameCount, noticeOrigin.y, noticeOrigin.y + noticeLayer.bounds.height, listY, barBottom))
        }
        let link = CADisplayLink(target: sampler, selector: #selector(FrameTicker.tick))
        link.add(to: .main, forMode: .common)
        defer { link.invalidate() }

        try press(session, "open-detail")
        until("detail push started") { nav.viewControllers.count == 2 }
        XCTAssertNotNil(nav.transitionCoordinator, "push is animated")
        XCTAssertTrue(navigation.changing, "navigation reports transition in progress")
        let detailRoute = try XCTUnwrap(nav.topViewController as? RouteController)

        until("animated push completes") {
            nav.viewControllers.count == 2 && nav.transitionCoordinator == nil && !navigation.transitioning
        }
        link.invalidate()
        settle()

        XCTAssertTrue(revealedDuringPush, "notice mounted while push was still animating")
        XCTAssertEqual(collapsedBeforeReveal, true, "detail-list started as collapseScroll immediately before reveal")
        XCTAssertEqual(departingCoverAfterReveal, departingCoverBeforeReveal, "departing route keeps its settled cover across mid-push refresh")
        let committedY = try XCTUnwrap(pressFrameNoticeY, "notice mounted and laid out on the press frame")
        let committedBarBottom = try XCTUnwrap(pressFrameBarBottom)
        XCTAssertEqual(committedY, committedBarBottom, accuracy: 1.0,
                       "press frame: notice laid out at y=\(committedY) instead of barBottom=\(committedBarBottom)")
        XCTAssertFalse(missedFirstNoticeFrame, "first mounted notice frame had an inspectable presentation layer")
        XCTAssertGreaterThan(preRevealFrames, 0, "observed push frame before reveal")
        XCTAssertFalse(samples.isEmpty, "sampled at least one mid-push frame with notice visible")

        let settledBarBottom = nav.navigationBar.convert(nav.navigationBar.bounds, to: nil).maxY
        let settledNotice = try XCTUnwrap(node("notice"))
        let settledNoticeY = settledNotice.convert(settledNotice.bounds, to: nil).minY
        XCTAssertEqual(settledNoticeY, settledBarBottom, accuracy: 1.0, "settled notice sits below bar")

        for s in samples {
            XCTAssertEqual(s.noticeY, s.barBottom, accuracy: 1.0,
                           "mid-push frame \(s.frame): notice drawn at y=\(s.noticeY) instead of barBottom=\(s.barBottom)")
            if let listY = s.listY {
                XCTAssertEqual(listY, s.noticeMaxY, accuracy: 1.0,
                               "mid-push frame \(s.frame): list drawn at y=\(listY) instead of noticeMaxY=\(s.noticeMaxY)")
            }
        }
        XCTAssertNil(detailRoute.collapseScroll, "collapseScroll cleared once notice sits above list")
    }

    /// Exercises the exact Contract `after(80, reveal)` timer path from #2055 (async posted timer -> `landFill`).
    func testATimedNoticeMountedDuringAnAnimatedPushClearsTheBar() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "animated push transitions require a hosted window scene")
        }
        let (session, nav) = try fixture(scene)
        let navigation = session.presenter.navigationHost

        var samples: [(frame: Int, noticeY: CGFloat, barBottom: CGFloat)] = []
        var frameCount = 0
        let sampler = FrameTicker { [weak self] in
            guard let self, let navTop = nav.topViewController as? RouteController,
                  navTop.node.props["testId"] == "route-timed",
                  nav.transitionCoordinator != nil,
                  let notice = self.node("timed-notice"), notice.window != nil,
                  let layer = notice.layer.presentation(),
                  layer.superlayer?.model() === notice.layer.superlayer else { return }
            frameCount += 1
            let barBottom = self.presentedBarBottom(of: nav)
            samples.append((frameCount, layer.convert(CGPoint.zero, to: nil).y, barBottom))
        }
        let link = CADisplayLink(target: sampler, selector: #selector(FrameTicker.tick))
        link.add(to: .main, forMode: .common)
        defer { link.invalidate() }

        try press(session, "open-timed")
        until("timed push started") { nav.viewControllers.count == 2 }
        XCTAssertNotNil(nav.transitionCoordinator)

        until("timed push completes and notice renders") {
            nav.viewControllers.count == 2 && nav.transitionCoordinator == nil && !navigation.transitioning
                && self.node("timed-notice")?.window != nil
        }
        link.invalidate()
        settle()

        XCTAssertFalse(samples.isEmpty, "timed notice appeared while push was still in flight")
        for s in samples {
            XCTAssertEqual(s.noticeY, s.barBottom, accuracy: 1.0,
                           "timed mid-push frame \(s.frame): notice drawn at y=\(s.noticeY), expected barBottom=\(s.barBottom)")
        }
    }

    /// Mounting an empty `column flex-shrink=0` above the list before the push and inserting the notice
    /// inside it mid-push (promoting the flat container to a NodeView) keeps the notice under the bar.
    func testANoticeMountedInsideAPremountedEmptyContainerClearsTheBarDuringPush() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "animated push transitions require a hosted window scene")
        }
        let (session, nav) = try fixture(scene)
        let navigation = session.presenter.navigationHost

        var revealed = false
        var samples: [(frame: Int, noticeY: CGFloat, barBottom: CGFloat)] = []
        var frameCount = 0
        let sampler = FrameTicker { [weak self] in
            guard let self, let navTop = nav.topViewController as? RouteController,
                  navTop.node.props["testId"] == "route-slot",
                  nav.transitionCoordinator != nil, navTop.view.window != nil else { return }
            frameCount += 1
            if !revealed {
                revealed = true
                if let btn = self.node("reveal-from-start") { session.presenter.press(btn.id) }
                return
            }
            guard let notice = self.node("slot-notice"), notice.window != nil,
                  let layer = notice.layer.presentation(),
                  layer.superlayer?.model() === notice.layer.superlayer else { return }
            let barBottom = self.presentedBarBottom(of: nav)
            samples.append((frameCount, layer.convert(CGPoint.zero, to: nil).y, barBottom))
        }
        let link = CADisplayLink(target: sampler, selector: #selector(FrameTicker.tick))
        link.add(to: .main, forMode: .common)
        defer { link.invalidate() }

        try press(session, "open-slot")
        until("slot push started") { nav.viewControllers.count == 2 }
        XCTAssertNotNil(nav.transitionCoordinator)

        until("slot push completes") {
            nav.viewControllers.count == 2 && nav.transitionCoordinator == nil && !navigation.transitioning
        }
        link.invalidate()
        settle()

        XCTAssertTrue(revealed)
        XCTAssertFalse(samples.isEmpty, "sampled slot-notice frames during push")
        for s in samples {
            XCTAssertEqual(s.noticeY, s.barBottom, accuracy: 1.0,
                           "slot mid-push frame \(s.frame): notice drawn at y=\(s.noticeY), expected barBottom=\(s.barBottom)")
        }
    }

    /// A permanent flat-leaf banner (`box height=24 background-color="#ffd9b3"`, no props/handlers)
    /// above `navigationScroll` stays a flat leaf, prevents top collapse, and sits under the bar.
    func testAPermanentFlatLeafAboveNavigationScrollPreventsCollapseAndClearsTheBar() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "navigation bar geometry requires a hosted window scene")
        }
        let (session, nav) = try fixture(scene)
        let navigation = session.presenter.navigationHost

        try press(session, "open-flat")
        until("flat route push completes") {
            nav.viewControllers.count == 2 && nav.transitionCoordinator == nil && !navigation.transitioning
        }
        settle()

        let route = try XCTUnwrap(nav.topViewController as? RouteController)
        let list = try XCTUnwrap(node("flat-list"))
        let barBottom = nav.navigationBar.convert(nav.navigationBar.bounds, to: nil).maxY
        let flatChildIDs = session.presenter.flats.children(of: route.node.id) ?? []
        let flatLeaves = flatChildIDs.compactMap { session.presenter.flats.leaves[$0] }
        XCTAssertEqual(flatLeaves.count, 1, "banner remains a FlatLeaf directly under the route")
        let banner = try XCTUnwrap(flatLeaves.first)
        XCTAssertNil(route.collapseScroll, "permanent flat leaf above navigationScroll prevents top collapse")
        let leafY = route.node.convert(banner.frame, to: nil).minY
        XCTAssertEqual(leafY, barBottom, accuracy: 1.0, "flat leaf draws below the bar, not at y=0")
        XCTAssertEqual(list.convert(list.bounds, to: nil).minY, barBottom + 24, accuracy: 1.0,
                       "list sits directly below the 24pt flat leaf")
    }

    /// Level-1 large-title route where a notice mounts mid-push: notice clears the bar on every frame
    /// and the large title remains visible after the push settles (LLP 1116.005 D4).
    func testALargeTitleRouteNoticeMountedDuringPushClearsBarAndKeepsLargeTitleVisible() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "large-title push transitions require a hosted window scene")
        }
        let (session, nav) = try fixture(scene)
        let navigation = session.presenter.navigationHost

        var revealed = false
        var samples: [(frame: Int, noticeY: CGFloat, barBottom: CGFloat)] = []
        var frameCount = 0
        let sampler = FrameTicker { [weak self] in
            guard let self, let navTop = nav.topViewController as? RouteController,
                  navTop.node.props["testId"] == "route-large",
                  nav.transitionCoordinator != nil, navTop.view.window != nil else { return }
            frameCount += 1
            if !revealed {
                revealed = true
                if let btn = self.node("reveal-from-start") { session.presenter.press(btn.id) }
                return
            }
            guard let notice = self.node("large-notice"), notice.window != nil,
                  let layer = notice.layer.presentation(),
                  layer.superlayer?.model() === notice.layer.superlayer else { return }
            let barBottom = self.presentedBarBottom(of: nav)
            samples.append((frameCount, layer.convert(CGPoint.zero, to: nil).y, barBottom))
        }
        let link = CADisplayLink(target: sampler, selector: #selector(FrameTicker.tick))
        link.add(to: .main, forMode: .common)
        defer { link.invalidate() }

        try press(session, "open-large")
        until("large-title push started") { nav.viewControllers.count == 2 }
        XCTAssertNotNil(nav.transitionCoordinator)

        until("large-title push completes") {
            nav.viewControllers.count == 2 && nav.transitionCoordinator == nil && !navigation.transitioning
        }
        link.invalidate()
        settle()

        XCTAssertTrue(revealed)
        XCTAssertFalse(samples.isEmpty, "sampled large-notice frames during push")
        for s in samples {
            XCTAssertEqual(s.noticeY, s.barBottom, accuracy: 1.0,
                           "large-title mid-push frame \(s.frame): notice drawn at y=\(s.noticeY), expected barBottom=\(s.barBottom)")
        }
        XCTAssertTrue(largeTitleShows("Large"), "large title remains visible after notice clears collapseScroll")
    }

    /// Flat-leaf transparency unit tests for `topScroller` and `bottomScroller` (no window scene needed).
    func testTopAndBottomScrollerRespectFlatLeafSiblings() throws {
        let p = Presenter()
        // Positive control: scroller alone inside neutral wrapper resolves for both topScroller and bottomScroller.
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 3, "kind": "view", "style": ["overflow_y": "scroll"]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 600.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 400.0, "h": 600.0],
            ["op": "children", "id": 1, "ids": [3]],
            ["op": "roots", "ids": [1]],
        ]))
        let wrapper1 = try XCTUnwrap(p.views[1])
        let scroller1 = try XCTUnwrap(p.views[3])
        XCTAssertNotNil(scroller1.scrollView, "scroller must have a physical UIScrollView")
        XCTAssertIdentical(NavigationHost.topScroller(wrapper1, target: scroller1), scroller1,
                           "control: scroller alone resolves via topScroller")
        XCTAssertIdentical(NavigationHost.bottomScroller(in: wrapper1, scale: 3), scroller1,
                           "control: scroller alone resolves via bottomScroller")

        // Out-of-flow siblings (`display: none` and `position: absolute`) before scroller do not block
        // topScroller or bottomScroller; a nested absolute FlatLeaf also reports `isInFlow == false`.
        p.apply(wireBatch([
            ["op": "create", "id": 5, "kind": "view", "style": ["background_color": [255, 217, 179, 255], "display": "none"]],
            ["op": "create", "id": 6, "kind": "view", "style": ["background_color": [255, 217, 179, 255], "position_type": "absolute"]],
            ["op": "frame", "id": 5, "x": 0.0, "y": 0.0, "w": 400.0, "h": 24.0],
            ["op": "frame", "id": 6, "x": 0.0, "y": 0.0, "w": 400.0, "h": 24.0],
            ["op": "children", "id": 1, "ids": [5, 6, 3]],
        ]))
        XCTAssertIdentical(NavigationHost.topScroller(wrapper1, target: scroller1), scroller1,
                           "out-of-flow siblings are ignored by topScroller")
        XCTAssertIdentical(NavigationHost.bottomScroller(in: wrapper1, scale: 3), scroller1,
                           "out-of-flow siblings are ignored by bottomScroller")
        p.apply(wireBatch([
            ["op": "create", "id": 7, "kind": "view", "style": ["background_color": [255, 217, 179, 255]]],
            ["op": "create", "id": 8, "kind": "view", "style": ["background_color": [255, 217, 179, 255], "position_type": "absolute"]],
            ["op": "frame", "id": 7, "x": 0.0, "y": 0.0, "w": 400.0, "h": 24.0],
            ["op": "frame", "id": 8, "x": 0.0, "y": 0.0, "w": 40.0, "h": 24.0],
            ["op": "children", "id": 7, "ids": [8]],
            ["op": "children", "id": 1, "ids": [7, 3]],
        ]))
        XCTAssertTrue(p.flats.isFlat(7), "flat box 7 stays a FlatLeaf")
        XCTAssertTrue(p.flats.isFlat(8), "nested absolute child 8 stays a FlatLeaf")
        XCTAssertEqual(p.flats.leaves[7]?.isInFlow, true)
        XCTAssertEqual(p.flats.leaves[8]?.isInFlow, false)

        // Case 1: in-flow flat leaf above scroller inside a neutral wrapper -> topScroller is nil.
        p.apply(wireBatch([
            ["op": "create", "id": 2, "kind": "view", "style": ["background_color": [255, 217, 179, 255]]],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 400.0, "h": 24.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 24.0, "w": 400.0, "h": 576.0],
            ["op": "children", "id": 1, "ids": [2, 3]],
        ]))
        XCTAssertTrue(p.flats.isFlat(2), "node 2 stays a FlatLeaf")
        XCTAssertEqual(p.flats.leaves[2]?.isInFlow, true)
        XCTAssertNil(NavigationHost.topScroller(wrapper1, target: scroller1),
                     "a flat leaf before a scroller prevents topScroller")

        // Case 2: flat leaf beside scroller in a row (before scroller so bottomScroller reaches its
        // sibling-overlap check, and after scroller so topScroller reaches its sibling-overlap check).
        p.apply(wireBatch([
            ["op": "frame", "id": 2, "x": 200.0, "y": 0.0, "w": 200.0, "h": 600.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 200.0, "h": 600.0],
            ["op": "children", "id": 1, "ids": [3, 2]],
        ]))
        XCTAssertTrue(p.flats.isFlat(2), "node 2 stays flat beside scroller")
        XCTAssertNil(NavigationHost.topScroller(wrapper1, target: scroller1),
                     "a flat leaf beside a scroller prevents topScroller")
        p.apply(wireBatch([
            ["op": "children", "id": 1, "ids": [2, 3]],
        ]))
        XCTAssertNil(NavigationHost.bottomScroller(in: wrapper1, scale: 3),
                     "a flat leaf before and overlapping a scroller prevents bottomScroller")

        // Case 3 & 4: trailing flat leaf (34pt and 0pt) matches a NodeView footer twin.
        p.apply(wireBatch([
            ["op": "create", "id": 4, "kind": "view", "props": ["testId": "view-footer"], "style": ["background_color": [255, 217, 179, 255]]],
        ]))
        XCTAssertFalse(p.flats.isFlat(4), "node 4 is a real NodeView twin")
        for (height, cover) in [(34.0, CGFloat(34)), (0.0, CGFloat(0))] {
            p.apply(wireBatch([
                ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 400.0, "h": 600.0 - height],
                ["op": "frame", "id": 2, "x": 0.0, "y": 600.0 - height, "w": 400.0, "h": height],
                ["op": "children", "id": 1, "ids": [3, 2]],
            ]))
            XCTAssertTrue(p.flats.isFlat(2))
            let leafResult = NavigationHost.bottomScroller(in: wrapper1, cover: cover, scale: 3)

            p.apply(wireBatch([
                ["op": "frame", "id": 4, "x": 0.0, "y": 600.0 - height, "w": 400.0, "h": height],
                ["op": "children", "id": 1, "ids": [3, 4]],
            ]))
            let viewResult = NavigationHost.bottomScroller(in: wrapper1, cover: cover, scale: 3)
            XCTAssertNil(leafResult, "trailing flat leaf (h=\(height)) prevents bottomScroller")
            XCTAssertNil(viewResult, "trailing NodeView twin (h=\(height)) prevents bottomScroller")
        }
    }
}

private final class FrameTicker: NSObject {
    private let body: () -> Void
    init(_ body: @escaping () -> Void) { self.body = body }
    @objc func tick() { body() }
}
#endif
