// @ref LLP 1075.003.000 — nodes the Contract marks `hatch="word"`: the app
// module's `element` hatch receives each one's view and the platform object
// of its kind, after the batch that mounts it (`built`), when its `data-*`
// words change (`changed`), and before its view leaves (`ended`); a cold
// launch's are replayed once the module connects. A hatched node leaves the
// fast path, and this says so: on iOS it is never a flat leaf and its list
// row is never parked (FlatLeavesIOS, NodePoolIOS, by its `hatch` prop),
// unless its hatch made it `reusable` (LLP 1075.003.000.000 §8); the journal names what
// each word gave up, once, and warns once for a word in a list's row;
// `state.hatches` counts them. Shared by the AppKit and UIKit presenters; the
// module side is ExactNativeModule.swift, the call NativeHatches.swift.
import Foundation
import ExactKit
#if os(macOS)
import AppKit
#else
import UIKit
#endif

final class ElementHatches: ElementHatching {
    unowned let presenter: Presenter
    private struct Entry {
        let node: NodeView
        var told: Bool
        var data: String
        let inList: Bool
    }
    private var nodes: [UInt32: Entry] = [:]
    /// The words the journal has named, those it has warned for, and those
    /// it has said a reusable hatch frees.
    private var said: Set<String> = [], warned: Set<String> = [], freed: Set<String> = []
    /// Calls by word and moment, since launch.
    private var calls: [String: [String: Int]] = [:]
    /// The words the plan marks that the module does not handle.
    private var unhandled: Set<String> = []
    /// The words the plan marks and does not give this platform.
    private var unhandledByPlan: Set<String> = []
    /// What hatches told the agent of what they made (HatchRegions.swift).
    lazy var regions = HatchRegions(presenter)
    /// Hatched frames and videos waiting for the module's hatches, and
    /// nodes that heard `configuring` before their `built`.
    private var waiting: [UInt32: () -> NodeView?] = [:]
    private var configured: Set<UInt32> = []

    init(_ presenter: Presenter) { self.presenter = presenter }

    /// What a hatched node gives up on this host: on iOS its flat leaf, and
    /// its row's reuse unless its hatch is `reusable`.
    static func lost(reusable: Bool) -> [String] {
        #if os(iOS) || os(tvOS)
        reusable ? ["flat"] : ["flat", "pool"]
        #else
        []
        #endif
    }

    // MARK: The moments

    /// A node the batch created: told once the batch is applied.
    func created(_ node: NodeView) {
        // A word the plan does not give this platform (the Rust host sends it
        // as `hatchOff`, LLP 1075.003.000.001 §4.3): shown, never called, listed.
        if let off = node.props["hatchOff"], unhandledByPlan.insert(off).inserted {
            presenter.session?.log("hatch element \(off): the plan does not give it to this platform; its nodes are shown and never called")
        }
        guard node.props["hatch"] != nil else { return }
        nodes[node.id] = Entry(node: node, told: false, data: node.props["dataset"] ?? "", inList: false)
        presenter.afterBatch { [weak self, weak node] in if let self, let node { self.build(node) } }
    }

    private func build(_ node: NodeView) {
        guard let known = nodes[node.id], known.node === node, !known.told, presenter.views[node.id] === node,
              let natives = presenter.session?.nativeViews, natives.hatchesConnected, let word = node.props["hatch"] else { return }
        // A word the installed module was not built to handle is never
        // called (LLP 1075.003.000.001 §4.3): shown, journaled once, listed.
        if let handled = natives.handledHatches, !handled.contains(word) {
            if unhandled.insert(word).inserted {
                presenter.session?.log("hatch element \(word): not handled by this build's module; its nodes are shown and never called")
            }
            nodes.removeValue(forKey: node.id)
            return
        }
        let entry = Entry(node: node, told: true, data: known.data, inList: Self.list(holding: node) != nil)
        nodes[node.id] = entry
        configured.remove(node.id)
        let reusable = call(.built, entry)
        say(word, node: node, inList: entry.inList, reusable: reusable)
    }

