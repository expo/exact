// The drag-in capability on UIKit (LLP 1094.001 D4, the roster's `drag_in`):
// a drag from another app, or from this one, dropped on a node that declares
// `drop`, carrying files, plain text or a link. An app's composition links
// this module when its plan hears `drop`, `dragenter` or `dragleave`, and
// installs it before any session is made; the core reaches it only through
// `DragInLink`.
//
// Each node that hears `drop`, `dragenter` or `dragleave` has a
// `UIDropInteraction`, wherever its view is (a route's, a sheet's), each one
// session at a time (`allowsSimultaneousDropSessions` stays false), and all
// report to one tracker per presenter. At each report the tracker finds the
// nodes under the drag up the authored tree, as the Mac's does
// (FileDragMac.swift): those that hear `dragenter` or `dragleave`, innermost
// first, and the innermost enabled node with `drop`. A text field keeps
// UIKit's own text drop, which takes text, links and text files and refuses
// other files, which then reach the node's interaction; its
// `textDropDelegate` reports the drag's location here. While a field's own
// node declares `drop`, its text drop is taken off, so every drag over the
// field reaches the node (D2).
#if os(iOS)
import ExactKit
import UIKit

public enum ExactDragIn {
    public static func install() { DragInLink.installed = DragInHost() }
}

final class DragInHost: DragInCapability {
    func target(_ presenter: Presenter) -> DragInTarget { DropTarget(presenter) }
}

final class DropTarget: NSObject, DragInTarget, UIDropInteractionDelegate, UITextDropDelegate {
    private weak var presenter: Presenter?
    /// The nodes the drag is over that hear `dragenter` or `dragleave`,
    /// innermost first, and its last window point.
    private var entered: [NodeView] = []
    private var last = CGPoint.zero
    /// Bumped by every report: an exit's clear is stale once the drag is
    /// reported again.
    private var turn = 0
    /// The nodes a drop still loading its items was over, each to its count
    /// of such drops. A newer drag's `dragenter` there is held (`held`) until
    /// those drops land, or until the newer drag drops, so a waiting drop's
    /// action cannot clear what the newer drag drew; the newer drag's leave
    /// cancels its held enter.
    private var waiting: [ObjectIdentifier: Int] = [:]
    private var held: Set<ObjectIdentifier> = []
    /// Each field's own text drop while its node declares `drop`, taken off
    /// it and kept here: UIKit answers `textDropInteraction` only while the
    /// interaction is on the field.
    private let parked = NSMapTable<UIView, UIInteraction>.weakToStrongObjects()

    private static let events: Set<String> = ["drop", "dragenter", "dragleave"]

    init(_ presenter: Presenter) {
        self.presenter = presenter
        super.init()
    }

    func reset() {
        entered = []
        waiting = [:]
        held = []
    }

    /// A node that hears a drag's events has an interaction of ours, and one
    /// that no longer does has none; with none in the window, its fields stay
    /// as UIKit makes them.
    func sync() {
        guard let presenter else { return }
        var heard = false
        for view in presenter.views.values {
            let hears = !view.handlers.isDisjoint(with: Self.events), ours = view.interactions.first { ($0 as? UIDropInteraction)?.delegate === self }
            if hears, ours == nil { view.addInteraction(UIDropInteraction(delegate: self)) } else if !hears, let ours { view.removeInteraction(ours) }
            heard = heard || hears
        }
        guard heard else { return }
        for view in presenter.views.values {
            for field in [view.field as (UIView & UITextDroppable)?, view.textArea].compactMap({ $0 }) {
                field.textDropDelegate = self
                if view.handlers.contains("drop"), let own = field.textDropInteraction {
                    field.removeInteraction(own)
                    parked.setObject(own, forKey: field)
                } else if !view.handlers.contains("drop"), let own = parked.object(forKey: field) {
                    field.addInteraction(own)
                    parked.removeObject(forKey: field)
                }
            }
        }
    }

    func dropPoints(of view: NodeView) -> [UIAccessibilityLocationDescriptor]? {
        guard view.handlers.contains("drop"), !view.inert, !view.formDisabled else { return nil }
        return [UIAccessibilityLocationDescriptor(name: view.accessibleName, view: view)]
    }

    // MARK: A node's interaction

    func dropInteraction(_ interaction: UIDropInteraction, canHandle session: UIDropSession) -> Bool { !session.items.isEmpty }

    func dropInteraction(_ interaction: UIDropInteraction, sessionDidEnter session: UIDropSession) { moved(session) }

    func dropInteraction(_ interaction: UIDropInteraction, sessionDidUpdate session: UIDropSession) -> UIDropProposal {
        let point = moved(session)
        return UIDropProposal(operation: nodes(at: point).target != nil ? .copy : .forbidden)
    }

    func dropInteraction(_ interaction: UIDropInteraction, sessionDidExit session: UIDropSession) { exited() }

    func dropInteraction(_ interaction: UIDropInteraction, sessionDidEnd session: UIDropSession) { ended(dropped: false) }

