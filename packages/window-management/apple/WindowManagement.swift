// @exact/window-management on macOS (LLP 1178 D1, D2, D6): `NSScreen`'s
// displays and `NSEvent.mouseLocation`, in the Window Management API's
// coordinates. The app's composed module calls it outside the agent; under
// the agent the runner's substitute answers, unless the drive binds it.
//
// - AppKit's global space has its origin at the primary display's bottom
//   left, y upward, in points; the web's has it at the primary's top left,
//   y downward, in CSS pixels. A point is a CSS pixel, so each rectangle is
//   flipped at the primary's top and nothing is scaled.
// - The primary is the screen `NSScreen.screens` lists first, the menu
//   bar's (`NSScreen.main` is the key window's, which can be another app's).
// - `currentScreen` is the screen holding most of the session's window,
//   shown or hidden; with no window, the primary.
// - The source re-answers on `didChangeScreenParametersNotification` (a
//   display added, removed, rearranged or rescaled, the Dock or menu bar
//   moved) and when the session's window moves to another screen.
import AppKit
import ExactPackage

public final class WindowManagementModule: ExactPackage {
    private let context: ExactPackageContext
    private var observers: [NSObjectProtocol] = []
    /// The window whose screen changes are heard, and the screen it was on.
    private weak var watchedWindow: NSWindow?
    private var lastScreen: CGDirectDisplayID?

    public init(context: ExactPackageContext) { self.context = context }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        DispatchQueue.main.async { [self] in
            switch name {
            case "getScreenDetails":
                watch()
                reply(details())
            case "getPointerPosition":
                reply(pointer())
            case "windowManagementStatus":
                reply(["available": true, "state": "granted"])
            default:
                reply(["error": "NotSupportedError"])
            }
        }
    }

    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { ["error": "NotSupportedError"] }

    public func release(_ name: String) {
        guard name == "getScreenDetails" else { return }
        DispatchQueue.main.async { [self] in unwatch() }
    }

    public func destroy(_ done: @escaping () -> Void) {
        DispatchQueue.main.async { [self] in
            unwatch()
            done()
        }
    }

    // MARK: The displays

    /// The session's window, shown or hidden; nil when it has none.
    private var window: NSWindow? { context.view?.window }

    /// The top of the primary display, where AppKit's y is flipped.
    private var primaryTop: CGFloat { NSScreen.screens.first?.frame.maxY ?? 0 }

    /// `rect` (AppKit's global space) as `[left, top, width, height]` in the web's.
    private func flipped(_ rect: NSRect) -> [CGFloat] {
        [rect.minX, primaryTop - rect.maxY, rect.width, rect.height]
    }

    private static func id(_ screen: NSScreen) -> CGDirectDisplayID {
        (screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value ?? 0
    }

    /// The screens in the specification's order, by `left` then `top`.
    private func sorted() -> [NSScreen] {
        NSScreen.screens.sorted { a, b in
            let (fa, fb) = (flipped(a.frame), flipped(b.frame))
            return fa[0] != fb[0] ? fa[0] < fb[0] : fa[1] < fb[1]
        }
    }

    /// The screen holding most of the session's window, else the primary.
    private func current() -> NSScreen? {
        let primary = NSScreen.screens.first
        guard let frame = window?.frame else { return primary }
        let area = { (s: NSScreen) -> CGFloat in
            let i = s.frame.intersection(frame)
            return i.isNull ? 0 : i.width * i.height
        }
        let best = NSScreen.screens.max { area($0) < area($1) }
        return (best.map(area) ?? 0) > 0 ? best : primary
    }

    private func screen(_ s: NSScreen) -> [String: Any] {
        let (r, a) = (flipped(s.frame), flipped(s.visibleFrame))
        return [
            "left": r[0], "top": r[1], "width": r[2], "height": r[3],
            "availLeft": a[0], "availTop": a[1], "availWidth": a[2], "availHeight": a[3],
            "devicePixelRatio": s.backingScaleFactor,
            "isPrimary": NSScreen.screens.first.map(Self.id) == Self.id(s),
            "isInternal": CGDisplayIsBuiltin(Self.id(s)) != 0,
            "label": s.localizedName,
        ]
    }

    private func details() -> [String: Any] {
        let screens = sorted()
        guard let first = screens.first else { return ["error": "NotSupportedError", "message": "no display"] }
        return ["screens": screens.map(screen), "currentScreen": screen(current() ?? first)]
    }

    // MARK: The pointer

    /// The session view's top left in the web's global space, or nil with no window.
    private func viewOrigin() -> CGPoint? {
        guard let view = context.view, let window = view.window else { return nil }
        let inScreen = window.convertToScreen(view.convert(view.bounds, to: nil))
        return CGPoint(x: inScreen.minX, y: primaryTop - inScreen.maxY)
    }

    private func pointer() -> [String: Any] {
        let at = NSEvent.mouseLocation
        let screens = NSScreen.screens
        // The screen holding it; at an edge AppKit's frames are half-open, so the nearest.
        let holder = screens.first { NSMouseInRect(at, $0.frame, false) } ?? screens.min { a, b in
            distance(at, a.frame) < distance(at, b.frame)
        }
        guard let holder else { return ["error": "NotSupportedError", "message": "no display"] }
        let (x, y) = (at.x, primaryTop - at.y)
        var on = screen(holder)
        let origin = viewOrigin()
        if let origin {
            let r = flipped(holder.frame)
            on["clientRect"] = ["x": r[0] - origin.x, "y": r[1] - origin.y, "width": r[2], "height": r[3]]
        } else {
            on["clientRect"] = NSNull()
        }
        return [
            "screenX": x, "screenY": y,
            "clientX": origin.map { x - $0.x } ?? NSNull(),
            "clientY": origin.map { y - $0.y } ?? NSNull(),
            "screen": on,
        ]
    }

    private func distance(_ p: NSPoint, _ r: NSRect) -> CGFloat {
        let dx = max(r.minX - p.x, p.x - r.maxX, 0), dy = max(r.minY - p.y, p.y - r.maxY, 0)
        return dx * dx + dy * dy
    }

    // MARK: Changes

    private func changed() { context.changed("window-management.getScreenDetails") }

    private func watch() {
        if observers.isEmpty {
            observers.append(NotificationCenter.default.addObserver(
                forName: NSApplication.didChangeScreenParametersNotification, object: nil, queue: .main
            ) { [weak self] _ in self?.changed() })
        }
        // The window's screen: a move between two displays at one scale is
        // no display change, so its moves are compared too (D1).
        guard let window, window !== watchedWindow else { return }
        watchedWindow = window
        lastScreen = current().map(Self.id)
        for name in [NSWindow.didChangeScreenNotification, NSWindow.didMoveNotification, NSWindow.didResizeNotification] {
            observers.append(NotificationCenter.default.addObserver(forName: name, object: window, queue: .main) { [weak self] _ in
                guard let self else { return }
                let now = self.current().map(Self.id)
                if now != self.lastScreen {
                    self.lastScreen = now
                    self.changed()
                }
            })
        }
    }

    private func unwatch() {
        for o in observers { NotificationCenter.default.removeObserver(o) }
        observers = []
        watchedWindow = nil
    }
}
