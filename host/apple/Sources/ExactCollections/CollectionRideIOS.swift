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
            // A row the batch mounted moves as one that showed: a kept row out of the port says nothing of the screen
            if entry.batchShown.contains(row) { moves.append((at, showed - at)) }
            if abs(showed - at) > 0.5 { view.layer.add(Self.batchMotion(path, from: showed - at), forKey: key) }
        }
        for (view, at) in mounted {
            guard let nearest = moves.min(by: { abs($0.at - at) < abs($1.at - at) }), abs(nearest.by) > 0.5 else { continue }
            view.layer.add(Self.batchMotion(path, from: nearest.by), forKey: key)
        }
    }

    /// Whether a row that showed in the port as the batch began is still in the list's scroll view, mounted or
    /// leaving on its exit: the place a batch's rows ride from (`rideBatch`), as LLP 1010 §6.12 says.
    func ridesFrom(_ entry: Entry, in id: UInt32) -> Bool {
        guard let presenter, let scroll = presenter.views[id]?.scroll else { return false }
        return entry.batchShown.contains { row in
            (presenter.views[row] ?? presenter.leaving[row]?.view)?.isDescendant(of: scroll) == true
        }
    }

    /// Where each of a list's mounted rows, and each row still leaving beside them, shows along its axis (its start
    /// in the scroll view less the offset), and which of them show in its port (in a window, nothing from it up hidden
    /// or clear, a positive extent overlapping the scroll view's bounds): one capture over one set of rows, so a row the batch may ride from is one whose place it
    /// knows, never a view inside a row or a fixed item.
    func rowsAtBegin(_ id: UInt32) -> (starts: [UInt32: CGFloat], shown: Set<UInt32>) {
        guard let presenter, let entry = entries[id], let scroll = presenter.views[id]?.scroll else { return ([:], []) }
        let horizontal = entry.snapshot.horizontal, offset = horizontal ? scroll.contentOffset.x : scroll.contentOffset.y
        let visible = scroll.bounds
        var starts: [UInt32: CGFloat] = [:], shown = Set<UInt32>(), parent: UIView?
        func take(_ row: UInt32, _ view: UIView) {
            starts[row] = start(of: view, in: scroll, horizontal: horizontal) - offset
            let frame = view.superview === scroll ? view.frame : view.convert(view.bounds, to: scroll)
            let overlaps = horizontal ? frame.maxX > visible.minX && frame.minX < visible.maxX : frame.maxY > visible.minY && frame.minY < visible.maxY
            // Shown only in a window, it and every view above it unhidden and not clear, as the port's own check reads
            let drawn = view.window != nil && sequence(first: view, next: \.superview).allSatisfy { !$0.isHidden && $0.alpha > 0.01 }
            if overlaps, (horizontal ? frame.width : frame.height) > 0, drawn { shown.insert(row) }
        }
        for row in entry.snapshot.rows {
            guard let view = presenter.views[row.view] else { continue }
            take(row.view, view)
            parent = view.superview
        }
        // A row still leaving lies beside the mounted ones: one from an earlier batch counts while a mounted row does
        for (row, leaving) in presenter.leaving where parent != nil && leaving.view.superview === parent { take(row, leaving.view) }
        return (starts, shown)
    }

    /// A row's start in the scroll view along the list's axis, where its
    /// layout put it (its layer's model, not a ride under way).
    private func start(of view: UIView, in scroll: UIScrollView, horizontal: Bool) -> CGFloat {
        let frame = view.superview === scroll ? view.frame : view.convert(view.bounds, to: scroll)
        return horizontal ? frame.minX : frame.minY
    }
}
#endif
