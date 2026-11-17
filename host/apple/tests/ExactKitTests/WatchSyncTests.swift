import XCTest
#if os(macOS)
import AppKit
#endif
import CryptoKit
import ExactCore
@testable import ExactWatchSync
@testable import ExactKit

final class WatchSyncTests: XCTestCase {
    /// A store in its own app group directory, as a watch or phone process has it.
    private func withStore(_ body: (WatchStore, Data, URL) throws -> Void) throws {
        let plan = try fixturePlan("WATCH_SYNC")
        let receipt = try XCTUnwrap(try JSONSerialization.jsonObject(with: Runtime.bakedCompat()) as? [String: Any])
        let app = try XCTUnwrap((receipt["inputs"] as? [String: Any])?["app"] as? String)
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_WATCH_SYNC_PLAN"])
        let root = URL(fileURLWithPath: path).deletingLastPathComponent().appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let variables = ["EXACT_APP_GROUP_ROOT": root.path, "EXACT_APP_GROUP_APP": app, "EXACT_AGENT": "1", "EXACT_AGENT_STORAGE": "sealed", "EXACT_STORE": "memory"]
        let saved = variables.keys.map { ($0, getenv($0).map { String(cString: $0) }) }
        for (key, value) in variables { setenv(key, value, 1) }
        defer {
            for (key, value) in saved { if let value { setenv(key, value, 1) } else { unsetenv(key) } }
            try? FileManager.default.removeItem(at: root)
        }
        try body(WatchStore(app: app, policy: ["state": ["favourite"], "secrets": ["seed"]], plan: plan, grants: "secret.keep seed"), plan, root)
    }

    /// WCSession and the app, as `WatchReceiver` sees them.
    private final class Wire {
        let key = Curve25519.KeyAgreement.PrivateKey()
        var retained: Data?
        /// Nil while WCSession has yet to answer.
        var activation: WatchReceive.Event? = .activated
        var opened: [Bool] = []
        var refreshes = 0
        /// Each publication's outcome, in order; past its end, WCSession takes the key.
        var publications: [Bool] = []
        /// The phone received the key.
        var published: () -> Void = {}
        var timers: [() -> Void] = []
        var boot: () -> Void = {}
        var refresh: () -> Void = {}
        func receiver(_ store: WatchStore) -> WatchReceiver {
            WatchReceiver(ports: .init(
                activate: { self.activation }, retained: { self.retained }, store: { store },
                key: { WatchKey(installation: "watch", key: self.key) },
                open: { result in
                    if case .success = result { self.opened.append(true); self.boot() } else { self.opened.append(false) }
                },
                refresh: { self.refreshes += 1; self.refresh() }, reloadComplications: {},
                publish: { _ in
                    let taken = self.publications.isEmpty || self.publications.removeFirst()
                    if taken { self.published() }
                    return taken
                },
                schedule: { _, work in self.timers.append(work) }))
        }
        func hold(_ sender: String, _ revision: Int, seed: Any = "seed", favourite: Bool) throws {
            let snapshot: [String: Any] = ["sender": sender, "revision": revision, "values": ["seed": seed, "exact.state.favourite": "\(favourite)"]]
            retained = try WatchEnvelope.wireData(WatchEnvelope.seal(snapshot: snapshot, recipient: "watch", publicKey: key.publicKey.rawRepresentation))
        }
    }

    private func favourite(_ session: HeadlessSession) throws -> Bool? {
        let state = try JSONSerialization.jsonObject(with: Data(session.agent("{\"op\":\"state\"}").utf8)) as? [String: Any]
        return (state?["slots"] as? [String: Any])?["favourite"] as? Bool
    }

