// Host metadata and LE feedback for runner/src/instance/collection/api.rs.
// No record data, keys, height index, or row lifetime policy lives in Swift.
import Foundation
import ExactKit
import ExactCore

struct CollectionMeasurement: Equatable {
    var view: UInt32
    var epoch: UInt64
    /// The wrapper's border-box size on the list's main axis.
    var size: Double
}

/// What a list's report says of the reader and of the host's own motion,
/// flag bits 5 to 7: the reader's scroll under way, a gesture since the
/// list's `scrollIntoView` (LLP 1070.000 §2.6), and the host's own smooth
/// correction running (§11).
struct Reading: Equatable {
    var scrolling = false, gesture = false, animating = false
}

/// A list's port on its own axes (LLP 1070 H1): `offset` along the main
/// axis, the port's main and cross sizes, and the rows' cross size.
package struct CollectionFacts: Equatable {
    package var offset: Double
    var portMain: Double
    var portCross: Double
    var cross: Double
    var measurements: [CollectionMeasurement]
    var focus: UInt32?
    var interaction: UInt32?
    /// The main-axis padding before the rows and after them, as laid out:
    /// the room `offset` counts past (LLP 1010 §6.9).
    var paddingStart = 0.0
    var paddingEnd = 0.0
    #if DEBUG
    static var encodedReports = 0
    #endif

    /// Wire version 4: `velocity` (points/s, positive toward the end),
    /// `limit`, the rows past what it owes this report may build (nil: any),
    /// and whether an enclosing list is moving (LLP 1070 F2).
    /// `createOnly` and `noBuild` split a moving list's report around the
    /// owner thread (LLP 1072 §5): build only, off main; then retire only.
    /// `glimpse`: `limit` bounds the rows of what shows it builds too.
    /// `reading`: flag bits 5 to 7 (`Reading`).
    func encode(view: UInt32, revision: UInt64, sequence: UInt64, velocity: Double = 0, limit: UInt32? = nil, ancestorMoving: Bool = false, createOnly: Bool = false, noBuild: Bool = false, glimpse: Bool = false, reading: Reading = Reading()) -> Data {
        #if DEBUG
        Self.encodedReports += 1
        #endif
        var bytes = Data(count: 100 + measurements.count * 20)
        bytes.withUnsafeMutableBytes { buffer in
            var cursor = 0
            func integer<T: FixedWidthInteger>(_ value: T) {
                var le = value.littleEndian
                withUnsafeBytes(of: &le) {
                    buffer.baseAddress!.advanced(by: cursor).copyMemory(from: $0.baseAddress!, byteCount: $0.count)
                    cursor += $0.count
                }
            }
            integer(UInt32(4)); integer(view); integer(revision); integer(sequence)
            for value in [offset, portMain, portCross, cross, paddingStart, paddingEnd] { integer(value.bitPattern) }
            integer(focus ?? 0); integer(interaction ?? 0)
            integer((velocity.isFinite ? velocity : 0).bitPattern); integer(limit.map { Swift.min($0, UInt32.max - 1) } ?? UInt32.max)
            integer(UInt32(ancestorMoving ? 1 : 0) | UInt32(createOnly ? 2 : 0) | UInt32(noBuild ? 4 : 0) | UInt32(glimpse ? 16 : 0)
                | UInt32(reading.scrolling ? 32 : 0) | UInt32(reading.gesture ? 64 : 0) | UInt32(reading.animating ? 128 : 0))
            integer(UInt32(measurements.count))
            for row in measurements { integer(row.view); integer(row.epoch); integer(row.size.bitPattern) }
        }
        return bytes
    }
}

struct CollectionCursor {
    private(set) var sequence: UInt64 = 0
    private var correctedRevision: UInt64?
    mutating func advance() { if sequence < UInt64.max { sequence += 1 } }
    /// The sequence of the last move that was not the reader's travel (an
    /// authored offset, a port resize): a relative correction from before
    /// it is stale, as an absolute one is.
    private(set) var jumpedAt: UInt64 = 0
    mutating func jump() { advance(); jumpedAt = sequence }
    /// `authored`: the correction is a `scrollIntoView`'s, which applies whatever the reader scrolled since.
    mutating func takeCorrection(revision: UInt64, sequence: UInt64, authored: Bool = false) -> Bool {
        guard authored || sequence == self.sequence, correctedRevision.map({ revision > $0 }) ?? true else { return false }
        correctedRevision = revision
        return true
    }
    /// The correction an anchor's (`from`) still owes, once per revision:
    /// a relative move, whatever the port did since. A later revision
    /// carrying the same anchor's correction owes only what it adds.
    private var shifted: (sequence: UInt64, from: Double, offset: Double)?
    /// `jumpedBefore`: where `jumpedAt` was before a port resize in this
    /// same batch; a correction planned since then still lands.
    mutating func takeShift(revision: UInt64, _ c: CollectionSnapshot.Correction, jumpedBefore: UInt64? = nil) -> Double? {
        guard let from = c.from, c.sequence >= (jumpedBefore ?? jumpedAt),
              correctedRevision.map({ revision > $0 }) ?? true else { return nil }
        correctedRevision = revision
        let done = shifted.flatMap { $0.sequence == c.sequence && $0.from == from ? $0.offset : nil } ?? from
        shifted = (c.sequence, from, c.offset)
        return c.offset - done
    }
}

struct CollectionTurnBudget {
    private var passes = 0
    private var busy = false
    /// Passes refused, by why: the turn had spent its two (`spent`), or a
    /// pass was running and a report's batch reentered (`busy`).
    private(set) var refusedSpent = 0, refusedBusy = 0
    /// A rescue (what shows is uncovered, LLP 1050.000 D1) is never refused
    /// for the turn's passes: a turn that spent them on other lists, or on
    /// a slice, would otherwise leave the port blank until the next slice.
    mutating func begin(rescue: Bool = false) -> Bool {
        if busy { refusedBusy += 1; return false }
        if passes >= 2 && !rescue { refusedSpent += 1; return false }
        passes += 1; busy = true
        return true
    }
    /// A pass that sent no report (an unchanged list, visited because a
    /// batch dirtied every list) is given back: with nested lists most
    /// visits are those, and they would spend the pass a slice needs.
    mutating func end(reported: Bool = true) {
        busy = false
        if !reported && passes > 0 { passes -= 1 }
    }
    mutating func nextTurn() { passes = 0 }
}

