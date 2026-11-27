import Foundation

/// A windowed list's state as a batch's `collections` op carries it, which
/// the core reads through `ListHosting.snapshot`. The lists family decodes it
/// (`CoreExports.collectionSnapshots`); without the family a batch's items
/// are skipped.
package struct CollectionSnapshot {
    package struct Window {
        package let built: [Double], retire: [Double], reveal: [Double], edges: [Double], margin: [Double]
        package let mask: Int
        package let leadSeconds: Double, maxLead: Double
        package init(built: [Double], retire: [Double], reveal: [Double], edges: [Double], margin: [Double],
                     mask: Int, leadSeconds: Double, maxLead: Double) {
            self.built = built; self.retire = retire; self.reveal = reveal; self.edges = edges; self.margin = margin
            self.mask = mask; self.leadSeconds = leadSeconds; self.maxLead = maxLead
        }
        package func owesEdge(offset: Double, port: Double, velocity: Double) -> Bool {
            let extra = min(abs(velocity) * leadSeconds, maxLead)
            let reached = [max(0, offset) - margin[0] - (velocity < 0 ? extra : 0) < edges[0],
                max(0, offset) + port + margin[1] + (velocity > 0 ? extra : 0) > edges[1]]
            return (0..<2).contains { mask & (1 << $0) != 0 && reached[$0] == (mask & (4 << $0) != 0) }
        }
        package func owesReveal(offset: Double, port: Double, extent: Double) -> Bool {
            max(0, offset) < reveal[0] || min(extent, max(0, offset) + port) > reveal[1]
        }
        package func owesRetirement(offset: Double, port: Double, moving: Bool) -> Bool {
            let bound = port + (moving ? maxLead : 0)
            return retire[0] <= max(0, offset) - bound || retire[1] >= max(0, offset) + port + bound
        }
    }
    package struct Row {
        package let view: UInt32
        package let root: UInt32
        package let epoch: UInt64
        /// Logical position; -1 when a snapshot omits it.
        package let index: Int
        package let size: Double?
        package let measured: Bool
        package init(view: UInt32, root: UInt32, epoch: UInt64, index: Int, size: Double?, measured: Bool) {
            self.view = view; self.root = root; self.epoch = epoch; self.index = index; self.size = size; self.measured = measured
        }
    }
    package struct Fixed {
        package let view: UInt32
        package let roots: [UInt32]
        package let epoch: UInt64
        package let size: Double
        package init(view: UInt32, roots: [UInt32], epoch: UInt64, size: Double) {
            self.view = view; self.roots = roots; self.epoch = epoch; self.size = size
        }
    }
    package struct Correction {
        package let sequence: UInt64
        package let offset: Double
        /// An anchor's correction: the offset it was taken at (relative).
        package var from: Double? = nil
        /// Animate there (LLP 1070.000 §6.2): a smooth `scrollIntoView`, or
        /// a `scroll-behavior: smooth` list following its end.
        package var smooth = false
        package init(sequence: UInt64, offset: Double, from: Double? = nil, smooth: Bool = false) {
            self.sequence = sequence; self.offset = offset; self.from = from; self.smooth = smooth
        }
    }
    package let view: UInt32
    /// The main axis (LLP 1070 H1): a row list scrolls on x, and `extent`,
    /// a correction's offset and every measured size run along it.
    package let horizontal: Bool
    /// The outer list whose mounted row holds this one (LLP 1070 N1).
    package let parent: UInt32?
    package let revision: UInt64
    package let sequence: UInt64
    package let extent: Double
    package let rows: [Row]
    package let fixed: [Fixed]
    package let correction: Correction?
    /// Logical item count, mounted or not.
    package let count: Int
    /// A limited report left window rows unbuilt: another report is owed.
    package let pending: Bool
    /// A `scrollIntoView` under way (LLP 1070.000): its corrections are the
    /// app's, applied whatever the reader scrolled since, until the reader's
    /// gesture. `waiting`: it waits for the reader's scroll to end (§2.6).
    package let seeking: Bool
    package let waiting: Bool
    /// Which request that is: a gesture counts against the one it began after.
    package let request: UInt64
    /// The latest request the list took, under way or ended (0 for none). A
    /// host compares it before and after an input to tell whether a handler
    /// asked for a row in answer.
    package let lastRequest: UInt64
    package var underWay: Bool { seeking || waiting }
    package var window: Window? = nil
    /// It has shown its settled first rows: a change to it may move them (LLP 1010 §6.12).
    package var live = false
    package init(view: UInt32, horizontal: Bool, parent: UInt32?, revision: UInt64, sequence: UInt64, extent: Double,
                 rows: [Row], fixed: [Fixed], correction: Correction?, count: Int, pending: Bool, seeking: Bool,
                 waiting: Bool, request: UInt64, lastRequest: UInt64, window: Window? = nil) {
        self.view = view; self.horizontal = horizontal; self.parent = parent; self.revision = revision
        self.sequence = sequence; self.extent = extent; self.rows = rows; self.fixed = fixed
        self.correction = correction; self.count = count; self.pending = pending; self.seeking = seeking
        self.waiting = waiting; self.request = request; self.lastRequest = lastRequest; self.window = window
    }
}

extension BatchReader {
    /// A batch's `collections` items as the lists family decodes them
    /// (`CoreExports.collectionSnapshots`); without the family, skipped.
    package mutating func linkedCollectionSnapshots() throws -> [CollectionSnapshot]? {
        guard let decode = CoreExports.collectionSnapshots else { try skip(); return nil }
        return try decode(&self)
    }
}
