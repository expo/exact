// Observe's service: a separate dylib ExactKit loads after startup. It stores journal
// events as Observe's metric and log rows, records crashes from earlier launches, and
// sends the rows the way expo-observe does. Event processing, queries and export run
// on one serial background queue.
import Foundation
#if os(iOS)
import UIKit
#endif

final class ObserveService {
    let queue = DispatchQueue(label: "exact.observe.service")
    let config: [String: Any]
    let session: String
    private(set) var store: ObserveStore?
    let clientId: UUID
    let metadata: [String: Any]
    var device: [String: Any] = [:]
    /// The route shown at launch. If the user navigates before startup ends, it gets no `tti`.
    var launchRoute: [String: Any]?
    var navigatedBeforeStartup = false
    var globals: [String: Any] = [:]
    var gate: (after: Date?, failures: Int) = (nil, 0)
    var sending = false
    var scheduled = false

    static let logEnabled = ProcessInfo.processInfo.environment["EXACT_OBSERVE_LOG"] == "1"

    /// Batch new rows for five seconds, or wake at the retry deadline. Only one timer is pending.
    func scheduleDispatch(after delay: TimeInterval = 5) {
        guard !scheduled else { return }
        scheduled = true
        queue.asyncAfter(deadline: .now() + max(0, delay)) { [weak self] in
            self?.scheduled = false
            self?.dispatch()
        }
    }

    /// The crash directory passed by the launch part, shared with `ObserveCrash`
    let crashDirectory: String?

    /// `root` and `clientId` are for tests: `root` holds the store and the crash records.
    init(config: [String: Any], handoff: Data, root: URL? = nil, clientId fixedId: UUID? = nil) {
        self.config = config
        session = config["session"] as? String ?? UUID().uuidString
        if let fixedId {
            clientId = fixedId
        } else {
            let defaults = UserDefaults.standard
            let id = defaults.string(forKey: "expo.eas-client-id") ?? UUID().uuidString
            defaults.set(id, forKey: "expo.eas-client-id")
            clientId = UUID(uuidString: id) ?? UUID()
        }
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let directory = root ?? base.appendingPathComponent("exact/\(config["app.CFBundleIdentifier"] as? String ?? Bundle.main.bundleIdentifier ?? "app")/observe", isDirectory: true)
        crashDirectory = root?.path ?? config["crashDirectory"] as? String
        metadata = Self.metadata(config)
        for record in Self.records(handoff) {
            if record["kind"] as? String == "device" { device = record["params"] as? [String: Any] ?? [:] }
        }
        if Self.logEnabled { fputs("observe: service started, session \(session)\n", stderr) }
        queue.async { [self] in
            store = ObserveStore(directory: directory)
            store?.saveSession(session, start: config["sessionStart"] as? Double ?? Date().timeIntervalSince1970, metadata: metadata)
            ingestCrashes()
            scheduleDispatch()
        }
    }

    // MARK: Journal events

    func event(_ json: Data) {
        queue.async { [self] in
            guard let e = (try? JSONSerialization.jsonObject(with: json)) as? [String: Any], let kind = e["kind"] as? String else { return }
            let wall = e["wall"] as? Double ?? Date().timeIntervalSince1970
            switch kind {
            case "startup": startup(e, wall: wall)
            case "navigation.launch": launchRoute = e
            case "navigation": navigation(e, wall: wall)
            case "update.download": updateDownload(e, wall: wall)
            case "app.attributes": globals = ObserveRules.attributes(e["attributes"] as? [String: Any] ?? [:]).kept
            case "app.event": log(e, wall: wall, error: false)
            case "app.error": log(e, wall: wall, error: true)
            case "background": dispatch()
            default: return
            }
            if kind != "background" { scheduleDispatch() }
        }
    }

