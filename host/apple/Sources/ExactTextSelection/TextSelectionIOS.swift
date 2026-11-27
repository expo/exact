// @ref LLP 1115.000 D11.6 — `user-select="text"` on UIKit. Text its author
// made selectable is selected as a hand-built screen's is: a long press or a
// double tap takes the word under it with UIKit's handles, loupe and
// highlight, the handles drag, and the edit menu is UIKit's (Copy, Look Up,
// Translate, Search Web, Share…). All of that is a `UITextInteraction` in
// its non-editable mode; what Exact supplies is the text and its geometry,
// from the paragraph's own lines (`paragraphLayout`), so a selection's rects
// follow its glyph runs on the lines the kernel broke.
//
// A selectable prose container shares one input and one touch surface across
// its paragraphs (TextSelectionDocumentIOS). The surface paints over child
// backgrounds so highlights and handles stay visible above opaque blocks.
// A paragraph with a context menu keeps its own interaction so its row's menu
// still wins the long press. Nothing is installed until text is touched;
// at most TextSelectionHost.kept independent selections keep interactions.
// A single tap with a selection up clears it without pressing; tapping outside
// the selection clears it while still delivering the tap there.
//
// CSS `user-select: all` (CSS UI 4 §6.1): a selection that takes part of
// such an element takes all of it, so a long press, a double tap or a handle
// that reaches into one selects the element, not a word. A tap selects
// nothing, as on a `UITextView`: a click's selection is the web's, not
// UIKit's. A shared input expands the selection across the element's paragraphs.
#if os(iOS)
import CoreText
import ExactCore
import ExactKit
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// The paragraphs that hold a text interaction, and the one selection.
final class TextSelectionHost: TextSelecting {
    private weak var presenter: Presenter?
    /// Oldest first.
    private(set) var texts: [SelectableText] = []
    /// Passive observers mounted on session roots while a selection is active,
    /// so tapping any view or native control outside the selection clears it.
    private var dismissObservers: [SelectionDismissObserver] = []
    /// Touch whose dismissal tap landed on highlighted text, recorded before
    /// `clear()` so `NodeView.touchesEnded` still suppresses press activation.
    private weak var suppressedTapTouch: UITouch?
    /// How many paragraphs keep their interaction once a finger has left.
    static let kept = 3
    /// Where Copy writes: the system's pasteboard (a test names its own,
    /// since a unit test's process may not read the system's).
    var pasteboard = UIPasteboard.general
    init(_ presenter: Presenter) { self.presenter = presenter }

    /// Whether `node` is text a reader can select now: all of it, or only
    /// its `user-select: all` runs (`SelectableText.atomic`).
    func admits(_ node: NodeView) -> Bool {
        node.isParagraph && presenter?.views[node.id] === node && !node.inert && !node.isHidden
            && (node.textSelectable || node.inlineText.contains(where: \.selectsAll))
    }
    /// The input that owns touches on `node`. Paragraphs inside a nested
    /// `UIScrollView` or pressable ancestor stay local so hit testing never
    /// routes their touches out of the nested scroller to an outer surface.
    private func touchText(for node: NodeView) -> SelectableText? {
        if let exact = texts.first(where: { $0.document == nil && $0.node === node }) { return exact }
        return texts.first { held in
            guard let doc = held.document, let owner = doc.owner, doc.contains(node) else { return false }
            return SelectionDocument.sharesTouchSurface(node, with: owner)
        }
    }
    func text(for node: NodeView) -> SelectableText? {
        // An active selection wins for selection queries (`copy`, `selectAll`,
        // `hasSelection`) so an idle standalone input inside a nested scroller
        // cannot shadow an active spanning document.
        if let active = texts.first(where: { $0.selected.length > 0 && ($0.node === node || $0.document?.contains(node) == true) }) {
            return active
        }
        return touchText(for: node)
    }
    /// The selection, if there is one: its paragraph, and its range in that
    /// paragraph's own text (its source, as `selectionchange` reports it).
    var selection: (node: NodeView, text: String, range: NSRange)? {
        for held in texts { if let node = held.node, let found = held.reportable { return (node, found.text, found.range) } }
        return nil
    }

