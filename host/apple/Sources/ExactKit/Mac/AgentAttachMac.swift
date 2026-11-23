// What a person sees of an attached agent session on the Mac (LLP 1012.000
// D1): Develop's items, the approval sheet on the window, the title bar's
// Agent Attached and the window server's picture of the window; and, once the
// person allows input, the click and text entry an attached tap and type make.
// The carrier is `AgentAttach.swift`.
#if EXACT_ATTACH && os(macOS)
import AppKit

extension AgentAttach {
    static let menuTarget = AttachMenuTarget()

    /// Develop's items (LLP 1012.000 D1), after the app's first paint.
    public static func addMenuItems(to menu: NSMenu) {
        guard admitted else { return }
        menu.addItem(.separator())
        menu.addItem(withTitle: "Enable Agent Attach", action: #selector(AttachMenuTarget.toggle(_:)), keyEquivalent: "").target = menuTarget
        menu.addItem(withTitle: "Allow Agent Input", action: #selector(AttachMenuTarget.input(_:)), keyEquivalent: "").target = menuTarget
        menu.addItem(withTitle: "Detach Agent", action: #selector(AttachMenuTarget.detach(_:)), keyEquivalent: "").target = menuTarget
    }
}

final class AttachMenuTarget: NSObject, NSMenuItemValidation {
    @objc func toggle(_ sender: Any?) { AgentAttach.toggle() }
    @objc func detach(_ sender: Any?) { AgentAttach.detach("detached in the app") }
    @objc func input(_ sender: Any?) { AgentAttach.toggleInput() }
    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        if item.action == #selector(toggle(_:)) {
            item.state = AgentAttach.listening ? .on : .off
            return AgentAttach.listening || DevMenu.session?.firstDrawMs != nil
        }
        if item.action == #selector(input(_:)) {
            item.state = AgentAttach.inputAllowed ? .on : .off
            return AgentAttach.attached
        }
        return AgentAttach.connected
    }
}

/// The window a connection reads: pinned at the handshake, asked in a sheet,
/// marked in its title bar while attached. Main thread.
final class AttachSurface {
    weak var window: NSWindow?
    weak var session: ExactSession?
    private var alert: NSAlert?
    private var indicator: NSTitlebarAccessoryViewController?
    private weak var label: NSTextField?
    /// While input is allowed, the app's own trackpad and wheel events, which hold no button or modifier: a person's
    /// scroll, pinch, rotation or swipe in progress, and when the last of them came.
    private var gestureMonitor: Any?
    private var gesturing = false, lastGesture = 0.0
    private var closeObserver: NSObjectProtocol?
    private var shownObserver: NSKeyValueObservation?

    private init(window: NSWindow, session: ExactSession) {
        self.window = window
        self.session = session
    }

    var isOpen: Bool { window != nil }
    var title: String { window?.title ?? "" }
    var number: Int? { window?.windowNumber }

    static func couldNotStart(_ why: String) {
        let alert = NSAlert()
        alert.messageText = "Agent Attach could not start"
        alert.informativeText = why
        alert.runModal()
    }

    /// The windows a driver can attach to: each visible window that shows one session.
    static func eligible() -> [(NSWindow, ExactSession)] {
        func sessions(in view: NSView) -> [ExactSession] {
            if let exact = view as? ExactView { return [exact.session] }
            return view.subviews.flatMap { sessions(in: $0) }
        }
        return NSApp.windows.compactMap { window in
            guard window.isVisible, window.sheetParent == nil, let root = window.contentView else { return nil }
            let found = sessions(in: root)
            return found.count == 1 ? (window, found[0]) : nil
        }
    }

    /// The one window `wanted` names, or the app's only one.
    static func pin(_ wanted: String?) -> Result<AttachSurface, AttachRequest.Refusal> {
        let windows = eligible()
        let chosen = wanted.map { label in windows.filter { $0.1.label == label } } ?? windows
        // Before the person allows it, the driver learns only the labels it chooses a window by (`main`, `window-2`, a
        // link's target), and only when it named none of several; never a window's title.
        guard chosen.count == 1, let (window, session) = chosen.first else {
            let why = windows.isEmpty ? "no window of this app shows a session"
                : wanted.map { "no window shows session \($0)" } ?? "the app has \(windows.count) windows: name one with --session <label>"
            return .failure(.init(reply: ["error": "attach: \(why)", "refused": "window", "sessions": windows.map(\.1.label)]))
        }
        // The indicator is a title bar accessory, so a window without one cannot say it is attached.
        guard window.styleMask.contains(.titled) else {
            return .failure(.init(reply: ["error": "attach: the window has no title bar to show Agent Attached in", "refused": "window"]))
        }
        return .success(AttachSurface(window: window, session: session))
    }