    func dropInteraction(_ interaction: UIDropInteraction, performDrop session: UIDropSession) {
        guard let presenter else { return }
        let point = location(session)
        guard let target = nodes(at: point).target else {
            released(into: nil, at: point)
            return
        }
        // The records are the release's: the point in each box as it dropped
        let line = Self.line(target, point), id = target.id
        let leaves = released(into: target, at: point)
        // Every item loads before the action runs (UIKit starts each load here)
        DropItems.read(session.items.map(\.itemProvider), for: presenter.session) { [weak self] files, text, url in
            guard let self else { return }
            // A node gone, or no longer hearing `drop`, while its items loaded takes nothing
            let live = presenter.views[id] === target && target.handlers.contains("drop")
            if !(live && self.deliver(files, text, url, to: target, line: line)) { self.leave(leaves.records) }
            self.lands(leaves.nodes)
        }
    }

    // MARK: A field's own text drop

    func textDroppableView(_ textDroppableView: UIView & UITextDroppable, dropSessionDidEnter session: UIDropSession) { moved(session) }

    func textDroppableView(_ textDroppableView: UIView & UITextDroppable, dropSessionDidUpdate session: UIDropSession) { moved(session) }

    func textDroppableView(_ textDroppableView: UIView & UITextDroppable, dropSessionDidExit session: UIDropSession) { exited() }

    func textDroppableView(_ textDroppableView: UIView & UITextDroppable, dropSessionDidEnd session: UIDropSession) { ended(dropped: false) }

    /// A drop the field takes is a leave at the nodes the drag was over.
    func textDroppableView(_ textDroppableView: UIView & UITextDroppable, willPerformDrop drop: UITextDropRequest) {
        _ = released(into: nil, at: last)
    }

    // MARK: The agent's drag (`tap <target> drop|dragenter|dragleave`, D6)

    func agentDrag(_ req: [String: Any], over view: NodeView) -> [String: Any] {
        let at = (req["at"] as? [Double]).flatMap { $0.count == 2 ? CGPoint(x: $0[0], y: $0[1]) : nil }
        let point = view.convert(at ?? CGPoint(x: view.bounds.midX, y: view.bounds.midY), to: nil)
        if req["dragleave"] != nil {
            ended(dropped: false)
            return ["tapped": Int(view.id), "dragleave": true, "delivery": "presenter"]
        }
        let kind = req["drop"] != nil ? "drop" : "dragenter"
        let paths = req[kind] as? [String] ?? [], text = req["text"] as? String ?? "", url = req["url"] as? String ?? ""
        guard !paths.isEmpty || !text.isEmpty || !url.isEmpty else { return ["malformed": true, "error": "\(kind): expected the dropped paths, --text or --url"] }
        let hit = nodes(at: point)
        turn &+= 1
        update(hit.chain, at: point)
        if kind == "dragenter" { return ["tapped": Int(view.id), "at": at.map { [$0.x, $0.y] } ?? [], "dragenter": paths.count, "delivery": "presenter"] }
        guard let target = hit.target else {
            ended(dropped: false)
            return ["error": "view \(view.id) takes no drop: nothing under it declares `drop`"]
        }
        // UIKit's field inserts text and links itself, which the agent does not
        if paths.isEmpty, let owner = hit.fieldOwner, owner !== target {
            ended(dropped: false)
            return ["error": "drop: the field under the point inserts text and links itself (LLP 1094.001 D2), which the agent's drop does not: drop on the node's own box, or declare `drop` on the field"]
        }
        let line = Self.line(target, point), leaves = released(into: target, at: point)
        defer { lands(leaves.nodes) }
        let docs = paths.compactMap { presenter?.session?.mintDocument(URL(fileURLWithPath: $0)) }
        if docs.isEmpty, !paths.isEmpty { presenter?.session?.log("drop: no file could be opened") }
        guard deliver(docs, text, url, to: target, line: line) else {
            leave(leaves.records)
            return ["error": "drop: refused: no file could be opened, and it carried no text or link"]
        }
        return ["tapped": Int(target.id), "drop": paths.count, "delivery": "presenter"]
    }

    // MARK: The tracker

    /// The presenter's nodes under a window point, up the authored tree: the
    /// chain that hears `dragenter` or `dragleave`, innermost first; the
    /// innermost enabled node with `drop`; and the node whose text field the
    /// point is over, when that field is at or below the target.
    private func nodes(at point: CGPoint) -> (chain: [NodeView], target: NodeView?, fieldOwner: NodeView?) {
        guard let presenter, let hit = presenter.viewport.window?.hitTest(point, with: nil) else { return ([], nil, nil) }
        var chain: [NodeView] = [], target: NodeView?, fieldOwner: NodeView?
        for node in presenter.authoredNodes(from: hit) where node.presenter === presenter {
            if target == nil, fieldOwner == nil, let field = node.field ?? node.textArea, hit.isDescendant(of: field) { fieldOwner = node }
            guard !node.inert, !node.formDisabled else { continue }
            if node.handlers.contains("dragenter") || node.handlers.contains("dragleave") { chain.append(node) }
            if target == nil, node.handlers.contains("drop") { target = node }
        }
        return (chain, target, fieldOwner)
    }

