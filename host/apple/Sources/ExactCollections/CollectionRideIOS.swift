// @ref LLP 1010 §6.12 — a list's rows move as a collection view's batch update
// moves its cells: from where they showed to where they are, on one spring from
// one frame, the port's own change included, when a runner commit moved them
// (`Batch.rides`) or a row left with an exit animation.
#if os(iOS)
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

    /// Whether the list's rows ride this batch: motion is on (not Reduce
    /// Motion, not the agent's clock, not inside an animation the rows
    /// already ride: the keyboard's, a pane's), and a row the port showed when
    /// the batch began is still mounted in it, to move from where it showed.
    /// A window the batch replaced, or one only offscreen or leaving rows are
    /// left of, keeps its correction's own motion (LLP 1010 §6.8). Decided
    /// before the correction lands, which a ride takes at once.
    func canRide(_ id: UInt32) -> Bool {
        guard !DisplayPreferences.reducedMotion, !ExactEnv.agentFreezes, !Presenter.animationMayBeOpen,
              let presenter, presenter.session?.clock == nil, let entry = entries[id], let scroll = presenter.views[id]?.scroll else { return false }
        let horizontal = entry.snapshot.horizontal, port = horizontal ? scroll.bounds.width : scroll.bounds.height
        return entry.snapshot.rows.contains { row in
            guard let showed = entry.batchRows[row.view], let view = presenter.views[row.view], view.isDescendant(of: scroll) else { return false }
            return showed < port && showed + (horizontal ? view.bounds.width : view.bounds.height) > 0
        }
    }

    /// Moves each row the batch shifted on the screen, the leaving ones too,
    /// from where it showed to where it is, additively: rows after a removal
    /// at a followed end stay put and the rows before it come down over the
    /// gap as the leaving row fades; rows above a row that grew at a followed
    /// end go up as the port takes the new end at once. A row the batch
    /// mounted moves with the run of logical positions it joins, from the
    /// nearest row in that run that showed; across a gap of rows not mounted
    /// it takes no move. Runs once the presenter has written its offsets and
    /// scroll snap, from where the rows end up (`canRide` decided it).
    func rideBatch(_ id: UInt32) {
        guard let presenter, let entry = entries[id], let scroll = presenter.views[id]?.scroll else { return }
        let horizontal = entry.snapshot.horizontal, key = "\(NodeView.rideAnimation)\(CACurrentMediaTime())", path = horizontal ? "position.x" : "position.y"
        let offset = horizontal ? scroll.contentOffset.x : scroll.contentOffset.y
        var carried: [UInt32: CGFloat] = [:], seen = Set<UInt32>()
        let ride = { (view: UIView, by: CGFloat) in if abs(by) > 0.5 { view.layer.add(Self.batchMotion(path, from: by), forKey: key) } }
        for row in Array(entry.batchRows.keys) + entry.snapshot.rows.map(\.view) where seen.insert(row).inserted {
            guard let showed = entry.batchRows[row], let view = presenter.views[row] ?? presenter.leaving[row]?.view, view.isDescendant(of: scroll) else { continue }
            let by = showed - (start(of: view, in: scroll, horizontal: horizontal) - offset)
            carried[row] = by
            ride(view, by)
        }
        let run = entry.snapshot.rows.filter { $0.index >= 0 }.sorted { $0.index < $1.index }
        for pass in [run, run.reversed()] {
            var last: (index: Int, by: CGFloat)?
            for row in pass {
                if let by = carried[row.view] { last = (row.index, by); continue }
                guard let joined = last, abs(row.index - joined.index) == 1, let view = presenter.views[row.view] else { last = nil; continue }
                carried[row.view] = joined.by
                last = (row.index, joined.by)
                ride(view, joined.by)
            }
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
        // A row still leaving lies beside the mounted ones; with none left the
        // list does not ride (`canRide`)
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
