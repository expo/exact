// Module launch parts and the journal of lifecycle events they subscribe to.
// A launch part is linked into the executable, so nothing is dlopened during
// launch. It runs once at the app delegate's init and receives journal events
// in their own main-thread turn, never inside a draw, commit or apply. Its
// heavier service, `lib<module>_service.dylib`, loads later when it asks.
import Foundation
import QuartzCore

/// `launch` runs on the main thread inside `UIApplicationMain`, before
/// `didFinishLaunching`. It must do no I/O and no UI, and has no runner.
/// All launch parts together have a 500 µs budget, which is logged when exceeded.
public protocol ExactLaunchPart {
    static var module: String { get }
    static func launch(_ context: ExactLaunchContext)
}

/// `at` is seconds on `CACurrentMediaTime`; `wall` is seconds since 1970.
public struct ExactJournalEvent {
    public let kind: String
    public let at: Double
    public let wall: Double
    public let fields: [String: Any]

    /// The JSON object a service receives.
    public var json: String {
        var o = fields
        o["kind"] = kind
        o["at"] = at
        o["wall"] = wall
        guard let d = try? JSONSerialization.data(withJSONObject: o, options: [.sortedKeys]) else { return "{}" }
        return String(decoding: d, as: UTF8.self)
    }
}

/// The process's append-only event journal. Past `capacity` it drops and
/// counts its oldest events.
public final class ExactJournal {
    public static let shared = ExactJournal()
    public static let capacity = 4096
    private(set) var events: [ExactJournalEvent] = []
    private(set) var dropped = 0
    private var subscribers: [(fn: (ExactJournalEvent) -> Void, next: Int)] = []
    private var delivering = false
    private var services: [ExactService] = []
    /// Pending query callbacks, grouped by service module
    private var waiting: [String: [(ExactService?) -> Void]] = [:]

    /// Seconds all subscribers may spend in one turn; the rest is delivered next turn.
    static let turnBudget = 0.001

    func record(_ kind: String, _ fields: [String: Any] = [:], at: Double = CACurrentMediaTime()) {
        let wall = Date().timeIntervalSince1970 - (CACurrentMediaTime() - at)
        events.append(ExactJournalEvent(kind: kind, at: at, wall: wall, fields: fields))
        if events.count > Self.capacity {
            // Dropping startup marks is safe: ExactLaunch keeps them.
            events.removeFirst()
            dropped += 1
            for i in subscribers.indices { subscribers[i].next = max(0, subscribers[i].next - 1) }
        }
        for s in services { s.event(events[events.count - 1]) }
        scheduleDelivery()
    }

    func subscribe(_ fn: @escaping (ExactJournalEvent) -> Void) {
        subscribers.append((fn, 0))
        scheduleDelivery()
    }

    func attach(_ service: ExactService) {
        services.append(service)
        for e in events { service.event(e) }
        loadEnded(service.module, service)
    }

    /// Queue a callback for service loading to finish on the main thread
    func whenLoaded(_ module: String, _ then: @escaping (ExactService?) -> Void) {
        waiting[module, default: []].append(then)
    }

    /// Resume pending queries with the loaded service, or nil when loading fails
    func loadEnded(_ module: String, _ service: ExactService?) {
        for then in waiting.removeValue(forKey: module) ?? [] { then(service) }
    }

    func background() { for s in services { s.background() } }

    /// The loaded service of `module`, if it is loaded (main thread).
    func service(_ module: String) -> ExactService? { services.first { $0.module == module } }

    private func scheduleDelivery() {
        guard !delivering, !subscribers.isEmpty else { return }
        delivering = true
        DispatchQueue.main.async { [weak self] in self?.deliver() }
    }

    private func deliver() {
        delivering = false
        let start = CACurrentMediaTime()
        var more = false
        outer: for i in subscribers.indices {
            while subscribers[i].next < events.count {
                if CACurrentMediaTime() - start > Self.turnBudget { more = true; break outer }
                let e = events[subscribers[i].next]
                subscribers[i].next += 1
                subscribers[i].fn(e)
            }
        }
        if more { scheduleDelivery() }
    }
}