    func testStoreCallbackReturnsOnlyTheSealedEnvelope() throws {
        try withStore { store, plan, root in
            _ = try store.request("apply", fields: ["snapshot": ["sender": "fixture", "revision": 1,
                "values": ["seed": "synthetic-private-seed", "exact.state.favourite": "true"]]])
            let key = Curve25519.KeyAgreement.PrivateKey()
            let response = try store.request("snapshot", fields: ["sender": "fixture", "recipient": "watch", "publicKey": key.publicKey.rawRepresentation.base64EncodedString()])
            XCTAssertNil(response["values"])
            let bytes = try JSONSerialization.data(withJSONObject: response)
            XCTAssertFalse(String(decoding: bytes, as: UTF8.self).contains("synthetic-private-seed"))
            let envelope = try JSONDecoder().decode(WatchEnvelope.self, from: bytes)
            let values = try XCTUnwrap(try envelope.open(recipient: "watch", key: key)["values"] as? [String: Any])
            XCTAssertEqual(values["seed"] as? String, "synthetic-private-seed")
            XCTAssertEqual(values["exact.state.favourite"] as? String, "true")
            XCTAssertThrowsError(try store.request("snapshot", fields: ["sender": "fixture"]))

            let cache = try WidgetTimelineCache(root: root, kind: "account", generation: "one")
            _ = try cache.resolve { Data("account A".utf8) }
            _ = try store.request("apply", fields: ["snapshot": ["sender": "fixture", "revision": 2,
                "values": ["seed": NSNull(), "exact.state.favourite": "false"]]])
            enum Offline: Error { case unavailable }
            let loggedOut = try WidgetTimelineCache(root: root, kind: "account", generation: "one")
            XCTAssertThrowsError(try loggedOut.resolve { throw Offline.unavailable }, "logout must invalidate account A's cached complication")
            XCTAssertFalse(cache.isCurrent)

            _ = try store.request("apply", fields: ["snapshot": ["sender": "fixture", "revision": 3,
                "values": ["seed": "account B", "exact.state.favourite": "true"]]])
            let next = try WidgetTimelineCache(root: root, kind: "account", generation: "one")
            _ = try next.resolve { Data("account B".utf8) }
            let removed = WatchStore(app: store.app, policy: ["state": [], "secrets": []], plan: plan, grants: "")
            _ = try removed.request("cleanup")
            let retired = try WidgetTimelineCache(root: root, kind: "account", generation: "one")
            XCTAssertThrowsError(try retired.resolve { throw Offline.unavailable }, "retirement must invalidate account B's cached complication")
            XCTAssertFalse(next.isCurrent)
        }
    }

    /// The scene is active before WCSession's activation completes, and the retained
    /// context is unreadable until then: the session boots on the imported values.
    func testReadersOpenOnlyAfterActivationImportsTheRetainedContext() throws {
        // The scene's first phase and the boot's request arrive in either order.
        for early in [[WatchReceive.Event.launch, .scene(active: true)], [.scene(active: true), .launch]] {
            try withStore { store, plan, _ in
                _ = try store.request("apply", fields: ["snapshot": ["sender": "phone", "revision": 1, "values": ["seed": "seed", "exact.state.favourite": "false"]]])
                let wire = Wire(), receiver = wire.receiver(store)
                wire.activation = nil
                var booted = false
                wire.boot = {
                    do {
                        let session = try HeadlessSession(plan: plan, size: CGSize(width: 200, height: 200))
                        XCTAssertEqual(try self.favourite(session), true, "the session booted on the store the retained context replaces")
                        booted = true
                    } catch { XCTFail("\(error)") }
                }
                for event in early + [.scene(active: false), .scene(active: true), .retry] { receiver.feed(event) }
                XCTAssertEqual(wire.opened, [], "readers opened before activation completed")
                try wire.hold("phone", 2, favourite: true)
                receiver.feed(.activated)
                XCTAssertEqual(wire.opened, [true])
                XCTAssertTrue(booted)
            }
        }
    }

