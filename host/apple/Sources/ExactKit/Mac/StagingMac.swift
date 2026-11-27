// Preparing a staged batch on AppKit (LLP 1072 T13, §3.5; `StagedApply.swift`).
#if os(macOS)
import AppKit
import ExactCore
import QuartzCore

extension Presenter {
    var revealsStage: Bool { revealing != nil }

    /// The plan for `batch` when it is large enough to stage. A prepared
    /// subtree joins a plain box in the window: one whose children no host
    /// arranges, and no paragraph, whose runs it would draw at once.
    func stagePlan(_ batch: Batch) -> StagePlan? {
        StagePlan(batch, joins: { id in
            guard let v = views[id] else { return false }
            return v.kind == "view" && v.window != nil && v.offstageAlpha == nil
        }, holds: holds)
    }

    /// Applies `plan`'s prepared ops, then writes what it made to the
    /// accessibility tree, until `deadline`. The views it makes are in the
    /// table only meanwhile, so nothing between slices finds them.
    func prepare(_ plan: StagePlan, until deadline: CFTimeInterval) {
        let wasApplying = applying
        applying = true
        preparing = plan // what it asks to run after its batch waits for the reveal
        views.merge(plan.made) { live, _ in live }
        defer {
            regridMoved()
            for id in plan.made.keys { views.removeValue(forKey: id) }
            preparing = nil
            applying = wasApplying
        }
        let ops = plan.batch.ops
        var reparented = Set<UInt32>(), done = 0
        while plan.cursor < ops.count {
            if done & 15 == 15, CACurrentMediaTime() >= deadline { return }
            let index = plan.cursor
            plan.cursor += 1
            guard plan.prepared[index] else { continue }
            done += 1
            let op = plan.preparingOp(index), id = op.id
            if op.op == .children, plan.large.contains(id) { plan.attachments.append(index); continue }
            if op.op == .children { touched(id, children: true) } else if op.op != .create { touched(id, textChanged: op.op == .props || op.op == .style) }
            switch op.op {
            case .create:
                let v = newView(op, stage: .preparing(plan.publication))
                views[id] = v
                plan.made[id] = v
                plan.madeOrder.append(id)
                elements.created(v)
                if let parent = plan.tops[id], let container = views[parent]?.container {
                    v.holdOffstage()
                    v.prepareToMount()
                    mountTop(v, in: container, parent: parent, plan: plan)
                    regrid(v)
                }
            case .paragraph: applyParagraph(id, op.runs)
            case .props:
                fragmentOffsetsWritten(op)
                views[id]?.applyProps(set: op.props, clear: op.clear)
            case .style:
                guard let v = views[id] else { continue }
                let color = v.style["text_color"]
                v.applyStyle(v.presented(op.style))
                v.refreshPressInheritance()
                if v.surface != nil { v.applySurface() }
                if v.style["text_color"] != color { session?.noteAppearance(v) }
            case .children: placeChildren(op, reparented: &reparented)
            case .frame:
                guard let v = views[id] else { continue }
                applyFrame(v, NSRect(x: op.x, y: op.y, width: op.w, height: op.h))
            case .flow: views[id]?.applyFlow(op.payload["shapes"] as? [[String: Any]] ?? [])
            case .content:
                guard let v = views[id] else { continue }
                v.content = CGSize(width: op.w, height: op.h)
                v.fitScroll()
            case .rank: if let rank = op.rank { views[id]?.setRank(rank) }
            default: break
            }
        }
        if !plan.attachmentsOrdered {
            plan.attachmentsOrdered = true
            plan.attachments.sort { plan.depth[ops[$0].id] ?? 0 < plan.depth[ops[$1].id] ?? 0 }
            // What joins the window draws: the next slice starts small and learns its cost.
            if !plan.attachments.isEmpty { plan.sliceSeconds = StagePlan.firstAttachSeconds; return }
        }
        while let index = plan.attachments.first {
            let op = plan.preparingOp(index)
            if let parent = views[op.id] {
                let container = parent.container
                if plan.attachedChildren == 0 { touched(op.id, children: true) }
                while plan.attachedChildren < op.ids.count {
                    if CACurrentMediaTime() >= deadline { return }
                    let child = views[op.ids[plan.attachedChildren]]
                    plan.attachedChildren += 1
                    guard let child, child.superview !== container else { continue }
                    child.prepareToMount()
                    container.addSubview(child)
                    regrid(child)
                }
                orderChildren(op.ids.compactMap { views[$0] }, in: container)
            }
            plan.attachments.removeFirst()
            plan.attachedChildren = 0
        }
        while plan.accessibilityCursor < plan.madeOrder.count {
            if plan.accessibilityCursor & 15 == 15, CACurrentMediaTime() >= deadline { return }
            let id = plan.madeOrder[plan.accessibilityCursor]
            plan.accessibilityCursor += 1
            guard let node = plan.made[id] else { continue }
            if !prepareAccessibility(node) { plan.revealAccessibility.insert(id) }
            inputGates.prepared(node)
            if node.props["title"] != nil || node.isButton { plan.tips.append(node) }
            if node.hasCursorRects { plan.cursorRects.append(node) }
        }
        prepareRetireScan(plan, until: deadline)
    }