    /// Stores the launch metrics from ExactLaunch's startup report under Observe's names.
    func startup(_ e: [String: Any], wall: Double) {
        launchNavigation(e, wall: wall)
        guard let store, let metrics = e["metrics"] as? [String: Double], !metrics.isEmpty else { return }
        let marks = e["marks"] as? [String: Double] ?? [:]
        let ms = { (a: String, b: String) -> NSDecimalNumber? in
            guard let x = marks[a], let y = marks[b] else { return nil }
            return Self.seconds(x - y)
        }
        var phases: [String: Any] = [:]
        // Before the app has control: dyld and this image's constructor, the
        // other images' initializers up to `main`, UIKit up to the app
        // delegate's init, and the delegate's launch up to didFinishLaunching.
        phases["exact.phase.exec"] = ms("constructor", "process")
        phases["exact.phase.initializers"] = ms("main", "constructor")
        phases["exact.phase.uikit"] = ms("delegateInit", "main")
        phases["exact.phase.launching"] = ms("didFinishLaunching", "delegateInit")
        phases["exact.phase.scene"] = ms("scene", "didFinishLaunching")
        phases["exact.phase.boot_wait"] = ms("boot", "scene")
        phases["exact.phase.boot"] = ms("commit", "boot")
        phases["exact.phase.present"] = ms("present", "commit")
        phases["exact.phase.activate"] = ms("activated", "commit")
        phases["exact.since_process_start.ttr"] = marks["present"].map { Self.seconds($0) }
        phases["exact.since_process_start.tti"] = marks["interactive"].map { Self.seconds($0) }
        phases["exact.present.method"] = "next_vsync"
        // Activation's parts (ms): the app module's load, the wait for the
        // data module (and how many polls), its ready call, its first batch.
        if let b = e["boot"] as? [String: Any] {
            for key in ["runner", "apply"] { if let v = b[key] as? Double { phases["exact.boot.\(key)"] = Self.seconds(v) } }
            if let parts = b["parts"] as? String { phases["exact.boot.apply_parts"] = parts }
        }
        if let d = e["data"] as? [String: Any] {
            for (key, name) in [("receipt", "draw_receipt"), ("appModule", "app_module"), ("waited", "wait"), ("ready", "ready"), ("apply", "apply")] {
                if let v = d[key] as? Double { phases["exact.data.\(name)"] = Self.seconds(v) }
            }
            if let polls = d["polls"] as? Int { phases["exact.data.polls"] = polls }
        }
        if let path = e["bootPath"] { phases["exact.boot.path"] = path }
        // Cold or warm, by Observe's heuristic (a reboot since the last launch).
        if let type = e["launchType"] { phases["exact.launch.type"] = type }
        if e["debugger"] as? Bool == true { phases["exact.launch.debugger"] = true }
        for (name, value) in metrics {
            var params = globals
            for (k, v) in phases { params[k] = v }
            if name == "timeToInteractive" {
                for (k, v) in device { params[k] = v }
                params["exact.tti.reason"] = e["tti"] ?? "settled"
                // What was outstanding over time, ms from process start: shows what held TTI.
                if let trace = e["trace"] as? [String], !trace.isEmpty { params["exact.tti.trace"] = trace.joined(separator: " → ") }
                // Batches that still changed the screen once nothing was outstanding.
                if let changes = e["changes"] as? [String], !changes.isEmpty { params["exact.tti.changes"] = changes.joined(separator: "; ") }
                // Each item that held it, ms from process start, the latest to clear first.
                if let items = e["items"] as? [[String: Any]], !items.isEmpty {
                    let ordered = items.sorted { ($0["to"] as? Double ?? .infinity) > ($1["to"] as? Double ?? .infinity) }
                    params["exact.tti.items"] = ordered.map { item -> String in
                        let from = String(format: "%.0f", item["from"] as? Double ?? 0)
                        let to = (item["to"] as? Double).map { String(format: "%.0f", $0) } ?? "…"
                        return "\(item["item"] as? String ?? "?") \(from)–\(to)"
                    }.joined(separator: "; ")
                }
                if let failed = e["failed"] as? [String] { params["exact.tti.failed"] = failed.count }
            }
            store.addMetric(session: session, time: wall, category: "appStartup", name: name, value: value, params: params)
        }
    }