    func touched(_ node: NodeView) {
        guard touchText(for: node) == nil, admits(node) else { return }
        install(node)
    }
    func surface(for node: NodeView) -> UIView? { touchText(for: node)?.document?.surface }
    func routesToSurface(_ node: NodeView) -> Bool {
        if let held = touchText(for: node) { return held.document != nil }
        guard admits(node), let found = documentParagraphs(for: node) else { return false }
        return SelectionDocument.sharesTouchSurface(node, with: found.owner)
    }
    func hasSelection(in view: UIView) -> Bool {
        texts.contains { held in
            guard held.selected.length > 0 else { return false }
            if let doc = held.document { return doc.selects(view, in: held.selected) }
            return held.node === view
        }
    }
    var hasActiveSelection: Bool { texts.contains { $0.selected.length > 0 } }
    func dismissedSelection(for touch: UITouch) -> Bool { suppressedTapTouch === touch }
    func recordSuppressedTap(_ touch: UITouch) { suppressedTapTouch = touch }
    func owns(_ view: UIView) -> Bool {
        texts.contains { held in
            guard held.selected.length > 0 else { return false }
            let hostView = held.textInputView
            if view === hostView || view.isDescendant(of: hostView) { return true }
            if let doc = held.document {
                guard let owner = doc.owner, let part = doc.part(for: view) else { return false }
                return SelectionDocument.sharesTouchSurface(part.node, with: owner)
            }
            return held.node === view
        }
    }
    func canSelectAll(in node: NodeView) -> Bool { text(for: node)?.canSelectAll == true }
    @discardableResult
    func selectAll(_ node: NodeView) -> Bool { text(for: node)?.selectAllDocument() == true }
    func interactionHolds(_ gesture: UIGestureRecognizer, on paragraph: NodeView) -> Bool {
        touchText(for: paragraph)?.interaction.gesturesForFailureRequirements.contains { $0 === gesture } == true
    }
    @discardableResult
    func install(_ node: NodeView) -> SelectableText {
        if let held = touchText(for: node) { return held }
        while texts.count >= Self.kept, let old = texts.first(where: { !$0.busy }) { drop(old) }
        let document = documentParagraphs(for: node).flatMap {
            SelectionDocument.sharesTouchSurface(node, with: $0.owner)
                ? SelectionDocument(owner: $0.owner, paragraphs: $0.paragraphs, host: self)
                : nil
        }
        if let document, let owner = document.owner {
            for old in texts where !old.busy && old.document == nil {
                if let n = old.node, document.contains(n), SelectionDocument.sharesTouchSurface(n, with: owner) { drop(old) }
            }
        }
        let made = SelectableText(node: node, host: self, document: document)
        texts.append(made)
        return made
    }
    /// The container and paragraphs a selection document of `node`'s would
    /// span: more than one paragraph, none with a menu of its own.
    private func documentParagraphs(for node: NodeView) -> (owner: NodeView, paragraphs: [NodeView])? {
        guard let owner = SelectionDocument.container(of: node) else { return nil }
        let paragraphs = SelectionDocument.paragraphs(in: owner, host: self)
        guard paragraphs.count > 1, paragraphs.allSatisfy({ $0.contextMenuSource == nil && !$0.hasInlineContextMenu }) else { return nil }
        return (owner, paragraphs)
    }
    func remove(_ node: NodeView) {
        for held in texts where held.node === node || held.document?.contains(node) == true { drop(held) }
    }
    private func drop(_ held: SelectableText) {
        texts.removeAll { $0 === held }
        held.detach()
        syncDismissObservers()
    }
    /// After a batch: text that changed drops its selection, text
    /// that is no longer selectable gives its interaction back, and a
    /// selection whose paragraph was laid out again is shown where it is.
    /// Idle standalone inputs whose scroll or pressable boundary went away
    /// are reconciled back to their enclosing document.
    func batchEnded() {
        for held in texts where !held.settle() { drop(held) }
        for held in texts where !held.busy && held.document == nil {
            guard let n = held.node else { continue }
            if let found = documentParagraphs(for: n), SelectionDocument.sharesTouchSurface(n, with: found.owner) {
                drop(held)
            }
        }
        syncDismissObservers()
    }
    private func selectionAffected(by scrollView: UIScrollView, held: SelectableText) -> Bool {
        if let doc = held.document {
            return doc.owner?.scrollView === scrollView || doc.parts.contains(where: { $0.node.isDescendant(of: scrollView) })
        }
        return held.node?.isDescendant(of: scrollView) == true
    }
    func scrolled(_ scrollView: UIScrollView) {
        guard hasActiveSelection else { return }
        for held in texts where held.selected.length > 0 && selectionAffected(by: scrollView, held: held) {
            if let doc = held.document {
                doc.syncFrame()
                doc.syncSurfaceMask(for: held.selected)
            }
            held.invalidateDisplay()
        }
    }
    func scrollWillBeginDragging(_ scrollView: UIScrollView) {
        guard hasActiveSelection else { return }
        for held in texts where held.selected.length > 0 && selectionAffected(by: scrollView, held: held) {
            held.dismissMenu()
        }
    }
    func clear() {
        suppressedTapTouch = nil
        texts.forEach { $0.clear() }
        syncDismissObservers()
    }
    /// One selection on the page: `keep`'s.
    func selected(in keep: SelectableText) {
        suppressedTapTouch = nil
        if keep.document == nil, let node = keep.node, node.textSelectable,
           let owner = node.selectAllElement, owner !== node,
           let found = documentParagraphs(for: node), found.owner === owner {
            let shared: SelectableText
            if let existing = texts.first(where: { $0.document?.owner === owner }) {
                shared = existing
            } else {
                while texts.count >= Self.kept, let old = texts.first(where: { !$0.busy && $0 !== keep }) { drop(old) }
                let doc = SelectionDocument(owner: owner, paragraphs: found.paragraphs, host: self)
                let anchor = found.paragraphs.first { SelectionDocument.sharesTouchSurface($0, with: owner) } ?? found.paragraphs[0]
                let made = SelectableText(node: anchor, host: self, document: doc)
                texts.append(made)
                shared = made
            }
            _ = shared.selectAllDocument()
            return
        }
        for held in texts where held !== keep { held.clear() }
        if let doc = keep.document, let owner = doc.owner {
            for old in texts where old !== keep && !old.busy && old.document == nil {
                if let n = old.node, doc.contains(n), SelectionDocument.sharesTouchSurface(n, with: owner) { drop(old) }
            }
        }
        syncDismissObservers()
    }
    func selectionCleared() { syncDismissObservers() }
    private func syncDismissObservers() {
        guard hasActiveSelection, let presenter else {
            for observer in dismissObservers { observer.view?.removeGestureRecognizer(observer) }
            dismissObservers.removeAll()
            return
        }
        var roots: [UIView] = [presenter.documentHost]
        for surface in presenter.presentations() where !roots.contains(where: { $0 === surface }) {
            roots.append(surface)
        }
        dismissObservers.removeAll { observer in
            guard let view = observer.view, roots.contains(where: { $0 === view }) else {
                observer.view?.removeGestureRecognizer(observer)
                return true
            }
            return false
        }
        for root in roots where !dismissObservers.contains(where: { $0.view === root }) {
            let observer = SelectionDismissObserver(host: self, presenter: presenter)
            root.addGestureRecognizer(observer)
            dismissObservers.append(observer)
        }
    }
    func report(_ node: NodeView, _ text: String, _ range: NSRange) {
        presenter?.selectionChange(node.id, text, range.location, NSMaxRange(range))
    }

