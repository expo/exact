// A drag from another app (Finder, Photos, a browser, another Exact app)
// over a window, carrying files, promised files, plain text or a link
// (LLP 1094.001): DOM's `dragenter` and `dragleave` at each node that hears
// them, and its `drop` at the innermost node that declares `drop`
// (MouseEventsMac.swift), when the source allows a copy.
//
// A node hears `dragenter` when the drag comes over its box from outside
// it, and `dragleave` when it leaves the box, is cancelled or leaves the
// window: moves between its own descendants are neither, which is what a
// web handler gets by ignoring an event whose `relatedTarget` it contains,
// as a drop overlay does. A drop a node takes ends the drag with no
// `dragleave`, as a browser's does (an overlay is cleared by the `drop`'s
// action); a drop no node takes, one a field takes, and one that carried
// nothing that arrived is a leave at each node entered when it dropped.
// Every such drag is heard, whether or not a `drop` takes it; a disabled
// control or an inert node hears none.
//
// AppKit hands a drag to the deepest view under it that registered for its
// types, and a text field or text area registers for text, links and file
// URLs, inserting them. A drag with files over one whose node or an
// ancestor declares `drop` goes to that node instead, as the web's ancestor
// listener prevents the drop's default, and so does any drag over a field
// whose own node declares `drop`; a drag with no files, or with no `drop`
// above, is the field's own.
#if os(macOS)
import AppKit

enum FileDrag {
    static let events: Set<String> = ["drop", "dragenter", "dragleave"]

    /// What a node hearing one of `events` registers for: file URLs,
    /// promised files (Photos', Exact's own), plain text and links.
    static let types: [NSPasteboard.PasteboardType] = [.fileURL, .URL, .string] + NSFilePromiseReceiver.readableDraggedTypes.map { .init($0) }

    /// The nodes the drag is over that hear `dragenter` or `dragleave`,
    /// innermost first, and the drag's last window point.
    nonisolated(unsafe) private static var entered: [NodeView] = []
    nonisolated(unsafe) private static var last = NSPoint.zero
    /// The window `last` is measured in.
    nonisolated(unsafe) private static weak var lastWindow: NSWindow?
    /// Bumped by every callback: a pending exit's clear is stale once it moves.
    nonisolated(unsafe) private static var turn = 0
    /// The nodes a drop still waiting for promised files was over, each to its
    /// count of such drops. A newer drag's `dragenter` there is held (`held`)
    /// until those drops land, or until the newer drag drops, so a waiting
    /// drop's action cannot clear what the newer drag drew; a leave of the
    /// newer drag cancels its held enter.
    nonisolated(unsafe) private static var waiting: [ObjectIdentifier: Int] = [:]
    nonisolated(unsafe) private static var held: Set<ObjectIdentifier> = []

    /// Whether the drag carries what a node with `drop` takes.
    static func carries(_ info: NSDraggingInfo) -> Bool { info.draggingPasteboard.availableType(from: types) != nil }

