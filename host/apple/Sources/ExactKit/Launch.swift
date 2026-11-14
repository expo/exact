// Startup metrics with Observe's definitions (expo-app-metrics `AppStartupMarkers`).
// All marks are seconds on `CACurrentMediaTime` (mach_absolute_time).
//   launch time = (constructor − process start) + (didFinishLaunching − delegate init)
//   time to first render = first vsync after the first tree's commit − didFinishLaunching
// Time to first render assumes the render server shows a commit at the next vsync.
import CExact
import CExactLaunch
import Foundation
import QuartzCore
#if os(macOS)
import AppKit
#else
import UIKit
#endif

public final class ExactLaunch: NSObject {
    public static let shared = ExactLaunch()

    public enum Mark: String, CaseIterable {
        case process, constructor, main, delegateInit, didFinishLaunching, scene, boot, commit, present, world, activated, interactive
    }

    let facts = exact_launch_constructor_facts()
    /// Each mark's time in seconds, for the launch session only.
    private(set) var marks: [Mark: Double] = [:] {
        didSet {
            for (m, t) in marks where oldValue[m] == nil { ExactJournal.shared.record("mark", ["mark": m.rawValue], at: t) }
        }
    }
    /// Services the launch parts asked for, waiting for their moment.
    private var services: [(module: String, when: ExactServiceLoad, config: [String: Any], handoff: () -> Data)] = []
    private var loaded: Set<String> = []
    private var failed: Set<String> = []
    /// "cold", "warm", or nil before didFinishLaunching.
    private(set) var launchType: String?
    /// Why this launch reports no startup metrics, when it doesn't.
    private(set) var suppressed: String?
    private(set) var activation = "pending"
    private(set) var bootPath: String?
    private weak var launchSession: ExactSession?
    private var commitPending = false
    private(set) var link: CADisplayLink?
    private var vsync: Vsync?
    /// How time to interactive ended: settled, declared, failed, timeout, or a failure.
    private(set) var ttiOutcome: String?
    private(set) var lastOutstanding: [String] = []
    /// Each outstanding item's span, seconds on the `CACurrentMediaTime`
    /// clock: first seen at an evaluation, cleared at a later one (what held
    /// TTI, by item, not only the set at each change the trace keeps).
    private var spans: [String: (from: Double, to: Double?)] = [:]
    private var spanOrder: [String] = []
    /// Each batch that changed the screen after nothing was outstanding: ms
    /// from process start and what it changed.
    private var changes: [String] = []
    /// Each distinct set of outstanding work, with ms from process start, for
    /// debugging how time to interactive was reached.
    private(set) var trace: [String] = []
    private(set) var failedResources: [String] = []
    /// The screen changed since its last presented frame.
    private var contentDirty = false
    /// The vsync at which a settled screen becomes interactive, unless work reappears.
    private(set) var candidate: Double?
    private var evaluateScheduled = false
    /// The app's `aria-busy` elements were the last outstanding work.
    private var declaredLast = false
    static let ttiTimeout = 30.0

    override init() {
        super.init()
        if facts.valid != 0 {
            marks[.constructor] = facts.constructorMono
            marks[.process] = facts.constructorMono - facts.processAge
        }
        if facts.prewarm != 0 { suppressed = "prewarmed" }
    }

    // MARK: Host hooks (the adapter's app delegate)

    /// Called first thing in the adapter's `main`: what ran between the
    /// constructor and here is every other image's initializers.
    public func main() {
        if marks[.main] == nil { marks[.main] = CACurrentMediaTime() }
        // The app's preloads leave now, before UIKit. iOS prewarming runs this
        // long before a person opens the app, and then they wait (LLP 1162.000 D4).
        exact_preloads_begin(facts.prewarm != 0 ? 1 : 0)
    }

    /// Called from the app delegate's init, inside `UIApplicationMain` and after
    /// any prewarm pause. Observe calls this mark "main".
    public func delegateInit() {
        if marks[.delegateInit] == nil { marks[.delegateInit] = CACurrentMediaTime() }
        // The first code of a prewarmed process's real launch: what `main` deferred leaves here.
        exact_preloads_launched()
    }

