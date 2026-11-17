// Host events (LLP 1133.001.000 D2): a call the system makes on the app's or
// the scene's delegate that a package takes, held here until it does. The
// build lists in Info.plist's `ExactHostEvents` each event taken (a bound
// package declares it and its grant is signed: `host/apple/plist.mjs`
// `HOST_EVENTS`, the same names as `events` below). At launch, before any
// scene connects or the Mac's delegate is set, `installHostEvents` adds each
// taken event's delegate methods by selector, where the class does not answer
// them already, as `PushRegistration` does; so an app that takes no event
// answers none of them, and no Swift declaration is related to UIKit's or
// AppKit's protocol. The payload is kept as an `NSObject`: ExactKit links
// neither CloudKit nor GroupActivities, and the package, which does, casts it.
//
// Each payload is addressed to a session, by label: the one the embedder
// names, else the one the host's resolver gives (the Mac's front window),
// else the first live session. A payload for a session that has ended goes
// to the first live one. At most `bound` are held per event, the oldest
// dropped and journaled beyond that; one identical or `isEqual` to one still
// held for the same session is not held twice.
//
// A package instance drains what is addressed to its label through
// `exact.hostEvents` (an `NSMutableDictionary` `{event, session}` whose
// `reply` this sets, `{items: [NSObject]}`, removing them) and hears later
// ones as `exact.hostEvents.arrived` (`userInfo` `{event, session}`). A
// package's test delivers one by posting `exact.hostEvents.deliver` with
// `{event, payload, session?}`. The mailbox is ExactKit's, so it outlives a
// module reload. Every entry may be called on any thread; the mailbox is
// read and written on the main thread only.
import Foundation
import ObjectiveC

enum HostEvents {
    /// One listed event: the selectors that deliver it, on the application
    /// delegate and on the scene delegate, and the key of the scene's
    /// connection options that carries it when it launched the app. Each
    /// method takes the sender and the payload (`v@:@@`).
    struct Event {
        let name: String
        let application: [String]
        let scene: [String]
        let connectionOption: String?
    }

    /// The list, as `HOST_EVENTS` has it; a platform without a delivery
    /// point has no selectors.
    static let events: [Event] = [
        Event(name: "cloudkit-share",
              application: macOS ? ["application:userDidAcceptCloudKitShareWithMetadata:"] : [],
              scene: iOS ? ["windowScene:userDidAcceptCloudKitShareWithMetadata:"] : [],
              connectionOption: iOS ? "cloudKitShareMetadata" : nil),
    ]
    #if os(macOS)
    private static let macOS = true, iOS = false
    #elseif os(iOS)
    private static let macOS = false, iOS = true
    #else
    private static let macOS = false, iOS = false
    #endif

    static let bound = 16
    private struct Held { let session: String?; let payload: NSObject }
    private static var held: [String: [Held]] = [:]
    private static var listening = false
    /// The session an app-wide delivery goes to when none is named.
    static var resolve: () -> ExactSession? = { nil }

    private static func onMain(_ body: @escaping () -> Void) {
        if Thread.isMainThread { body() } else { DispatchQueue.main.async(execute: body) }
    }

    /// The events this build takes that ExactKit has a delivery point for.
    static func taken(_ info: [String: Any]? = Bundle.main.infoDictionary) -> [Event] {
        let names = info?["ExactHostEvents"] as? [String] ?? []
        return events.filter { names.contains($0.name) && !($0.application.isEmpty && $0.scene.isEmpty) }
    }

    /// Before any package can ask: at a session's making, as push's.
    static func listen() {
        guard !listening else { return }
        listening = true
        NotificationCenter.default.addObserver(forName: Notification.Name("exact.hostEvents"), object: nil, queue: nil) { note in
            guard let box = note.object as? NSMutableDictionary, let event = box["event"] as? String, let label = box["session"] as? String else { return }
            let run = { box["reply"] = ["items": drain(event, label)] }
            if Thread.isMainThread { run() } else { DispatchQueue.main.sync(execute: run) }
        }
        NotificationCenter.default.addObserver(forName: Notification.Name("exact.hostEvents.deliver"), object: nil, queue: nil) { note in
            guard let body = note.object as? NSDictionary, let event = body["event"] as? String, let payload = body["payload"] as? NSObject else { return }
            received(event, payload, label: body["session"] as? String)
        }
    }

    private static var live: [ExactSession] { ExactApp.shared.sessions.filter { $0.state != .destroyed } }

    /// Where a payload addressed to `label` goes now: that session while it
    /// lives, else the first live one.
    private static func target(_ label: String?) -> String? {
        let sessions = live
        if let label, sessions.contains(where: { $0.label == label }) { return label }
        return sessions.first?.label
    }

    /// A delivery: `session` named, else the resolver's (unless `resolving`
    /// is false), else the first live.
    static func received(_ event: String, _ payload: NSObject, session: ExactSession?, resolving: Bool = true) {
        onMain { received(event, payload, label: (session ?? (resolving ? resolve() : nil))?.label) }
    }