    /// Places a newly created empty top in its reveal slot relative to surviving
    /// siblings so the reveal's `orderChildren` does not reorder a populated subtree.
    private func mountTop(_ view: NodeView, in container: NSView, parent: UInt32, plan: StagePlan) {
        if let ids = plan.parentChildren[parent], let idx = ids.firstIndex(of: view.id) {
            for sibling in ids[(idx + 1)...] where !plan.destroyed.contains(sibling) {
                if let after = views[sibling], after.superview === container {
                    container.addSubview(view, positioned: .below, relativeTo: after)
                    return
                }
            }
            for sibling in ids[..<idx].reversed() where !plan.destroyed.contains(sibling) {
                if let before = views[sibling], before.superview === container {
                    container.addSubview(view, positioned: .above, relativeTo: before)
                    return
                }
            }
        }
        container.addSubview(view)
    }

    /// Classifies plain destroyed descendants under a plain retiring top during
    /// preparation so the reveal retires them in one bulk pass instead of
    /// scanning every per-op batch pass over them.
    private func prepareRetireScan(_ plan: StagePlan, until deadline: CFTimeInterval) {
        guard !plan.retireScanDone else { return }
        let ops = plan.batch.ops
        if plan.retiredOps.isEmpty && !ops.isEmpty { plan.retiredOps = [Bool](repeating: false, count: ops.count) }
        func validTop(of node: NodeView) -> (id: UInt32, view: ObjectIdentifier)? {
            guard let first = node.nodeSuperview, plan.destroyed.contains(first.id) else { return nil }
            var chain: [UInt32] = []
            var cur = first
            while true {
                if let known = plan.retireTopValid[cur.id] {
                    for id in chain { plan.retireTopValid[id] = known }
                    return known
                }
                chain.append(cur.id)
                let ok = !plan.created.contains(cur.id) && !plan.touchedDestroyed.contains(cur.id)
                    && StagePlan.plainKinds.contains(cur.kind)
                    && leaving[cur.id] == nil && !navigation.keepsForExit(cur.id) && cur.superview != nil
                    && cur.props["hatch"] == nil && !blocks.hasEntry(cur.id)
                    && tableGrid.sizes[cur.id] == nil && tableGrid.places[cur.id] == nil
                guard ok else {
                    for id in chain { plan.retireTopValid[id] = .some(nil) }
                    return nil
                }
                if let up = cur.nodeSuperview, plan.destroyed.contains(up.id) { cur = up; continue }
                let top = (id: cur.id, view: ObjectIdentifier(cur))
                for id in chain { plan.retireTopValid[id] = .some(top) }
                return top
            }
        }
        while plan.retireCursor < ops.count {
            if plan.retireCursor & 63 == 63, CACurrentMediaTime() >= deadline { return }
            let index = plan.retireCursor
            plan.retireCursor += 1
            let op = ops[index]
            guard op.op == .destroy else { continue }
            let id = op.id
            guard !plan.created.contains(id), !plan.touchedDestroyed.contains(id),
                  let v = views[id], let superview = v.superview, StagePlan.plainKinds.contains(v.kind) else { continue }
            guard v.scroll == nil, !scrollers.contains(id), listViews[id] == nil, leaving[id] == nil,
                  !navigation.keepsForExit(id), !navigation.tracks(id), v.props["hatch"] == nil, controls.controls[id] == nil else { continue }
            guard subsystemTracked.isEmpty || !subsystemTracked.contains(id) else { continue }
            guard !boxAnimations.tracks(id), !blocks.hasEntry(id), tableGrid.sizes[id] == nil, tableGrid.places[id] == nil else { continue }
            guard !toolbar.reads.nodes.contains(id) else { continue }
            guard let top = validTop(of: v) else { continue }
            plan.retiredOps[index] = true
            plan.retiredPlain.append(StagePlan.RetiredCandidate(id: id, view: ObjectIdentifier(v), superview: ObjectIdentifier(superview)))
            plan.retiredPlainSet.insert(id)
            plan.retiredTops[top.id] = top.view
        }
        plan.retireScanDone = true
    }

