// What the dev connection has to say that the page cannot (dtool2's pass 3
// #8; llp/1023.001's dev status; LLP 1159.001): a small pill in the window's
// bottom corner. "Dev loop: building the module producer …" while a native
// client's first join waits on the loop's producers, or the error their
// start ended with; then, with the last good plan still on screen, a save
// that did not build, or that this binary's native code is behind the
// server's. Dev only (a connection to a dev URL; never in a release build),
// drawn by the host outside the page and the kernel's tree, never under the
// agent or with EXACT_DEV_PILL=0, and gone when there is nothing left to say.
#if os(macOS)
import AppKit
#else
import UIKit
#endif

#if os(macOS)
/// A label clicks pass through: the page under it keeps them.
private final class DevPillLabel: NSTextField {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
#else
private final class DevPillLabel: UILabel {}
#endif

extension ExactEnv {
    /// `EXACT_DEV_PILL=0`: a development launch draws no dev pill, for a capture that should show the page alone, such
    /// as a simulator screenshot whose emulated clock overruns turn budgets a device keeps. Only the drawing goes: the
    /// status, the budget warning, its console line and every other dev behaviour are as without it. Settable for tests.
    nonisolated(unsafe) static package var devPill = environment["EXACT_DEV_PILL"] != "0"
}

extension Presenter {
    /// Show `text` in the pill, or take the pill away (nil). Kept on the app
    /// (`ExactApp.devStatusText`), so a window that opens or a view that
    /// moves meanwhile shows it too (`ExactView`'s window callbacks).
    func showDevStatus(_ text: String?, expanded: Bool = false) {
        guard !ExactEnv.agentMode, !ExactEnv.releaseBuild, ExactEnv.devPill, let text, let window = viewport.window else {
            devPill?.removeFromSuperview()
            devPill = nil
            return
        }
        #if os(macOS)
        let pill = (devPill as? DevPillLabel) ?? DevPillLabel(labelWithString: "")
        pill.stringValue = text
        pill.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        pill.textColor = .white
        pill.drawsBackground = true
        pill.backgroundColor = NSColor.black.withAlphaComponent(0.72)
        pill.wantsLayer = true
        pill.layer?.cornerRadius = 6
        pill.lineBreakMode = expanded ? .byWordWrapping : .byTruncatingTail
        pill.maximumNumberOfLines = expanded ? 0 : 1
        pill.preferredMaxLayoutWidth = expanded ? max(40, (window.contentView?.bounds.width ?? 64) - 40) : 0
        pill.setAccessibilityElement(false)
        guard let host = window.contentView else { return }
        // One pill a window: a second session in it (a split's pane) shows none.
        if host.subviews.contains(where: { $0 is DevPillLabel && $0 !== devPill }) { devPill?.removeFromSuperview(); devPill = nil; return }
        if pill.superview !== host { host.addSubview(pill, positioned: .above, relativeTo: nil) }
        let size = pill.intrinsicContentSize
        let width = min(size.width + 16, max(host.bounds.width - 24, 40))
        pill.frame = NSRect(x: host.bounds.maxX - width - 12, y: 12, width: width, height: size.height + 8)
        pill.autoresizingMask = [.minXMargin, .maxYMargin]
        #else
        if window.subviews.contains(where: { $0 is DevPillLabel && $0 !== devPill }) { devPill?.removeFromSuperview(); devPill = nil; return }
        let pill = (devPill as? DevPillLabel) ?? DevPillLabel()
        pill.text = "  \(text)  "
        pill.font = .preferredFont(forTextStyle: .caption1)
        pill.textColor = .white
        pill.backgroundColor = UIColor.black.withAlphaComponent(0.72)
        pill.layer.cornerRadius = 8
        pill.clipsToBounds = true
        pill.numberOfLines = expanded ? 0 : 2
        pill.isAccessibilityElement = false
        pill.isUserInteractionEnabled = false
        if pill.superview !== window { window.addSubview(pill) }
        window.bringSubviewToFront(pill)
        let inset = window.safeAreaInsets, room = window.bounds.width - inset.left - inset.right - 24
        let size = pill.sizeThatFits(CGSize(width: room, height: .greatestFiniteMagnitude))
        let width = min(size.width, room)
        pill.frame = CGRect(x: window.bounds.maxX - inset.right - 12 - width, y: window.bounds.maxY - inset.bottom - 12 - size.height - 8,
                            width: width, height: size.height + 8)
        pill.autoresizingMask = [.flexibleLeftMargin, .flexibleTopMargin]
        #endif
        devPill = pill
    }
}

extension DevLoopStatus {
    /// As the dev menu's `url:` line says it.
    var said: String {
        switch self {
        case .preparing(let message): "dev loop: \(message)"
        case .failed(let message): "dev loop failed to start: \(message)"
        }
    }
}

extension ExactSession {
    func showBudgetWarning(_ warning: String?) {
        guard !ExactEnv.releaseBuild, budgetWarning != warning else { return }
        budgetWarning = warning
        app.refreshDevPills()
    }
}

extension ExactApp {
    /// The dev loop's status the windows show, or nil (one app, one status).
    nonisolated(unsafe) static var devStatusText: String?

    /// What the dev connection has to say in every window, or nothing.
    package func showDevStatus(_ notice: DevNotice?) {
        let text: String? = switch notice {
        case .loop(.preparing(let message)): "Dev loop: \(message)…"
        case .loop(.failed(let message)): "Dev loop failed to start: \(message)"
        case .stale: "Rebuild this app: its native code changed"
        case .refused(let message): "Edit not applied: \(message)"
        case nil: nil
        }
        Self.devStatusText = text
        refreshDevPills()
    }

    /// Each window's pill as the status is now: a view that left a window
    /// takes its pill away first, so a session still in that window can take it.
    func refreshDevPills() {
        let live = sessions.filter { $0.state != .destroyed }
        for session in live where session.presenter.viewport.window == nil { session.presenter.showDevStatus(nil) }
        for session in live {
            session.presenter.showDevStatus(Self.devStatusText ?? session.budgetWarning, expanded: Self.devStatusText == nil)
        }
    }
}
