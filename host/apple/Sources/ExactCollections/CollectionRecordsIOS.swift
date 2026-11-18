#if os(iOS) || os(tvOS)
import UIKit
import ExactKit
import ExactCore

/// Geometry belongs to a mounted row epoch. Reports keep these records until
/// a batch changes the row; acknowledgement belongs to the runner's snapshot.
final class CollectionRecords {
    struct Row {
        var epoch: UInt64
        var measurement: CollectionMeasurement?
        var cross: Double?
        var span: ClosedRange<CGFloat>?
        var dirty = true
        var spanDirty = true
        var pending = true
    }
    struct Run {
        var start: CGFloat, end: CGFloat
        var first: Int, last: Int
    }
    struct Coverage {
        var start: CGFloat, end: CGFloat
        var first: Bool, last: Bool
        var runs: [ClosedRange<CGFloat>]

        func covers(_ port: ClosedRange<CGFloat>) -> Bool {
            let start = first ? max(port.lowerBound, self.start) : port.lowerBound
            let end = last ? min(port.upperBound, self.end) : port.upperBound
            return start >= end || runs.contains { $0.lowerBound <= start + 0.5 && $0.upperBound >= end }
        }
    }
    struct Port: Equatable {
        var bounds: CGRect
        var insets: UIEdgeInsets
        var band: UIEdgeInsets
    }
    var rows: [UInt32: Row] = [:]
    var measurements: [CollectionMeasurement] = []
    var unacknowledged: [CollectionMeasurement] = []
    var eligible: [CollectionMeasurement] = []
    var eligibleUnacknowledged: [CollectionMeasurement] = []
    var measurementCross: Double?
    var runs: [Run] = []
    var spans: [(index: Int, span: ClosedRange<CGFloat>?)] = []
    var regular = true
    var mean: CGFloat = 0
    /// Geometric order determines coverage and padding, independently of row order.
    var coverage: Coverage?
    var origin: GridOrigin?
    var port: Port?
    var facts: CollectionFacts?
    var content = CGRect.zero
    var changed = true
    private var hasPending = false
    private var fixedPending = false
    private var portPending = false
    private(set) var measuredRows = 0
    private(set) var spanReads = 0

    func update(_ snapshot: CollectionSnapshot, entry: CollectionHost.Entry) {
        var kept: [UInt32: Row] = [:]
        for row in snapshot.rows {
            if let old = rows[row.view], old.epoch == row.epoch, snapshot.horizontal == entry.snapshot.horizontal { kept[row.view] = old }
            else { kept[row.view] = Row(epoch: row.epoch); hasPending = true }
            if rows[row.view] == nil { entry.rowsAdded += 1 }
            if snapshot.revision > entry.snapshot.revision,
               let m = rows[row.view]?.measurement, m.epoch == row.epoch,
               !CollectionHost.acknowledges(row, m) { entry.owed = true }
        }
        rows = kept
        fixedPending = !snapshot.fixed.isEmpty
        changed = true
    }

    func invalidateFixedCoverage() {
        fixedPending = true
        changed = true
        port = nil
    }

    func invalidate(_ id: UInt32, measurement: Bool = true, pending: Bool = true) {
        if rows[id] != nil {
            if measurement { rows[id]?.dirty = true }
            rows[id]?.spanDirty = true
            if pending { rows[id]?.pending = true; hasPending = true }
            changed = true
        }
        port = nil
    }

    func invalidatePort() {
        port = nil
        portPending = true
    }

    /// Reads during application cannot settle row or port geometry before
    /// the outer batch has applied all its ops.
    func endBatch() {
        if portPending { port = nil; portPending = false }
        fixedPending = false
        guard hasPending else { return }
        for id in rows.keys { rows[id]?.pending = false }
        hasPending = false
    }