    /// An import the store fails leaves an unfinished journal. The same process tries
    /// again, and the accepted phone's snapshot finishes a replacement phone's import.
    func testAFailedImportIsRetriedWithoutARestart() throws {
        try withStore { store, plan, root in
            _ = try store.request("apply", fields: ["snapshot": ["sender": "fixture", "revision": 4, "values": ["seed": "account A", "exact.state.favourite": "true"]]])
            let secret = root.appendingPathComponent("Library/Caches/exact/\(store.app)/agent/sealed/secrets/seed")
            let saved = secret.appendingPathExtension("saved")
            try FileManager.default.moveItem(at: secret, to: saved)
            try FileManager.default.createDirectory(at: secret, withIntermediateDirectories: false)

            let wire = Wire(), receiver = wire.receiver(store)
            var booted = false
            wire.boot = {
                do {
                    let session = try HeadlessSession(plan: plan, size: CGSize(width: 200, height: 200))
                    XCTAssertEqual(try self.favourite(session), true)
                    booted = true
                } catch { XCTFail("\(error)") }
            }
            try wire.hold("replacement", 5, seed: "account B", favourite: false)
            receiver.feed(.scene(active: true))
            receiver.feed(.launch)
            XCTAssertEqual(wire.opened, [false], "a store with an unfinished import opened")
            XCTAssertFalse(store.ready())
            XCTAssertEqual(wire.timers.count, 1, "the failed import has no retry")
            // An older snapshot of the replacement cannot finish its import.
            try wire.hold("replacement", 4, seed: "older B", favourite: false)
            receiver.feed(.context)
            XCTAssertEqual(wire.opened, [false, false])

            try FileManager.default.removeItem(at: secret)
            try FileManager.default.moveItem(at: saved, to: secret)
            try wire.hold("fixture", 4, seed: "account A", favourite: true)
            wire.timers.first?()
            XCTAssertEqual(wire.opened, [false, false, true])
            XCTAssertTrue(booted)
            XCTAssertTrue(store.ready())
        }
    }

    /// An import that lands after the session booted reaches it: at once when it has
    /// subscribed, at subscription otherwise. A context already held changes nothing.
    func testAnImportAfterBootRefreshesTheSession() throws {
        try withStore { store, plan, _ in
            let wire = Wire(), receiver = wire.receiver(store)
            receiver.feed(.launch)
            XCTAssertEqual(wire.opened, [true])
            let session = try HeadlessSession(plan: plan, size: CGSize(width: 200, height: 200))
            try session.refresh()
            XCTAssertEqual(try favourite(session), false)
            var seen: [Bool?] = []
            wire.refresh = {
                do { _ = try session.storeChanged(); seen.append(try self.favourite(session)) } catch { XCTFail("\(error)") }
            }
            try wire.hold("phone", 1, favourite: true)
            receiver.feed(.context)
            XCTAssertEqual(seen, [], "the session has not subscribed")
            receiver.feed(.subscribed)
            XCTAssertEqual(seen, [true])
            receiver.feed(.context)
            receiver.feed(.scene(active: true))
            XCTAssertEqual(seen, [true], "a context already imported refreshed the session")
            try wire.hold("phone", 2, favourite: false)
            receiver.feed(.context)
            XCTAssertEqual(seen, [true, false])
        }
    }

    func testARejectedContextOrAFailedActivationOpensTheStore() throws {
        try withStore { store, plan, _ in
            for activation in [WatchReceive.Event.activated, .activationFailed] {
                let wire = Wire(), receiver = wire.receiver(store)
                wire.activation = activation
                wire.retained = Data("not an envelope".utf8)
                receiver.feed(.launch)
                XCTAssertEqual(wire.opened, [true])
                try HeadlessSession(plan: plan, size: CGSize(width: 200, height: 200)).refresh()
            }
        }
    }

