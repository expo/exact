// A large batch, prepared over display frames and published in one turn
// (LLP 1072 T13, §3.5).
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif
import ExactCore
import QuartzCore

/// What preparation applies of one batch, and how far it has got.
///
/// Preparation builds the batch's new subtrees where they will stand, each
/// top held off screen (`NodeView.offstageAlpha`): drawn, but seen, hit and
/// read by nothing. Everything else the batch does waits for the reveal,
/// which applies the whole batch in one turn with the prepared ops done.
final class StagePlan {
    /// Native controls and the children they own enter the window at the reveal.
    static let plainKinds: Set<String> = ["view", "text"]
    /// Below this many prepared ops the batch applies in its own turn: on
    /// the Mac, 300 ops of plain boxes apply in about a frame.
    #if DEBUG
    nonisolated(unsafe) static var minimumOps = 300
    #else
    static let minimumOps = 300
    #endif
    /// A display frame's preparation and the drawing of what it put in the
    /// window, which the slice commits itself to measure.
    static let frameSeconds = 0.004
    /// A box with more below it than this joins the window a child at a
    /// time, after the rest is built: what joins draws in that frame.
    static let attachLimit = 32
    static let firstAttachSeconds = 0.0005

    let batch: Batch
    /// Per op of `batch`: whether preparation applies any of it.
    let prepared: [Bool]
    /// A mixed child list mounts only the prepared nodes until its complete list can be revealed.
    private let earlyChildren: [Int: [UInt32]]
    func preparingOp(_ index: Int) -> BatchOp {
        var op = batch.ops[index]
        if let ids = earlyChildren[index] { op.ids = ids }
        return op
    }
    /// The ops the reveal applies: the rest, less superseded samples, with
    /// the newest style a passing batch gave. The rest is made in the last
    /// slice, not in the reveal's turn.
    private(set) var revealCount = 0
    private var rest: Batch?
    func prepareReveal() {
        guard rest == nil else { return }
        var reveal = batch
        reveal.ops = batch.ops.indices.compactMap { prepared[$0] && earlyChildren[$0] == nil ? nil : batch.ops[$0] }
        rest = reveal
    }
    func reveal() -> Batch {
        prepareReveal()
        var reveal = rest!
        if !superseded.isEmpty { reveal.ops.removeAll { $0.op == .present && superseded.contains("\($0.id) \($0.property)") } }
        for i in reveal.ops.indices where reveal.ops[i].op == .style { if let newer = newerStyles[reveal.ops[i].id] { reveal.ops[i].style = newer } }
        revealCount = reveal.ops.count
        return reveal
    }
    /// Each new subtree's top, with the live node it joins.
    let tops: [UInt32: UInt32]
    /// The nodes the batch destroys.
    let destroyed: Set<UInt32>
    /// The next op preparation looks at.
    var cursor = 0
    /// The `children` ops of boxes with more than `attachLimit` below
    /// them, applied a child at a time once the other ops are (`prepare`),
    /// outermost first; and the next child of the first.
    var attachments: [Int] = []
    var attachedChildren = 0, attachmentsOrdered = false
    let depth: [UInt32: Int]
    let large: Set<UInt32>
    /// The preparation a slice may spend, from what drawing cost the last.
    var sliceSeconds = 0.001
    /// What preparation made, in the order it made it. These views join the
    /// presenter's table only for a slice and at the reveal.
    var made: [UInt32: NodeView] = [:]
    var madeOrder: [UInt32] = []
    /// The next made view whose accessibility preparation writes.
    var accessibilityCursor = 0
    /// Made nodes whose accessibility the reveal syncs: what it announces or
    /// focuses happens when the node is seen.
    var revealAccessibility: Set<UInt32> = []
    /// What preparation asked to run after its batch (`Presenter.afterBatch`): run after the reveal's.
    var afterReveal: [() -> Void] = []
    /// The motion samples (`present` ops, by node and property) a batch that
    /// passed it wrote later: the reveal leaves its own out.
    private(set) var superseded = Set<String>()
    /// The newest style a batch that passed it gave a node it styles: the
    /// reveal applies it in place of its own (`withheld`).
    private(set) var newerStyles: [UInt32: NodeStyle] = [:]
    /// The session's `viewsEpoch` it was planned in: a reveal in a later one is refused.
    var epoch = 0
    /// What the batch makes, destroys or moves, and the properties it writes on each other node.
    let structural: Set<UInt32>
    let written: [UInt32: Set<String>]
    /// The native control state it carries (`Batch.controlReads`).
    let reads: [UInt32: Set<String>]
    /// The style it gives each live node: a style op carries a node's whole
    /// style, a sampled paint value with the rest.
    let styles: [UInt32: NodeStyle]
    var frames: StageFrames?
    var preparedAll: Bool { cursor == batch.ops.count && attachments.isEmpty && accessibilityCursor == madeOrder.count }