    /// Observe's `expo.navigation.*` metrics. The route is the pattern, not the concrete path.
    func navigation(_ e: [String: Any], wall: Double) {
        guard let store, let name = e["name"] as? String, let value = e["value"] as? Double else { return }
        navigatedBeforeStartup = true
        var params = navigationParams(e, launch: false)
        for (k, v) in e where k.hasPrefix("exact.") { params[k] = v }
        store.addMetric(session: session, time: wall, category: "navigation", name: name, value: value, route: e["route"] as? String, params: params)
    }

    /// The launch route's `cold_ttr` and `tti`, measured from boot. Observe measures from
    /// its integration start, which Exact has no equivalent of; boot is the closest mark.
    func launchNavigation(_ e: [String: Any], wall: Double) {
        guard let store, let route = launchRoute, let marks = e["marks"] as? [String: Double], let boot = marks["boot"] else { return }
        var params = navigationParams(route, launch: true)
        params["exact.nav.anchor"] = "boot"
        if let present = marks["present"] {
            store.addMetric(session: session, time: wall, category: "navigation", name: "cold_ttr", value: (present - boot) / 1000, route: route["route"] as? String, params: params)
        }
        if let interactive = marks["interactive"], !navigatedBeforeStartup, (e["metrics"] as? [String: Any])?["timeToInteractive"] != nil {
            store.addMetric(session: session, time: wall, category: "navigation", name: "tti", value: (interactive - boot) / 1000, route: route["route"] as? String, params: params)
        }
    }

    /// Observe's navigation params, on top of the global attributes.
    func navigationParams(_ e: [String: Any], launch: Bool) -> [String: Any] {
        var params = globals.merging(["isAppLaunch": launch, "routeParams": e["routeParams"] ?? [:]]) { $1 }
        if let url = e["url"] { params["url"] = url }
        return params
    }

    /// Observe's `updates/updateDownloadTime`: from the first request to the update being staged.
    /// The update id is Exact's envelope digest, not an EAS update id.
    func updateDownload(_ e: [String: Any], wall: Double) {
        guard let store, let seconds = e["seconds"] as? Double else { return }
        store.run("INSERT INTO metrics (session, time, category, name, value, updateId, params) VALUES (?, ?, ?, ?, ?, ?, ?)",
                  [session, wall, "updates", "updateDownloadTime", seconds, e["entry"] as? String,
                   String(decoding: (try? JSONSerialization.data(withJSONObject: ["exact.update.files_fetched": e["files"] ?? 0, "exact.update.seq": e["seq"] ?? 0, "exact.update.id_kind": "digest"], options: [.sortedKeys])) ?? Data(), as: UTF8.self)])
    }

    func log(_ e: [String: Any], wall: Double, error: Bool) {
        guard let store else { return }
        if error {
            var attrs: [String: Any] = ["expo.error.source": e["source"] as? String ?? "reportedByUser", "expo.error.is_fatal": false,
                                        "exception.type": e["type"] as? String ?? "Error", "exception.message": e["message"] as? String ?? ""]
            if let stack = e["stack"] as? String { attrs["exception.stacktrace"] = stack }
            store.addLog(session: session, time: wall, severity: "error", name: "js.exception", body: nil, attributes: attrs, dropped: 0)
            return
        }
        guard let name = ObserveRules.name(e["name"] as? String ?? "") else { return }
        let user = ObserveRules.attributes(e["attributes"] as? [String: Any] ?? [:])
        var attrs = globals
        for (k, v) in user.kept { attrs[k] = v }
        if let display = ObserveRules.displayName(e["displayName"] as? String) { attrs["expo.log.display_name"] = display }
        let severity = (e["severity"] as? String).flatMap { ObserveWire.severities[$0] != nil ? $0 : nil } ?? "info"
        store.addLog(session: session, time: wall, severity: severity, name: name, body: ObserveRules.body(e["body"] as? String),
                     attributes: attrs, dropped: user.dropped)
    }