    /// A hatched node's props changed: its hatch hears new `data-*` words.
    func propsChanged(_ id: UInt32) {
        presenter.session?.nativeViews?.rootPropsChanged(id)   // a root's words are the app hatch's (ScopeHatches.swift)
        guard var entry = nodes[id], entry.node.props["dataset"] ?? "" != entry.data else { return }
        entry.data = entry.node.props["dataset"] ?? ""
        nodes[id] = entry
        guard entry.told else { return }
        presenter.afterBatch { [weak self] in
            guard let self, let now = self.nodes[id], now.node === entry.node, let word = now.node.props["hatch"] else { return }
            let reusable = self.call(.changed, now)
            self.say(word, node: now.node, inList: now.inList, reusable: reusable)
        }
    }

    /// A held heavy leaf (a video, frame or native view in a list) is made
    /// after its hatch heard `built`: the hatch hears `changed`, its platform
    /// object there now.
    func realized(_ node: NodeView) {
        guard let entry = nodes[node.id], entry.node === node, entry.told, let word = node.props["hatch"] else { return }
        let reusable = call(.changed, entry)
        say(word, node: node, inList: entry.inList, reusable: reusable)
    }

    /// A frame's web view or a video's item is about to be made: its hatch
    /// hears `configuring` synchronously, before construction (LLP 1133 D3).
    /// Within a batch or a held leaf's release, so the node may not be in
    /// `presenter.views` yet, nor have a superview.
    func configuring(_ node: NodeView, _ configuration: AnyObject) {
        guard let word = node.props["hatch"], let natives = presenter.session?.nativeViews, natives.hatchesConnected,
              natives.handledHatches?.contains(word) != false else { return }
        let count = (calls[word]?[RouteHatchEvent.configuring.name] ?? 0) + 1
        calls[word, default: [:]][RouteHatchEvent.configuring.name] = count
        configured.insert(node.id)
        natives.elementHatch(node, event: RouteHatchEvent.configuring.rawValue, platform: configuration, quiet: count > 1 && Self.inList(node, presenter))
    }

    /// Whether a hatched frame's web view or video's item waits for the module
    /// to connect, so its hatch hears `configuring` before the first one, on a
    /// cold launch's first screen too. A word the module does not handle,
    /// or a module that will not connect, waits for nothing.
    func waits(_ node: NodeView) -> Bool {
        guard let word = node.props["hatch"], presenter.session?.nativeViews?.hatchesMayConnect(word) == true else { return false }
        waiting[node.id] = { [weak node] in node }
        return true
    }

    /// The hatches connected, or never will: the waiting objects are made.
    func releaseConfigurations() {
        let waiting = waiting
        self.waiting = [:]
        for id in waiting.keys.sorted() {
            if let node = waiting[id]?(), presenter.views[id] === node {
                if node.kind == "video" { node.videoUpdate() } else { node.embedFrame(presenter) }
            }
        }
        // Outside a batch: a held frame outside a list's rows is made now, as
        // at a batch's end, not by the tick, which makes only the near ones.
        if !waiting.isEmpty { presenter.leavesMoved() }
    }

    /// Before a batch's ops: every hatched node it destroys ends now, its view
    /// still in its row. A row's root is destroyed before its children, and
    /// the node pool decides at the root whether the row parks, so a
    /// `reusable` hatch must have undone its additions by then (iOS).
    func begin(_ batch: Batch) {
        regions.sweep()
        guard !nodes.isEmpty else { return }
        for op in batch.ops where op.op == .destroy { destroyed(op.id) }
    }

    /// A node the batch destroys: its hatch hears `ended` while its view is
    /// still there.
    func destroyed(_ id: UInt32) {
        waiting[id] = nil
        // Configured and not yet built: its element ends too.
        guard let entry = nodes.removeValue(forKey: id), configured.remove(id) != nil || entry.told else { return }
        call(.ended, entry)
    }