    /// Whether the drag carries files, as file URLs or promised files.
    static func carriesFiles(_ info: NSDraggingInfo) -> Bool {
        let board = info.draggingPasteboard
        return board.canReadObject(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true])
            || board.canReadObject(forClasses: [NSFilePromiseReceiver.self], options: nil)
    }

    /// The node a drag over `view` goes to (LLP 1094.001 D2): the innermost
    /// node with `drop` from `view` up, when the source allows a copy. Over a
    /// field, a drag with no files goes there only when that node is the
    /// field's own; otherwise it is the field's to insert.
    static func destination(_ view: NSView, _ info: NSDraggingInfo) -> NodeView? {
        guard carries(info), info.draggingSourceOperationMask.contains(.copy) else { return nil }
        return destination(over(view, at: info.draggingLocation), files: carriesFiles(info))
    }

    /// The view a drag reported to `view` is over: for a scroller's clip
    /// view, the drag's one destination over its scroller, what is under the
    /// drag's point, as AppKit's scrolling under a still drag does not move
    /// the drag to another destination (LLP 1094.000 D15); for any other,
    /// `view`.
    static func over(_ view: NSView, at point: NSPoint) -> NSView {
        guard view is DropClipView else { return view }
        return hit(at: point, in: view.window) ?? view
    }

    /// The view under `point` in `window`, a list's row cell for the row.
    static func hit(at point: NSPoint, in window: NSWindow?) -> NSView? {
        guard let content = window?.contentView, content.bounds.contains(content.convert(point, from: nil)) else { return nil }
        return TableRowCell.whatIsThere { content.hitTest(content.superview?.convert(point, from: nil) ?? point) }
    }

    /// `destination` for a drag that does or does not carry files, as the
    /// agent's drop asks it too.
    static func destination(_ view: NSView, files: Bool) -> NodeView? {
        guard let target = NodeView.dropTarget(view) else { return nil }
        // An input being edited takes the drag in the window's field editor, whose delegate is the input
        let field = view is FieldEditor ? (view as? FieldEditor)?.delegate as? NSView : view is TextArea || view is Field ? view : nil
        guard !files, let field else { return target }
        return target.textArea === field || target.field === field ? target : nil
    }

    /// The nodes hearing `dragenter` or `dragleave` whose box holds `point`
    /// in `window`, innermost first: what is under it and its ancestors.
    static func chain(at point: NSPoint, in window: NSWindow?) -> [NodeView] {
        var view = hit(at: point, in: window)
        var out: [NodeView] = []
        while let at = view {
            let node = at as? NodeView
            if let node, !node.inert, !node.formDisabled,
               node.handlers.contains("dragenter") || node.handlers.contains("dragleave") { out.append(node) }
            view = parent(of: at, node: node)
        }
        return out
    }

    /// Where a drag's chain and its target go up from `view`, which is `node`
    /// when it is one: a node's
    /// authored parent, as a press's does (`presentationParent`, then
    /// `eventParent`), so a data table's cell reaches its row and a node in
    /// an open popover the node it is declared under; else the superview.
    static func parent(of view: NSView, node: NodeView?) -> NSView? {
        guard let node, let presenter = node.presenter else { return view.superview }
        let parent = presenter.presentationParent(of: node) ?? presenter.eventParent(of: node)
        // The walk stays in the drag's window, whose points its records are measured in
        if let other = parent?.window, other !== view.window { return view.superview }
        return parent
    }

    /// The drag is over `next`: the nodes it came over hear `dragenter`,
    /// then those it left `dragleave`, each innermost first, as the DOM
    /// fires the new target's enter (bubbling) before the old one's leave.
    static func update(_ next: [NodeView], at point: NSPoint, in window: NSWindow?, records: [ObjectIdentifier: String] = [:]) {
        let before = entered, previous = (last, lastWindow)
        entered = next
        (last, lastWindow) = (point, window)
        // Every record is taken before any handler runs, as one may move what is under the drag; a node in the
        // window the drag left takes its last point there, as `point` is measured in the new one
        let record = { (node: NodeView) in records[ObjectIdentifier(node)] ?? node.dragLine(at(node, point, in: window, previous)) }
        let enters = next.filter { node in node.handlers.contains("dragenter") && !before.contains(where: { $0 === node }) }
            .map { ($0, record($0)) }
        let leaves = before.filter { node in !next.contains(where: { $0 === node }) }.map { ($0, record($0)) }
        for (node, line) in enters {
            if waiting[ObjectIdentifier(node)] != nil { held.insert(ObjectIdentifier(node)); continue }
            // An earlier handler may have made this node inert or disabled: then the drag is over it no longer
            guard !node.inert, !node.formDisabled, node.handlers.contains("dragenter") else { entered.removeAll { $0 === node }; continue }
            node.presenter?.mouseEvent(node.id, 55, line)
        }
        for (node, line) in leaves {
            if held.remove(ObjectIdentifier(node)) != nil || !node.handlers.contains("dragleave") { continue }
            node.presenter?.mouseEvent(node.id, 56, line)
        }
    }

    /// Where `node`'s record is measured for a drag now at `point` in
    /// `window`: there, or, for a node of the window the drag came from, at
    /// its last point in that window, as `point` is measured in the new one.
    private static func at(_ node: NodeView, _ point: NSPoint, in window: NSWindow?, _ previous: (NSPoint, NSWindow?)) -> NSPoint {
        node.window !== window && node.window === previous.1 ? previous.0 : point
    }

    /// The drag drops, into a node or a field: an enter it has held is heard first.
    static func heldEnters(at point: NSPoint, records: [ObjectIdentifier: String] = [:]) {
        for node in entered where held.remove(ObjectIdentifier(node)) != nil { release(node, at: point, record: records[ObjectIdentifier(node)]) }
    }

    /// A held enter is heard, unless the node has since stopped hearing it (the waiting drop's action disabled it or
    /// made it inert): then the drag is over it no longer, with no leave for an enter it never heard. Its record is
    /// `record` when the release took it with the others.
    private static func release(_ node: NodeView, at point: NSPoint, record: String? = nil) {
        guard !node.inert, !node.formDisabled, node.handlers.contains("dragenter") else { return entered.removeAll { $0 === node } }
        node.presenter?.mouseEvent(node.id, 55, record ?? node.dragLine(point))
    }

    /// A drop over `nodes` waits for promised files.
    static func waits(_ nodes: [NodeView]) {
        for node in nodes { waiting[ObjectIdentifier(node), default: 0] += 1 }
    }

    /// A waiting drop landed: a newer drag's enter held at a node it was over
    /// is heard once no other waiting drop holds it.
    static func lands(_ nodes: [NodeView]) {
        for node in nodes {
            let key = ObjectIdentifier(node)
            guard let count = waiting[key] else { continue }
            if count > 1 { waiting[key] = count - 1; continue }
            waiting[key] = nil
            if held.remove(key) != nil, entered.contains(where: { $0 === node }) { release(node, at: last) }
        }
    }

    /// A destination's `draggingEntered` or `draggingUpdated` at `view`: the
    /// drag tracked, and `.copy` when a node takes the drop (`destination`).
    /// Nil when it carries nothing a node takes, or no node takes it (a
    /// field's own then).
    static func moved(_ view: NSView, _ info: NSDraggingInfo) -> NSDragOperation? {
        guard carries(info) else { return nil }
        turn &+= 1
        update(chain(at: info.draggingLocation, in: view.window), at: info.draggingLocation, in: view.window)
        return destination(view, info) != nil ? .copy : nil
    }

    /// The drag left `view`. AppKit's last point as it leaves the window
    /// can still be inside it, so what is still under that point lets go a
    /// moment later unless another callback comes first (the drag moved into
    /// a registered child, or came back).
    static func exited(_ view: NSView, _ info: NSDraggingInfo?) {
        guard let info, carries(info) else { return }
        turn &+= 1
        let mine = turn, point = info.draggingLocation
        update(chain(at: point, in: view.window), at: point, in: view.window)
        guard !entered.isEmpty else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { if turn == mine { update([], at: last, in: lastWindow) } }
    }

    /// A destination's `performDragOperation` at `view`: the drop at the
    /// node that takes it, once its promised files are received, and the
    /// drag's end. The nodes entered when it dropped are kept: a drop that
    /// carried nothing that arrived is their leave, whenever it resolves.
    /// Nil when no node takes it.
    static func perform(_ view: NSView, _ info: NSDraggingInfo) -> Bool? {
        // The chain, the target and their records are taken together at the release, before any handler runs: a scroll
        // since the last update may have moved what is under it (LLP 1094.000 D15). The records are the point in each
        // box and the keys held as it dropped.
        // A deferred exit's clear is a turn behind now, so a handler that turns the main queue cannot run it
        turn &+= 1
        let point = info.draggingLocation, carried = carries(info), previous = (last, lastWindow)
        let target = destination(view, info), line = target?.dragLine(point)
        let now = carried ? chain(at: point, in: view.window) : entered
        let records = Dictionary((now + entered).map { (ObjectIdentifier($0), $0.dragLine(at($0, point, in: view.window, previous))) },
                                 uniquingKeysWith: { first, _ in first })
        if carried { update(now, at: point, in: view.window, records: records) }
        guard let target, let line else { heldEnters(at: point, records: records); ended(records: records); return nil }
        let id = target.id
        heldEnters(at: point, records: records)
        // Who the drop was over, after the enters ran: one an earlier handler made ineligible heard no enter, and
        // owes no leave
        let dropped = entered
        let leaves = dropped.filter { $0.handlers.contains("dragleave") }.compactMap { node in records[ObjectIdentifier(node)].map { (node, $0) } }
        ended(dropped: true)
        var landed: Bool?, delayed = false
        receive(info, for: target.presenter?.session) { files, text, url in
            // A node gone, no longer hearing `drop`, or made inert or disabled (by an enter at the release, or while its
            // promised files arrived) takes nothing
            let live = target.presenter?.views[id] === target && target.handlers.contains("drop") && !target.inert && !target.formDisabled
            let ok = live && target.drop(files, text: text, url: url, line: line)
            if !ok { leave(leaves) }
            if delayed { lands(dropped) }
            landed = ok
        }
        guard let landed else {
            // A promise still pending lands later: the drop is taken now, and holds a newer drag's enters meanwhile
            delayed = true
            waits(dropped)
            return true
        }
        return landed
    }

    /// Whether a node takes this drag over `view` (a field's forwarding).
    static func takes(_ view: NSView, _ info: NSDraggingInfo?) -> Bool {
        guard let info else { return false }
        return destination(view, info) != nil
    }

    /// Each node hears its `dragleave` record.
    private static func leave(_ leaves: [(NodeView, String)]) {
        for (node, line) in leaves { node.presenter?.mouseEvent(node.id, 56, line) }
    }

    /// What the drag drops, in its order (LLP 1094.001 D1): its files, a
    /// promise before a file URL as Apple's file-promise sample takes them,
    /// each promised file once received into a folder of its own that the
    /// session holds; the plain text of each other item, joined by line
    /// breaks; and the first link that is not a file. `done` runs on the main
    /// queue, before this returns when nothing is promised. AppKit lets a
    /// destination receive promised files only inside its drag callbacks, as
    /// `perform` calls this.
    static func receive(_ info: NSDraggingInfo, for session: ExactSession?, done: @escaping ([URL], String, String) -> Void) {
        var found: [Int: [URL]] = [:], promises: [(Int, NSFilePromiseReceiver)] = []
        info.enumerateDraggingItems(options: [], for: nil, classes: [NSFilePromiseReceiver.self, NSURL.self],
                                    searchOptions: [.urlReadingFileURLsOnly: true]) { item, index, _ in
            if let promise = item.item as? NSFilePromiseReceiver { promises.append((index, promise)); found[index] = [] }
            else if let url = item.item as? URL { found[index] = [url] }
        }
        var texts: [String] = [], link = ""
        for (index, item) in (info.draggingPasteboard.pasteboardItems ?? []).enumerated() where found[index] == nil {
            if let text = item.string(forType: .string), !text.isEmpty { texts.append(text) }
            if link.isEmpty, let url = item.string(forType: .URL), URL(string: url).map({ !$0.isFileURL }) == true { link = url }
        }
        let text = texts.joined(separator: "\n")
        // Each item's files, a promise's in the order it names them
        var names: [Int: [String]] = [:]
        let files = { found.keys.sorted().flatMap { index in
            (found[index] ?? []).sorted { (names[index]?.firstIndex(of: $0.lastPathComponent) ?? .max) < (names[index]?.firstIndex(of: $1.lastPathComponent) ?? .max) }
        } }
        guard !promises.isEmpty, let session else { return done(files(), text, link) }
        let folder = stagingRoot.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        PastedImages.held(folder, by: session)
        // The reader hears each file a promise names, and AppKit names them once receiving starts; the callbacks come
        // to the main queue, after this has counted them
        var pending = 0
        for (index, promise) in promises {
            promise.receivePromisedFiles(atDestination: folder, options: [:], operationQueue: promiseQueue) { url, error in
                DispatchQueue.main.async {
                    if let error { session.log("drop: a promised file did not arrive: \(error.localizedDescription)") } else { found[index, default: []].append(url) }
                    pending -= 1
                    if pending == 0 { done(files(), text, link) }
                }
            }
            let named = promise.fileNames
            names[index] = named
            pending += max(1, named.count)
        }
    }

    /// Where promised files are received: a copy there is the session's,
    /// never a document the person keeps (`NodeView.drop`).
    static let stagingRoot = FileManager.default.temporaryDirectory.appendingPathComponent("exact-dropped", isDirectory: true)

    /// Where promised files are written, off the main queue, as AppKit asks.
    private static let promiseQueue: OperationQueue = {
        let queue = OperationQueue()
        queue.qualityOfService = .userInitiated
        return queue
    }()

    /// The drag ended: cancelled, or dropped where nothing took it, every
    /// node still entered leaves; `dropped`, none does (the DOM's `drop` is
    /// the last event of a drag that lands).
    static func ended(dropped: Bool = false, records: [ObjectIdentifier: String] = [:]) {
        turn &+= 1
        if dropped { entered = [] } else if !entered.isEmpty { update([], at: last, in: lastWindow, records: records) }
    }

    // The agent's drag (`tap <target> dragenter <path…>`, `tap … drop`,
    // `tap … dragleave`): the session a Finder drag would run, without one.
    /// A drag of files comes over `point`, and stays.
    static func agentEnter(at point: NSPoint, in window: NSWindow) {
        turn &+= 1
        update(chain(at: point, in: window), at: point, in: window)
    }
}

