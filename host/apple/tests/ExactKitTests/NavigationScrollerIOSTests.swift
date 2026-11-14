#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

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

    private func fixture(_ scene: UIWindowScene) throws -> ExactSession {
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
        XCTAssertNil(session.boot(plan: try fixturePlan("NAVIGATION_SCROLLER"), size: view.bounds.size).error)
        view.layoutIfNeeded()
        settle(session)
        return session
    }

    private func settle(_ session: ExactSession) {
        waitFor("navigation settles") {
            session.presenter.navigation.activeRoute != nil && !Agent(session: session).nativeInFlight()
        }
    }

    private func node(_ session: ExactSession, _ name: String) throws -> NodeView {
        let route = try XCTUnwrap(session.presenter.navigation.activeRoute?.node)
        return try XCTUnwrap(session.presenter.views.values.first {
            $0.props["testId"] == name && $0.isDescendant(of: route)
        }, name)
    }

    private func press(_ session: ExactSession, _ name: String) throws {
        session.presenter.press(try node(session, name).id)
        settle(session)
    }

    func testTheContainerKeepsItsAuthoredFrameAndScrollsWithItsOuterScroller() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "authored navigation scrolling needs UIKit containment") }
        let session = try fixture(scene)
        let p = session.presenter, owner = try XCTUnwrap(p.navigation.primaryOwner)
        let root = try XCTUnwrap(p.navigation.container), outer = try XCTUnwrap(root.scroll)
        XCTAssertTrue(owner.view.isDescendant(of: outer), "the authored scroller holds the native container")
        XCTAssertEqual(owner.view.bounds.size, root.bounds.size)
        XCTAssertEqual(owner.view.bounds.width, 320, accuracy: 0.5)
        XCTAssertEqual(owner.view.bounds.height, 620, accuracy: 0.5)
        let before = owner.view.convert(owner.view.bounds, to: window)
        outer.contentOffset.y = 120
        p.navigation.fitOwner()
        XCTAssertEqual(owner.view.convert(owner.view.bounds, to: window).minY, before.minY - 120, accuracy: 0.5)
        XCTAssertEqual(owner.view.frame.origin, .zero, "refitting must not pin the container over scrolling content")
        XCTAssertGreaterThan(outer.contentSize.height, outer.bounds.height)
        let route = try XCTUnwrap(p.navigation.activeRoute)
        waitFor("the runner receives the scrolled route placement") {
            guard let placed = p.owners.told[route.node.id]?.placement?.rect else { return false }
            let actual = route.view.convert(route.view.bounds, to: root)
            return abs(placed.minY - actual.minY) < 0.5
        }
        outer.contentOffset = .zero
        try press(session, "contain")
        XCTAssertNil(root.scroll)
        XCTAssertTrue(p.navigation.primaryOwner === owner)
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
        let p = session.presenter, nav = try XCTUnwrap(p.navigation.navigation)
        let root = try XCTUnwrap(p.navigation.container), outer = try XCTUnwrap(root.scroll)
        XCTAssertTrue(nav.view.isDescendant(of: outer))
        let frame = nav.view.frame
        let home = try XCTUnwrap(nav.topViewController)
        try press(session, "open-detail")
        XCTAssertEqual(nav.viewControllers.count, 2)
        XCTAssertEqual(p.navigation.activeRoute?.node.props["testId"], "route-detail")
        nav.popViewController(animated: false)
        settle(session)
        XCTAssertTrue(nav.topViewController === home)
        XCTAssertEqual(p.navigation.activeRoute?.node.props["testId"], "route-home")
        try press(session, "open-sheet")
        let sheet = try XCTUnwrap(p.navigation.modalNavigation)
        XCTAssertNotNil(sheet.view.window)
        XCTAssertFalse(sheet.view.isDescendant(of: outer), "a presentation escapes authored scrolling")
        XCTAssertEqual(nav.view.frame, frame)
        let close = try XCTUnwrap(session.presenter.views.values.first { $0.props["id"] == "back" })
        p.press(close.id)
        settle(session)
        XCTAssertNil(p.navigation.modalNavigation)
        XCTAssertTrue(nav.view.isDescendant(of: outer))
        XCTAssertEqual(nav.view.frame, frame)
    }

    func testASidebarPaintsAndTakesTouchesWhenTheRoutesMoveClearOfIt() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "authored navigation scrolling needs UIKit containment") }
        let session = try fixture(scene)
        let p = session.presenter, owner = try XCTUnwrap(p.navigation.primaryOwner)
        let root = try XCTUnwrap(p.navigation.container), outer = try XCTUnwrap(root.scroll)
        let sidebar = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "sidebar" })
        let route = try XCTUnwrap(p.navigation.activeRoute)
        XCTAssertTrue(sidebar.superview === outer && owner.view.superview === outer)
        XCTAssertEqual(sidebar.bounds.width, 80, accuracy: 0.5)
        XCTAssertEqual(route.node.convert(route.node.bounds, to: root).minX, 0, accuracy: 0.5)
        XCTAssertFalse(sidebar.clearsNavigation)
        XCTAssertLessThanOrEqual(sidebar.layer.zPosition, owner.view.layer.zPosition)

        try press(session, "inset")
        XCTAssertEqual(route.node.convert(route.node.bounds, to: root).minX, 80, accuracy: 0.5)
        XCTAssertTrue(sidebar.clearsNavigation)
        XCTAssertGreaterThan(sidebar.layer.zPosition, owner.view.layer.zPosition, "the sidebar paints above the native container")
        let point = sidebar.convert(CGPoint(x: sidebar.bounds.midX, y: sidebar.bounds.midY), to: window)
        let hit = try XCTUnwrap(window?.hitTest(point, with: nil))
        XCTAssertTrue(hit.isDescendant(of: sidebar), "the sidebar takes the touch: \(type(of: hit))")
        let reply = Agent(session: session).tap(["id": Int(sidebar.id)])
        XCTAssertEqual(reply["pressed"] as? Int, Int(sidebar.id), "\(reply)")
        settle(session)
        XCTAssertEqual(route.node.convert(route.node.bounds, to: root).minX, 0, accuracy: 0.5)
        XCTAssertFalse(sidebar.clearsNavigation)
        XCTAssertLessThanOrEqual(sidebar.layer.zPosition, owner.view.layer.zPosition, "overlapping restores the sidebar's authored paint order")
        let covered = try XCTUnwrap(window?.hitTest(point, with: nil))
        XCTAssertFalse(covered.isDescendant(of: sidebar))
    }

    func testSearchAndKeyboardAvoidanceStayInsideTheAuthoredColumn() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "authored navigation scrolling needs UIKit containment") }
        let session = try fixture(scene)
        let p = session.presenter
        try press(session, "open-find")
        let route = try XCTUnwrap(p.navigation.activeRoute), nav = try XCTUnwrap(p.navigation.navigation)
        let outer = try XCTUnwrap(p.navigation.container?.scroll)
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