    /// Every hatched node is leaving (a reload, the session's end).
    func reset() {
        for id in nodes.keys.sorted() { destroyed(id) }
        nodes = [:]
        waiting = [:]
        configured = []
        regions.reset()
        // The window and the app end with them, and are built again after a reload (ScopeHatches.swift).
        presenter.session?.nativeViews?.scopesReset()
    }

    /// The module connected after these were built (LLP 1075.003 Q3 (c)).
    func replay() {
        releaseConfigurations()
        for id in nodes.keys.sorted() { if let entry = nodes[id], !entry.told { build(entry.node) } }
    }

    /// The call, and whether the hatch made the node `reusable` (its view
    /// carries the answer to the node pool, LLP 1075.003.000.000 §8).
    @discardableResult
    private func call(_ event: RouteHatchEvent, _ entry: Entry) -> Bool {
        let id = entry.node.id
        let word = entry.node.props["hatch"] ?? ""
        calls[word, default: [:]][event.name, default: 0] += 1
        // In a list's row, the first of each moment is journaled; `state`
        // counts the rest, so a fling does not flood the journal.
        let quiet = entry.inList && (calls[word]?[event.name] ?? 0) > 1
        // A development build notices a recognizer the call added and did not declare (§3.4).
        let had = HatchDiagnosticsLink.measuring && event != .ended ? regions.recognizers(under: entry.node) : nil
        let reusable = presenter.session?.nativeViews?.elementHatch(entry.node, event: event.rawValue, platform: Self.platform(of: entry.node, presenter), quiet: quiet) ?? false
        if let had { regions.undeclared(under: entry.node, by: "element \(word)") { had.contains(ObjectIdentifier($0)) } }
        // A frame's arm says what the call changed of what it owns (LLP 1020.002 §9).
        if entry.node.kind == "iframe", event != .ended { presenter.session?.webViewHatched(id) }
        // What the node's hatch registered ends with it; a view lent to another node starts clean.
        if event == .ended { regions.ended(node: id) }
        #if os(iOS) || os(tvOS)
        // The answer is the view's while it is still this node's: a click
        // inside the hatch (after the batch) can have replaced the node and
        // given its view to another. At `ended` it is the last word.
        if event == .ended || (nodes[id]?.node === entry.node && presenter.views[id] === entry.node) {
            entry.node.hatchReusable = reusable
        }
        #endif
        return reusable
    }

    // MARK: Saying so (LLP 1075.003.000 §3.5)

    private func say(_ word: String, node: NodeView, inList: Bool, reusable: Bool) {
        let lost = Self.lost(reusable: reusable)
        if said.insert(word).inserted {
            let gave = lost.isEmpty ? "nothing beyond the call on this host"
                : lost.contains("pool") ? "a view, not a flat leaf; its row is not reused"
                : "a view, not a flat leaf; its row is reused, its hatch undoing what it adds"
            presenter.session?.log("hatch element \(word): \(gave) (LLP 1075.003.000)")
            if !lost.isEmpty, !lost.contains("pool") { freed.insert(word) }
        } else if !lost.isEmpty, !lost.contains("pool"), freed.insert(word).inserted {
            presenter.session?.log("hatch element \(word): its hatch undoes what it adds; its row is reused (LLP 1075.003.000.000 §8)")
        }
        if inList, warned.insert(word).inserted, let list = Self.list(holding: node) {
            let name = list.props["testId"].map { "list \($0)" } ?? "list #\(list.id)"
            presenter.session?.log("hatch element \(word) is in a row of \(name): each row's mount calls its hatch on the main thread"
                + (lost.contains("pool") ? ", and its row is never reused" : ""))
        }
    }