/// A scroller's clip view: the drag's one destination over its scroller
/// while the presenter holds a node hearing `drop`, `dragenter` or
/// `dragleave`, so that AppKit scrolls it under a drag held near its edge,
/// and the nodes inside register for nothing (LLP 1094.000 D15). As an
/// `NSTableView` finds the row under a drag by its point, the tracker finds
/// the node under it (`FileDrag.over`).
package class DropClipView: NSClipView {
    /// Whether this clip view is a destination.
    package var isDestination = false {
        didSet {
            guard isDestination != oldValue else { return }
            if isDestination { registerForDraggedTypes(FileDrag.types) } else { unregisterDraggedTypes() }
        }
    }
    package override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation { FileDrag.moved(self, sender) ?? [] }
    package override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation { FileDrag.moved(self, sender) ?? [] }
    package override func draggingExited(_ sender: NSDraggingInfo?) { FileDrag.exited(self, sender) }
    package override func performDragOperation(_ sender: NSDraggingInfo) -> Bool { FileDrag.perform(self, sender) ?? false }
    package override func draggingEnded(_ sender: NSDraggingInfo) { FileDrag.ended() }
}

extension Presenter {
    /// Each scroller's clip view is the drag's one destination over it while
    /// a node here hears a drag's events, and a node that hears them inside
    /// one registers for nothing (LLP 1094.000 D15); checked at each pass, as
    /// iOS installs its window's interaction.
    func syncDragDestinations() {
        let heard = views.values.contains { !$0.handlers.isDisjoint(with: FileDrag.events) }
        guard heard || dragDestinations else { return }
        dragDestinations = heard
        (viewport.contentView as? DropClipView)?.isDestination = heard
        for view in views.values { (view.scroll?.contentView as? DropClipView)?.isDestination = heard }
        for clip in dataTables.clips { clip.isDestination = heard }
        for view in views.values where !view.handlers.isDisjoint(with: FileDrag.events) { view.syncDropTypes() }
    }
}