    /// A person has opened the app: the preloads that wait for that leave
    /// (`"when": "foreground"`, LLP 1162.000 D4). On iOS this is the scene
    /// entering the foreground: a background start can connect a scene that is
    /// never drawn.
    public func opened() { exact_preloads_opened() }

    /// macOS has no prewarming: its `main` begins the preloads.
    public static func beginPreloads() { exact_preloads_begin(0) }

    /// Called as `didFinishLaunching` returns, after the session is made.
    /// `hidden` means macOS launched the app hidden, as a login item does.
    /// A scene-based iOS app reports `.background` here even for a tap on its
    /// icon, so `sceneConnected` detects background launches instead.
    public func didFinishLaunching(hidden: Bool = false) {
        guard marks[.didFinishLaunching] == nil else { return }
        marks[.didFinishLaunching] = CACurrentMediaTime()
        if hidden, suppressed == nil { suppressed = "hidden" }
        if marks[.delegateInit] == nil, suppressed == nil { suppressed = "no delegate mark" }
        launchType = classify()
        ExactJournal.shared.record("launch", ["type": launchType ?? "", "suppressed": suppressed ?? ""])
        watchInterruption()
        guard suppressed == nil, !ExactEnv.agentMode else { return }
        link = mainDisplayLink(self, #selector(tick(_:)))
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.ttiTimeout) { [weak self] in
            guard let self, ttiOutcome == nil, suppressed == nil else { return }
            if let o = outstandingNow() { lastOutstanding = o }
            finish("timeout")
        }
    }

    /// UIKit connects a foreground launch's scene within tens of ms of
    /// `didFinishLaunching`. A background launch (a push, a background task)
    /// gets its scene only when the user opens the app later. A gap over
    /// `backgroundGap` seconds therefore suppresses startup metrics.
    public func sceneConnected() {
        guard marks[.scene] == nil else { return }
        let now = CACurrentMediaTime()
        marks[.scene] = now
        if let dfl = marks[.didFinishLaunching], now - dfl > Self.backgroundGap, suppressed == nil {
            suppressed = "background launch"
            stopWatching()
        }
    }
    static let backgroundGap = 5.0

    // MARK: Session hooks

    /// The first session to boot after `didFinishLaunching` is the launch session.
    func bootEntered(_ session: ExactSession) {
        guard marks[.didFinishLaunching] != nil, launchSession == nil, marks[.boot] == nil else { return }
        launchSession = session
        marks[.boot] = CACurrentMediaTime()
    }

    /// The launch session's first tree is applied. Core Animation draws and
    /// commits it next, which `drawn` records.
    func treeApplied(_ session: ExactSession, path: String) {
        guard session === launchSession, marks[.commit] == nil, bootPath == nil else { return }
        bootPath = path
        commitPending = true
    }

    func activated(_ session: ExactSession, ok: Bool) {
        guard session === launchSession, activation == "pending" else { return }
        activation = ok ? "ready" : "failed"
        if ok { marks[.activated] = CACurrentMediaTime() } else { finish("activation_failed") }
        scheduleEvaluate()
    }

    /// What activation spent, seconds: the app's Swift module load, each
    /// `dataReady` call (the last one runs the data module's start), how
    /// long and how often it said pending, and applying its first batch.
    private var data: (module: Double, waited: Double, polls: Int, ready: Double, apply: Double, first: Double?) = (0, 0, 0, 0, 0, nil)

    func modulePrepared(_ session: ExactSession, seconds: Double) {
        guard session === launchSession, ttiOutcome == nil else { return }
        if data.first == nil { data.first = CACurrentMediaTime() - seconds }
        data.module += seconds
    }

    func dataStep(_ session: ExactSession, appModule: Double, ready: Double, pending: Bool) {
        guard session === launchSession, ttiOutcome == nil else { return }
        let now = CACurrentMediaTime()
        if data.first == nil { data.first = now - appModule - ready }
        data.module += appModule
        if pending {
            data.polls += 1
        } else {
            data.ready = ready
            data.waited = now - ready - data.module - (data.first ?? now)
        }
    }