    /// `closed` hears the window close. A kept window's close orders it out
    /// (`ExactDocuments.hidesOnClose`): that ends the attachment too.
    /// Minimizing it or hiding the app does not.
    func watch(_ closed: @escaping (String) -> Void) {
        guard let window else { return }
        closeObserver = NotificationCenter.default.addObserver(forName: NSWindow.willCloseNotification, object: window, queue: .main) { _ in
            closed("the attached window closed")
        }
        shownObserver = window.observe(\.isVisible) { window, _ in
            if !window.isVisible, PageFacts.hiddenForClose(window) { closed("the attached window closed"); return }
            DispatchQueue.main.async {
                if !window.isVisible, !window.isMiniaturized, !NSApp.isHidden { closed("the attached window closed") }
            }
        }
    }

    /// Ask the person in a sheet on the window: `answer(true)` for Allow, `false` for Don’t Allow, `nil` when the
    /// sheet was taken down without an answer.
    func ask(_ answer: @escaping (Bool?) -> Void) {
        guard let window else { answer(nil); return }
        let alert = NSAlert()
        alert.messageText = "Allow an agent to read this window?"
        alert.informativeText = "A program running as you asks to read “\(window.title.isEmpty ? ExactEnv.appName : window.title)”: its views, state, logs and pictures. These can show private information. Password fields are masked; logs and pictures are not. It cannot tap, type or run the app’s tools unless you also choose Develop ▸ Allow Agent Input, and it never changes the app’s clock."
        // Allow is a deliberate click: Return, typed into the document as the sheet appears, answers nothing.
        alert.addButton(withTitle: "Allow").keyEquivalent = ""
        alert.addButton(withTitle: "Don’t Allow").keyEquivalent = "\u{1b}"
        self.alert = alert
        alert.beginSheetModal(for: window) { [weak self] response in
            self?.alert = nil
            answer(response == .alertFirstButtonReturn ? true : response == .alertSecondButtonReturn ? false : nil)
        }
    }

    /// Agent Attached, in the title bar; with input allowed, it says so.
    func showAttached(input: Bool = false) {
        guard let window else { return }
        watchGestures(input)
        if let label {
            label.stringValue = input ? "Agent Attached · Input Allowed" : "Agent Attached"
            label.sizeToFit()
            label.superview?.frame.size.width = label.frame.width + 16
            return
        }
        let label = NSTextField(labelWithString: input ? "Agent Attached · Input Allowed" : "Agent Attached")
        label.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        label.textColor = .secondaryLabelColor
        label.sizeToFit()
        let holder = NSView(frame: NSRect(x: 0, y: 0, width: label.frame.width + 16, height: label.frame.height))
        label.frame.origin = NSPoint(x: 8, y: 0)
        holder.addSubview(label)
        let accessory = NSTitlebarAccessoryViewController()
        accessory.view = holder
        accessory.layoutAttribute = .trailing
        window.addTitlebarAccessoryViewController(accessory)
        indicator = accessory
        self.label = label
    }

    /// Why input cannot be delivered now, or nil: the app must be active with this window key, since an attached
    /// session never activates the app or makes a window key; and no button or modifier may be down, a person's
    /// gesture or chord in progress.
    func inputRefusal() -> [String: Any]? {
        guard let window else { return ["error": "the attached window closed", "refused": "window"] }
        if !NSApp.isActive || !window.isKeyWindow {
            return ["error": "background: input reaches only the key window of the active app, which the person brings forward; an attached session never does", "refused": "background"]
        }
        if window.attachedSheet != nil { return ["error": "background: a sheet is up on the window", "refused": "background"] }
        // A wheel event a quarter second old with no phase is a wheel still turning.
        if NSEvent.pressedMouseButtons != 0 || !NSEvent.modifierFlags.intersection([.shift, .control, .option, .command]).isEmpty
            || gesturing || ProcessInfo.processInfo.systemUptime - lastGesture < 0.25 {
            return ["error": "person-input: a mouse button, a modifier key or a trackpad gesture is down in the app; input waits for the person's own to end", "refused": "person-input"]
        }
        return nil
    }

    /// A granted tap or text entry, through the click and text paths a driven app takes.
    func perform(_ form: AttachRequest.Input, session: ExactSession) -> [String: Any] {
        let found: Any
        switch form {
        case let .tap(target), let .type(target, _): found = target
        case .aim, .log, .textTarget: return ["error": "unsupported: aim, the dispatch log and a text target's check are the iOS simulator's touch runner's; a Mac tap is the app's own click", "refused": "unsupported"]
        case .call, .called: return ["error": "a call is the carrier's", "refused": "unsupported"]
        }
        let v: NodeView
        switch AttachRequest.view(found, in: session, window: window) {
        case let .failure(refusal): return refusal.reply
        case let .success(view): v = view
        }
        guard v.window === window else { return ["error": "view #\(v.id) is not in the attached window", "refused": "target"] }
        let agent = Agent(session: session)
        if case let .type(_, text) = form { return agent.attachedType(v, text) }
        return agent.tap(["op": "tap", "id": Int(v.id), "attached": true])
    }

