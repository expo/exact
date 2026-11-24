#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation
@testable import ExactCollections
import ExactGroupedLists

/// LLP 1130: a page with no native container keeps the area it is laid out
/// in, the view less its safe area, and is drawn in a viewport that takes
/// the whole view, so it scrolls under the status bar and the home
/// indicator; a scroller at the area's top and foot reaches into those bands
/// with its content where it was and its `scrollTop` unchanged.
final class PageEdgesIOSTests: XCTestCase {
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

    /// `scripts/fixtures/page-edges.contract` in a window of the test host's
    /// scene, the screen's size, so its safe area is the device's.
    private func fixture(_ key: String = "PAGE_EDGES") throws -> (ExactSession, ExactView) {
        let plan = try fixturePlan(key)
        let session = ExactApp.shared.makeSession(label: "page-edges")
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        self.host = host
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        until("the page is laid out") { (try? node(session, "list"))?.scrollView != nil }
        view.fit()
        return (session, view)
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    func testAControlBarInsetKeepsTheScrollStartBeforeUIKitAdoptsIt() throws {
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["overflow_y": "scroll"]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 300, "h": 200],
            ["op": "content", "id": 1, "w": 300, "h": 1000],
        ]))
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        scroll.contentInsetAdjustmentBehavior = .never
        scroll.contentOffset = .zero
        let band = UIEdgeInsets(top: 69, left: 0, bottom: 0, right: 0)
        let inset = UIEdgeInsets(top: 138, left: 0, bottom: 0, right: 0)
        node.setPageBand(band, inset: inset)
        XCTAssertEqual(scroll.contentOffset.y, -inset.top, accuracy: 0.5, "the first row clears the entire control bar inset")
        XCTAssertEqual(scroll.contentOffset.y + scroll.adjustedContentInset.top, 0, accuracy: 0.5)
        scroll.contentInsetAdjustmentBehavior = .automatic
        XCTAssertEqual(scroll.contentOffset.y + node.scrollTopInset(scroll), 0, accuracy: 0.5, "UIKit adoption preserves the same logical start")
        scroll.contentInsetAdjustmentBehavior = .never
        scroll.contentOffset.y += 100
        node.setPageBand(.zero)
        XCTAssertEqual(scroll.contentOffset.y, 100, accuracy: 0.5, "removing the bar preserves the reader's position")
        node.setPageBand(band, inset: inset)
        XCTAssertEqual(scroll.contentOffset.y, 100 - inset.top, accuracy: 0.5)
    }

    /// A scroller whose overflow clips (`overflow-x: hidden` beside `overflow-y: scroll`) leaves the clip
    /// to its scroll view while a band draws the scroll view past the scroller's box: the scroller's own
    /// clip would cut the band, and the edge effect in it, off at its top. A restyle and a layout
    /// transition keep that. A text keeps its clip: its lines are drawn outside its scroll view.
    func testABandedScrollerLeavesItsClipToItsScrollView() throws {
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["overflow_x": "hidden", "overflow_y": "scroll"]],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "A long status line"], "style": ["overflow_x": "hidden", "overflow_y": "auto"]],
            ["op": "roots", "ids": [1, 2]],
            ["op": "frame", "id": 1, "x": 0, "y": 56, "w": 300, "h": 200],
            ["op": "content", "id": 1, "w": 300, "h": 1000],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 100, "h": 22],
        ]))
        let text = try XCTUnwrap(p.views[2])
        XCTAssertNotNil(text.scrollView)
        text.setPageBand(UIEdgeInsets(top: 56, left: 0, bottom: 0, right: 0))
        XCTAssertTrue(text.clipsToBounds, "a banded text still clips its lines")
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        XCTAssertTrue(node.clipsToBounds)
        let band = UIEdgeInsets(top: 56, left: 0, bottom: 0, right: 0)
        node.setPageBand(band)
        XCTAssertFalse(node.clipsToBounds)
        XCTAssertTrue(scroll.clipsToBounds)
        XCTAssertEqual(scroll.frame.minY, -56)
        node.syncScroll()
        XCTAssertFalse(node.clipsToBounds, "a restyle keeps the band's clip off")
        node.setPageBand(.zero)
        XCTAssertTrue(node.clipsToBounds, "a scroller with no band clips again")
        // A layout transition's surface clips the scroll view in the node's place; as it ends the node clips or not by the band it then has
        for after in [band, .zero] {
            node.layoutResize = CGSize(width: 0, height: 20)
            node.applySurface()
            XCTAssertFalse(node.clipsToBounds)
            node.setPageBand(after)
            XCTAssertFalse(node.clipsToBounds, "the surface keeps the clip while it lasts")
            node.layoutResize = .zero
            node.applySurface()
            XCTAssertEqual(node.clipsToBounds, after == .zero)
        }
    }

    func testThePageKeepsItsAreaAndIsDrawnPastIt() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the status bar and the safe area are a scene window's") }
        let (session, view) = try fixture()
        let p = session.presenter, safe = view.safeAreaInsets
        XCTAssertGreaterThan(safe.top + safe.bottom, 0, "the test host's window has a status bar or a home indicator")
        XCTAssertEqual(p.viewport.frame, view.bounds, "the viewport takes the whole view")
        XCTAssertEqual(p.viewportSize, view.bounds.inset(by: safe).size, "the kernel lays the page out in its area, as before")
        XCTAssertEqual(p.pageMargins.top, safe.top)
        XCTAssertEqual(p.pageMargins.bottom, safe.bottom)
        XCTAssertEqual(p.root.frame.minY, safe.top, "the document starts at the area's top")
        XCTAssertEqual(p.viewport.contentSize.height, view.bounds.height, accuracy: 0.5, "a page that fits its area does not scroll")
        XCTAssertEqual(p.viewport.contentOffset, .zero, "and rests at offset zero, its scrollTop")
        let top = try node(session, "list-top")
        XCTAssertEqual(top.convert(top.bounds, to: nil).minY, safe.top, accuracy: 0.5, "the content shows where it did")
    }

    func testAnEmptyResizeRefitsTheDocument() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "an embedded page resizes inside a scene's safe area") }
        let (session, view) = try fixture("FIXED_PAGE")
        let p = session.presenter
        let origin = CGPoint(x: view.safeAreaInsets.left + 20, y: view.safeAreaInsets.top + 20)
        view.frame = CGRect(origin: origin, size: CGSize(width: 300, height: 500))
        view.fit()
        waitForActivation(session)
        XCTAssertEqual(p.pageMargins, .zero, "the embedded view is inside the safe area")
        XCTAssertEqual(p.root.bounds.size, view.bounds.size)
        XCTAssertEqual(p.viewport.contentSize, view.bounds.size)
        let list = try node(session, "list")
        XCTAssertEqual(list.bounds.size, CGSize(width: 100, height: 100))
        for size in [CGSize(width: 200, height: 300), CGSize(width: 300, height: 500)] {
            let applied = session.appliedBatches
            view.frame.size = size
            view.fit()
            XCTAssertEqual(session.appliedBatches, applied, "the fixed root produces no runtime operations")
            XCTAssertEqual(p.pageMargins, .zero, "the margins did not change")
            XCTAssertEqual(list.bounds.size, CGSize(width: 100, height: 100))
            XCTAssertEqual(p.root.bounds.size, size, "the document follows the viewport")
            XCTAssertEqual(p.viewport.contentSize, size, "a fitting page has no blank scroll range")
        }
    }

    func testAnEmptyResizeRefitsTheEdgeBands() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the home indicator's band belongs to a scene window") }
        let (session, view) = try fixture("FIXED_PAGE")
        let p = session.presenter, host = try XCTUnwrap(host?.view)
        let bottom = host.safeAreaInsets.bottom
        XCTAssertGreaterThan(bottom, 0, "the window has a home indicator")
        func resize(_ height: CGFloat) {
            view.frame = CGRect(x: 20, y: host.bounds.maxY - height - bottom, width: 200, height: height + bottom)
            view.fit()
        }
        resize(300)
        waitForActivation(session)
        let list = try node(session, "list")
        let margins = p.pageMargins
        XCTAssertEqual(margins, UIEdgeInsets(top: 0, left: 0, bottom: bottom, right: 0))
        XCTAssertEqual(list.pageBand.bottom, 0)
        for height: CGFloat in [100, 300] {
            let applied = session.appliedBatches
            resize(height)
            XCTAssertEqual(session.appliedBatches, applied, "the fixed root produces no runtime operations")
            XCTAssertEqual(p.pageMargins, margins, "the margins did not change")
            XCTAssertEqual(p.root.bounds.size, CGSize(width: 200, height: height))
            XCTAssertEqual(p.viewport.contentSize, view.bounds.size)
            XCTAssertEqual(list.pageBand.bottom, height == 100 ? bottom : 0, accuracy: 0.5, "only a scroller at the page's foot reaches into its band")
        }
    }

    func testTheScrollerAtTheEdgesReachesIntoTheBands() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the status bar and the safe area are a scene window's") }
        let (session, view) = try fixture()
        let safe = view.safeAreaInsets, list = try node(session, "list"), sv = try XCTUnwrap(list.scroll)
        XCTAssertEqual(list.pageBand, UIEdgeInsets(top: safe.top, left: 0, bottom: safe.bottom, right: 0))
        XCTAssertEqual(sv.convert(sv.bounds, to: nil).minY, 0, accuracy: 0.5, "its scroll view starts at the screen's top")
        XCTAssertEqual(sv.convert(sv.bounds, to: nil).maxY, view.bounds.maxY, accuracy: 0.5, "and ends at its foot")
        XCTAssertEqual(sv.contentOffset.y + list.scrollTopInset(sv), 0, accuracy: 0.5, "scrollTop 0 is the rest under the band")
        let top = try node(session, "list-top")
        XCTAssertEqual(top.convert(top.bounds, to: nil).minY, safe.top, accuracy: 0.5, "its content shows where it did")
        // The CSS port is the box: the band is neither port nor content.
        XCTAssertEqual(sv.bounds.height - list.scrollPortInsets(sv).top - list.scrollPortInsets(sv).bottom, list.bounds.height, accuracy: 0.5)
        // A scroller that clips its own overflow (`overflow-x: hidden`) takes the bands, as one with `overflow-x: auto` does
        list.style["overflow_x"] = .string("hidden")
        list.syncScroll()
        list.setPageBand(.zero)
        XCTAssertTrue(list.clipsToBounds)
        session.presenter.reachPageEdges()
        XCTAssertEqual(list.pageBand, UIEdgeInsets(top: safe.top, left: 0, bottom: safe.bottom, right: 0))
        XCTAssertFalse(list.clipsToBounds)
        // The agent still names it as what clips its content
        let facts = try XCTUnwrap(Agent(session: session).layout(["id": Int(top.id)])["node"] as? [String: Any])
        let clip = try XCTUnwrap(facts["clip"] as? [[String: Any]], "\(facts)")
        XCTAssertTrue(clip.contains { $0["id"] as? Int == Int(list.id) && $0["kind"] as? String == "overflow" }, "\(clip)")
    }

    func testATouchInABandReachesTheScrollerThere() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the status bar and the safe area are a scene window's") }
        let (session, view) = try fixture()
        let list = try node(session, "list"), sv = try XCTUnwrap(list.scroll), window = try XCTUnwrap(view.window)
        // A finger under the status bar or on the home indicator's band, as on a hand-built table view.
        for y in [view.safeAreaInsets.top / 2, view.bounds.maxY - view.safeAreaInsets.bottom / 2] {
            let hit = try XCTUnwrap(window.hitTest(CGPoint(x: view.bounds.midX + 60, y: y), with: nil), "nothing at y \(y)")
            XCTAssertTrue(hit.isDescendant(of: sv), "y \(y) reaches the scroller, not \(type(of: hit))")
        }
    }

    func testARouteWithNoBarRunsItsScrollerUnderTheStatusBar() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the status bar and the safe area are a scene window's") }
        let (session, view) = try fixture("BARLESS_ROUTE")
        guard view.safeAreaInsets.top > 0 else {
            throw XCTSkip("no status bar above the route for its scroller to run under: the iPhone Duo's stands in a side column")
        }
        let list = try node(session, "list"), route = try node(session, "route"), top = try node(session, "list-top")
        until("the route's scroller is UIKit's to inset") { list.scrollView?.adjustedContentInset.top ?? 0 > 0 }
        let sv = try XCTUnwrap(list.scrollView), safe = view.safeAreaInsets
        XCTAssertEqual(sv.convert(sv.bounds, to: nil).minY, 0, accuracy: 0.5, "its scroll view starts at the screen's top")
        XCTAssertEqual(route.convert(route.bounds, to: nil).minY, 0, accuracy: 0.5)
        XCTAssertEqual(top.convert(top.bounds, to: nil).minY, safe.top, accuracy: 0.5, "its content starts below the status bar")
        XCTAssertEqual(sv.contentOffset.y + list.scrollTopInset(sv), 0, accuracy: 0.5, "at scrollTop 0")
    }

    func testAGroupedListAtTheEdgesReachesIntoTheBands() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the status bar and the safe area are a scene window's") }
        ExactGroupedLists.install()
        let (session, view) = try fixture("PAGE_EDGES_GROUPED")
        let list = try node(session, "list"), safe = view.safeAreaInsets
        // The bands at the top and the foot, or only the foot's on the iPhone Duo, whose status bar stands in a side column
        until("UIKit draws the list") { list.scrollView is UICollectionView && list.pageBand.top + list.pageBand.bottom > 0 }
        let cv = try XCTUnwrap(list.scrollView as? UICollectionView)
        XCTAssertEqual(list.pageBand, UIEdgeInsets(top: safe.top, left: 0, bottom: safe.bottom, right: 0))
        XCTAssertEqual(cv.convert(cv.bounds, to: nil).minY, 0, accuracy: 0.5, "its collection view starts at the screen's top")
        XCTAssertEqual(cv.convert(cv.bounds, to: nil).maxY, view.bounds.maxY, accuracy: 0.5, "and ends at its foot")
        XCTAssertEqual(cv.contentOffset.y + list.scrollTopInset(cv), 0, accuracy: 0.5, "scrollTop 0 is the rest under the band")
        cv.layoutIfNeeded()
        let first = try XCTUnwrap(cv.visibleCells.min { $0.frame.minY < $1.frame.minY })
        XCTAssertGreaterThanOrEqual(first.convert(first.bounds, to: nil).minY, safe.top - 0.5, "its first card starts below the status bar")
        if safe.top > 0 {
            let hit = try XCTUnwrap(view.window?.hitTest(CGPoint(x: view.bounds.midX, y: safe.top / 2), with: nil))
            XCTAssertTrue(hit.isDescendant(of: cv), "a finger under the status bar reaches the list, not \(type(of: hit))")
        }
    }

    /// A scroller at the edges shown again inside an animation UIKit has
    /// open (`display: none` lifted): it takes its band, its insets and its
    /// offset together and at once, so its content does not slide
    /// (`NodeView.setPageBand` through `Presenter.moveOrPlace`).
    func testAScrollerShownAgainInsideAnAnimationTakesItsBandAtOnce() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the status bar and the safe area are a scene window's") }
        let (session, view) = try fixture()
        let p = session.presenter, list = try node(session, "list"), sv = try XCTUnwrap(list.scroll)
        let band = list.pageBand
        XCTAssertGreaterThan(band.top + band.bottom, 0, "a band to take")
        list.isHidden = true
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, .zero, "a hidden scroller has no band")
        waitFor("a frame shows it hidden") { list.layer.presentation()?.isHidden == true }
        var open = false
        UIView.animate(withDuration: 0.5) {
            open = UIView.inheritedAnimationDuration > 0
            list.isHidden = false
            p.reachPageEdges()
        }
        XCTAssertTrue(open, "UIKit had an animation open")
        XCTAssertEqual(list.pageBand, band)
        XCTAssertEqual(sv.contentOffset.y + list.scrollTopInset(sv), 0, accuracy: 0.5, "at its rest under the band")
        XCTAssertEqual(sv.layer.animationKeys() ?? [], [], "its frame and its offset at once, together")
        _ = view
    }

    /// The offset of a grouped list's collection view, which UIKit sets as
    /// it insets it, follows the same rule as a `ScrollView`'s: set inside an
    /// animation on a list no frame has shown, it is there at once.
    func testAGroupedListsOffsetInsideAnAnimationMovesOnlyIfItShowed() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the status bar and the safe area are a scene window's") }
        ExactGroupedLists.install()
        let (session, _) = try fixture("PAGE_EDGES_GROUPED")
        let list = try node(session, "list")
        until("UIKit draws the list") { list.scrollView is UICollectionView && list.pageBand.top + list.pageBand.bottom > 0 }
        let cv = try XCTUnwrap(list.scrollView as? UICollectionView)
        waitFor("a frame shows the list") { session.presenter.showed(list) }
        let rest = cv.contentOffset
        UIView.animate(withDuration: 0.5) { cv.contentOffset.y = rest.y + 20 }
        XCTAssertNotNil(cv.layer.animation(forKey: "bounds.origin") ?? cv.layer.animation(forKey: "bounds"), "a list that showed moves with the animation")
        cv.layer.removeAllAnimations()
        list.isHidden = true
        waitFor("a frame shows it hidden") { list.layer.presentation()?.isHidden == true }
        list.isHidden = false
        UIView.animate(withDuration: 0.5) { cv.contentOffset.y = rest.y }
        XCTAssertEqual(cv.layer.animationKeys() ?? [], [], "one shown again since the last frame is at its offset at once")
    }

    /// LLP 1131: a route's list under its control bars, drawn through them and the route's cover.
    func testAListRunsUnderItsControlBarsAndKeepsItsPlace() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bars and the safe area are a scene window's") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("control bars are iOS 26's, where UIKit draws the edge effect over them") }
        let (session, _) = try fixture("CONTROL_BARS")
        let p = session.presenter, list = try node(session, "list"), filter = try node(session, "filter"), top = try node(session, "list-top")
        until("the switcher is a bar over the list") { list.pageBand.top > 0 }
        let sv = try XCTUnwrap(list.scrollView), box = list.convert(list.bounds, to: nil)
        XCTAssertEqual(sv.convert(sv.bounds, to: nil).minY, 0, accuracy: 0.5, "its scroll view runs up under the switcher and the bar")
        XCTAssertEqual(top.convert(top.bounds, to: nil).minY, box.minY, accuracy: 0.5, "its content rests where it was laid out")
        XCTAssertEqual(sv.contentOffset.y + list.scrollTopInset(sv), 0, accuracy: 0.5, "at scrollTop 0")
        XCTAssertGreaterThan(filter.layer.zPosition, list.layer.zPosition, "the switcher draws over the list")
        let hit = try XCTUnwrap(filter.window?.hitTest(filter.convert(CGPoint(x: filter.bounds.midX, y: filter.bounds.midY), to: nil), with: nil))
        XCTAssertTrue(hit.isDescendant(of: filter), "a tap on the switcher is the switcher's, not \(type(of: hit))")
        XCTAssertTrue(filter.interactions.contains { $0 is UIScrollEdgeElementContainerInteraction }, "UIKit's edge effect over the list")
        // An edge whose effect the list hides is neither banded nor open (LLP 1131 D2): here, with no open foot over a
        // bottom cover, a hidden top keeps both edges where they are, as a top with more than bars does, and a hidden foot
        // leaves the top its band
        let style = list.style, board = list.pageBand
        list.style["scroll_edge_effect_hidden"] = .number(1)
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, .zero, "a hidden top keeps both edges where they are")
        list.style["scroll_edge_effect_hidden"] = .number(4)
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, UIEdgeInsets(top: board.top, left: 0, bottom: 0, right: 0), "a hidden foot leaves the top its band")
        list.style = style
        list.style["scroll_edge_effect"] = .string("none")
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, .zero, "the retired `none` hides both")
        list.style = style
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, board)
        // An app's own scroll view holds the bar's edge through a batch, and its nil gives the edge
        // back to the list by the route's next layout, through the list's band under the switcher,
        // since the title does not collapse with the list
        let route = try XCTUnwrap(p.navigationHost.activeRoute)
        XCTAssertNil(route.collapseScroll, "the switcher stands between the title and the list")
        XCTAssertTrue(route.contentScrollView(for: .top) === sv, "the bar follows the list under the switcher")
        let own = UIScrollView(frame: route.view.bounds)
        route.view.addSubview(own)
        route.setContentScrollView(own, for: .top)
        p.apply(wireBatch([]))
        XCTAssertTrue(route.contentScrollView(for: .top) === own)
        route.setContentScrollView(nil, for: .top)
        own.removeFromSuperview()
        route.view.layoutIfNeeded()
        XCTAssertTrue(route.contentScrollView(for: .top) === sv, "nil gives the edge back to the list drawn under the switcher")
        sv.contentOffset.y = 100 - list.scrollTopInset(sv)
        p.press(try node(session, "pick-chat").id)
        until("the switcher goes and a composer comes") { list.pageBand.top == 0 && list.pageBand.bottom > 0 }
        XCTAssertEqual(sv.contentOffset.y + list.scrollTopInset(sv), 100, accuracy: 1, "scrollTop is kept as the bands change")
        XCTAssertNil(filter.window, "the switcher is gone")
        XCTAssertEqual(sv.convert(sv.bounds, to: nil).maxY, try XCTUnwrap(list.window).bounds.maxY, accuracy: 0.5, "the transcript runs under the composer to the screen's foot")
        XCTAssertGreaterThan(try node(session, "composer").layer.zPosition, list.layer.zPosition, "the composer draws over it")
        let chat = list.pageBand
        list.style["scroll_edge_effect_hidden"] = .number(1)
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, .zero, "a hidden top keeps the transcript above the composer too")
        list.style["scroll_edge_effect_hidden"] = .number(4)
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, .zero, "a hidden foot leaves the composer no band, and the open top none without it")
        list.style = style
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, chat)
        for shape in ["caption", "painted", "large"] {
            p.press(try node(session, "pick-\(shape)").id)
            until("\(shape) is no bar over the list") { list.pageBand == .zero }
        }
    }

    /// An authored `edge-effect` registration (LLP 1123 D12) gives a bar's view UIKit's scroll edge element
    /// interaction with the scroller it names, on its edge, over LLP 1131's inferred one for that view, and one
    /// naming no scroller removes it. The same registration keeps its interaction, one follows its scroller's new
    /// scroll view, a scroller out of the window gives none, clearing the authored ones gives the inferred one back,
    /// a painted bar 1131 rejects and a flat box register when authored, a destroyed bar loses its interaction with
    /// its registration, and a reset takes every interaction away.
    func testAnAuthoredBarEdgeEffectWinsOverTheInferredOne() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the bars and the safe area are a scene window's") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("UIScrollEdgeElementContainerInteraction is iOS 26's") }
        let (session, _) = try fixture("CONTROL_BARS")
        let p = session.presenter, list = try node(session, "list"), filter = try node(session, "filter")
        until("the switcher is a bar over the list") { list.pageBand.top > 0 }
        let sv = try XCTUnwrap(list.scrollView)
        let held = { (bar: NodeView) in bar.interactions.compactMap { $0 as? UIScrollEdgeElementContainerInteraction } }
        XCTAssertEqual(held(filter).map(\.edge), [.top])
        XCTAssertTrue(held(filter).first?.scrollView === sv, "LLP 1131 infers the switcher's registration")
        ControlBars.setBarEdgeEffects([(filter.id, .top, nil)], in: p)
        XCTAssertTrue(held(filter).isEmpty, "an authored registration with no scroller wins")
        ControlBars.setBarEdgeEffects([(filter.id, .bottom, list.id)], in: p)
        let authored = try XCTUnwrap(held(filter).first)
        XCTAssertEqual(held(filter).count, 1)
        XCTAssertTrue(authored.scrollView === sv)
        XCTAssertEqual(authored.edge, .bottom)
        ControlBars.setBarEdgeEffects([(filter.id, .bottom, list.id)], in: p)
        XCTAssertTrue(held(filter).first === authored, "the same registration keeps its interaction")
        // The runner's restyles replace the list's scroll view, and the batch's end moves the registration onto the new one
        let scrolling = list.style.mapValues(\.any)
        let still = scrolling.merging(["overflow_x": "visible", "overflow_y": "visible"]) { _, b in b }
        p.apply(wireBatch([["op": "style", "id": list.id, "style": still]]))
        XCTAssertNil(list.scrollView, "no scroll view while it does not scroll")
        XCTAssertTrue(held(filter).isEmpty, "and no registration")
        p.apply(wireBatch([["op": "style", "id": list.id, "style": scrolling]]))
        let recreated = try XCTUnwrap(list.scrollView)
        XCTAssertTrue(recreated !== sv && held(filter).first?.scrollView === recreated, "it follows the list's new scroll view")
        p.apply(wireBatch([["op": "create", "id": 9000, "kind": "scroll", "style": ["overflow_y": "auto"]]]))
        XCTAssertNotNil(p.views[9000]?.scrollView)
        ControlBars.setBarEdgeEffects([(filter.id, .top, 9000)], in: p)
        XCTAssertTrue(held(filter).isEmpty, "a scroller out of the window gives none")
        ControlBars.setBarEdgeEffects([], in: p)
        XCTAssertEqual(held(filter).map(\.edge), [.top])
        XCTAssertTrue(held(filter).first?.scrollView === recreated, "the inferred registration comes back")
        // It goes as its scroller leaves the window and comes back with it, a reach pass between them included
        let home = try XCTUnwrap(list.superview), place = try XCTUnwrap(home.subviews.firstIndex(of: list))
        list.removeFromSuperview()
        until("its scroller's leaving takes the inferred one away") { held(filter).isEmpty }
        p.reachPageEdges()
        home.insertSubview(list, at: place)
        until("and it comes back with its scroller") { held(filter).first?.scrollView === recreated }
        p.press(try node(session, "pick-painted").id)
        // The list keeps the foot's margin, at the foot of a page that fits (LLP 1131 D2)
        until("the painted switcher is no bar") { list.pageBand.top == 0 }
        let painted = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "filter" && $0.window != nil })
        XCTAssertTrue(held(painted).isEmpty)
        ControlBars.setBarEdgeEffects([(painted.id, .top, list.id)], in: p)
        XCTAssertTrue(held(painted).first?.scrollView === recreated, "an authored registration needs no inferred bar")
        let (viewport, bar) = (try XCTUnwrap(p.viewport as? Viewport), painted.id)
        p.press(try node(session, "pick-chat").id)
        until("the switcher goes") { painted.window == nil }
        XCTAssertNil(viewport.authoredBars[bar], "a destroyed bar's registration goes with it")
        XCTAssertTrue(held(painted).isEmpty, "and its view's interaction")
        // A flat box takes a view to hold its interaction, and a reset takes every interaction away
        let q = Presenter()
        q.viewport.frame = try XCTUnwrap(list.window).bounds
        try XCTUnwrap(list.window).addSubview(q.viewport)
        defer { q.viewport.removeFromSuperview() }
        q.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "props": ["testId": "root"]],
            ["op": "create", "id": 2, "kind": "scroll", "props": ["testId": "pane"], "style": ["overflow_y": "auto"]],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 400.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 40.0, "w": 300.0, "h": 360.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 300.0, "h": 40.0],
        ]))
        XCTAssertTrue(q.flats.isFlat(3), "a plain box is a flat leaf")
        ControlBars.setBarEdgeEffects([(3, .top, 2)], in: q)
        let (promoted, root) = (try XCTUnwrap(q.views[3]), try XCTUnwrap(q.views[1]))
        let pane = try XCTUnwrap(q.views[2]?.scrollView)
        XCTAssertNotNil(promoted.window, "the promoted bar is in the window, under \(String(describing: promoted.superview))")
        XCTAssertNotNil(pane.window, "and so is the pane's scroll view")
        XCTAssertTrue(held(promoted).first?.scrollView === pane, "and registers as a view: \(held(promoted))")
        // It goes as its scroller leaves the window and comes back with it, the bar mounted throughout
        let scroller = try XCTUnwrap(q.views[2])
        scroller.removeFromSuperview()
        until("its scroller's leaving takes it away") { held(promoted).isEmpty }
        root.insertSubview(scroller, at: 0)
        until("and its return gives it back") { held(promoted).first?.scrollView === pane }
        // Its interaction goes as the bar leaves the window, at once as it is forgotten or parked, and at a
        // reset; its return gives it back, as a reach pass does while it is registered
        promoted.removeFromSuperview()
        until("leaving the window") { held(promoted).isEmpty }
        root.addSubview(promoted)
        until("back in the window") { held(promoted).count == 1 }
        promoted.forget()
        XCTAssertTrue(held(promoted).isEmpty, "forgotten")
        q.reachPageEdges()
        promoted.recycle()
        XCTAssertTrue(held(promoted).isEmpty, "parked")
        q.reachPageEdges()
        XCTAssertEqual(held(promoted).count, 1)
        q.reset()
        XCTAssertTrue(held(promoted).isEmpty, "a reset takes the interactions away")
    }

    /// A transcript in a box over a composer its author filled: the composer is no bar, so
    /// the rows stop above it, and the top still runs under the inline bar, as a hand-built
    /// table under its navigation bar does whatever is pinned under it (bake-off r28's chat).
    func testAListOverAFilledComposerStillRunsUnderTheBar() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bars and the safe area are a scene window's") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("control bars are iOS 26's, where UIKit draws the edge effect over them") }
        let (session, _) = try fixture("CONTROL_BARS")
        session.presenter.press(try node(session, "pick-filled").id)
        until("the transcript runs under the bar") {
            guard let sv = (try? self.node(session, "wrapped"))?.scrollView else { return false }
            return abs(sv.convert(sv.bounds, to: nil).minY) < 0.5
        }
        let list = try node(session, "wrapped"), sv = try XCTUnwrap(list.scrollView), composer = try node(session, "filled-composer")
        XCTAssertEqual(list.pageBand.bottom, 0, "the filled composer keeps the rows above it")
        XCTAssertEqual(sv.convert(sv.bounds, to: nil).minY, 0, accuracy: 0.5, "its scroll view runs up under the bar")
        XCTAssertEqual(sv.convert(sv.bounds, to: nil).maxY, composer.convert(composer.bounds, to: nil).minY, accuracy: 0.5, "and stops at the composer")
        let route = try XCTUnwrap(session.presenter.navigationHost.activeRoute)
        XCTAssertTrue(route.contentScrollView(for: .top) === sv, "the bar's edge effect follows it")
    }

    /// A virtualized transcript over its composer (the chat recipe), shown when its messages
    /// arrive mid-push, opens with its newest row above the composer. As the push lands its
    /// box grows under the bar and its top band goes to UIKit's inset, which leaves its port
    /// the same size: keeping its scrollTop put the last two rows under the composer, and the
    /// runner, which saw only the offset move, took it for the reader leaving the end (round
    /// 32's chat-rooms).
    func testAPushedTranscriptOpensWithItsNewestRowAboveItsComposer() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bars and the safe area are a scene window's") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("control bars are iOS 26's, where UIKit draws the edge effect over them") }
        let (session, _) = try fixture("CONTROL_BARS")
        session.presenter.press(try node(session, "pick-thread").id)
        until("the push lands, the transcript under the bar and its composer") {
            // Not `node`: its XCTUnwrap records a failure while the messages are on their way.
            guard let list = session.presenter.views.values.first(where: { $0.props["testId"] == "transcript" }), list.window != nil else { return false }
            return list.pageBand.top == 0 && list.pageBand.bottom > 0
        }
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        let composer = try node(session, "thread-composer"), last = try node(session, "message-30")
        XCTAssertEqual(last.convert(last.bounds, to: nil).maxY, composer.convert(composer.bounds, to: nil).minY - 6, accuracy: 1,
                       "the newest message ends at the composer's margin")
    }

    func testScrollTopIsKeptWhenTheBandGoes() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the status bar and the safe area are a scene window's") }
        let (session, view) = try fixture()
        guard view.safeAreaInsets.top > 0 else {
            throw XCTSkip("no status bar above the page for its band to leave: the iPhone Duo's stands in a side column")
        }
        let p = session.presenter, list = try node(session, "list"), sv = try XCTUnwrap(list.scroll)
        sv.contentOffset.y = 100 - list.scrollTopInset(sv)
        p.press(try node(session, "head").id)
        until("a line above the scroller takes it off the top edge") { list.pageBand.top == 0 && (try? node(session, "header")) != nil }
        XCTAssertGreaterThan(list.pageBand.bottom, 0, "it keeps the bottom band")
        XCTAssertEqual(sv.contentOffset.y + list.scrollTopInset(sv), 100, accuracy: 0.5, "the logical scrollTop is kept")
        p.press(try node(session, "head").id)
        until("the band comes back") { list.pageBand.top > 0 }
        XCTAssertEqual(sv.contentOffset.y + list.scrollTopInset(sv), 100, accuracy: 0.5, "and kept again")
    }

    func testAScrollerThatFitsDoesNotScrollByItsBands() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the status bar and the safe area are a scene window's") }
        let (session, _) = try fixture()
        let p = session.presenter, list = try node(session, "list"), sv = try XCTUnwrap(list.scroll)
        p.press(try node(session, "shorten").id)
        until("the content is short") { (try? node(session, "filler"))?.bounds.height == 40 }
        let i = sv.adjustedContentInset
        XCTAssertLessThanOrEqual(sv.contentSize.height + i.top + i.bottom, sv.bounds.height + 0.5, "no range from the bands, as a table that fits")
    }
    func testALargeTitleCollapsesOverAFixedControlBar() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the title and safe area belong to a scene window") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("control bars require iOS 26") }
        let (session, _) = try fixture("CONTROL_BARS")
        let p = session.presenter
        p.press(try node(session, "pick-large").id)
        let filter = try node(session, "filter"), list = try node(session, "list")
        p.apply(wireBatch([["op": "props", "id": Int(filter.id), "set": ["accessibilityRole": "group"]]]))
        let route = try XCTUnwrap(p.navigationHost.activeRoute)
        until("the filter is page content and its bar has settled") { route.scoped == nil && !Agent(session: session).nativeInFlight() }
        let nav = try XCTUnwrap(route.navigationController), sv = try XCTUnwrap(list.scrollView)
        guard !nav.traitCollection.placesVerticalBar else { throw XCTSkip("a vertical bar has no expanding title") }
        nav.view.layoutIfNeeded()
        let expanded = nav.navigationBar.bounds.height
        XCTAssertGreaterThan(expanded, 80)
        XCTAssertTrue(route.collapseScroll === list)
        XCTAssertGreaterThan(list.pageBand.top, 0)
        let top = try node(session, "list-top")
        XCTAssertEqual(top.convert(top.bounds, to: nil).minY, filter.convert(filter.bounds, to: nil).maxY + 8, accuracy: 1)
        let beforeDrag = sv.contentOffset.y + list.scrollTopInset(sv)
        Agent.rest(sv, at: CGPoint(x: 0, y: sv.contentOffset.y + 30), x: false, y: true)
        nav.view.layoutIfNeeded()
        let turn = expectation(description: "bar covers delivered")
        DispatchQueue.main.async { turn.fulfill() }
        wait(for: [turn], timeout: 5)
        let shortDragHeight = nav.navigationBar.bounds.height
        let shortDragTop = sv.contentOffset.y + list.scrollTopInset(sv)
        XCTAssertGreaterThan(shortDragTop, beforeDrag, "the released drag must move content")
        Agent.rest(sv, at: CGPoint(x: 0, y: 300 - sv.adjustedContentInset.top), x: false, y: true)
        nav.view.layoutIfNeeded()
        XCTAssertLessThan(nav.navigationBar.bounds.height, 65)
        let collapsed = nav.navigationBar.bounds.height
        XCTAssertEqual(min(abs(shortDragHeight - expanded), abs(shortDragHeight - collapsed)), 0, accuracy: 0.5,
                       "the short drag rests at an exact native title height")
        XCTAssertEqual(shortDragTop, beforeDrag + expanded - collapsed, accuracy: 0.5,
                       "the offset follows the same native snap instead of returning to the top")
        XCTAssertEqual(filter.convert(filter.bounds, to: route.view).minY, route.view.safeAreaInsets.top + 8, accuracy: 1)
        // A route that clips to its own box (`overflow="hidden"` at `inset=0`) keeps its bars: no band passes its edge.
        route.node.clipsToBounds = true
        defer { route.node.clipsToBounds = false }
        XCTAssertEqual(ControlBars(p).bars(of: list, in: route.node, top: true)?.isEmpty, false)
    }

    /// LLP 1131 D3: rows of buttons above and below a column holding a row and the list are lifted over that
    /// column, and the inner row over the list, whatever the ranks, so each draws over the list and keeps its
    /// touches, and the band beside each is the list's. A lifted row follows its sibling's rank as the parent
    /// is ranked. A `z-index` written on the column that ranks it over the rows refuses the band; one written
    /// on the list ranks it only within the column.
    func testABarIsLiftedOverTheBoxHoldingItsScroller() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bars and the safe area are a scene window's") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("control bars are iOS 26's, where UIKit draws the edge effect over them") }
        let (session, _) = try fixture("CONTROL_BARS")
        let p = session.presenter
        p.press(try node(session, "pick-nested").id)
        let top = try node(session, "nested-bar"), foot = try node(session, "nested-foot"), inner = try node(session, "nested-inner"),
            wrap = try node(session, "nested-wrap"), list = try node(session, "nested-list")
        let rows = [(top, wrap, try node(session, "nested-all")), (foot, wrap, try node(session, "nested-foot-all")),
                    (inner, list, try node(session, "nested-inner-mine"))]
        let pickers = try node(session, "pickers")
        until("the rows are bars over the list") { list.pageBand.top > 0 && list.pageBand.bottom > 0 }
        let window = try XCTUnwrap(top.window), scroller = try XCTUnwrap(list.scrollView)
        // The list at the origin; at the pickers' rank (as authored); at one no sibling of the outer rows has (a
        // `z-index: 1`'s); and the column ranked as a positioned box, the pickers' rank.
        for (wrapRank, listRank) in [(Int64(0), Int64(0)), (Int64(0), Int64(1)), (Int64(0), Int64(2)), (Int64(1), Int64(0))] {
            let ranks = "column \(wrapRank), list \(listRank)"
            wrap.setRank(wrapRank)
            list.setRank(listRank)
            XCTAssertGreaterThan(top.layer.zPosition, wrap.layer.zPosition, "\(ranks): the top row follows the column's rank at once")
            p.reachPageEdges()
            if listRank == 2 {
                XCTAssertFalse(try XCTUnwrap(top.superview).subviews.contains { ($0 as? NodeView)?.paintRank == 2 }, "no sibling of the outer rows has the list's rank")
            }
            XCTAssertGreaterThan(list.pageBand.top, 0, "\(ranks): the list still runs under the top rows")
            XCTAssertGreaterThan(list.pageBand.bottom, 0, "\(ranks): and under the foot's")
            for (row, over, button) in rows {
                let which = "\(ranks), \(button.props["testId"] ?? "")"
                XCTAssertGreaterThan(row.layer.zPosition, over.layer.zPosition, "\(which): the row draws over its sibling holding the list")
                if over === wrap, row.paintRank < pickers.paintRank {
                    XCTAssertLessThan(row.layer.zPosition, pickers.layer.zPosition, "\(which): and stays under a sibling that painted over the column")
                }
                let hit = try XCTUnwrap(window.hitTest(button.convert(CGPoint(x: button.bounds.midX, y: button.bounds.midY), to: nil), with: nil))
                XCTAssertTrue(hit.isDescendant(of: button), "\(which): a tap on the row's button is the button's, not \(type(of: hit))")
                let beside = try XCTUnwrap(window.hitTest(row.convert(CGPoint(x: -8, y: row.bounds.midY), to: nil), with: nil))
                XCTAssertTrue(beside.isDescendant(of: scroller), "\(which): the band beside the row is the list's scroll view's, not \(type(of: beside))")
            }
        }
        wrap.setRank(0)
        list.setRank(0)
        let wrapStyle = wrap.style
        wrap.style["written"] = .array([.string("z_index")])
        wrap.style["z_index"] = .number(5)
        wrap.setRank(10)
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, .zero, "a column its author ranks over its rows takes no band")
        // The column positioned, over the top row's rank by no `z-index`, and the list ranked under it by its own
        wrap.style = wrapStyle
        wrap.setRank(1)
        list.style["written"] = .array([.string("z_index")])
        list.style["z_index"] = .number(-1)
        list.setRank(-2)
        p.reachPageEdges()
        XCTAssertGreaterThan(list.pageBand.top, 0, "a list's own z-index ranks it only within its column")
        XCTAssertGreaterThan(top.layer.zPosition, wrap.layer.zPosition)
    }

    /// LLP 1131 D1: a control that scrolls or holds a shown scroller is content, not a bar. A pressable column
    /// holding a list, scrolling itself, or in a box with a button and a spacer drawn as a layer is no bar over
    /// the list below it, so that list takes no band through it; the preview's own scroller takes one through
    /// the row of buttons above, and the row keeps its touches. A list the column hides leaves it a bar, and a
    /// dialog up in its sheet is no longer the box's.
    func testAControlHoldingAScrollerIsNoBar() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bars and the safe area are a scene window's") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("control bars are iOS 26's, where UIKit draws the edge effect over them") }
        let (session, _) = try fixture("CONTROL_BARS")
        let p = session.presenter
        func shown(_ shape: String, _ scroller: String) throws -> (lower: NodeView, preview: NodeView) {
            p.press(try node(session, "pick-\(shape)").id)
            until("\(shape) is laid out") {
                [scroller, "lower-list"].allSatisfy { id in (try? node(session, id)).map { $0.frame.height > 0 && $0.scrollView != nil } == true }
            }
            p.reachPageEdges()
            return (try node(session, "lower-list"), try node(session, scroller))
        }
        for (shape, scroller) in [("preview", "preview-list"), ("preview-scrolls", "preview"), ("preview-wrapped", "preview-list")] {
            let (lower, preview) = try shown(shape, scroller)
            XCTAssertEqual(lower.pageBand.top, 0, "\(shape): no band through the pressable column above it")
            XCTAssertGreaterThan(preview.pageBand.top, 0, "\(shape): the preview's scroller is banded through the row")
            let all = try node(session, "preview-all"), window = try XCTUnwrap(all.window)
            let hit = try XCTUnwrap(window.hitTest(all.convert(CGPoint(x: all.bounds.midX, y: all.bounds.midY), to: nil), with: nil))
            XCTAssertTrue(hit.isDescendant(of: all), "\(shape): a tap on the row's button is the button's, not \(type(of: hit))")
        }
        p.press(try node(session, "pick-preview-hidden").id)
        let lower = try node(session, "lower-list"), column = try node(session, "preview")
        until("the hidden list's preview is laid out") { column.frame.height > 0 && lower.frame.height > 0 && lower.scrollView != nil }
        p.reachPageEdges()
        XCTAssertGreaterThan(lower.pageBand.top, column.frame.height, "a list the column hides leaves it a bar: the band runs through it and the row")
        // A dialog up in its sheet is no longer the row's: a row beside it, read through a spacer drawn as a layer,
        // stays a bar.
        p.press(try node(session, "pick-sheet-opener").id)
        until("the opener is laid out") { (try? node(session, "lower-list")).map { $0.frame.height > 0 && $0.scrollView != nil } == true }
        let opener = try node(session, "lower-list"), sheet = try node(session, "opener-sheet")
        p.reachPageEdges()
        let band = opener.pageBand.top
        XCTAssertGreaterThan(band, 0, "the row is a bar over the list")
        p.press(try node(session, "opener-open").id)
        until("the dialog is up in its sheet") { p.dialogs.isOpen(sheet) && !p.dialogs.inTransition }
        p.reachPageEdges()
        XCTAssertEqual(opener.pageBand.top, band, "the row stays a bar while its dialog is up")
        p.dialogs.close(sheet, animated: false)
        until("the sheet is gone") { !p.dialogs.owns(sheet) && !p.dialogs.inTransition }
        p.reachPageEdges()
        XCTAssertEqual(opener.pageBand.top, band, "and once the dialog is back in the row")
    }

    /// LLP 1131 D1: a scroll view that scrolls nothing makes no control content. A tablist UIKit draws as its
    /// segmented control, written `overflow-x: auto`, and a button whose label truncates (`overflow-x: hidden`
    /// gives the text a scroll view) are bars over the list.
    func testAScrollViewThatScrollsNothingLeavesItsControlABar() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the bars and the safe area are a scene window's") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("control bars are iOS 26's, where UIKit draws the edge effect over them") }
        let (session, _) = try fixture("CONTROL_BARS")
        let p = session.presenter
        for shape in ["tabs-scroll", "truncating"] {
            p.press(try node(session, "pick-\(shape)").id)
            // The scroll view that would have made the row no bar: the tablist's, behind UIKit's control, or the label's
            until("\(shape) is laid out with its scroll view") {
                guard let filter = try? node(session, "filter"), filter.frame.height > 0 else { return false }
                return shape == "tabs-scroll" ? filter.scrollView != nil && p.segments.projects(filter.id)
                    : (try? node(session, "filter-label"))?.scrollView != nil
            }
            p.reachPageEdges()
            XCTAssertGreaterThan(try node(session, "list").pageBand.top, 0, "\(shape): the list runs under the bar")
        }
    }

    func testLargeTitleRidersRequireBothEdgesAndTheBarsPaintOrder() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the title belongs to a scene window") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("control bars require iOS 26") }
        let (session, _) = try fixture("CONTROL_BARS")
        let p = session.presenter
        p.press(try node(session, "pick-large").id)
        let filter = try node(session, "filter"), list = try node(session, "list")
        p.apply(wireBatch([["op": "props", "id": Int(filter.id), "set": ["accessibilityRole": "group"]]]))
        let route = try XCTUnwrap(p.navigationHost.activeRoute)
        until("the filter is page content and its bar has settled") { route.scoped == nil && !Agent(session: session).nativeInFlight() }
        let nav = try XCTUnwrap(route.navigationController)
        XCTAssertTrue(route.collapseScroll === list)
        p.onCovers = nil
        let originalStyle = list.style, originalFrame = list.frame
        // Riders need both edges banded or open, and an edge whose effect the list hides is neither (LLP 1131 D2)
        for hide in ["top", "bottom", "none"] {
            p.navigationHost.prepareRoutes([route], in: nav)
            p.reachPageEdges()
            XCTAssertFalse(route.ridingBars.isEmpty, "the filter rides the title with both edges shown")
            if hide == "none" { list.style["scroll_edge_effect"] = .string("none") } else { list.style["scroll_edge_effect_hidden"] = .number(hide == "top" ? 1 : 4) }
            p.navigationHost.prepareRoutes([route], in: nav)
            p.reachPageEdges()
            XCTAssertTrue(route.ridingBars.isEmpty, "a hidden \(hide) keeps the filter from riding")
            XCTAssertEqual(filter.controlRide, 0)
            list.style = originalStyle
        }
        list.style["written"] = .array([.string("z_index")])
        list.style["z_index"] = .number(100)
        list.setRank(filter.paintRank + 100)
        for _ in 0..<2 {
            p.navigationHost.prepareRoutes([route], in: nav)
            p.reachPageEdges()
            XCTAssertNil(route.collapseScroll, "a scroller above its bar cannot drive collapse")
            XCTAssertTrue(route.ridingBars.isEmpty)
            XCTAssertEqual(list.pageBand.top, 0)
            XCTAssertEqual(filter.controlRide, 0)
        }
        list.style = originalStyle
        list.setRank(filter.paintRank - 1)
        list.frame = CGRect(x: originalFrame.minX, y: originalFrame.minY, width: originalFrame.width, height: originalFrame.height - 40)
        for _ in 0..<2 {
            p.navigationHost.prepareRoutes([route], in: nav)
            p.reachPageEdges()
            XCTAssertNil(route.collapseScroll, "an open opposite edge must reach the route's edge")
            XCTAssertTrue(route.ridingBars.isEmpty)
            XCTAssertEqual(list.pageBand.top, 0)
        }
    }

    /// A page's scroller at the area's top over a control bar runs under the bar to the area's foot (LLP 1131 D2), but
    /// not with its top hidden: an edge whose effect the scroller hides is neither banded nor open at the area's edge
    /// too, so the foot keeps its place, and the top keeps the status bar's margin (LLP 1130), whatever its effect. The
    /// same holds at the area's foot under a bar.
    func testAPageScrollerHidingAnEdgeKeepsItsPlaceBesideABar() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the status bar and the safe area are a scene window's") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("control bars require iOS 26") }
        let (session, view) = try fixture()
        let p = session.presenter, list = try node(session, "list"), safe = view.safeAreaInsets
        // Not `node`: its XCTUnwrap records a failure while a bar is on its way or gone
        let shown = { (testId: String) in p.views.values.contains { $0.props["testId"] == testId && $0.window != nil } }
        p.press(try node(session, "foot-toggle").id)
        until("the foot is a bar under the list") { shown("foot") && list.pageBand.bottom > safe.bottom + 0.5 }
        let banded = list.pageBand
        XCTAssertEqual(banded.top, safe.top, accuracy: 0.5)
        list.style["scroll_edge_effect_hidden"] = .number(1)
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, UIEdgeInsets(top: safe.top, left: 0, bottom: 0, right: 0))
        list.style["scroll_edge_effect_hidden"] = nil
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, banded)
        p.press(try node(session, "foot-toggle").id)
        p.press(try node(session, "top-toggle").id)
        until("the list is at the area's foot under a bar") { !shown("foot") && shown("top") && list.pageBand.top > safe.top + 0.5 }
        let under = list.pageBand
        list.style["scroll_edge_effect_hidden"] = .number(4)
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, UIEdgeInsets(top: 0, left: 0, bottom: safe.bottom, right: 0), "a hidden foot keeps its place too")
        list.style["scroll_edge_effect_hidden"] = nil
        p.reachPageEdges()
        XCTAssertEqual(list.pageBand, under)
    }

    /// A route that opts out of its sides (`-exact-safe-area="left right"`, LLP 1123 D4) lays its content
    /// out to its edges there, though its host's cover holds them (a split's floating sidebar): its
    /// scroller spans the room its bars leave, so they stay its bars, as they do with no cover at all.
    func testARouteOptedOutOfItsSidesKeepsItsBars() {
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "button"],
            ["op": "create", "id": 4, "kind": "scroll", "props": ["flexGrows": "true"]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 1, "ids": [2, 4]],
            ["op": "roots", "ids": [1]]
        ]))
        let route = p.views[1]!, bar = p.views[2]!, list = p.views[4]!
        route.frame = CGRect(x: 0, y: 0, width: 951, height: 669)
        bar.frame = CGRect(x: 0, y: 82, width: 951, height: 51)
        list.frame = CGRect(x: 0, y: 133, width: 951, height: 502)
        p.navigationHost.covers[route.id] = .edges(.init(top: 82, right: 84, bottom: 34, left: 320))
        XCTAssertNil(ControlBars(p).bars(of: list, in: route, top: true), "a scroller wider than the cleared room is no bar's")
        route.style["safe_area"] = .number(Double(2 | 8))
        XCTAssertEqual(ControlBars(p).bars(of: list, in: route, top: true).map { $0.map(\.id) }, [bar.id])
        // One side: the scroller reaches the left edge it opts out of and stops at the right cover it keeps
        list.frame = CGRect(x: 0, y: 133, width: 951 - 84, height: 502)
        route.style["safe_area"] = .number(8)
        XCTAssertEqual(ControlBars(p).bars(of: list, in: route, top: true).map { $0.map(\.id) }, [bar.id])
        route.style["safe_area"] = .number(2)
        XCTAssertNil(ControlBars(p).bars(of: list, in: route, top: true), "the left cover it keeps is not its room")
        // The foot: open at the bottom cover it keeps, short of the route's edge it opts out of
        XCTAssertEqual(ControlBars(p).bars(of: list, in: route, top: false).map { $0.map(\.id) }, [])
        route.style["safe_area"] = .number(Double(2 | 4))
        XCTAssertNil(ControlBars(p).bars(of: list, in: route, top: false))
    }

    /// A route padding its left by `env(safe-area-inset-left)`, 59, beside a 59-point cover, as
    /// in landscape or beside an open Duo's sidebar: layout pads that edge by the larger of the
    /// two (LLP 1123 D3), so its list starts at 59, where the room its bars leave starts too.
    func testARoutePaddedByItsOwnInsetKeepsItsBars() {
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "create", "id": 3, "kind": "button"],
            ["op": "create", "id": 4, "kind": "scroll", "props": ["flexGrows": "true"]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 1, "ids": [2, 4]],
            ["op": "roots", "ids": [1]]
        ]))
        let route = p.views[1]!, bar = p.views[2]!, list = p.views[4]!
        route.frame = CGRect(x: 0, y: 0, width: 951, height: 669)
        route.applyKernelContentBox(route.bounds.inset(by: UIEdgeInsets(top: 0, left: 59, bottom: 0, right: 0)), padding: [0, 0, 0, 59])
        bar.frame = CGRect(x: 59, y: 82, width: 892, height: 51)
        list.frame = CGRect(x: 59, y: 133, width: 892, height: 502)
        p.navigationHost.covers[route.id] = .edges(.init(top: 82, right: 0, bottom: 34, left: 59))
        route.style["inset_credit"] = .number(8)
        XCTAssertEqual(ControlBars(p).bars(of: list, in: route, top: true).map { $0.map(\.id) }, [bar.id])
    }

    func testAControlBarMayIncludeACaptionButACaptionAloneIsNotABar() {
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "textarea"],
            ["op": "create", "id": 3, "kind": "text", "props": ["text": "Messages are saved locally"]],
            ["op": "children", "id": 1, "ids": [2, 3]],
            ["op": "create", "id": 4, "kind": "view"],
            ["op": "create", "id": 5, "kind": "text", "props": ["text": "Signed in as Ada"]],
            ["op": "children", "id": 4, "ids": [5]],
            ["op": "roots", "ids": [1, 4]]
        ]))
        XCTAssertTrue(ControlBars(p).isBar(p.views[1]!))
        XCTAssertFalse(ControlBars(p).isBar(p.views[4]!))
        p.views[1]?.style["written"] = .array([.string("background_color")])
        XCTAssertFalse(ControlBars(p).isBar(p.views[1]!))
    }

    /// #223: under `viewport-fit="cover"` a tab's route keeps the tab bar's cover, and its
    /// last scroller, with nothing under it, is drawn on under the bar and inset by the gap,
    /// as a hand-built table runs under its tab bar: layout is unchanged, its end rests where
    /// its box ends, UIKit adds no inset of its own and the bar's edge effect follows it. The
    /// route clears the status bar itself and its scroller runs under that too (LLP 1123 D3);
    /// the route's own `env()` padding counts once, inside the cover; a caption under the
    /// scroller keeps it above the bar; content that fits gains no scroll range; a virtualized
    /// list is banded.
    func testUnderACoverRootARoutesLastScrollerRunsUnderTheTabBar() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "the tab bar and the safe area are a scene window's") }
        guard #available(iOS 26.0, *) else { throw XCTSkip("control bars are iOS 26's, where UIKit draws the edge effect over them") }
        let (session, view) = try fixture("COVER_TABS")
        let p = session.presenter, routeNode = try node(session, "route-home")
        let route = try XCTUnwrap(p.navigationHost.controllers[routeNode.id])
        let foot = { routeNode.convert(routeNode.bounds, to: nil).maxY }
        // Not `node`: its XCTUnwrap records a failure while a variant is on its way.
        let list = { p.views.values.first { $0.props["testId"] == "list" && $0.window != nil } }
        until("the scroller runs under the tab bar") { (list()?.pageBand.bottom ?? 0) > 0.5 }
        guard case .edges(let cover)? = p.navigationHost.covers[routeNode.id] else { return XCTFail("the route keeps the tab bar's cover") }
        XCTAssertGreaterThan(cover.bottom, 0.5)
        XCTAssertEqual(cover.top, view.safeAreaInsets.top, accuracy: 0.5, "the route clears the status bar itself")
        var v = try XCTUnwrap(list()), sv = try XCTUnwrap(v.scrollView)
        let box = v.convert(v.bounds, to: nil)
        XCTAssertEqual(box.maxY, foot() - cover.bottom, accuracy: 0.5, "laid out above the cover, as before")
        XCTAssertEqual(v.pageBand, UIEdgeInsets(top: cover.top, left: 0, bottom: cover.bottom, right: 0), "under the status bar too, as a tab's table runs")
        XCTAssertEqual(sv.convert(sv.bounds, to: nil).maxY, foot(), accuracy: 0.5, "its scroll view reaches the route's foot, under the bar")
        XCTAssertEqual(sv.adjustedContentInset.bottom, cover.bottom, accuracy: 0.5, "inset by the band alone")
        XCTAssertTrue(route.contentScrollView(for: .bottom) === sv, "the tab bar's edge effect follows it")
        // A hidden top keeps its place, and the open foot keeps its band over the tab bar (LLP 1131 D2)
        let banded = v.pageBand, shown = v.style
        v.style["scroll_edge_effect_hidden"] = .number(1)
        p.reachPageEdges()
        XCTAssertEqual(v.pageBand, UIEdgeInsets(top: 0, left: 0, bottom: cover.bottom, right: 0), "a hidden top keeps its place over the tab bar")
        v.style = shown
        p.reachPageEdges()
        XCTAssertEqual(v.pageBand, banded)
        sv.contentOffset.y = sv.contentSize.height + sv.adjustedContentInset.bottom - sv.bounds.height
        let end = try node(session, "end")
        XCTAssertEqual(end.convert(end.bounds, to: nil).maxY, box.maxY, accuracy: 0.5, "at its end the content rests where the box ends, above the bar")

        p.press(try node(session, "use-padded").id)
        until("the route pads its bottom by its own inset") { ((routeNode.kernelPadding?[2] ?? nil) ?? 0) > 0.5 && list() != nil }
        v = try XCTUnwrap(list()); sv = try XCTUnwrap(v.scrollView)
        XCTAssertEqual(v.convert(v.bounds, to: nil).maxY, foot() - cover.bottom, accuracy: 0.5, "the padding counts once, inside the cover")
        XCTAssertEqual(v.pageBand.bottom, cover.bottom, accuracy: 0.5)
        XCTAssertEqual(sv.convert(sv.bounds, to: nil).maxY, foot(), accuracy: 0.5)
        XCTAssertEqual(sv.adjustedContentInset.bottom, v.pageBand.bottom, accuracy: 0.5, "the padding and the cover, once")

        p.press(try node(session, "use-footer").id)
        until("a caption under the scroller keeps it above the bar") { list()?.pageBand.bottom == 0 }
        XCTAssertEqual(list()?.pageBand, UIEdgeInsets(top: cover.top, left: 0, bottom: 0, right: 0))
        XCTAssertNil(route.bandedBottom)

        p.press(try node(session, "use-short").id)
        until("content that fits is banded") { (list()?.pageBand.bottom ?? 0) > 0.5 }
        v = try XCTUnwrap(list()); sv = try XCTUnwrap(v.scrollView)
        let i = sv.adjustedContentInset
        XCTAssertLessThanOrEqual(sv.contentSize.height + i.top + i.bottom, sv.bounds.height + 0.5, "and gains no scroll range")

        p.press(try node(session, "use-list").id)
        until("a virtualized list is banded") {
            guard let l = list(), l.scrollView != nil, p.listHost.entries[l.id] != nil else { return false }
            return abs(l.pageBand.bottom - cover.bottom) < 0.5
        }
    }
}
#endif
