// What a person sees of an attached agent session in the iOS simulator (LLP
// 1012.000 D1, D2): the developer menu's Enable Agent Attach, Allow Agent
// Input and Detach Agent, an alert asking to allow reads, Agent Attached at
// the top of the screen while attached, and UIKit's snapshot of the app's
// window. With input allowed, a tap is a real touch the driver plays through
// the touch runner, aimed and checked here as a driven app's is. A simulator app
// alone: the driver reaches its socket through the Mac's `/tmp`, which a
// physical iPhone does not share. The carrier is `AgentAttach.swift`.
#if EXACT_ATTACH && os(iOS) && targetEnvironment(simulator)
import UIKit

extension AgentAttach {
    /// The developer menu's rows, once the app has painted.
    static func menuCommands() -> [DevMenuController.Command] {
        guard admitted else { return [] }
        // Turning it off and Detach end the connection at the tap.
        var enable = DevMenuController.Command(title: "Enable Agent Attach", symbol: "dot.radiowaves.left.and.right", action: toggle, checked: listening, immediate: true)
        if !listening, DevMenu.session?.firstDrawMs == nil {
            enable.unavailable = ("Agent Attach Is Unavailable", "Agent Attach can be enabled once the app has drawn its first frame.")
        }
        var detach = DevMenuController.Command(title: "Detach Agent", symbol: "xmark.circle", action: { AgentAttach.detach("detached in the app") }, immediate: true)
        if !connected { detach.unavailable = ("No Agent Is Attached", "Detach Agent ends an agent's connection to this app. No agent is connected now.") }
        var input = DevMenuController.Command(title: "Allow Agent Input", symbol: "hand.tap", action: toggleInput, checked: inputAllowed, immediate: true)
        if !attached { input.unavailable = ("No Agent Is Attached", "Allow Agent Input lets an attached agent tap and type in this app. Allow an agent to read the app first.") }
        return [enable, input, detach]
    }
}

/// The app's window a connection reads: pinned at the handshake, asked in an
/// alert, marked at the top of the screen while attached. Main thread.
final class AttachSurface {
    weak var window: UIWindow?
    weak var session: ExactSession?
    private weak var alert: UIAlertController?
    private var indicator: UIWindow?
    private var appearance: (any UITraitChangeRegistration)?
    private weak var label: UILabel?
    private var observers: [NSObjectProtocol] = []
    private var closed = false

    private init(window: UIWindow, session: ExactSession) {
        self.window = window
        self.session = session
    }

    /// The window still shows the session the person allowed: a session moved to another window is not read under
    /// this approval.
    var isOpen: Bool {
        guard !closed, let window, let session, window.windowScene != nil, !window.isHidden else { return false }
        return Self.appWindow(session) === window
    }

    /// The window that shows `session`: its view's, or, under a full-screen presentation that takes the view out of
    /// the window, the window of the root controller's topmost presentation.
    private static func appWindow(_ session: ExactSession) -> UIWindow? {
        if let window = session.view?.window { return window }
        var top = DevMenu.controller
        while let next = top?.presentedViewController { top = next }
        return top?.viewIfLoaded?.window
    }
    var title: String { window?.windowScene?.title.flatMap { $0.isEmpty ? nil : $0 } ?? ExactEnv.appName }
    var number: Int? { nil }