// Forwarding a field's file drag (the header's last paragraph).
extension TextArea {
    package override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation { FileDrag.moved(self, sender) ?? super.draggingEntered(sender) }
    package override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation { FileDrag.moved(self, sender) ?? super.draggingUpdated(sender) }
    package override func draggingExited(_ sender: NSDraggingInfo?) { FileDrag.exited(self, sender); super.draggingExited(sender) }
    package override func prepareForDragOperation(_ sender: NSDraggingInfo) -> Bool { FileDrag.takes(self, sender) || super.prepareForDragOperation(sender) }
    package override func performDragOperation(_ sender: NSDraggingInfo) -> Bool { FileDrag.perform(self, sender) ?? super.performDragOperation(sender) }
    package override func concludeDragOperation(_ sender: NSDraggingInfo?) { if !FileDrag.takes(self, sender) { super.concludeDragOperation(sender) } }
}

extension FieldEditor {
    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation { FileDrag.moved(self, sender) ?? super.draggingEntered(sender) }
    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation { FileDrag.moved(self, sender) ?? super.draggingUpdated(sender) }
    override func draggingExited(_ sender: NSDraggingInfo?) { FileDrag.exited(self, sender); super.draggingExited(sender) }
    override func prepareForDragOperation(_ sender: NSDraggingInfo) -> Bool { FileDrag.takes(self, sender) || super.prepareForDragOperation(sender) }
    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool { FileDrag.perform(self, sender) ?? super.performDragOperation(sender) }
    override func concludeDragOperation(_ sender: NSDraggingInfo?) { if !FileDrag.takes(self, sender) { super.concludeDragOperation(sender) } }
}