    func dataApplied(_ session: ExactSession, _ seconds: Double) {
        guard session === launchSession, ttiOutcome == nil else { return }
        data.apply = seconds
    }

    /// The data module is still starting (activation pending): sample what
    /// is outstanding, so its span starts when it does, not at the next apply.
    func waitingForData(_ session: ExactSession) {
        guard session === launchSession, ttiOutcome == nil else { return }
        scheduleEvaluate()
    }

    /// Work ended (`ExactSession.workEnded`): re-reads outstanding work after
    /// this run-loop turn and its commit finish.
    func workEnded(_ session: ExactSession) {
        guard session === launchSession, ttiOutcome == nil else { return }
        scheduleEvaluate()
    }

    /// Records a batch that changed the screen, including an orientation change the presenter resolved.
    func applied(_ session: ExactSession, _ batch: Batch, orientationChanged: Bool = false) {
        guard session === launchSession, ttiOutcome == nil, batch.changesScreen || orientationChanged else { return }
        // Nothing outstanding, but the screen still changing: TTI waits for
        // a frame with no change, so this is what holds it then (a fresh
        // answer replacing a kept one, a fade). Its span: first to last.
        if lastOutstanding.isEmpty, marks[.present] != nil {
            let now = CACurrentMediaTime(), item = "screen:changing"
            if spans[item] == nil { spanOrder.append(item) }
            spans[item] = (spans[item]?.from ?? now, now)
            if changes.count < 12, let p = marks[.process] {
                changes.append("\(String(format: "%.0f", (now - p) * 1000)) \(Self.summary(batch, session.presenter))")
            }
        }
        contentDirty = true; candidate = nil
    }

    /// A batch's op kinds and a few of the views it touched (test ID, else
    /// kind), or what it changed with no ops, for the TTI trace.
    private static func summary(_ batch: Batch, _ presenter: Presenter) -> String {
        if batch.ops.isEmpty { return [batch.controls ? "controls" : nil, batch.facts ? "facts" : nil, batch.orientation != nil ? "orientation" : nil].compactMap { $0 }.joined(separator: " ") }
        var kinds: [String: Int] = [:], names: [String] = []
        for op in batch.ops {
            kinds[op.op.rawValue, default: 0] += 1
            guard op.nodeID != nil, names.count < 4 else { continue }
            let view = presenter.views[op.id]; let name = view?.props["testId"] ?? view?.kind ?? op.kind
            if !names.contains(name) { names.append(name) }
        }
        let ops = kinds.sorted { $0.value > $1.value }.prefix(3).map { "\($0.key)×\($0.value)" }.joined(separator: " ")
        return names.isEmpty ? ops : "\(ops) (\(names.joined(separator: ", ")))"
    }

    // MARK: Commit and vsync

