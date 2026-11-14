#if os(macOS)
import AppKit

extension Presenter {
    func orderChildren(_ children: [NodeView], in container: NSView) {
        // Detaching a retained child clears AppKit's first responder. Sort
        // in place; a flight's slot stands for its lifted child (LLP 1013.000).
        // Exit ghosts keep their slots among both live and flying children.
        let wanted = Set(children.map(ObjectIdentifier.init))
        let current = container.subviews
        var ordered: [NSView] = children.compactMap { child in
            if child.superview === container { return child }
            guard let flight = flights[child.id], flight.view === child,
                  let slot = flight.slot, slot.superview === container else { return nil }
            return slot
        }
        let placed = Set(ordered.map(ObjectIdentifier.init))
        let peers = current.filter { $0 is NodeView || $0 is FlightSlot }
        for (index, child) in peers.enumerated() where !placed.contains(ObjectIdentifier(child)) &&
            ((child as? NodeView)?.paintGhost == true || child is FlightSlot) {
            ordered.insert(child, at: min(index, ordered.count))
        }
        let leaving = current.filter { ($0 as? NodeView).map { !$0.paintGhost } ?? false && !wanted.contains(ObjectIdentifier($0)) }
        let native = current.filter { !($0 is NodeView) && !($0 is FlightSlot) }
        ordered = native + ordered + leaving
        guard !ordered.elementsEqual(current, by: { $0 === $1 }) else { return }
        var ranks = Dictionary(uniqueKeysWithValues: ordered.enumerated().map { (ObjectIdentifier($0.element), $0.offset) })
        withUnsafeMutablePointer(to: &ranks) { context in
            container.sortSubviews({ left, right, raw in
                let order = raw!.assumingMemoryBound(to: [ObjectIdentifier: Int].self).pointee
                let a = order[ObjectIdentifier(left)]!, b = order[ObjectIdentifier(right)]!
                return a < b ? .orderedAscending : a > b ? .orderedDescending : .orderedSame
            }, context: context)
        }
    }
}
#endif