    /// The drag is reported over the window: the nodes it came over hear
    /// `dragenter`, then those it left `dragleave`, each innermost first.
    @discardableResult
    private func moved(_ session: UIDropSession) -> CGPoint {
        let point = location(session)
        turn &+= 1
        update(nodes(at: point).chain, at: point)
        return point
    }

    /// The session's point in the presenter's window.
    private func location(_ session: UIDropSession) -> CGPoint {
        guard let window = presenter?.viewport.window else { return .zero }
        return session.location(in: window)
    }

    private func update(_ next: [NodeView], at point: CGPoint) {
        let before = entered
        entered = next
        last = point
        for node in next where node.handlers.contains("dragenter") && !before.contains(where: { $0 === node }) {
            if waiting[ObjectIdentifier(node)] != nil { held.insert(ObjectIdentifier(node)); continue }
            send(node, 55, point)
        }
        for node in before where !next.contains(where: { $0 === node }) {
            if held.remove(ObjectIdentifier(node)) != nil || !node.handlers.contains("dragleave") { continue }
            send(node, 56, point)
        }
    }

    /// The drag left a view taking it: what is under its last point lets go
    /// on the next turn, unless it is reported first (it moved onto a field,
    /// or back).
    private func exited() {
        turn &+= 1
        let mine = turn
        DispatchQueue.main.async { [weak self] in
            guard let self, self.turn == mine else { return }
            self.update([], at: self.last)
        }
    }

    /// The drag ended: cancelled, or dropped where nothing took it, every
    /// node still entered leaves; `dropped`, none does.
    private func ended(dropped: Bool) {
        turn &+= 1
        if dropped { entered = [] } else if !entered.isEmpty { update([], at: last) }
    }

    /// The drag drops at `point`, into `target` or into nothing of Exact's
    /// (a field, or no node): its held enters are heard first. Into nothing,
    /// the nodes it was over leave now. Into `target`, they are kept, with
    /// their leave records, for a delivery that fails, and hold a newer
    /// drag's enters until `lands`.
    @discardableResult
    private func released(into target: NodeView?, at point: CGPoint) -> (nodes: [NodeView], records: [(NodeView, String)]) {
        for node in entered where held.remove(ObjectIdentifier(node)) != nil { release(node, at: point) }
        guard target != nil else {
            ended(dropped: false)
            return ([], [])
        }
        let nodes = entered, records = entered.filter { $0.handlers.contains("dragleave") }.map { ($0, Self.line($0, point)) }
        ended(dropped: true)
        waits(nodes)
        return (nodes, records)
    }

    /// A drop over `nodes` waits for its items.
    func waits(_ nodes: [NodeView]) {
        for node in nodes { waiting[ObjectIdentifier(node), default: 0] += 1 }
    }

    /// A drop over `nodes` landed: a newer drag's enter held at one of them
    /// is heard once no other waiting drop holds it.
    func lands(_ nodes: [NodeView]) {
        for node in nodes {
            let key = ObjectIdentifier(node)
            guard let count = waiting[key] else { continue }
            if count > 1 { waiting[key] = count - 1; continue }
            waiting[key] = nil
            if held.remove(key) != nil, entered.contains(where: { $0 === node }) { release(node, at: last) }
        }
    }

    /// A held enter is heard, unless the node has since stopped hearing it
    /// (the waiting drop's action disabled it or made it inert): then the
    /// drag is over it no longer, with no leave for an enter it never heard.
    private func release(_ node: NodeView, at point: CGPoint) {
        guard !node.inert, !node.formDisabled, node.handlers.contains("dragenter") else { return entered.removeAll { $0 === node } }
        send(node, 55, point)
    }

    private func leave(_ records: [(NodeView, String)]) {
        for (node, line) in records { presenter?.dragEvent(node.id, 56, line) }
    }

    /// The drop's record (LLP 1094.001 D1): its point line, then
    /// `[files, text, url]` as JSON. False, and nothing heard, when it
    /// carried nothing that arrived.
    private func deliver(_ files: [String], _ text: String, _ url: String, to node: NodeView, line: String) -> Bool {
        guard !files.isEmpty || !text.isEmpty || !url.isEmpty, let data = try? JSONSerialization.data(withJSONObject: [files, text, url]) else { return false }
        presenter?.dragEvent(node.id, 38, line + "\n" + String(decoding: data, as: UTF8.self))
        return true
    }

    private func send(_ node: NodeView, _ kind: UInt32, _ point: CGPoint) { presenter?.dragEvent(node.id, kind, Self.line(node, point)) }

    /// A `DragEvent`'s point line at a window point: from the node's content
    /// box, with no modifier keys, which a drop session does not report.
    static func line(_ node: NodeView, _ point: CGPoint) -> String {
        let at = node.convert(point, from: nil), box = node.contentBox()
        return "\(Double(at.x - box.minX)),\(Double(at.y - box.minY))"
    }

}
#endif