    /// The first tree's layers draw inside the Core Animation commit that
    /// carries them, so this marks the commit. A run-loop observer would miss
    /// commits that macOS makes from HIToolbox's own run-loop mode.
    func drawn(_ session: ExactSession) {
        guard session === launchSession, commitPending else { return }
        commitPending = false
        marks[.commit] = CACurrentMediaTime()
        if link == nil { link = mainDisplayLink(self, #selector(tick(_:))) }
        resolvePresent()
    }

    private var firstWorldPresentation: Double?

    /// Keep the first gpu_on_presented notice until the tree presents, so the
    /// present → world → interactive marks cannot date the world before first pixel.
    public func worldPresented() {
        guard launchSession != nil, firstWorldPresentation == nil else { return }
        firstWorldPresentation = CACurrentMediaTime()
        resolveWorldPresentation()
    }

    private func resolveWorldPresentation() {
        guard marks[.world] == nil, let at = firstWorldPresentation,
              let present = marks[.present] else { return }
        marks[.world] = max(at, present)
    }

    @objc private func tick(_ link: CADisplayLink) { frame(Vsync(link)) }

    /// A display-link callback's sample (a test's own, as its timestamps are read-only).
    func frame(_ sample: Vsync) {
        vsync = sample
        resolvePresent()
        if marks[.present] != nil, ttiOutcome != nil || suppressed != nil || ExactEnv.agentMode { stopWatching(); return }
        confirmCandidate()
    }

    /// Marks `present` as the first vsync after the commit, on the last sample's
    /// grid. A late display-link callback still carries its own vsync time, so
    /// a busy main thread does not move the mark.
    private func resolvePresent() {
        guard let commit = marks[.commit], marks[.present] == nil, let present = vsync?.next(after: commit) else { return }
        marks[.present] = present
        resolveWorldPresentation()
        loadServices(.afterFirstPixel)
        if ttiOutcome != nil || suppressed != nil || ExactEnv.agentMode { loadServices(.afterStartup) }
        scheduleEvaluate()
    }

    // MARK: Time to interactive

    private func scheduleEvaluate() {
        guard !evaluateScheduled, ttiOutcome == nil, suppressed == nil, !ExactEnv.agentMode else { return }
        evaluateScheduled = true
        DispatchQueue.main.async { [weak self] in
            self?.evaluateScheduled = false
            self?.evaluate()
        }
    }

    /// The launch session's outstanding work, from the runner and the host.
    /// Empty when the screen can be used.
    private func outstandingNow() -> [String]? {
        guard let session = launchSession, let ledger = Self.ledger(session) else { return nil }
        if ledger.poisoned { finish("logic_failed"); return nil }
        failedResources = ledger.failed
        return ledger.items + (activation != "ready" ? ["activation"] : [])
    }

    /// A session's outstanding work from the runner and the host, as
    /// `kind:name` items, plus failed resources and whether the runner is
    /// poisoned. Nil when there is none to read: a destroyed session, or a
    /// runner that answers with an error (busy, gone), whose missing lists
    /// would otherwise read as nothing outstanding. The runner's call comes
    /// first and waits for the owner, so work sent there before it has
    /// published its batch when `hostOutstanding` reads the published queue.
    static func ledger(_ session: ExactSession) -> (items: [String], failed: [String], poisoned: Bool)? {
        guard session.state != .destroyed, let data = session.agent("{\"op\":\"outstanding\"}").data(using: .utf8),
              let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any], o["error"] == nil else { return nil }
        var out: [String] = []
        for key in ["requests", "streams", "awaiting", "oneShots", "busy"] {
            out += (o[key] as? [String] ?? []).map { "\(key):\($0)" }
        }
        out += session.hostOutstanding.map { "host:\($0)" }
        return (out, o["failed"] as? [String] ?? [], o["poisoned"] as? Bool == true)
    }

    /// Runs the turn after work ended or activation. With nothing outstanding,
    /// the interactive mark is the next vsync if the screen changed, else now.
    /// It is never earlier than the first render.
    private func evaluate() {
        guard ttiOutcome == nil, suppressed == nil, let outstanding = outstandingNow() else { return }
        if outstanding.isEmpty, !lastOutstanding.isEmpty, lastOutstanding.allSatisfy({ $0.hasPrefix("busy:") }) { declaredLast = true }
        if outstanding != lastOutstanding || trace.isEmpty, trace.count < 32, let p = marks[.process] {
            trace.append("\(String(format: "%.1f", (CACurrentMediaTime() - p) * 1000)) [\(outstanding.joined(separator: ", "))]")
        }
        let seen = CACurrentMediaTime()
        for item in outstanding where spans[item]?.to != nil || spans[item] == nil {
            if spans[item] == nil { spanOrder.append(item) }
            spans[item] = (spans[item]?.from ?? seen, nil)
        }
        for item in lastOutstanding where !outstanding.contains(item) { spans[item]?.to = seen }
        lastOutstanding = outstanding
        guard outstanding.isEmpty, marks[.present] != nil else { candidate = nil; return }
        let now = CACurrentMediaTime()
        if contentDirty {
            // A candidate stands until work reappears or a batch changes the
            // screen (`applied`): work that ends showing nothing new, after
            // its vsync and before the callback that confirms it, would
            // otherwise move it a frame later each time.
            if candidate == nil { candidate = vsync?.next(after: now) }
        } else {
            interactive(at: now)
        }
    }

    /// Confirms the candidate vsync if no work became outstanding before it.
    private func confirmCandidate() {
        guard let target = candidate, let v = vsync, v.at >= target, ttiOutcome == nil else { return }
        candidate = nil
        guard let outstanding = outstandingNow(), outstanding.isEmpty else { scheduleEvaluate(); return }
        contentDirty = false
        interactive(at: target)
    }

    private func interactive(at t: Double) {
        marks[.interactive] = max(t, marks[.present] ?? t)
        finish(!failedResources.isEmpty ? "failed" : declaredLast ? "declared" : "settled")
    }

    private func finish(_ outcome: String) {
        guard ttiOutcome == nil else { return }
        ttiOutcome = outcome
        stopWatching()
        ExactJournal.shared.record("startup", report())
        if ExactEnv.environment["EXACT_OBSERVE_LOG"] == "1", launchSession != nil { fputs("observe: startup \(smokeLine())\n", stderr) }
        if marks[.present] != nil { loadServices(.afterStartup) }
    }

    func requestService(module: String, when: ExactServiceLoad, config: [String: Any], handoff: @escaping () -> Data) {
        services.append((module, when, config, handoff))
        if Self.serviceIsReady(when, presented: marks[.present] != nil, settled: ttiOutcome != nil,
                               suppressed: suppressed != nil || ExactEnv.agentMode) { loadServices(when) }
    }

    static func serviceIsReady(_ when: ExactServiceLoad, presented: Bool, settled: Bool, suppressed: Bool) -> Bool {
        presented && (when == .afterFirstPixel || settled || suppressed)
    }

    /// Loads due services in a later turn, never inside the work that triggered them.
    private func loadServices(_ moment: ExactServiceLoad) {
        for s in services where (s.when == moment || moment == .afterStartup) && !loaded.contains(s.module) { load(s) }
    }

    /// Schedules a declared service for a query, sharing a load already in flight.
    /// Returns false when the service is undeclared or its load failed. Main thread only.
    func loadForQuery(_ module: String) -> Bool {
        guard let s = services.first(where: { $0.module == module }), !failed.contains(module) else { return false }
        if !loaded.contains(module) { load(s) }
        return true
    }

    private func load(_ s: (module: String, when: ExactServiceLoad, config: [String: Any], handoff: () -> Data)) {
        loaded.insert(s.module)
        DispatchQueue.main.async {
            let config = (try? JSONSerialization.data(withJSONObject: s.config)) ?? Data("{}".utf8)
            guard let service = ExactService.load(module: s.module, config: config, handoff: s.handoff()) else {
                self.failed.insert(s.module)
                return ExactJournal.shared.loadEnded(s.module, nil)
            }
            ExactJournal.shared.attach(service)
        }
    }

    private func stopWatching() {
        // Keep the first committed frame observable when startup is interrupted
        guard marks[.commit] == nil || marks[.present] != nil else { return }
        link?.invalidate()
        link = nil
    }

    private func watchInterruption() {
        #if os(macOS)
        let name = NSApplication.didHideNotification
        #else
        let name = UIApplication.didEnterBackgroundNotification
        #endif
        NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
            ExactJournal.shared.record("background")
            ExactJournal.shared.background()
            guard let self, ttiOutcome == nil, suppressed == nil else { return }
            suppressed = "interrupted"
            stopWatching()
            if marks[.present] != nil { loadServices(.afterStartup) }
        }
    }

