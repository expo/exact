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
        if !deferSessionResize(frameRect, display: flag, animate: animateFlag) {
            super.setFrame(frameRect, display: flag, animate: animateFlag)
        }
    }
}

/// Float’s keyboard shortcuts use AppKit’s nonactivating HUD (LLP 1030.004 D7).
package final class RoutePanel: NSPanel {
    private let drivenBehind: Bool

    package init(contentRect: NSRect, behind: Bool) {
        drivenBehind = behind
        super.init(contentRect: contentRect, styleMask: [.titled, .closable, .resizable, .utilityWindow, .hudWindow, .nonactivatingPanel], backing: .buffered, defer: false)
        isFloatingPanel = true
        requestedFloating = true
        level = behind ? .agentBehind : .floating
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .canJoinAllApplications]
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = true
        isExcludedFromWindowsMenu = true
        tabbingMode = .disallowed
        isReleasedWhenClosed = false
    }

    // Releasing the session also removes its toolbar. Save the frame with the
    // toolbar still attached so AppKit restores the same content size.
    package override func close() {
        let name = frameAutosaveName
        if !name.isEmpty {
            saveFrame(usingName: name)
            setFrameAutosaveName("")
        }
        super.close()
    }

    package override var canBecomeMain: Bool { false }
    // A nonactivating panel can take the real keyboard even while another
    // app is frontmost. The driver can edit its first responder without it.
    package override var canBecomeKey: Bool { !drivenBehind }

    #if EXACT_AGENT
    package override var isKeyWindow: Bool { (!NSApp.isActive && self === ExactAgentWindow.pointerEvent?.window) || super.isKeyWindow }
    package override var occlusionState: NSWindow.OcclusionState { drivenOcclusion(super.occlusionState) }
    package var physicalOcclusionState: NSWindow.OcclusionState { super.occlusionState }
    #endif

    package override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown, event.keyCode == 53, event.window === self,
           (firstResponder as? NSTextInputClient)?.hasMarkedText() != true,
           event.modifierFlags.intersection([.shift, .option, .control, .command]).isEmpty {
            performClose(nil)
        } else {
            super.sendEvent(event)
        }
    }

    package override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        ExactEnv.agentMode ? frameRect : super.constrainFrameRect(frameRect, to: screen)
    }

    package override func setFrame(_ frameRect: NSRect, display flag: Bool, animate animateFlag: Bool) {
        if !deferSessionResize(frameRect, display: flag, animate: animateFlag) {
            super.setFrame(frameRect, display: flag, animate: animateFlag)
        }
    }
}

#if EXACT_AGENT
extension RoutePanel: DrivenOcclusion {}
#endif

extension NSWindow {
    fileprivate func deferSessionResize(_ rect: NSRect, display: Bool, animate: Bool) -> Bool {
        guard animate, isVisible, Owner.shared.isServingMain else { return false }
        let asked = frame, ticket = placement.begin(on: self)
        DispatchQueue.main.async { [weak self] in
            guard let self, placement.isCurrent(ticket), frame == asked else { return }
            setFrame(rect, display: display, animate: true)
        }
        return true
    }

    public internal(set) var requestedFloating: Bool {
        get { placement.requestedFloating }
        set { placement.requestedFloating = newValue }
    }
}
#endif