    /// Nil when the batch makes too little to stage. `joins` says whether a
    /// live node can take a prepared subtree; `holds` whether a live node is
    /// already a child of another.
    init?(_ batch: Batch, joins: (UInt32) -> Bool, holds: (UInt32, UInt32) -> Bool) {
        let ops = batch.ops
        guard ops.count >= Self.minimumOps else { return nil }
        var kinds: [UInt32: String] = [:], destroyed = Set<UInt32>()
        for op in ops {
            if op.op == .create { kinds[op.id] = op.kind } else if op.op == .destroy { destroyed.insert(op.id) }
        }
        var parent: [UInt32: UInt32] = [:], refused = Set<UInt32>()
        let structural = Self.restructured(ops, holds: holds)
        var written: [UInt32: Set<String>] = [:], styles: [UInt32: NodeStyle] = [:]
        for op in ops {
            if op.op != .children, op.op != .roots, let id = op.nodeID { written[id, default: []].formUnion(Self.writes(op)) }
            if op.op == .style, kinds[op.id] == nil { styles[op.id] = op.style }
            let made = kinds[op.id] != nil
            switch op.op {
            case .create: if !Self.plainKinds.contains(op.kind) { refused.insert(op.id) }
            case .children:
                for child in op.ids where kinds[child] != nil { parent[child] = op.id }
            case .props, .style, .paragraph, .frame, .flow, .content, .rank: break
            default: if made { refused.insert(op.id) }
            }
        }
        var topOf: [UInt32: UInt32?] = [:]
        func top(_ id: UInt32) -> UInt32? {
            var chain: [UInt32] = [], at = id, found: UInt32? = nil
            while chain.count <= kinds.count {
                if let known = topOf[at] { found = known; break }
                chain.append(at)
                // A root, or a box no `children` op places, joins nothing.
                guard !refused.contains(at), let up = parent[at] else { break }
                if kinds[up] == nil { found = !destroyed.contains(up) && joins(up) ? at : nil; break }
                at = up
            }
            for id in chain { topOf[id] = found }
            return found
        }
        var earlyChildren: [Int: [UInt32]] = [:]
        var prepared = [Bool](repeating: false, count: ops.count), count = 0
        for (index, op) in ops.enumerated() where kinds[op.id] != nil {
            guard let group = top(op.id) else { continue }
            if op.op == .children {
                // Moving a live child now would take it off the committed screen.
                func ready(_ child: UInt32) -> Bool { kinds[child] != nil && top(child) == group }
                if !op.ids.allSatisfy(ready) { earlyChildren[index] = op.ids.filter(ready) }
            }
            prepared[index] = true
            count += 1
        }
        guard count >= Self.minimumOps else { return nil }
        var tops: [UInt32: UInt32] = [:], depth: [UInt32: Int] = [:], below: [UInt32: Int] = [:]
        for (id, found) in topOf where found == id { tops[id] = parent[id] }
        for (id, found) in topOf {
            guard let found, tops[found] != nil else { continue }
            var at = id, d = 0
            while at != found, let up = parent[at] { below[up, default: 0] += 1; at = up; d += 1 }
            depth[id] = d
        }
        self.depth = depth
        self.large = Set(below.filter { $0.value > Self.attachLimit }.keys)
        self.structural = structural
        self.written = written
        self.reads = (batch.controlReads ?? [:]).mapValues(\.parts)
        self.styles = styles
        self.batch = batch
        self.earlyChildren = earlyChildren
        self.prepared = prepared
        self.tops = tops
        self.destroyed = destroyed
    }
}