    /// Turns each pending ObserveCrash record into a `native.exception` log for the crashed session.
    /// A record is the only copy of its crash, so it is removed only once its row is stored.
    func ingestCrashes() {
        guard let store, let dir = crashDirectory,
              let files = try? FileManager.default.contentsOfDirectory(atPath: dir) else { return }
        for f in files where f.hasPrefix("exact-observe-pending-") && !f.contains(session) {
            let path = "\(dir)/\(f)"
            guard let data = FileManager.default.contents(atPath: path),
                  let o = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
                  let crashed = o["session"] as? String, let signal = o["signal"] as? Int else {
                fputs("observe: an incomplete pending crash record cannot be read and is removed\n", stderr)
                try? FileManager.default.removeItem(atPath: path)
                continue
            }
            let start = o["sessionStart"] as? Double ?? 0
            store.saveSession(crashed, start: start, metadata: metadata)
            let names = [6: "SIGABRT", 11: "SIGSEGV", 10: "SIGBUS", 4: "SIGILL", 8: "SIGFPE", 5: "SIGTRAP"]
            let attrs: [String: Any] = ["exception.type": names[signal] ?? "SIG\(signal)", "exception.message": "fatal signal \(signal)",
                                        "expo.error.source": "nativeCrash", "expo.error.is_fatal": true, "expo.crash.signal_number": signal]
            let modified = (try? FileManager.default.attributesOfItem(atPath: path)[.modificationDate] as? Date)?.timeIntervalSince1970 ?? start
            if store.addLog(session: crashed, time: modified, severity: "fatal", name: "native.exception", body: nil, attributes: attrs, dropped: 0) {
                try? FileManager.default.removeItem(atPath: path)
            } else {
                fputs("observe: a pending crash record could not be stored and is kept for the next launch\n", stderr)
            }
        }
    }

    // MARK: Dispatch

    /// Observe's gate: dispatching enabled, this install in the sample, and not a development
    /// build unless `dispatchInDebug`. Refusal advances processing without acknowledging rows.
    var shouldDispatch: Bool {
        let enabled = config["dispatchingEnabled"] as? Bool ?? true
        let rate = min(max(config["sampleRate"] as? Double ?? 1, 0), 1)
        let development = config["fact.development"] as? Bool == true || config["fact.agent"] as? Bool == true
        return enabled && ObserveWire.uniform(clientId) < rate && (!development || config["dispatchInDebug"] as? Bool == true)
    }

    func dispatch() {
        guard let store, !sending else { return }
        if let after = gate.after, after > Date() {
            scheduleDispatch(after: after.timeIntervalSinceNow)
            return
        }
        guard shouldDispatch, let project = config["projectId"] as? String else {
            store.setCursor("metrics", store.maxId("metrics"))
            store.setCursor("logs", store.maxId("logs"))
            return
        }
        let base = (config["endpoint"] as? String ?? "https://o.expo.dev").trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        gate.after = nil
        sending = true
        send("metrics", url: "\(base)/\(project)/v1/metrics", limit: ObserveWire.chunk) { [self] in
            send("logs", url: "\(base)/\(project)/v1/logs", limit: ObserveWire.chunk) { [self] in
                sending = false
                if store.cursor("metrics") < store.maxId("metrics") || store.cursor("logs") < store.maxId("logs") {
                    scheduleDispatch(after: gate.after?.timeIntervalSinceNow ?? 5)
                }
            }
        }
    }

    /// Sends one signal's rows chunk by chunk until none are left or a send must wait.
    /// Follows expo-observe's `DispatchLoop.swift`.
    func send(_ signal: String, url: String, limit: Int, done: @escaping () -> Void) {
        if let after = gate.after, after > Date() { return done() }
        guard let store, let next = chunk(signal, limit: limit), let highest = next.rows.last?.first as? Int64,
              let data = try? JSONSerialization.data(withJSONObject: next.body), let endpoint = URL(string: url) else { return done() }
        let rows = next.rows
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.httpBody = data
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("1", forHTTPHeaderField: "Expo-AppMetrics-Skip")
        URLSession.shared.dataTask(with: request) { [self] _, response, error in
            queue.async { [self] in
                let status = (response as? HTTPURLResponse)?.statusCode
                let result: ObserveWire.Result = error != nil || status == nil
                    ? .retryable(nil)
                    : ObserveWire.classify(status: status!, retryAfter: (response as? HTTPURLResponse)?.value(forHTTPHeaderField: "Retry-After"))
                if Self.logEnabled { fputs("observe: \(signal) \(rows.count) rows → \(status.map(String.init) ?? "transport error")\n", stderr) }
                switch result {
                case .payloadTooLarge where rows.count > 1:
                    gate.failures = 0
                    send(signal, url: url, limit: rows.count / 2, done: done)
                case .success:
                    if let first = rows.first?.first as? Int64 { store.acknowledge(signal, from: first, through: highest) }
                    gate.failures = 0
                    store.setCursor(signal, highest)
                    send(signal, url: url, limit: ObserveWire.chunk, done: done)
                case .nonRetryable, .payloadTooLarge:
                    gate.failures = 0
                    store.setCursor(signal, highest)
                    send(signal, url: url, limit: ObserveWire.chunk, done: done)
                case .retryable(let after):
                    gate.failures += 1
                    gate.after = Date().addingTimeInterval(after ?? ObserveWire.backoff(attempt: gate.failures))
                    done()
                }
            }
        }.resume()
    }

