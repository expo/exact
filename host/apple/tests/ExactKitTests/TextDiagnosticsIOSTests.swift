#if os(iOS)
import UIKit
import XCTest
import ExactCore
@testable import ExactKit

final class TextDiagnosticsIOSTests: XCTestCase {
    func testScheduledTimestampSurvivesAnIntentionalRunLoopWait() {
        let screen = FrameHealthScreen()
        var observed: [Double] = []
        screen.completedUpdate = { scheduled, start, deadline, before, finished, waited, _ in
            observed = [scheduled, start, deadline, before, finished, waited]
        }
        screen.scheduled(presented: 1000.020, deadline: 1000.010, now: 1000)
        screen.wait(at: 1000.002)
        screen.wake(at: 1000.006)
        screen.completed(presented: 1000.020, now: 1000.014)
        XCTAssertEqual(observed.count, 6)
        XCTAssertEqual(observed[0], 1000)
        XCTAssertEqual(observed[1], 1000.004, accuracy: 1e-9)
        XCTAssertEqual(observed[2], 1000.010)
        XCTAssertEqual(observed[5], 0.004, accuracy: 1e-9)
    }

    func testCardFreeCaptureExportsLayerPaintsAndStopsItsRecorder() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "text diagnostics recording and export in a real host window")
        }
        let session = ExactApp.shared.makeSession(label: "text-diagnostics-test")
        let window = UIWindow(windowScene: scene), root = UIViewController()
        window.rootViewController = root
        window.makeKeyAndVisible()
        DevMenu.install(on: window, session: session, controller: root, planPath: nil, frameHealthContainer: root.view)
        let recorder = TextDiagnostics.shared
        defer {
            recorder.finish()
            window.isHidden = true
            session.destroy()
        }
        recorder.start()
        XCTAssertTrue(recorder.recording)
        XCTAssertFalse(session.frameHealth.hudVisible)
        let presenter = session.presenter
        presenter.viewport.frame = root.view.bounds
        root.view.addSubview(presenter.viewport)
        presenter.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "text", "props": ["text": "Coverage"], "style": ["font_size": 16.0]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 200.0, "h": 40.0]
        ]))
        let node = try XCTUnwrap(presenter.views[1])
        presenter.textRasters.ensureUrgent([node])
        presenter.textRasters.settleVisible([node])
        recorder.finish()
        XCTAssertFalse(recorder.recording)
        XCTAssertFalse(TextRenderMetrics.shared.recording)
        let url = try XCTUnwrap(recorder.saved)
        let packet = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
        XCTAssertEqual(packet["format"] as? String, "exact.text-diagnostics/2")
        XCTAssertEqual(packet["cardVisibleDuringCapture"] as? Bool, false)
        XCTAssertEqual(packet["rateRequestedByCapture"] as? Bool, false)
        let text = try XCTUnwrap(packet["text"] as? [String: Any])
        let rows = try XCTUnwrap(text["rows"] as? [[Any]])
        XCTAssertTrue(rows.contains { ($0[2] as? Int) == TextRenderMetrics.Kind.coveragePaint.rawValue })
        XCTAssertNotNil(packet["updates"])
        XCTAssertNotNil(packet["health"])
    }
}
#endif