extension StagePlan {
    /// The nodes `ops` make, destroy or move, and each parent whose children
    /// they change: a live child that stays in its parent (`holds`) is not
    /// moved, and one another parent takes is. Both what a staged batch
    /// leaves alone (`disjoint`) and what a waiting one does
    /// (`BatchFootprint`) are read from it.
    static func restructured(_ ops: [BatchOp], holds: (UInt32, UInt32) -> Bool) -> Set<UInt32> {
        var out = Set<UInt32>()
        for op in ops {
            switch op.op {
            case .create, .destroy: out.insert(op.id)
            case .children: out.insert(op.id); out.formUnion(op.ids.filter { !holds(op.id, $0) })
            case .roots: out.formUnion(op.ids)
            case .renew, .exit, .flight, .land, .reorder: if let id = op.nodeID { out.insert(id) }; out.formUnion(op.ids)
            default: break
            }
        }
        return out
    }
    /// What `op` writes on its node, as `disjoint` compares it.
    static func writes(_ op: BatchOp) -> Set<String> {
        switch op.op {
        case .props: return Set(op.props.keys.map { "p." + $0 }).union(op.clear.map { "p." + $0 })
        case .style: return Set(op.style.keys.map { "s." + $0 })
        case .present: return ["present." + op.property]
        default: return [op.op.rawValue]
        }
    }

    /// Whether `other`, made later, leaves this batch alone: it changes no
    /// node this one makes, destroys or moves, and no property this one
    /// writes, except with a newer sample. It may then apply while this one
    /// is prepared (`ExactSession.order`): what shows is the old tree with
    /// newer values on properties this batch leaves alone, and newer
    /// samples, which the two in order also give. A sample is a `present`
    /// op's motion value, or a paint value, which Apple's runtime sends in
    /// the node's whole `style` (`present_colors`, LLP 1055.000 D6); a
    /// style op passes with this batch's own values withheld (`withheld`).
    /// The navigation roots' tabs a batch carries replace those shown, so
    /// two that carry them keep their order.
    func disjoint(_ other: Batch) -> Bool {
        if other.rootTabs != nil, batch.rootTabs != nil { return false }
        if other.routeInspectors != nil, batch.routeInspectors != nil { return false }
        if let theirs = other.controlReads, theirs.contains(where: { structural.contains($0.key) || reads[$0.key]?.isDisjoint(with: $0.value.parts) == false }) { return false }
        for op in other.ops {
            switch op.op {
            case .create, .destroy, .children, .roots, .renew, .exit, .flight, .land: return false
            default: break
            }
            guard let id = op.nodeID, !structural.contains(id) else { return false }
            if op.op == .present || op.op == .style { continue }
            if let mine = written[id], !mine.isDisjoint(with: Self.writes(op)) { return false }
        }
        return true
    }
    /// `other`, disjoint, applies ahead of it: its samples supersede this
    /// batch's, and its styles are the newest.
    func passed(_ other: Batch) {
        for op in other.ops {
            if op.op == .present { superseded.insert("\(op.id) \(op.property)") }
            if op.op == .style, styles[op.id] != nil { newerStyles[op.id] = op.style }
        }
    }
    /// `other` as it applies ahead of this batch: a style it gives a node
    /// this one styles keeps, as it shows (`committed`), each value this
    /// batch changes or removes and `other` carries as this batch left it,
    /// so nothing this batch writes or removes shows before its reveal, and
    /// a newer sample shows at once.
    func withheld(_ other: Batch, committed: (UInt32) -> NodeStyle?) -> Batch {
        var shown = other
        for i in shown.ops.indices where shown.ops[i].op == .style {
            guard let mine = styles[shown.ops[i].id] else { continue }
            let before = committed(shown.ops[i].id) ?? [:]
            for key in Set(mine.keys).union(before.keys) where before[key] != mine[key] && shown.ops[i].style[key] == mine[key] { shown.ops[i].style[key] = before[key] }
        }
        return shown
    }
}

/// A display frame for each slice of preparation, and one more for the
/// reveal, so the last slice's drawing commits before it.
final class StageFrames: NSObject {
    private weak var session: ExactSession?
    #if !canImport(UIKit)
    private var link: CADisplayLink?
    #endif
    init(_ session: ExactSession) { self.session = session }

