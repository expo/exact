#if os(iOS) || os(tvOS)
import UIKit

/// A subsystem's membership beneath each scroller. Geometry does not change
/// ancestry; attachment and changes to the subsystem's members invalidate it.
final class ScrollMembership {
    static var epoch: UInt64 = 0
    private struct Entry {
        weak var scroll: UIScrollView?
        let epoch: UInt64
        let contains: Bool
    }
    private var entries: [ObjectIdentifier: Entry] = [:]
    private var epoch = ScrollMembership.epoch

    func contains(_ scroll: UIScrollView, members: @autoclosure () -> [UIView]) -> Bool {
        if epoch != Self.epoch {
            entries.removeAll(keepingCapacity: true)
            epoch = Self.epoch
        }
        let key = ObjectIdentifier(scroll)
        if let entry = entries[key], entry.scroll === scroll, entry.epoch == Self.epoch { return entry.contains }
        let found = members().contains { $0.isDescendant(of: scroll) }
        entries[key] = Entry(scroll: scroll, epoch: Self.epoch, contains: found)
        return found
    }
}

extension Presenter {
    func noteFixedGradient(_ node: NodeView, fixed: Bool) {
        guard fixedGradients.contains(node) != fixed else { return }
        if fixed { fixedGradients.add(node) } else { fixedGradients.remove(node) }
        ScrollMembership.epoch &+= 1
    }
    func gradientsMove(in scroll: UIScrollView) -> Bool {
        guard fixedGradients.count > 0 else { return false }
        return gradientScrolls.contains(scroll, members: fixedGradients.allObjects)
    }
}

extension ScrollView {
    package override func didMoveToSuperview() {
        super.didMoveToSuperview()
        ScrollMembership.epoch &+= 1
    }
}
#endif
