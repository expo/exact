// The commands an app gives its own window (@ref LLP 1069.010): a quit the
// app holds leaves full screen and fades the window first, as Electron's
// `setFullScreen(false)` and `setOpacity(0)` do for T3 Code's ⌘Q hold; a
// utility that keeps running puts its window away (@ref LLP 1133.025 D4).
// The window's owner (the Mac app's delegate) calls them, as it does `close()`.
#if os(macOS)
import AppKit

public enum ExactWindowCommands {
    /// Run `exitFullscreen()` or `setWindowOpacity(opacity)` on `window`;
    /// false for any other command. A refusal is a `log` line. Called on
    /// the main thread, as the delegate's commands are.
    public static func perform(_ name: String, _ args: [Any], on window: NSWindow, log: (String) -> Void) -> Bool {
        switch name {
        case "exitFullscreen":
            // AppKit animates the window out of its space; a window that is
            // not full screen stays as it is, as the web's call does nothing then.
            if window.styleMask.contains(.fullScreen) { window.toggleFullScreen(nil) }
        case "setWindowOpacity":
            // `alphaValue`: a window at 0 is unseen but stays key and hears keys.
            guard let opacity = (args.first as? NSNumber)?.doubleValue, args.count == 1, opacity.isFinite else {
                log("setWindowOpacity refused: the opacity is a number from 0 to 1")
                return true
            }
            window.alphaValue = CGFloat(min(1, max(0, opacity)))
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
