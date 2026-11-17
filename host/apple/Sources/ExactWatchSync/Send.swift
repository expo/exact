// The phone's side of replication as one transition function, shaped as `WatchReceive` is.
//
// A send is one `submit`: read the selected values, seal them, measure the dictionary
// WatchConnectivity will be handed, and only then spend a revision on it. `WatchSender`
// performs the effects on one serial queue; `submitted` always answers the `submit`
// before it. Changes that arrive while a send waits on the queue share it.
enum WatchSend {
    enum Link { case idle, activating, active, failed }

    /// The longest wait between retries, in seconds; the waits double up to it.
    static let longestWait = 32

    enum Refusal: Hashable {
        /// No paired watch has announced a key.
        case noWatch
        /// The selected values do not fit WatchConnectivity's application context.
        case oversized
    }

    struct State: Hashable {
        var link = Link.idle
        /// The foreground observer is registered, once for the process.
        var observing = false
        /// The store may hold values no accepted send carried.
        var owed = false
        /// A send waits on the queue.
        var queued = false
        var timer = false
        /// The next retry's wait, in seconds.
        var wait = 1
        /// The last submit found no watch to send to; `peer` sends it.
        var noWatch = false
        /// The oversized refusal was reported, and no send has been accepted since.
        var reported = false
    }

    enum Submitted {
        /// WatchConnectivity took the context.
        case accepted
        case refused(Refusal)
        /// The store or WatchConnectivity failed; the same values may be sent later.
        case failed
    }

    enum Event {
        /// First pixel, or a durable write to the app's store.
        case changed
        case foreground
        case activated
        case activationFailed
        /// The active watch changed: activate for the new one.
        case deactivated
        /// The watch announced a key, or was paired, unpaired, installed or removed.
        case peer
        /// The queued send's turn.
        case send
        case submitted(Submitted)
        case retry
    }

    enum Effect: Hashable {
        case observeForeground
        /// Activate WCSession; answered by `activated` or `activationFailed`.
        case activate
        /// Feed `send` in a later turn of the queue.
        case enqueue
        /// Send one snapshot; answered by `submitted`.
        case submit
        /// Tell the developer a selection that can never be sent.
        case report(Refusal)
        /// Feed `retry` after this many seconds.
        case schedule(seconds: Int)
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
        /// Move an owed send forward: activate, or queue it once.
        func advance() {
            guard s.owed else { return }
            switch s.link {
            case .idle, .failed:
                s.link = .activating
                effects.append(.activate)
            case .activating: break
            case .active:
                guard !s.queued else { return }
                s.queued = true
                effects.append(.enqueue)
            }
        }
        switch event {
        case .changed:
            if !s.observing {
                s.observing = true
                effects.append(.observeForeground)
            }
            s.owed = true
            advance()
        case .foreground:
            s.wait = 1
            advance()
        case .activated:
            s.link = .active
            advance()
        case .activationFailed:
            s.link = .failed
            if s.owed, !s.timer {
                s.timer = true
                effects.append(.schedule(seconds: s.wait))
                s.wait = min(s.wait * 2, longestWait)
            }
        case .deactivated:
            s.link = .activating
            s.owed = true
            effects.append(.activate)
        case .peer:
            s.owed = true
            s.noWatch = false
            advance()
        case .send:
            s.queued = false
            guard s.owed, s.link == .active else { break }
            // A write after this read raises `changed`, which owes the next send.
            s.owed = false
            effects.append(.submit)
        case let .submitted(result):
            switch result {
            case .accepted:
                s.noWatch = false
                s.reported = false
                s.wait = 1
            case .refused(.noWatch):
                s.owed = true
                s.noWatch = true
            case .refused(.oversized):
                // The same values cannot fit: the next `changed` owes a send.
                s.noWatch = false
                if !s.reported {
                    s.reported = true
                    effects.append(.report(.oversized))
                }
            case .failed:
                s.owed = true
                if !s.timer {
                    s.timer = true
                    effects.append(.schedule(seconds: s.wait))
                    s.wait = min(s.wait * 2, longestWait)
                }
            }
        case .retry:
            s.timer = false
            advance()
        }
        return effects
    }
}