    /// A delivery addressed by label. A name not on the list is ignored.
    static func received(_ event: String, _ payload: NSObject, label: String?) {
        onMain {
            listen()
            guard events.contains(where: { $0.name == event }) else { return }
            let to = target(label)
            var list = held[event, default: []]
            guard !list.contains(where: { target($0.session) == to && ($0.payload === payload || $0.payload.isEqual(payload)) }) else { return }
            list.append(Held(session: to, payload: payload))
            if list.count > bound {
                list.removeFirst()
                let line = "host event \(event): more than \(bound) held; the oldest is dropped"
                live.forEach { $0.log(line) }
                if live.isEmpty { FileHandle.standardError.write(Data("\(line)\n".utf8)) }
            }
            held[event] = list
            arrived(event, to)
        }
    }

    private static func arrived(_ event: String, _ label: String?) {
        var info: [String: Any] = ["event": event]
        if let label { info["session"] = label }
        NotificationCenter.default.post(name: Notification.Name("exact.hostEvents.arrived"), object: nil, userInfo: info)
    }

    /// What is held of `event` for session `label`, in arrival order, removed.
    static func drain(_ event: String, _ label: String) -> [NSObject] {
        let list = held[event] ?? []
        let mine = list.filter { target($0.session) == label }
        held[event] = list.filter { target($0.session) != label }
        return mine.map(\.payload)
    }

    /// Session `label` ended: what was held for it now goes to the first
    /// live session, which hears that it arrived.
    static func ended(_ label: String) {
        onMain {
            guard let to = target(nil) else { return }
            for (event, list) in held where list.contains(where: { $0.session == label }) {
                held[event] = list.map { $0.session == label ? Held(session: to, payload: $0.payload) : $0 }
                arrived(event, to)
            }
        }
    }

    /// Adds to `type` each of `selectors` it does not answer already, its own
    /// or inherited, delivering `event`. Whether any was added.
    @discardableResult
    static func install(_ event: String, _ selectors: [String], on type: AnyClass) -> Bool {
        let deliver: @convention(block) (AnyObject, AnyObject?, AnyObject?) -> Void = { _, _, payload in
            if let payload = payload as? NSObject { HostEvents.received(event, payload, session: nil) }
        }
        var added = false
        for name in selectors {
            let selector = NSSelectorFromString(name)
            guard class_getInstanceMethod(type, selector) == nil else { continue }
            added = class_addMethod(type, selector, imp_implementationWithBlock(deliver), "v@:@@") || added
        }
        return added
    }

    /// Each of `taken`'s delivery methods added to the delegate classes given.
    static func install(_ taken: [Event], applicationDelegate: AnyClass?, sceneDelegate: AnyClass?) {
        for event in taken {
            if let type = applicationDelegate { install(event.name, event.application, on: type) }
            if let type = sceneDelegate { install(event.name, event.scene, on: type) }
        }
    }

    /// The payload of each of `taken` that a scene's connection options carry.
    static func connected(_ options: NSObject, _ taken: [Event], session: ExactSession?) {
        for event in taken {
            guard let key = event.connectionOption, let payload = options.value(forKey: key) as? NSObject else { continue }
            received(event.name, payload, session: session)
        }
    }

    /// A test's fresh process.
    static func reset() { held = [:]; resolve = { nil } }
}

extension ExactApp {
    /// At launch, before any scene connects (iOS: in `didFinishLaunching`,
    /// before `configurationForConnecting` names the scene delegate) and
    /// before the Mac's application delegate is set: the delegate methods of
    /// each event this build takes, added to the classes given. `session`
    /// names the session an app-wide call goes to (the Mac's front window);
    /// nil, the first live one. Main thread.
    public func installHostEvents(applicationDelegate: AnyClass?, sceneDelegate: AnyClass?, session: @escaping () -> ExactSession? = { nil }) {
        HostEvents.listen()
        HostEvents.resolve = session
        HostEvents.install(HostEvents.taken(), applicationDelegate: applicationDelegate, sceneDelegate: sceneDelegate)
    }

    /// A scene's connection options (`UIScene.ConnectionOptions`): the
    /// payload of each taken event that launched the app, read by key so
    /// that no framework type is named, for `session` (nil: as an app-wide
    /// delivery). Call from `scene(_:willConnectTo:options:)`.
    public func hostEvents(connectionOptions options: NSObject, session: ExactSession? = nil) {
        HostEvents.connected(options, HostEvents.taken(), session: session)
    }

    /// A host event an embedder's own delegate received, forwarded to the
    /// package that takes it: `name` from the list (any other is ignored),
    /// the system's object, and the session it is for (nil: the first live
    /// one). Any thread.
    public func hostEvent(_ name: String, _ payload: NSObject, session: ExactSession? = nil) {
        HostEvents.received(name, payload, session: session, resolving: false)
    }
}