    /// WatchConnectivity refuses the watch's first publication of its key while both
    /// apps stay active. Readers open meanwhile, the watch publishes again by itself,
    /// and the phone then sends the change it had no key for.
    func testAFailedKeyPublicationIsRetriedWhileBothAppsStayActive() throws {
        try withStore { store, _, _ in
            let wire = Wire(), receiver = wire.receiver(store)
            var keyed = false, sent: [Bool] = [], queued: [() -> Void] = []
            let sender = WatchSender(ports: .init(
                observeForeground: {}, activate: { .activated }, enqueue: { queued.append($0) },
                submit: { sent.append(keyed); return keyed ? .accepted : .refused(.noWatch) },
                report: { _ in }, schedule: { _, _ in }))
            func phone(_ event: WatchSend.Event) {
                sender.feed(event)
                while !queued.isEmpty { queued.removeFirst()() }
            }
            wire.publications = [false]
            wire.published = { keyed = true; phone(.peer) }

            phone(.changed)
            receiver.feed(.scene(active: true))
            receiver.feed(.launch)
            XCTAssertEqual(wire.opened, [true], "a failed publication kept readers closed")
            XCTAssertEqual(sent, [false])
            XCTAssertEqual(wire.timers.count, 1, "the failed publication has no retry")
            wire.timers.popLast()?()
            XCTAssertEqual(sent, [false, true], "the phone never received the key")
            XCTAssertTrue(wire.timers.isEmpty, "the key was published and a retry is still scheduled")
        }
    }

    /// A revision is spent once the dictionary WatchConnectivity is handed is known to
    /// fit, and given back when WatchConnectivity refuses it.
    func testARevisionIsSpentOnlyOnASnapshotWatchConnectivityTook() throws {
        try withStore { store, _, _ in
            let key = Curve25519.KeyAgreement.PrivateKey().publicKey.rawRepresentation
            func stored(_ revision: Int, seed: String) throws {
                _ = try store.request("apply", fields: ["snapshot": ["sender": "other", "revision": revision, "values": ["seed": seed, "exact.state.favourite": "true"]]])
            }
            var handed: [Int] = []
            func submit(fail: Bool = false) -> WatchSend.Submitted {
                store.submit(sender: "phone", recipient: "watch", publicKey: key) { context in
                    struct Refused: Error {}
                    if fail { throw Refused() }
                    let data = try XCTUnwrap(context["snapshot"] as? Data)
                    XCTAssertLessThanOrEqual(try PropertyListSerialization.data(fromPropertyList: context, format: .binary, options: 0).count, WatchEnvelope.limit)
                    handed.append(try XCTUnwrap((JSONSerialization.jsonObject(with: data) as? [String: Any])?["revision"] as? Int))
                }
            }
            func result(_ submitted: WatchSend.Submitted) -> String { "\(submitted)" }
            try stored(1, seed: "small")
            XCTAssertEqual(result(submit(fail: true)), "failed")
            XCTAssertEqual(result(submit()), "accepted")
            XCTAssertEqual(handed, [1], "the revision of a send WatchConnectivity refused was not given back")
            try stored(2, seed: String(repeating: "s", count: 50_000))
            XCTAssertEqual(result(submit()), "refused(ExactWatchSync.WatchSend.Refusal.oversized)")
            XCTAssertEqual(handed, [1], "an oversized snapshot was handed over")
            try stored(3, seed: "small again")
            XCTAssertEqual(result(submit()), "accepted")
            XCTAssertEqual(result(submit()), "accepted")
            XCTAssertEqual(handed, [1, 2, 3], "a revision was spent on a snapshot that never left")
        }
    }

    /// `WatchSender` performs what `WatchSend` decides: twenty changes before the
    /// queued send's turn are one submit, a failure is retried, a refusal reported.
    func testTheSenderSubmitsOncePerQueuedSend() {
        var observers = 0, queued: [() -> Void] = [], timers: [() -> Void] = [], reports: [WatchSend.Refusal] = []
        var answers: [WatchSend.Submitted] = [.accepted, .failed, .accepted, .refused(.oversized)], submits = 0
        let sender = WatchSender(ports: .init(
            observeForeground: { observers += 1 }, activate: { .activated }, enqueue: { queued.append($0) },
            submit: { submits += 1; return answers.removeFirst() }, report: { reports.append($0) },
            schedule: { _, work in timers.append(work) }))
        /// The oldest waiting work's turn, if any waits.
        func turn(_ waiting: inout [() -> Void]) { if !waiting.isEmpty { waiting.removeFirst()() } }
        for _ in 0..<20 { sender.feed(.changed) }
        XCTAssertEqual(queued.count, 1)
        turn(&queued)
        XCTAssertEqual(submits, 1)
        sender.feed(.changed)
        turn(&queued)
        XCTAssertEqual(submits, 2, "a later change still sends")
        XCTAssertEqual(timers.count, 1, "the failed send has no retry")
        turn(&timers)
        turn(&queued)
        XCTAssertEqual(submits, 3)
        sender.feed(.changed)
        turn(&queued)
        XCTAssertEqual(reports, [.oversized])
        XCTAssertEqual(observers, 1)
        XCTAssertTrue(queued.isEmpty && timers.isEmpty)
    }