    /// The next unsent rows of a signal, up to `limit`, and their OTLP body.
    func chunk(_ signal: String, limit: Int) -> (rows: [[Any?]], body: [String: Any])? {
        guard let store else { return nil }
        let cursor = store.cursor(signal)
        let rows = signal == "metrics"
            ? store.rows("SELECT id, session, time, category, name, value, route, updateId, params FROM metrics WHERE id > ? ORDER BY id LIMIT ?", [cursor, limit])
            : store.rows("SELECT id, session, time, severity, name, body, attributes, dropped FROM logs WHERE id > ? ORDER BY id LIMIT ?", [cursor, limit])
        guard !rows.isEmpty else { return nil }
        var sessions: [String: [String: Any]] = [:]
        for r in rows { if let s = r[1] as? String, sessions[s] == nil { sessions[s] = store.metadata(s) } }
        let id = clientId.uuidString.lowercased()
        return (rows, signal == "metrics"
            ? ObserveWire.metricsBody(rows, sessions: sessions, clientId: id)
            : ObserveWire.logsBody(rows, sessions: sessions, clientId: id))
    }

    /// Milliseconds to seconds, rounded to 0.1 ms, as a decimal so JSON prints it exactly.
    static func seconds(_ ms: Double) -> NSDecimalNumber {
        NSDecimalNumber(string: String(format: "%.4f", ms / 1000))
    }

    // MARK: Snapshot

    static func metadata(_ config: [String: Any]) -> [String: Any] {
        var u = utsname()
        uname(&u)
        let machine = withUnsafeBytes(of: &u.machine) { String(decoding: $0.prefix { $0 != 0 }, as: UTF8.self) }
        let info = Bundle.main.infoDictionary ?? [:]
        let v = ProcessInfo.processInfo.operatingSystemVersion
        var m: [String: Any] = [
            "osVersion": "\(v.majorVersion).\(v.minorVersion).\(v.patchVersion)",
            // On a simulator, `uname` reports the Mac's architecture, so use the simulated model.
            "deviceModel": ProcessInfo.processInfo.environment["SIMULATOR_MODEL_IDENTIFIER"] ?? machine,
            "language": Locale.preferredLanguages.first ?? "en",
            "clientVersion": "0.1.0",
            "environment": config["environment"] as? String ?? ((config["fact.development"] as? Bool ?? false) ? "development" : "production"),
        ]
        #if os(iOS)
        m["osName"] = "iOS"
        m["deviceName"] = UIDevice.current.model
        #else
        m["osName"] = "macOS"
        m["deviceName"] = "Mac"
        #endif
        let app = { (k: String) in config["app." + k] ?? info[k] }
        m["appIdentifier"] = app("CFBundleIdentifier")
        m["appVersion"] = app("CFBundleShortVersionString")
        m["appBuildNumber"] = app("CFBundleVersion")
        m["appName"] = app("CFBundleDisplayName") ?? app("CFBundleName")
        return m
    }

    static func records(_ data: Data) -> [[String: Any]] {
        var out: [[String: Any]] = []
        var i = 0
        let bytes = [UInt8](data)
        while i + 4 <= bytes.count {
            let n = Int(UInt32(bytes[i]) | UInt32(bytes[i + 1]) << 8 | UInt32(bytes[i + 2]) << 16 | UInt32(bytes[i + 3]) << 24)
            i += 4
            guard i + n <= bytes.count else { break }
            if let o = (try? JSONSerialization.jsonObject(with: Data(bytes[i..<i + n]))) as? [String: Any] { out.append(o) }
            i += n
        }
        return out
    }
}

