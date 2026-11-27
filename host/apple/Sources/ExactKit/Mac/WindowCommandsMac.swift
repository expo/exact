// The commands an app gives its own window (@ref LLP 1069.010): a quit the
// app holds leaves full screen and fades the window first, as Electron's
// `setFullScreen(false)` and `setOpacity(0)` do for T3 Code's ⌘Q hold; a
// utility that keeps running puts its window away (@ref LLP 1133.025 D4).
// The window's owner (the Mac app's delegate) calls them, as it does `close()`.
#if os(macOS)
import AppKit

/// AppKit's resize duration and easing, with each frame identifiable and
/// the remaining steps cancellable when another source moves the window.
private final class FrameAnimation: NSAnimation {
    var step: ((CGFloat) -> Void)?
    override var currentProgress: NSAnimation.Progress {
        get { super.currentProgress }
        set {
            super.currentProgress = newValue
            step?(newValue == 1 ? 1 : CGFloat(currentValue))
        }
    }
}

private nonisolated(unsafe) var placementKey: UInt8 = 0

extension NSWindow {
    /// A session may be in Exact's window or in one the embedding app supplies.
    var placement: ExactWindowPlacement {
        if let existing = objc_getAssociatedObject(self, &placementKey) as? ExactWindowPlacement { return existing }
        let placement = ExactWindowPlacement()
        objc_setAssociatedObject(self, &placementKey, placement, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        return placement
    }
}

public final class ExactWindowPlacement: NSObject {
    var requestedFloating = false
    private var expected: NSRect?
    private var animation: FrameAnimation?
    private var request = 0
    private var watching = false

    deinit { NotificationCenter.default.removeObserver(self) }

    private func cancel() {
        request += 1
        expected = nil
        animation?.step = nil
        animation?.stop()
        animation = nil
    }

    @objc private func frameChanged(_ note: Notification) {
        if let expected, let window = note.object as? NSWindow, window.frame != expected { cancel() }
    }

    @objc private func displaysChanged(_ note: Notification) { cancel() }

