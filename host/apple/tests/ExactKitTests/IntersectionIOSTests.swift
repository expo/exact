import XCTest
@testable import ExactKit
#if os(macOS)
import AppKit
#elseif os(iOS)
import UIKit
#endif

#if os(macOS) || os(iOS)
class IntersectionFixture: XCTestCase {
    func plan() throws -> Data {
        #if os(macOS)
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let source = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("../../../../scripts/fixtures/intersect.contract").standardizedFileURL
        let out = dir.appendingPathComponent("app.plan")
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
        compiler.arguments = ["build", source.path, "-o", out.path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: out)
        #else
        return try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_INTERSECT_PLAN"])))
        #endif
    }
    func exercise() throws {
        #if os(macOS)
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 400), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        #else
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        defer { window.isHidden = true }
        #endif
        let session = ExactApp.shared.makeSession(label: "intersect")
        defer { session.destroy() }
        #if os(macOS)
        window.contentView = session.presenter.viewport
        #else
        session.presenter.viewport.frame = window.bounds
        window.addSubview(session.presenter.viewport)
        window.makeKeyAndVisible()
        #endif
        XCTAssertNil(session.boot(plan: try plan(), size: CGSize(width: 400, height: 400)).error)
        func facts(_ name: String) -> String { session.presenter.views.values.first { $0.props["testId"] == name }?.props["text"] ?? "" }
        func until(_ value: String, _ name: String = "facts") {
            let end = Date().addingTimeInterval(20)
            while facts(name) != value, Date() < end {
                RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05))
            }
            XCTAssertEqual(facts(name), value)
        }
        func scroll(_ top: CGFloat, _ name: String = "port") throws {
            let root = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == name })
            #if os(macOS)
            let scroll = try XCTUnwrap(root.scroll)
            scroll.contentView.scroll(to: CGPoint(x: 0, y: top))
            scroll.reflectScrolledClipView(scroll.contentView)
            root.clipScrolled()
            #else
            let scroll = try XCTUnwrap(root.scrollView)
            scroll.setContentOffset(CGPoint(x: 0, y: top), animated: false)
            root.scrollViewDidScroll(scroll)
            #endif
        }
        let end = Date().addingTimeInterval(20)
        func initialFacts() -> String { session.presenter.views.values.first { $0.props["testId"] == "initial-facts" }?.props["text"] ?? "" }
        while initialFacts() == "0:false:0", Date() < end {
            RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05))
        }
        XCTAssertEqual(initialFacts(), "1:true:1", "initial observation follows authored scrollTop")
        until("1:false:0")
        try scroll(60)
        until("2:true:0.6")
        try scroll(211)
        until("3:false:0")
        // No scroll handler is declared: geometry reports alone must wake delivery.
        try scroll(60)
        until("4:true:0.6")
        until("1:true:1", "sticky-facts")
        try scroll(201, "sticky-port")
        until("2:false:0", "sticky-facts")
        try scroll(60, "sticky-port")
        until("3:true:1", "sticky-facts")
    }
}
#if os(macOS)
final class IntersectionMacTests: IntersectionFixture {
    func testMountAndNativeScrollWithoutScrollHandler() throws { try exercise() }
}
#else
final class IntersectionIOSTests: IntersectionFixture {
    func testMountAndNativeScrollWithoutScrollHandler() throws { try exercise() }

    func testPushedPageScrollDeliversLeaveAndEnterWithoutLoadingAnOffscreenMarker() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a pushed route requires UIKit containment") }
        let session = ExactApp.shared.makeSession(label: "intersect-route")
        defer { session.destroy() }
        let host = UIViewController(), window = UIWindow(windowScene: scene)
        window.frame = window.screen.bounds; window.rootViewController = host; window.makeKeyAndVisible()
        defer { window.isHidden = true }
        let view = ExactView(session: session)
        host.view.addSubview(view)
        XCTAssertNil(session.boot(plan: try fixturePlan("INTERSECT_ROUTE"), size: window.bounds.size).error)
        view.frame = host.view.bounds; view.layoutIfNeeded()
        let p = session.presenter
        func node(_ name: String) throws -> NodeView {
            try XCTUnwrap(p.views.values.first { $0.props["testId"] == name })
        }
        func facts(_ name: String) -> String { p.views.values.first { $0.props["testId"] == name }?.props["text"] ?? "" }
        waitFor("home appears") { p.navigation.activeRoute != nil && !p.navigation.inTransition }
        p.press(try node("open").id)
        waitFor("the pushed page appears") { p.navigation.activeRoute?.pushed == true && !p.navigation.inTransition }
        let route = try XCTUnwrap(p.navigation.activeRoute)
        let scroll = try XCTUnwrap(route.contentViewport, "the route's native page scrolls")
        waitFor("initial observations") { facts("route-facts") == "1:true" && facts("marker-facts") == "1:false" }
        scroll.setContentOffset(CGPoint(x: 0, y: 500), animated: false)
        waitFor("scrolling alone leaves the target") { facts("route-facts") == "2:false" }
        XCTAssertEqual(facts("marker-facts"), "1:false")
        scroll.setContentOffset(.zero, animated: false)
        waitFor("scrolling alone reenters the target") { facts("route-facts") == "3:true" }
        XCTAssertEqual(facts("marker-facts"), "1:false")
        scroll.setContentOffset(CGPoint(x: 0, y: 1500), animated: false)
        waitFor("the paging marker enters once") { facts("marker-facts") == "2:true" }
        scroll.setContentOffset(.zero, animated: false)
        waitFor("the paging marker leaves once") { facts("marker-facts") == "3:false" }
        p.press(try node("open-list").id)
        waitFor("the virtual list appears") { facts("list-facts") == "true:false" && !p.navigation.inTransition }
        let list = try XCTUnwrap(try node("virtual-list").scrollView)
        list.setContentOffset(CGPoint(x: 0, y: 250), animated: false)
        waitFor("a preloaded list row enters as the first leaves") { facts("list-facts") == "false:true" }
        list.setContentOffset(.zero, animated: false)
        waitFor("list rows return to their initial visibility") { facts("list-facts") == "true:false" }
    }
}
#endif
#endif