    /// The word at `point` of `node` becomes the selection, as a double tap
    /// or a long press there makes it: the agent's `tap <id> dblclick` and
    /// `contextmenu`, which synthesize no touch. Nil when `node` is not
    /// selectable text or has no word there.
    @discardableResult
    func selectWord(in node: NodeView, at point: CGPoint) -> (text: String, range: NSRange)? {
        guard admits(node) else { return nil }
        let held = install(node)
        let point = held.node.map { node.convert(point, to: $0) } ?? point
        guard held.selectWord(at: point) else { return nil }
        let active = text(for: node) ?? held
        if let document = active.document, let part = document.parts.first(where: { $0.node === node }) {
            let selected = NSIntersectionRange(active.selected, part.range)
            guard selected.length > 0 else { return nil }
            let local = NSRange(location: selected.location - part.start, length: selected.length)
            return SelectableText.sourceSelection(in: node, shaped: local)
        }
        return active.reportable
    }
    @discardableResult
    func copy(_ node: NodeView) -> Bool {
        guard let chosen = text(for: node)?.selectedText, !chosen.isEmpty else { return false }
        pasteboard.string = chosen
        return true
    }
    #if EXACT_AGENT
    func agentSelect(_ hit: UIView, at point: CGPoint, event: String) -> [String: Any]? {
        guard let node = hit as? NodeView, admits(node) else { return nil }
        if event == "contextmenu", node.contextMenuSource(at: node.convert(point, from: nil)) != nil { return nil }
        guard let found = selectWord(in: node, at: node.convert(point, from: nil)) else { return nil }
        return ["text": found.text, "start": found.range.location, "end": NSMaxRange(found.range)]
    }
    #endif
}

final class SelectablePosition: UITextPosition {
    let offset: Int
    init(_ offset: Int) { self.offset = offset }
}
final class SelectableRange: UITextRange {
    let range: NSRange
    init(_ range: NSRange) { self.range = range }
    override var start: UITextPosition { SelectablePosition(range.location) }
    override var end: UITextPosition { SelectablePosition(NSMaxRange(range)) }
    override var isEmpty: Bool { range.length == 0 }
}
final class SelectableRect: UITextSelectionRect {
    private let box: CGRect, first: Bool, last: Bool, rtl: Bool
    /// The paragraph and the line of it the rect is on. Lines in different
    /// columns can share a band.
    let paragraph: ObjectIdentifier?, line: Int
    init(_ box: CGRect, first: Bool, last: Bool, rtl: Bool, paragraph: ObjectIdentifier?, line: Int) {
        self.box = box; self.first = first; self.last = last; self.rtl = rtl; self.paragraph = paragraph; self.line = line
    }
    /// The rect at `box`, another view's coordinates for it. It keeps an
    /// endpoint only where `first` or `last` allows.
    func converted(_ box: CGRect, first: Bool = true, last: Bool = true) -> SelectableRect {
        SelectableRect(box, first: first && self.first, last: last && self.last, rtl: rtl, paragraph: paragraph, line: line)
    }
    override var rect: CGRect { box }
    override var writingDirection: NSWritingDirection { rtl ? .rightToLeft : .leftToRight }
    override var containsStart: Bool { first }
    override var containsEnd: Bool { last }
    override var isVertical: Bool { false }
}

