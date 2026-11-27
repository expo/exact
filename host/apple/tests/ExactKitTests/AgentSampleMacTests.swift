#if os(macOS)
import AppKit
import Network
import WebKit
import XCTest
@testable import ExactKit
@testable import ExactWebViews

extension SplitMacTests {
    func testSampleRefusesAnUnreadableLayerOnlyWhereItContributes() throws {
        let session = try boot(tabs: false, size: CGSize(width: 900, height: 500))
        _ = compose(ExactView(session: session), width: 900, sidebar: false)
        session.presenter.press(try node("row-a").id)
        let agent = Agent(session: session), node = try node("half-a")
        let window = try XCTUnwrap(window)
        XCTAssertNil(agent.sample(["points": [[0, 0]]])["error"])
        window.setFrameOrigin(NSPoint(x: -20000, y: -20000))
        window.orderFrontRegardless()
        let parent = try XCTUnwrap(node.layer)
        let metal = CAMetalLayer(), cover = CALayer()
        metal.frame = parent.bounds
        cover.frame = parent.bounds.divided(atDistance: parent.bounds.width / 2, from: .minXEdge).slice
        cover.backgroundColor = CGColor(srgbRed: 0, green: 1, blue: 0, alpha: 1)
        cover.zPosition = 2
        parent.addSublayer(cover)
        parent.addSublayer(metal)
        let box = agent.box(node)
        func sample(_ fraction: CGFloat) -> [String: Any] {
            agent.sample(["points": [[box.minX + fraction * box.width, box.midY]]])
        }
        let covered = sample(0.25)
        XCTAssertEqual((covered["values"] as? [[Double]])?.first, [0, 1, 0, 1], "\(covered)")
        XCTAssertTrue((sample(0.75)["error"] as? String)?.contains("half-a") == true)
        cover.opacity = 0.5
        XCTAssertTrue((sample(0.25)["error"] as? String)?.contains("Metal") == true)
        metal.isHidden = true
        XCTAssertNil(sample(0.25)["error"])
        XCTAssertTrue(parent.sublayers?.last === metal)
    }
}

final class AgentSampleIframeMacTests: XCTestCase {
    private var sessions: [ExactSession] = []

    override func tearDown() {
        sessions.forEach { $0.destroy() }
        sessions = []
        super.tearDown()
    }

    /// Linear-sRGB `#ececef`: the backdrop behind every test iframe.
    private let backdrop: [Double] = [0.8388, 0.8388, 0.8632, 1.0]
    /// Linear-sRGB `#ef4444`.
    private let red: [Double] = [0.8632, 0.0578, 0.0578, 1.0]
    /// Linear-sRGB `#16a34a`.
    private let green: [Double] = [0.0080, 0.3663, 0.0685, 1.0]

    private func assertLinearColor(_ actual: [Double], matches expected: [Double], tolerance: Double = 0.03, _ message: String = "", file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(actual.count, 4, message, file: file, line: line)
        guard actual.count == 4 else { return }
        for i in 0..<4 {
            XCTAssertEqual(actual[i], expected[i], accuracy: tolerance, "\(message) channel \(i): got \(actual) expected \(expected)", file: file, line: line)
        }
    }

    private func sampleIframe(
        label: String,
        html: String? = nil,
        src externalSrc: String? = nil,
        props extraProps: [String: Any] = [:],
        afterLoadJS: String? = nil,
        points: [[Double]]
    ) throws -> [[Double]] {
        guard let arm = ProcessInfo.processInfo.environment["EXACT_TEST_WEB_MODULE"] else {
            throw missing("module", "no WebKit arm: name a built one with EXACT_TEST_WEB_MODULE")
        }
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "sample-\(label)")
        sessions.append(session)
        session.webHost.moduleLocation = { arm }
        XCTAssertNil(session.boot(size: CGSize(width: 400, height: 300)).error)
        let win = NSWindow(contentRect: NSRect(x: 40, y: 40, width: 400, height: 300), styleMask: [.titled], backing: .buffered, defer: false)
        win.isReleasedWhenClosed = false
        win.contentView = ExactView(session: session)
        win.orderFrontRegardless()
        defer { win.close() }
        waitForActivation(session)

        let src: String
        var tempFile: URL?
        if let externalSrc {
            src = externalSrc
        } else {
            let name = "app:/tmp/sample-\(UUID().uuidString).html"
            let file = try XCTUnwrap(AppFiles.url(name))
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data((html ?? "<!doctype html><body></body>").utf8).write(to: file)
            tempFile = file
            src = name
        }
        defer { if let tempFile { try? FileManager.default.removeItem(at: tempFile) } }

