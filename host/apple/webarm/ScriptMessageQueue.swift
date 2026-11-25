// The issuing package session owns one FIFO across its frames and sandboxes (LLP 1020.002 D9).
import Foundation

final class ScriptMessageQueue {
    typealias Send = (NSDictionary) -> Void
    typealias Hold = @convention(block) (Bool) -> Void
    struct Start: Equatable {
        let handler: String
        let view: ObjectIdentifier
        let turn: String
    }
    private struct Entry {
        let start: Start
        let record: NSDictionary
        let send: Send
        var deadline: DispatchTime
        var ready = false
    }

    private var unsent: [Entry] = []
    private var reported = false
    private var draining = false
    private var held = false
    private var wake: DispatchWorkItem?
    private var closed = false

    func admit(_ start: Start, _ record: NSDictionary, at time: DispatchTime = .now(), send: @escaping Send) {
        guard !closed else { return }
        let overflow = unsent.count == 32
        if overflow {
            // A busy guest may never publish its turn. Keep the pending wake's deadline
            // when overflow replaces its head, so sustained input cannot postpone delivery.
            let deadline = unsent.removeFirst().deadline
            unsent[0].deadline = deadline
        }
        unsent.append(Entry(start: start, record: record, send: send, deadline: time + .milliseconds(50)))
        if overflow && !reported {
            reported = true
            send(["overflow": true])
        }
        schedule()
    }

    func publish(_ start: Start) {
        for index in unsent.indices where unsent[index].start == start { unsent[index].ready = true }
        drain()
    }

    func close() {
        closed = true
        wake?.cancel()
        wake = nil
        unsent.removeAll()
    }

    // A guest can stay busy after posting. Its checkpoint normally publishes the turn,
    // but the host already owns each admitted value and can deliver it on its own clock.
    private func schedule() {
        guard !held, !draining, wake == nil, let first = unsent.first else { return }
        let work = DispatchWorkItem { [self] in wake = nil; drain() }
        wake = work
        DispatchQueue.main.asyncAfter(deadline: first.deadline, execute: work)
    }

    private func drain() {
        guard !draining, !held else { return }
        draining = true
        wake?.cancel()
        wake = nil
        while !held, let next = unsent.first, next.ready || next.deadline <= .now() {
            unsent.removeFirst()
            if unsent.isEmpty { reported = false }
            let message = next.record.mutableCopy() as! NSMutableDictionary
            // A synchronous receiver returns here. A package holds delivery until its handler
            // has answered and the session's pacing interval has elapsed.
            var receiving = true, claimed = false
            let hold: Hold = { [weak self] busy in
                guard let self else { return }
                if busy {
                    if receiving { claimed = true; held = true }
                } else if claimed {
                    claimed = false
                    held = false
                    drain()
                }
            }
            message["holdDelivery"] = hold as AnyObject
            next.send(message)
            receiving = false
        }
        draining = false
        schedule()
    }
}