/// One selectable paragraph's text input: what UIKit's interaction asks for
/// the text, the selection and where they are. Offsets are UTF-16 offsets of
/// the shaped text, the layout's own (CoreText's and NSString's).
final class SelectableText: UIResponder, UITextInput, UITextInteractionDelegate {
    private(set) weak var node: NodeView?
    private unowned let host: TextSelectionHost
    lazy var interaction = UITextInteraction(for: .nonEditable)
    /// The recognizers and the interactions UIKit added with the
    /// interaction, which it does not all take back with it.
    private var recognizers: [UIGestureRecognizer] = []
    private var companions: [UIInteraction] = []
    private(set) var selected = NSRange(location: 0, length: 0)
    /// The text the selection is in; a different one drops it (`settle`).
    private var text: NSString
    private var laid = CGSize.zero
    private var reported = NSRange(location: 0, length: 0)

    static func shaped(_ node: NodeView) -> NSString { node.paragraphSpec().runs.map(\.text).joined() as NSString }

    let document: SelectionDocument?
    var shapedText: NSString { text }
    init(node: NodeView, host: TextSelectionHost, document: SelectionDocument? = nil, attach: Bool = true) {
        self.node = node; self.host = host
        self.document = document
        text = document?.text ?? Self.shaped(node); laid = node.bounds.size
        super.init()
        guard attach else { return }
        document?.surface.input = self
        let node: UIView = document?.surface ?? node
        interaction.textInput = self
        interaction.delegate = self
        let before = node.gestureRecognizers ?? [], held = node.interactions
        node.addInteraction(interaction)
        recognizers = (node.gestureRecognizers ?? []).filter { added in !before.contains { $0 === added } }
        companions = node.interactions.filter { added in added !== interaction && !held.contains { $0 === added } }
        for recognizer in recognizers {
            // None of them holds back the node's own touch: its press is a
            // touch down and up, and fires at the lift, once.
            recognizer.delaysTouchesEnded = false
        }
        syncTaps()
    }

    /// Selecting, or a finger of its own down: not to be taken away.
    var busy: Bool {
        selected.length > 0 || recognizers.contains { $0.numberOfTouches > 0 || $0.state == .began || $0.state == .changed }
    }
    func detach() {
        clear()
        if isFirstResponder { _ = resignFirstResponder() }
        guard let held = node else { return }
        let node: UIView = document?.surface ?? held
        node.removeInteraction(interaction)
        // Measured: two of UIKit's eighteen interactions stay behind.
        for left in node.interactions where companions.contains(where: { $0 === left }) { node.removeInteraction(left) }
        for left in node.gestureRecognizers ?? [] where recognizers.contains(where: { $0 === left }) { node.removeGestureRecognizer(left) }
        recognizers = []; companions = []
        document?.surface.setPaintForeground(false)
        document?.surface.removeFromSuperview()
    }
    /// After a batch. False when the node no longer takes a selection.
    func settle() -> Bool {
        guard let node, host.admits(node) else { return false }
        if let document, !document.settle(host: host) { clear(); return false }
        let now = document?.text ?? Self.shaped(node)
        if now != text {
            clear()
            text = now
        } else if selected.length > 0, (document?.geometryChanged() == true || node.bounds.size != laid) {
            document?.syncSurfaceMask(for: selected)
            display?.setNeedsSelectionUpdate()
        }
        laid = node.bounds.size
        return true
    }
    func invalidateDisplay() {
        display?.setNeedsSelectionUpdate()
    }

    /// UIKit's single taps swallow the touch they recognize. With nothing
    /// selected they have nothing to do, so the node's own press hears the
    /// tap; with a selection up a tap in the text clears it and is not a
    /// press (measured: a text view does the same).
    private func syncTaps() {
        let on = selected.length > 0
        for recognizer in recognizers {
            if let tap = recognizer as? UITapGestureRecognizer, tap.numberOfTapsRequired == 1, tap.isEnabled != on { tap.isEnabled = on }
        }
    }
    private var display: UITextSelectionDisplayInteraction? {
        (document?.surface ?? node)?.interactions.lazy.compactMap { $0 as? UITextSelectionDisplayInteraction }.first
    }

    // MARK: the selection