    /// Whether `node` is in a list's row, or in one the batch is making (iOS).
    private static func inList(_ node: NodeView, _ presenter: Presenter) -> Bool {
        #if os(iOS) || os(tvOS)
        if presenter.listCreating(node.id) { return true }
        #endif
        return list(holding: node) != nil
    }

    /// The virtualized list whose row holds `node`, if any.
    private static func list(holding node: NodeView) -> NodeView? {
        var at = node.superview
        while let view = at {
            if let n = view as? NodeView, n.kind == "list" { return n }
            at = view.superview
        }
        return nil
    }

    /// `state.hatches`: under `words`, per word, how many are live, what they
    /// gave up, and the calls so far (LLP 1075.003.000.001 §3.3: words are
    /// apart from the reply's other fields, so no word can be taken for one).
    func observation(_ diagnostics: (any HatchDiagnosing)?) -> [String: Any] {
        var live: [String: Int] = [:], reused: [String: Int] = [:]
        for entry in nodes.values {
            let word = entry.node.props["hatch"] ?? ""
            live[word, default: 0] += 1
            #if os(iOS) || os(tvOS)
            if entry.node.hatchReusable { reused[word, default: 0] += 1 }
            #endif
        }
        var out: [String: Any] = [:]
        for word in Set(live.keys).union(calls.keys) {
            // A word whose live nodes are all reusable gives up only its flat leaf.
            let reusable = (reused[word] ?? 0) > 0 && reused[word] == live[word]
            out[word] = ["live": live[word] ?? 0, "reusable": reused[word] ?? 0, "lost": Self.lost(reusable: reusable),
                         "calls": calls[word] ?? [:]] as [String: Any]
        }
        // What each hatch counted and published, and the other scopes (HatchDiagnostics.swift).
        var reply = diagnostics?.state(words: out) ?? ["words": out]
        if let handled = presenter.session?.nativeViews?.handledHatches { reply["platform"] = handled.sorted() }
        reply["unhandled"] = unhandledByPlan.sorted().map { ["word": $0, "reason": "plan"] } + unhandled.sorted().map { ["word": $0, "reason": "module"] }
        reply["refused"] = refused
        reply["inFlight"] = inFlight
        regions.state(into: &reply)
        // What the last run left in its crash breadcrumb (§4.4), if anything.
        if !HatchBreadcrumb.lastEnds.isEmpty { reply["lastEnd"] = HatchBreadcrumb.lastEnds }
        return reply
    }

    #if os(iOS) || os(tvOS)
    private var reported: Set<String> = []
    /// Before each batch, in a development build (LLP 1075.003 §3.5,
    /// 1075.003.000 §3.6): what Exact owns of each hatched node's scroll view,
    /// text field or text view against what it writes, journaled once.
    func checkOwned() {
        guard Presenter.checksHatchOwnership, !nodes.isEmpty, presenter.session?.natives.hatchesConnected == true else { return }
        for entry in nodes.values where entry.told {
            let node = entry.node
            var changed: [String] = []
            changed += presenter.scrollOwnedChanges(node)
            if let field = node.field, field.delegate !== node { changed.append("the text field's delegate") }
            if let text = node.textArea, text.delegate !== node { changed.append("the text view's delegate") }
            for property in changed {
                let line = "element \(node.props["hatch"] ?? "") #\(node.id): \(property) changed outside Exact, which owns it"
                if reported.insert(line).inserted { presenter.session?.log(line) }
            }
        }
    }
    #endif

    // MARK: The node's platform object (§3.2)

    /// The platform object of the node's kind, or nil: a text field or view,
    /// a control (a switch, slider, date picker…), an iframe's `WKWebView` or
    /// a local PDF's `PDFView` (LLP 1020.002 §9), a video's `AVPlayer`, a scroll view.
    static func platform(of node: NodeView, _ presenter: Presenter) -> AnyObject? {
        if node.kind == "video" { return node.videoPlatform }
        #if os(iOS) || os(tvOS)
        return node.field ?? node.textArea ?? presenter.control(of: node.id) ?? node.web ?? node.scrollView
        #else
        return node.field ?? node.textArea ?? presenter.control(of: node.id) ?? node.web ?? node.scroll
        #endif
    }