        let loaded = expectation(description: "iframe loaded \(label)")
        var fulfilled = false
        session.presenter.onLoad = { if $0 == 7002 && !fulfilled { fulfilled = true; loaded.fulfill() } }
        var props: [String: Any] = ["src": src, "testId": "page"]
        for (k, v) in extraProps { props[k] = v }
        session.presenter.apply(wireBatch([
            ["op": "create", "id": 7001, "kind": "view", "props": ["testId": "bg"], "style": ["background_color": [236, 236, 239, 255]]],
            ["op": "create", "id": 7002, "kind": "iframe", "props": props, "style": [:], "handlers": ["load"]],
            ["op": "children", "id": 7001, "ids": [7002]],
            ["op": "roots", "ids": [7001]],
            ["op": "frame", "id": 7001, "x": 0.0, "y": 0.0, "w": 400.0, "h": 300.0],
            ["op": "frame", "id": 7002, "x": 0.0, "y": 0.0, "w": 400.0, "h": 300.0],
        ]))
        wait(for: [loaded], timeout: 15)

        if let afterLoadJS, let web = session.presenter.views[7002]?.web as? WKWebView {
            let done = expectation(description: "iframe script \(label)")
            web.evaluateJavaScript("\(afterLoadJS) true") { _, error in
                if let error { XCTFail("\(label): script failed: \(error)") }
                done.fulfill()
            }
            wait(for: [done], timeout: 10)
        }