    /// The selectable shaped slices within `range` as a sorted, disjoint union:
    /// the whole slice when the paragraph itself is selectable, otherwise only
    /// its `user-select: all` runs.
    func selectableSlices(in range: NSRange) -> [NSRange] {
        guard let node, range.length > 0 else { return [] }
        if node.textSelectable { return [range] }
        let clipped = runs(of: node).compactMap { run -> NSRange? in
            let hit = NSIntersectionRange(run, range)
            return hit.length > 0 ? hit : nil
        }.sorted { $0.location != $1.location ? $0.location < $1.location : $0.length > $1.length }
        var merged: [NSRange] = []
        for slice in clipped {
            if let last = merged.last, slice.location <= NSMaxRange(last) {
                let end = max(NSMaxRange(last), NSMaxRange(slice))
                merged[merged.count - 1] = NSRange(location: last.location, length: end - last.location)
            } else {
                merged.append(slice)
            }
        }
        return merged
    }

    /// The selected text as it is copied.
    var selectedText: String {
        guard selected.length > 0 else { return "" }
        guard let document else {
            guard let node else { return text.substring(with: selected) }
            let slices = selectableSlices(in: selected)
            guard !slices.isEmpty else { return "" }
            return slices.map { node.copiedText(text.substring(with: $0), from: $0.location) }.joined()
        }
        // Each paragraph's selectable slices as they copy (an image as its label, LLP 1011.002 D4).
        var pieces: [String] = []
        for part in document.parts {
            let overlap = NSIntersectionRange(selected, part.range)
            guard overlap.length > 0 else { continue }
            let local = NSRange(location: overlap.location - part.start, length: overlap.length)
            let slices = part.input.selectableSlices(in: local)
            guard !slices.isEmpty else { continue }
            let copied = slices.map { part.node.copiedText(part.input.shapedText.substring(with: $0), from: $0.location) }.joined()
            pieces.append(copied)
        }
        return pieces.joined(separator: "\n\n")
    }
    /// Map shaped offsets inside `node` to a clamped slice of `node.paragraphText`.
    static func sourceSelection(in node: NodeView, shaped: NSRange) -> (text: String, range: NSRange)? {
        guard shaped.length > 0 else { return nil }
        let source = node.paragraphText as NSString
        let lo = min(max(0, node.sourceOffset(shaped.location)), source.length)
        let hi = min(max(lo, node.sourceOffset(NSMaxRange(shaped))), source.length)
        guard hi > lo else { return nil }
        let range = NSRange(location: lo, length: hi - lo)
        return (source.substring(with: range), range)
    }
    /// The selection in the node's source text, as `selectionchange` has it.
    var reportable: (text: String, range: NSRange)? {
        guard let node, selected.length > 0 else { return nil }
        if document != nil { return (selectedText, selected) }
        return Self.sourceSelection(in: node, shaped: selected)
    }
    private func set(_ range: NSRange) {
        let lo = min(max(0, range.location), text.length), hi = min(max(lo, NSMaxRange(range)), text.length)
        let asked = NSRange(location: lo, length: hi - lo), next = atomic(asked)
        // UIKit's own gesture asked for less: it is shown what it got.
        if next != asked { display?.setNeedsSelectionUpdate() }
        guard next != selected else { return }
        selected = next
        document?.syncSurfaceMask(for: next)
        syncTaps()
        if next.length > 0 { host.selected(in: self) } else { host.selectionCleared() }
        report()
    }
    /// The web's `selectionchange` on a `text`: its part of the selection
    /// when that changes; nothing selected there is "" at 0, 0.
    private func report() {
        if let document { document.report(selected, host: host); return }
        guard let node, node.handlers.contains("selectionchange") else { return }
        let found = reportable
        let range = found?.range ?? NSRange(location: 0, length: 0)
        guard range != reported else { return }
        reported = range
        host.report(node, found?.text ?? "", range)
    }
    func dismissMenu() {
        textInputView.interactions.compactMap { $0 as? UIEditMenuInteraction }.forEach { $0.dismissMenu() }
    }
    /// Nothing is selected any more, said to UIKit so its handles go.
    func clear() {
        guard selected.length > 0 else { return }
        inputDelegate?.selectionWillChange(self)
        set(NSRange(location: 0, length: 0))
        inputDelegate?.selectionDidChange(self)
        dismissMenu()
    }
    /// The whole selectable range of this paragraph or shared document.
    var allSelectableRange: NSRange {
        guard text.length > 0 else { return NSRange(location: 0, length: 0) }
        return atomic(NSRange(location: 0, length: text.length))
    }
    var canSelectAll: Bool {
        let all = allSelectableRange
        return selected.length > 0 && all.length > 0 && selected != all
    }
    @discardableResult
    func selectAllDocument() -> Bool {
        let all = allSelectableRange
        guard all.length > 0 else { return false }
        if !isFirstResponder { _ = becomeFirstResponder() }
        inputDelegate?.selectionWillChange(self)
        set(all)
        inputDelegate?.selectionDidChange(self)
        display?.isActivated = true
        display?.setNeedsSelectionUpdate()
        return true
    }
    /// The word at `point` (the node's space), by UIKit's own tokenizer, as
    /// its double tap takes it; shown as a gesture's selection is.
    func selectWord(at point: CGPoint) -> Bool {
        let point = document.map { node?.convert(point, to: $0.surface) ?? point } ?? point
        let at = SelectablePosition(index(at: point))
        guard let word = (tokenizer.rangeEnclosingPosition(at, with: .word, inDirection: .storage(.forward))
            ?? tokenizer.rangeEnclosingPosition(at, with: .word, inDirection: .storage(.backward))) as? SelectableRange, word.range.length > 0 else { return false }
        if !isFirstResponder { _ = becomeFirstResponder() }
        inputDelegate?.selectionWillChange(self)
        set(word.range)
        inputDelegate?.selectionDidChange(self)
        display?.isActivated = true
        display?.setNeedsSelectionUpdate()
        return true
    }
    /// What a range that takes part of a `user-select: all` element becomes:
    /// one that takes all of it. An empty range stays empty. In a paragraph
    /// whose own text is not selectable, only those runs are: a range that
    /// takes none of them selects nothing.
    func atomic(_ range: NSRange) -> NSRange {
        if let document { return document.atomic(range) }
        guard let node, range.length > 0 else { return range }
        if node.textSelectable, node.selectAllElement != nil { return NSRange(location: 0, length: text.length) }
        let own = node.textSelectable
        var lo = own ? range.location : Int.max, hi = own ? NSMaxRange(range) : Int.min
        for whole in runs(of: node) where whole.location < NSMaxRange(range) && NSMaxRange(whole) > range.location {
            lo = min(lo, whole.location); hi = max(hi, NSMaxRange(whole))
        }
        return hi > lo ? NSRange(location: lo, length: hi - lo) : NSRange(location: 0, length: 0)
    }
    /// The `user-select: all` inline runs, in this paragraph's offsets.
    private func runs(of node: NodeView) -> [NSRange] {
        node.inlineText.filter(\.selectsAll).map {
            let lo = node.shapedOffset($0.range.location)
            return NSRange(location: lo, length: node.shapedOffset(NSMaxRange($0.range)) - lo)
        }
    }

