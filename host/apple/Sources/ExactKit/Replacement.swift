#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif
import Foundation

/// What `ExactApp.apply` answers (LLP 1029.000.002 D9): committed now; held,
/// with what it waits for, by the app until that clears; or refused, with why.
public enum ExactApplyResult: Equatable {
    case applied
    case held(String)
    case refused(String)
}

/// How a held candidate ended other than by being applied, which
/// `ExactAppLifecycle.generationStarted` says.
public enum ExactReplacementOutcome: Equatable {
    /// It failed when it was prepared, and is not tried again.
    case refused
    /// A newer candidate took its place.
    case superseded
    /// Every session it was held for was destroyed, or the producer that
    /// brought it went: the connection `disconnect` or `connect` closed.
    case dropped
}

/// The producer's final result, distinct from `apply`'s immediate answer.
package enum ReplacementCompletion: Equatable {
    case applied
    case ended(ExactReplacementOutcome, String)
}

extension ExactApp {
    /// `development`: requests in flight hold it for a second at most, as
    /// the web's dev loop does (LLP 1007 §6). `now`: the person asked for it,
    /// so no interaction holds it. `producer`: what brought it, which
    /// `ReplacementQueue.withdraw` names when it goes. `finished` hears a
    /// held one's end.
    package func applyTogether(_ bytes: Data, label: String, resolver candidateResolver: AssetResolver, token: UInt64, identity: String? = nil, module: ExactModule? = nil, development: Bool = false, now: Bool = false, producer: AnyObject? = nil, commit: @escaping () -> Bool, finished: @escaping (ReplacementCompletion) -> Void = { _ in }) -> ExactApplyResult {
        guard !transaction else { return .refused("a replacement is already committing") }
        var requestsSince: TimeInterval?
        var displaced = Set<ObjectIdentifier>()
        return replacements.submit(token: token, now: now, producer: producer, checkMarked: { !displaced.contains(ObjectIdentifier($0)) }, attempt: { [unowned self] in
            if now { endCompositions() }
            let letGo = development && requestsSince.map { ProcessInfo.processInfo.systemUptime - $0 >= 1 } == true
            let result = commitTogether(bytes, label: label, resolver: candidateResolver, token: token, identity: identity, module: module, letGo: letGo, heldFields: { displaced = $0 }, commit: commit)
            if development, result == .held("requests"), requestsSince == nil {
                requestsSince = ProcessInfo.processInfo.systemUptime
                DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in self?.replacements.changed() }
            }
            return result
        }, finished: finished)
    }

    /// The person's own reload, the dev menu's (D9): each composition ends
    /// as it does when the person clicks elsewhere, its marked text kept.
    func endCompositions() {
        for session in sessions where session.state != .destroyed {
            for node in session.presenter.views.values where node.fieldComposing { node.endComposition() }
        }
    }
}

extension NodeView {
    /// What an input method has marked becomes text, as a press elsewhere
    /// makes it (`commitComposition` on the Mac: the input method lets go
    /// first).
    func endComposition() {
        #if os(macOS)
        guard let editor = textArea ?? (field?.currentEditor() as? NSTextView) else { return }
        commitComposition(in: editor)
        #else
        if let textArea {
            textArea.unmarkText()
        } else if let field = selectionField {
            field.unmarkText()
            fieldChanged() // UIKit sends no editing change for a commit that changes no text
        }
        #endif
    }
}

extension ExactSession {
    /// Only a retained native editor can finish the input method's marked text.
    func displacedCompositions(_ batch: Batch) -> Set<ObjectIdentifier> {
        #if os(macOS)
        let windows = [presenter.viewport.window].compactMap { $0 } + (NSApp?.windows ?? [])
        var displaced = Set<ObjectIdentifier>()
        for window in windows {
            guard let editor = window.firstResponder as? NSTextView, editor.hasMarkedText(),
                  let node = presenter.owner(ofMarked: editor) else { continue }
            if !node.keepsComposition(in: batch) { displaced.insert(ObjectIdentifier(node)) }
        }
        return displaced
        #else
        return []
        #endif
    }
}

#if os(macOS)
extension Presenter {
    func owner(ofMarked editor: NSTextView) -> NodeView? {
        if let node = keyTarget(editor), node.textArea === editor || node.field?.currentEditor() === editor { return node }
        // AppKit can host the shared field editor outside its field's view hierarchy.
        return views.values.first { $0.textArea === editor || $0.field?.currentEditor() === editor }
    }
}

