// The tool door (LLP 1133.023 D7, D15 item 4): what a system's call of a
// tool reaches. The generated intents module (host/apple/intents.mjs) posts
// `exact.tools.call` `{tool, args, door, digest, reply}`, an
// `NSMutableDictionary` this marks `taken`, and an app's own native intent
// calls `ExactTools.call` directly. Either way the call is handed to the
// session's runner, which runs it as an invocation of origin `tool`
// (runner/src/runner/modules/calls.rs), and the door polls it by id until it
// ends or a reload replaces that runner. The module posts
// `exact.tools.cancel` with the same dictionary when the system cancels the
// intent's task: the call's `Cancellation`, which the dictionary carries,
// refuses it at the door before the runner has it and tells the runner at
// once after, which drops a call still waiting and lets one that runs
// finish.
//
// A call that launched the app with no scene boots the session with none,
// as a wake does (LLP 1133.017 D7): `boot`, which the app's host sets.
import Foundation
import os
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

public enum ExactTools {
    /// Boots a session with no view for a call that launched the app with
    /// none; the app's host sets it. Nil: the call waits for a session.
    public static var boot: (() -> Void)?
    /// How long a call waits for its session to boot, and then for its end.
    private static let bootBound: Double = 20, endBound: Double = 15
    private static var installed = false
    static let logger = Logger(subsystem: "dev.exact", category: "tools")

    /// Hear the generated module's calls. Install at launch, before the app
    /// finishes launching: the system may call as soon as it has.
    public static func install() {
        guard !installed else { return }
        installed = true
        NotificationCenter.default.addObserver(forName: Notification.Name("exact.tools.call"), object: nil, queue: nil) { note in
            guard let body = note.object as? NSMutableDictionary, let tool = body["tool"] as? String,
                  let reply = body["reply"] as? ([String: Any]) -> Void else { return }
            body["taken"] = true
            let args = body["args"] as? [String: Any] ?? [:]
            let door = body["door"] as? String ?? "apple", digest = body["digest"] as? String
            // Posted on the main actor, so the dictionary is read and written on the main thread only.
            let cancellation = Cancellation()
            if body["cancelled"] as? Bool == true { cancellation.cancel() }
            body["cancellation"] = cancellation
            DispatchQueue.main.async {
                call(tool: tool, args: args, door: door, digest: digest, cancellation: cancellation, reply: reply)
            }
        }
        // Posted from the task's cancellation handler, on any thread. A task cancelled before it posted its call
        // posts this first, and the mark is read when the call is taken.
        NotificationCenter.default.addObserver(forName: Notification.Name("exact.tools.cancel"), object: nil, queue: nil) { note in
            guard let body = note.object as? NSMutableDictionary else { return }
            DispatchQueue.main.async {
                if let cancellation = body["cancellation"] as? Cancellation { cancellation.cancel() } else { body["cancelled"] = true }
            }
        }
        ExactContentIndex.install()
    }

    /// A call's cancellation by its caller, on the main thread (LLP 1133.023
    /// D7). Before the runner has the call, the door refuses it `cancelled`;
    /// after, `cancel()` tells the runner at once, so a call still waiting is
    /// dropped before the one ahead of it ends and the runner dispatches it.
    public final class Cancellation {
        public private(set) var isCancelled = false
        /// Tells the runner, once it has the call.
        var tell: (() -> Void)?
        public init() {}
        public func cancel() {
            guard !isCancelled else { return }
            isCancelled = true
            tell?()
        }
    }