    func start() {
        #if canImport(UIKit)
        FrameClock.shared.want(self, .stage) { [weak self] _ in self?.session?.stepStage() }
        #else
        guard link == nil, let viewport = session?.presenter.viewport else { return }
        let l = viewport.displayLink(target: self, selector: #selector(tick(_:)))
        l.add(to: .main, forMode: .common)
        link = l
        #endif
    }

    func stop() {
        #if canImport(UIKit)
        FrameClock.shared.drop(self)
        #else
        link?.invalidate()
        link = nil
        #endif
    }

    #if !canImport(UIKit)
    @objc private func tick(_ link: CADisplayLink) { session?.stepStage() }
    #endif
}

/// Views a reveal took out, removed a few each turn, innermost first so
/// that each removal is one view's.
final class Teardown {
    private var pending: [NodeView?] = []
    private var next = 0
    private var tops = Set<ObjectIdentifier>()
    /// Released boxes whose caches go as each is removed.
    private var forgetting: [ObjectIdentifier: NodeView] = [:]
    private var scheduled = false
    private static let turnSeconds = 0.0015

    func add(_ top: NodeView, destroying: Set<UInt32>) {
        // A control owns its native subviews. A surviving Exact child may move elsewhere at the reveal.
        func walk(_ node: NodeView) {
            for child in node.structuralChildren where destroying.contains(child.id) { walk(child) }
            pending.append(node)
        }
        walk(top)
        tops.insert(ObjectIdentifier(top))
        schedule()
    }

    /// Takes every waiting view down now: the presenter's tree is being replaced.
    func finish() {
        for case let view? in pending[next...] { view.removeFromSuperview() }
        for node in forgetting.values { node.forget() }
        pending = []; next = 0; tops = []; forgetting = [:]
    }

    func forgetAsRemoved(_ node: NodeView) {
        forgetting[ObjectIdentifier(node)] = node
        schedule()
    }

    /// Whether `view` is a top that waits here to be removed.
    func holds(_ view: PlatformView) -> Bool { !tops.isEmpty && tops.contains(ObjectIdentifier(view)) }

    private func schedule() {
        guard !scheduled, next < pending.count || !forgetting.isEmpty else { return }
        scheduled = true
        DispatchQueue.main.async { [weak self] in self?.step() }
    }

    private func step() {
        scheduled = false
        let deadline = CACurrentMediaTime() + Self.turnSeconds
        while next < pending.count, CACurrentMediaTime() < deadline {
            // Released as it goes: freeing them all at once costs AppKit a frame too.
            if let view = pending[next] {
                forgetting.removeValue(forKey: ObjectIdentifier(view))?.forget()
                view.removeFromSuperview()
                tops.remove(ObjectIdentifier(view))
            }
            pending[next] = nil
            next += 1
        }
        if next == pending.count {
            pending = []; next = 0
            // A box whose top was removed at once rather than here.
            for node in forgetting.values { node.forget() }
            forgetting = [:]
        }
        schedule()
    }
}

extension ExactSession {
    /// Whether a batch the runtime made still waits to apply: a stage being prepared or one behind it.
    package var batchesWaiting: Bool { stage != nil || !pending.isEmpty }

    /// Begins preparing a large batch (LLP 1072 T13): true when `batch` is
    /// staged, or was prepared and published here. Under the agent's clock
    /// and `EXACT_FILL_SYNC` the same preparation runs to its end at once
    /// (T9, T12).
    func stages(_ batch: Batch) -> Bool {
        guard stage == nil, !applying, !fillLanding, !presenter.revealsStage, let plan = presenter.stagePlan(batch) else { return false }
        plan.epoch = viewsEpoch
        stage = plan
        // Its timers and frames are the runtime's latest: a deadline before the reveal is asked on time.
        followRunner(batch, shown: false)
        if clock != nil || !ExactEnv.postsLiveWork { finishStage(); return true }
        // Motion its commits began on its nodes starts at the reveal; what already runs runs on (LLP 1003.001).
        runtime.unshown(Array(plan.structural.union(plan.written.keys)))
        let frames = StageFrames(self)
        plan.frames = frames
        frames.start()
        stepStage()
        return true
    }