    // MARK: Cold or warm, by Observe's heuristic (`AppStartupMonitoring.getAppLaunchType`)

    private static let defaults = UserDefaults(suiteName: "dev.exact.observe")
    private func classify() -> String {
        let boot = Self.bootTime()
        let build = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? ""
        let previous = Self.defaults?.dictionary(forKey: "launch")
        Self.defaults?.set(["boot": boot, "build": build], forKey: "launch")
        guard let previous, previous["build"] as? String == build else { return "cold" }
        if facts.tty != 0 { return "cold" }
        guard let was = previous["boot"] as? Double, abs(was - boot) < 1 else { return "cold" }
        return "warm"
    }

    private static func bootTime() -> Double {
        var t = timeval()
        var size = MemoryLayout<timeval>.stride
        var mib: [Int32] = [CTL_KERN, KERN_BOOTTIME]
        guard sysctl(&mib, 2, &t, &size, nil, 0) == 0 else { return 0 }
        return Double(t.tv_sec) + Double(t.tv_usec) / 1e6
    }

    // MARK: Report for `state.observe` and the smoke test

    /// Observe's metric names with values in seconds, plus Exact's marks in ms from process start.
    public func report(for session: ExactSession? = nil) -> [String: Any] {
        var out: [String: Any] = ["activation": activation]
        if let launchType { out["launchType"] = launchType }
        if let suppressed { out["suppressed"] = suppressed }
        if let bootPath { out["bootPath"] = bootPath }
        if let ttiOutcome { out["tti"] = ttiOutcome }
        if !lastOutstanding.isEmpty { out["outstanding"] = lastOutstanding }
        if !failedResources.isEmpty { out["failed"] = failedResources }
        if !trace.isEmpty { out["trace"] = trace }
        if !changes.isEmpty { out["changes"] = changes }
        // Boot's two parts (ms): the runner's boot with the first layout, and
        // applying its batch to the views.
        if let s = launchSession, s.rustMs > 0 {
            var boot: [String: Any] = ["runner": (s.rustMs * 10).rounded() / 10, "apply": (s.applyMs * 10).rounded() / 10]
            // The apply's largest parts, so a slow first batch says which part.
            let parts = s.applyParts.filter { $0.value >= 0.5 }.sorted { $0.value > $1.value }
            if !parts.isEmpty { boot["parts"] = parts.filter { $0.key != "runner" }.prefix(16).map { $0.key.hasPrefix("#") ? "\($0.key.dropFirst()) ×\(Int($0.value))" : "\($0.key) \(Int($0.value.rounded()))" }.joined(separator: ", ") }
            out["boot"] = boot
        }
        if data.first != nil {
            let ms = { (s: Double) in (s * 1000 * 10).rounded() / 10 }
            // From the first commit to the first node's draw receipt, which
            // starts activation (the render server's first draw).
            let receipt = marks[.commit].flatMap { c in data.first.map { ms(max(0, $0 - c)) } }
            out["data"] = ["appModule": ms(data.module), "waited": ms(max(0, data.waited)), "polls": data.polls,
                           "ready": ms(data.ready), "apply": ms(data.apply), "receipt": receipt as Any]
        }
        if let p = marks[.process], !spans.isEmpty {
            let ms = { (t: Double) in ((t - p) * 1000 * 10).rounded() / 10 }
            out["items"] = spanOrder.compactMap { item -> [String: Any]? in
                guard let span = spans[item] else { return nil }
                var o: [String: Any] = ["item": item, "from": ms(span.from)]
                if let to = span.to { o["to"] = ms(to) }
                return o
            }
        }
        if !NavigationMarks.shared.recent.isEmpty { out["navigation"] = NavigationMarks.shared.recent }
        let preloads = preloadMarks()
        if !preloads.isEmpty { out["preloads"] = preloads }
        if let session { out["launchSession"] = session === launchSession }
        if facts.traced != 0 { out["debugger"] = true }
        if let p = marks[.process] {
            out["marks"] = Dictionary(uniqueKeysWithValues: marks.map { ($0.key.rawValue, (($0.value - p) * 1000 * 10).rounded() / 10) })
        }
        out["metrics"] = metrics()
        return out
    }

