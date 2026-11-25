#if os(iOS)
import UIKit
import XCTest
@testable import ExactDialogs
@testable import ExactKit
@testable import ExactNavigation

extension NavigationBasicsIOSTests {
    func setCoveredBackHistoryTitle(_ session: ExactSession, route: RouteController) throws {
        let photo = try node(session, "photo-back")
        let heading = try node(session, "title-chat")
        session.presenter.apply(wireBatch([
            ["op": "props", "id": photo.id, "clear": ["id"]],
            ["op": "props", "id": heading.id, "set": ["text": ""]],
            ["op": "props", "id": route.node.id, "set": ["headTitle": "Conversation"]]
        ]))
    }

    func testLocalizedBackHistorySurvivesPlanHandoffAndClears() throws {
        let session = try fixture("localized-back-history", plan: "EXACT_NAVIGATION_BARS_PLAN")
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        let start = try XCTUnwrap(nav.topViewController as? RouteController)
        let item = start.navigationItem
        func setLocale(_ locale: String) {
            session.launchPlace = LaunchPlace(environment: [
                "EXACT_AGENT": "1", "EXACT_AGENT_LOCALE": locale, "EXACT_AGENT_SEED": "7"
            ])
            session.tellTime()
        }
        setLocale("en-US")
        XCTAssertNil(item.backButtonTitle)
        setLocale("fr-FR")
        XCTAssertEqual(item.backButtonTitle, "Sections en français")
        XCTAssertEqual(item.backButtonDisplayMode, .generic)

        let plan = try fixturePlan("NAVIGATION_BARS")
        waitFor("navigation is available for plan replacement") { !Agent(session: session).nativeInFlight() }
        XCTAssertTrue(session.apply(plan))
        let kept = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation?.topViewController)
        XCTAssertEqual(kept.navigationItem.backButtonTitle, "Sections en français")
        setLocale("en-US")
        XCTAssertNil(kept.navigationItem.backButtonTitle)
        XCTAssertEqual(kept.navigationItem.backButtonDisplayMode, .default)
    }

    func testAnUndimmedDialogsHeadLeavesTheRouteLabelAlone() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit presents the standalone dialog in a window scene") }
        let session = try fixture("undimmed-back-history", plan: "EXACT_NAVIGATION_BARS_PLAN")
        let agent = Agent(session: session), p = session.presenter
        XCTAssertTrue(session.navigate("/detail"))
        XCTAssertEqual(agent.clock(["settle": true])["settled"] as? Bool, true)
        let route = try XCTUnwrap(p.navigationHost.primaryNavigation?.topViewController as? RouteController)
        let item = route.navigationItem
        let heading = try XCTUnwrap(HeaderShape.bar(of: route.node)?.heading)
        p.apply(wireBatch([
            ["op": "props", "id": heading.id, "set": ["text": ""]],
            ["op": "props", "id": route.node.id, "set": ["headTitle": "Details"]]
        ]))
        XCTAssertEqual(item.backButtonTitle, "Details")
        p.press(try node(session, "open-history-dialog").id)
        XCTAssertEqual(agent.clock(["settle": true])["settled"] as? Bool, true)
        let dialog = try XCTUnwrap(p.dialogHost.top as? DialogSheetController)
        let sheet = try XCTUnwrap((dialog.navigationController ?? dialog).sheetPresentationController)
        XCTAssertEqual(sheet.largestUndimmedDetentIdentifier, sheet.detents.last?.identifier)
        XCTAssertEqual(item.backButtonTitle, "Details")
        p.press(try node(session, "close-history-dialog").id)
        XCTAssertEqual(agent.clock(["settle": true])["settled"] as? Bool, true)
        XCTAssertEqual(item.backButtonTitle, "Details")
    }

    func testBackHistoryIsOnTheBlankLargeItemBeforePushAndUpdatesFieldByField() throws {
        let session = try fixture("blank-large-back-history")
        let p = session.presenter
        let nav = try XCTUnwrap(p.navigationHost.primaryNavigation)
        let home = try XCTUnwrap(nav.topViewController as? RouteController)
        let item = home.navigationItem
        let heading = try XCTUnwrap(HeaderShape.bar(of: home.node)?.heading)
        XCTAssertEqual(item.largeTitleDisplayMode, .always)
        p.apply(wireBatch([
            ["op": "props", "id": heading.id, "set": ["text": ""]],
            ["op": "props", "id": home.node.id, "set": ["headTitle": "Sections"]]
        ]))
        XCTAssertEqual(item.title, "")
        XCTAssertEqual(item.largeTitleDisplayMode, .always)
        XCTAssertEqual(item.backButtonTitle, "Sections")
        XCTAssertEqual(item.backButtonDisplayMode, .generic)
        var beforePush = false
        try withNavigationStackWrites(shouldRefuse: { navigation, target in
            if navigation === nav, target.count == 2 {
                beforePush = true
                XCTAssertTrue(target.first?.navigationItem === item)
                XCTAssertEqual(item.title, "")
                XCTAssertEqual(item.backButtonTitle, "Sections")
                XCTAssertEqual(item.backButtonDisplayMode, .minimal)
            }
            return false
        }) {
            p.press(try node(session, "detail").id)
            waitFor("the push settles") { nav.viewControllers.count == 2 && nav.transitionCoordinator == nil }
        }
        XCTAssertTrue(beforePush)

        item.backButtonDisplayMode = .default
        p.apply(wireBatch([["op": "props", "id": home.node.id, "set": ["headTitle": "Chapters"]]]))
        XCTAssertEqual(item.backButtonTitle, "Chapters")
        XCTAssertEqual(item.backButtonDisplayMode, .default, "a title-only write preserves the mode override")
        item.backButtonTitle = "Hook title"
        p.apply(wireBatch([["op": "props", "id": home.node.id, "set": ["headTitle": "Chapters", "testId": "changed-home"]]]))
        XCTAssertEqual(item.backButtonTitle, "Hook title")
        p.apply(wireBatch([["op": "props", "id": home.node.id, "clear": ["headTitle"]]]))
        XCTAssertNil(item.backButtonTitle)
        XCTAssertEqual(item.backButtonDisplayMode, .default)
    }
}

