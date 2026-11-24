// The drag-in capability on UIKit (LLP 1094.001 D4, the roster's `drag_in`):
// a drag from another app, or from this one, dropped on a node that declares
// `drop`. The core reaches it only through `DragInLink`.
//
// UIKit offers a drag only to the drop interactions on the views under it,
// and a route's, a sheet's or a popover's views are not under their
// presenter's root, though they are in its window. So each window has one
// interaction, which every presenter in it shares (`WindowDrops`), and each
// callback goes to the tracker of the presenter whose node is under the drag
// (`DropTarget`). A text field keeps UIKit's own text drop, which is deeper,
// so it takes the drags its text view takes (text, links, text files, and the
// text of a drag that also carries files); its `textDropDelegate` reports the
// drag's location to the tracker. While a field's own node declares `drop`,
// its text drop is taken off, so every drag over it reaches the node (D2).
#if os(iOS)
import ExactKit
import UIKit

public enum ExactDragIn {
    public static func install() { DragInLink.installed = DragInHost() }
}

final class DragInHost: DragInCapability {
    func target(_ presenter: Presenter) -> DragInTarget { DropTarget(presenter) }
}

/// A window's one drop interaction, while a presenter in it hears a drag's
/// events. Each callback goes to the tracker that follows its session, so a
/// callback for another drag reaches none.
final class WindowDrops: NSObject, UIDropInteractionDelegate {
    private static let windows = NSMapTable<UIWindow, WindowDrops>.weakToStrongObjects()
    private weak var window: UIWindow?
    private let trackers = NSHashTable<DropTarget>.weakObjects()
    private lazy var interaction = UIDropInteraction(delegate: self)

    static func of(_ window: UIWindow) -> WindowDrops {
        if let drops = windows.object(forKey: window) { return drops }
        let drops = WindowDrops()
        drops.window = window
        windows.setObject(drops, forKey: window)
        return drops
    }

    func join(_ tracker: DropTarget) {
        trackers.add(tracker)
        if let window, !window.interactions.contains(where: { $0 === interaction }) { window.addInteraction(interaction) }
    }

    func leave(_ tracker: DropTarget) {
        trackers.remove(tracker)
        if trackers.allObjects.isEmpty { window?.removeInteraction(interaction) }
    }

    func dropInteraction(_ interaction: UIDropInteraction, canHandle session: UIDropSession) -> Bool { !session.items.isEmpty }

    func dropInteraction(_ interaction: UIDropInteraction, sessionDidEnter session: UIDropSession) { _ = report(session) }

    func dropInteraction(_ interaction: UIDropInteraction, sessionDidUpdate session: UIDropSession) -> UIDropProposal {
        UIDropProposal(operation: report(session) ? .copy : .forbidden)
    }

    func dropInteraction(_ interaction: UIDropInteraction, sessionDidExit session: UIDropSession) {
        follower(of: session)?.exited(session, from: self)
    }

    func dropInteraction(_ interaction: UIDropInteraction, sessionDidEnd session: UIDropSession) { follower(of: session)?.ended(session) }

    /// The presenter under the drop is found again at its point, with no
    /// handler run first, and takes it; only then does a tracker the drag
    /// left let it go, so no leave runs before the drop's target is taken.
    func dropInteraction(_ interaction: UIDropInteraction, performDrop session: UIDropSession) {
        guard let window else { return }
        let point = session.location(in: window), under = tracker(at: point)
        under?.perform(session, at: point)
        handOff(session, to: under)
    }

    /// The drag is over the window: the tracker of the presenter under it
    /// follows it, and one it left lets it go. Whether a node there takes it.
    func report(_ session: UIDropSession) -> Bool {
        guard let window else { return false }
        let point = session.location(in: window), under = tracker(at: point)
        handOff(session, to: under)
        return under?.moved(session, at: point, from: self) ?? false
    }