    /// One display frame's preparation; on the frame after the last, the reveal.
    func stepStage() {
        guard let plan = stage else { return }
        guard plan.epoch == viewsEpoch else { return cancelStage() }
        if plan.preparedAll { return finishStage() }
        defer { if plan.preparedAll { plan.prepareReveal() } }
        let began = CACurrentMediaTime()
        presenter.prepare(plan, until: began + plan.sliceSeconds)
        // Draw what the slice made now, to spend the next slice by it.
        let prepared = CACurrentMediaTime()
        CATransaction.flush()
        let drawn = CACurrentMediaTime(), spent = max(prepared - began, 0.0001)
        let fits = StagePlan.frameSeconds * spent / (drawn - began)
        plan.sliceSeconds = min(max(fits, 0.00025), min(plan.sliceSeconds * 2, StagePlan.frameSeconds))
    }

    /// Prepares what is left and publishes the batch, then applies what
    /// waited and what the owner published (`landFill`, T4). The order holds
    /// through the reveal: what the reveal makes (an autofocus's batch, a
    /// due advance) is numbered after what waited and applies after it.
    /// Reached from the frame after preparation ends, and from a read that
    /// must see the batch whole (`drainFill`, T9).
    func finishStage() {
        guard let plan = stage else { return }
        guard plan.epoch == viewsEpoch else { return cancelStage() }
        while !plan.preparedAll { presenter.prepare(plan, until: .infinity) }
        plan.frames?.stop()
        stage = nil
        let outer = landing
        landing = true
        presenter.revealing = plan
        applyNow(plan.reveal())
        presenter.revealing = nil
        if plan.frames != nil { runtime.unshown([]) }
        landing = outer
        landFill()
    }

    /// Every batch the session applies comes here (`order`).
    package func apply(_ batch: Batch) { order(PendingBatch(batch: batch)) }
    /// `apply` for a batch that may change nothing (`applyUnlessEmptyNow`).
    func applyUnlessEmpty(_ batch: Batch, scrolled: Bool = false, resized: Bool = false) {
        order(PendingBatch(batch: batch, unlessEmpty: true, scrolled: scrolled, resized: resized))
    }