    func testJSONRevisionMustBeAPositiveInteger() throws {
        let key = Curve25519.KeyAgreement.PrivateKey()
        for revision in ["0", "-1", "1.5", "true", "18446744073709551616"] {
            let data = Data("{\"sender\":\"phone\",\"revision\":\(revision),\"values\":{}}".utf8)
            let snapshot = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
            XCTAssertThrowsError(try WatchEnvelope.seal(snapshot: snapshot, recipient: "watch", publicKey: key.publicKey.rawRepresentation), revision)
        }
    }

    func testNodeDrawsReplicateOnlyOncePerGeneration() throws {
        let app = ExactApp.shared
        let previous = app.replicateStore
        defer { app.replicateStore = previous }
        var sends = 0
        app.replicateStore = { sends += 1 }
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let session = app.makeSession(label: "watch-send")
        session.boot(size: CGSize(width: 200, height: 200))
        defer { session.destroy() }
        for _ in 0..<20 { session.firstDrawn(generation: session.generation, token: 0) }
        XCTAssertEqual(sends, 1)
    }

    func testRetainedContextHasCiphertextAndAuthenticatesStateAndRevision() throws {
        let key = Curve25519.KeyAgreement.PrivateKey()
        let snapshot: [String: Any] = ["sender": "phone", "revision": UInt64(8),
            "values": ["seed": "synthetic-secret-only", "exact.state.liked": "true", "deleted": NSNull()]]
        let envelope = try WatchEnvelope.seal(snapshot: snapshot, recipient: "watch", publicKey: key.publicKey.rawRepresentation)
        let bytes = try WatchEnvelope.wireData(envelope)
        XCTAssertFalse(String(decoding: bytes, as: UTF8.self).contains("synthetic-secret-only"))
        let result = try envelope.open(recipient: "watch", key: key)
        let values = try XCTUnwrap(result["values"] as? [String: Any])
        XCTAssertEqual(values["seed"] as? String, "synthetic-secret-only")
        XCTAssertTrue(values["deleted"] is NSNull)
        XCTAssertEqual(values["exact.state.liked"] as? String, "true")
        var wire = try XCTUnwrap(JSONSerialization.jsonObject(with: bytes) as? [String: Any])
        wire["revision"] = 9
        let changed = try JSONDecoder().decode(WatchEnvelope.self, from: JSONSerialization.data(withJSONObject: wire))
        XCTAssertThrowsError(try changed.open(recipient: "watch", key: key))
        wire["revision"] = 8; wire["state"] = ["exact.state.liked": "false"]
        let state = try JSONDecoder().decode(WatchEnvelope.self, from: JSONSerialization.data(withJSONObject: wire))
        XCTAssertThrowsError(try state.open(recipient: "watch", key: key))
    }
    func testReplacementWatchAndSkippedRevisions() throws {
        let key = Curve25519.KeyAgreement.PrivateKey(), replacement = Curve25519.KeyAgreement.PrivateKey()
        for revision in [UInt64(1), 10, 100] {
            let envelope = try WatchEnvelope.seal(snapshot: ["sender": "phone", "revision": revision, "values": ["seed": "fixture"]], recipient: "first", publicKey: key.publicKey.rawRepresentation)
            XCTAssertNoThrow(try envelope.open(recipient: "first", key: key))
            XCTAssertThrowsError(try envelope.open(recipient: "second", key: key))
            XCTAssertThrowsError(try envelope.open(recipient: "first", key: replacement))
        }
    }
}
