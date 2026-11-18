import CExact
import Foundation

/// Numeric scroll facts, guarded by the owner's queue lock. Observed
/// offsets retain their order even when a newer mailbox fact drains first.
package struct ScrollMailbox {
    package struct Values {
        package enum Position {
            case offset(page: Bool, left: Double, top: Double)
            case collection(revision: UInt64, sequence: UInt64, offset: Double)
        }
        package let lease: RuntimeLease
        package let runtime: UInt32, view: UInt32
        package let position: Position

        fileprivate var key: Key { Key(runtime: runtime, view: view) }
        fileprivate var orderKey: OrderKey {
            OrderKey(key: key, session: ObjectIdentifier(lease.session), generation: lease.generation)
        }
        func apply() {
            switch position {
            case .offset(let page, let left, let top):
                exact_scrolled(runtime, page ? 1 : 0, view, left, top)
            case .collection(let revision, let sequence, let offset):
                guard let bytes = exact_in(runtime, 16) else { return }
                UnsafeMutableRawPointer(bytes).storeBytes(of: revision.littleEndian, as: UInt64.self)
                UnsafeMutableRawPointer(bytes).storeBytes(of: sequence.littleEndian, toByteOffset: 8, as: UInt64.self)
                exact_scrolled(runtime, 2, view, 0, offset)
            }
        }
    }
    package struct Note {
        package let values: Values
        fileprivate let serial: UInt64
    }
    fileprivate struct Key: Hashable {
        let runtime: UInt32, view: UInt32
    }
    fileprivate struct OrderKey: Hashable {
        let key: Key
        let session: ObjectIdentifier
        let generation: Int
    }
    private struct Order {
        var applied: UInt64 = 0
        var pending = 0
    }
    private struct Slot {
        var offset: Note?
        var collection: Note?
    }
    private var serial: UInt64 = 0
    private var offsets: [Key: Slot] = [:]
    private var orders: [OrderKey: Order] = [:]
    package var pending: Int { offsets.count }
    package var observed: Int { orders.count }

    @discardableResult
    package mutating func record(_ values: Values, observing: Bool = false) -> Note {
        serial += 1
        let note = Note(values: values, serial: serial)
        if case .offset = values.position { orders[values.orderKey, default: Order()].pending += 1 }
        if !observing {
            switch values.position {
            case .offset:
                if let replaced = offsets[values.key]?.offset { _ = finish(replaced, applying: false) }
                offsets[values.key, default: Slot()].offset = note
            case .collection: offsets[values.key, default: Slot()].collection = note
            }
        }
        return note
    }

    package mutating func take() -> [Note] {
        var pending: [Note] = []
        pending.reserveCapacity(offsets.count * 2)
        for slot in offsets.values {
            if let offset = slot.offset { pending.append(offset) }
            if let collection = slot.collection { pending.append(collection) }
        }
        offsets.removeAll(keepingCapacity: true)
        return pending
    }

    package mutating func claim(_ note: Note) -> Bool {
        if case .collection = note.values.position { return note.values.lease.session.accepts(note.values.lease) }
        return finish(note, applying: true) && note.values.lease.session.accepts(note.values.lease)
    }

    private mutating func finish(_ note: Note, applying: Bool) -> Bool {
        let key = note.values.orderKey
        guard var order = orders[key] else { return false }
        let apply = applying && note.serial > order.applied
        if apply { order.applied = note.serial }
        order.pending -= 1
        // Buffered offsets can arrive after the last queued observation.
        orders[key] = order.pending == 0 ? nil : order
        return apply
    }
}
