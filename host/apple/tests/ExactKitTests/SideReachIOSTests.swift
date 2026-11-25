#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactCollections
@testable import ExactNavigation

/// LLP 1188 D2: before activation, a route's vertical scroller that meets its route's side clearance reaches across it,
/// its content and its CSS scroll position where they were. An iPhone Duo's bands are at its right; an iPhone has them
/// turned.
final class SideReachIOSTests: XCTestCase {
    private var session: ExactSession?
    private var window: UIWindow?
    /// A test's own presenter, which its nodes hold weakly.
    private var presenter: Presenter?

    override func tearDown() {
        session?.destroy()
        session = nil
        presenter = nil
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    func testARoutesScrollerReachesAcrossItsSideClearance() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a route's side clearance is a scene window's safe area") }
        let (band, route) = try booted(scene)
        let node = try node("scroll"), scroll = try XCTUnwrap(node.scroll)
        let reached = scroll.convert(scroll.bounds, to: route)
        XCTAssertEqual([reached.minX, reached.maxX], [0, route.bounds.width])
        // Its box, its content and its scrollport are the route's content's
        XCTAssertEqual(node.convert(node.bounds, to: route).minX, band.left, accuracy: 0.5)
        XCTAssertEqual(node.convert(node.bounds, to: route).maxX, route.bounds.width - band.right, accuracy: 0.5)
        let rows = try self.node("rows"), shown = rows.convert(rows.bounds, to: node)
        XCTAssertEqual([shown.minX, shown.maxX], [0, node.bounds.width])
        XCTAssertEqual(scroll.adjustedContentInset.left, band.left, accuracy: 0.5)
        XCTAssertEqual(scroll.adjustedContentInset.right, band.right, accuracy: 0.5)
        XCTAssertEqual(scroll.contentSize.width + scroll.adjustedContentInset.left + scroll.adjustedContentInset.right,
                       scroll.bounds.width, accuracy: 0.5, "no sideways range")
        XCTAssertEqual(scroll.contentOffset.x + node.scrollLeftInset(scroll), 0, accuracy: 0.5, "CSS scrollLeft")
        XCTAssertEqual(scroll.horizontalScrollIndicatorInsets.left, band.left, accuracy: 0.5)
        XCTAssertEqual(scroll.horizontalScrollIndicatorInsets.right, band.right, accuracy: 0.5)
        XCTAssertEqual(RouteGeometry.ownedChanges(scroll, of: node, insetByUIKit: scroll.contentInsetAdjustmentBehavior != .never), [], "Exact's own")
        // The kernel and a `scroll` event hear CSS's position
        var heard: [Double] = []
        let onScrolled = session?.presenter.onScrolled
        session?.presenter.onScrolled = { heard = [$1, $2]; onScrolled?($0, $1, $2) }
        scroll.contentOffset.y += 40
        XCTAssertEqual(heard.first ?? -1, 0, accuracy: 0.5, "the kernel's x")
        // A touch in the strip scrolls
        let strip = CGPoint(x: band.right > 0 ? node.bounds.width + band.right / 2 : -band.left / 2, y: node.bounds.midY)
        let hit = node.hitTest(strip, with: nil)
        XCTAssertTrue(hit === scroll || hit?.isDescendant(of: scroll) == true, "\(String(describing: hit))")
        // The indicator lies where a hand-built scroll view's does
        let own = UIScrollView(frame: reached)
        own.contentInsetAdjustmentBehavior = .never
        own.contentSize = CGSize(width: scroll.contentSize.width, height: 3000)
        route.addSubview(own)
        defer { own.removeFromSuperview() }
        XCTAssertEqual(try vertical(scroll, in: route).minX, try vertical(own, in: route).minX, accuracy: 0.5)
    }