    func refresh(_ snapshot: CollectionSnapshot, presenter: Presenter, scroll: UIScrollView) {
        guard changed else { return }
        measurementCross = nil
        measurements.removeAll(keepingCapacity: true)
        unacknowledged.removeAll(keepingCapacity: true)
        runs.removeAll(keepingCapacity: true)
        spans.removeAll(keepingCapacity: true)
        regular = true
        var previous: CGFloat?
        var incomplete = false
        var total: CGFloat = 0, count = 0
        for row in snapshot.rows {
            guard var record = rows[row.view] else { continue }
            if record.dirty {
                record.measurement = nil; record.cross = nil
                if let node = presenter.views[row.view] {
                    let bounds = node.layoutBounds
                    let size = Double(snapshot.horizontal ? bounds.width : bounds.height)
                    record.cross = Double(snapshot.horizontal ? bounds.height : bounds.width)
                    measuredRows += 1
                    var ancestor: UIView? = node
                    var hidden = false
                    // The list and its ancestors are checked once by geometry.
                    while let current = ancestor, current !== presenter.views[snapshot.view] {
                        if current.isHidden { hidden = true; break }
                        ancestor = current.superview
                    }
                    if !hidden, size.isFinite, size >= 0 {
                        record.measurement = CollectionMeasurement(view: row.view, epoch: row.epoch, size: size)
                    }
                }
                record.dirty = record.pending || presenter.views[row.view] == nil
                incomplete = incomplete || record.dirty
            }
            if record.spanDirty {
                record.span = nil
                if let node = presenter.views[row.view] {
                    let frame = node.superview === scroll ? node.frame : node.convert(node.bounds, to: scroll)
                    record.span = snapshot.horizontal ? frame.minX...frame.maxX : frame.minY...frame.maxY
                    spanReads += 1
                }
                record.spanDirty = record.pending || presenter.views[row.view] == nil
                incomplete = incomplete || record.spanDirty
            }
            rows[row.view] = record
            if let m = record.measurement {
                measurements.append(m)
                if !CollectionHost.acknowledges(row, m) { unacknowledged.append(m) }
            }
            spans.append((row.index, record.span))
            guard let span = record.span else { regular = false; continue }
            if let previous, span.lowerBound < previous { regular = false }
            previous = span.upperBound
            total += span.upperBound - span.lowerBound; count += 1
            if let last = runs.last, span.lowerBound <= last.end + 0.5 {
                runs[runs.count - 1].end = max(last.end, span.upperBound)
                runs[runs.count - 1].last = row.index
            } else {
                runs.append(Run(start: span.lowerBound, end: span.upperBound, first: row.index, last: row.index))
            }
        }
        mean = count > 0 ? total / CGFloat(count) : 0
        var frames = snapshot.rows.compactMap { row -> (first: Bool, last: Bool, range: ClosedRange<CGFloat>)? in
            guard let span = rows[row.view]?.span else { return nil }
            return (row.index == 0, row.index == snapshot.count - 1, span)
        }
        if !snapshot.fixed.isEmpty {
            let origin = presenter.views[snapshot.view]?.contentBox().minY ?? 0
            frames += presenter.collections.fixedSpans(snapshot.view).map {
                (first: $0.first, last: $0.last, range: (origin + CGFloat($0.start))...(origin + CGFloat($0.end)))
            }
        }
        frames.sort { $0.range.lowerBound < $1.range.lowerBound }
        coverage = nil
        if let head = frames.first, let tail = frames.last {
            var cover = Coverage(start: head.range.lowerBound, end: tail.range.upperBound,
                first: head.first, last: tail.last, runs: [])
            for frame in frames {
                if let last = cover.runs.last, frame.range.lowerBound <= last.upperBound + 0.5 {
                    cover.runs[cover.runs.count - 1] = last.lowerBound...max(last.upperBound, frame.range.upperBound)
                } else { cover.runs.append(frame.range) }
            }
            coverage = cover
        }
        changed = incomplete || fixedPending
        port = nil
    }
    func reach(from edge: CGFloat, forward: Bool) -> (edge: CGFloat, index: Int?) {
        if regular {
            let run = forward ? runs.first { $0.start <= edge + 0.5 && $0.end >= edge }
                : runs.last { $0.end >= edge - 0.5 && $0.start <= edge }
            return (forward ? run?.end ?? edge : run?.start ?? edge, forward ? run?.last : run?.first)
        }
        var reached = edge, index: Int?
        if forward {
            for row in spans {
                guard let span = row.span, span.lowerBound <= reached + 0.5, span.upperBound >= reached else { continue }
                reached = span.upperBound; index = row.index
            }
        } else {
            for row in spans.reversed() {
                guard let span = row.span, span.upperBound >= reached - 0.5, span.lowerBound <= reached else { continue }
                reached = span.lowerBound; index = row.index
            }
        }
        return (reached, index)
    }

}

extension CollectionHost {
    /// Frames carry intrinsic-size changes too. Structural changes can move a
    /// wrapper through a native container, so its converted span is read again.
    func invalidateRecords(_ batch: Batch) {
        for op in batch.ops {
            if let entry = entries[op.id], !entry.snapshot.fixed.isEmpty {
                entry.records.invalidateFixedCoverage()
            }
            switch op.op {
            case .frame:
                for (id, entry) in entries {
                    if let row = entry.records.rows[op.id] {
                        let main = entry.snapshot.horizontal ? op.w : op.h
                        let cross = entry.snapshot.horizontal ? op.h : op.w
                        let resized = row.measurement?.size != main || row.cross != cross
                        entry.records.invalidate(op.id, measurement: resized)
                    }
                    if id == op.id { entry.records.invalidatePort() }
                }
            case .create, .style, .props, .present, .renew, .destroy:
                for (id, entry) in entries {
                    if entry.records.rows[op.id] != nil { entry.records.invalidate(op.id) }
                    if id == op.id { entry.records.invalidatePort() }
                }
            case .children, .roots:
                for (id, entry) in entries {
                    if op.op == .roots || op.id == id {
                        if !entry.snapshot.fixed.isEmpty { entry.records.invalidateFixedCoverage() }
                        for row in entry.records.rows.keys { entry.records.invalidate(row, measurement: false) }
                    } else if entry.records.rows[op.id] != nil {
                        entry.records.invalidate(op.id)
                    }
                }
            case .content, .fieldContent:
                entries[op.id]?.records.invalidatePort()
            default: break
            }
        }
    }

    func rowRecords(_ id: UInt32) -> CollectionRecords? {
        guard let entry = entries[id], let presenter, let node = presenter.views[id], let scroll = node.scroll else { return nil }
        if entry.records.origin != node.grid?.origin {
            for row in entry.records.rows.keys { entry.records.invalidate(row, measurement: false, pending: false) }
            entry.records.origin = node.grid?.origin
        }
        entry.records.refresh(entry.snapshot, presenter: presenter, scroll: scroll)
        return entry.records
    }

    func measurements(_ id: UInt32, cross: Double) -> [CollectionMeasurement] {
        guard let records = rowRecords(id) else { return [] }
        if records.measurementCross != cross {
            records.eligible = records.measurements.filter { records.rows[$0.view]?.cross == cross }
            records.eligibleUnacknowledged = records.unacknowledged.filter { records.rows[$0.view]?.cross == cross }
            records.measurementCross = cross
        }
        return records.eligible
    }
}
#endif