    static func couldNotStart(_ why: String) {
        let alert = UIAlertController(title: "Agent Attach Could Not Start", message: why, preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: "OK", style: .default))
        DevMenu.present(alert)
    }

    /// The app's one session, in a scene in the foreground. `wanted` must be its label.
    static func pin(_ wanted: String?) -> Result<AttachSurface, AttachRequest.Refusal> {
        guard let session = DevMenu.session, let window = appWindow(session), let scene = window.windowScene else {
            return .failure(.init(reply: ["error": "attach: no window of this app shows a session", "refused": "window", "sessions": []]))
        }
        if let wanted, wanted != session.label {
            return .failure(.init(reply: ["error": "attach: no window shows session \(wanted)", "refused": "window", "sessions": [session.label]]))
        }
        // In the background the app is soon suspended, and its person is elsewhere.
        guard scene.activationState == .foregroundActive || scene.activationState == .foregroundInactive else {
            return .failure(.init(reply: ["error": "attach: the app is not in the foreground: open it in the simulator", "refused": "window"]))
        }
        return .success(AttachSurface(window: window, session: session))
    }

    /// `ended` hears the scene leave: disconnected, or sent to the background,
    /// where the system suspends the app and nothing could answer a request.
    func watch(_ ended: @escaping (String) -> Void) {
        guard let scene = window?.windowScene else { return }
        let center = NotificationCenter.default
        observers = [
            center.addObserver(forName: UIScene.didDisconnectNotification, object: scene, queue: .main) { [weak self] _ in
                self?.closed = true
                ended("the attached window closed")
            },
            center.addObserver(forName: UIScene.didEnterBackgroundNotification, object: scene, queue: .main) { [weak self] _ in
                self?.closed = true
                ended("the app went to the background")
            },
        ]
    }

    /// Ask the person in an alert over whatever the app presents: `answer(true)` for Allow, `false` for Don’t Allow,
    /// `nil` when the alert was taken down without an answer. Neither is the preferred action, so Return on a
    /// hardware keyboard allows nothing; Escape is Don’t Allow, the cancel action.
    func ask(_ answer: @escaping (Bool?) -> Void) {
        let alert = UIAlertController(title: "Allow an Agent to Read This App?",
            message: "A program running as you on this Mac asks to read \(title): its views, state, logs and pictures. These can show private information. Password fields are masked; logs and pictures are not. It cannot tap, type or run the app’s tools unless you also choose Allow Agent Input in the developer menu, and it never changes the app’s clock.",
            preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: "Don’t Allow", style: .cancel) { _ in answer(false) })
        alert.addAction(UIAlertAction(title: "Allow", style: .default) { _ in answer(true) })
        self.alert = alert
        DevMenu.present(alert)
    }

    /// Agent Attached, in a small window above the app's at the top of its safe area, which no touch reaches: what
    /// the app presents, a sheet or an alert, cannot cover it, and the app's own picture leaves it out.
    func showAttached(input: Bool = false) {
        // The window logs touches while input is allowed: the driver's runner touch is checked against it.
        // A grant starts the log afresh: no connection reads another's touches.
        if input != TouchLog.attached { TouchLog.forget() }
        TouchLog.attached = input
        TouchLog.attachedSession = input ? session : nil
        if let label {
            label.text = input ? "Agent Attached · Input Allowed" : "Agent Attached"
            label.superview?.superview?.accessibilityLabel = label.text
            // The label is in the capsule's content view, which its new width resizes.
            if let pill = label.superview?.superview { pill.layoutIfNeeded(); pill.layer.cornerRadius = pill.bounds.height / 2 }
            return
        }
        guard let scene = window?.windowScene, let appWindow = window else { return }
        let overlay = FrameHealthWindow(windowScene: scene)
        // The card's root controller: it defers the status bar, the orientations and the home indicator to the app.
        let root = FrameHealthWindowController()
        overlay.rootViewController = root
        overlay.windowLevel = .alert + 1
        overlay.backgroundColor = .clear
        overlay.isUserInteractionEnabled = false
        // It descends from no app view, so it follows the app window's appearance, which a `color-scheme` overrides.
        overlay.overrideUserInterfaceStyle = appWindow.traitCollection.userInterfaceStyle
        appearance = MainActor.assumeIsolated {
            appWindow.registerForTraitChanges([UITraitUserInterfaceStyle.self]) { [weak overlay] (window: UIWindow, _: UITraitCollection) in
                overlay?.overrideUserInterfaceStyle = window.traitCollection.userInterfaceStyle
            }
        }
        let pill = UIVisualEffectView(effect: UIBlurEffect(style: .systemThinMaterial))
        pill.layer.cornerCurve = .continuous
        pill.clipsToBounds = true
        let label = UILabel()
        self.label = label
        label.text = input ? "Agent Attached · Input Allowed" : "Agent Attached"
        label.font = .preferredFont(forTextStyle: .caption1)
        label.adjustsFontForContentSizeCategory = true
        label.textColor = .secondaryLabel
        label.translatesAutoresizingMaskIntoConstraints = false
        pill.contentView.addSubview(label)
        pill.translatesAutoresizingMaskIntoConstraints = false
        root.view.addSubview(pill)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: pill.contentView.leadingAnchor, constant: 10),
            label.trailingAnchor.constraint(equalTo: pill.contentView.trailingAnchor, constant: -10),
            label.topAnchor.constraint(equalTo: pill.contentView.topAnchor, constant: 4),
            label.bottomAnchor.constraint(equalTo: pill.contentView.bottomAnchor, constant: -4),
            pill.centerXAnchor.constraint(equalTo: root.view.centerXAnchor),
            pill.topAnchor.constraint(equalTo: root.view.safeAreaLayoutGuide.topAnchor, constant: 2),
        ])
        root.view.layoutIfNeeded()
        pill.layer.cornerRadius = pill.bounds.height / 2
        pill.isAccessibilityElement = true
        pill.accessibilityLabel = "Agent Attached"
        // Shown without becoming key: the app's window keeps the keyboard and the first responder.
        overlay.isHidden = false
        indicator = overlay
    }

    /// The indicator, the alert and the observers go. An alert still waiting for a presentation to finish
    /// (`DevMenu.present`) is never shown.
    func end() {
        TouchLog.attached = false
        TouchLog.attachedSession = nil
        indicator?.isHidden = true
        indicator = nil
        if let appearance, let window { MainActor.assumeIsolated { window.unregisterForTraitChanges(appearance) } }
        appearance = nil
        if let alert {
            if alert.presentingViewController != nil { alert.dismiss(animated: true) }
            else if DevMenu.waiting === alert { DevMenu.pending += 1; DevMenu.waiting = nil }
        }
        alert = nil
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
        observers = []
    }

    /// UIKit's snapshot of the app's window as the screen last showed it,
    /// what it presents included: nothing is laid out, drawn again or settled
    /// for it (`afterScreenUpdates: false`).
    func picture() -> [String: Any] {
        guard let window, window.bounds.width > 0, window.bounds.height > 0 else { return ["error": "the app's window is gone"] }
        let format = UIGraphicsImageRendererFormat()
        format.scale = window.traitCollection.displayScale
        format.opaque = true
        // 8-bit sRGB, which scripts/png.mjs reads: a wide-color screen's renderer would write 16-bit.
        format.preferredRange = .standard
        var drawn = false
        let png = UIGraphicsImageRenderer(bounds: window.bounds, format: format).pngData { _ in
            drawn = window.drawHierarchy(in: window.bounds, afterScreenUpdates: false)
        }
        guard drawn else { return ["error": "UIKit gave no snapshot of the app's window"] }
        let state = window.windowScene?.activationState == .foregroundActive ? "foregroundActive" : "foregroundInactive"
        var reply: [String: Any] = ["data": png.base64EncodedString(), "bytes": png.count,
                "width": Int((window.bounds.width * format.scale).rounded()), "height": Int((window.bounds.height * format.scale).rounded()),
                "scale": Double(format.scale), "captureSpace": "window", "capture": "uikit-snapshot", "scene": state]
        // A web view or native module draws in another process, whose layer this snapshot can leave blank; the driver's
        // own capture flushes it first, which an attached session does not do to the person's app.
        if let session, session.presenter.views.values.contains(where: { $0.web != nil }) || !session.natives.snapshotViews.isEmpty {
            reply["limitation"] = "web views and native modules in the window may be blank in this picture: an attached session does not flush their layers"
        }
        return reply
    }

    /// Why input cannot be delivered now, or nil: the scene must be active in front (not behind Control Center, a
    /// call or an alert of the system's), and no finger of the person's down in the window.
    func inputRefusal() -> [String: Any]? {
        guard let scene = window?.windowScene else { return ["error": "the attached window closed", "refused": "window"] }
        if scene.activationState != .foregroundActive {
            return ["error": "background: the app's scene is not active in front; input reaches it only there", "refused": "background"]
        }
        if TouchLog.touching { return ["error": "person-input: a finger is down in the app, or a system gesture took one; input waits for the person's own touch to end", "refused": "person-input"] }
        return nil
    }

    /// The touch runner's two questions under the grant (LLP 1080.000 D4, D5): where a finger aimed at a target
    /// lands, refused by name as a driven app's tap is, and what the window dispatched since. The touch itself is
    /// the runner's, which the driver plays only after an aim.
    func perform(_ form: AttachRequest.Input, session: ExactSession) -> [String: Any] {
        switch form {
        case let .log(after): return TouchLog.read(after: after)
        case let .aim(target):
            switch AttachRequest.view(target, in: session, window: window) {
            case let .failure(refusal): return refusal.reply
            case let .success(v):
                var reply = Agent(session: session).aim(["op": "tap", "id": Int(v.id), "aim": ["press": true]])
                reply["id"] = Int(v.id)
                return reply
            }
        case let .textTarget(target, focused):
            switch AttachRequest.view(target, in: session, window: window) {
            case let .failure(refusal): return refusal.reply
            case let .success(v):
                let editor: UIView? = v.textArea ?? v.field
                guard let editor, !v.inert, !v.disabled, v.props["editable"] != "false" else {
                    return ["error": "target: view #\(v.id) is not an editable text input", "refused": "target"]
                }
                if focused, !editor.isFirstResponder { return ["error": "target: view #\(v.id) does not have the keyboard; nothing was typed", "refused": "target"] }
                return ["id": Int(v.id), "editable": true, "focused": editor.isFirstResponder]
            }
        case .tap, .type: return ["error": "unsupported: in the simulator a tap is a real touch the driver's touch runner plays, after `aim`; text is the runner's keystrokes", "refused": "unsupported"]
        case .call, .called: return ["error": "a call is the carrier's", "refused": "unsupported"]
        }
    }
}
#endif