/// At most one pending callback and two feedback calls per main-queue turn.
/// All caches are proportional to active collections and their mounted wrappers.
final class CollectionHost: ListHosting {
    final class Entry {
        var snapshot: CollectionSnapshot
        var rowsAdded: UInt64 = 0
        func added(since before: UInt64) -> Int {
            rowsAdded >= before ? Int(clamping: rowsAdded - before) : 0
        }
        #if os(iOS) || os(tvOS)
        let records = CollectionRecords()
        #endif
        var cursor = CollectionCursor()
        var lastFacts: CollectionFacts?
        var lastSequence: UInt64?
        /// The limit the last report carried: a slice that may build more
        /// reports again with the same facts (a rescue just reported them,
        /// building only what shows).
        var lastLimit: UInt32?
        /// A report the runner turned away (a revision it had moved past) carried measurements its newer snapshot
        /// doesn't acknowledge: they're owed again, whether or not the port moves (`update`).
        var owed = false
        #if os(iOS) || os(tvOS)
        var travelled = false
        #endif
        var port: [Double]?
        /// The offset when the batch began: where the reader is. Rows that
        /// leave above a deep offset shrink the document first, and the
        /// platform's clamp to it is not a scroll (`endBatch`'s shift).
        var batchStart: Double?
        /// The platform's insets of the port when the batch began
        /// (`portInsets`): one that changed under it moved the port.
        var batchInsets: [Double]?
        /// What the platform covers of the port at its start and end, as the runner was last told.
        var covered: [Double] = [0, 0]
        /// The reader began a gesture on the port since the host learnt of
        /// the request under way (`snapshot.request`).
        var gesture = false
        /// When the host learnt of that request, on the events' clock
        /// (seconds since startup): an input that began before it is not
        /// counted against it (`gestureBegan`).
        var requestAt = ProcessInfo.processInfo.systemUptime
        /// What the last report said of the reader and the host's motion.
        var lastReading: Reading?
        #if os(macOS)
        /// When the reader last moved the port outside AppKit's live scroll
        /// (a wheel's notch, a key), and whether a check of its end is due.
        var movedAt = -Double.infinity
        var quietDue = false
        #endif
        /// The request's corrections are the app's: the reader has made no
        /// gesture since it was made (the web glue's `authored`).
        var authored: Bool { snapshot.seeking && !gesture }
        init(_ snapshot: CollectionSnapshot) { self.snapshot = snapshot }
    }
    weak var presenter: Presenter?
    var onFeedback: ((Data) -> Void)?
    /// Whether the runner's snapshot row has taken measurement `m`: measured at that epoch, its height within the
    /// runner's noise of the size and on the same side of zero (runner `SizeIndex::noise_at`, `MEASURE_NOISE`).
    static func acknowledges(_ row: CollectionSnapshot.Row, _ m: CollectionMeasurement) -> Bool {
        guard row.measured, row.epoch == m.epoch, let height = row.size else { return false }
        return (height == 0) == (m.size == 0) && abs(height - m.size) <= 0.01
    }
    var entries: [UInt32: Entry] = [:]
    var interaction: UInt32?
    var contactEvent: AnyObject?
    var batchDepth = 0
    var correcting = false
    private var dirty = Set<UInt32>()
    private(set) var budget = CollectionTurnBudget()
    #if DEBUG
    private(set) var flushes = 0, slices = 0
    var geometryReads = 0
    #endif
    /// Lists whose uncovered port a rescue reports first, whatever the
    /// turn has spent (`CollectionTurnBudget.begin(rescue:)`).
    private var rescuing = Set<UInt32>()
    /// Each turn of the main run loop gets its passes: a continuation
    /// (`schedule`) or a slice is not the only start of a turn.
    private var turnObserver: CFRunLoopObserver?
    private var queued = false
    private var generation = 0
    private var contactSequence: UInt64 = 0
    private var gestureContact: UInt64?
    private var lastVisited: UInt32 = 0
    private var refreshPins = false
    private(set) var resting = Set<UInt32>()
    #if os(iOS) || os(tvOS)
    /// `focusedView()` walks every node. UIKit's responder changes all
    /// reach `pinsChanged` (a node's become and resign, an input's begin and
    /// end editing), which forgets it.
    private var focusFound: UInt32??
    #else
    /// The phase of the last scroll event of a trackpad gesture, and when its
    /// contact began: a `began` after a `mayBegin` is the same contact.
    var wheelPhase: UInt = 0 // `NSEvent.Phase`'s raw value
    var contactAt: TimeInterval = 0
    #endif
    /// LLP 1050.000's fill. A platform with a pump reports each moving
    /// collection's velocity (nil at rest) and builds `fillPending` in slices;
    /// one without leaves `motion` nil, and every report is unlimited.
    var motion: ((UInt32) -> Double?)?
    /// The reader's scroll is under way on `id` (LLP 1070.000 §2.6): a
    /// `scrollIntoView` made meanwhile waits for it. On AppKit, whose jump
    /// would not end it: the live scroll and the motion after it. Never on
    /// UIKit, whose own calls land at once, under a finger too (the drag goes
    /// on from there), nor under the agent's frozen clock.
    func readerScrolling(_ id: UInt32) -> Bool {
        #if os(macOS)
        guard !ExactEnv.agentFreezes, let scroll = presenter?.views[id]?.scroll, let entry = entries[id] else { return false }
        return KnobDrag.of(scroll)?.live == true || entry.quietDue
        #else
        return false
        #endif
    }
    #if os(macOS)
    /// After the reader's last move outside a live scroll, its scroll is
    /// taken to have ended, as the web glue's `QUIET` after its last `scroll`.
    static let quiet = 0.3
    /// The reader moved `id`'s port outside a live scroll (a wheel's notch, a
    /// key's step, momentum AppKit carries on past a live scroll's end: any
    /// move the host did not make): its scroll is under way until `quiet`
    /// after the last, and the report that follows says when it ended. A live
    /// scroll ends with its own notification.
    private func readerMoved(_ id: UInt32) {
        guard let entry = entries[id], let scroll = presenter?.views[id]?.scroll, KnobDrag.of(scroll)?.live != true else { return }
        entry.movedAt = ProcessInfo.processInfo.systemUptime
        if !entry.quietDue { entry.quietDue = true; checkQuiet(id, after: Self.quiet) }
    }
    private func checkQuiet(_ id: UInt32, after delay: Double) {
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self, let entry = entries[id], entry.quietDue else { return }
            let left = entry.movedAt + Self.quiet - ProcessInfo.processInfo.systemUptime
            if left > 0 { checkQuiet(id, after: left) } else { entry.quietDue = false; changed(id) }
        }
    }
    #endif
    /// Lists the host is animating to a smooth correction: their offset
    /// moves by the platform's scroll animation, not the reader, so the fill
    /// reports no travel for them.
    ///
    /// While one runs the list reports where it is headed, and its scroll
    /// ticks are not the reader's travel. A report from mid-way told the
    /// runner the reader had left the end, and each tick advanced the
    /// scroll sequence past the corrections still to come: a message sent
    /// while the port's own follow ran was never followed.
    var animating = Set<UInt32>()
    /// Where each running animation is headed, and a later target taken
    /// when it lands (a correction, or a relative shift, that arrived
    /// while it ran).
    var animationTargets: [UInt32: CGPoint] = [:]
    var owedTargets: [UInt32: CGPoint] = [:]
    /// iOS: lists whose smooth correction begins on the next turn, the
    /// animation each was scheduled for, and the drivers running.
    var startOwed = Set<UInt32>()
    var pendingSerial: [UInt32: Int] = [:]
    var pendingToken: [UInt32: Int] = [:]
    var nextPendingToken = 0
    #if os(iOS) || os(tvOS)
    var offsetDrivers: [UInt32: OffsetDriver] = [:]
    #endif
    /// The running animation's number, for each animating list only: a
    /// callback for one that has since been stopped, or replaced, is not
    /// this one's.
    private(set) var animationSerial: [UInt32: Int] = [:]
    private var lastAnimation = 0
    @discardableResult
    func beginAnimation(_ view: UInt32, to target: CGPoint) -> Int {
        animating.insert(view)
        animationTargets[view] = target
        lastAnimation += 1
        animationSerial[view] = lastAnimation
        animationMoved.remove(view)
        return lastAnimation
    }
    /// An animation stops: by a drag, an ordinary correction, or the list's
    /// retirement. What it owed goes with it.
    /// A list whose port changed outside a report reports again.
    func reportAgain(_ view: UInt32) { dirty.insert(view); schedule() }
    func stopAnimation(_ view: UInt32) {
        startOwed.remove(view); pendingSerial[view] = nil; pendingToken[view] = nil
        #if os(iOS) || os(tvOS)
        offsetDrivers.removeValue(forKey: view)?.cancel()
        presenter?.forgetTravel(view)
        #else
        haltClip(view)
        #endif
        animating.remove(view)
        animationTargets[view] = nil; owedTargets[view] = nil; animationSerial[view] = nil
        animationMoved.remove(view)
    }
    /// Animating lists whose port has moved since their animation began.
    private(set) var animationMoved = Set<UInt32>()
    /// `atTarget`: whether the platform's end is at the running animation's
    /// target, clamped to the content as it is now (UIKit's delegate). An end
    /// elsewhere before this animation moved the port is a stopped one's; one
    /// after it moved is this one's, stopped short (a snap, a clamp).
    func animationEnded(_ view: UInt32, dragging: Bool = false, atTarget: Bool? = nil) {
        guard animating.contains(view) else { return }
        if atTarget == false, !dragging, !animationMoved.contains(view) { return }
        if !dragging, owedTargets[view] != nil {
            // UIKit starts no new animation from inside the callback that
            // ends one: the owed target goes on the next turn, still headed
            // there in the meantime.
            let serial = animationSerial[view]
            DispatchQueue.main.async { [weak self] in
                guard let self, self.animationSerial[view] == serial, self.animating.remove(view) != nil else { return }
                self.animationTargets[view] = nil; self.animationSerial[view] = nil
                self.landAnimation(view)
                self.dirty.insert(view)
                self.schedule()
            }
            return
        }
        stopAnimation(view)
        dirty.insert(view)
        schedule()
    }
    var requestFill: (() -> Void)?
    /// A glimpse (the runner's `CollectionFill::glimpse`): travel that crosses
    /// a port or more each frame (the scroll indicator dragged) shows rows no
    /// frame before showed, more than a frame can build; a photo grid's took
    /// 30 to 70 ms a frame. The rows a rescue of a list may build, those
    /// nearest the port's middle, or nil to build all that shows
    /// (`ScrollPump.glimpse`); and what one cost.
    var glimpse: ((UInt32) -> UInt32?)?
    var glimpsed: ((UInt32, TimeInterval, Int) -> Void)?
    /// Lists whose last report was a glimpse: the rest of what shows is
    /// built in the frames that follow, a glimpse at a time.
    private(set) var glimpses = Set<UInt32>()
    private var glimpsing = Set<UInt32>()
    /// After a report that built what shows inside the scroll callback: the
    /// platform paints what those rows show before the frame commits.
    var rescued: (() -> Void)?
    private(set) var fillPending = Set<UInt32>()
    private var sliceLimits: [UInt32: UInt32] = [:]
    /// @ref LLP 1072 §3, §5 — a moving list's slice is built off main. The
    /// session sends it (`onFill`), holds reports behind earlier owner work
    /// (`filling`), and lands that work (`drain`: wait and apply).
    var onFill: ((UInt32, Data) -> Void)?
    var filling: (() -> Bool)?
    var drain: (() -> Void)?
    /// The slice sent off main, and any retire-only report its landing owes,
    /// each with its limit.
    private var building: (view: UInt32, limit: UInt32)?
    private var retireOwed: [UInt32: UInt32] = [:]
    private var fillLimits: [UInt32: UInt32] = [:]
    /// Seconds from a slice's send to its landing, per list: the lead a
    /// moving list builds ahead covers it.
    private(set) var fillLatency: [UInt32: Double] = [:]
    private var fillSent: [UInt32: Double] = [:]
    // Platform hooks remove event monitors/recognizers when the adapter resets.
    var stopTracking: (() -> Void)?
    init(_ presenter: Presenter) { self.presenter = presenter }
    deinit {
        stopTracking?()
        if let turnObserver { CFRunLoopRemoveObserver(CFRunLoopGetMain(), turnObserver, .commonModes) }
    }
    private func observeTurns() {
        guard turnObserver == nil else { return }
        let observer = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.afterWaiting.rawValue, true, 0) { [weak self] _, _ in
            self?.budget.nextTurn()
        }
        CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
        turnObserver = observer
    }
    var observingTurns: Bool { turnObserver != nil }

    func reset() {
        gestureContact = nil
        generation += 1; queued = false; batchDepth = 0; correcting = false
        stopTracking?(); stopTracking = nil
        // A list's animation goes with it, stopped where the port is (LLP 1070.000 §2.5).
        for view in animating { stopAnimation(view) }
        entries.removeAll(); dirty.removeAll(); interaction = nil; contactEvent = nil
        fillPending.removeAll(); sliceLimits.removeAll(); glimpses.removeAll(); glimpsing.removeAll()
        building = nil; retireOwed.removeAll(); fillLimits.removeAll(); fillSent.removeAll()
        budget = CollectionTurnBudget(); rescuing.removeAll()
        refreshPins = false; lastVisited = 0
        resting.removeAll()
        #if os(iOS) || os(tvOS)
        focusFound = nil
        #endif
    }
    func beginBatch(_ batch: Batch) {
        // A kept scroll view has a new collection behind it. Its row epochs,
        // feedback sequences and correction revisions all start over.
        if batch.replacesTree { reset() }
        if batchDepth == 0 { for (view, entry) in entries { entry.batchInsets = portInsets(view); entry.batchStart = batchOffset(view) } }
        batchDepth += 1
        #if os(iOS) || os(tvOS)
        invalidateRecords(batch)
        #endif
        for op in batch.ops where op.op == .collections {
            guard let snapshots = op.collections else { continue }
            let live = Set(snapshots.map(\.view))
            entries = entries.filter { live.contains($0.key) }
            for view in animating.subtracting(live) { stopAnimation(view) }
            retireOwed = retireOwed.filter { live.contains($0.key) }
            dirty.formIntersection(live)
            resting.formIntersection(live)
            fillPending.formIntersection(live)
            glimpses.formIntersection(live)
            for snapshot in snapshots {
                if let entry = entries[snapshot.view] {
                    if snapshot.revision >= entry.snapshot.revision {
                        // A rejected report still owes any measurement the newer snapshot
                        // has not acknowledged at that identity and epoch.
                        #if os(iOS) || os(tvOS)
                        entry.records.update(snapshot, entry: entry)
                        #else
                        if snapshot.revision > entry.snapshot.revision, let sent = entry.lastFacts?.measurements {
                            let rows = Dictionary(snapshot.rows.map { ($0.view, $0) }, uniquingKeysWith: { a, _ in a })
                            if sent.contains(where: { m in
                                rows[m.view].map { $0.epoch == m.epoch && !Self.acknowledges($0, m) } ?? false
                            }) {
                                entry.owed = true
                            }
                        }
                        #endif
                        if snapshot.revision > entry.snapshot.revision, !snapshot.fixed.isEmpty,
                           let sent = entry.lastFacts?.measurements,
                           sent.contains(where: { m in
                               snapshot.fixed.contains { $0.view == m.view && $0.epoch == m.epoch && $0.size != m.size }
                           }) {
                            entry.owed = true
                        }
                        #if os(macOS)
                        let prior = Set(entry.snapshot.rows.map(\.view))
                        for row in snapshot.rows where !prior.contains(row.view) { entry.rowsAdded += 1 }
                        #endif
                        // A gesture counts against the request it began after: a new one starts with none.
                        // A request that ends or is replaced takes its animation with it, stopped where the
                        // port is (LLP 1070.000 §2.5); a newer one is admitted from the report that follows.
                        if snapshot.request != entry.snapshot.request {
                            entry.gesture = false
                            entry.requestAt = ProcessInfo.processInfo.systemUptime
                            stopAnimation(snapshot.view)
                        } else if !snapshot.underWay { entry.gesture = false }
                        entry.snapshot = snapshot
                    }
                } else {
                    let entry = Entry(snapshot)
                    entries[snapshot.view] = entry
                    #if os(iOS) || os(tvOS)
                    entry.records.update(snapshot, entry: entry)
                    #endif
                }
                dirty.insert(snapshot.view)
                if entries[snapshot.view]!.snapshot.pending { fillPending.insert(snapshot.view) }
                #if os(macOS)
                watchContact(snapshot.view, entries[snapshot.view]!.snapshot.underWay)
                #endif
            }
            if !fillPending.isEmpty { requestFill?() }
        }
        if !entries.isEmpty, stopTracking == nil { startTracking() }
        if entries.isEmpty { stopTracking?(); stopTracking = nil; interaction = nil }
    }
    func endBatch() {
        #if os(iOS) || os(tvOS)
        if batchDepth == 1 { for entry in entries.values { entry.records.endBatch() } }
        #endif
        // Frames/content have all landed before offsets change; platform methods
        // clamp against this coherent extent and suppress correction notifications.
        for (view, entry) in entries {
            guard let port = geometry(view) else { continue }
            let dimensions = [port.portCross, port.portMain, port.cross]
            let planned = entry.cursor.sequence, jumped = entry.cursor.jumpedAt
            if let previous = entry.port, previous != dimensions { entry.cursor.jump() }
            entry.port = dimensions
            // The port went under the bar or came out from under it (LLP
            // 1116.000 D1.3): the platform's clamp to the new frame and its
            // compensation for the new inset are not the reader moving, and
            // together they put an opened transcript's end an inset short.
            // Where the port starts stays (CSS's scrollTop), before any
            // correction lands.
            if let start = entry.batchStart, let insets = entry.batchInsets, insets != portInsets(view) {
                correcting = true
                keepStart(view, at: start)
                correcting = false
            }
            entry.batchInsets = nil
            if let correction = entry.snapshot.correction, correction.from != nil {
                // Rows before the anchor changed size in this batch: the
                // offset moves with them before this frame displays, even
                // under a pan or a fling, which go on from there. A port this
                // same batch resized is not the reader moving either: rows
                // put above the reader as a pull-to-refresh zone closes stay
                // put on the page (feed F14), so a correction planned
                // before it still lands.
                if let delta = entry.cursor.takeShift(revision: entry.snapshot.revision, correction, jumpedBefore: jumped) {
                    correcting = true
                    shift(view, by: delta, extent: entry.snapshot.extent, from: entry.batchStart)
                    correcting = false
                }
            } else if let correction = entry.snapshot.correction,
               // The port this batch resized was not the reader moving: an
               // absolute correction planned at the sequence before it (a
               // sent message's end-follow as the composer shrinks back)
               // still lands, clamped to the new port.
               entry.cursor.takeCorrection(revision: entry.snapshot.revision,
                   sequence: correction.sequence == planned ? entry.cursor.sequence : correction.sequence, authored: entry.authored) {
                correcting = true
                correct(view, top: correction.offset, extent: entry.snapshot.extent, smooth: correction.smooth, authored: entry.authored)
                correcting = false
            }
            entry.batchStart = nil
            dirty.insert(view)
        }
        batchDepth = max(0, batchDepth - 1)
    }
    /// The collection whose mounted row holds `descendant`: the nearest row
    /// wrapper at or above it names the owner, as the web host's `liveView`
    /// does, so a nested collection owns its descendants' pins and ancestors
    /// never pin the outer row as well. A view in no mounted row (a list
    /// itself, a spacer, the scroller a press landed on) pins nothing; the
    /// runner discards a report whose pin is outside its rows, and the window
    /// would stop following the scroll. Native container views may sit
    /// between nodes.
    package func owningCollection(_ descendant: UInt32?) -> UInt32? {
        guard let descendant, let node = presenter?.views[descendant] else { return nil }
        func owner(_ node: NodeView) -> UInt32? {
            guard presenter?.views[node.id] === node else { return nil }
            return entries.first { $0.value.snapshot.rows.contains { $0.view == node.id } }?.key
        }
        if let id = owner(node) { return id }
        var parent = node.superview
        while let current = parent {
            if let node = current as? NodeView, let id = owner(node) { return id }
            parent = current.superview
        }
        return nil
    }
    func owns(_ view: UInt32) -> Bool { entries[view] != nil }
    var listIDs: [UInt32] { Array(entries.keys) }
    func snapshot(_ id: UInt32) -> CollectionSnapshot? { entries[id]?.snapshot }
    func mainOffset(_ id: UInt32) -> Double? { geometry(id)?.offset }
    func animationTarget(_ id: UInt32) -> CGPoint? { animationTargets[id] }
    #if os(iOS) || os(tvOS)
    func drivesOffset(_ id: UInt32) -> Bool { offsetDrivers[id] != nil }
    #else
    func drivesOffset(_ id: UInt32) -> Bool { false }
    #endif
    /// An inner list whose outer list moves builds only what it owes (LLP
    /// 1070 F2): a slice for its pending remainder would report and build
    /// nothing, once a frame for every mounted inner list.
    func ancestorMoving(_ view: UInt32) -> Bool {
        entries[view]?.snapshot.parent.flatMap { motion?($0) } != nil
    }
    /// The port is about to move: by the reader (`travel`, a drag or a
    /// wheel beginning) or to an authored offset.
    func userIntent(_ view: UInt32, travel: Bool = false) {
        guard let entry = entries[view], !correcting else { return }
        #if os(macOS)
        if travel { readerMoved(view) }
        #endif
        if travel { entry.cursor.advance() } else { entry.cursor.jump() }
        dirty.insert(view)
        schedule()
    }
    /// The reader began a gesture on `view`'s port: it cancels the request
    /// under way there, reported at once.
    /// An input counts against the request under way when it began (`at`, an
    /// event's timestamp): not against one a handler made in answer to it,
    /// which reached the host after, whatever order the platform runs the
    /// input's handlers in; a tie is not after. An event made with no time
    /// (0, a synthesized one's) counts.
    func gestureBegan(_ view: UInt32, at time: TimeInterval? = nil) {
        guard let entry = entries[view], entry.snapshot.underWay, !entry.gesture,
              time.map({ $0 == 0 || $0 > entry.requestAt }) ?? true else { return }
        entry.gesture = true
        dirty.insert(view)
        schedule()
    }
    /// The reader's gesture at `time` on every list around `view`, which is
    /// found only while a request is under way.
    func gestureBegan(around view: @autoclosure () -> PlatformView?, at time: TimeInterval? = nil) {
        guard entries.values.contains(where: { $0.snapshot.underWay }) else { return }
        var view = view()
        while let current = view {
            if let node = current as? NodeView { gestureBegan(node.id, at: time) }
            view = current.superview
        }
    }
    #if os(iOS) || os(tvOS)
    private func quiet(_ id: UInt32, facts: CollectionFacts, ignoringDirty: Bool = false, requiringLead: Bool = true) -> Bool {
        guard let entry = entries[id], let previous = entry.lastFacts, let window = entry.snapshot.window,
              (facts.portMain, facts.portCross, facts.cross, facts.paddingStart, facts.paddingEnd)
                == (previous.portMain, previous.portCross, previous.cross, previous.paddingStart, previous.paddingEnd),
              !ExactEnv.agentFreezes, !resting.contains(id),
              ignoringDirty || !dirty.contains(id),
              sliceLimits[id] == nil, !refreshPins, !entry.owed, !entry.gesture,
              entry.travelled || !entry.snapshot.pending,
              !animating.contains(id), !ancestorMoving(id), !entry.records.changed,
              entry.records.unacknowledged.isEmpty, covered(id) == entry.covered else { return false }
        let focus = focusFound ?? focusedView()
        focusFound = focus
        guard (owningCollection(focus) == id ? focus : nil) == previous.focus,
              (owningCollection(interaction) == id ? interaction : nil) == previous.interaction else { return false }
        let speed = motion?(id) ?? 0
        let ahead = speed * (presenter?.leadTime(id) ?? 0)
        let top = max(0, facts.offset), end = min(entry.snapshot.extent, top + facts.portMain)
        guard !requiringLead || (max(0, top + min(0, ahead)) >= window.built[0]
              && min(entry.snapshot.extent, end + max(0, ahead)) <= window.built[1]),
              !window.owesReveal(offset: top, port: facts.portMain, extent: entry.snapshot.extent),
              !window.owesEdge(offset: top, port: facts.portMain, velocity: speed),
              !window.owesRetirement(offset: top, port: facts.portMain, moving: entry.travelled)
        else { return false }
        // A reversal can bring an owed retirement back inside its band.
        retireOwed[id] = nil
        return true
    }
    /// Rest asks the pump to restore the ordinary window in bounded slices.
    func rested(_ id: UInt32) {
        guard let entry = entries[id], entry.travelled else { return }
        entry.travelled = false
        presenter?.forgetTravel(id)
        resting.insert(id)
        retireOwed[id] = nil
        fillPending.insert(id)
        dirty.insert(id)
        requestFill?()
    }
    #endif
    /// Record the reader's move before draining corrections planned on an older frame.
    func advanceScrollSequence(_ view: UInt32) {
        guard let entry = entries[view], !correcting, batchDepth == 0, !animating.contains(view) else { return }
        entry.cursor.advance()
    }
    func changed(_ view: UInt32, user: Bool = false, sequenceAdvanced: Bool = false) {
        guard let entry = entries[view], !correcting else { return }
        // The host's own animation, not the reader: reported as where it
        // is headed (`geometry`), and the sequence stays.
        if user && animating.contains(view) { animationMoved.insert(view); dirty.insert(view); schedule(); return }
        #if os(macOS)
        // Not the clip AppKit clamps inside a batch's layout. A request that
        // has moved the port is the reader's once the reader moves it, by
        // whatever input: its corrections no longer land whatever the port
        // did, and the next report ends it (LLP 1070.000 §2.5).
        if user && batchDepth == 0 {
            if entry.authored { entry.gesture = true }
            readerMoved(view)
        }
        #endif
        if user && !sequenceAdvanced { advanceScrollSequence(view) }
        #if os(iOS) || os(tvOS)
        if user && batchDepth == 0 {
            entry.travelled = true
            if let facts = geometry(view) {
                presenter?.session?.runtime.noteCollectionScroll(view, revision: entry.snapshot.revision,
                    sequence: entry.cursor.sequence, offset: facts.offset)
                if quiet(view, facts: facts) { return }
                if covers(view), let window = entry.snapshot.window {
                    let speed = motion?(view) ?? 0
                    if window.owesReveal(offset: facts.offset, port: facts.portMain, extent: entry.snapshot.extent)
                        || window.owesEdge(offset: facts.offset, port: facts.portMain, velocity: speed) {
                        // Entering an edge or revealing a row cannot wait for a
                        // later offset. Land earlier work before gathering facts.
                        drain?()
                        dirty.insert(view)
                        retireOwed[view] = 0
                        rescuing.insert(view)
                        flushIfNeeded()
                        rescuing.remove(view)
                        return
                    }
                    if window.owesRetirement(offset: facts.offset, port: facts.portMain, moving: entry.travelled) {
                        retireOwed[view] = 0
                    }
                }
            }
        }
        #endif
        dirty.insert(view)
        guard batchDepth == 0 else { return }
        if !user { schedule(); return }
        // While mounted rows cover the port, the pump reports after the
        // scroll callback returns, building ahead in its slice; otherwise
        // the rows the reader sees are built now (LLP 1050.000 D1).
        if motion != nil, covers(view) {
            glimpses.remove(view)
            fillPending.insert(view)
            requestFill?()
        } else {
            // What shows is owed by estimated heights; two more rows cover
            // a port the estimates overstate (a jump into unmeasured rows).
            let bounded = glimpse?(view)
            if let bounded { sliceLimits[view] = bounded; glimpsing.insert(view) } else if motion != nil { sliceLimits[view] = 2 }
            lastVisited = view &- 1
            // What shows cannot wait for a slice in flight (T7): land it,
            // then report from the geometry that shows now.
            if filling?() == true { drain?() }
            let before = entry.rowsAdded, started = ProcessInfo.processInfo.systemUptime
            rescuing.insert(view)
            flushIfNeeded()
            rescuing.remove(view)
            sliceLimits[view] = nil
            if bounded != nil {
                glimpsing.remove(view)
                glimpsed?(view, ProcessInfo.processInfo.systemUptime - started, entries[view]?.added(since: before) ?? 0)
            }
            if motion != nil { rescued?() }
        }
    }
    /// One slice of fill for `view`: a report that may build `limit` rows
    /// past what it owes, and its measurement pass. The slice is its own
    /// main-queue turn. Returns the rows it created.
    /// `glimpse`: a glimpse's slice, which leaves what shows past its rows.
    @discardableResult
    func fillSlice(_ view: UInt32, limit: UInt32, glimpse: Bool = false) -> Int {
        #if DEBUG
        slices += 1
        #endif
        // One slice in flight at a time. Any owed retirement runs first,
        // from main's fresh facts (LLP 1072 §3.1, §5).
        if filling?() == true { return 0 }
        if !retireOwed.isEmpty {
            dirty.formUnion(retireOwed.keys)
            flushIfNeeded()
            return 0
        }
        fillPending.remove(view)
        guard let entry = entries[view], batchDepth == 0 else { return 0 }
        let before = entry.rowsAdded
        budget.nextTurn()
        dirty.insert(view)
        sliceLimits[view] = limit
        // The slice's own list first: the round-robin would visit it last.
        lastVisited = view &- 1
        if glimpse { glimpsing.insert(view) } else if onFill != nil { building = (view, limit) }
        flushIfNeeded()
        building = nil
        glimpsing.remove(view)
        sliceLimits[view] = nil
        return entries[view]?.added(since: before) ?? 0
    }
    /// A landed build may leave rows past the retirement bound or an edge
    /// owed. Main reports those from its current offset and pins (LLP 1072 §5).
    func landed(_ view: UInt32, at now: Double) {
        if let sent = fillSent.removeValue(forKey: view) { fillLatency[view] = now - sent }
        defer { if !dirty.isEmpty { schedule() } }
        guard let limit = fillLimits.removeValue(forKey: view), entries[view] != nil else { return }
        #if os(iOS) || os(tvOS)
        let facts = geometry(view)
        if let entry = entries[view], let facts {
            // Travel while this build was unpublished named the old
            // revision. Refresh it after the applied corrections.
            presenter?.session?.runtime.noteCollectionScroll(view, revision: entry.snapshot.revision,
                sequence: entry.cursor.sequence, offset: facts.offset)
        }
        // Missing lead asks for a build, not a retire-only round trip. The
        // retirement band leaves two ports of hysteresis behind travel.
        if let facts, quiet(view, facts: facts, ignoringDirty: true, requiringLead: false) {
            let built = quiet(view, facts: facts, ignoringDirty: true)
            dirty.remove(view)
            fillPending.remove(view)
            if !built { fillPending.insert(view); requestFill?() }
            return
        }
        retireOwed[view] = entries[view]?.travelled != true || entries[view]?.snapshot.window == nil ? limit : 0
        #else
        retireOwed[view] = limit
        #endif
        // The pump's next frame: the slice's apply and its retirement are
        // two frames' work, not one (LLP 1072 §5).
        fillPending.insert(view)
        requestFill?()
    }
    /// Both identities are reported even when no data row is mounted.
    /// The kernel measures the authored flow, before pixel snapping, and
    /// excludes display-hidden margins just as an eager list does.
    func fixedMeasurements(_ id: UInt32) -> [CollectionMeasurement] {
        guard let fixed = entries[id]?.snapshot.fixed, fixed.count == 2 else { return [] }
        guard let sizes = presenter?.session?.runtime.collectionFixedSizes(id, lead: fixed[0].roots.count, trail: fixed[1].roots.count) else { return [] }
        return fixed.enumerated().map { side, item in CollectionMeasurement(view: item.view, epoch: item.epoch, size: sizes[side]) }
    }
    /// Fixed regions join the data rows' coverage, without becoming data
    /// items. Their logical bounds use the same height index as corrections.
    func fixedSpans(_ id: UInt32) -> [(first: Bool, last: Bool, start: Double, end: Double)] {
        guard let snapshot = entries[id]?.snapshot, snapshot.fixed.count == 2 else { return [] }
        let lead = snapshot.fixed[0].size, trail = snapshot.fixed[1].size
        return [(first: true, last: snapshot.count == 0 && trail == 0, start: 0, end: lead),
                (first: snapshot.count == 0 && lead == 0, last: true, start: snapshot.extent - trail, end: snapshot.extent)]
            .filter { $0.end > $0.start }
    }
    /// The agent's `clock settle`: every list reports until none is owed a
    /// report, so rows a reply mounted are measured before the agent reads
    /// or moves them, as a turn of the main queue would do between a
    /// reader's frames. Bounded.
    func settle() {
        #if os(iOS) || os(tvOS)
        for (id, entry) in entries where entry.travelled { rested(id) }
        for id in resting { sliceLimits[id] = UInt32.max - 1; dirty.insert(id) }
        #endif
        for _ in 0..<8 {
            guard !dirty.isEmpty, batchDepth == 0 else { return }
            budget.nextTurn()
            flushIfNeeded()
        }
    }
    func dataReady() {
        // Retry armed edges once after deferred activation without forgetting
        // the accepted pin reservations used to order transfers.
        for entry in entries.values { entry.lastSequence = nil }
        dirty.formUnion(entries.keys)
        schedule()
    }
    func pinsChanged() {
        #if os(iOS) || os(tvOS)
        focusFound = nil
        #endif
        guard !entries.isEmpty else { return }
        dirty.formUnion(entries.keys)
        // Responder callbacks may precede AppKit/UIKit committing the new
        // first responder. Recheck once after that native event completes.
        refreshPins = true
        schedule()
        if batchDepth == 0 { flushIfNeeded() }
    }
    func pointer(_ view: UInt32?) {
        gestureContact = nil
        contactSequence &+= 1
        interaction = view
        pinsChanged()
    }
    package func holdPointer(_ view: UInt32) -> UInt64 {
        pointer(view)
        gestureContact = contactSequence
        return contactSequence
    }
    package func releaseInteractionLater(ifCurrent expected: UInt64? = nil) {
        if let expected {
            guard contactSequence == expected else { return }
            gestureContact = nil
        } else if gestureContact == contactSequence { return }
        let captured = generation
        let prior = contactSequence
        DispatchQueue.main.async { [weak self] in
            guard let self, generation == captured, contactSequence == prior else { return }
            pointer(nil)
        }
    }

    func flushIfNeeded() {
        guard batchDepth == 0, onFeedback != nil, !dirty.isEmpty, filling?() != true else { return }
        #if os(iOS) || os(tvOS)
        // A queued measurement turn can precede the rest slice. Leave its
        // forced report to the pump, which supplies the retirement limit.
        for id in dirty where resting.contains(id) && sliceLimits[id] == nil && retireOwed[id] == nil {
            dirty.remove(id)
            fillPending.insert(id)
            requestFill?()
        }
        for id in dirty {
            if let facts = geometry(id), quiet(id, facts: facts, ignoringDirty: true) {
                dirty.remove(id)
                fillPending.remove(id)
            }
        }
        #endif
        if !dirty.isEmpty { flush() }
    }

    private func flush() {
        #if DEBUG
        flushes += 1
        #endif
        guard batchDepth == 0, let onFeedback, !dirty.isEmpty else { return }
        // A fill lands before a report gathers fresh facts (T4).
        // Its landing releases the reports; a continuation here would
        // retry on every main-queue turn until then (LLP 1072 §3.2).
        if filling?() == true { return }
        observeTurns()
        defer { if !dirty.isEmpty && filling?() != true { schedule() } }
        // Each list once per flush, at most, without a report (bounded
        // even if a visit re-dirties its list).
        var free = entries.count
        // Lists whose fixed sizes the kernel could not answer this flush:
        // they retry on the next turn, not again in this loop.
        var unmeasured = Set<UInt32>()
        defer { dirty.formUnion(unmeasured) }
        while !dirty.isEmpty {
            // A fill holds every report until it lands (T4).
            if filling?() == true { return }
            let rescue = !rescuing.isDisjoint(with: dirty)
            guard budget.begin(rescue: rescue) else { return }
            #if os(iOS) || os(tvOS)
            let focus = focusFound ?? focusedView()
            focusFound = focus
            #else
            let focus = focusedView()
            #endif
            let focusOwner = owningCollection(focus)
            let interactionOwner = owningCollection(interaction)
            // Retire old owners before publishing replacement pins, even when
            // the two-pass budget carries the new owner to a later callback.
            var priority = 3
            var following: UInt32?, wrapped: UInt32?
            for id in dirty {
                let facts = entries[id]?.lastFacts
                let retiring = (facts?.focus != nil && focusOwner != id) ||
                    (facts?.interaction != nil && interactionOwner != id)
                let rank = retiring ? 0 : rescuing.contains(id) ? 1 : 2
                if rank < priority { priority = rank; following = nil; wrapped = nil }
                guard rank == priority else { continue }
                if wrapped == nil || id < wrapped! { wrapped = id }
                if id > lastVisited, following == nil || id < following! { following = id }
            }
            let id = (following ?? wrapped)!
            lastVisited = id
            dirty.remove(id)
            let retireLimit = retireOwed.removeValue(forKey: id)
            let atRest = resting.remove(id) != nil
            // Cached geometry permits a hidden previous owner to release its
            // pin. Measurements below still come only from visible live rows.
            var reported = false
            let visibleFacts = geometry(id)
            if let entry = entries[id], var facts = visibleFacts ?? entry.lastFacts {
                facts.focus = focusOwner == id ? focus : nil
                facts.interaction = interactionOwner == id ? interaction : nil
                if priority == 0 {
                    // Focus and interaction can swap owners together. Clearing
                    // one old pin must not publish the other new pin early.
                    if facts.focus != entry.lastFacts?.focus { facts.focus = nil }
                    if facts.interaction != entry.lastFacts?.interaction { facts.interaction = nil }
                    if (focusOwner == id && facts.focus != focus) ||
                       (interactionOwner == id && facts.interaction != interaction) { dirty.insert(id) }
                }
                #if os(iOS) || os(tvOS)
                facts.measurements = visibleFacts == nil ? [] : measurements(id, cross: facts.cross)
                #else
                let horizontal = entry.snapshot.horizontal
                facts.measurements = entry.snapshot.rows.compactMap { row in
                    guard let size = size(row.view, horizontal: horizontal), size.isFinite, size >= 0,
                          crossSize(row.view, horizontal: horizontal) == facts.cross else { return nil }
                    return CollectionMeasurement(view: row.view, epoch: row.epoch, size: size)
                }
                #endif
                // The runner rejects a fixed list's report without both
                // fixed sizes, and a sent report is taken as delivered. The
                // kernel has no answer while main serves the owner, so the
                // list stays owed until it does.
                let fixedSizes = fixedMeasurements(id)
                if entry.snapshot.fixed.count == 2 && fixedSizes.isEmpty {
                    entry.owed = true
                    unmeasured.insert(id)
                    if let retireLimit { retireOwed[id] = retireLimit }
                    free -= 1
                    budget.end(reported: free < 0)
                    continue
                }
                if !fixedSizes.isEmpty { facts.measurements += fixedSizes }
                // A slice builds its limit once; later passes and every
                // report while moving or owed a continuation only measure
                // and rescue what shows. Rest uses the pump's slice limit.
                // Not while a request's corrections are the app's: neither their
                // moves nor what was sampled before it are the reader's travel.
                let velocity = entry.authored ? nil : motion?(id)
                #if os(iOS) || os(tvOS)
                let defaultLimit: UInt32? = motion == nil ? nil : 0
                #else
                let defaultLimit: UInt32? = motion != nil && (entry.lastFacts == nil || velocity != nil || entry.snapshot.pending) ? 0 : nil
                #endif
                let limit: UInt32? = retireLimit ?? sliceLimits.removeValue(forKey: id)
                    ?? defaultLimit
                let further = limit.map { $0 > 0 && $0 > (entry.lastLimit ?? .max) } ?? false
                // Off main only when the report keeps the port, the width and
                // the pins: a report that changes them may retire (§5).
                let same = entry.lastFacts.map { ($0.portMain, $0.portCross, $0.cross, $0.focus, $0.interaction)
                    == (facts.portMain, facts.portCross, facts.cross, facts.focus, facts.interaction) } ?? false
                let glimpse = glimpsing.contains(id) && retireLimit == nil && limit != nil
                let offMain = building?.view == id && same && retireLimit == nil && !glimpse ? building : nil
                let reading = Reading(scrolling: readerScrolling(id), gesture: entry.gesture, animating: animating.contains(id))
                // What the bars cover goes to the runner with a report, so a change of it alone owes one
                #if os(iOS) || os(tvOS)
                let coveredMoved = covered(id) != entry.covered
                #else
                let coveredMoved = false
                #endif
                if entry.lastFacts != facts || entry.lastSequence != entry.cursor.sequence || entry.lastReading != reading
                    || (entry.snapshot.pending && limit != 0) || further || retireLimit != nil || entry.owed || atRest || coveredMoved {
                    var report = facts
                    // A sent report may be rejected atomically. Only the
                    // runner's measured snapshot acknowledges a height;
                    // invalidated or unacknowledged sizes are sent again.
                    if let previous = entry.lastFacts, previous.cross == facts.cross {
                        #if os(iOS) || os(tvOS)
                        report.measurements = visibleFacts == nil ? [] : entry.records.eligibleUnacknowledged
                        if !fixedSizes.isEmpty { report.measurements += fixedSizes }
                        #else
                        let known = Dictionary(entry.snapshot.rows.map { ($0.view, $0) }, uniquingKeysWith: { a, _ in a })
                        report.measurements.removeAll { m in known[m.view].map { Self.acknowledges($0, m) } ?? false }
                        #endif
                    }
                    entry.lastFacts = facts
                    entry.lastReading = reading
                    entry.owed = false
                    entry.lastSequence = entry.cursor.sequence
                    entry.lastLimit = limit
                    reported = true
                    #if os(iOS) || os(tvOS)
                    reportCovered(id, entry)
                    #endif
                    // While its outer list moves, an inner list builds only
                    // what it owes (LLP 1070 F2); its pending report
                    // continues once the outer list rests.
                    let ancestorMoving = entry.snapshot.parent.flatMap { motion?($0) } != nil
                    if let offMain, let onFill {
                        fillLimits[id] = offMain.limit
                        fillSent[id] = ProcessInfo.processInfo.systemUptime
                        onFill(id, report.encode(view: id, revision: entry.snapshot.revision, sequence: entry.cursor.sequence,
                            velocity: velocity ?? 0, limit: limit == UInt32.max - 1 ? nil : limit, ancestorMoving: ancestorMoving, createOnly: true, reading: reading))
                        budget.end()
                        return
                    }
                    if glimpse { glimpses.insert(id) } else { glimpses.remove(id) }
                    onFeedback(report.encode(view: id, revision: entry.snapshot.revision, sequence: entry.cursor.sequence,
                        velocity: velocity ?? 0, limit: limit, ancestorMoving: ancestorMoving, noBuild: retireLimit != nil, glimpse: glimpse,
                        reading: reading))
                }
            }
            free -= reported ? 0 : 1
            budget.end(reported: reported || free < 0)
        }
    }
    /// Continuations run (`schedule`), for tests.
    private(set) var continuations = 0
    private func schedule() {
        guard !queued else { return }
        queued = true
        let captured = generation
        DispatchQueue.main.async { [weak self] in
            guard let self, generation == captured else { return }
            queued = false
            continuations += 1
            budget.nextTurn()
            if refreshPins {
                refreshPins = false; dirty.formUnion(entries.keys)
                #if os(iOS) || os(tvOS)
                focusFound = nil
                #endif
            }
            flushIfNeeded()
        }
    }
}

extension CollectionHost {
    /// The row wrapper at or above `descendant`: what a ghost snapshots.
    func wrapper(of descendant: UInt32) -> NodeView? {
        guard let presenter, var node: NSObject = presenter.views[descendant] else { return nil }
        while true {
            if let view = node as? NodeView, presenter.views[view.id] === view,
               entries.values.contains(where: { $0.snapshot.rows.contains { $0.view == view.id } }) { return view }
            guard let parent = (node as? PlatformView)?.superview else { return nil }
            node = parent
        }
    }
}
