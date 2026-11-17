#if os(iOS) || os(macOS)
#if os(iOS)
import UIKit
#else
import AppKit
#endif
import XCTest
import ImageIO
@testable import ExactKit
@testable import ExactSurfaces

final class CanvasCaptureSessionTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() }

    func testACaptureIncludesTheActionsDrawingAndResize() throws {
        #if os(iOS)
        guard hostedWindowScene() != nil else { throw missing("scene", "a capture uses the session's window") }
        #endif
        try capture(deferred: false)
    }
    func testACaptureWaitsForTheDeferredDrawingAndResize() throws {
        #if os(iOS)
        guard hostedWindowScene() != nil else { throw missing("scene", "a capture uses the session's window") }
        #endif
        try capture(deferred: true)
    }

    func testALaterCommitBeforeDeferredDrawingSuppliesTheCapture() throws {
        #if os(iOS)
        guard hostedWindowScene() != nil else { throw missing("scene", "a capture uses the session's window") }
        #endif
        try capture(deferred: true, superseded: true)
    }

    func testUnmountAfterCopyPublishesTheOwnedPixels() throws {
        #if os(iOS)
        guard hostedWindowScene() != nil else { throw missing("scene", "a capture uses the session's window") }
        #endif
        try capture(deferred: false, unmountAfterCopy: true)
    }

    func testUnmountBeforeCopyPreservesTheDestination() throws {
        #if os(iOS)
        guard hostedWindowScene() != nil else { throw missing("scene", "a capture uses the session's window") }
        #endif
        try capture(deferred: false, unmountAfterCopy: false)
    }

    private func capture(deferred: Bool, superseded: Bool = false, unmountAfterCopy: Bool? = nil) throws {
        guard ProcessInfo.processInfo.environment["EXACT_TEST_APP_ID"] == "com.exact.canvasgallery" else {
            throw XCTSkip("run with --test canvas-gallery-apple: this fixture uses the gallery's capture source")
        }
        let wasAsync = ExactSession.asyncFills
        ExactSession.asyncFills = deferred
        defer { ExactSession.asyncFills = wasAsync }
        let session = ExactApp.shared.makeSession(label: "canvas-capture-session")
        defer { session.destroy() }
        let size = CGSize(width: 300, height: 300)
        #if os(iOS)
        let scene = try XCTUnwrap(hostedWindowScene())
        let window = UIWindow(windowScene: scene), controller = UIViewController()
        window.frame = CGRect(origin: .zero, size: size)
        window.rootViewController = controller
        window.makeKeyAndVisible()
        let view = ExactView(session: session)
        view.frame = controller.view.bounds; controller.view.addSubview(view)
        defer { window.isHidden = true }
        #else
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false; window.contentView = ExactView(session: session)
        window.makeKeyAndOrderFront(nil)
        defer { window.close() }
        #endif
        XCTAssertNil(session.boot(plan: try fixturePlan("CANVAS_CAPTURE"), size: size).error)
        waitForActivation(session)
        AppFiles.learn(session.runtime)
        let paths = (0...1).map { "app:/tmp/session-capture-\($0).png" }
        for path in paths { if let url = AppFiles.url(path) { try? FileManager.default.removeItem(at: url) } }
        defer { for path in paths { if let url = AppFiles.url(path) { try? FileManager.default.removeItem(at: url) } } }
        func node(_ name: String) -> NodeView? { session.presenter.views.values.first { $0.props["testId"] == name } }
        waitFor("the initial canvas replay") {
            session.drainFill()
            session.presenter.canvas2d.waitForReplays()
            return !session.canvasInFlight && session.presenter.canvas2d.loadingCount == 0
        }
        for (index, button) in ["capture", "resize"].enumerated() {
            let release = DispatchSemaphore(value: 0)
            defer { release.signal() }
            let reached = XCTestExpectation(description: "the capture boundary")
            let canvas = try XCTUnwrap(session.presenter.canvas2d as? Canvas2DHost)
            if let afterCopy = unmountAfterCopy {
                let hold = {
                    reached.fulfill()
                    release.wait()
                }
                if afterCopy { canvas.beforeCaptureEncode = hold }
                else { canvas.beforeCaptureCopy = hold }
            } else { reached.fulfill() }
            if superseded || unmountAfterCopy != nil {
                let url = try XCTUnwrap(AppFiles.url(paths[index]))
                try Data("keep".utf8).write(to: url)
            }
            session.presenter.onPress?(try XCTUnwrap(node(button)?.id))
            if let afterCopy = unmountAfterCopy {
                wait(for: [reached], timeout: 10)
                session.presenter.onPress?(try XCTUnwrap(node("hide")?.id))
                XCTAssertNil(node("canvas"))
                release.signal()
                if !afterCopy {
                    waitFor("the unmount refusal") { (node("result")?.props["text"] ?? "").contains("EncodingError") }
                    let url = try XCTUnwrap(AppFiles.url(paths[index]))
                    XCTAssertEqual(try Data(contentsOf: url), Data("keep".utf8))
                    return
                }
            }
            if superseded {
                session.presenter.onPress?(try XCTUnwrap(node("change")?.id))

            }
            waitFor("the capture reply", facts: { "result=\(String(describing: node("result")?.props))" }) { let result = node("result")?.props["text"] ?? ""; return result.hasPrefix(paths[index]) || result.contains("Error:") }
            XCTAssertTrue((node("result")?.props["text"])?.hasPrefix(paths[index]) == true, String(describing: node("result")?.props))
            let file = try XCTUnwrap(AppFiles.open(paths[index])); defer { try? file.close() }
            let bytes = try XCTUnwrap(try file.readToEnd())
            let source = try XCTUnwrap(CGImageSourceCreateWithData(bytes as CFData, nil))
            let image = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
            let pixels = Canvas2DCaptureTests.rgba(image, image.width, image.height)
            let expected: [UInt8] = (index == 0) != superseded ? [0, 0, 255, 255] : [255, 0, 0, 255]
            XCTAssertEqual(Array(pixels.prefix(4)), expected, "the action's drawing, including after resize")
            XCTAssertEqual(image.width * 30, image.height * (index == 0 ? 40 : 60))
            if superseded || unmountAfterCopy != nil { return }
        }
    }
}
#endif