    func testAListsRowsKeepTheBoxesWidth() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a route's side clearance is a scene window's safe area") }
        let (band, route) = try booted(scene)
        try show("list")
        let node = try node("scroll"), scroll = try XCTUnwrap(node.scroll)
        waitFor("rows") { (try? self.node("row-0")) != nil }
        XCTAssertEqual(scroll.convert(scroll.bounds, to: route).minX, 0, accuracy: 0.5)
        XCTAssertEqual(scroll.convert(scroll.bounds, to: route).width, route.bounds.width, accuracy: 0.5)
        let row = try self.node("row-0")
        XCTAssertEqual(row.convert(row.bounds, to: route).minX, band.left, accuracy: 0.5)
        XCTAssertEqual(row.bounds.width, route.bounds.width - band.left - band.right, accuracy: 0.5)
        XCTAssertEqual(session?.presenter.listHost.geometry(node.id)?.portCross ?? 0, Double(node.bounds.width), accuracy: 0.5)
    }

    func testWhatKeepsAScrollerInItsBox() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a route's side clearance is a scene window's safe area") }
        _ = try booted(scene)
        for shape in ["opted", "padded", "clipped", "moved", "material", "wide", "hidden", "round", "bordered", "held"] {
            try show(shape)
            let node = try node("scroll"), scroll = try XCTUnwrap(node.scroll)
            waitFor("\(shape): in its box") { scroll.frame.width == node.bounds.width && scroll.frame.minX == 0 }
            XCTAssertEqual([scroll.contentInset.left, scroll.contentInset.right], [0, 0], shape)
            XCTAssertEqual(scroll.horizontalScrollIndicatorInsets.right, 0, shape)
            XCTAssertEqual(scroll.contentOffset.x, 0, accuracy: 0.5, shape)
            try show("plain")
            waitFor("plain after \(shape): reaching") { scroll.frame.width > node.bounds.width }
        }
    }

    func testTheScrollPositionIsKeptAsTheReachGoesAndComes() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a route's side clearance is a scene window's safe area") }
        let (band, _) = try booted(scene)
        let node = try node("scroll"), scroll = try XCTUnwrap(node.scroll)
        scroll.contentOffset.y = 300 - node.scrollTopInset(scroll)
        for shape in ["opted", "plain", "padded", "plain"] {
            try show(shape)
            let reach = shape == "plain" ? band.left + band.right : 0
            waitFor(shape) { abs(scroll.frame.width - node.bounds.width - reach) < 0.5 }
            XCTAssertEqual(scroll.contentOffset.y + node.scrollTopInset(scroll), 300, accuracy: 0.5, shape)
            XCTAssertEqual(scroll.contentOffset.x + node.scrollLeftInset(scroll), 0, accuracy: 0.5, shape)
        }
    }

    func testTheScrollViewItsInsetsAndItsOffsetMoveAsOne() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a scroll view's insets are a window's") }
        // A scroller UIKit does not inset, under a page band, its other insets a keyboard's and a border's
        let window = UIWindow(windowScene: scene), p = Presenter()
        window.frame = scene.coordinateSpace.bounds
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        self.window = window
        self.presenter = p
        window.rootViewController?.view.addSubview(p.viewport)
        p.viewport.frame = window.bounds
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["overflow_y": "scroll", "overflow_x": "auto"]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 40, "y": 200, "w": 300, "h": 400],
            ["op": "content", "id": 1, "w": 300, "h": 2000],
        ]))
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .never)
        node.setPageBand(UIEdgeInsets(top: 20, left: 0, bottom: 0, right: 0))
        scroll.contentInset.bottom += 50
        scroll.horizontalScrollIndicatorInsets = UIEdgeInsets(top: 0, left: 3, bottom: 0, right: 9)
        scroll.contentOffset.y = 300 - node.scrollTopInset(scroll)
        var heard: [[Double]] = []
        p.onScrolled = { heard.append([$1, $2]) }
        node.handlers = ["scroll"]
        node.sendScrollEvent()
        let event = node.lastScrollEvent, before = (scroll.frame, scroll.contentInset, scroll.horizontalScrollIndicatorInsets, scroll.verticalScrollIndicatorInsets)
        let sides = UIEdgeInsets(top: 0, left: 30, bottom: 0, right: 20)
        node.setSideReach(sides)
        XCTAssertEqual(scroll.frame, before.0.inset(by: UIEdgeInsets(top: 0, left: -30, bottom: 0, right: -20)))
        XCTAssertEqual(scroll.contentInset, before.1.adding(sides))
        XCTAssertEqual(scroll.horizontalScrollIndicatorInsets, before.2.adding(sides))
        XCTAssertEqual(scroll.verticalScrollIndicatorInsets, before.3)
        XCTAssertEqual(scroll.contentOffset, CGPoint(x: -30, y: 280))
        XCTAssertEqual([node.scrollLeftInset(scroll), node.scrollTopInset(scroll)], [30, 20])
        XCTAssertEqual(scroll.contentSize.width, 300)
        node.scrollViewDidScroll(scroll)
        XCTAssertEqual(Set(heard), [[0, 300]], "the kernel's position, at each step")
        node.sendScrollEvent()
        XCTAssertEqual(node.lastScrollEvent, event, "CSS's position and extents")
        node.setSideReach(.zero)
        XCTAssertEqual(scroll.frame, before.0)
        XCTAssertEqual(scroll.contentInset, before.1)
        XCTAssertEqual(scroll.horizontalScrollIndicatorInsets, before.2)
        XCTAssertEqual(scroll.contentOffset, CGPoint(x: 0, y: 280))
        // The kernel's reach takes its place, the writer's insets not here
        node.setSideReach(sides)
        node.setReach(UIEdgeInsets(top: 0, left: 10, bottom: 0, right: 0))
        XCTAssertEqual(scroll.contentInset, before.1)
        XCTAssertEqual(scroll.frame, before.0.inset(by: UIEdgeInsets(top: 0, left: -10, bottom: 0, right: 0)))
        XCTAssertEqual(scroll.contentOffset.y, 280)
        node.setSideReach(sides)
        XCTAssertEqual(node.reach, UIEdgeInsets(top: 0, left: 10, bottom: 0, right: 0), "and is the host's no more")
    }

    func testTheReachGoesWithItsScrollView() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a route's side clearance is a scene window's safe area") }
        _ = try booted(scene)
        let node = try node("scroll")
        let scrolling = node.style
        var still = scrolling
        still["overflow_x"] = .string("visible"); still["overflow_y"] = .string("visible")
        node.applyStyle(still)
        XCTAssertNil(node.scrollView)
        XCTAssertEqual([node.reach, node.sideInset], [.zero, .zero])
        node.applyStyle(scrolling)
        let scroll = try XCTUnwrap(node.scroll)
        XCTAssertEqual(scroll.frame, node.bounds)
        XCTAssertEqual(scroll.contentOffset.x + node.scrollLeftInset(scroll), 0, accuracy: 0.5)
        session?.presenter.reachPageEdges()
        XCTAssertGreaterThan(scroll.frame.width, node.bounds.width, "the new scroll view reaches")
    }

    /// The fixture in the scene's window, once its route is laid out: the window's side bands and the route's node.
    private func booted(_ scene: UIWindowScene) throws -> (band: UIEdgeInsets, route: NodeView) {
        let session = ExactApp.shared.makeSession(label: "side-reach")
        self.session = session
        let view = ExactView(session: session), host = UIViewController(), window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        self.window = window
        view.frame = host.view.bounds
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        host.view.addSubview(view)
        XCTAssertNil(session.boot(plan: try fixturePlan("SIDE_REACH"), size: view.bounds.size).error)
        view.layoutIfNeeded()
        waitFor("the route is laid out") { !Agent(session: session).nativeInFlight() && (try? self.node("scroll"))?.scroll != nil }
        let band = window.safeAreaInsets
        try XCTSkipIf(band.left + band.right < 1, "no side band on this device")
        let node = try node("scroll")
        waitFor("the scroller reaches") { node.scroll.map { $0.frame.width > node.bounds.width } ?? false }
        return (band, try self.node("route"))
    }

    private func node(_ name: String) throws -> NodeView {
        try XCTUnwrap(session?.presenter.views.values.first { $0.props["testId"] == name }, name)
    }

    private func show(_ shape: String) throws {
        let session = try XCTUnwrap(session)
        session.presenter.press(try node("show-\(shape)").id)
        waitFor("\(shape) is laid out") { !Agent(session: session).nativeInFlight() }
    }

    /// The vertical indicator as UIKit draws it, shown, in `view`.
    private func vertical(_ scroll: UIScrollView, in view: UIView) throws -> CGRect {
        scroll.flashScrollIndicators()
        window?.layoutIfNeeded()
        let bars = scroll.subviews.filter { String(describing: type(of: $0)) == "_UIScrollViewScrollIndicator" }
        for bar in bars { bar.layer.removeAllAnimations(); bar.alpha = 1 }
        return try XCTUnwrap(bars.map { view.convert($0.bounds, from: $0) }.first { $0.height > $0.width }, "\(scroll.subviews)")
    }
}
#endif
