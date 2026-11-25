#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// LLP 1141.001 D4 over `scripts/fixtures/edge-effect.contract`: content
/// scrolled under the bar takes iOS 26's scroll edge effect, as a hand-built
/// screen's does, when the scroller's rows paint above their in-flow
/// siblings. UIKit hangs the effect in a view after the content; a row ranked
/// above depth 0 (one holding a native button, a `position: relative` card,
/// a sticky child with a `z-index`) drew over it, and ran sharp under the
/// title. Under a large title and an inline one, each scroller with pull to
/// refresh.
final class EdgeEffectIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?

    override func tearDown() {
        session?.destroy()
        session = nil
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    /// A session booted from the edge-effect fixture, showing its large-title
    /// route. Its window fills `scene` and the session boots at that size, so
    /// the route has the bars and safe area of the screen (docs/testing.md,
    /// "Safe-area tests fill the screen").
    private func fixture(in scene: UIWindowScene) throws -> ExactSession {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_EDGE_EFFECT_PLAN"], "build.mjs --test --ios compiles the edge-effect fixture")
        let session = ExactApp.shared.makeSession(label: "edge-effect")
        self.session = session
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        try waitForRoute("large")
        return session
    }

    private func node(_ testId: String) throws -> NodeView {
        try XCTUnwrap(session?.presenter.views.values.first { $0.props["testId"] == testId && $0.window != nil }, "no \(testId)")
    }

    private func waitForRoute(_ name: String) throws {
        let session = try XCTUnwrap(session), agent = Agent(session: session)
        waitFor("route \(name) is shown and settled") {
            let top = session.presenter.navigationHost.primaryNavigation?.topViewController as? RouteController
            return top?.node.props["testId"] == "route-\(name)" && !agent.nativeInFlight()
        }
    }

    /// The route's scroller, scrolled under its bar, is the one the bar's
    /// top edge follows. Its own effect draws above the content without
    /// an enclosing document scroller.
    private func assertEffectOverContent(_ name: String, file: StaticString = #filePath, line: UInt = #line) throws {
        let session = try XCTUnwrap(self.session)
        let route = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation?.topViewController as? RouteController)
        let sv = try XCTUnwrap(session.presenter.navigationHost.shownScroll(try node("content-\(name)")), file: file, line: line)
        XCTAssertNotNil(sv.refreshControl, "the scroller pulls to refresh", file: file, line: line)
        sv.contentOffset.y = -sv.adjustedContentInset.top + 400
        // Lay the scroller out at its new offset now: UIKit adds and moves its views in that layout, and `keepPlatformViewsOver` follows it
        sv.layoutIfNeeded()
        if #available(iOS 26.0, *) {
            // Match UIKit's edge effect as AgentNativeIOS.swift `inspectionAccount` does: a passthrough container holding its effect view
            let effect = { (v: UIView) in
                String(describing: type(of: v)) == "_UITouchPassthroughView"
                    && v.subviews.contains { ["ScrollEdgeEffectView", "BackdropView"].contains(String(describing: type(of: $0))) }
            }
            waitFor("UIKit's edge effect hangs after the content") {
                sv.subviews.lastIndex { $0 is NodeView }.map { sv.subviews[($0 + 1)...].contains(where: effect) } ?? false
            }
        }
        XCTAssertTrue(route.contentScrollView(for: .top) === sv, "the bar's top edge follows the scroller", file: file, line: line)

        XCTAssertNil(route.contentViewport, "the authored content scroller needs no document wrapper", file: file, line: line)
        var ancestor = sv.superview
        while let view = ancestor, view !== route.view {
            XCTAssertFalse(view is UIScrollView, "the content has one native scroll owner", file: file, line: line)
            ancestor = view.superview
        }
        XCTAssertTrue(ancestor === route.view, file: file, line: line)
        if #available(iOS 26.0, *) {
            let style: UIScrollEdgeEffect.Style = name == "large" ? .soft : .automatic
            XCTAssertEqual(sv.topEdgeEffect.style, style, "the content scroller keeps its own effect", file: file, line: line)
        }
        let rows = sv.subviews.filter { $0 is NodeView }
        let content = rows.map(\.layer.zPosition).max() ?? 0
        XCTAssertGreaterThan(content, 0, "the fixture ranks its content above depth 0", file: file, line: line)
        let last = try XCTUnwrap(sv.subviews.lastIndex { $0 is NodeView })
        let platform = sv.subviews[(last + 1)...]
        // Later subviews draw above content at the same layer depth
        for view in platform {
            XCTAssertGreaterThanOrEqual(view.layer.zPosition, content, "\(type(of: view)) draws over the content in \(type(of: sv))", file: file, line: line)
        }
    }

    func testAuthoredEffectsStayWithTheNativeContentScroller() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("scroll edge effects require iOS 26") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "the bar and its edge effect are shown in a scene's window") }
        let session = try fixture(in: scene), p = session.presenter
        let route = try XCTUnwrap(p.navigationHost.primaryNavigation?.topViewController as? RouteController)
        let node = try node("content-large"), scroll = try XCTUnwrap(p.navigationHost.shownScroll(node))
        for style in ["hard", "none", "soft", "automatic"] {
            var changed = node.style
            changed["scroll_edge_effect"] = .string(style)
            node.applyStyle(changed)
            p.reachPageEdges()
            XCTAssertNil(route.contentViewport)
            XCTAssertTrue(route.contentScrollView(for: .top) === scroll)
            XCTAssertEqual(scroll.topEdgeEffect.isHidden, style == "none")
            if style == "hard" { XCTAssertEqual(scroll.topEdgeEffect.style, .hard) }
            if style == "soft" { XCTAssertEqual(scroll.topEdgeEffect.style, .soft) }
            if style == "automatic" { XCTAssertEqual(scroll.topEdgeEffect.style, .automatic) }
        }
        let authored = node.style
        var unscrolling = authored
        unscrolling["overflow_x"] = .string("visible")
        unscrolling["overflow_y"] = .string("visible")
        node.applyStyle(unscrolling)
        XCTAssertNil(node.scrollView)
        XCTAssertNil(route.contentViewport, "a screen keeps its plain native mount when a backend disappears")
        node.applyStyle(authored)
        let recreated = try XCTUnwrap(node.scrollView)
        XCTAssertTrue(route.contentScrollView(for: .top) === recreated, "the bar follows the recreated content scroller without another batch")
        XCTAssertNil(route.contentViewport, "a late content backend becomes the only scroller without another batch")
    }

    /// A route whose page scrolls in a viewport (`resizes-visual`) shows the
    /// viewport's top effect only while its list is scrolled from its top. A
    /// route node that sets any aspect of its edge effect, by LLP 1123 D14's
    /// forms or by the retired one, stops the follow from the reach pass after
    /// the restyle: the top shows again, or stays as it stands where the node
    /// hides its top.
    func testARouteThatSetsItsEdgeEffectStopsItsViewportFollowingTheList() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("scroll edge effects require iOS 26") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "the bar and its edge effect are shown in a scene's window") }
        let session = try fixture(in: scene), p = session.presenter
        p.apply(wireBatch([["op": "props", "id": Int(try node("app").id), "set": ["interactiveWidget": "resizes-visual"]]]))
        XCTAssertNil(Agent(session: session).tap(["id": Int(try node("tab-inline").id)])["error"])
        try waitForRoute("inline")
        let route = try XCTUnwrap(p.navigationHost.primaryNavigation?.topViewController as? RouteController)
        let page = try XCTUnwrap(route.contentViewport, "the visual policy scrolls the route's page in a viewport")
        let list = try XCTUnwrap(try node("content-inline").scrollView)
        XCTAssertTrue(route.contentScrollView(for: .top) === list, "the bar follows the list in the page")
        XCTAssertTrue(page.topEdgeEffect.isHidden, "the viewport shows no effect over the list at rest")
        let style = route.node.style, rest = -list.adjustedContentInset.top
        route.node.style["scroll_edge_effect_hidden"] = .number(1)
        p.reachPageEdges()
        list.contentOffset.y = rest + 200
        XCTAssertTrue(page.topEdgeEffect.isHidden, "a node hiding its top leaves the top as it stands, hidden, the list followed no more")
        route.node.style["scroll_edge_effect_hidden"] = .number(4)
        p.reachPageEdges()
        XCTAssertFalse(page.topEdgeEffect.isHidden, "and one hiding only its bottom shows it again")
        list.contentOffset.y = rest
        route.node.style = style
        route.node.style["scroll_edge_effect"] = .string("none")
        p.reachPageEdges()
        XCTAssertFalse(page.topEdgeEffect.isHidden, "the retired `none` leaves it as it stands, shown, the list at rest")
        route.node.style = style
        p.reachPageEdges()
        XCTAssertTrue(page.topEdgeEffect.isHidden, "and one that sets no form has it follow the list again, at rest")
    }

    /// UIKit draws the bar's effect on the viewport, the outermost scroll view
    /// under it, so the viewport shows the list's own top effect (LLP 1077 D16):
    /// UIKit's where the list says nothing, the style the list writes from the
    /// reach pass after a restyle, and none where the list hides its top. A
    /// route node that sets its own edge effect stops the follow, and the
    /// viewport's top takes UIKit's style again.
    func testAViewportShowsTheTopEffectOfTheListItFollows() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("scroll edge effects require iOS 26") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "the bar and its edge effect are shown in a scene's window") }
        let session = try fixture(in: scene), p = session.presenter
        p.apply(wireBatch([["op": "props", "id": Int(try node("app").id), "set": ["interactiveWidget": "resizes-visual"]]]))
        XCTAssertNil(Agent(session: session).tap(["id": Int(try node("tab-inline").id)])["error"])
        try waitForRoute("inline")
        let route = try XCTUnwrap(p.navigationHost.primaryNavigation?.topViewController as? RouteController)
        let page = try XCTUnwrap(route.contentViewport, "the visual policy scrolls the route's page in a viewport")
        let content = try node("content-inline"), list = try XCTUnwrap(content.scrollView)
        XCTAssertTrue(route.contentScrollView(for: .top) === list, "the bar follows the list in the page")
        let scrollers = sequence(first: list as UIView, next: \.superview).prefix { $0 !== route.view }.filter { $0 is UIScrollView }
        XCTAssertTrue(scrollers.last === page, "the viewport is the outermost scroll view under the bar, where UIKit draws its effect")
        list.contentOffset.y = -list.adjustedContentInset.top + 200
        XCTAssertFalse(page.topEdgeEffect.isHidden)
        XCTAssertEqual(page.topEdgeEffect.style, .automatic, "a list that says nothing leaves UIKit's")
        let unsaid = content.style
        for (style, hidden) in [("soft", false), ("hard", false), ("none", true)] {
            var restyled = unsaid
            restyled["scroll_edge_effect"] = .string(style)
            content.applyStyle(restyled)
            p.reachPageEdges()
            XCTAssertEqual(page.topEdgeEffect.isHidden, hidden, style)
            if !hidden { XCTAssertEqual(page.topEdgeEffect.style, style == "hard" ? .hard : .soft, style) }
        }
        var soft = unsaid
        soft["scroll_edge_effect"] = .string("soft")
        content.applyStyle(soft)
        let own = route.node.style
        route.node.style["scroll_edge_effect_hidden"] = .number(4)
        p.reachPageEdges()
        XCTAssertFalse(page.topEdgeEffect.isHidden)
        XCTAssertEqual(page.topEdgeEffect.style, .automatic, "a route node that sets its own edge effect stops the follow, in UIKit's style")
        route.node.style = own
        p.reachPageEdges()
        XCTAssertEqual(page.topEdgeEffect.style, .soft, "and the list's again once the node sets none")
        content.applyStyle(unsaid)
        p.reachPageEdges()
        XCTAssertFalse(page.topEdgeEffect.isHidden)
        XCTAssertEqual(page.topEdgeEffect.style, .automatic, "UIKit's again once the list says nothing")
    }

    /// The bar's top edge can take a route's list before the page has a
    /// viewport, as at a launch onto the route. The viewport follows that list
    /// once made: hidden at rest, and the list's own soft once scrolled.
    func testAViewportMadeAfterItsListWasTakenFollowsIt() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("scroll edge effects require iOS 26") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "the bar and its edge effect are shown in a scene's window") }
        let session = try fixture(in: scene), p = session.presenter, agent = Agent(session: session)
        let list = try XCTUnwrap(try node("content-large").scrollView)
        let route = try XCTUnwrap(p.navigationHost.primaryNavigation?.topViewController as? RouteController)
        XCTAssertNil(route.contentViewport)
        XCTAssertTrue(route.contentScrollView(for: .top) === list, "the bar took the list before there was a viewport")
        p.apply(wireBatch([["op": "props", "id": Int(try node("app").id), "set": ["interactiveWidget": "resizes-visual"]]]))
        XCTAssertNil(agent.tap(["id": Int(try node("tab-inline").id)])["error"])
        try waitForRoute("inline")
        XCTAssertNil(agent.tap(["id": Int(try node("tab-large").id)])["error"])
        try waitForRoute("large")
        let page = try XCTUnwrap(route.contentViewport, "the visual policy scrolls the route's page in a viewport")
        XCTAssertTrue(route.contentScrollView(for: .top) === list)
        list.contentOffset.y = -list.adjustedContentInset.top
        XCTAssertTrue(page.topEdgeEffect.isHidden, "no effect over the list at rest")
        list.contentOffset.y = -list.adjustedContentInset.top + 200
        XCTAssertFalse(page.topEdgeEffect.isHidden)
        XCTAssertEqual(page.topEdgeEffect.style, .soft, "the list's own soft")
    }

    func testADocumentScrollerRevealsContentOutsideItsInnerScroller() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the bar and its edge effect are shown in a scene's window") }
        let session = try fixture(in: scene)
        XCTAssertNil(Agent(session: session).tap(["id": Int(try node("tab-mixed").id)])["error"])
        try waitForRoute("mixed")
        let route = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation?.topViewController as? RouteController)
        XCTAssertNil(route.contentViewport)
        let document = try XCTUnwrap(try node("document-mixed").scrollView)
        let inner = try XCTUnwrap(try node("content-mixed").scrollView)
        XCTAssertTrue(inner.isDescendant(of: document))
        XCTAssertTrue(route.contentScrollView(for: .top) === document)
        if #available(iOS 26.0, *) { XCTAssertEqual(document.topEdgeEffect.style, .soft) }
        let field = try node("mixed-field")
        session.presenter.reveal(field)
        XCTAssertGreaterThan(document.contentOffset.y, 0, "document content beyond the inner scroller remains reachable")
        XCTAssertLessThanOrEqual(field.convert(field.bounds, to: route.view).maxY, route.view.bounds.maxY + 1)
    }

    func testTheEdgeEffectDrawsOverRowsUnderALargeTitle() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the bar and its edge effect are shown in a scene's window") }
        _ = try fixture(in: scene)
        try assertEffectOverContent("large")
    }

    /// The bar's large title, which UIKit hangs in the scroller the bar
    /// follows, is the bar's in the agreement walk (LLP 1080.001 D3), as
    /// the edge effect beside it is: not a stray.
    func testTheLargeTitleInTheScrollerIsTheBarsNotAStray() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the bar and its edge effect are shown in a scene's window") }
        let session = try fixture(in: scene)
        let sv = try XCTUnwrap(session.presenter.navigationHost.shownScroll(try node("content-large")))
        sv.layoutIfNeeded()
        let title = { sv.subviews.first { String(describing: type(of: $0)) == "_UINavigationBarLargeTitleView" } }
        guard title() != nil else { throw XCTSkip("this UIKit keeps the large title in the bar") }
        let agreement = try XCTUnwrap(Agent(session: session).agreement(limit: 32)["agreement"] as? [String: Any])
        XCTAssertNil((agreement["counts"] as? [String: Int])?["stray"], "\(agreement["disagreements"] ?? [])")
        let content = try node("content-large")
        let entries = try XCTUnwrap(session.presenter.inspectionDump(content, kind: "view", depth: 2, limit: 256, flatOnly: nil) { v, r in v.convert(r ?? v.bounds, to: nil) }["entries"] as? [[String: Any]])
        XCTAssertEqual(entries.first { $0["class"] as? String == "_UINavigationBarLargeTitleView" }?["role"] as? String, "large-title")
    }

    func testTheEdgeEffectDrawsOverRowsUnderAnInlineTitle() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the bar and its edge effect are shown in a scene's window") }
        let session = try fixture(in: scene)
        let reply = Agent(session: session).tap(["id": Int(try node("tab-inline").id)])
        XCTAssertNil(reply["error"], "\(reply)")
        try waitForRoute("inline")
        try assertEffectOverContent("inline")
    }
}
#endif