    /// Both deferred paths take their place in the same window's request order.
    func begin(on window: NSWindow) -> Int {
        if !watching {
            watching = true
            let center = NotificationCenter.default
            for name in [NSWindow.didMoveNotification, NSWindow.didResizeNotification, NSWindow.didEndLiveResizeNotification] {
                center.addObserver(self, selector: #selector(frameChanged(_:)), name: name, object: window)
            }
            center.addObserver(self, selector: #selector(displaysChanged(_:)), name: NSApplication.didChangeScreenParametersNotification, object: nil)
        }
        cancel()
        expected = window.frame
        return request
    }

    func isCurrent(_ ticket: Int) -> Bool { ticket == request }

    public func setBounds(_ args: [Any], on window: NSWindow, log: (String) -> Void) {
        guard (1...2).contains(args.count), let values = args[0] as? [NSNumber], values.count == 4,
              values.allSatisfy({ CFGetTypeID($0) != CFBooleanGetTypeID() && $0.doubleValue.isFinite }),
              values[2].doubleValue > 0, values[3].doubleValue > 0,
              args.count == 1 || (args[1] as? NSNumber).map({ CFGetTypeID($0) == CFBooleanGetTypeID() }) == true,
              let top = NSScreen.screens.first?.frame.maxY else {
            return log("setBounds refused: expected a finite DOMRect with positive dimensions and an optional boolean")
        }
        let height = values[3].doubleValue
        let rect = NSRect(x: values[0].doubleValue, y: top - values[1].doubleValue - height,
                          width: values[2].doubleValue, height: height)
        guard rect.minY.isFinite, rect.maxX.isFinite, rect.maxY.isFinite else { return log("setBounds refused: frame exceeds screen coordinates") }
        let animate = args.last as? Bool == true && !ExactEnv.agentFreezes && !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        let start = window.frame, ticket = begin(on: window)
        guard rect != start else { return }
        DispatchQueue.main.async { [weak self, weak window] in
            guard let self, let window, request == ticket, window.frame == start else { return }
            let target = window.constrainFrameRect(rect, to: nil)
            guard animate, window.isVisible, target != start else {
                expected = nil
                window.setFrame(target, display: true)
                return
            }
            let motion = FrameAnimation(duration: window.animationResizeTime(target), animationCurve: .easeInOut)
            animation = motion
            motion.animationBlockingMode = .nonblocking
            motion.step = { [weak self, weak window] progress in
                guard let self, let window, self.request == ticket else { return }
                guard window.frame == self.expected else { self.cancel(); return }
                func mix(_ a: CGFloat, _ b: CGFloat) -> CGFloat { (a + (b - a) * progress).rounded() }
                let step = NSRect(x: mix(start.minX, target.minX), y: mix(start.minY, target.minY),
                                  width: mix(start.width, target.width), height: mix(start.height, target.height))
                // The title bar can meet another screen's menu bar on the way.
                let frame = progress == 1 ? target : window.constrainFrameRect(step, to: nil)
                self.expected = progress == 1 ? nil : frame
                window.setFrame(frame, display: true)
            }
            motion.start()
        }
    }

    public static func observed(_ window: NSWindow) -> [String: Any] {
        let frame = window.frame, aspect = window.contentAspectRatio
        return ["screenX": frame.minX, "screenY": (NSScreen.screens.first?.frame.maxY ?? 0) - frame.maxY,
                "outerWidth": frame.width, "outerHeight": frame.height,
                "aspectRatio": aspect.height > 0 ? aspect.width / aspect.height : 0]
    }
}

public enum ExactWindowCommands {
    /// An app command for this window. The caller handles names this returns false for.
    public static func perform(_ name: String, _ args: [Any], on window: NSWindow, log: (String) -> Void) -> Bool {
        switch name {
        case "exitFullscreen":
            // AppKit animates the window out of its space; a window that is
            // not full screen stays as it is, as the web's call does nothing then.
            if window.styleMask.contains(.fullScreen) { window.toggleFullScreen(nil) }
        case "setBounds":
            window.placement.setBounds(args, on: window, log: log)
        case "setWindowAlwaysOnTop", "setWindowIgnoreMouseEvents":
            guard args.count == 1, let n = args.first as? NSNumber, CFGetTypeID(n) == CFBooleanGetTypeID() else {
                log("\(name) refused: expected one boolean")
                return true
            }
            if name == "setWindowIgnoreMouseEvents" { window.ignoresMouseEvents = n.boolValue }
            else {
                window.requestedFloating = n.boolValue
                window.level = AgentLink.windowsBehind ? .agentBehind : n.boolValue ? .floating : .normal
            }
        case "setAspectRatio":
            guard args.count == 1, let n = args.first as? NSNumber, CFGetTypeID(n) != CFBooleanGetTypeID(),
                  n.doubleValue.isFinite, n.doubleValue >= 0 else {
                log("setAspectRatio refused: expected a positive ratio or zero to unlock")
                return true
            }
            if n.doubleValue == 0 { window.contentResizeIncrements = NSSize(width: 1, height: 1) }
            else { window.contentAspectRatio = NSSize(width: n.doubleValue, height: 1) }
        case "setWindowOpacity":
            // `alphaValue`: a window at 0 is unseen but stays key and hears keys.
            guard let number = args.first as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID(),
                  number.doubleValue.isFinite, (1...2).contains(args.count) else {
                log("setWindowOpacity refused: the opacity is a number from 0 to 1")
                return true
            }
            var duration = 0.0
            if args.count == 2 {
                guard let ms = args[1] as? NSNumber, CFGetTypeID(ms) != CFBooleanGetTypeID(),
                      ms.doubleValue.isFinite, ms.doubleValue >= 0 else {
                    log("setWindowOpacity refused: duration is a nonnegative number of milliseconds")
                    return true
                }
                duration = ms.doubleValue / 1000
            }
            let opacity = CGFloat(min(1, max(0, number.doubleValue)))
            if duration > 0 && !ExactEnv.agentFreezes && !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
                NSAnimationContext.runAnimationGroup { context in
                    context.duration = duration
                    window.animator().alphaValue = opacity
                }
            } else { window.alphaValue = opacity }
        default:
            return false
        }
        return true
    }

    /// `hideWindow()` (LLP 1133.025 D4): `window` is put away as the
    /// person's close puts it away when the app keeps running without it
    /// (`keeps`, `ExactDocuments.keepsWindow`): ordered out, its session
    /// kept, its page `hidden` and `persisted`, and the next showing a
    /// restore. Where the app would not keep running, a hidden window would
    /// be out of reach, so it is refused and the window stays. A window not
    /// showing is left as it is.
    public static func hide(_ window: NSWindow, keeps: Bool, log: (String) -> Void) {
        guard keeps else {
            return log("hideWindow refused: the app ends without its window; it keeps running with host.macos.window.keepRunning or a status item placed")
        }
        guard window.isVisible || window.isMiniaturized else { return }
        _ = ExactDocuments.hidesOnClose(window, only: true, keepsRunning: true)
    }
}
#endif