    /// Retires the plain descendants that `prepareRetireScan` pulled out of
    /// `reveal.ops`. If every outermost top entered `Teardown` and every
    /// scanned descendant still stands at its scanned view and superview, each
    /// descendant is detached from `views` and marked for `Teardown` to forget
    /// as it leaves; otherwise every candidate falls back to `destroyOne`.
    func retirePlainDescendants(_ plan: StagePlan, retiring: inout Set<UInt32>) {
        guard !plan.retiredPlain.isEmpty else { return }
        let intact = plan.retiredTops.values.allSatisfy({ teardown.holds($0) }) && plan.retiredPlain.allSatisfy { c in
            guard let v = views[c.id], ObjectIdentifier(v) == c.view else { return false }
            return v.superview.map(ObjectIdentifier.init) == c.superview
        }
        guard intact else {
            for c in plan.retiredPlain { destroyOne(c.id, stage: plan, retired: &retiring) }
            return
        }
        let ids = plan.retiredPlainSet
        if let sw = mouseSwipe.candidate?.id, ids.contains(sw) { mouseSwipe.retire(sw) }
        if let lp = mouseLayoutPan.candidate?.id, ids.contains(lp) { mouseLayoutPan.retire(lp) }
        if let hd = mouseDrags.height.armed?.id, ids.contains(hd) { mouseDrags.height.retire(hd) }
        if let td = mouseDrags.transform.armed?.id, ids.contains(td) { mouseDrags.transform.retire(td) }
        if let rm = reorders.mouse.armed?.id, ids.contains(rm) { reorders.mouse.retire(rm) }
        focusEvents.removed(all: ids)
        if let ph = pointerHeld, ids.contains(ph) { pointerHeld = nil }
        if !pointerPath.isEmpty { pointerPath.removeAll { ids.contains($0) } }
        MainActor.assumeIsolated { if let held = HeldState.view, held.presenter === self, ids.contains(held.id) { held.holdGone() } }
        let bubbled = mouseChain.bubbledPress
        let hasReader = session?.text.readerParagraphs.isEmpty == false
        let hasAutofocus = !autofocusProcessed.isEmpty
        let hasPendingScrolls = !pendingScrolls.isEmpty
        let hasListTravel = !listTravel.isEmpty
        let hasListFill = !listFillCosts.isEmpty
        retiring.reserveCapacity(retiring.count + plan.retiredPlain.count)
        for c in plan.retiredPlain {
            let id = c.id
            guard let gone = views.removeValue(forKey: id) else { continue }
            if let bubbled { bubbled.cancel(in: gone) }
            if hasReader { session?.text.readerParagraphs.removeValue(forKey: id) }
            if hasAutofocus { autofocusProcessed.remove(ObjectIdentifier(gone)) }
            if hasPendingScrolls { pendingScrolls.remove(id) }
            if hasListTravel { listTravel.removeValue(forKey: id) }
            if hasListFill { listFillCosts.removeValue(forKey: id) }
            gone.stage = .retired
            gone.needsTeardownForget = true
            if gone.hasToolTip { gone.setToolTip(nil) }
            if gone.hasCursorRects { gone.window?.invalidateCursorRects(for: gone) }
            retiring.insert(id)
        }
    }