    // MARK: the responder

    override var next: UIResponder? { document?.surface ?? node }
    override var canBecomeFirstResponder: Bool { node?.window != nil }
    override func resignFirstResponder() -> Bool {
        let resigned = super.resignFirstResponder()
        if resigned { clear() }
        return resigned
    }
    /// Read only: no keyboard, and a long press selects a word.
    var isEditable: Bool { false }
    /// Before iOS 18 nothing asks `isEditable`; an empty input view keeps
    /// the keyboard away there.
    override var inputView: UIView? {
        if #available(iOS 18.0, *) { return super.inputView }
        return UIView()
    }
    func interactionShouldBegin(_ interaction: UITextInteraction, at point: CGPoint) -> Bool {
        // Not under a touch whose `pointerdown` an action prevented: it starts no selection (LLP 1005 §3).
        guard let node, host.admits(node), !node.keepsFocus(from: node), !(document.map { node.keepsFocus(from: $0.surface) } ?? false) else { return false }
        return true
    }

    // MARK: geometry, from the paragraph's lines

    private var layout: (paragraph: Paragraph, spec: Spec, box: CGRect)? {
        guard let node, let paragraph = node.paragraphLayout(), !paragraph.lines.isEmpty else { return nil }
        return (paragraph, node.paragraphSpec(), node.paragraphBox())
    }
    /// A rect of line `line` of the unfragmented paragraph, where its
    /// fragment shows it (LLP 1093 D8).
    private func shown(_ rect: CGRect, line: Int) -> CGRect {
        guard let fragment = node?.columnRecord?.fragments.first(where: { $0.lines.contains(line) }) else { return rect }
        return rect.offsetBy(dx: fragment.dx, dy: fragment.dy)
    }
    /// The line an offset is on: the last that starts at or before it.
    private func line(of offset: Int, in paragraph: Paragraph) -> Int {
        var found = 0
        for i in paragraph.lines.indices where CTLineGetStringRange(paragraph.lines[i]).location <= offset { found = i }
        return found
    }
    /// The caret nearest `point`, in the node's space.
    func index(at point: CGPoint) -> Int {
        if let document { return document.index(at: point) }
        guard let node, let (paragraph, spec, box) = layout else { return 0 }
        let mapped = node.caretPoint(point)
        if mapped.y < box.minY { return 0 }
        guard let line = paragraph.lineIndex(at: CGPoint(x: mapped.x - box.minX, y: mapped.y - box.minY), align: spec.align, width: box.width) else { return 0 }
        let found = paragraph.stringIndex(in: line, at: mapped.x - box.minX - paragraph.origin(line, align: spec.align, width: box.width))
        return found == kCFNotFound ? text.length : min(max(0, found), text.length)
    }
    private func rects(_ range: NSRange) -> [SelectableRect] {
        if let document { return document.rects(range).compactMap { $0 as? SelectableRect } }
        guard let (paragraph, spec, box) = layout else { return [] }
        let everywhere = CGRect(x: -1e7, y: -1e7, width: 2e7, height: 2e7)
        let id = node.map { ObjectIdentifier($0) }
        let slices = selectableSlices(in: range)
        var rects: [SelectableRect] = []
        for slice in slices {
            let segments = paragraph.selectionSegments(slice, align: spec.align, in: box, dirty: everywhere)
            for found in segments {
                let baseline = paragraph.baselines[found.line]
                let shift = TextEngine.paintedBaseline(baseline, scale: node?.traitCollection.displayScale ?? 1) - baseline.rounded()
                let first = found.span.containsStart && slice.location == range.location
                let last = found.span.containsEnd && NSMaxRange(slice) == NSMaxRange(range)
                rects.append(SelectableRect(shown(found.rect.offsetBy(dx: 0, dy: shift), line: found.line), first: first,
                                            last: last, rtl: found.span.rtl, paragraph: id, line: found.line))
            }
        }
        // An endpoint on no shown line (one `line-clamp` hides) still needs its handle.
        guard range.length > 0 else { return rects }
        if !rects.contains(where: \.containsStart) { rects.insert(endpoint(at: range.location, first: true), at: 0) }
        if !rects.contains(where: \.containsEnd) { rects.append(endpoint(at: NSMaxRange(range), first: false)) }
        return rects
    }
    /// A selection endpoint that no glyph's rect carries: a rect of no
    /// width at its caret.
    func endpoint(at offset: Int, first: Bool) -> SelectableRect {
        let caret = caretRect(for: SelectablePosition(offset))
        return SelectableRect(CGRect(x: caret.midX, y: caret.minY, width: 0, height: caret.height), first: first, last: !first,
                              rtl: node?.paragraphSpec().direction == 1, paragraph: node.map { ObjectIdentifier($0) },
                              line: layout.map { line(of: offset, in: $0.paragraph) } ?? 0)
    }

