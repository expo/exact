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
                let v = newView(op)
                v.held = true
                views[id] = v
                plan.made[id] = v
                plan.madeOrder.append(id)
                elements.created(v)
                if let parent = plan.tops[id], let container = views[parent]?.container {
                    v.holdOffstage()
                    v.prepareToMount()
                    container.addSubview(v)
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
        }
    }

    /// The reveal: what preparation made joins the committed tree, with the
    /// tooltips, cursor rectangles and after-batch work it held back.
    func publish(_ plan: StagePlan) {
        for id in plan.madeOrder {
            guard let v = plan.made[id] else { continue }
            v.held = false
            settleTip(v)
            v.window?.invalidateCursorRects(for: v)
        }
        afterBatchWork.append(contentsOf: plan.afterReveal)
        plan.afterReveal = []
        // What a box lays out from its committed children (a form row's bezel) counts the new ones.
        for parent in Set(plan.tops.values) { views[parent]?.needsLayout = true }
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
        view.held = true
        view.toolTip = nil
        view.window?.invalidateCursorRects(for: view)
        if let up = view.nodeSuperview, plan.destroyed.contains(up.id) { return true }
        guard view.superview != nil else { return false }
        // A field being edited in it ends editing and gives up the keyboard, as removing it would.
        if let window = view.window, let responder = window.firstResponder as? NSView, responder.isDescendant(of: view), !window.makeFirstResponder(nil) {
            window.endEditing(for: nil)
        }
        view.holdOffstage()
        teardown.add(view, destroying: plan.destroyed)
        return true
    }
}

extension NodeView {
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
