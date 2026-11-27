#if os(iOS)
import UIKit
import WebKit
import XCTest
@testable import ExactKit
@testable import ExactWebViews

final class AgentSampleIframeIOSTests: XCTestCase {
    private var sessions: [ExactSession] = []
    private var windows: [UIWindow] = []

    override func tearDown() {
        sessions.forEach { $0.destroy() }
        sessions = []
        windows.forEach { $0.isHidden = true }
        windows = []
        super.tearDown()
    }

    private func assertLinearColor(_ actual: [Double], matches expected: [Double], tolerance: Double = 0.03, _ message: String = "", file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(actual.count, 4, message, file: file, line: line)
        guard actual.count == 4 else { return }
        for i in 0..<4 {
            XCTAssertEqual(actual[i], expected[i], accuracy: tolerance, "\(message) channel \(i): got \(actual) expected \(expected)", file: file, line: line)
        }
    }

    func testSampleOverTransparentIframeShowsBackdrop() throws {
        guard let arm = ProcessInfo.processInfo.environment["EXACT_TEST_WEB_MODULE"] else {
            throw missing("module", "no WebKit arm")
        }
        guard let scene = hostedWindowScene() else {
            throw missing("scene", "no hosted window scene")
        }
        func sample(_ label: String, html: String) throws -> [Double] {
            let session = ExactApp.shared.makeSession(label: "ios-sample-\(label)")
            sessions.append(session)
            session.webHost.moduleLocation = { arm }
            let size = CGSize(width: 300, height: 200)
            XCTAssertNil(session.boot(size: size).error)
            let win = UIWindow(windowScene: scene)
            win.frame = CGRect(origin: .zero, size: size)
            let root = UIViewController()
            let exactView = ExactView(session: session)
            exactView.frame = CGRect(origin: .zero, size: size)
            root.view.addSubview(exactView)
            win.rootViewController = root
            win.makeKeyAndVisible()
            windows.append(win)
            waitForActivation(session)

            let name = "app:/tmp/sample-ios-\(UUID().uuidString).html"
            let file = try XCTUnwrap(AppFiles.url(name))
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data(html.utf8).write(to: file)
            defer { try? FileManager.default.removeItem(at: file) }

            let loaded = expectation(description: "ios loaded \(label)")
            var fulfilled = false
            session.presenter.onLoad = { if $0 == 7002 && !fulfilled { fulfilled = true; loaded.fulfill() } }
            session.presenter.apply(wireBatch([
                ["op": "create", "id": 7001, "kind": "view", "props": ["testId": "bg"], "style": ["background_color": [236, 236, 239, 255]]],
                ["op": "create", "id": 7002, "kind": "iframe", "props": ["src": name, "testId": "page"], "style": [:], "handlers": ["load"]],
                ["op": "children", "id": 7001, "ids": [7002]],
                ["op": "roots", "ids": [7001]],
                ["op": "frame", "id": 7001, "x": 0.0, "y": 0.0, "w": 300.0, "h": 200.0],
                ["op": "frame", "id": 7002, "x": 0.0, "y": 0.0, "w": 300.0, "h": 200.0],
            ]))
            wait(for: [loaded], timeout: 15)

            let reply = Agent(session: session).sample(["points": [[150, 100]]])
            XCTAssertNil(reply["error"], "\(label): \(reply)")
            return try XCTUnwrap((reply["values"] as? [[Double]])?.first, "\(label): \(reply)")
        }

        let blank = try sample("blank", html: "<!doctype html><meta name='viewport' content='width=device-width, initial-scale=1'><body></body>")
        assertLinearColor(blank, matches: [0.8388, 0.8388, 0.8632, 1.0], "iOS blank iframe shows backdrop")

        let blue = try sample("blue", html: "<!doctype html><meta name='viewport' content='width=device-width, initial-scale=1'><body style='margin:0;background:#2563eb'></body>")
        assertLinearColor(blue, matches: [0.0185, 0.1247, 0.8314, 1.0], "iOS opaque iframe shows page color")

        let half = try sample("half-blue", html: "<!doctype html><meta name='viewport' content='width=device-width, initial-scale=1'><body style='margin:0;background:rgba(37,99,235,0.5)'></body>")
        XCTAssertGreaterThan(half[0], 0.15, "translucent blue blends with backdrop red: \(half)")
        XCTAssertLessThan(half[0], 0.60, "translucent blue is tinted below backdrop red: \(half)")
        XCTAssertGreaterThan(half[2], 0.80, "translucent blue keeps blue channel: \(half)")
    }
}
#endif
