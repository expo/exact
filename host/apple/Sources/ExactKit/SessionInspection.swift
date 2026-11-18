import CExact
import Foundation

/// An accessibility object standing for one Exact view it is not itself.
/// The platform projection uses this ownership independent of who inspects it.
package protocol InspectionOwned { var inspectionViewId: UInt32? { get } }

public extension ExactEnv {
    /// `EXACT_AGENT_CHROME=platform` (opt-in; splitter rough 4, 11): under
    /// the agent, the native navigation bar and tab bar show as a person
    /// sees them, their items pressing the authored controls they stand for.
    /// The default paints the authored header and tablist in their place.
    static let agentChrome = environment["EXACT_AGENT_CHROME"] ?? "agent"
    /// Whether a driver can reach this launch. On iOS it is named by
    /// `EXACT_AGENT_SOCKET` or `EXACT_AGENT_CONNECT`; the Mac's driver owns
    /// stdio. An iOS launch with `EXACT_AGENT=1` and neither has no driver to
    /// read painted chrome, so it shows what a person sees.
    #if os(iOS) || os(tvOS)
    static let agentDriven = agentMode && (environment["EXACT_AGENT_SOCKET"] != nil || environment["EXACT_AGENT_CONNECT"] != nil)
    #else
    static let agentDriven = agentMode
    #endif
    /// Whether the authored header and tablist paint instead of UIKit's
    /// bars: the agent's default chrome.
    static let authoredChrome = agentDriven && agentChrome != "platform"
    /// Whether the agent paints the popovers a person sees as UIKit's
    /// menus (LLP 1021 D4). `EXACT_AGENT_PRESENTATION=native`, which
    /// `--touch platform` launches with (LLP 1080.000 D7), shows UIKit's:
    /// a real touch opens one and picks its item.
    static let agentPaints = agentDriven && environment["EXACT_AGENT_PRESENTATION"] != "native"
}

extension ExactSession {
    #if EXACT_AGENT
    /// The runner's inspection API, in development builds only, as the
    /// dispatcher it reaches is (LLP 1047.001 D11).
    public func agent(_ request: String) -> String { runtime.ask(exact_agent, request) }
    #endif
    /// A line for the runner's journal (LLP 1012 §3; LLP 1035.001 D6).
    public func log(_ line: String) { runtime.log(line) }
    /// The number of live views the presenter holds.
    public var viewCount: Int { presenter.views.count }
    /// The first root's frame size.
    public var rootSize: CGSize { presenter.rootSize }
    /// The viewport's size in points.
    public var viewportSize: CGSize { presenter.viewportSize }
    /// The text engine's counters since this session started.
    public var measureCount: Int { Owner.shared.sync { text.measuring.measureCount } }
    public var measureHits: Int { Owner.shared.sync { text.measuring.measureHits } }
    public var measureSeconds: Double { Owner.shared.sync { text.measuring.measureSeconds } }
    /// The GPU module's and the web arm's status lines.
    public var gpuStatus: String { canvases.status }
    public var webStatus: String { webviews.status }
    /// The window's occlusion changed (macOS): the canvases follow.
    public func occlusionChanged() {
        #if os(macOS)
        canvases.occlusionChanged()
        #endif
        frames.run(frames.hasWork)
    }
    #if os(iOS) || os(tvOS)
    /// A hardware keyboard's Tab when no node of this session holds focus.
    public func moveFocus(backward: Bool) { presenter.moveFocus(backward: backward) }
    #endif
}
