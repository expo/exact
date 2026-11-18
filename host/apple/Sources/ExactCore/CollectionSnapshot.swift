import Foundation

package struct CollectionSnapshot {
    package struct Window {
        package let built: [Double], retire: [Double], reveal: [Double], edges: [Double], margin: [Double]
        package let mask: Int
        package let leadSeconds: Double, maxLead: Double
        init?(_ value: BatchValue) {
            guard case .object(let o) = value,
                  let built = o["built"]?.numbers, built.count == 2,
                  let retire = o["retire"]?.numbers, retire.count == 2,
                  let reveal = o["reveal"]?.numbers, reveal.count == 2,
                  let edges = o["edges"]?.numbers, edges.count == 2,
                  let margin = o["margin"]?.numbers, margin.count == 2,
                  let flags = o["mask"]?.number, let mask = Int(exactly: flags), (0...15).contains(mask),
                  let seconds = o["leadSeconds"]?.number, let maximum = o["maxLead"]?.number,
                  (built + retire + reveal + edges + margin + [seconds, maximum]).allSatisfy({ $0.isFinite }),
                  built[0] <= built[1], seconds >= 0, maximum >= 0 else { return nil }
            self.built = built; self.retire = retire; self.reveal = reveal; self.edges = edges; self.margin = margin
            self.mask = mask; leadSeconds = seconds; maxLead = maximum
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
    }
    package struct Fixed {
        package let view: UInt32
        package let roots: [UInt32]
        package let epoch: UInt64
        package let size: Double
    }
    package struct Correction {
        package let sequence: UInt64
        package let offset: Double
        /// An anchor's correction: the offset it was taken at (relative).
        package var from: Double? = nil
        /// Animate there (LLP 1070.000 §6.2): a smooth `scrollIntoView`, or
        /// a `scroll-behavior: smooth` list following its end.
        package var smooth = false
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

}

extension BatchReader {
    package mutating func collectionSnapshots() throws -> [CollectionSnapshot]? {
        whitespace()
        guard byte == 91 else { try skip(); return nil }
        let items = try array { try $0.collectionSnapshot() }
        let snapshots = items.compactMap { $0 }
        guard snapshots.count == items.count, Set(snapshots.map(\.view)).count == snapshots.count else { return nil }
        return snapshots
    }

    private mutating func collectionSnapshot() throws -> CollectionSnapshot? {
        whitespace()
        guard byte == 123 else { try skip(); return nil }
        var view: UInt32?, parent: UInt32?, revision: UInt64?, sequence: UInt64?, extent: Double?
        var rows: [CollectionSnapshot.Row]?, fixed: [CollectionSnapshot.Fixed] = [], correction: CollectionSnapshot.Correction?
        var count: UInt64?, request: UInt64?, lastRequest: UInt64?
        var window: CollectionSnapshot.Window?
        var horizontal = false, pending = false, seeking = false, waiting = false, valid = true
        try object { r, key in
            switch key {
            case "view": view = try r.collectionID()
            case "parent": parent = try r.collectionID()
            case "revision": revision = try r.collectionUInt()
            case "scrollSequence": sequence = try r.collectionUInt()
            case "totalExtent": extent = try r.collectionNumber(positive: true)
            case "axis": horizontal = try r.value().string == "x"
            case "count": count = try r.collectionUInt()
            case "pending": pending = try r.collectionBool()
            case "seeking": seeking = try r.collectionBool()
            case "waiting": waiting = try r.collectionBool()
            case "request": request = try r.collectionUInt()
            case "lastRequest": lastRequest = try r.collectionUInt()
            case "window": window = CollectionSnapshot.Window(try r.value())
            case "rows":
                r.whitespace()
                guard r.byte == 91 else { try r.skip(); rows = nil; return }
                let items = try r.array { try $0.collectionRow() }
                let decoded = items.compactMap { $0 }
                rows = decoded.count == items.count && Set(decoded.map(\.view)).count == decoded.count ? decoded : nil
            case "fixed":
                r.whitespace()
                guard r.byte == 91 else { try r.skip(); valid = false; return }
                let items = try r.array { try $0.collectionFixed() }
                fixed = items.compactMap { $0 }
                valid = valid && items.count == 2 && fixed.count == 2
            case "correction":
                if try r.null() { correction = nil; return }
                correction = try r.collectionCorrection()
                valid = valid && correction != nil
            default: try r.skip()
            }
        }
        guard valid, let view, let revision, let sequence, let extent, let rows,
              fixed.isEmpty || !horizontal,
              Set(rows.map(\.view) + fixed.map(\.view)).count == rows.count + fixed.count else { return nil }
        return CollectionSnapshot(view: view, horizontal: horizontal, parent: parent, revision: revision,
            sequence: sequence, extent: extent, rows: rows, fixed: fixed, correction: correction,
            count: count.map { Int(clamping: $0) } ?? rows.count, pending: pending, seeking: seeking,
            waiting: waiting, request: request ?? 0, lastRequest: lastRequest ?? 0, window: window)
    }

    private mutating func collectionRow() throws -> CollectionSnapshot.Row? {
        whitespace()
        guard byte == 123 else { try skip(); return nil }
        var view: UInt32?, root: UInt32?, epoch: UInt64?, index: UInt64?, size: Double?, measured = false
        try object { r, key in
            switch key {
            case "view": view = try r.collectionID()
            case "root": root = try r.collectionID()
            case "epoch": epoch = try r.collectionUInt()
            case "index": index = try r.collectionUInt()
            case "size": size = try r.collectionNumber(positive: true)
            case "measured": measured = try r.collectionBool()
            default: try r.skip()
            }
        }
        guard let view, let root, let epoch else { return nil }
        return .init(view: view, root: root, epoch: epoch, index: index.map { Int(clamping: $0) } ?? -1, size: size, measured: measured)
    }

    private mutating func collectionFixed() throws -> CollectionSnapshot.Fixed? {
        whitespace()
        guard byte == 123 else { try skip(); return nil }
        var view: UInt32?, epoch: UInt64?, size: Double?, roots: [UInt32]?
        try object { r, key in
            switch key {
            case "view": view = try r.collectionID()
            case "epoch": epoch = try r.collectionUInt()
            case "size": size = try r.collectionNumber(positive: true)
            case "roots":
                r.whitespace()
                guard r.byte == 91 else { try r.skip(); return }
                let items = try r.array { try $0.collectionID() }
                let ids = items.compactMap { $0 }
                roots = items.count == ids.count ? ids : nil
            default: try r.skip()
            }
        }
        guard let view, let epoch, epoch == 1, let size, let roots else { return nil }
        return .init(view: view, roots: roots, epoch: epoch, size: size)
    }

    private mutating func collectionCorrection() throws -> CollectionSnapshot.Correction? {
        whitespace()
        guard byte == 123 else { try skip(); return nil }
        var sequence: UInt64?, offset: Double?, from: Double?, smooth = false
        try object { r, key in
            switch key {
            case "scrollSequence": sequence = try r.collectionUInt()
            case "offset": offset = try r.collectionNumber()
            case "from": from = try r.collectionNumber()
            case "smooth": smooth = try r.collectionBool()
            default: try r.skip()
            }
        }
        guard let sequence, let offset else { return nil }
        return .init(sequence: sequence, offset: offset, from: from, smooth: smooth)
    }

    private mutating func collectionUInt() throws -> UInt64? {
        whitespace()
        if byte == 34 { return UInt64(try string()) }
        guard byte == 45 || (48...57).contains(byte) else { try skip(); return nil }
        let span = try numberSpan()
        if let integer = UInt64(String(decoding: bytes[span], as: UTF8.self)) { return integer }
        return UInt64(exactly: try number(span))
    }

    private mutating func collectionID() throws -> UInt32? {
        guard let integer = try collectionUInt(), integer > 0 else { return nil }
        return UInt32(exactly: integer)
    }

    private mutating func collectionNumber(positive: Bool = false) throws -> Double? {
        whitespace()
        guard byte == 45 || (48...57).contains(byte) else { try skip(); return nil }
        let value = try number()
        guard abs(value) <= Double(Float.greatestFiniteMagnitude), !positive || value >= 0 else { return nil }
        return value
    }

    private mutating func collectionBool() throws -> Bool {
        whitespace()
        if byte == 116 || byte == 102 { return try bool() }
        if byte == 45 || (48...57).contains(byte) { return try number() != 0 }
        try skip()
        return false
    }
}