    // MARK: UITextInput

    var hasText: Bool { text.length > 0 }
    func insertText(_ text: String) {}
    func deleteBackward() {}
    func text(in range: UITextRange) -> String? {
        guard let range = (range as? SelectableRange)?.range, NSMaxRange(range) <= text.length else { return nil }
        return text.substring(with: range)
    }
    func replace(_ range: UITextRange, withText text: String) {}
    var selectedTextRange: UITextRange? {
        get { selected.length > 0 ? SelectableRange(selected) : nil }
        set { set((newValue as? SelectableRange)?.range ?? NSRange(location: 0, length: 0)) }
    }
    var markedTextRange: UITextRange? { nil }
    var markedTextStyle: [NSAttributedString.Key: Any]? { get { nil } set {} }
    func setMarkedText(_ markedText: String?, selectedRange: NSRange) {}
    func unmarkText() {}
    var beginningOfDocument: UITextPosition { SelectablePosition(0) }
    var endOfDocument: UITextPosition { SelectablePosition(text.length) }
    func textRange(from fromPosition: UITextPosition, to toPosition: UITextPosition) -> UITextRange? {
        guard let a = (fromPosition as? SelectablePosition)?.offset, let b = (toPosition as? SelectablePosition)?.offset else { return nil }
        return SelectableRange(NSRange(location: min(a, b), length: abs(b - a)))
    }
    func position(from position: UITextPosition, offset: Int) -> UITextPosition? {
        guard let from = (position as? SelectablePosition)?.offset else { return nil }
        let to = from + offset
        return to < 0 || to > text.length ? nil : SelectablePosition(to)
    }
    func position(from position: UITextPosition, in direction: UITextLayoutDirection, offset: Int) -> UITextPosition? {
        self.position(from: position, offset: direction == .left || direction == .up ? -offset : offset)
    }
    func compare(_ position: UITextPosition, to other: UITextPosition) -> ComparisonResult {
        let a = (position as? SelectablePosition)?.offset ?? 0, b = (other as? SelectablePosition)?.offset ?? 0
        return a < b ? .orderedAscending : a > b ? .orderedDescending : .orderedSame
    }
    func offset(from: UITextPosition, to toPosition: UITextPosition) -> Int {
        ((toPosition as? SelectablePosition)?.offset ?? 0) - ((from as? SelectablePosition)?.offset ?? 0)
    }
    weak var inputDelegate: UITextInputDelegate?
    lazy var tokenizer: UITextInputTokenizer = UITextInputStringTokenizer(textInput: self)
    func position(within range: UITextRange, farthestIn direction: UITextLayoutDirection) -> UITextPosition? {
        direction == .left || direction == .up ? range.start : range.end
    }
    func characterRange(byExtending position: UITextPosition, in direction: UITextLayoutDirection) -> UITextRange? {
        guard let at = (position as? SelectablePosition)?.offset else { return nil }
        let back = direction == .left || direction == .up
        return SelectableRange(back ? NSRange(location: max(0, at - 1), length: min(1, at)) : NSRange(location: at, length: min(1, text.length - at)))
    }
    func baseWritingDirection(for position: UITextPosition, in direction: UITextStorageDirection) -> NSWritingDirection {
        let node = (position as? SelectablePosition).flatMap { document?.part(at: $0.offset)?.node } ?? self.node
        return node?.style["direction"]?.string == "rtl" ? .rightToLeft : .leftToRight
    }
    func setBaseWritingDirection(_ writingDirection: NSWritingDirection, for range: UITextRange) {}
    /// The range on the line where it starts. A line of both directions holds
    /// several of its rects, ordered left to right; this encloses them all.
    func firstRect(for range: UITextRange) -> CGRect {
        let rects = (range as? SelectableRange).map { self.rects($0.range) } ?? []
        guard let first = rects.first(where: \.containsStart) ?? rects.first else { return caretRect(for: range.start) }
        return rects.filter { $0.paragraph == first.paragraph && $0.line == first.line }.reduce(first.rect) { $0.union($1.rect) }
    }
    func caretRect(for position: UITextPosition) -> CGRect {
        if let offset = (position as? SelectablePosition)?.offset, let document { return document.caret(at: offset) }
        guard let offset = (position as? SelectablePosition)?.offset, let (paragraph, spec, box) = layout else { return .zero }
        let i = line(of: offset, in: paragraph), line = paragraph.lines[i]
        var above: CGFloat = 0, below: CGFloat = 0
        _ = CTLineGetTypographicBounds(line, &above, &below, nil)
        let x = box.minX + paragraph.origin(i, align: spec.align, width: box.width) + CTLineGetOffsetForStringIndex(line, offset, nil)
        return shown(CGRect(x: x - 1, y: box.minY + TextEngine.paintedBaseline(paragraph.baselines[i], scale: node?.traitCollection.displayScale ?? 1) - above, width: 2, height: above + below), line: i)
    }
    func selectionRects(for range: UITextRange) -> [UITextSelectionRect] {
        (range as? SelectableRange).map { rects($0.range) } ?? []
    }
    func closestPosition(to point: CGPoint) -> UITextPosition? { SelectablePosition(index(at: point)) }
    func closestPosition(to point: CGPoint, within range: UITextRange) -> UITextPosition? {
        guard let within = (range as? SelectableRange)?.range else { return nil }
        return SelectablePosition(min(max(index(at: point), within.location), NSMaxRange(within)))
    }
    func characterRange(at point: CGPoint) -> UITextRange? {
        let at = index(at: point)
        return at < text.length ? SelectableRange(text.rangeOfComposedCharacterSequence(at: at)) : nil
    }
    var textInputView: UIView { document?.surface ?? node ?? UIView() }
    func copyDocument() { host.pasteboard.string = selectedText }
}