    // MARK: Acting on an element (LLP 1075.003 §3.4)

    /// What a hatch asks of an element is queued, never run here (LLP
    /// 1075.003.000.001 §2.5): it runs on a later turn of the main queue,
    /// after the batch being applied and after the session has finished with
    /// it, so a hatch called from a batch or a reset never applies a batch of
    /// its own there. One FIFO a session, at most 256 acts and 1 MB of the
    /// text they carry (an `input`'s, a tab container's URL), an act past
    /// either refused by name and counted. False when refused.
    @discardableResult
    func later(_ id: UInt32, _ name: String, bytes: Int = 0, _ work: @escaping () -> Void) -> Bool {
        later(named(id), name, bytes: bytes, work)
    }

    /// The same, for an act the journal names by `what` rather than by a node.
    @discardableResult
    func later(_ what: String, _ name: String, bytes: Int = 0, _ work: @escaping () -> Void) -> Bool {
        guard queue.count < Self.maxQueued, queuedBytes + bytes <= Self.maxBytes else {
            refused += 1
            presenter.session?.log("hatch \(what): \(name) refused: the act queue is full")
            return false
        }
        queue.append(Act(what: what, name: name, bytes: bytes, work: work))
        queuedBytes += bytes
        if !scheduled {
            scheduled = true
            DispatchQueue.main.async { [weak self] in self?.scheduled = false; self?.drain() }
        }
        return true
    }

    /// Refuse text an act carries over 64 KB, at the asking: counted, and
    /// journaled under the caller's `name` with the text's length as the
    /// reason. False when the text is within the bound.
    func refusedOverBound(_ what: String, _ name: String, bytes: Int) -> Bool {
        guard bytes > hatchTextBytes else { return false }
        refused += 1
        presenter.session?.log("hatch \(what): \(name) refused: \(bytes) bytes is over 64 KB")
        return true
    }

    private struct Act { let what: String, name: String, bytes: Int, work: () -> Void }
    static let maxQueued = 256, maxBytes = 1 << 20, snapshot = 64
    private var queue: [Act] = [], queuedBytes = 0, scheduled = false
    /// Acts refused, and drains run, since launch; the acts still queued.
    private(set) var refused = 0, drains = 0
    var inFlight: Int { queue.count }

    /// Run the acts queued when this starts, at most 64, in order. What they
    /// queue (a `changed` that clicks again) waits for a later drain: the
    /// main queue's next turn, or the agent's settle (Agent.swift), which
    /// drains here on the main thread it is blocking.
    func drain() {
        guard !queue.isEmpty else { return }
        drains += 1
        let batch = Array(queue.prefix(Self.snapshot))
        queue.removeFirst(batch.count)
        for act in batch {
            queuedBytes -= act.bytes
            act.work()
        }
        if !queue.isEmpty, !scheduled {
            scheduled = true
            DispatchQueue.main.async { [weak self] in self?.scheduled = false; self?.drain() }
        }
    }

    private func said(_ id: UInt32, _ line: String) { presenter.session?.log("hatch \(named(id)): \(line)") }
    private func named(_ id: UInt32) -> String { presenter.views[id]?.props["hatch"].map { "element \($0) #\(id)" } ?? "node #\(id)" }

