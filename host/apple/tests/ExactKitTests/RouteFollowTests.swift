import XCTest
@testable import ExactKit

final class RouteFollowTests: XCTestCase {
    func testMountedIncomingLinksOpenAppRelativeRoutesColdAndWarm() throws {
        let bytes = try fixturePlan("PUSHED_TITLE")
        let session = ExactApp.shared.makeSession(label: "mounted-links")
        defer { session.destroy() }
        session.webMount = ["origin": "https://site.invalid", "base": "/app/"]
        let link = try XCTUnwrap(URL(string: "https://site.invalid/app/short?q=1#part"))
        XCTAssertTrue(session.openURL(link))
        XCTAssertEqual(session.launchLocation, "/short?q=1#part")
        XCTAssertNil(session.boot(plan: bytes, size: CGSize(width: 390, height: 844)).error)
        XCTAssertEqual(session.location, "/short?q=1#part")
        XCTAssertTrue(session.openURL(try XCTUnwrap(URL(string: "https://site.invalid/app/inline?q=2#next"))))
        XCTAssertEqual(session.location, "/inline?q=2#next")
        for (url, expected) in [
            ("https://site.invalid/app", "/"),
            ("https://site.invalid/app/?q=1#part", "/?q=1#part"),
            ("https://site.invalid/application/short", "/application/short"),
            ("https://site.invalid/app%2Fshort", "/app%2Fshort"),
            ("https://other.invalid/app/short", "/app/short"),
            ("https://site.invalid:444/app/short", "/app/short"),
            ("http://site.invalid/app/short", "/app/short"),
            ("exacttest://app/short", "/app/short"),
        ] {
            XCTAssertEqual(session.incomingLocation(try XCTUnwrap(URL(string: url))), expected, url)
        }
        session.protocolHandlers = [["protocol": "exacttest", "url": "/open?u=%s"]]
        XCTAssertEqual(session.incomingLocation(try XCTUnwrap(URL(string: "exacttest://app/short"))), "/open?u=exacttest%3A%2F%2Fapp%2Fshort")
    }

    private final class Delegate: ExactSessionDelegate {
        var opened: [String] = []
        func exactSession(_ session: ExactSession, command name: String, args: [Any]) {
            if name == "openURL", let href = args.first as? String { opened.append(href) }
        }
    }

    func testLinksAndIncomingURLsFollowRoutesWithoutANavigateHandler() throws {
        let delegate = Delegate()
        let session = ExactApp.shared.makeSession(delegate: delegate, label: "route-follow")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try fixturePlan("PUSHED_TITLE"), size: CGSize(width: 390, height: 844)).error)
        XCTAssertNil(session.navigationRoot)
        func hasRoute(_ name: String) -> Bool {
            session.presenter.views.values.contains { $0.props["testId"] == "route-" + name }
        }
        XCTAssertTrue(session.follow("/short"))
        XCTAssertTrue(hasRoute("short"))
        XCTAssertTrue(session.follow("/short"), "following the selected route is accepted")
        XCTAssertTrue(session.openURL(try XCTUnwrap(URL(string: "exacttest://inline?x=1#part"))))
        XCTAssertTrue(hasRoute("inline"))
        XCTAssertTrue(session.follow("/assets/readme.md"))
        XCTAssertEqual(delegate.opened, ["/assets/readme.md"], "a non-route is the containing app's to open")
        XCTAssertFalse(session.navigate("/missing"))
    }
}