    /// A field's own text drop reports the drag to `tracker`, which follows
    /// it alone, as a report through the window's interaction does.
    func field(_ session: UIDropSession, to tracker: DropTarget, at point: CGPoint, from field: AnyObject) {
        handOff(session, to: tracker)
        _ = tracker.moved(session, at: point, from: field)
    }

    /// Every tracker but `tracker` that follows `session` lets it go.
    private func handOff(_ session: UIDropSession, to tracker: DropTarget?) {
        for other in trackers.allObjects where other !== tracker && other.follows(session) { other.ended(session) }
    }

    private func tracker(at point: CGPoint) -> DropTarget? {
        window?.hitTest(point, with: nil).flatMap { hit in trackers.allObjects.first { $0.owns(hit) } }
    }

    private func follower(of session: UIDropSession) -> DropTarget? { trackers.allObjects.first { $0.follows(session) } }
}

/// A presenter's drag tracker: the one drag it follows at a time, the nodes
/// under it, their events, and its drop.
final class DropTarget: NSObject, DragInTarget, UITextDropDelegate {
    private weak var presenter: Presenter?
    private(set) weak var drops: WindowDrops?
    private static let events: Set<String> = ["drop", "dragenter", "dragleave"]
    /// The agent's drag, a session of its own.
    private static let agent = NSObject()

    /// The drag followed, and the interaction or field that last reported
    /// it: an exit from another, late after a handoff, lets nothing go.
    private var session: ObjectIdentifier?
    private var reporter: ObjectIdentifier?
    /// The nodes the drag is over that hear `dragenter` or `dragleave`,
    /// innermost first, and its last window point. A node is kept by its id
    /// and incarnation, not its view, which the pool lends to other nodes.
    private var entered: [Node] = []
    private var last = CGPoint.zero
    /// Bumped by every report: an exit's clear is stale once the drag is
    /// reported again.
    private var turn = 0
    /// The nodes a drop still loading its items was over, each to its count
    /// of such drops. A newer drag's `dragenter` there is held (`held`) until
    /// those drops land, or until the newer drag drops, so a waiting drop's
    /// action cannot clear what the newer drag drew; the newer drag's leave
    /// cancels its held enter.
    private(set) var waiting: [Node: Int] = [:]
    private var held: Set<Node> = []
    /// Each field's own text drop while its node declares `drop`, taken off
    /// it and kept here: UIKit answers `textDropInteraction` only while the
    /// interaction is on the field.
    private let parked = NSMapTable<UIView, UIInteraction>.weakToStrongObjects()

    /// A node as it is now: a view the pool lends to another node is another.
    struct Node: Hashable {
        let id: UInt32, incarnation: UInt64
        init(_ view: NodeView) { id = view.id; incarnation = view.incarnation }
    }

    init(_ presenter: Presenter) {
        self.presenter = presenter
        super.init()
    }

    func reset() {
        entered = []
        waiting = [:]
        held = []
        session = nil
        reporter = nil
    }