    /// A hatch's `input(text)` (§2.5): an authored text field's whole value,
    /// through the field's own primitive, cut to its limits, reported as a
    /// keystroke's change is, without moving focus. Journaled by length,
    /// never by text. False when refused at the asking; a field that cannot
    /// take it when the act runs is refused there, by name.
    func input(_ id: UInt32, _ text: String) -> Bool {
        guard let node = presenter.views[id] else { return false }
        let bytes = text.utf8.count
        if refusedOverBound(named(id), "input", bytes: bytes) { return false }
        return later(id, "input", bytes: bytes) { [weak self, weak node] in
            guard let self, let node, self.presenter.views[id] === node else { return }
            let refuse = { (why: String) in self.refused += 1; self.said(id, "input refused: \(why)") }
            guard node.field != nil || node.textArea != nil else { return refuse("not an editable text field") }
            guard !node.disabled else { return refuse("the field is disabled") }
            guard node.props["editable"] != "false", node.props["readonly"] != "true" else { return refuse("the field is readonly") }
            let value = TextInputLimit.prefix(text, props: node.props)
            #if os(iOS) || os(tvOS)
            if let f = node.field {
                guard f.markedTextRange == nil else { return refuse("the field is composing") }
                f.text = value
            } else if let t = node.textArea {
                guard t.markedTextRange == nil else { return refuse("the field is composing") }
                t.text = value
            }
            #else
            if let f = node.field {
                // While it is edited the field editor holds the text; focus stays where it is.
                if let editor = f.currentEditor() as? NSTextView {
                    guard !editor.hasMarkedText() else { return refuse("the field is composing") }
                    editor.replaceCharacters(in: NSRange(location: 0, length: (editor.string as NSString).length), with: value)
                } else {
                    f.stringValue = value
                }
            } else if let t = node.textArea {
                guard !t.hasMarkedText() else { return refuse("the field is composing") }
                t.replaceCharacters(in: NSRange(location: 0, length: (t.string as NSString).length), with: value)
            }
            #endif
            self.presenter.hatchWrote(node, value)
            self.said(id, "input (\(node.props["type"] == "password" ? "protected" : "\(value.count) chars"), delivery: hatch)")
        }
    }

    /// A hatch's `click()`, `focus()` or `blur()` on a node, as the DOM's, on
    /// the next turn (`later`). False when refused.
    func act(_ id: UInt32, _ action: UInt32) -> Bool {
        #if os(iOS) || os(tvOS)
        return presenter.hatchAct(id, action)
        #else
        guard let node = presenter.views[id] else { return false }
        switch action {
        case 0:
            guard node.handlers.contains("press"), !node.disabled else { return false }
            return later(id, "click") { [weak self, weak node] in
                guard let self, let node, self.presenter.views[id] === node else { return }
                self.said(id, "click (delivery: hatch)")
                NavigationMarks.shared.acting(.program) { self.presenter.press(id) }
            }
        case 1, 2:
            return later(id, action == 1 ? "focus" : "blur") { [weak self, weak node] in
                guard let self, let node, self.presenter.views[id] === node, let window = node.window else { return }
                self.said(id, "\(action == 1 ? "focus" : "blur") (delivery: hatch)")
                let responder: NSView = node.textArea ?? node.field ?? node
                if action == 1 {
                    if responder.acceptsFirstResponder { window.makeFirstResponder(responder) }
                } else if window.firstResponder === responder || window.firstResponder === node.field?.currentEditor() {
                    window.makeFirstResponder(nil)
                }
            }
        default: return false
        }
        return true
        #endif
    }
}

extension ElementHatches {
    func decorateRegions(_ tree: [String: Any]) -> [String: Any] { regions.decorate(tree) }
    func regionOwner(of view: PlatformView) -> String? { regions.owner(of: view) }
    func regionPart(owning object: AnyObject) -> String? { regions.part(owning: object) }
    func regionPart(_ id: String, of node: UInt32) -> PlatformView? { regions.part(id, of: node)?.1 }
    func regionCoverage(joined: Set<String>) -> [[String: Any]] { regions.coverage(joined: joined) }
}

// The family's own code reaches its objects through the core's holders.
extension Presenter {
    var hatchElements: ElementHatches? { elements as? ElementHatches }
}
extension ExactSession {
    var nativeViews: NativeViews? { natives as? NativeViews }
}
