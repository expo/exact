// @ref LLP 1010 §6.12 — a list's rows move as a collection view's batch update
// moves its cells: from where they showed to where they are, on one spring from
// one frame, the port's own change included, when a runner commit moved them
// (`Batch.rides`) or a row left with an exit animation.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit
import ExactCore

extension CollectionHost {
    /// A collection view's batch update moving a layer back by `gap` points,
    /// additively: UIKit's spring for it, a response of 0.3 s and no bounce
    /// (read in the iOS 27.0 simulator's UIKit).
    static func batchMotion(_ keyPath: String, from gap: CGFloat) -> CAAnimation {
        let motion = CASpringAnimation(perceptualDuration: 0.3, bounce: 0)
        motion.keyPath = keyPath
        motion.fromValue = gap; motion.toValue = 0; motion.isAdditive = true
        motion.duration = motion.settlingDuration
        return motion
    }

    /// Whether this batch began the exit of one of the list's rows (a
    /// deleted message fading in place); `began`: the exits under way before it.
    func removes(_ id: UInt32, began: Set<UInt32>) -> Bool {
        guard let presenter, let rows = entries[id]?.batchRows else { return false }
        return presenter.leaving.keys.contains { !began.contains($0) && rows[$0] != nil }
    }

    /// Moves each row the batch shifted on the screen, the leaving ones too,
    /// from where it showed to where it is, additively: rows after a removal
    /// at a followed end stay put and the rows before it come down over the
    /// gap as the leaving row fades; rows above a row that grew at a followed
    /// end go up as the port takes the new end at once. A row the batch
    /// mounted moves as the row it is nearest to. Under Reduce Motion, the
    /// agent's frozen clock, or inside an animation (the keyboard's, which the
    /// rows already ride), they take their places at once.
    func rideBatch(_ id: UInt32) {
        guard !DisplayPreferences.reducedMotion, !ExactEnv.agentFreezes, UIView.inheritedAnimationDuration == 0,
              let presenter, let entry = entries[id], let scroll = presenter.views[id]?.scroll else { return }
        let horizontal = entry.snapshot.horizontal, key = "ride-\(CACurrentMediaTime())", path = horizontal ? "position.x" : "position.y"
        let offset = horizontal ? scroll.contentOffset.x : scroll.contentOffset.y
        var seen = Set<UInt32>(), moves: [(at: CGFloat, by: CGFloat)] = [], mounted: [(view: UIView, at: CGFloat)] = []
        for row in Array(entry.batchRows.keys) + entry.snapshot.rows.map(\.view) where seen.insert(row).inserted {
            guard let view = presenter.views[row] ?? presenter.leaving[row]?.view, view.isDescendant(of: scroll) else { continue }
            let at = start(of: view, in: scroll, horizontal: horizontal) - offset
            guard let showed = entry.batchRows[row] else { mounted.append((view, at)); continue }
            moves.append((at, showed - at))
            if abs(showed - at) > 0.5 { view.layer.add(Self.batchMotion(path, from: showed - at), forKey: key) }
        }
        for (view, at) in mounted {
            guard let nearest = moves.min(by: { abs($0.at - at) < abs($1.at - at) }), abs(nearest.by) > 0.5 else { continue }
            view.layer.add(Self.batchMotion(path, from: nearest.by), forKey: key)
        }
    }

    /// Where each of a list's mounted rows, and each row still leaving it,
    /// shows along its axis: its start in the scroll view less the offset.
    func rowStarts(_ id: UInt32) -> [UInt32: CGFloat] {
        guard let presenter, let entry = entries[id], let scroll = presenter.views[id]?.scroll else { return [:] }
        let horizontal = entry.snapshot.horizontal, offset = horizontal ? scroll.contentOffset.x : scroll.contentOffset.y
        var starts: [UInt32: CGFloat] = [:], parent: UIView?
        for row in entry.snapshot.rows {
            guard let view = presenter.views[row.view] else { continue }
            starts[row.view] = start(of: view, in: scroll, horizontal: horizontal) - offset
            parent = view.superview
        }
        // A row still leaving lies beside the mounted ones
        for (row, leaving) in presenter.leaving where parent != nil && leaving.view.superview === parent {
            starts[row] = start(of: leaving.view, in: scroll, horizontal: horizontal) - offset
        }
        return starts
    }

    /// A row's start in the scroll view along the list's axis, where its
    /// layout put it (its layer's model, not a ride under way).
    private func start(of view: UIView, in scroll: UIScrollView, horizontal: Bool) -> CGFloat {
        let frame = view.superview === scroll ? view.frame : view.convert(view.bounds, to: scroll)
        return horizontal ? frame.minX : frame.minY
    }
}
#endif