    func destroyOne(_ id: UInt32, stage: StagePlan?, retired: inout Set<UInt32>) {
        elements.destroyed(id)
        blocks.destroyed(id)
        stickies.forget(id)
        tableGrid.forget(id)
        flightHost.forget(id)
        views[id]?.holdGone()
        if endExit(id) { return }
        // A route that left with its exit to play stays for it (RouteTransitionMac.swift).
        if navigation.keepsForExit(id) { beginExit(id); return }
        // A plain box under one the reveal takes down lets its caches go as it leaves (`Teardown`).
        let underRetired = stage.map { stage in views[id].map { StagePlan.plainKinds.contains($0.kind) && $0.nodeSuperview.map { stage.destroyed.contains($0.id) } == true } == true } == true
        if let gone = release(id, forget: !underRetired, retiring: &retired) {
            if underRetired { teardown.forgetAsRemoved(gone) }
            if !(stage.map { retire(gone, $0) } ?? false) { gone.removeFromSuperview() }
        }
    }

    /// The reveal: what preparation made joins the committed tree, with the
    /// tooltips, cursor rectangles and after-batch work it held back.
    func publish(_ plan: StagePlan) {
        plan.publication.committed = true
        for view in plan.tips { settleTip(view) }
        for view in plan.cursorRects { view.window?.invalidateCursorRects(for: view) }
        afterBatchWork.append(contentsOf: plan.afterReveal)
        plan.afterReveal = []
        // What a box lays out from its committed children (a form row's bezel) counts the new ones.
        for parent in Set(plan.tops.values) where views[parent]?.style["appearance"] != nil { views[parent]?.needsLayout = true }
    }

    /// Ends a stage before its reveal (`ExactSession.cancelStage`): what it
    /// made leaves the window, and the views a reveal took down go at once.
    func abandon(_ plan: StagePlan?) {
        teardown.finish()
        guard let plan else { return }
        for id in plan.madeOrder {
            guard let v = plan.made[id] else { continue }
            elements.destroyed(id)
            v.forget()
        }
        for top in plan.tops.keys { plan.made[top]?.removeFromSuperview() }
    }

    /// Whether the reveal takes `view`, which it destroyed, down later. A
    /// destroyed box under another goes with it; the outermost is held off
    /// screen and removed a few views a turn (`Teardown`): one removal of
    /// a 2,400-view subtree costs AppKit about 13 ms.
    func retire(_ view: NodeView, _ plan: StagePlan) -> Bool {
        view.stage = .retired
        if view.hasToolTip { view.setToolTip(nil) }
        if view.hasCursorRects { view.window?.invalidateCursorRects(for: view) }
        if let up = view.nodeSuperview, plan.destroyed.contains(up.id) { return true }
        guard view.superview != nil else { return false }
        // A field being edited in it ends editing and gives up the keyboard, as removing it would.
        if let window = view.window, let responder = window.firstResponder as? NSView, responder.isDescendant(of: view), !window.makeFirstResponder(nil) {
            window.endEditing(for: nil)
        }
        view.holdOffstage()
        teardown.add(view)
        return true
    }
}

