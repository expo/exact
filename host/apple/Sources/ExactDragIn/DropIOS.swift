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
/// events, and the tracker the drag is over.
final class WindowDrops: NSObject, UIDropInteractionDelegate {
    private static let windows = NSMapTable<UIWindow, WindowDrops>.weakToStrongObjects()
    private weak var window: UIWindow?
    private let trackers = NSHashTable<DropTarget>.weakObjects()
    private weak var current: DropTarget?
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

    func dropInteraction(_ interaction: UIDropInteraction, sessionDidExit session: UIDropSession) { current?.exited(session, from: self) }

    /// Every tracker hears the end: each lets go only of the drag it follows.
    func dropInteraction(_ interaction: UIDropInteraction, sessionDidEnd session: UIDropSession) {
        for tracker in trackers.allObjects { tracker.ended(session) }
        current = nil
    }

    func dropInteraction(_ interaction: UIDropInteraction, performDrop session: UIDropSession) {
        guard let window else { return }
        current?.perform(session, at: session.location(in: window))
        current = nil
    }

    /// The drag is over the window: the tracker of the presenter under it
    /// follows it, and one it left lets it go. Whether a node there takes it.
    func report(_ session: UIDropSession) -> Bool {
        guard let window else { return false }
        let point = session.location(in: window), hit = window.hitTest(point, with: nil)
        let under = hit.flatMap { hit in trackers.allObjects.first { $0.owns(hit) } }
        if under !== current {
            current?.handOff(session)
            current = under
        }
        return under?.moved(session, at: point, from: self) ?? false
    }
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
    private var waiting: [Node: Int] = [:]
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

