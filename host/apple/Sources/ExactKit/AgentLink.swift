import Foundation
#if os(macOS)
import AppKit
#endif

/// The development composition's agent carrier. A production bake installs
/// no hooks and compiles no `Agent` implementation (LLP 1047.001 D6,
/// superseded question 7; LLP 1069.007 D2).
public enum AgentLink {
    package struct Hooks {
        let prepareStdio: () -> Void
        let startStdio: ([(String, ExactSession)]) -> Void
        let startSocket: ([String: Any], [(String, ExactSession)]) -> Void
        let reply: ([String: Any]) -> Void
        let route: (String, ExactSession?) -> Void
        let windowsBehind: () -> Bool
        let setHostState: ((() -> [String: Any])?) -> Void
        let layout: (ExactSession) -> [String: Any]
    }

    nonisolated(unsafe) package static var hooks: Hooks?

    /// Install the hooks before an adapter reads one. In production this is
    /// deliberately a no-op, so no reference reaches the implementation.
    public static func install() {
        #if EXACT_AGENT
        Agent.installLink()
        #endif
    }

    public static func prepareStdio() { hooks?.prepareStdio() }
    public static func startStdio(sessions: [(String, ExactSession)]) { hooks?.startStdio(sessions) }
    public static func startSocket(ready: [String: Any], sessions: [(String, ExactSession)]) { hooks?.startSocket(ready, sessions) }
    public static func reply(_ value: [String: Any]) { hooks?.reply(value) }
    public static func route(_ label: String, _ session: ExactSession?) { hooks?.route(label, session) }
    public static var windowsBehind: Bool {
        install()
        return hooks?.windowsBehind() ?? false
    }
    package static func layout(_ session: ExactSession) -> [String: Any] {
        install()
        return hooks?.layout(session) ?? ["available": false, "reason": "native layout inspection is not linked in a production composition"]
    }
    public static var hostState: (() -> [String: Any])? {
        didSet { hooks?.setHostState(hostState) }
    }
}

#if os(macOS)
extension NSWindow.Level {
    /// A driven window's level, below every other app's ordinary windows.
    public static let agentBehind = NSWindow.Level(rawValue: NSWindow.Level.normal.rawValue - 1)
}

#if EXACT_AGENT
/// Exact's own windows keep WebKit live at the driver's reserved level in a
/// development composition. Production compiles this name as plain AppKit.
public typealias ExactDrivenWindow = ExactAgentWindow
#else
public typealias ExactDrivenWindow = ExactSessionWindow
#endif

extension NSWindow {
    /// Rendering and frame measurements use physical visibility even when a
    /// development drive tells WebKit that its covered window is visible.
    public var screenOcclusionState: NSWindow.OcclusionState {
        #if EXACT_AGENT
        return (self as? any DrivenOcclusion)?.physicalOcclusionState ?? occlusionState
        #else
        return occlusionState
        #endif
    }
}
#endif
