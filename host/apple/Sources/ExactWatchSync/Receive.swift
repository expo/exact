// The watch app's side of replication as one transition function.
//
// `step` takes the process's state and one event. It answers the effects to perform, in
// order. `WatchReceiver` performs a step's effects on one serial queue, then consumes
// their answers in order before outside events. Local readiness must precede activation:
// a synchronous activation answer can start a retained import.
//
// Readers open on the last applied store, without waiting for activation. WCSession's
// retained context is valid only after activation completes (`WCSession.h`); importing
// it then refreshes readers. An unfinished import still keeps new readers closed.
//
// The phone seals to the key this watch publishes, so a publication WatchConnectivity
// refuses is owed and retried. Readers never wait for it: the store is readable with no
// phone in reach. Nothing here names a WatchConnectivity type.
enum WatchReceive {
    enum Link { case idle, activating, active, failed }
    enum Readers {
        /// Nobody has asked for the store.
        case closed
        /// The boot waits for `open`.
        case waiting
        case open
    }

    /// The longest wait between retries, in seconds; the waits double up to it.
    static let longestWait = 32

    struct State: Hashable {
        var link = Link.idle
        /// The retained context may hold a snapshot this process has not reconciled.
        var held = false
        var readers = Readers.closed
        /// A live session can be told to read its store again.
        var subscribed = false
        /// An import landed after readers opened, before a session could be told.
        var refreshOwed = false
        /// The last publication of the key failed: the phone may not hold it.
        var keyOwed = false
        var sceneActive = false
        /// A retry is scheduled.
        var timer = false
        /// The next retry's wait, in seconds.
        var wait = 1
    }

    enum Outcome {
        /// The retained context was imported: the store changed.
        case applied
        /// Nothing to import: no context, one already imported, or one this watch refuses
        /// (another installation's key, an unknown format, a retired phone).
        case settled
        /// The context could not be imported now, and stays owed.
        case failed
    }

    enum Event {
        /// The app asks for its store's readers.
        case launch
        case activated
        case activationFailed
        /// WCSession delivered a newer context.
        case context
        /// A `reconcile`'s answer; `ready` is whether the store holds no unfinished import.
        case reconciled(Outcome, ready: Bool)
        /// A `publishKey`'s answer: whether WatchConnectivity took the key.
        case published(Bool)
        case scene(active: Bool)
        /// A live session registered for `refresh`.
        case subscribed
        /// A scheduled retry is due.
        case retry
    }

    enum Effect: Hashable {
        /// Activate WCSession; answered by `activated` or `activationFailed`.
        case activate
        /// Import the retained context when `context`, then ask whether the store is
        /// readable; answered by `reconciled`.
        case reconcile(context: Bool)
        /// Readers may open. Performed once.
        case open
        /// The store is not readable yet. The boot keeps waiting for `open`.
        case fail
        /// The live session reads its store again.
        case refresh
        case reloadComplications
        /// Hand the phone this watch's key; answered by `published`.
        case publishKey
        /// Feed `retry` after this many seconds.
        case schedule(seconds: Int)
    }

    /// Work a retry can finish: an activation to try again, a context to import,
    /// readers waiting for their store, or a key to publish.
    static func owed(_ s: State) -> Bool {
        s.link == .failed || s.readers == .waiting || (s.link == .active && (s.held || s.keyOwed))
    }

    /// Feed `event`, then each effect's answer, until none is left.
    static func run(_ s: inout State, _ event: Event, perform: (Effect) -> Event?) {
        var events = [event]
        while !events.isEmpty {
            for effect in step(&s, events.removeFirst()) {
                if let answer = perform(effect) { events.append(answer) }
            }
        }
    }

    static func step(_ s: inout State, _ event: Event) -> [Effect] {
        var effects: [Effect] = []
        func activate() {
            s.link = .activating
            effects.append(.activate)
        }
        func reconcile() {
            if s.link == .active, s.held || s.readers == .waiting {
                effects.append(.reconcile(context: true))
            } else if s.readers == .waiting {
                effects.append(.reconcile(context: false))
            }
        }
        func retryLater() {
            guard owed(s), s.sceneActive, !s.timer else { return }
            s.timer = true
            effects.append(.schedule(seconds: s.wait))
            s.wait = min(s.wait * 2, longestWait)
        }
        switch event {
        case .launch:
            if s.readers == .closed { s.readers = .waiting }
            reconcile()
            if s.link == .idle { activate() }
        case .activated:
            s.link = .active
            s.held = true
            effects.append(.publishKey)
            reconcile()
        case .activationFailed:
            s.link = .failed
            reconcile()
            retryLater()
        case .context:
            // Only an activated session delivers one; until then `activated` reads it.
            guard s.link == .active else { break }
            s.held = true
            reconcile()
        case let .reconciled(outcome, ready):
            if s.link == .active, outcome != .failed { s.held = false }
            if outcome == .applied {
                effects.append(.reloadComplications)
                if s.readers == .open {
                    if s.subscribed { effects.append(.refresh) } else { s.refreshOwed = true }
                }
            }
            if s.readers == .waiting {
                if ready {
                    s.readers = .open
                    effects.append(.open)
                } else {
                    effects.append(.fail)
                }
            }
            if !owed(s) { s.wait = 1 }
            retryLater()
        case let .published(taken):
            s.keyOwed = !taken
            if !owed(s) { s.wait = 1 }
            retryLater()
        case let .scene(active):
            s.sceneActive = active
            guard active else { break }
            s.wait = 1
            switch s.link {
            case .idle, .failed:
                reconcile()
                activate()
            case .activating: reconcile()
            case .active:
                effects.append(.publishKey)
                reconcile()
            }
        case .subscribed:
            s.subscribed = true
            if s.refreshOwed {
                s.refreshOwed = false
                effects.append(.refresh)
            }
        case .retry:
            s.timer = false
            guard s.sceneActive else { break }
            if s.link == .failed {
                reconcile()
                activate()
            } else {
                if s.keyOwed { effects.append(.publishKey) }
                reconcile()
            }
        }
        return effects
    }
}
