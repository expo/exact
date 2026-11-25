#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

final class NavigationScrollerIOSTests: XCTestCase {
    private var session: ExactSession?
    private var window: UIWindow?

    override func tearDown() {
        window?.endEditing(true)
        session?.destroy()
        session = nil
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    private func fixture(_ scene: UIWindowScene, plan: String = "NAVIGATION_SCROLLER") throws -> ExactSession {
        let session = ExactApp.shared.makeSession(label: "navigation-scroller")
        self.session = session
        let view = ExactView(session: session), host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        self.window = window
        view.frame = host.view.bounds
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        host.view.addSubview(view)
        XCTAssertNil(session.boot(plan: try fixturePlan(plan), size: view.bounds.size).error)
        view.layoutIfNeeded()
        settle(session)
        return session
    }

    private func settle(_ session: ExactSession) {
        waitFor("navigation settles") {
            session.presenter.navigationHost.activeRoute != nil && !Agent(session: session).nativeInFlight()
        }
    }

    private func node(_ session: ExactSession, _ name: String) throws -> NodeView {
        let route = try XCTUnwrap(session.presenter.navigationHost.activeRoute?.node)
        return try XCTUnwrap(session.presenter.views.values.first {
            $0.props["testId"] == name && $0.isDescendant(of: route)
        }, name)
    }

    private func press(_ session: ExactSession, _ name: String) throws {
        session.presenter.press(try node(session, name).id)
        settle(session)
    }

    func testBottomToolbarFollowsTheLiveRowsOnScreen() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit places the route toolbar in a window") }
        let session = try fixture(scene, plan: "BOTTOM_TOOLBAR_LIVE"), navigation = session.presenter.navigationHost
        let route = try XCTUnwrap(navigation.activeRoute), nav = try XCTUnwrap(route.navigationController)
        func tap(_ name: String) throws {
            let id = try node(session, name).id
            XCTAssertNil(Agent(session: session).tap(["id": Int(id)])["error"], name)
        }
        XCTAssertNil(navigation.split)
        XCTAssertTrue(try node(session, "load-tools").isHidden)
        let initial = try XCTUnwrap(route.toolbarItems)
        XCTAssertEqual(initial.count, 2)
        XCTAssertNil(initial.first?.action, "a spacer with a button role still makes a flexible space")
        XCTAssertNotNil(initial.last?.frame(in: route.view))
        try tap("load")
        let line = "route \(route.key) has multiple bottom toolbars"
        func notices() -> Int { session.agent(#"{"op":"logs","since":0}"#).components(separatedBy: line).count - 1 }
        for (name, rows) in [("equal", 1), ("unequal", 1), ("filtered", 1), ("unequal", 2)] {
            waitFor("one toolbar item for \(name)") {
                route.toolbarItems?.count == 1 && route.toolbarItems?.first?.accessibilityIdentifier == name
            }
            let item = try XCTUnwrap(route.toolbarItems?.first)
            XCTAssertNotNil(item.frame(in: route.view), "UIKit presents the route's item")
            XCTAssertEqual(route.liftedBottomRows.count, rows)
            XCTAssertEqual(notices(), rows == 2 ? 1 : 0)
            if rows == 2 {
                XCTAssertEqual(route.liftedBottomRows.map { $0.props["testId"] }, ["unequal-tools", "loop-tools"])
                XCTAssertTrue(route.liftedBottomRows.allSatisfy(\.isHidden))
            }
            try tap(name)
        }
        waitFor("only the spacer row is live") { route.liftedBottomRows.first?.props["testId"] == "space-tools" }
        let space = try XCTUnwrap(route.bottomToolbar?.barSpaces.values.first)
        XCTAssertEqual(route.toolbarItems?.count, 1)
        XCTAssertTrue(route.toolbarItems?.first === space)
        XCTAssertFalse(nav.isToolbarHidden)
        XCTAssertEqual(notices(), 1)
        try tap("advance")
        waitFor("removing the final row clears its items") { route.liftedBottomRows.isEmpty && (route.toolbarItems ?? []).isEmpty }
        XCTAssertTrue(nav.isToolbarHidden)
    }

    func testBottomToolbarBelongsToItsRouteAndClearsItsLastRow() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit places the tab bar and toolbar in a window") }
        let session = try fixture(scene, plan: "BOTTOM_TOOLBAR"), navigation = session.presenter.navigationHost
        let route = try XCTUnwrap(navigation.activeRoute), nav = try XCTUnwrap(route.navigationController)
        let items = try XCTUnwrap(route.toolbarItems)
        XCTAssertFalse(nav.isToolbarHidden)
        XCTAssertEqual(items.count, 3)
        let star = try XCTUnwrap(items.first), add = try XCTUnwrap(items.last)
        XCTAssertEqual(star.accessibilityIdentifier, "favorite")
        XCTAssertEqual(star.accessibilityLabel, "Favorite")
        XCTAssertNotNil(star.image)
        XCTAssertEqual(add.accessibilityIdentifier, "add")
        XCTAssertEqual(add.title, "Add")
        let traits = route.traitCollection
        XCTAssertEqual(try XCTUnwrap(add.tintColor).resolvedColor(with: traits), try XCTUnwrap(route.node.ownAccent).resolvedColor(with: traits))
        for item in [star, add] {
            XCTAssertTrue(UIApplication.shared.sendAction(try XCTUnwrap(item.action), to: item.target, from: item, for: nil))
        }
        let counts = try node(session, "counts")
        waitFor("both toolbar actions reach the route") { counts.accessibleText == "Favorites 1, additions 1" && route.toolbarItems?.last?.isEnabled == false && route.toolbarItems?.first?.menu != nil }
        XCTAssertTrue(try node(session, "tools").isHidden)
        XCTAssertTrue(try XCTUnwrap(route.toolbarItems?.last).accessibilityTraits.contains(.notEnabled))
        let scroll = try XCTUnwrap(try node(session, "content").scrollView)
        let last = try node(session, "row-30")
        scroll.setContentOffset(CGPoint(x: 0, y: max(0, scroll.contentSize.height - scroll.bounds.height + scroll.adjustedContentInset.bottom)), animated: false)
        let starBox = try XCTUnwrap(route.toolbarItems?.first?.frame(in: route.view)), addBox = try XCTUnwrap(route.toolbarItems?.last?.frame(in: route.view))
        if route.traitCollection.userInterfaceIdiom == .pad {
            XCTAssertGreaterThan(min(starBox.midY, addBox.midY), route.view.bounds.midY)
            XCTAssertLessThan(starBox.midX, route.view.bounds.width / 4)
            XCTAssertGreaterThan(addBox.midX, route.view.bounds.width * 3 / 4)
        } else if !nav.navigationBar.traitCollection.placesVerticalBar {
            let tabs = try XCTUnwrap(navigation.tabController?.tabBar)
            let agent = Agent(session: session)
            for item in try XCTUnwrap(tabs.items) {
                let control = try XCTUnwrap(agent.drawnTabControl(item, in: tabs))
                let button = control.convert(control.bounds, to: route.view)
                XCTAssertGreaterThan(button.midX, starBox.maxX)
                XCTAssertLessThan(button.midX, addBox.minX)
                XCTAssertEqual(button.midY, starBox.midY, accuracy: 2)
            }
        }
        let lastBox = last.convert(last.bounds, to: route.view)
        // On the Duo one item can be in the rail beside the list.
        let overList = [starBox, addBox].filter { $0.minX < lastBox.maxX && lastBox.minX < $0.maxX }
        let visibleBottom = try XCTUnwrap(overList.map(\.minY).min())
        let lastBottom = lastBox.maxY
        XCTAssertLessThanOrEqual(lastBottom, visibleBottom + 1)
        XCTAssertEqual(lastBottom, route.view.safeAreaLayoutGuide.layoutFrame.maxY, accuracy: 1, "the last row scrolls to UIKit's safe area")
        scroll.setContentOffset(CGPoint(x: 0, y: -scroll.adjustedContentInset.top), animated: false)
        try press(session, "detail-link")
        XCTAssertNil(nav.topViewController?.toolbarItems)
        XCTAssertTrue(nav.isToolbarHidden)
        nav.popViewController(animated: true)
        settle(session)
        XCTAssertTrue(nav.topViewController === route)
        XCTAssertFalse(nav.isToolbarHidden)
        XCTAssertEqual(route.toolbarItems?.map(\.accessibilityIdentifier), ["favorite", nil, "add"])
        XCTAssertNotNil(route.toolbarItems?.first?.menu)
        XCTAssertEqual(route.toolbarItems?.last?.isEnabled, false)
    }

    func testACompletedProjectedTransitionDoesNotMakeContentUpdatesOweNavigation() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "native navigation completion needs a scene") }
        let session = try fixture(scene), p = session.presenter
        try press(session, "open-detail")
        let navigation = p.navigationHost
        let stack = try XCTUnwrap(navigation.navigation), top = try XCTUnwrap(stack.topViewController)
        let before = navigation.syncCalls
        navigation.navigationController(stack, didShow: top, animated: false)
        XCTAssertNil(navigation.restDifference)
        XCTAssertFalse(navigation.syncOwed, "UIKit finished the stack already projected")
        let field = try node(session, "field")
        let batch = wireBatch([["op": "style", "id": Int(field.id), "style": ["opacity": 0.9]]])
        XCTAssertTrue(p.onlyListRows(batch))
        p.apply(batch)
        XCTAssertEqual(navigation.syncCalls, before)
        stack.popViewController(animated: true)
        settle(session)
        XCTAssertEqual(navigation.activeRoute?.node.props["testId"], "route-home", "native Back still reaches the router")
        XCTAssertNil(navigation.restDifference)
    }

    func testTheContainerKeepsItsAuthoredFrameAndScrollsWithItsOuterScroller() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "authored navigation scrolling needs UIKit containment") }
        let session = try fixture(scene)
        let p = session.presenter, owner = try XCTUnwrap(p.navigationHost.primaryOwner)
        let root = try XCTUnwrap(p.navigationHost.container), outer = try XCTUnwrap(root.scroll)
        XCTAssertTrue(owner.view.isDescendant(of: outer), "the authored scroller holds the native container")
        XCTAssertEqual(owner.view.bounds.size, root.bounds.size)
        XCTAssertEqual(owner.view.bounds.width, 320, accuracy: 0.5)
        XCTAssertEqual(owner.view.bounds.height, 620, accuracy: 0.5)
        let before = owner.view.convert(owner.view.bounds, to: window)
        outer.contentOffset.y = 120
        p.navigationHost.fitOwner()
        XCTAssertEqual(owner.view.convert(owner.view.bounds, to: window).minY, before.minY - 120, accuracy: 0.5)
        XCTAssertEqual(owner.view.frame.origin, .zero, "refitting must not pin the container over scrolling content")
        XCTAssertGreaterThan(outer.contentSize.height, outer.bounds.height)
        let route = try XCTUnwrap(p.navigationHost.activeRoute)
        waitFor("the runner receives the scrolled route placement") {
            guard let placed = p.owners.told[route.node.id]?.placement?.rect else { return false }
            let actual = route.view.convert(route.view.bounds, to: root)
            return abs(placed.minY - actual.minY) < 0.5
        }
        outer.contentOffset = .zero
        try press(session, "contain")
        XCTAssertNil(root.scroll)
        XCTAssertTrue(p.navigationHost.primaryOwner === owner)
        XCTAssertTrue(owner.view.isDescendant(of: root))
        XCTAssertEqual(owner.view.convert(owner.view.bounds, to: p.viewport), p.viewport.pageBounds)
        try press(session, "contain")
        let recreated = try XCTUnwrap(root.scroll)
        XCTAssertTrue(owner.view.isDescendant(of: recreated))
        XCTAssertEqual(owner.view.bounds.size, root.bounds.size)
    }

    func testPushPopAndSheetsKeepTheAuthoredContainer() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "authored navigation scrolling needs UIKit containment") }
        let session = try fixture(scene)
        let p = session.presenter, nav = try XCTUnwrap(p.navigationHost.navigation)
        let root = try XCTUnwrap(p.navigationHost.container), outer = try XCTUnwrap(root.scroll)
        XCTAssertTrue(nav.view.isDescendant(of: outer))
        let frame = nav.view.frame
        let home = try XCTUnwrap(nav.topViewController)
        try press(session, "open-detail")
        XCTAssertEqual(nav.viewControllers.count, 2)
        XCTAssertEqual(p.navigationHost.activeRoute?.node.props["testId"], "route-detail")
        nav.popViewController(animated: false)
        settle(session)
        XCTAssertTrue(nav.topViewController === home)
        XCTAssertEqual(p.navigationHost.activeRoute?.node.props["testId"], "route-home")
        try press(session, "open-sheet")
        let sheet = try XCTUnwrap(p.navigationHost.modalNavigation)
        XCTAssertNotNil(sheet.view.window)
        XCTAssertFalse(sheet.view.isDescendant(of: outer), "a presentation escapes authored scrolling")
        XCTAssertEqual(nav.view.frame, frame)
        let close = try XCTUnwrap(session.presenter.views.values.first { $0.props["id"] == "back" })
        p.press(close.id)
        settle(session)
        XCTAssertNil(p.navigationHost.modalNavigation)
        XCTAssertTrue(nav.view.isDescendant(of: outer))
        XCTAssertEqual(nav.view.frame, frame)
    }

    func testASidebarPaintsAndTakesTouchesWhenTheRoutesMoveClearOfIt() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "authored navigation scrolling needs UIKit containment") }
        let session = try fixture(scene)
        let p = session.presenter, owner = try XCTUnwrap(p.navigationHost.primaryOwner)
        let root = try XCTUnwrap(p.navigationHost.container), outer = try XCTUnwrap(root.scroll)
        let sidebar = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "sidebar" })
        let route = try XCTUnwrap(p.navigationHost.activeRoute)
        XCTAssertTrue(sidebar.superview === outer && owner.view.superview === outer)
        XCTAssertEqual(sidebar.bounds.width, 80, accuracy: 0.5)
        XCTAssertEqual(route.node.convert(route.node.bounds, to: root).minX, 0, accuracy: 0.5)
        XCTAssertEqual(sidebar.navigationPaint, .sibling)
        XCTAssertLessThanOrEqual(sidebar.layer.zPosition, owner.view.layer.zPosition)

        try press(session, "inset")
        XCTAssertEqual(route.node.convert(route.node.bounds, to: root).minX, 80, accuracy: 0.5)
        XCTAssertEqual(sidebar.navigationPaint, .besideRoutes)
        XCTAssertGreaterThan(sidebar.layer.zPosition, owner.view.layer.zPosition, "the sidebar paints above the native container")
        let point = sidebar.convert(CGPoint(x: sidebar.bounds.midX, y: sidebar.bounds.midY), to: window)
        let hit = try XCTUnwrap(window?.hitTest(point, with: nil))
        XCTAssertTrue(hit.isDescendant(of: sidebar), "the sidebar takes the touch: \(type(of: hit))")
        let reply = Agent(session: session).tap(["id": Int(sidebar.id)])
        XCTAssertEqual(reply["pressed"] as? Int, Int(sidebar.id), "\(reply)")
        settle(session)
        XCTAssertEqual(route.node.convert(route.node.bounds, to: root).minX, 0, accuracy: 0.5)
        XCTAssertEqual(sidebar.navigationPaint, .sibling)
        XCTAssertLessThanOrEqual(sidebar.layer.zPosition, owner.view.layer.zPosition, "overlapping restores the sidebar's authored paint order")
        let covered = try XCTUnwrap(window?.hitTest(point, with: nil))
        XCTAssertFalse(covered.isDescendant(of: sidebar))
    }

    func testSearchAndKeyboardAvoidanceStayInsideTheAuthoredColumn() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "authored navigation scrolling needs UIKit containment") }
        let session = try fixture(scene)
        let p = session.presenter
        try press(session, "open-find")
        let route = try XCTUnwrap(p.navigationHost.activeRoute), nav = try XCTUnwrap(p.navigationHost.navigation)
        let outer = try XCTUnwrap(p.navigationHost.container?.scroll)
        XCTAssertTrue(nav.view.isDescendant(of: outer))
        XCTAssertFalse(nav.isNavigationBarHidden)
        let search = try XCTUnwrap(route.search?.controller)
        search.isActive = true
        let field = search.searchBar.searchTextField
        XCTAssertTrue(field.becomeFirstResponder())
        waitFor("the search keyboard appears") { p.keyboardTop != nil && field.isFirstResponder }
        let column = nav.view.convert(nav.view.bounds, to: window)
        let bar = search.searchBar.convert(search.searchBar.bounds, to: window)
        XCTAssertGreaterThanOrEqual(bar.minX, column.minX - 1)
        XCTAssertLessThanOrEqual(bar.maxX, column.maxX + 1)
        XCTAssertEqual(nav.view.bounds.width, 320, accuracy: 0.5)
        search.isActive = false
        field.resignFirstResponder()
        waitFor("search stops editing") { !field.isFirstResponder }
        settle(session)
        let editorNode = try node(session, "field"), editor = try XCTUnwrap(editorNode.field)
        XCTAssertTrue(editor.becomeFirstResponder())
        waitFor("the authored editor is revealed above the keys") {
            guard let top = p.keyboardTop else { return false }
            let rect = editor.convert(editor.bounds, to: self.window)
            return editor.isFirstResponder && rect.minY >= 0 && rect.maxY <= top + 1
        }
        XCTAssertTrue(nav.view.isDescendant(of: outer))
        XCTAssertEqual(nav.view.bounds.size, CGSize(width: 320, height: 620))
        editor.resignFirstResponder()
    }
}
#endif