    /// Batches apply in the order the runtime made them (`Batch.sequence`,
    /// LLP 1072 T13), per node: one goes ahead of an earlier batch that has
    /// not applied, a staged one included, only where the two commute
    /// (`BatchFootprint.admits`, `StagePlan.disjoint`). One applies at once
    /// when it may go ahead of everything made before it and no batch is
    /// being applied; else it waits in `pending`, which `landFill` applies
    /// merged with the owner's publications by number, after the batch
    /// being applied (`applyNow`). A handler's event results reach the
    /// presenter here, whenever the batch applies (`takeEventResults`).
    func order(_ entry: PendingBatch) {
        var entry = entry
        guard state != .destroyed, !entry.batch.busy, !entry.batch.held else { return }
        entry.cause = coversCause
        takeEventResults(&entry.batch)
        if entry.batch.sequence == 0 { entry.batch.sequence = runtime.sequence.next() }
        let made = entry.batch.sequence
        if !landing, !insideApply, published.earliest.map({ $0 > made }) ?? true {
            let mine = footprint(entry.batch)
            if passes(entry.batch), pending.allSatisfy({ $0.batch.sequence > made || $0.footprint.admits(mine) }) { supersede(mine, before: made); return land(entry) }
        }
        wait(entry)
        if !landing, !insideApply { landFill() }
    }
    /// Queues a batch in number order (`landFill` applies it), with the views epoch it belongs to.
    /// A motion sample it carries supersedes one that waits before it, so a
    /// sample that waits frame after frame is only the newest.
    func wait(_ entry: PendingBatch) {
        var entry = entry
        entry.footprint = footprint(entry.batch)
        entry.epoch = viewsEpoch
        supersede(entry.footprint, before: entry.batch.sequence)
        let at = pending.lastIndex { $0.batch.sequence <= entry.batch.sequence }.map { $0 + 1 } ?? 0
        pending.insert(entry, at: at)
    }
    /// The first waiting batch made before `limit` that may go ahead of every
    /// earlier one still waiting and of a staged batch (`landFill`), or the
    /// part of a tick that may (`divide`).
    func nextLanding(before limit: UInt64) -> Int? {
        for entry in pending where entry.epoch != viewsEpoch { entry.dropped?() }
        pending.removeAll { $0.epoch != viewsEpoch }
        var earlier: [BatchFootprint] = []
        for (index, entry) in pending.enumerated() {
            guard entry.batch.sequence < limit else { break }
            if admitted(entry.batch, entry.footprint, after: earlier) || divide(index, after: earlier) {
                supersede(pending[index].footprint, before: entry.batch.sequence)
                return index
            }
            earlier.append(entry.footprint)
        }
        return nil
    }
    func admitted(_ batch: Batch, _ print: BatchFootprint, after earlier: [BatchFootprint]) -> Bool {
        passes(batch) && earlier.allSatisfy { $0.admits(print) }
    }
    /// A tick that commits nothing is per node: each node's samples, motion
    /// (`present`) or paint (`style`, LLP 1055.000 D6), and canvas draws
    /// (LLP 1003.001, LLP 1072 §3.5). The nodes whose part may go ahead split
    /// from it, before it at `index`, and the rest waits: true when there is
    /// such a part.
    func divide(_ index: Int, after earlier: [BatchFootprint]) -> Bool {
        let tick = pending[index]
        guard tick.clock == .tick, tick.batch.seq == nil, tick.batch.controlReads?.isEmpty != false,
              tick.batch.ops.allSatisfy({ BatchFootprint.sampleOps.contains($0.op) }) else { return false }
        let free = Set(Dictionary(grouping: tick.batch.ops, by: \.id).compactMap { id, ops in
            var part = tick.batch
            part.ops = ops
            return admitted(part, footprint(part), after: earlier) ? id : nil
        })
        guard !free.isEmpty, free.count < Set(tick.batch.ops.map(\.id)).count else { return false }
        var now = tick, rest = tick
        now.batch.ops.removeAll { !free.contains($0.id) }
        now.footprint = footprint(now.batch)
        rest.batch.ops.removeAll { free.contains($0.id) }
        rest.footprint = footprint(rest.batch)
        rest.landed = nil; rest.dropped = nil
        pending[index] = rest
        pending.insert(now, at: index)
        return true
    }
    /// A batch going ahead of those made before it that wait, or waiting after
    /// them: a motion sample it carries is newer than theirs of the same
    /// property, and a style it gives a node newer than a tick's paint sample
    /// of it, which they then leave.
    func supersede(_ later: BatchFootprint, before made: UInt64) {
        guard !later.samples.isEmpty || !later.styled.isEmpty else { return }
        for i in pending.indices where pending[i].batch.sequence < made {
            let count = pending[i].batch.ops.count, paints = pending[i].footprint.paints
            pending[i].batch.ops.removeAll {
                $0.op == .present ? later.samples.contains(BatchFootprint.sample($0)) : $0.op == .style && paints.contains($0.id) && later.styled.contains($0.id)
            }
            if pending[i].batch.ops.count != count { pending[i].footprint = footprint(pending[i].batch) }
        }
    }
    func footprint(_ batch: Batch) -> BatchFootprint { BatchFootprint(batch) { [presenter] in presenter.holds($0, $1) } }
    /// Whether a batch may apply beside a staged batch: always, but beside one only where it leaves that one
    /// alone. A native control's state comes with the batch that changes it (`Batch.controlReads`), and is
    /// written as any property is. A 2D canvas's draw lists are drawn at the runtime's state, the staged batch
    /// included: a draw of a canvas whose node, props, style or lists the staged batch changes waits for
    /// the reveal, and any other canvas draws now.
    func passes(_ batch: Batch) -> Bool {
        guard let plan = stage else { return true }
        guard plan.disjoint(batch) else { return false }
        return !batch.ops.contains { $0.op == .canvas2d && (plan.written[$0.id] != nil || plan.styles[$0.id] != nil) }
    }
    /// Applies a batch that may (`order`): beside a staged batch, with what it
    /// passes recorded (`StagePlan.passed`, `withheld`).
    func land(_ entry: PendingBatch) {
        var entry = entry
        if let plan = stage {
            plan.passed(entry.batch)
            entry.batch = plan.withheld(entry.batch) { [unowned self] in presenter.views[$0]?.style }
        }
        inCoversChain(entry.cause) {
            if entry.unlessEmpty { applyUnlessEmptyNow(entry.batch, scrolled: entry.scrolled, resized: entry.resized, beside: stage) } else { applyNow(entry.batch, beside: stage) }
            entry.landed?()
        }
    }
    /// Whether a tick's or an advance's batch waits. The next advance is held
    /// until it applies, as a slice in flight holds it; a tick is not, since
    /// only its samples on what waits wait (`divide`), and the next tick's
    /// supersede them (`wait`).
    func waits(_ kind: Batch.ClockWork) -> Bool { pending.contains { $0.clock == kind } }