    func dropPoints(of view: NodeView) -> [UIAccessibilityLocationDescriptor]? {
        guard let presenter, let node = presenter.authoredNodes(from: view).first(where: { $0.handlers.contains("drop") }) else { return nil }
        guard !node.inert, !node.formDisabled else { return [] }
        return [UIAccessibilityLocationDescriptor(name: node.accessibleName, view: node)]
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

    /// The drag moved over another presenter's view.
    func handOff(_ session: UIDropSession) { ended(session) }

    /// The drag ended without dropping here: every node it was over leaves.
    func ended(_ session: UIDropSession) {
        guard self.session == ObjectIdentifier(session) else { return }
        finish()
    }

    func perform(_ session: UIDropSession, at point: CGPoint) {
        guard self.session == ObjectIdentifier(session), let presenter, let release = release(at: point) else { return }
        // Every item loads before the action runs (UIKit starts each load here)
        DropItems.read(session.items.map(\.itemProvider), log: { [weak presenter] in presenter?.session?.log($0) }) { [weak self] files, text, url, folder in
            guard let self else { return }
            // A node gone, lent to another, disabled, or no longer hearing `drop` while its items loaded takes nothing
            let target = presenter.views[release.target.id], session = presenter.session
            let live = target.map { Node($0) == release.target && $0.handlers.contains("drop") && !$0.inert && !$0.formDisabled } == true
            // The copies become the session's only as they are delivered
            if live, let session, !files.isEmpty { PastedImages.held(folder, by: session) } else { try? FileManager.default.removeItem(at: folder) }
            let docs = live ? files.compactMap { session?.mintDocument($0) } : []
            if !(live && self.deliver(docs, text, url, to: release.target, line: release.line)) { self.leave(release.leaves) }
            self.lands(release.over)
        }
    }

    // MARK: A field's own text drop

    func textDroppableView(_ textDroppableView: UIView & UITextDroppable, dropSessionDidEnter session: UIDropSession) {
        _ = moved(session, at: location(session), from: textDroppableView)
    }

    func textDroppableView(_ textDroppableView: UIView & UITextDroppable, dropSessionDidUpdate session: UIDropSession) {
        _ = moved(session, at: location(session), from: textDroppableView)
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
        update(found.chain, at: point)
        if kind == "dragenter" { return ["tapped": Int(view.id), "dragenter": paths.count, "delivery": "presenter"] }
        guard let target = found.target else {
            finish()
            return ["error": "view \(view.id) takes no drop: nothing under it declares `drop`"]
        }
        // UIKit's field inserts text and links itself, which the agent does not
        if paths.isEmpty, let owner = found.fieldOwner, owner !== target {
            finish()
            return ["error": "drop: the field under the point inserts text and links itself (LLP 1094.001 D2), which the agent's drop does not: drop on the node's own box, or declare `drop` on the field"]
        }
        guard let release = release(at: point) else { return ["error": "view \(view.id) takes no drop: nothing under it declares `drop`"] }
        defer { lands(release.over) }
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

    private func update(_ next: [NodeView], at point: CGPoint) {
        let before = entered
        entered = next
        last = point
        for node in next where node.handlers.contains("dragenter") && !before.contains(where: { $0 === node }) {
            if waiting[Node(node)] != nil { held.insert(Node(node)); continue }
            send(node, 55, point)
        }
        for node in before where !next.contains(where: { $0 === node }) {
            if held.remove(Node(node)) != nil || !node.handlers.contains("dragleave") { continue }
            send(node, 56, point)
        }
    }

    /// The drag ends here without a drop: every node it was over leaves.
    private func finish() {
        turn &+= 1
        if !entered.isEmpty { update([], at: last) }
        session = nil
        reporter = nil
    }

    /// A drop at `point`: the nodes under it and its target, resolved
    /// together, with the records as it dropped (the point in each box). Its
    /// held enters are heard first. With no target, the nodes it was over
    /// leave now; with one, they are kept for a delivery that fails, and hold
    /// a newer drag's enters until `lands`.
    private func release(at point: CGPoint) -> (target: Node, line: String, over: [Node], leaves: [(Node, String)])? {
        let found = nodes(at: point)
        update(found.chain, at: point)
        heardHeld(at: point)
        guard let target = found.target else {
            finish()
            return nil
        }
        let over = entered.map(Node.init), leaves = entered.filter { $0.handlers.contains("dragleave") }.map { (Node($0), Self.line($0, point)) }
        turn &+= 1
        entered = []
        session = nil
        reporter = nil
        waits(over)
        return (Node(target), Self.line(target, point), over, leaves)
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
            if held.remove(node) != nil, let view = entered.first(where: { Node($0) == node }) { release(view, at: last) }
        }
    }

    private func heardHeld(at point: CGPoint) {
        for view in entered where held.remove(Node(view)) != nil { release(view, at: point) }
    }

    /// A held enter is heard, unless the node has since stopped hearing it
    /// (the waiting drop's action disabled it or made it inert): then the
    /// drag is over it no longer, with no leave for an enter it never heard.
    private func release(_ node: NodeView, at point: CGPoint) {
        guard !node.inert, !node.formDisabled, node.handlers.contains("dragenter") else { return entered.removeAll { $0 === node } }
        send(node, 55, point)
    }

    /// Each node still the one it was hears its `dragleave` record.
    private func leave(_ records: [(Node, String)]) {
        for (node, line) in records where presenter?.views[node.id].map(Node.init) == node { presenter?.clipboard(node.id, 56, line) }
    }

    /// The drop's record (LLP 1094.001 D1): its point line, then
    /// `[files, text, url]` as JSON. False, and nothing heard, when it
    /// carried nothing that arrived or its node is gone.
    private func deliver(_ files: [String], _ text: String, _ url: String, to node: Node, line: String) -> Bool {
        guard !files.isEmpty || !text.isEmpty || !url.isEmpty, let data = try? JSONSerialization.data(withJSONObject: [files, text, url]) else { return false }
        return presenter?.clipboard(node.id, 38, line + "\n" + String(decoding: data, as: UTF8.self)) == true
    }

    private func send(_ node: NodeView, _ kind: UInt32, _ point: CGPoint) { presenter?.clipboard(node.id, kind, Self.line(node, point)) }

    /// A `DragEvent`'s point line at a window point: from the node's content
    /// box, with no modifier keys, which a drop session does not report.
    static func line(_ node: NodeView, _ point: CGPoint) -> String {
        let at = node.convert(point, from: nil), box = node.contentBox()
        return "\(Double(at.x - box.minX)),\(Double(at.y - box.minY))"
    }
}
#endif
