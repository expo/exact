#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class RouteOverflowIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?

    override func tearDown() {
        session?.destroy()
        session = nil
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    private func named(_ name: String) throws -> NodeView {
        try XCTUnwrap(session?.presenter.views.values.first { $0.props["testId"] == name })
    }

    private func warnings(_ session: ExactSession) -> Int {
        session.runtime.agent("{\"op\":\"logs\"}").components(separatedBy: "route-overflow").count - 1
    }

    private func settle(_ session: ExactSession) {
        let agent = Agent(session: session)
        waitFor("native layout settles") { !agent.nativeInFlight() && !session.fillInFlight }
        waitForMainQueue()
    }

    func testOnlyWindowOverflowJournalsAcrossDetentsAndTheResizesContentKeyboard() throws {
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "route offers need the hosted app's window scene")
        }
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_ROUTE_OVERFLOW_PLAN"])
        let session = ExactApp.shared.makeSession(label: "route-overflow")
        self.session = session
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = window.screen.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        view.frame = host.view.bounds
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: view.bounds.size).error)
        view.layoutIfNeeded()
        waitForActivation(session)
        settle(session)
        let p = session.presenter
        XCTAssertEqual(p.keyboardPolicy, "resizes-content")
        XCTAssertEqual(warnings(session), 0, session.runtime.agent("{\"op\":\"logs\"}"))

        p.press(try named("push-home").id)
        settle(session)
        let detail = try named("route-detail")
        XCTAssertNotNil(p.owners.told[detail.id])
        XCTAssertEqual(p.navigation.covers[try named("header-detail").id], .whole, "the navigation bar lifted the header")
        XCTAssertEqual(warnings(session), 0, "a push must wait for its native geometry")
        p.press(try named("back-detail").id)
        settle(session)

        // A real overflowing window proves the journal is connected to UIKit.
        p.press(try named("toggle-home").id)
        settle(session)
        XCTAssertEqual(warnings(session), 1)
        p.press(try named("toggle-home").id)
        settle(session)

        let field = try XCTUnwrap(try named("name-home").field)
        let shown = expectation(forNotification: UIResponder.keyboardDidShowNotification, object: nil)
        field.becomeFirstResponder()
        wait(for: [shown], timeout: 10)
        waitFor("keyboard avoidance reaches the route") { p.keyboardInset > 100 }
        settle(session)
        let home = try named("route-home")
        if case .size(let height) = p.owners.told[home.id]?.block.height {
            XCTAssertLessThan(height, view.bounds.height - 100)
        } else { XCTFail("the route has a keyboard-sized native block") }
        let body = try named("body-home")
        XCTAssertGreaterThan(body.convert(body.bounds, to: home).maxY, home.bounds.height)
        XCTAssertEqual(warnings(session), 1, "the keyboard must not journal overflow")
        let hidden = expectation(forNotification: UIResponder.keyboardDidHideNotification, object: nil)
        field.resignFirstResponder()
        wait(for: [hidden], timeout: 10)
        waitFor("keyboard avoidance ends") { p.keyboardInset == 0 }
        settle(session)

        p.press(try named("open-home").id)
        waitFor("the sheet is up") { p.modals.topNavigation?.view.window != nil }
        settle(session)
        let nav = try XCTUnwrap(p.modals.topNavigation)
        let sheet = try XCTUnwrap(nav.sheetPresentationController)
        let route = try named("route-sheet")
        waitFor("the small detent is laid out") { route.bounds.height < 400 && route.bounds.height > 0 }
        let sheetBody = try named("body-sheet")
        XCTAssertGreaterThan(sheetBody.convert(sheetBody.bounds, to: route).maxY, route.bounds.height)
        XCTAssertEqual(warnings(session), 1, "a small detent must not journal overflow")
        sheet.animateChanges { sheet.selectedDetentIdentifier = .large }
        waitFor("the large detent is laid out") { route.bounds.height > 400 }
        settle(session)
        sheet.animateChanges { sheet.selectedDetentIdentifier = .init("authored-0") }
        waitFor("the small detent returns") { route.bounds.height < 400 }
        settle(session)
        XCTAssertEqual(warnings(session), 1)
        p.press(try named("toggle-sheet").id)
        settle(session)
        XCTAssertEqual(warnings(session), 1, "the covered home route must not journal")
        p.press(try named("back-sheet").id)
        waitFor("the sheet is dismissed") { p.modals.topNavigation == nil }
        settle(session)
        XCTAssertEqual(warnings(session), 2, "the window warning was not latched by a detent or keyboard")
    }
}
#endif