    /// Each preload's marks in ms from process start (from the delegate's
    /// initializer on a prewarmed launch, whose process start is the prewarm):
    /// handed to the client, reply complete, and the commit that first carried
    /// its content, with that commit's place after the first (LLP 1162.000 D4).
    func preloadMarks() -> [[String: Any]] {
        let origin = facts.prewarm != 0 ? marks[.delegateInit] : marks[.process]
        guard let origin else { return [] }
        let ms = { (t: Double) in ((t - origin) * 1000 * 10).rounded() / 10 }
        return (0..<exact_preloads_count()).map { index in
            let mark = exact_preloads_mark(index)
            var bytes = [UInt8](repeating: 0, count: exact_preloads_name(index, nil, 0))
            _ = exact_preloads_name(index, &bytes, bytes.count)
            var row: [String: Any] = ["name": String(decoding: bytes, as: UTF8.self)]
            if mark.handed > 0 { row["handed"] = ms(mark.handed) }
            if mark.completed > 0 { row["completed"] = ms(mark.completed) }
            if mark.committed > 0 {
                row["committed"] = ms(mark.committed)
                row["sequence"] = mark.sequence
                if let first = marks[.commit] { row["afterFirstCommit"] = ((mark.committed - first) * 1000 * 10).rounded() / 10 }
            }
            return row
        }
    }