    /// A key, pointer or `beforeinput` handler's `preventDefault()` and
    /// `stopPropagation()` reach the presenter as its batch comes, staged or
    /// waiting or not: the dispatch reads them as it returns (`KeyEvents`,
    /// `FieldInput`).
    func takeEventResults(_ batch: inout Batch) {
        func result(_ op: BatchOp) -> String? {
            guard op.op == .command, let name = op.payload["name"] as? String, name == "preventDefault" || name == "stopPropagation" else { return nil }
            return name
        }
        guard batch.ops.contains(where: { result($0) != nil }) else { return }
        for op in batch.ops {
            if result(op) == "preventDefault" { presenter.defaultPrevented = true } else if result(op) == "stopPropagation" { presenter.propagationStopped = true }
        }
        batch.ops.removeAll { result($0) != nil }
    }

    /// Ends a stage the session's tree no longer has: a reset or reboot
    /// replaces that tree, and destruction ends it. What it prepared and
    /// every batch that waits belonged to the old tree: they are dropped
    /// without applying.
    func cancelStage() {
        let plan = stage
        stage = nil
        plan?.frames?.stop()
        if plan?.frames != nil { runtime.unshown([]) }
        presenter.abandon(plan)
        let dropped = pending
        pending = []
        newestApplied = nil; newestCommit = 0
        for entry in dropped { entry.dropped?() }
    }

    /// What a batch says of the runner as a whole as of when it was made:
    /// its timers and the frames it wants, and whether a canvas draw is owed.
    /// The session holds the newest applied batch's, so a batch applied after
    /// a newer one (a staged batch's reveal) leaves the newer one's in place.
    /// A batch that stages gives only its schedule as it begins (`shown`
    /// false), so a deadline during preparation is asked on time; the rest
    /// is the session's once it shows. A batch the host made unnumbered is
    /// the newest. The hatches' frames report the last commit of the newest
    /// applied batch that has one (`ExactFrame.seq`): most batches, a tick's
    /// among them, commit nothing.
    func followRunner(_ batch: Batch, shown: Bool = true) {
        var newest = batch
        if let applied = newestApplied, batch.sequence != 0, applied.sequence > batch.sequence { newest = applied } else if shown { newestApplied = batch }
        if shown, let q = batch.seq, batch.sequence == 0 || batch.sequence > newestCommit {
            natives.hatchSeq(q.1)
            newestCommit = max(newestCommit, batch.sequence)
        }
        frames.motion = newest.motion
        frames.spatial = newest.spatial
        frames.presentOnly = newest.presentOnly
        frames.canvas2d = newest.canvas
        frames.tasks = newest.frames
        updateTimerState(newest)
        timerDue = newest.timerDueMs; timerLanding = newest.landing
        scheduleClock(due: newest.timerDueMs)
        guard shown else { return }
        canvasOwed = newest.canvasOwed
        if canvasOwed { askCanvasDraw() }
    }

