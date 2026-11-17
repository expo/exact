#if os(iOS)
import UIKit
import ExactCore

/// An opt-in, thirty-second recording with no card, chart, display-rate request or per-frame serialization.
final class TextDiagnostics {
    static let shared = TextDiagnostics()
    private struct Update {
        var scheduled = 0.0, start = 0.0, deadline = 0.0, beforeCommit = 0.0, finished = 0.0, waited = 0.0
    }
    private var updates = Array(repeating: Update(), count: 8192)
    private var written = 0
    private var origin = 0.0
    private var screen: FrameHealthScreen?
    private var timer: Timer?
    private var background: NSObjectProtocol?
    private weak var session: ExactSession?
    private(set) var saved: URL?
    var recording: Bool { screen != nil }

    func start() {
        guard let session = DevMenu.session, let scene = DevMenu.frameHealthContainer?.window?.windowScene else {
            explain("Recording could not start because the app has no active scene. Return to the gallery and try again.")
            return
        }
        if session.frameHealth.hudVisible { DevMenu.frameHealthOverlay()?.setVisible(false) }
        if recording { finish(reason: "restarted") }
        self.session = session
        origin = CACurrentMediaTime(); written = 0; saved = nil
        let screen = FrameHealthScreen()
        screen.workload = { [weak session] in session.map { FrameHealth.workload($0.frames, session: $0) } ?? 0 }
        screen.completedUpdate = { [weak self] scheduled, start, deadline, before, finished, waited, _ in
            guard let self else { return }
            updates[written % updates.count] = Update(scheduled: scheduled, start: start, deadline: deadline, beforeCommit: before,
                finished: finished, waited: waited)
            written += 1
        }
        screen.start(scene)
        guard screen.running, screen.health.enabled else {
            screen.stop()
            explain("Recording could not start because UI-update observation is unavailable. This capture requires iOS 18 or later and a free frame-health collector.")
            return
        }
        self.screen = screen
        _ = screen.health.setAgentCapture("start", duration: 30_000)
        TextRenderMetrics.shared.start()
        let timer = Timer(timeInterval: 30, repeats: false) { [weak self] _ in self?.finish(reason: "duration") }
        self.timer = timer
        RunLoop.main.add(timer, forMode: .common)
        background = NotificationCenter.default.addObserver(forName: UIScene.didEnterBackgroundNotification, object: scene, queue: .main) { [weak self] _ in
            self?.finish(reason: "background")
        }
    }

    func finish(reason: String = "manual") {
        guard let screen else { return }
        let ended = CACurrentMediaTime()
        timer?.invalidate(); timer = nil
        if let background { NotificationCenter.default.removeObserver(background); self.background = nil }
        TextRenderMetrics.shared.stop()
        screen.completedUpdate = nil
        screen.stop()
        self.screen = nil
        // Export while the bounded capture is still armed; stop() preserves its records.
        let health = FrameHealthReport.snapshot(health: screen.health, session: session, now: ended)
        _ = screen.health.setAgentCapture("stop")
        let us: (Double) -> Int = { Int((($0 - self.origin) * 1e6).rounded()) }
        let rows: [[Any]] = (max(0, written - updates.count)..<written).map { index in
            let u = updates[index % updates.count]
            return [us(u.scheduled), us(u.start), us(u.deadline), u.beforeCommit > 0 ? us(u.beforeCommit) as Any : NSNull(), us(u.finished),
                    Int((u.waited * 1e6).rounded()), u.finished > u.deadline]
        }
        let packet: [String: Any] = ["format": "exact.text-diagnostics/2", "reason": reason, "duration_us": us(ended),
            "renderer": "profiled-coverage", "cardVisibleDuringCapture": false, "rateRequestedByCapture": false, "health": health,
            "updates": ["capacity": updates.count, "written": written, "overwritten": max(0, written - updates.count),
                        "columns": ["scheduled_start_us", "synthetic_work_start_us", "deadline_us", "before_last_commit_us", "after_last_commit_us", "waited_us", "late"], "rows": rows],
            "text": TextRenderMetrics.shared.snapshot(origin: origin),
            "fallbackBits": ["1": "HDR-paint", "2": "HDR-paragraph-shadow", "4": "complex-paint-or-color-glyphs"],
            "singleColorPublicationDetail": ["0": "new-coverage", "1": "reused-coverage"],
            "workerWaitDetail": ["0": "timed-out", "1": "completed"],
            "limits": ["Elapsed spans include descheduling; worker spans may overlap and must not be added to the main-thread budget.",
                       "Single-color publication includes nested coveragePaint when a glyph mask is generated; do not add the two spans. It does not measure GPU upload or presentation.",
                       "Fallback bits describe tested blockers; eligibility checks stop at the first failing stage.",
                       "Glyph dimensions are exact mask dimensions; image_bytes counts image capacity, not measured backing-store allocation.",
                       "Use scheduled_start for span joins; synthetic_work_start shifts it by total run-loop waits and is not a real wake timestamp.",
                       "The last commit bracket includes Core Animation work beyond text; updates without a commit use update completion.",
                       "Fixed rings and span locks add diagnostic overhead; this is not a memory baseline."]]
        do {
            let directory = try FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            let url = directory.appendingPathComponent("Exact-text-diagnostics.json")
            try JSONSerialization.data(withJSONObject: packet, options: [.sortedKeys]).write(to: url, options: .atomic)
            saved = url
        } catch {
            explain("The recording finished, but its JSON file could not be saved: \(error). Free device storage and record again.")
        }
    }

    func share() {
        if recording { finish() }
        guard let saved else {
            explain("There is no saved recording. Choose Record Text Diagnostics, reproduce the stutter for 30 seconds, then share the recording.")
            return
        }
        DevMenu.share(saved)
    }

    private func explain(_ message: String) {
        let alert = UIAlertController(title: "Text Diagnostics", message: message, preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: "OK", style: .default))
        DevMenu.present(alert)
    }
}
#endif