    func metrics() -> [String: Double] {
        guard suppressed == nil, let dfl = marks[.didFinishLaunching] else { return [:] }
        var m: [String: Double] = [:]
        if let p = marks[.process], let l = marks[.constructor], let md = marks[.delegateInit], let type = launchType {
            m["\(type)LaunchTime"] = (l - p) + (dfl - md)
        }
        if let f0 = marks[.present] { m["timeToFirstRender"] = f0 - dfl }
        if let i = marks[.interactive] { m["timeToInteractive"] = i - dfl }
        return m
    }
}

extension ExactLaunch {
    /// One line for `EXACT_SMOKE`: the metrics in ms and the marks from process start.
    public func smokeLine() -> String {
        let metrics = metrics().sorted { $0.key < $1.key }
            .map { "\($0.key) \(String(format: "%.1f", $0.value * 1000)) ms" }.joined(separator: "; ")
        let marks = self.marks[.process].map { p in
            Mark.allCases.compactMap { m in self.marks[m].map { "\(m.rawValue) \(String(format: "%.1f", ($0 - p) * 1000))" } }.joined(separator: " · ")
        } ?? ""
        let line = "\(launchType ?? "?") \(bootPath ?? "?") tti \(ttiOutcome ?? "pending")\(lastOutstanding.isEmpty ? "" : " outstanding \(lastOutstanding)") \(suppressed.map { "suppressed: \($0) " } ?? "")activation \(activation); \(metrics.isEmpty ? "no metrics" : metrics); marks \(marks); ledger \(trace.joined(separator: " → "))"
        // Each preload: handed to the client, reply complete, the commit that carried it (LLP 1162.000 D4).
        let preloads = preloadMarks().map { row -> String in
            let part = { (key: String) in (row[key] as? Double).map { " \(key) \(String(format: "%.1f", $0))" } ?? "" }
            return " · preload \(row["name"] as? String ?? "?")" + part("handed") + part("completed") + part("committed") + part("afterFirstCommit")
        }.joined()
        return line + preloads
    }
}

/// A display link on the main run loop's common modes, so it fires during tracking.
func mainDisplayLink(_ target: Any, _ selector: Selector) -> CADisplayLink? {
    #if os(macOS)
    let link = NSScreen.main?.displayLink(target: target, selector: selector)
    #else
    let link: CADisplayLink? = CADisplayLink(target: target, selector: selector)
    #endif
    link?.add(to: .main, forMode: .common)
    return link
}

/// A display-link sample: its vsync time and observed interval, in seconds.
struct Vsync {
    let at: Double
    let interval: Double

    /// The first vsync strictly after `t`, on this sample's grid.
    func next(after t: Double) -> Double? {
        guard interval > 0 else { return nil }
        let g = at + ((t - at) / interval).rounded(.up) * interval
        return g <= t ? g + interval : g
    }
}

// An extension, so the struct keeps its memberwise initializer.
extension Vsync {
    init(_ link: CADisplayLink) {
        // On ProMotion, `duration` can stay nominal while the real cadence changes.
        let interval = link.targetTimestamp - link.timestamp
        self.init(at: link.timestamp, interval: interval > 0 ? interval : link.duration)
    }
}