    /// The one door for host input while a stage is open (LLP 1072 T13).
    /// Every input reaches the runtime as it comes, a press or a key as a
    /// scroll does, and its batch is ordered like any other (`order`):
    /// preparation goes on in its slices, and a batch that cannot pass
    /// applies after the reveal. Platform feedback that needs no runtime (a
    /// press's highlight) does not wait, and what the platform does by
    /// default reads the committed tree, which is what shows. An input aimed
    /// at a node the stage removes is refused and logged, as T6 refuses
    /// input on a view a commit removed.
    func admit(_ id: UInt32?, _ input: () -> Void) {
        if admits(id) { input() }
    }
    func admits(_ id: UInt32?) -> Bool {
        guard let plan = stage, let id, plan.destroyed.contains(id) else { return true }
        log("host: input: #\(id) refused: a staged batch removed the node")
        return false
    }
}

/// A batch on its way to the presenter (`ExactSession.order`), and what its
/// caller does once it has applied (a publication's landing) or if it is
/// dropped (a reset's).
struct PendingBatch {
    var batch: Batch
    var unlessEmpty = false, scrolled = false, resized = false
    /// A frame's tick or a timer's advance, as the bridge read it: no path to `order` can leave it off.
    var clock: Batch.ClockWork? { batch.clockWork }
    var landed: (() -> Void)? = nil
    var dropped: (() -> Void)? = nil
    var footprint = BatchFootprint()
    /// A navigation correction keeps its chain across the wait. An owner publication has none.
    var cause: AnyObject?
    var epoch = 0
}

/// What a batch restructures and what it writes, for whether a later batch
/// may apply before it (LLP 1072 T13): two that touch disjoint state
/// commute, so either order shows a state one order gives.
struct BatchFootprint {
    /// Ops that change what ids refer to, or what a later batch's ids reach.
    static let structuralOps: Set<BatchOp.Kind> = [.create, .destroy, .children, .roots, .renew, .exit, .flight, .land, .reorder]
    /// Nodes it makes, destroys or moves, and parents whose children it changes.
    private(set) var restructured = Set<UInt32>()
    /// What it writes on each node (`UInt32.max` for what belongs to no node).
    private(set) var written: [UInt32: Set<String>] = [:]
    /// Whether it may go ahead of anything: it restructures nothing and runs
    /// no command, which may name what an earlier batch makes.
    private(set) var movable = true
    /// The motion samples it carries (`present` ops), which a newer one supersedes.
    private(set) var samples = Set<String>()
    static func sample(_ op: BatchOp) -> String { "\(op.id) \(op.property)" }
    /// The nodes it styles; those whose style is a tick's paint sample,
    /// which a newer style supersedes.
    private(set) var styled = Set<UInt32>(), paints = Set<UInt32>()
    /// What a tick that commits nothing carries: samples and canvas draws, each of one node.
    static let sampleOps: Set<BatchOp.Kind> = [.present, .style, .canvas2d]

    init() {}
    /// `holds`: whether a live node is a child of another, as `StagePlan.restructured` asks.
    init(_ batch: Batch, holds: (UInt32, UInt32) -> Bool) {
        restructured = StagePlan.restructured(batch.ops, holds: holds)
        let sampling = batch.clockWork == .tick && batch.seq == nil
        // A native control's state, as the runtime made it with the batch, and the roots' tabs, which replace all.
        for (id, read) in batch.controlReads ?? [:] { written[id, default: []].formUnion(read.parts) }
        if batch.routeInspectors != nil { written[.max, default: []].insert("routeInspectors") }
        if batch.rootTabs != nil { written[.max, default: []].insert("rootTabs") }
        for op in batch.ops {
            if op.op == .style { styled.insert(op.id); if sampling { paints.insert(op.id) } }
            if Self.structuralOps.contains(op.op) { movable = false; continue }
            if op.op == .command { movable = false }
            if op.op == .present { samples.insert(Self.sample(op)) }
            // A style op carries the node's whole style: two never commute.
            let key = op.op == .props ? nil : op.op == .present ? "present." + op.property : op.op.rawValue
            written[op.nodeID ?? .max, default: []].formUnion(key.map { [$0] } ?? Set(op.props.keys.map { "p." + $0 }).union(op.clear.map { "p." + $0 }))
        }
    }

    /// Whether `later` may apply before the batch this is of: where they are
    /// disjoint, or with a newer sample of a property this one samples.
    func admits(_ later: BatchFootprint) -> Bool {
        guard later.movable else { return false }
        for (id, keys) in later.written where restructured.contains(id) || !(written[id]?.isDisjoint(with: keys.filter { !$0.hasPrefix("present.") }) ?? true) { return false }
        return true
    }
}

extension Presenter {
    /// Whether live node `child` is a child of `parent` (`StagePlan.restructured`).
    func holds(_ parent: UInt32, _ child: UInt32) -> Bool { views[child]?.nodeSuperview?.id == parent }
}