        let reply = Agent(session: session).sample(["points": points])
        XCTAssertNil(reply["error"], "\(label): \(reply)")
        return try XCTUnwrap(reply["values"] as? [[Double]], "\(label): \(reply)")
    }

    func testSampleOverBlankAndMagnifiedTransparentIframeShowsBackdrop() throws {
        let plain = try sampleIframe(label: "blank-1x", points: [[20, 20], [200, 150], [350, 250]])
        for (i, px) in plain.enumerated() {
            assertLinearColor(px, matches: backdrop, "1x blank point \(i)")
        }

        // apple.magnification < 1 shrinks the document inside the view; pixels outside the
        // scaled document must stay transparent rather than WebKit's white snapshot margin.
        let shrunk = try sampleIframe(
            label: "blank-mag-0.5",
            props: ["appleMagnification": "0.5"],
            points: [[20, 20], [190, 140], [200, 150], [350, 250]]
        )
        for (i, px) in shrunk.enumerated() {
            assertLinearColor(px, matches: backdrop, "0.5x blank point \(i)")
        }

        // Fractional magnification must not leave a white fringe on the document edge.
        let third = try sampleIframe(
            label: "blank-mag-0.33",
            props: ["appleMagnification": "0.33"],
            points: [[20, 20], [131, 98], [132, 99], [300, 200]]
        )
        for (i, px) in third.enumerated() {
            assertLinearColor(px, matches: backdrop, "0.33x blank point \(i)")
        }
    }

    func testSampleOverMagnifiedIframePreservesPaintedElementsAndOrientation() throws {
        let html = """
        <!doctype html>
        <body style="margin:0">
          <div style="position:absolute;left:0;top:0;width:100px;height:100px;background:#ef4444"></div>
          <div style="position:absolute;left:0;top:200px;width:100px;height:100px;background:#16a34a"></div>
        </body>
        """
        // At 0.5x, red occupies (0..50, 0..50), green occupies (0..50, 100..150),
        // (150, 75) is transparent inside the document, and (250, 200) is transparent outside it.
        let values = try sampleIframe(
            label: "elements-mag-0.5",
            html: html,
            props: ["appleMagnification": "0.5"],
            points: [[25, 25], [25, 125], [150, 75], [250, 200]]
        )
        assertLinearColor(values[0], matches: red, "top-left red block")
        assertLinearColor(values[1], matches: green, "lower-left green block")
        assertLinearColor(values[2], matches: backdrop, "transparent inside document")
        assertLinearColor(values[3], matches: backdrop, "transparent outside scaled document")
    }

    func testSampleOverMagnifiedPaintedEdgePreservesBoundaryPixels() throws {
        // A 499-CSS-pixel red document at 0.33x ends at 499 * 0.33 = 164.67 pt (165 pt ceiled),
        // and at 0.67x ends at 499 * 0.67 = 334.33 pt (335 pt ceiled). Points just inside the
        // painted edge must stay red rather than being clipped by point rounding, and points past
        // the edge must show the backdrop rather than WebKit's white margin.
        let html = """
        <!doctype html>
        <style>html,body{margin:0;width:499px;height:200px;background:#ef4444}</style>
        <body></body>
        """
        let third = try sampleIframe(
            label: "edge-499-0.33",
            html: html,
            props: ["appleMagnification": "0.33"],
            points: [[162, 50], [166, 50], [300, 50]]
        )
        assertLinearColor(third[0], matches: red, "inside 0.33x painted edge")
        assertLinearColor(third[1], matches: backdrop, "outside 0.33x painted edge")
        assertLinearColor(third[2], matches: backdrop, "far outside 0.33x painted edge")

        let twoThirds = try sampleIframe(
            label: "edge-499-0.67",
            html: html,
            props: ["appleMagnification": "0.67"],
            points: [[333, 50], [336, 50], [380, 50]]
        )
        assertLinearColor(twoThirds[0], matches: red, "inside 0.67x painted edge")
        assertLinearColor(twoThirds[1], matches: backdrop, "outside 0.67x painted edge")
        assertLinearColor(twoThirds[2], matches: backdrop, "far outside 0.67x painted edge")
    }

    func testSampleOverMagnifiedRTLIframePreservesLeftwardOverflow() throws {
        // An RTL document with 1000 CSS px of content in a 400 px viewport spans [-600, 400].
        // Scrolled to pageLeft = -400 at 0.5x, it projects to [-100, 400] and fills the entire
        // 400 pt viewport; left-edge content must not be cleared as if the document started at 0.
        let scrolledRTL = """
        <!doctype html>
        <html dir="rtl">
        <style>html,body{margin:0;background:transparent}.band{width:1000px;height:200px;background:#ef4444}</style>
        <body><div class="band"></div></body>
        </html>
        """
        let full = try sampleIframe(
            label: "rtl-scrolled-0.5",
            html: scrolledRTL,
            props: ["appleMagnification": "0.5"],
            afterLoadJS: "window.scrollTo(-400, 0);",
            points: [[20, 50], [150, 50], [350, 50]]
        )
        assertLinearColor(full[0], matches: red, "RTL left overflow visible at x=20")
        assertLinearColor(full[1], matches: red, "RTL left overflow visible at x=150")
        assertLinearColor(full[2], matches: red, "RTL content visible at x=350")

        // Unscrolled (pageLeft = 0) at 0.5x, [-600, 400] projects to [-300, 200]:
        // [0, 200) is red content and [200, 400) is outside margin showing the backdrop.
        let unscrolledRTL = """
        <!doctype html>
        <html dir="rtl">
        <style>html,body{margin:0;background:transparent}.band{width:1000px;height:200px;background:#ef4444}</style>
        <body><div class="band"></div></body>
        </html>
        """
        let half = try sampleIframe(
            label: "rtl-unscrolled-0.5",
            html: unscrolledRTL,
            props: ["appleMagnification": "0.5"],
            points: [[20, 50], [180, 50], [250, 50]]
        )
        assertLinearColor(half[0], matches: red, "RTL unscrolled left edge red")
        assertLinearColor(half[1], matches: red, "RTL unscrolled right edge red")
        assertLinearColor(half[2], matches: backdrop, "RTL unscrolled outside margin is backdrop")
    }

    func testPartitionedIframeRetainsOpaquePageBackground() throws {
        let html = """
        <!doctype html>
        <body style="margin:0"><div style="width:100px;height:100px;background:#ef4444"></div></body>
        """
        let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: \(html.utf8.count)\r\nConnection: close\r\n\r\n\(html)"
        let listener = try loopbackListener(port: 0) { connection in
            connection.start(queue: .main)
            connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { _, _, _, _ in
                connection.send(content: Data(response.utf8), completion: .contentProcessed { _ in connection.cancel() })
            }
        }
        defer { listener.cancel() }
        let port = try XCTUnwrap(listener.port).rawValue
        // A partitioned frame draws an opaque page background even at magnification < 1:
        // painted content and the area outside the scaled document both remain opaque.
        let values = try sampleIframe(
            label: "partition-opaque-0.5",
            src: "http://127.0.0.1:\(port)/",
            props: ["partition": "mem-sample-opaque", "appleMagnification": "0.5"],
            points: [[25, 25], [250, 200]]
        )
        assertLinearColor(values[0], matches: red, "partitioned red block at 0.5x")
        assertLinearColor(values[1], matches: [1.0, 1.0, 1.0, 1.0], "partitioned frame keeps opaque white background")
    }
}
#endif