/// Views a reveal took out, removed a few each turn, innermost first so
/// that each removal is one view's.
final class Teardown {
    private struct Branch {
        let node: NodeView
        var children: [NodeView]
        init(_ node: NodeView) {
            self.node = node
            children = node.structuralChildren
        }
    }
    private var roots: [NodeView?] = []
    private var next = 0
    private var path: [Branch] = []
    private var tops = Set<ObjectIdentifier>()
    /// Released boxes whose caches go as each is removed.
    private var forgetting: [ObjectIdentifier: NodeView] = [:]
    private var scheduled = false
    private static let turnSeconds = 0.0015

    func add(_ top: NodeView) {
        roots.append(top)
        tops.insert(ObjectIdentifier(top))
        schedule()
    }

    /// Takes every waiting view down now: the presenter's tree is being replaced.
    func finish() {
        while hasWork { advance() }
    }

    func forgetAsRemoved(_ node: NodeView) {
        forgetting[ObjectIdentifier(node)] = node
        schedule()
    }

    /// Whether `view` is a top that waits here to be removed.
    func holds(_ view: NSView) -> Bool { !tops.isEmpty && tops.contains(ObjectIdentifier(view)) }
    func holds(_ identity: ObjectIdentifier) -> Bool { !tops.isEmpty && tops.contains(identity) }

    private var hasWork: Bool { !path.isEmpty || next < roots.count || !forgetting.isEmpty }

    private func schedule() {
        guard !scheduled, hasWork else { return }
        scheduled = true
        DispatchQueue.main.async { [weak self] in self?.step() }
    }

    private func step() {
        scheduled = false
        let deadline = CACurrentMediaTime() + Self.turnSeconds
        while hasWork, CACurrentMediaTime() < deadline { advance() }
        schedule()
    }

    /// Discover one child or remove one leaf. Walking the whole outgoing tree during
    /// the reveal would still hold the display link, even if the removals happened later.
    private func advance() {
        if path.isEmpty {
            guard next < roots.count, let top = roots[next] else {
                // A box whose top was removed at once rather than here.
                forgetting.popFirst()?.value.forget()
                return
            }
            roots[next] = nil
            next += 1
            if next == roots.count { roots = []; next = 0 }
            path.append(Branch(top))
        }
        let last = path.count - 1
        if let child = path[last].children.popLast() {
            // A child may have joined the new tree since this branch was opened.
            if case .retired = child.stage, child.nodeSuperview === path[last].node {
                path.append(Branch(child))
            }
            return
        }
        let node = path.removeLast().node
        let parent = path.last?.node
        if node.needsTeardownForget { node.needsTeardownForget = false; node.forget() }
        else { forgetting.removeValue(forKey: ObjectIdentifier(node))?.forget() }
        if parent == nil || node.nodeSuperview === parent { node.removeFromSuperview() }
        tops.remove(ObjectIdentifier(node))
    }
}

extension NodeView {
    enum Stage { case committed, preparing(StagePlan.Publication), retired }
    /// Outside the committed tree during preparation or after retirement.
    var held: Bool {
        switch stage {
        case .committed: return false
        case .preparing(let publication): return !publication.committed
        case .retired: return true
        }
    }

    /// Drawn but unseen, unhit and off the accessibility tree, until
    /// `releaseOffstage`; what is written to `alphaValue` meanwhile is kept.
    /// The box stays where it stands: moving a 2,400-view subtree to
    /// another superview in the same window costs AppKit about 15 ms and
    /// draws all of it again.
    func holdOffstage() {
        guard offstageAlpha == nil else { return }
        let alpha = alphaValue
        super.alphaValue = 0
        offstageAlpha = alpha
    }

    func releaseOffstage() {
        guard let alpha = offstageAlpha else { return }
        offstageAlpha = nil
        alphaValue = alpha
    }

    func prepareToMount() {
        guard kind == "text" else { return }
        wantsLayer = true
        applyShadow()
        if let hide = boxFilter?.hide { layer?.mask = hide } else { applyBoxMask() }
        setPaintPosition(paintZPosition)
        applyTransform()
    }
}
#endif
