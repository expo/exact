// The only WCSession owner. Delegate and lifecycle callbacks become events on one queue;
// `WatchReceive` (the watch) and `WatchSend` (the phone) decide what follows.
#if os(iOS) || os(watchOS)
import CryptoKit
import ExactCore
import Foundation
import WatchConnectivity
import os
#if os(iOS)
import UIKit
#else
import WidgetKit
#endif

public final class ExactWatchSync: NSObject, WCSessionDelegate, @unchecked Sendable {
    private static let shared = ExactWatchSync()
    private let queue = DispatchQueue(label: "dev.exact.watch-sync")
    private let log = Logger(subsystem: "dev.exact.watch", category: "sync")
    private var store: WatchStore?
    private var session: WCSession?

    /// Nil while WCSession has yet to answer through its delegate.
    private func activate() -> Bool? {
        guard WCSession.isSupported() else { return false }
        let session = WCSession.default
        self.session = session
        session.delegate = self
        if session.activationState == .activated { return true }
        session.activate()
        return nil
    }
    private func loadStore(watch: Bool) throws -> WatchStore {
        if let store { return store }
        let loaded = try WatchStore(bundle: .main, watch: watch)
        store = loaded
        return loaded
    }
    private func after(_ seconds: Int, _ work: @escaping () -> Void) {
        queue.asyncAfter(deadline: .now() + .seconds(seconds), execute: work)
    }

    #if os(watchOS)
    private var key: WatchKey?
    private var ready: ((Result<Void, Error>) -> Void)?
    private var refresh: (() -> Void)?
    private lazy var receiver: WatchReceiver = .init(ports: .init(
        activate: { [unowned self] in self.activate().map { $0 ? .activated : .activationFailed } },
        retained: { [unowned self] in self.session?.receivedApplicationContext["snapshot"] as? Data },
        store: { [unowned self] in try self.loadStore(watch: true) },
        key: { [unowned self] in
            if let key = self.key { return key }
            let loaded = try WatchKey.load(defaults: .standard, service: self.loadStore(watch: true).app)
            self.key = loaded
            return loaded
        },
        open: { [unowned self] result in
            let boot = self.ready
            if case .success = result { self.ready = nil }
            boot?(result)
        },
        refresh: { [unowned self] in if let refresh = self.refresh { DispatchQueue.main.async(execute: refresh) } },
        reloadComplications: {
            let entries = Bundle.main.object(forInfoDictionaryKey: "ExactWatchEntries") as? [[String: Any]] ?? []
            for entry in entries {
                if let kind = entry["kind"] as? String { WidgetCenter.shared.reloadTimelines(ofKind: kind) }
            }
        },
        publish: { [unowned self] key in
            guard let session = self.session, session.activationState == .activated else { return false }
            do {
                try session.updateApplicationContext(["installation": key.installation, "publicKey": key.key.publicKey.rawRepresentation])
                return true
            } catch {
                self.log.notice("the watch's key was not published")
                return false
            }
        },
        schedule: { [unowned self] in self.after($0, $1) }))

    /// Ask for the store's readers. `ready` succeeds once, after the retained context is
    /// reconciled; each failure before that says the store is not readable yet.
    public static func prepareWatch(ready: @escaping (Result<Void, Error>) -> Void) {
        shared.queue.async { shared.ready = ready; shared.receiver.feed(.launch) }
    }
    public static func scene(active: Bool) {
        shared.queue.async { shared.receiver.feed(.scene(active: active)) }
    }
    /// `refresh` runs on the main queue after each import the live session has not read.
    public static func subscribe(refresh: @escaping () -> Void) {
        shared.queue.async { shared.refresh = refresh; shared.receiver.feed(.subscribed) }
    }
    #else
    private var foreground: NSObjectProtocol?
    private var report: ((String) -> Void)?
    private var peer: (id: String, key: Data)?
    private var pairing: URL?
    private let name = UUID().uuidString
    private lazy var sender: WatchSender = .init(ports: .init(
        observeForeground: { [unowned self] in
            self.foreground = NotificationCenter.default.addObserver(forName: UIApplication.didBecomeActiveNotification, object: nil, queue: nil) { [unowned self] _ in
                self.queue.async { self.sender.feed(.foreground) }
            }
        },
        activate: { [unowned self] in self.activate().map { $0 ? .activated : .activationFailed } },
        enqueue: { [unowned self] in self.queue.async(execute: $0) },
        submit: { [unowned self] in self.submit() },
        report: { [unowned self] refusal in
            guard refusal == .oversized else { return }
            let line = "watch.sync: the selected values exceed the \(WatchEnvelope.limit) bytes WatchConnectivity carries; the watch keeps its last copy until they fit"
            self.log.error("\(line, privacy: .public)")
            if let report = self.report { DispatchQueue.main.async { report(line) } }
        },
        schedule: { [unowned self] in self.after($0, $1) }))

    /// The phone's hook for first pixel and durable store changes. `report` receives, on
    /// the main queue, a line for the developer when a selection can never be sent.
    public static func phone(report: @escaping (String) -> Void) -> () -> Void {
        shared.queue.async { shared.report = report }
        return { shared.queue.async { shared.sender.feed(.changed) } }
    }

    private func submit() -> WatchSend.Submitted {
        guard let session, session.activationState == .activated, session.isPaired, session.isWatchAppInstalled else { return .refused(.noWatch) }
        // A watch switch changes this directory. A cached key never follows it.
        if pairing != session.watchDirectoryURL { pairing = session.watchDirectoryURL; peer = nil }
        if peer == nil { acceptKey(session.receivedApplicationContext) }
        guard let peer else { return .refused(.noWatch) }
        guard let store = try? loadStore(watch: false) else { return .failed }
        return store.submit(sender: name, recipient: peer.id, publicKey: peer.key) { try session.updateApplicationContext($0) }
    }
    private func acceptKey(_ context: [String: Any]) {
        guard let id = context["installation"] as? String, UUID(uuidString: id) != nil,
              let bytes = context["publicKey"] as? Data,
              (try? Curve25519.KeyAgreement.PublicKey(rawRepresentation: bytes)) != nil else { return }
        peer = (id, bytes)
    }
    #endif

    public func session(_ session: WCSession, activationDidCompleteWith state: WCSessionActivationState, error: Error?) {
        queue.async {
            guard session === self.session else { return }
            let active = state == .activated && error == nil
            #if os(watchOS)
            self.receiver.feed(active ? .activated : .activationFailed)
            #else
            if active { self.pairing = session.watchDirectoryURL; self.acceptKey(session.receivedApplicationContext) }
            self.sender.feed(active ? .activated : .activationFailed)
            #endif
        }
    }
    public func session(_ session: WCSession, didReceiveApplicationContext context: [String: Any]) {
        queue.async {
            guard session === self.session, session.activationState == .activated else { return }
            #if os(watchOS)
            self.receiver.feed(.context)
            #else
            self.acceptKey(session.receivedApplicationContext)
            self.sender.feed(.peer)
            #endif
        }
    }
    #if os(iOS)
    public func sessionDidBecomeInactive(_ session: WCSession) {
        queue.async { self.peer = nil; self.pairing = nil }
    }
    public func sessionDidDeactivate(_ session: WCSession) {
        queue.async { self.peer = nil; self.pairing = nil; self.sender.feed(.deactivated) }
    }
    public func sessionWatchStateDidChange(_ session: WCSession) { queue.async { self.sender.feed(.peer) } }
    #endif
}
#endif
