#if os(macOS)
import AppKit
import ExactCore

/// The window Exact's own hosts give a session: the app's document windows,
/// their sheets and the sample host's (`ExactDrivenWindow`).
///
/// An animated frame change asked while main serves a call the owner waits on
/// (a module's `native.call`) runs on the next main-queue turn. AppKit steps
/// that animation synchronously, and inside the call the page cannot lay out
/// (LLP 1072 T5), so each step would show the old layout until the call
/// returned. On the next turn each step lays out as a person's live resize
/// does. The window's `frame` is the old one until then, as after
/// `animator().setFrame(_:display:)`, and a frame change made in between, by
/// any means, wins. A window that is not on screen shows no steps and takes
/// its frame at once.
open class ExactSessionWindow: NSWindow {
    open override func setFrame(_ frameRect: NSRect, display flag: Bool, animate animateFlag: Bool) {
        guard animateFlag, isVisible, Owner.shared.isServingMain else {
            return super.setFrame(frameRect, display: flag, animate: animateFlag)
        }
        let asked = frame
        DispatchQueue.main.async { [weak self] in
            guard let self, frame == asked else { return }
            setFrame(frameRect, display: flag, animate: true)
        }
    }
}
#endif