/// When a module's service artifact loads.
public enum ExactServiceLoad: String {
    /// After the launch session is interactive, or startup ended without it.
    case afterStartup
    /// After the first presented frame.
    case afterFirstPixel
}

/// What a launch part sees.
public final class ExactLaunchContext {
    public let module: String
    /// `prewarmed`, `debugger`, `development` and `agent`. Cold or warm and
    /// whether metrics are suppressed arrive later, in the `launch` journal event.
    public let facts: [String: Any]
    /// A directory prepared before the launch part runs, so a signal handler can write without creating it.
    public let crashDirectory: String
    /// This module's settings from `app.json` `moduleConfig.<module>`.
    public let config: [String: Any]
    /// The app's Info.plist, or a bare build's sidecar: identity, version and build.
    public let app: [String: Any]

    init(module: String, facts: [String: Any]) {
        self.module = module
        self.facts = facts
        app = ExactEnv.appMetadata
        config = (app["ExactModuleConfig"] as? [String: Any])?[module] as? [String: Any] ?? [:]
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first ?? URL(fileURLWithPath: NSTemporaryDirectory())
        let identity = app["CFBundleIdentifier"] as? String ?? Bundle.main.bundleIdentifier ?? "app"
        crashDirectory = base.appendingPathComponent("exact/\(identity)/crashes", isDirectory: true).path
        try? FileManager.default.createDirectory(atPath: crashDirectory, withIntermediateDirectories: true)
    }

    /// Delivers every event so far, then each new one, in later main-thread turns.
    public func subscribe(_ fn: @escaping (ExactJournalEvent) -> Void) {
        ExactJournal.shared.subscribe(fn)
    }

    /// Loads `lib<module>_service.dylib` at `when` and starts it with `config` and
    /// `handoff`, the bytes the launch part kept (for example its ring).
    public func requestService(_ when: ExactServiceLoad, config: [String: Any], handoff: @escaping () -> Data) {
        ExactLaunch.shared.requestService(module: module, when: when, config: config, handoff: handoff)
    }
}

/// An in-memory ring of records a launch part keeps until its service loads.
/// `capacity` is in bytes. When full it drops and counts the oldest records.
public final class ExactRing {
    public let capacity: Int
    private var records: [Data] = []
    private var bytes = 0
    public private(set) var dropped = 0

    public init(capacity: Int) { self.capacity = capacity }

    public func append(_ record: Data) {
        records.append(record)
        bytes += record.count
        while bytes > capacity, !records.isEmpty {
            bytes -= records.removeFirst().count
            dropped += 1
        }
    }

    /// Length-prefixed (UInt32, little-endian) records, oldest first.
    public func encoded() -> Data {
        var out = Data()
        for r in records {
            var n = UInt32(r.count).littleEndian
            withUnsafeBytes(of: &n) { out.append(contentsOf: $0) }
            out.append(r)
        }
        return out
    }
}