    private func watchGestures(_ on: Bool) {
        if !on { gestureMonitor.map(NSEvent.removeMonitor); gestureMonitor = nil; gesturing = false; return }
        guard gestureMonitor == nil else { return }
        gestureMonitor = NSEvent.addLocalMonitorForEvents(matching: [.scrollWheel, .magnify, .rotate, .swipe, .smartMagnify]) { [weak self] event in
            let moving: NSEvent.Phase = [.began, .changed, .stationary, .mayBegin]
            self?.gesturing = !event.phase.intersection(moving).isEmpty || !event.momentumPhase.intersection(moving).isEmpty
            self?.lastGesture = ProcessInfo.processInfo.systemUptime
            return event
        }
    }

    /// The indicator, the sheet and the observers go.
    func end() {
        watchGestures(false)
        indicator?.removeFromParent()
        indicator = nil
        if let alert, let window { window.endSheet(alert.window, returnCode: .abort) }
        alert = nil
        if let closeObserver { NotificationCenter.default.removeObserver(closeObserver) }
        closeObserver = nil
        shownObserver?.invalidate()
        shownObserver = nil
    }

    /// The window server's picture of the whole window as it is, its title
    /// bar and every pane included: no draw, no reorder, no activation.
    func picture() -> [String: Any] {
        guard let window, let image = WindowServerPicture.ownWindow(window.windowNumber),
              let rep = NSBitmapImageRep(cgImage: image).converting(to: .sRGB, renderingIntent: .default),
              let png = rep.representation(using: .png, properties: [:]) else {
            return ["error": "the window server gave no picture of the window: it is minimized, the screen is locked or the display is asleep"]
        }
        return ["data": png.base64EncodedString(), "bytes": png.count, "width": image.width, "height": image.height,
                "scale": Double(image.width) / window.frame.width, "captureSpace": "window", "capture": "window-server",
                "key": window.isKeyWindow, "occlusion": window.occlusionState.contains(.visible) ? "visible" : "occluded"]
    }
}
#endif

#if EXACT_ATTACH && os(macOS)
extension Agent {
    /// An attached session's text entry (LLP 1012.000 D3): the field's or text area's own editor takes the whole text,
    /// as a driven app's does, in a window already key. A control that takes text another way (a picker, a toolbar
    /// search, a web view, a native module) is refused, not set.
    func attachedType(_ v: NodeView, _ text: String) -> [String: Any] {
        guard let win = v.window, win.isKeyWindow else { return ["error": "background: the field's window is not key", "refused": "background"] }
        guard !v.inert, !v.disabled, v.props["editable"] != "false" else { return ["error": "view #\(v.id) is hidden, inert, disabled or readonly", "refused": "target"] }
        guard !v.visibleRect.isEmpty else { return ["error": "target: view #\(v.id) is out of view; a person would scroll to it first", "refused": "target"] }
        guard v.kind != "native", v.kind != "iframe", v.pageField == nil, v.props["navigationBack"] == nil else {
            return ["error": "unsupported: view #\(v.id) takes text another way than a field's editor", "refused": "unsupported"]
        }
        let entry = TextInputLimit.prefix(text, props: v.props)
        let refusedFocus: [String: Any] = ["error": "target: view #\(v.id) did not take the keyboard focus", "refused": "target"]
        let gone: [String: Any] = ["error": "input-not-granted: the grant, the connection or the window changed before the text", "refused": "input-not-granted"]
        if let f = v.textArea {
            if win.firstResponder !== f, !win.makeFirstResponder(f) { return refusedFocus }
            guard Agent.inputGate?() == true else { return gone }
            f.selectAll(nil)
            f.insertText(entry, replacementRange: f.selectedRange())
            return ["typed": Int(v.id), "value": Agent.shownValue(f.string, of: v), "delivery": "platform"]
        }
        guard let f = v.field else { return ["error": "view #\(v.id) is not a text input", "refused": "target"] }
        // A field already being edited keeps its editor: asking again would end the editing and begin it.
        if f.currentEditor().map({ win.firstResponder !== $0 }) ?? true, !win.makeFirstResponder(f) { return refusedFocus }
        guard let editor = f.currentEditor() as? NSTextView else { return ["error": "the field has no editor"] }
        guard Agent.inputGate?() == true else { return gone }
        editor.selectAll(nil)
        editor.insertText(entry, replacementRange: editor.selectedRange())
        return ["typed": Int(v.id), "value": Agent.shownValue(f.stringValue, of: v), "delivery": "platform"]
    }
}
#endif