extension NodeView {
    fileprivate func keepsComposition(in batch: Batch) -> Bool {
        guard batch.replacesTree, let presenter else { return false }
        for op in batch.ops {
            switch op.op {
            case .create, .destroy, .renew, .exit: break
            case .props:
                if op.id == id && (op.props["disabled"] == "true" || op.props["editable"] == "false") { return false }
                guard op.props["inert"] == "true" else { continue }
            case .style:
                guard op.style["display"]?.string == "none" || (op.style["visibility"]?.string ?? "visible") != "visible" else { continue }
            default: continue
            }
            if let ancestor = presenter.views[op.id], presenter.logicallyWithin(self, ancestor) { return false }
        }
        return true
    }
}
#endif

/// The newest complete candidate that has to wait (D9): its bytes and its
/// durable commit, never a prepared runner, so the state it carries is the
/// state at its commit. It is tried at the first main run-loop turn with no
/// interaction under way after something changed: an input, a reply, a
/// presented batch.
package final class ReplacementQueue {
    struct Candidate {
        // Producer tokens may repeat; every submission has its own identity.
        let id = UUID()
        let token: UInt64
        /// The person asked for it: only a batch being applied holds it.
        let now: Bool
        let producer: ObjectIdentifier?
        let sessions: [WeakSession]
        let checkMarked: ((NodeView) -> Bool)?
        let attempt: () -> ExactApplyResult
        let finished: (ReplacementCompletion) -> Void
    }
    unowned let app: ExactApp
    private var candidate: Candidate?
    private var latestSubmission: UUID?
    /// What the held candidate waits for, and since when.
    private(set) var reason: String?
    private var heldSince: TimeInterval = 0
    /// The hold is an interaction's, which ends without telling anyone; a
    /// module's or requests' is told to `changed`.
    private var waitsOnInteraction = false
    private var observer: CFRunLoopObserver?
    private var scheduled = false
    private var changedSinceAttempt = false

    init(_ app: ExactApp) {
        self.app = app
        #if os(macOS)
        watchInput()
        #endif
    }

    /// Apply now, or keep `attempt` until no interaction holds it and try it
    /// then. A held candidate it replaces is superseded.
    func submit(token: UInt64, now: Bool = false, producer: AnyObject? = nil, checkMarked: ((NodeView) -> Bool)? = nil, attempt: @escaping () -> ExactApplyResult,
                finished: @escaping (ReplacementCompletion) -> Void) -> ExactApplyResult {
        let next = Candidate(token: token, now: now, producer: producer.map(ObjectIdentifier.init), sessions: app.sessions.map(WeakSession.init), checkMarked: checkMarked, attempt: attempt, finished: finished)
        let old = take()
        latestSubmission = next.id
        candidate = next
        if let old { end(old, .superseded, "a newer candidate arrived") }
        guard candidate?.id == next.id else { return .refused("a newer candidate arrived") }
        return tryCandidate(next)
    }

    /// Detach before calling the producer: its callbacks may submit another
    /// candidate. Only this submission can put its held attempt back.
    private func tryCandidate(_ next: Candidate, resuming: Bool = false) -> ExactApplyResult {
        _ = take()
        let interaction = hold(for: next)
        let result = interaction.map(ExactApplyResult.held) ?? next.attempt()
        if case .held(let why) = result {
            guard latestSubmission == next.id else {
                end(next, .superseded, "a newer candidate arrived")
                return result
            }
            candidate = next
            reason = why
            waitsOnInteraction = interaction != nil || why == "composition"
            if !resuming {
                heldSince = ProcessInfo.processInfo.systemUptime
                changedSinceAttempt = true
            }
            observe()
        }
        return result
    }

    /// What a held candidate waits for, once it has waited `seconds`.
    func reason(after seconds: TimeInterval) -> String? {
        ProcessInfo.processInfo.systemUptime - heldSince >= seconds ? reason : nil
    }

    /// A session went: the held candidate is dropped once none it was held for is left.
    func destroyed(_ session: ExactSession) {
        guard let held = candidate,
              !held.sessions.contains(where: { $0.session.map { $0 !== session && $0.state != .destroyed } ?? false })
        else { return }
        _ = take()
        end(held, .dropped, "every session it was held for was destroyed")
    }

    /// `producer` went: the held candidate it brought is dropped, and one
    /// another producer brought stays.
    func withdraw(_ producer: AnyObject, _ why: String) {
        guard let held = candidate, held.producer == ObjectIdentifier(producer) else { return }
        _ = take()
        end(held, .dropped, why)
    }

    /// Something that may clear a module or requests hold happened.
    func changed() { changedSinceAttempt = true }

    private func take() -> Candidate? {
        let old = candidate
        candidate = nil
        reason = nil
        if let observer { CFRunLoopRemoveObserver(CFRunLoopGetMain(), observer, .commonModes) }
        observer = nil
        return old
    }

    private func end(_ held: Candidate, _ outcome: ExactReplacementOutcome, _ why: String) {
        app.lifecycle?.replacementEnded(app, token: held.token, outcome: outcome, reason: why)
        held.finished(.ended(outcome, why))
    }

    private func observe() {
        guard observer == nil else { return }
        observer = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeWaiting.rawValue, true, 0) { [weak self] _, _ in
            self?.turn()
        }
        CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
    }

    /// Before the main run loop sleeps: an interaction still under way says
    /// so; one that has ended, or a change, tries the candidate on the next
    /// turn, outside whatever the loop was doing.
    private func turn() {
        guard let held = candidate, !scheduled, waitsOnInteraction || changedSinceAttempt else { return }
        if let hold = hold(for: held) { reason = hold; waitsOnInteraction = true; changedSinceAttempt = true; return }
        scheduled = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            scheduled = false
            guard let held = candidate, hold(for: held) == nil else { return }
            changedSinceAttempt = false
            let result = tryCandidate(held, resuming: true)
            if case .held = result { return }
            if case .refused(let why) = result {
                app.lifecycle?.replacementEnded(app, token: held.token, outcome: .refused, reason: why)
                held.finished(.ended(.refused, why))
            } else {
                held.finished(.applied)
            }
        }
    }

    private func hold(for candidate: Candidate) -> String? {
        guard candidate.now else { return interactionHold(checkMarked: candidate.checkMarked) }
        return app.sessions.contains { $0.state != .destroyed && $0.presentingPriorTree } ? "presentation" : nil
    }

    /// What the person holds now (D7, D9; LLP 1030.003 D5), asked of the
    /// platform and not of the app's own gestures: text an input method has
    /// marked, a button or a touch down, a gesture in flight. The cheap
    /// answers come first, for a candidate that asks at every turn of the
    /// run loop. A batch being applied holds a replacement too, so it never
    /// runs inside one.
    var interactionHold: String? { interactionHold(checkMarked: nil) }

    func interactionHold(checkMarked: ((NodeView) -> Bool)?) -> String? {
        let sessions = app.sessions.filter { $0.state != .destroyed }
        if sessions.contains(where: \.presentingPriorTree) { return "presentation" }
        #if os(macOS)
        if composing(sessions, checkMarked: checkMarked) { return "composition" }
        #endif
        if pressed(sessions) { return "pointer" }
        // A grouped reorder the keyboard lifted holds no button.
        if sessions.contains(where: { $0.presenter.reorders.groupActive }) { return "gesture" }
        #if os(macOS)
        // A loop AppKit runs for a menu, a control's press or a scroller keeps its events from every view.
        if !trackpad.isEmpty || RunLoop.main.currentMode == .eventTracking { return "gesture" }
        #endif
        for root in windows(of: sessions) {
            if let hold = held(under: root) { return hold }
            #if !os(macOS)
            if transitioning((root as? UIWindow)?.rootViewController) { return "gesture" }
            #endif
        }
        return nil
    }

    /// Each window a session is in, once.
    private func windows(of sessions: [ExactSession]) -> [PlatformView] {
        var seen = Set<ObjectIdentifier>()
        #if os(macOS)
        // The frame view, so the title bar's and the toolbar's views are under it.
        let roots = sessions.compactMap { $0.presenter.viewport.window }.compactMap { $0.contentView?.superview ?? $0.contentView }
        #else
        let roots: [PlatformView] = sessions.compactMap { $0.presenter.viewport.window }
        #endif
        return roots.filter { seen.insert(ObjectIdentifier($0)).inserted }
    }

    /// What a view under `root` says the person holds: a gesture recognizer
    /// that has begun and not ended and, on UIKit, a first responder with
    /// marked text or a scroll view following a finger. UIKit cancels a
    /// view's touch when a scroll view's pan takes it, with the finger still down
    /// ([`cancelsTouchesInView`](https://developer.apple.com/documentation/uikit/uigesturerecognizer/cancelstouchesinview)).
    private func held(under root: PlatformView) -> String? {
        var views = [root]
        while let view = views.popLast() {
            #if os(macOS)
            let recognizers = view.gestureRecognizers
            #else
            if view.isFirstResponder, (view as? UITextInput)?.markedTextRange != nil { return "composition" }
            if let scroll = view as? UIScrollView, scroll.isTracking || scroll.isDragging { return "gesture" }
            let recognizers = view.gestureRecognizers ?? []
            #endif
            for recognizer in recognizers where recognizer.state == .began || recognizer.state == .changed {
                #if os(iOS)
                // A pointer resting over a view is no gesture: its recognizer stays begun while it rests.
                if recognizer is UIHoverGestureRecognizer { continue }
                #endif
                return "gesture"
            }
            views.append(contentsOf: view.subviews)
        }
        return nil
    }

    #if os(macOS)
    /// Marked text is its window's first responder's, in a window that is not key too.
    private func composing(_ sessions: [ExactSession], checkMarked: ((NodeView) -> Bool)?) -> Bool {
        let windows = sessions.compactMap { $0.presenter.viewport.window } + (NSApp?.windows ?? [])
        return windows.contains { window in
            guard let client = window.firstResponder as? NSTextInputClient, client.hasMarkedText() else { return false }
            guard let checkMarked, let editor = client as? NSTextView else { return true }
            for session in sessions {
                if let owner = session.presenter.owner(ofMarked: editor) { return !checkMarked(owner) }
            }
            return true
        }
    }

    /// The mouse buttons down, as the hardware has them
    /// (`NSEvent.pressedMouseButtons`): a press AppKit tracks in a control, a
    /// scroller, a menu or the title bar reaches no view of Exact's. A
    /// test's stands in for it.
    var hardwareButtons: () -> Int = { NSEvent.pressedMouseButtons }
    /// A button went down in this app's events and is still down. Only such
    /// a press holds: its release turns this app's run loop (the up arrives,
    /// or the loop that took it ends), and another app's would not.
    private var pressedHere = false

    private func pressed(_ sessions: [ExactSession]) -> Bool {
        #if EXACT_AGENT
        // The driver's held contact: its events are posted, so no button is down to `pressedMouseButtons`.
        if sessions.contains(where: { $0.agentBox?.contact != nil }) { return true }
        #endif
        releasedIfUp()
        return pressedHere
    }

    private func releasedIfUp() {
        if pressedHere, hardwareButtons() == 0 { pressedHere = false; changed() }
    }

    /// The trackpad gestures whose fingers are down, by their events' type.
    private var trackpad = Set<UInt>()
    /// Those down as the app resigned active: their lift goes to the app the
    /// person moved to. They hold until this app hears its next press or
    /// gesture, which begins after they lifted, as `pressedMouseButtons`
    /// holds a button until the hardware says it is up.
    private var trackpadBeforeResigning = Set<UInt>()
    private var watching: [Any] = []

    /// Hears each press, scroll, magnify and rotate before its window does
    /// (`NSEvent.addLocalMonitorForEvents`): a pan or a pinch on the trackpad
    /// presses no button. A release, and AppKit's word that the window
    /// server finished a drag, say a press may be over.
    private func watchInput() {
        watching = [
            NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown, .leftMouseUp, .rightMouseUp, .otherMouseUp, .appKitDefined,
                                                        .scrollWheel, .magnify, .rotate]) { [weak self] event in
                self?.hear(event)
                return event
            } as Any,
            // A scroll view takes the rest of a gesture, its lift included, and says only when its
            // scrolling ends, momentum and all: the hold lasts that long.
            NotificationCenter.default.addObserver(forName: NSScrollView.didEndLiveScrollNotification, object: nil, queue: nil) { [weak self] _ in
                self?.lifted(.scrollWheel)
            },
            NotificationCenter.default.addObserver(forName: NSApplication.didResignActiveNotification, object: nil, queue: nil) { [weak self] _ in
                guard let self else { return }
                trackpadBeforeResigning = trackpad
            },
        ]
    }

    /// An event the monitor brought. A press is this app's from its down. A
    /// gesture's fingers are down from the phase that begins it (resting,
    /// `mayBegin`, too) until `ended` or `cancelled`; momentum has no phase
    /// of this kind, the fingers having lifted.
    func hear(_ event: NSEvent) {
        if !trackpadBeforeResigning.isEmpty, [.leftMouseDown, .rightMouseDown, .otherMouseDown, .scrollWheel, .magnify, .rotate].contains(event.type) {
            let before = trackpad
            trackpad.subtract(trackpadBeforeResigning)
            trackpadBeforeResigning = []
            if trackpad != before { changed() }
        }
        switch event.type {
        case .leftMouseDown, .rightMouseDown, .otherMouseDown:
            pressedHere = true
            // A loop that tracks the press takes its release too. In the default mode the run
            // loop is the app's again, and the hardware says whether the button came up.
            RunLoop.main.perform(inModes: [.default]) { [weak self] in self?.releasedIfUp() }
        case .scrollWheel, .magnify, .rotate:
            let phase = event.phase
            if phase.isEmpty { return }
            if phase.isDisjoint(with: [.ended, .cancelled]) { trackpad.insert(event.type.rawValue) } else { lifted(event.type) }
        default: releasedIfUp()
        }
    }

    /// The fingers of a gesture lifted, said by whoever tracked it: this
    /// monitor, a scroll view, or AppKit's swipe tracking (`BackSwipe`).
    package func lifted(_ type: NSEvent.EventType) {
        if trackpad.remove(type.rawValue) != nil { changed() }
    }

    func attach(_ session: ExactSession) {}
    #else
    /// The touches UIKit has delivered and not yet ended, and the presses of
    /// a remote's or a game controller's buttons, held weakly: UIKit owns
    /// them and ends them. A press has no touch: tvOS's Select arrives as a
    /// [`UIPressesEvent`](https://developer.apple.com/documentation/uikit/uipressesevent)
    /// alone. A keyboard's key is not kept, as on the Mac.
    let touches = NSHashTable<UITouch>.weakObjects()
    let presses = NSHashTable<UIPress>.weakObjects()

    /// A touch or a button is down.
    var contactDown: Bool {
        touches.allObjects.contains { $0.phase != .ended && $0.phase != .cancelled }
            || presses.allObjects.contains { $0.phase != .ended && $0.phase != .cancelled }
    }

    private func pressed(_ sessions: [ExactSession]) -> Bool { contactDown }

    /// Whether `controller` or one under it is in a transition: a push or a
    /// pop, interactive or not, a presentation or a dismissal
    /// ([`transitionCoordinator`](https://developer.apple.com/documentation/uikit/uiviewcontroller/transitioncoordinator)).
    private func transitioning(_ controller: UIViewController?) -> Bool {
        guard let controller else { return false }
        if controller.transitionCoordinator != nil || controller.children.contains(where: transitioning) { return true }
        // A child answers with its ancestor's presented controller: follow each once.
        guard let presented = controller.presentedViewController, presented.presentingViewController === controller else { return false }
        return transitioning(presented)
    }

    /// An event of a session's window, before UIKit delivers it (`ExactWindow.sendEvent`).
    func receive(_ event: UIEvent) {
        for touch in event.allTouches ?? [] where touch.phase != .ended && touch.phase != .cancelled { touches.add(touch) }
        for press in (event as? UIPressesEvent)?.allPresses ?? [] where press.key == nil && press.phase != .ended && press.phase != .cancelled { presses.add(press) }
        changed()
    }

    /// Hears the touches and the presses in a session's view, for an embedder
    /// whose window is not an `ExactWindow`; it recognizes nothing and delays
    /// nothing.
    func attach(_ session: ExactSession) {
        let contact = ReplacementContact(target: nil, action: nil)
        contact.queue = self
        contact.cancelsTouchesInView = false
        contact.delaysTouchesBegan = false
        contact.delaysTouchesEnded = false
        session.presenter.viewport.addGestureRecognizer(contact)
    }
    #endif
}

#if canImport(UIKit)
private final class ReplacementContact: UIGestureRecognizer {
    weak var queue: ReplacementQueue?
    override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool { false }
    override func canBePrevented(by preventingGestureRecognizer: UIGestureRecognizer) -> Bool { false }
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) { queue?.receive(event) }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) { ended() }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { ended() }
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent) { queue?.receive(event) }
    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent) { ended() }
    override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent) { ended() }
    /// Fails once the last touch and the last button are up, so UIKit resets it for the next sequence.
    private func ended() {
        queue?.changed()
        if queue?.contactDown != true { state = .failed }
    }
}
#endif

private extension ExactSession {
    var presentingPriorTree: Bool { isApplyingPresentation || insideApply || landing || fillLanding }
}