    /// The window's interaction follows the drag while a node here hears its
    /// events; with none, the window and the fields stay as UIKit makes them.
    func sync() {
        guard let presenter else { return }
        let heard = presenter.views.values.contains { !$0.handlers.isDisjoint(with: Self.events) }
        let window = heard ? presenter.viewport.window.map(WindowDrops.of) : nil
        if window !== drops {
            // A drag over the window it leaves ends here, as that window's end no longer reaches it
            if drops != nil { finish() }
            drops?.leave(self)
            drops = window
            window?.join(self)
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

    /// Whether the view a drag is over is this presenter's.
    func owns(_ hit: UIView) -> Bool { presenter.map { $0.authoredNodes(from: hit).first?.presenter === $0 } ?? false }

    func follows(_ session: UIDropSession) -> Bool { self.session == ObjectIdentifier(session) }

    /// The zone's drop point is one where a drop reaches the zone itself:
    /// its middle, else the middle of the first of a 4 × 4 grid of its parts
    /// that a nested zone, a field or another view does not take; none when
    /// every part's middle is taken, though a strip narrower than a part
    /// may still take a drop.
    func dropPoints(of view: NodeView) -> [UIAccessibilityLocationDescriptor]? {
        guard let presenter, let node = presenter.authoredNodes(from: view).first(where: { $0.handlers.contains("drop") }) else { return nil }
        guard !node.inert, !node.formDisabled else { return [] }
        let box = node.bounds, parts = 4
        let points = [CGPoint(x: box.midX, y: box.midY)] + (0..<parts * parts).map { i in
            CGPoint(x: box.minX + box.width * (CGFloat(i % parts) + 0.5) / CGFloat(parts), y: box.minY + box.height * (CGFloat(i / parts) + 0.5) / CGFloat(parts))
        }
        guard let point = points.first(where: { reaches(node, at: node.convert($0, to: nil)) }) else { return [] }
        return [UIAccessibilityLocationDescriptor(name: node.accessibleName, point: point, in: node)]
    }

    /// Whether a drop at window point `point` reaches `node` and no field takes it first.
    private func reaches(_ node: NodeView, at point: CGPoint) -> Bool {
        let found = nodes(at: point)
        return found.target === node && (found.fieldOwner == nil || found.fieldOwner === node)
    }

    // MARK: Reports of the drag (the window's interaction, a field's text drop)

    /// `session` is at window point `point`, as `source` reports: followed
    /// if no other drag is. Whether a node there takes what it carries.
    func moved(_ session: UIDropSession, at point: CGPoint, from source: AnyObject) -> Bool {
        let key = ObjectIdentifier(session)
        guard self.session == nil || self.session == key else { return false }
        self.session = key
        reporter = ObjectIdentifier(source)
        turn &+= 1
        let found = nodes(at: point)
        update(found.chain, at: point)
        return found.target != nil && session.items.contains { DropItems.kind(of: $0.itemProvider) != .none }
    }

    /// The drag left `source`'s view: what is under its last point lets go on
    /// the next turn, unless it is reported first (it moved onto a field, or
    /// back). An exit from a source that no longer reports it is late.
    func exited(_ session: UIDropSession, from source: AnyObject) {
        guard self.session == ObjectIdentifier(session), reporter == ObjectIdentifier(source) else { return }
        turn &+= 1
        let mine = turn
        DispatchQueue.main.async { [weak self] in
            guard let self, self.turn == mine else { return }
            self.finish()
        }
    }

    /// The drag ended without dropping here: every node it was over leaves.
    func ended(_ session: UIDropSession) {
        guard self.session == ObjectIdentifier(session) else { return }
        finish()
    }

    /// The drop of `session` at `point`, which this tracker follows, or takes
    /// up as the drop lands here.
    func perform(_ session: UIDropSession, at point: CGPoint) {
        guard self.session == nil || self.session == ObjectIdentifier(session), let presenter else { return }
        self.session = ObjectIdentifier(session)
        guard let release = release(at: point) else { return }
        // Every item loads before the action runs (UIKit starts each load here)
        DropItems.read(session.items.map(\.itemProvider), log: { [weak presenter] in presenter?.session?.log($0) }) { [weak self] files, text, url, folder in
            guard let self else { return }
            // A node gone, lent to another, disabled, no longer hearing `drop`, or removed by a staged batch while its items loaded takes nothing
            let session = presenter.session, live = self.takes(release.target)
            // The copies become the session's only as they are delivered
            if live, let session, !files.isEmpty { PastedImages.held(folder, by: session) } else { try? FileManager.default.removeItem(at: folder) }
            let docs = live ? files.compactMap { session?.mintDocument($0) } : []
            if !(live && self.deliver(docs, text, url, to: release.target, line: release.line)) { self.leave(release.leaves) }
            self.lands(release.over)
        }
    }

    // MARK: A field's own text drop

    func textDroppableView(_ textDroppableView: UIView & UITextDroppable, dropSessionDidEnter session: UIDropSession) {
        fieldMoved(session, textDroppableView)
    }

    func textDroppableView(_ textDroppableView: UIView & UITextDroppable, dropSessionDidUpdate session: UIDropSession) {
        fieldMoved(session, textDroppableView)
    }

    private func fieldMoved(_ session: UIDropSession, _ field: UIView) {
        if let drops { drops.field(session, to: self, at: location(session), from: field) } else { _ = moved(session, at: location(session), from: field) }
    }

    func textDroppableView(_ textDroppableView: UIView & UITextDroppable, dropSessionDidExit session: UIDropSession) {
        exited(session, from: textDroppableView)
    }

    func textDroppableView(_ textDroppableView: UIView & UITextDroppable, dropSessionDidEnd session: UIDropSession) { ended(session) }

    /// A drop the field takes is a leave at the nodes the drag was over,
    /// its held enters heard first.
    func textDroppableView(_ textDroppableView: UIView & UITextDroppable, willPerformDrop drop: UITextDropRequest) {
        guard session == ObjectIdentifier(drop.dropSession) else { return }
        heardHeld(at: last)
        finish()
    }

    private func location(_ session: UIDropSession) -> CGPoint {
        guard let window = presenter?.viewport.window else { return .zero }
        return session.location(in: window)
    }

    // MARK: The agent's drag (`tap <target> drop|dragenter|dragleave`, D6)

    func agentDrag(_ req: [String: Any], over view: NodeView) -> [String: Any] {
        let agent = ObjectIdentifier(Self.agent)
        guard session == nil || session == agent else { return ["error": "a drag from another app is over the window; let it end first"] }
        if req["dragleave"] != nil {
            if session == agent { finish() }
            return ["tapped": Int(view.id), "dragleave": true, "delivery": "presenter"]
        }
        guard let window = view.window else { return ["error": "view \(view.id) is not on screen"] }
        let at = (req["at"] as? [Double]).flatMap { $0.count == 2 ? CGPoint(x: $0[0], y: $0[1]) : nil }
        let point = view.convert(at ?? CGPoint(x: view.bounds.midX, y: view.bounds.midY), to: nil)
        let kind = req["drop"] != nil ? "drop" : "dragenter"
        let paths = req[kind] as? [String] ?? [], text = req["text"] as? String ?? "", url = req["url"] as? String ?? ""
        guard !paths.isEmpty || !text.isEmpty || !url.isEmpty else { return ["malformed": true, "error": "\(kind): expected the dropped paths, --text or --url"] }
        // A file a person drags is one that exists, and not a folder (D4)
        for path in paths {
            var folder: ObjCBool = false
            guard FileManager.default.fileExists(atPath: path, isDirectory: &folder), !folder.boolValue else { return ["error": "\(kind): \(path) is not a file; iOS drops files, not folders"] }
        }
        // The point must reach the view aimed at, not one over it
        guard let hit = window.hitTest(point, with: nil), presenter?.authoredNodes(from: hit).contains(where: { $0 === view }) == true else {
            return ["error": "view \(view.id) is covered at that point: a drag there reaches what is over it"]
        }
        let found = nodes(at: point)
        session = agent
        turn &+= 1
        if kind == "dragenter" {
            update(found.chain, at: point)
            return ["tapped": Int(view.id), "dragenter": paths.count, "delivery": "presenter"]
        }
        // UIKit's field takes a drag's text and links itself, files or not, which the agent's drop does not
        let byField = (!text.isEmpty || !url.isEmpty) && found.fieldOwner.map { $0 !== found.target } == true
        // A drop that reaches its target is resolved, its records taken, before any handler runs (`release`)
        guard found.target != nil, !byField else {
            update(found.chain, at: point)
            finish()
            if byField { return ["error": "drop: the field under the point inserts the drag's text and links itself (LLP 1094.001 D2, D4), which the agent's drop does not: drop on the node's own box, or declare `drop` on the field"] }
            return ["error": "view \(view.id) takes no drop: nothing under it declares `drop`"]
        }
        guard let release = release(at: point) else { return ["error": "view \(view.id) takes no drop: nothing under it declares `drop`"] }
        defer { lands(release.over) }
        // As a person's drop: a target its enter made inert, disabled or deaf to `drop` takes nothing
        guard takes(release.target) else {
            leave(release.leaves)
            return ["error": "drop: refused: view \(release.target.id) no longer takes a drop once its `dragenter` ran"]
        }
        let docs = paths.compactMap { presenter?.session?.mintDocument(URL(fileURLWithPath: $0)) }
        guard deliver(docs, text, url, to: release.target, line: release.line) else {
            leave(release.leaves)
            return ["error": "drop: refused: no file could be opened, and it carried no text or link"]
        }
        return ["tapped": Int(release.target.id), "drop": paths.count, "delivery": "presenter"]
    }

    // MARK: The tracker

    /// The presenter's nodes under a window point, up the authored tree: the
    /// chain that hears `dragenter` or `dragleave`, innermost first; the
    /// innermost enabled node with `drop`; and the node whose editable text
    /// field the point is over, when that field is at or below the target.
    private func nodes(at point: CGPoint) -> (chain: [NodeView], target: NodeView?, fieldOwner: NodeView?) {
        guard let presenter, let hit = presenter.viewport.window?.hitTest(point, with: nil) else { return ([], nil, nil) }
        var chain: [NodeView] = [], target: NodeView?, fieldOwner: NodeView?
        for node in presenter.authoredNodes(from: hit) where node.presenter === presenter {
            if target == nil, fieldOwner == nil, Self.editable(node).map({ hit.isDescendant(of: $0) }) == true { fieldOwner = node }
            guard !node.inert, !node.formDisabled else { continue }
            if node.handlers.contains("dragenter") || node.handlers.contains("dragleave") { chain.append(node) }
            if target == nil, node.handlers.contains("drop") { target = node }
        }
        return (chain, target, fieldOwner)
    }

    /// A node's text field while it can take text.
    private static func editable(_ node: NodeView) -> UIView? {
        if let area = node.textArea, area.isEditable { return area }
        if let field = node.field, field.isEnabled { return field }
        return nil
    }

    /// The drag is over `next` at `point`: each node it came over hears
    /// `dragenter`, unless a waiting drop holds it, and each it left
    /// `dragleave`. Every record is taken before any is heard, since a
    /// handler may change what a view shows, and each node is checked to be
    /// the same one before it hears its own.
    private func update(_ next: [NodeView], at point: CGPoint) {
        let before = entered, now = next.map(Node.init)
        entered = now
        last = point
        let enters = next.filter { $0.handlers.contains("dragenter") && !before.contains(Node($0)) }.map { (Node($0), Self.line($0, point)) }
        let leaves = before.filter { !now.contains($0) && held.remove($0) == nil }.compactMap { node in
            view(node).flatMap { $0.handlers.contains("dragleave") ? (node, Self.line($0, point)) : nil }
        }
        for (node, line) in enters {
            if waiting[node] != nil { held.insert(node) } else { hear(node, 55, line) }
        }
        for (node, line) in leaves { hear(node, 56, line) }
    }

    /// The drag ends here without a drop: every node it was over leaves.
    private func finish() {
        turn &+= 1
        if !entered.isEmpty { update([], at: last) }
        session = nil
        reporter = nil
    }

    /// A drop at `point`: the nodes under it and its target, resolved
    /// together, with the records as it dropped (the point in each box),
    /// taken before any handler runs. Its held enters are heard first. With
    /// no target, the nodes it was over leave now; with one, they are kept
    /// for a delivery that fails, and hold a newer drag's enters until
    /// `lands`.
    private func release(at point: CGPoint) -> (target: Node, line: String, over: [Node], leaves: [(Node, String)])? {
        let found = nodes(at: point)
        let target = found.target.map { (Node($0), Self.line($0, point)) }
        let leaves = found.chain.filter { $0.handlers.contains("dragleave") }.map { (Node($0), Self.line($0, point)) }
        update(found.chain, at: point)
        heardHeld(at: point)
        guard let target else {
            finish()
            return nil
        }
        let over = entered
        turn &+= 1
        entered = []
        session = nil
        reporter = nil
        waits(over)
        return (target.0, target.1, over, leaves.filter { over.contains($0.0) })
    }

    /// A drop over `nodes` waits for its items.
    func waits(_ nodes: [Node]) {
        for node in nodes { waiting[node, default: 0] += 1 }
    }

    /// A drop over `nodes` landed: a newer drag's enter held at one of them
    /// is heard once no other waiting drop holds it.
    func lands(_ nodes: [Node]) {
        for node in nodes {
            guard let count = waiting[node] else { continue }
            if count > 1 { waiting[node] = count - 1; continue }
            waiting[node] = nil
            if held.remove(node) != nil, entered.contains(node) { release(node, at: last) }
        }
    }

    private func heardHeld(at point: CGPoint) {
        for node in entered where held.remove(node) != nil { release(node, at: point) }
    }

    /// A held enter is heard, unless the node is gone or has since stopped
    /// hearing it (the waiting drop's action disabled it or made it inert):
    /// then the drag is over it no longer, with no leave for an enter it
    /// never heard.
    private func release(_ node: Node, at point: CGPoint) {
        guard let view = view(node), !view.inert, !view.formDisabled, view.handlers.contains("dragenter") else { return entered.removeAll { $0 == node } }
        hear(node, 55, Self.line(view, point))
    }

    /// Each node still the one it was hears its `dragleave` record.
    private func leave(_ records: [(Node, String)]) {
        for (node, line) in records { hear(node, 56, line) }
    }

    /// The drop's record (LLP 1094.001 D1): its point line, then
    /// `[files, text, url]` as JSON. False, and nothing heard, when it
    /// carried nothing that arrived, its node is gone or another's, or a
    /// staged batch removed it.
    private func deliver(_ files: [String], _ text: String, _ url: String, to node: Node, line: String) -> Bool {
        guard !files.isEmpty || !text.isEmpty || !url.isEmpty, takes(node),
              let data = try? JSONSerialization.data(withJSONObject: [files, text, url]) else { return false }
        return hear(node, 38, line + "\n" + String(decoding: data, as: UTF8.self))
    }

    /// Node `node` hears an event of `kind`, if its view is still the one it was.
    @discardableResult private func hear(_ node: Node, _ kind: UInt32, _ line: String) -> Bool {
        view(node) != nil && presenter?.clipboard(node.id, kind, line) == true
    }

    /// Whether `node` takes a drop now: the same node, enabled, not inert,
    /// hearing `drop`, and not removed by a staged batch.
    private func takes(_ node: Node) -> Bool {
        guard let view = view(node), view.handlers.contains("drop"), !view.inert, !view.formDisabled else { return false }
        return presenter?.session?.admits(node.id) != false
    }

    /// The view of `node`, while the pool has not lent it to another.
    private func view(_ node: Node) -> NodeView? { presenter?.views[node.id].flatMap { Node($0) == node ? $0 : nil } }

    /// A `DragEvent`'s point line at a window point: from the node's content
    /// box, with no modifier keys, which a drop session does not report.
    static func line(_ node: NodeView, _ point: CGPoint) -> String {
        let at = node.convert(point, from: nil), box = node.contentBox()
        return "\(Double(at.x - box.minX)),\(Double(at.y - box.minY))"
    }
}
#endif