// MARK: Queries from the app's own module

extension ObserveService {
    /// What this device recorded, newest first, for an app's own screens
    /// (`ExactServices.query("observe", …)`). `{"op": "recent", "limit": n}`
    /// (1–500, default 50) answers `{"session", "metrics": [{session, time,
    /// category, name, value, route, sent, params}]}`: `time` in seconds since
    /// 1970, `value` in seconds as sent, `sent` whether this journal holds a successful send acknowledgement.
    func query(_ request: [String: Any]) -> [String: Any] {
        guard request["op"] as? String == "recent" else { return ["error": "observe answers {\"op\": \"recent\"}"] }
        guard let store else { return ["session": session, "metrics": []] }
        let limit = max(1, min(500, (request["limit"] as? Int) ?? 50))
        let rows = store.rows("SELECT id, session, time, category, name, value, route, params, EXISTS(SELECT 1 FROM delivered WHERE signal = 'metrics' AND delivered.id = metrics.id) FROM metrics ORDER BY id DESC LIMIT ?", [limit])
        let metrics = rows.map { r -> [String: Any] in
            var m: [String: Any] = ["session": r[1] ?? "", "time": r[2] ?? 0.0, "category": r[3] ?? "", "name": r[4] ?? "",
                                    "value": r[5] ?? 0.0, "route": r[6] ?? "", "sent": (r[8] as? Int64) == 1]
            if let p = r[7] as? String, let o = try? JSONSerialization.jsonObject(with: Data(p.utf8)) { m["params"] = o }
            return m
        }
        return ["session": session, "metrics": metrics]
    }
}

// MARK: C entry points bound by ExactKit's service loader

@_cdecl("exact_service_start")
public func exactServiceStart(_ config: UnsafePointer<UInt8>?, _ configLength: UInt32, _ handoff: UnsafePointer<UInt8>?, _ handoffLength: UInt32) -> UnsafeMutableRawPointer? {
    let c = config.map { Data(bytes: $0, count: Int(configLength)) } ?? Data()
    let h = handoff.map { Data(bytes: $0, count: Int(handoffLength)) } ?? Data()
    let o = (try? JSONSerialization.jsonObject(with: c)) as? [String: Any] ?? [:]
    return Unmanaged.passRetained(ObserveService(config: o, handoff: h)).toOpaque()
}

@_cdecl("exact_service_event")
public func exactServiceEvent(_ state: UnsafeMutableRawPointer?, _ json: UnsafePointer<UInt8>?, _ length: UInt32) {
    guard let state, let json else { return }
    Unmanaged<ObserveService>.fromOpaque(state).takeUnretainedValue().event(Data(bytes: json, count: Int(length)))
}

@_cdecl("exact_service_background")
public func exactServiceBackground(_ state: UnsafeMutableRawPointer?) {
    guard let state else { return }
    let service = Unmanaged<ObserveService>.fromOpaque(state).takeUnretainedValue()
    service.queue.async { service.dispatch() }
}

@_cdecl("exact_service_query")
public func exactServiceQuery(_ state: UnsafeMutableRawPointer?, _ json: UnsafePointer<UInt8>?, _ length: UInt32, _ context: UnsafeMutableRawPointer?,
                              _ reply: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32) -> Void) {
    guard let state else { return reply(context, nil, 0) }
    let service = Unmanaged<ObserveService>.fromOpaque(state).takeUnretainedValue()
    let request = json.flatMap { (try? JSONSerialization.jsonObject(with: Data(bytes: $0, count: Int(length)))) as? [String: Any] } ?? [:]
    service.queue.async {
        let data = (try? JSONSerialization.data(withJSONObject: service.query(request))) ?? Data("{}".utf8)
        data.withUnsafeBytes { reply(context, $0.bindMemory(to: UInt8.self).baseAddress, UInt32(data.count)) }
    }
}