// An input that is not being edited takes a drag itself (its cell's).
extension Field {
    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation { FileDrag.moved(self, sender) ?? super.draggingEntered(sender) }
    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation { FileDrag.moved(self, sender) ?? super.draggingUpdated(sender) }
    override func draggingExited(_ sender: NSDraggingInfo?) { FileDrag.exited(self, sender); super.draggingExited(sender) }
    override func prepareForDragOperation(_ sender: NSDraggingInfo) -> Bool { FileDrag.takes(self, sender) || super.prepareForDragOperation(sender) }
    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool { FileDrag.perform(self, sender) ?? super.performDragOperation(sender) }
    override func concludeDragOperation(_ sender: NSDraggingInfo?) { if !FileDrag.takes(self, sender) { super.concludeDragOperation(sender) } }
}

#if EXACT_AGENT
extension Agent {
    /// `tap <target> drop [<path…>] [--text "…"] [--url "…"]`, `dragenter`
    /// alike and `dragleave`: the drag another app would bring to the window
    /// point `p` over `v`, through the same routing, minting and events (no
    /// `dragleave` after a drop that lands). Nil for any other tap.
    func fileDragTap(_ req: [String: Any], _ v: NodeView, at p: NSPoint, in win: NSWindow, reply at: [Double]) -> [String: Any]? {
        if req["dragleave"] != nil {
            FileDrag.ended()
            session.presenter.settlePump()
            return ["tapped": Int(v.id), "dragleave": true, "delivery": "presenter"]
        }
        let text = req["text"] as? String ?? "", url = req["url"] as? String ?? ""
        if let paths = req["dragenter"] as? [String] {
            guard !paths.isEmpty || !text.isEmpty || !url.isEmpty else { return ["malformed": true, "error": "dragenter: expected the dragged paths, --text or --url"] }
            FileDrag.agentEnter(at: p, in: win)
            session.presenter.settlePump()
            return ["tapped": Int(v.id), "at": at, "dragenter": paths.count, "delivery": "presenter"]
        }
        guard let paths = req["drop"] as? [String] else { return nil }
        guard !paths.isEmpty || !text.isEmpty || !url.isEmpty else { return ["malformed": true, "error": "drop: expected the dropped paths, --text or --url"] }
        let hit = FileDrag.hit(at: p, in: win) ?? v
        guard let node = FileDrag.destination(hit, files: !paths.isEmpty) else {
            FileDrag.ended()
            if NodeView.dropTarget(hit) != nil { return ["error": "drop: the field under the point inserts text and links itself (LLP 1094.001 D2), which the agent's drop does not: drop on the node's own box, or declare `drop` on the field"] }
            return ["error": "view \(v.id) takes no drop: nothing under it declares `drop`"]
        }
        // The record is the release's, taken before the enters run; the node takes the drop only if it still may then
        let line = node.dragLine(p), id = node.id
        FileDrag.agentEnter(at: p, in: win)
        FileDrag.heldEnters(at: p)
        let live = node.presenter?.views[id] === node && node.handlers.contains("drop") && !node.inert && !node.formDisabled
        let ok = live && node.drop(paths.map { URL(fileURLWithPath: $0) }, text: text, url: url, line: line)
        FileDrag.ended(dropped: ok)
        session.presenter.settlePump()
        guard ok else { return ["error": "drop: refused: no file could be opened, and it carried no text or link"] }
        return ["tapped": Int(node.id), "at": at, "drop": paths.count, "delivery": "presenter"]
    }
}
#endif
#endif