    /// Call `tool` with `args` as `door` hands it, and answer LLP 1111 D4's
    /// envelope: `outcome` (`ok`, `failed`, `superseded`, `timeout`,
    /// `refused`), `certainty`, `reason`, `value` and `message`. `digest` is
    /// the face's the caller was compiled with (D8), checked against the
    /// plan's. `cancellation` is the caller's. Main thread.
    public static func call(tool: String, args: [String: Any], door: String = "apple", digest: String? = nil,
                            cancellation: Cancellation = Cancellation(), reply: @escaping ([String: Any]) -> Void) {
        let started = Date()
        let refuse = { (reason: String, message: String) in
            reply(["outcome": "refused", "certainty": "none", "reason": reason, "message": message])
        }
        let dropped = { refuse("cancelled", "Its caller cancelled it before it ran.") }
        if cancellation.isCancelled { return dropped() }
        guard let session = ExactApp.shared.sessions.first(where: { $0.state != .destroyed }) else {
            return refuse("unavailable", "The app has no session to call.")
        }
        let cold = !session.booted || session.firstDrawMs == nil
        #if os(iOS)
        if cold, UIApplication.shared.applicationState == .background {
            log(session, "tools: \(tool) found no session ready; booting one with no view")
            boot?()
        }
        #endif
        func hand() {
            if cancellation.isCancelled { return dropped() }
            guard session.state != .destroyed else { return refuse("unavailable", "The app's session ended.") }
            guard session.booted else {
                if Date().timeIntervalSince(started) > bootBound { return refuse("busy", "The app did not start in time.") }
                return DispatchQueue.main.asyncAfter(deadline: .now() + 0.05, execute: hand)
            }
            var request: [String: Any] = ["op": "call", "tool": tool, "args": args, "door": door, "cold": cold, "hidden": hidden(session)]
            if let digest { request["digest"] = digest }
            let answer = json(session.agent(text(request)))
            guard let id = answer["call"] as? Int else {
                return refuse("unavailable", answer["error"] as? String ?? "The app refused the call.")
            }
            follow(id, agent: runner(session), cancellation: cancellation, reply: reply)
        }
        hand()
    }

    /// `session`'s agent while the runner that runs now does; nil after a
    /// reload or the session's end. A call's id is its runner's, and a
    /// reload's runner numbers its calls from the start again, so asking
    /// the next one would read another call's outcome.
    static func runner(_ session: ExactSession) -> (String) -> String? {
        let issued = GenerationStamp(session)
        return { issued.session?.agent($0) }
    }

    /// Poll call `id`, which the runner holds, through `agent` until it ends,
    /// and answer its envelope. From now on `cancellation` tells the runner
    /// the moment the caller cancels, not at the next poll. `agent` answers
    /// nil once that runner is gone (a reload, a plan replacement, the
    /// session's end): the call ends `interrupted` (LLP 1111 D4). Main thread.
    static func follow(_ id: Int, agent: @escaping (String) -> String?, cancellation: Cancellation,
                       reply: @escaping ([String: Any]) -> Void) {
        let handed = Date()
        cancellation.tell = { _ = agent(text(["op": "cancel", "id": id])) }
        func answer(_ envelope: [String: Any]) {
            cancellation.tell = nil
            reply(envelope)
        }
        func poll() {
            guard let polled = agent(text(["op": "called", "id": id])).map(json) else {
                return answer(["outcome": "interrupted", "certainty": "uncertain", "reason": "",
                               "message": "The app was reloaded while it ran: it may or may not have taken effect. Read before calling again."])
            }
            if polled["outcome"] != nil { return answer(polled) }
            if polled["error"] != nil || Date().timeIntervalSince(handed) > endBound {
                return answer(["outcome": "timeout", "certainty": "uncertain", "reason": "",
                               "message": "It may or may not have taken effect: read before calling again."])
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05, execute: poll)
        }
        poll()
    }

    /// No view of the session is visible: its timers stop while the call runs (D7).
    private static func hidden(_ session: ExactSession) -> Bool {
        #if os(iOS)
        return UIApplication.shared.applicationState == .background
        #elseif os(macOS)
        return !NSApp.windows.contains { $0.isVisible && $0.occlusionState.contains(.visible) }
        #else
        return false
        #endif
    }

    static func log(_ session: ExactSession?, _ line: String) {
        session?.log(line)
        logger.log("\(line, privacy: .public)")
    }

    private static func text(_ object: [String: Any]) -> String {
        (try? JSONSerialization.data(withJSONObject: object)).map { String(decoding: $0, as: UTF8.self) } ?? "{}"
    }
    private static func json(_ text: String) -> [String: Any] {
        (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any] ?? ["error": "unreadable reply"]
    }
}
