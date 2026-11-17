#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1168.000 D6.1: the chat bubble's trailing time (docs/recipes/chat-header.md) on UIKit. Each message's text ends
/// with a hidden copy of the time, the time drawn over the bubble's corner: a short last line keeps it beside the last
/// word, one with no room for it puts it on a line of its own, and a right-to-left bubble's time is at its left, its
/// hidden copy led by U+200F. Read from the message run's line fragments (`inlineRects`) and the time's frame.
final class TrailingTimeIOSTests: XCTestCase {
    private var session: ExactSession?
    private var window: UIWindow?
    override func tearDown() {
        session?.destroy(); session = nil
        window?.isHidden = true; window = nil
        super.tearDown()
    }

    private struct Bubble { let bubble: CGRect, time: CGRect, lines: [CGRect] }

    /// The bubble, the time and the message run's lines (one rect a line, joined), in the window's points.
    private func read(_ p: Presenter, _ name: String, in window: UIWindow) throws -> Bubble {
        let view = { (id: String) in try XCTUnwrap(p.views.values.first { $0.props["testId"] == id }, id) }
        let body = try view("body-\(name)"), words = try XCTUnwrap(body.inlineText.first, "\(name)'s words")
        var lines: [CGRect] = []
        for r in body.inlineRects(words).map({ body.convert($0, to: window) }).sorted(by: { $0.minY < $1.minY }) {
            if let last = lines.last, abs(last.midY - r.midY) < 2 { lines[lines.count - 1] = last.union(r) } else { lines.append(r) }
        }
        let frame = { (v: NodeView) in v.convert(v.bounds, to: window) }
        return Bubble(bubble: frame(try view("bubble-\(name)")), time: frame(try view("time-\(name)")), lines: lines)
    }

    func testATimeSitsOnAShortLastLineAndDropsWhenThereIsNoRoom() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the bubbles lay out in a scene's window") }
        let s = ExactApp.shared.makeSession(label: "trailing-time")
        session = s
        let view = ExactView(session: s), host = UIViewController(), w = UIWindow(windowScene: scene)
        w.frame = w.screen.bounds; w.rootViewController = host; w.makeKeyAndVisible(); host.view.addSubview(view)
        window = w
        XCTAssertNil(s.boot(plan: try fixturePlan("TRAILING_TIME"), size: w.bounds.size).error)
        view.frame = host.view.bounds; view.layoutIfNeeded()
        let p = s.presenter
        let end = Date(timeIntervalSinceNow: 5)
        while p.views.values.first(where: { $0.props["testId"] == "time-edge11" }) == nil, Date() < end {
            RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.02))
        }
        w.layoutIfNeeded()
        func holds(_ name: String, rtl: Bool = false, file: StaticString = #filePath, line: UInt = #line) throws -> String {
            let b = try read(p, name, in: w), last = try XCTUnwrap(b.lines.last, "\(name) has lines", file: file, line: line)
            XCTAssertTrue(b.bubble.insetBy(dx: -0.5, dy: -0.5).contains(b.time), "\(name): the time is inside its bubble", file: file, line: line)
            guard b.time.midY > last.minY, b.time.midY < last.maxY else {
                XCTAssertGreaterThanOrEqual(b.time.minY, last.maxY - 1, "\(name): below its last line", file: file, line: line)
                return "below"
            }
            if rtl { XCTAssertLessThanOrEqual(b.time.maxX, last.minX, "\(name): before the line's start", file: file, line: line) }
            else { XCTAssertGreaterThanOrEqual(b.time.minX, last.maxX, "\(name): after the line's end", file: file, line: line) }
            return "beside"
        }
        XCTAssertEqual(try read(p, "short", in: w).lines.count, 1)
        XCTAssertEqual(try holds("short"), "beside")
        XCTAssertEqual(try read(p, "two", in: w).lines.count, 2)
        XCTAssertEqual(try holds("two"), "beside")
        for name in ["rtl", "mixed"] {
            XCTAssertEqual(try read(p, name, in: w).lines.count, 1, name)
            XCTAssertEqual(try holds(name, rtl: true), "beside", name)
        }
        let edges = try (0..<12).map { try holds("edge\($0)") }
        XCTAssertTrue(edges.contains("beside") && edges.contains("below"), "\(edges)")
    }
}
#endif