extension NavigationMoreIOSTests {
    func checkBackHistoryTransfer(_ session: ExactSession, tabs: UITabBarController, own: UINavigationController,
                                  more: UINavigationController, deeper: RouteController) throws {
        let root = try XCTUnwrap(more.viewControllers.dropLast().last as? RouteController)
        let moreList = try XCTUnwrap(more.viewControllers.first)
        let moreTitle = moreList.navigationItem.backButtonTitle
        XCTAssertTrue(more === tabs.moreNavigationController)
        XCTAssertTrue(more.topViewController === deeper)
        XCTAssertFalse(own.viewControllers.contains(root))
        XCTAssertTrue(more.navigationBar.backItem === root.navigationItem)
        XCTAssertEqual(root.navigationItem.backButtonTitle, "Back")
        let p = session.presenter
        let control = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "back-deeper" })
        p.apply(wireBatch([["op": "props", "id": control.id, "clear": ["id"]]]))
        XCTAssertEqual(root.navigationItem.backButtonTitle, "Five history")
        XCTAssertEqual(root.navigationItem.backButtonDisplayMode, .generic)
        XCTAssertEqual(moreList.navigationItem.backButtonTitle, moreTitle)
    }
}

extension NavigationBarIOSTests {
    func testBackHistoryWritesRunTheRouteHatchOnce() throws {
        let session = try fixture("back-history-hatch", module: true)
        let p = session.presenter
        let home = try XCTUnwrap(p.navigationHost.primaryNavigation?.topViewController as? RouteController)
        let heading = try XCTUnwrap(HeaderShape.bar(of: home.node)?.heading)
        func journal() -> String { session.agent(#"{"op":"logs","since":0}"#) }
        waitFor("the route hatch is connected") { journal().contains("hatch route \(home.key): built") }
        func changes() -> Int { journal().components(separatedBy: "hatch route \(home.key): changed").count }
        let before = changes()
        p.apply(wireBatch([
            ["op": "props", "id": heading.id, "set": ["text": ""]],
            ["op": "props", "id": home.node.id, "set": ["headTitle": "Sections"]]
        ]))
        XCTAssertEqual(home.navigationItem.backButtonTitle, "Sections")
        XCTAssertEqual(changes(), before + 1)
        home.navigationItem.backButtonTitle = "Hook override"
        p.apply(wireBatch([["op": "props", "id": home.node.id, "set": ["testId": "unrelated"]]]))
        XCTAssertEqual(home.navigationItem.backButtonTitle, "Hook override")
        XCTAssertEqual(changes(), before + 1)
    }
}
#endif
