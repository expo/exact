#if os(iOS)
import QuartzCore
import UIKit
import XCTest
@testable import ExactKit

private final class Drawing: NSObject {
    private let change: () -> Void
    init(_ change: @escaping () -> Void) { self.change = change }
    @objc func tick() { change() }
}

/// What the card costs the app's frames while it measures the screen (LLP 1079.000 §7.4, §8.3).
final class FrameHealthCardCostIOSTests: XCTestCase {
    /// Opt-in measurement on the simulator, functional evidence only: the app changes a layer at every refresh for ten
    /// seconds, with the screen observed alone and with the card measuring it, and the report gives each update's
    /// lateness and the card's own work within it. Not a performance gate or a device qualification.
    func testTheCardsOwnWorkWhileTheAppDraws() throws {
        guard ProcessInfo.processInfo.environment["EXACT_FRAME_HEALTH_BENCHMARK"] == "1" else {
            throw XCTSkip("set EXACT_FRAME_HEALTH_BENCHMARK=1 to measure the card's cost")
        }
        guard #available(iOS 18.0, *) else { throw XCTSkip("UIUpdateLink is available from iOS 18") }
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "a real host window for the frame-health card")
        }
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        addTeardownBlock { window.isHidden = true }
        let dot = CALayer()
        dot.frame = CGRect(x: 0, y: 600, width: 20, height: 20)
        dot.backgroundColor = UIColor.systemBlue.cgColor
        window.layer.addSublayer(dot)
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        let seconds = 10.0

        /// The app's frames for `seconds`, each recorded in Exact's clock as well, and each update the screen saw.
        func run(_ health: FrameHealth, _ screen: FrameHealthScreen, card view: FrameHealthCardView? = nil) -> [String: Any] {
            var updates: [(late: Bool, card: Double)] = []
            // The face's and the plots' drawing, each part of the card's work, in the main thread's CPU time, which
            // other processes' load on the Mac disturbs less than elapsed time
            var pictures: [String: (count: Int, seconds: Double)] = [:]
            func timed(_ name: String, _ inner: ((() -> Void) -> Void)?) -> (() -> Void) -> Void {
                { draw in
                    let begin = clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)
                    if let inner { inner(draw) } else { draw() }
                    let entry = pictures[name] ?? (0, 0)
                    pictures[name] = (entry.count + 1, entry.seconds + Double(clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID) - begin) / 1e9)
                }
            }
            let saved = (view?.drawing, view?.chart.drawing)
            if let view { (view.drawing, view.chart.drawing) = (timed("face", saved.0), timed("plots", saved.1)) }
            defer { if let view { (view.drawing, view.chart.drawing) = saved } }
            screen.completedUpdate = { _, _, deadline, _, finished, _, card in updates.append((finished > deadline, card)) }
            let drawing = Drawing {
                dot.position.x = dot.position.x > 300 ? 10 : dot.position.x + 1
                let t = CACurrentMediaTime()
                health.record(arrival: t, target: t + 1 / 60, period: 1 / 60, finished: t + 0.001)
            }
            let link = CADisplayLink(target: drawing, selector: #selector(Drawing.tick))
            link.add(to: .main, forMode: .common)
            RunLoop.main.run(until: Date().addingTimeInterval(seconds))
            link.invalidate()
            screen.completedUpdate = nil
            let card = updates.map { $0.card * 1000 }.filter { $0 > 0 }.sorted()
            let quantile = { (q: Double) in card.isEmpty ? 0 : card[min(card.count - 1, Int((Double(card.count) * q).rounded(.down)))] }
            let rounded = { (value: Double) in (value * 1000).rounded() / 1000 }
            return ["updates": updates.count, "late": updates.filter(\.late).count,
                "lateWithCardWork": updates.filter { $0.late && $0.card > 0 }.count, "updatesWithCardWork": card.count,
                "cardMsPerSecond": rounded(card.reduce(0, +) / seconds), "cardMsMedian": rounded(quantile(0.5)),
                "cardMsP90": rounded(quantile(0.9)), "cardMsMaximum": rounded(card.last ?? 0),
                "pictures": pictures.mapValues { ["asked": $0.count, "cpuMsPerSecond": rounded($0.seconds * 1000 / seconds),
                    "cpuMsEach": rounded($0.count == 0 ? 0 : $0.seconds * 1000 / Double($0.count))] }]
        }

        // The screen observed with no card: what the observer alone sees of the same workload
        let alone = FrameHealthScreen(), exact = FrameHealth(enabled: true)
        defer { exact.destroy() }
        alone.start(scene)
        let hidden = run(exact, alone)
        alone.stop()
        // The card measuring the screen over the same workload
        let health = FrameHealth(enabled: true)
        defer { health.destroy() }
        let overlay = FrameHealthOverlay(health: health)
        overlay.frame = window.bounds
        window.rootViewController?.view.addSubview(overlay)
        overlay.setVisible(true)
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        let screen = try XCTUnwrap(overlay.screen, "the card measures the screen")
        XCTAssertTrue(overlay.observing)
        let shown = run(health, screen, card: overlay.card)
        overlay.setVisible(false)
        let report: [String: Any] = ["benchmark": "frame-health-card-cost/1", "seconds": seconds, "observerAlone": hidden, "cardShown": shown,
            "qualification": "simulator; functional evidence only", "os": ProcessInfo.processInfo.operatingSystemVersionString,
            "displayMaximumHz": scene.screen.maximumFramesPerSecond, "scale": scene.screen.scale]
        let data = try JSONSerialization.data(withJSONObject: report, options: [.sortedKeys])
        print("FRAME-HEALTH-CARD-COST " + String(decoding: data, as: UTF8.self))
    }
}
#endif
