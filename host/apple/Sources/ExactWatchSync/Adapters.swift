// The adapters perform `WatchReceive`'s and `WatchSend`'s effects and decide nothing.
// Each is used on one serial queue. Its ports are the I/O: WCSession and the store in
// the app, fakes in a test.
import Foundation

final class WatchReceiver {
    struct Ports {
        /// Start activating. An answer known at once is returned; otherwise it is fed later.
        var activate: () -> WatchReceive.Event?
        /// WCSession's retained snapshot, readable once activation completed.
        var retained: () -> Data?
        var store: () throws -> WatchStore
        var key: () throws -> WatchKey
        /// Success opens readers, once. A failure leaves the caller waiting for it.
        var open: (Result<Void, Error>) -> Void
        var refresh: () -> Void
        var reloadComplications: () -> Void
        /// Whether WatchConnectivity took the key for the phone.
        var publish: (WatchKey) -> Bool
        var schedule: (_ seconds: Int, _ work: @escaping () -> Void) -> Void
    }
    private(set) var state = WatchReceive.State()
    private let ports: Ports
    init(ports: Ports) { self.ports = ports }

    func feed(_ event: WatchReceive.Event) {
        WatchReceive.run(&state, event) { effect in
            switch effect {
            case .activate: return ports.activate()
            case let .reconcile(context):
                let (outcome, ready) = reconcile(context: context)
                return .reconciled(outcome, ready: ready)
            case .open: ports.open(.success(()))
            case .fail: ports.open(.failure(WatchEnvelope.Failure.unavailable))
            case .refresh: ports.refresh()
            case .reloadComplications: ports.reloadComplications()
            // A Keychain that refuses the key now may release it later.
            case .publishKey: return .published((try? ports.key()).map(ports.publish) ?? false)
            case let .schedule(seconds): ports.schedule(seconds) { [weak self] in self?.feed(.retry) }
            }
            return nil
        }
    }

    private func reconcile(context: Bool) -> (WatchReceive.Outcome, Bool) {
        do {
            let store = try ports.store()
            var outcome = WatchReceive.Outcome.settled
            if context, let data = ports.retained() {
                // A Keychain that refuses the key now may release it later.
                let key = try ports.key()
                outcome = store.receive(data, recipient: key.installation, key: key.key)
            }
            return (outcome, store.ready())
        } catch { return (.failed, false) }
    }
}

final class WatchSender {
    struct Ports {
        var observeForeground: () -> Void
        /// Start activating. An answer known at once is returned; otherwise it is fed later.
        var activate: () -> WatchSend.Event?
        /// Run `work` in a later turn of the queue.
        var enqueue: (_ work: @escaping () -> Void) -> Void
        var submit: () -> WatchSend.Submitted
        var report: (WatchSend.Refusal) -> Void
        var schedule: (_ seconds: Int, _ work: @escaping () -> Void) -> Void
    }
    private(set) var state = WatchSend.State()
    private let ports: Ports
    init(ports: Ports) { self.ports = ports }

    func feed(_ event: WatchSend.Event) {
        WatchSend.run(&state, event) { effect in
            switch effect {
            case .observeForeground: ports.observeForeground()
            case .activate: return ports.activate()
            case .enqueue: ports.enqueue { [weak self] in self?.feed(.send) }
            case .submit: return .submitted(ports.submit())
            case let .report(refusal): ports.report(refusal)
            case let .schedule(seconds): ports.schedule(seconds) { [weak self] in self?.feed(.retry) }
            }
            return nil
        }
    }
}
