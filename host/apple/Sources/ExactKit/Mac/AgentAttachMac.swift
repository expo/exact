// What a person sees of an attached agent session on the Mac (LLP 1012.000
// D1): Develop's two items, the approval sheet on the window, the title bar's
// Agent Attached and the window server's picture of the window. The carrier
// is `AgentAttach.swift`.
#if EXACT_ATTACH && os(macOS)
import AppKit

extension AgentAttach {
    static let menuTarget = AttachMenuTarget()

    /// Develop's two items (LLP 1012.000 D1), after the app's first paint.
    public static func addMenuItems(to menu: NSMenu) {
        guard admitted else { return }
        menu.addItem(.separator())
        menu.addItem(withTitle: "Enable Agent Attach", action: #selector(AttachMenuTarget.toggle(_:)), keyEquivalent: "").target = menuTarget
        menu.addItem(withTitle: "Detach Agent", action: #selector(AttachMenuTarget.detach(_:)), keyEquivalent: "").target = menuTarget
    }
}

final class AttachMenuTarget: NSObject, NSMenuItemValidation {
    @objc func toggle(_ sender: Any?) { AgentAttach.toggle() }
    @objc func detach(_ sender: Any?) { AgentAttach.detach("detached in the app") }
    func validateMenuItem(_ item: NSMenuItem) -> Bool {
        if item.action == #selector(toggle(_:)) {
            item.state = AgentAttach.listening ? .on : .off
            return AgentAttach.listening || DevMenu.session?.firstDrawMs != nil
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
        alert.informativeText = "A program running as you asks to read “\(window.title.isEmpty ? ExactEnv.appName : window.title)”: its views, state, logs and pictures. These can show private information. Password fields are masked; logs and pictures are not. It cannot tap, type or change the app’s clock."
        // Allow is a deliberate click: Return, typed into the document as the sheet appears, answers nothing.
        alert.addButton(withTitle: "Allow").keyEquivalent = ""
        alert.addButton(withTitle: "Don’t Allow").keyEquivalent = "\u{1b}"
        self.alert = alert
        alert.beginSheetModal(for: window) { [weak self] response in
            self?.alert = nil
            answer(response == .alertFirstButtonReturn ? true : response == .alertSecondButtonReturn ? false : nil)
        }
    }

    /// Agent Attached, in the title bar.
    func showAttached() {
        guard let window else { return }
        let label = NSTextField(labelWithString: "Agent Attached")
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
    }

    /// The indicator, the sheet and the observers go.
    func end() {
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