/// The C entry points of a loaded `lib<module>_service.dylib`.
final class ExactService {
    typealias Start = @convention(c) (UnsafePointer<UInt8>?, UInt32, UnsafePointer<UInt8>?, UInt32) -> UnsafeMutableRawPointer?
    typealias Event = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32) -> Void
    typealias Background = @convention(c) (UnsafeMutableRawPointer?) -> Void
    /// Optional: `exact_service_query(state, json, length, context, reply)`;
    /// the service calls `reply(context, json, length)` once, on any thread.
    typealias Reply = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32) -> Void
    typealias Query = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32, UnsafeMutableRawPointer?, Reply) -> Void
    let module: String
    private let state: UnsafeMutableRawPointer?
    private let eventFn: Event
    private let backgroundFn: Background
    private let queryFn: Query?

    /// Loads from beside the executable on macOS, or from Frameworks on iOS
    /// (wrapped as a framework in an archive: `serviceFramework` in launch-parts.mjs).
    static func load(module: String, config: Data, handoff: Data) -> ExactService? {
        let name = "lib\(module)_service.dylib"
        #if os(macOS)
        let path = Bundle.main.executableURL!.deletingLastPathComponent().appendingPathComponent(name).path
        #else
        let path = embeddedModule(framework: serviceFramework(module), dylib: name)
        #endif
        guard let h = dlopen(path, RTLD_NOW | RTLD_LOCAL) else {
            fputs("exact: \(name): \(String(cString: dlerror()))\n", stderr)
            return nil
        }
        guard let s = dlsym(h, "exact_service_start"), let e = dlsym(h, "exact_service_event"), let b = dlsym(h, "exact_service_background") else {
            fputs("exact: \(name) lacks the service entry points\n", stderr)
            return nil
        }
        let start = unsafeBitCast(s, to: Start.self)
        let state = config.withUnsafeBytes { c in
            handoff.withUnsafeBytes { r in
                start(c.bindMemory(to: UInt8.self).baseAddress, UInt32(config.count), r.bindMemory(to: UInt8.self).baseAddress, UInt32(handoff.count))
            }
        }
        return ExactService(module: module, state: state, event: unsafeBitCast(e, to: Event.self), background: unsafeBitCast(b, to: Background.self),
                            query: dlsym(h, "exact_service_query").map { unsafeBitCast($0, to: Query.self) })
    }

    private init(module: String, state: UnsafeMutableRawPointer?, event: Event, background: Background, query: Query?) {
        self.module = module
        self.state = state
        eventFn = event
        backgroundFn = background
        queryFn = query
    }

    /// Asks the service; `reply` gets its JSON object, or nil when it answers
    /// no queries or its answer is not one.
    func query(_ request: [String: Any], reply: @escaping ([String: Any]?) -> Void) {
        guard let queryFn, let bytes = try? JSONSerialization.data(withJSONObject: request) else { return reply(nil) }
        let box = Unmanaged.passRetained(ReplyBox(reply)).toOpaque()
        bytes.withUnsafeBytes { b in
            queryFn(state, b.bindMemory(to: UInt8.self).baseAddress, UInt32(bytes.count), box) { context, json, length in
                guard let context else { return }
                let box = Unmanaged<ReplyBox>.fromOpaque(context).takeRetainedValue()
                let data = json.map { Data(bytes: $0, count: Int(length)) } ?? Data()
                box.reply((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])
            }
        }
    }

    private final class ReplyBox {
        let reply: ([String: Any]?) -> Void
        init(_ reply: @escaping ([String: Any]?) -> Void) { self.reply = reply }
    }

    func event(_ e: ExactJournalEvent) {
        let bytes = Array(e.json.utf8)
        bytes.withUnsafeBufferPointer { eventFn(state, $0.baseAddress, UInt32($0.count)) }
    }

    func background() { backgroundFn(state) }
}

/// A module's service, asked by the app's own module (LLP 1067.000 keeps
/// one module per app; a service such as Observe's is the framework's).
/// `reply` runs once, on any thread, with the service's JSON answer. A query
/// made before its service has loaded (Observe's loads after startup) loads
/// it then and waits; nil when the app asks for no such service, it fails
/// to load, or it answers no queries.
public enum ExactServices {
    public static func query(_ module: String, _ request: [String: Any], reply: @escaping ([String: Any]?) -> Void) {
        DispatchQueue.main.async {
            if let service = ExactJournal.shared.service(module) { return service.query(request, reply: reply) }
            guard ExactLaunch.shared.loadForQuery(module) else { return reply(nil) }
            ExactJournal.shared.whenLoaded(module) { service in
                guard let service else { return reply(nil) }
                service.query(request, reply: reply)
            }
        }
    }
}

