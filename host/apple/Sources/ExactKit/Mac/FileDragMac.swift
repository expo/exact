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
        return destination(view, files: carriesFiles(info))
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
        guard let content = window?.contentView, content.bounds.contains(content.convert(point, from: nil)) else { return [] }
        var view = TableRowCell.whatIsThere { content.hitTest(content.superview?.convert(point, from: nil) ?? point) }
        var out: [NodeView] = []
        while let at = view {
            if let node = at as? NodeView, !node.inert, !node.formDisabled,
               node.handlers.contains("dragenter") || node.handlers.contains("dragleave") { out.append(node) }
            view = at.superview
        }
        return out
    }

    /// The drag is over `next`: the nodes it came over hear `dragenter`,
    /// then those it left `dragleave`, each innermost first, as the DOM
    /// fires the new target's enter (bubbling) before the old one's leave.
    static func update(_ next: [NodeView], at point: NSPoint) {
        let before = entered
        entered = next
        last = point
        for node in next where node.handlers.contains("dragenter") && !before.contains(where: { $0 === node }) {
            if waiting[ObjectIdentifier(node)] != nil { held.insert(ObjectIdentifier(node)); continue }
            node.presenter?.mouseEvent(node.id, 55, node.dragLine(point))
        }
        for node in before where !next.contains(where: { $0 === node }) {
            if held.remove(ObjectIdentifier(node)) != nil || !node.handlers.contains("dragleave") { continue }
            node.presenter?.mouseEvent(node.id, 56, node.dragLine(point))
        }
    }

    /// The drag drops, into a node or a field: an enter it has held is heard first.
    static func heldEnters(at point: NSPoint) {
        for node in entered where held.remove(ObjectIdentifier(node)) != nil { release(node, at: point) }
    }

    /// A held enter is heard, unless the node has since stopped hearing it (the waiting drop's action disabled it or
    /// made it inert): then the drag is over it no longer, with no leave for an enter it never heard.
    private static func release(_ node: NodeView, at point: NSPoint) {
        guard !node.inert, !node.formDisabled, node.handlers.contains("dragenter") else { return entered.removeAll { $0 === node } }
        node.presenter?.mouseEvent(node.id, 55, node.dragLine(point))
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
        update(chain(at: info.draggingLocation, in: view.window), at: info.draggingLocation)
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
        update(chain(at: point, in: view.window), at: point)
        guard !entered.isEmpty else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { if turn == mine { update([], at: last) } }
    }

    /// A destination's `performDragOperation` at `view`: the drop at the
    /// node that takes it, once its promised files are received, and the
    /// drag's end. The nodes entered when it dropped are kept: a drop that
    /// carried nothing that arrived is their leave, whenever it resolves.
    /// Nil when no node takes it.
    static func perform(_ view: NSView, _ info: NSDraggingInfo) -> Bool? {
        guard let target = destination(view, info) else { heldEnters(at: info.draggingLocation); ended(); return nil }
        // The records are the release's: the point in each box and the keys held as it dropped
        let point = info.draggingLocation, id = target.id, line = target.dragLine(point)
        heldEnters(at: point)
        let dropped = entered, leaves = entered.filter { $0.handlers.contains("dragleave") }.map { ($0, $0.dragLine(point)) }
        ended(dropped: true)
        var landed: Bool?, delayed = false
        receive(info, for: target.presenter?.session) { files, text, url in
            // A node gone, or no longer hearing `drop`, while its promised files arrived takes nothing
            let live = target.presenter?.views[id] === target && target.handlers.contains("drop")
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
    static func ended(dropped: Bool = false) {
        turn &+= 1
        if dropped { entered = [] } else if !entered.isEmpty { update([], at: last) }
    }

    // The agent's drag (`tap <target> dragenter <path…>`, `tap … drop`,
    // `tap … dragleave`): the session a Finder drag would run, without one.
    /// A drag of files comes over `point`, and stays.
    static func agentEnter(at point: NSPoint, in window: NSWindow) {
        turn &+= 1
        update(chain(at: point, in: window), at: point)
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
        let hit = win.contentView?.hitTest(p) ?? v
        guard let node = FileDrag.destination(hit, files: !paths.isEmpty) else {
            FileDrag.ended()
            if NodeView.dropTarget(hit) != nil { return ["error": "drop: the field under the point inserts text and links itself (LLP 1094.001 D2), which the agent's drop does not: drop on the node's own box, or declare `drop` on the field"] }
            return ["error": "view \(v.id) takes no drop: nothing under it declares `drop`"]
        }
        FileDrag.agentEnter(at: p, in: win)
        FileDrag.heldEnters(at: p)
        let ok = node.drop(paths.map { URL(fileURLWithPath: $0) }, text: text, url: url, line: node.dragLine(p))
        FileDrag.ended(dropped: ok)
        session.presenter.settlePump()
        guard ok else { return ["error": "drop: refused: no file could be opened, and it carried no text or link"] }
        return ["tapped": Int(node.id), "at": at, "drop": paths.count, "delivery": "presenter"]
    }
}
#endif
#endif