/// Passive observer attached to session roots while a selection is active so a
/// tap on any view or native control outside the selection clears it without
/// interfering with presses, controls, scrolling, or UIKit's edit menu.
final class SelectionDismissObserver: UIGestureRecognizer, UIGestureRecognizerDelegate {
    private unowned let host: TextSelectionHost
    private weak var presenter: Presenter?
    private var tracked: UITouch?
    private var start = CGPoint.zero
    private var moved = false

    init(host: TextSelectionHost, presenter: Presenter) {
        self.host = host
        self.presenter = presenter
        super.init(target: nil, action: nil)
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
        delegate = self
    }
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesBegan(touches, with: event)
        guard tracked == nil, touches.count == 1, host.hasActiveSelection, let presenter,
              let touch = touches.first, let hit = touch.view, !host.owns(hit),
              hit === view || NodeView.innermost(hit) != nil || hit is ScrollView,
              !presenter.contextRetainsFocus(hit) else {
            tracked = nil; moved = false; state = .failed
            return
        }
        tracked = touch
        start = touch.location(in: nil)
        moved = false
    }
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesMoved(touches, with: event)
        guard let touch = tracked, touches.contains(touch) else { return }
        let at = touch.location(in: nil)
        if hypot(at.x - start.x, at.y - start.y) > 10 {
            moved = true; tracked = nil; state = .failed
        }
    }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesEnded(touches, with: event)
        defer { tracked = nil; moved = false; state = .failed }
        guard let touch = tracked, touches.contains(touch), !moved, host.hasActiveSelection, let presenter,
              !presenter.cancelledPressTouches.contains(touch),
              !(touch.view.flatMap(NodeView.innermost)?.isCancelledPressTouch(touch) == true) else { return }
        let hitSelected = touch.view.map { host.hasSelection(in: $0) } ?? false
        host.clear()
        if hitSelected { host.recordSuppressedTap(touch) }
    }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesCancelled(touches, with: event)
        tracked = nil; moved = false; state = .failed
    }
    override func reset() {
        super.reset()
        tracked = nil; moved = false
    }
}

#endif