extension ExactLaunch {
    /// Runs the launch parts in `app.json` order and journals each one's time.
    public func runLaunchParts(_ parts: [ExactLaunchPart.Type]) {
        let facts: [String: Any] = ["prewarmed": facts.prewarm != 0, "debugger": facts.traced != 0, "development": !ExactEnv.productionBake, "agent": ExactEnv.agentMode]
        var total = 0.0
        for part in parts {
            let t = CACurrentMediaTime()
            part.launch(ExactLaunchContext(module: part.module, facts: facts))
            let took = CACurrentMediaTime() - t
            total += took
            ExactJournal.shared.record("launchPart", ["module": part.module, "ms": took * 1000])
            ExactEnv.stamp("launch part \(part.module)")
        }
        if total > Self.launchBudget {
            ExactJournal.shared.record("launchBudget", ["ms": total * 1000])
            fputs("exact: launch parts took \(String(format: "%.2f", total * 1000)) ms, over the \(Self.launchBudget * 1000) ms budget\n", stderr)
        }
    }

    static let launchBudget = 0.0005

    /// An input's event timestamp, from which the next navigation is timed.
    public func input(at timestamp: TimeInterval) { NavigationMarks.shared.input(at: timestamp) }
}

/// App-reported events, errors and attributes, journaled for a module such as
/// Observe. The module's service decides what to send.
public enum ExactEvents {
    /// Handles the contract's `observe`, `observeAttributes`, `observeError` and
    /// `observeFeedback` host commands. The compiler sends a record's fields as
    /// name, value pairs.
    static func hostCommand(_ name: String, _ args: [Any]) {
        let pairs = { (from: Int) -> [String: Any] in
            var out: [String: Any] = [:]
            var i = from
            while i + 1 < args.count { if let k = args[i] as? String { out[k] = args[i + 1] }; i += 2 }
            return out
        }
        let text = { (i: Int, or: String) in args.count > i ? args[i] as? String ?? or : or }
        switch name {
        case "observe": log(text(0, ""), attributes: pairs(2), severity: text(1, "info"))
        case "observeAttributes": setGlobalAttributes(pairs(0))
        case "observeFeedback":
            let number = { (i: Int) in args.count > i ? (args[i] as? NSNumber)?.doubleValue : nil }
            reportFeedback(id: text(0, ""), message: text(1, ""), rating: number(2), min: number(3), max: number(4), attributes: pairs(5))
        default: reportError(type: text(1, "ContractError"), message: text(0, ""))
        }
    }

    /// A custom event (Observe's `logEvent`): `severity` is trace, debug,
    /// info (the default), warn, error or fatal.
    public static func log(_ name: String, attributes: [String: Any] = [:], severity: String = "info", body: String? = nil, displayName: String? = nil) {
        var f: [String: Any] = ["name": name, "attributes": attributes, "severity": severity]
        if let body { f["body"] = body }
        if let displayName { f["displayName"] = displayName }
        ExactJournal.shared.record("app.event", f)
    }

    /// User feedback (Observe's `reportFeedback`). A blank message is none, and a rating
    /// without a scale is on 0 to 10.
    public static func reportFeedback(id: String, message: String, rating: Double? = nil, min: Double? = nil, max: Double? = nil, attributes: [String: Any] = [:]) {
        var f: [String: Any] = ["id": id, "message": message, "attributes": attributes]
        for (key, value) in [("rating", rating), ("min", min), ("max", max)] { if let value { f[key] = value } }
        ExactJournal.shared.record("app.feedback", f)
    }

    /// Attributes merged into every later metric and event; per-record keys win.
    public static func setGlobalAttributes(_ attributes: [String: Any]) {
        ExactJournal.shared.record("app.attributes", ["attributes": attributes])
    }

    /// A framework event for modules, such as an update's download.
    public static func journal(_ kind: String, _ fields: [String: Any]) {
        ExactJournal.shared.record(kind, fields)
    }

    /// A caught error (Observe's `reportError`).
    public static func reportError(type: String, message: String, stack: String? = nil) {
        var f: [String: Any] = ["type": type, "message": message]
        if let stack { f["stack"] = stack }
        ExactJournal.shared.record("app.error", f)
    }
}

/// A launch service's framework name in an archive (`serviceFramework` in launch-parts.mjs): `observe` is `ExactObserveService`.
func serviceFramework(_ module: String) -> String {
    let words = module.split(whereSeparator: { !$0.isLetter && !$0.isNumber })
    return "Exact" + words.map { $0.prefix(1).uppercased() + $0.dropFirst() }.joined() + "Service"
}
