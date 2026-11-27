// @ref LLP 1047.001 D10 — the plan from a URL (LLP 1023; cut list C8): the
// app's one dev connection (LLP 1031 D11; PlanURL.swift), which resolves the
// app URL, fetches and verifies each generation, applies it to every session
// and subscribes to the server's edits. `ExactApp` holds it, and the core
// reaches it only through what this file declares. Its readers are the dev
// pill, the build channel, live design and the inspector. Network plan
// loading is compiled only into dev-capable hosts (LLP 1023 §8): no store
// level of a production build takes a plan from a URL, so a production bake
// compiles neither `PlanURL.swift` nor its live design, and the slot is
// empty there. Without it `connect` is refused on stderr and the app stays
// unconnected.
import Foundation

/// What the dev loop said it is doing before it had a plan to give:
/// preparing (its producers' first build), or its start's error.
package enum DevLoopStatus: Equatable { case preparing(String), failed(String) }

/// What the window's dev pill says, the first that holds (LLP 1159.001 D2):
/// the loop's own status, a host whose native code the server no longer
/// runs, or a save that did not apply.
package enum DevNotice: Equatable { case loop(DevLoopStatus), stale, refused(String), disconnected(String) }

/// What the core asks of the app's dev connection.
package protocol PlanConnection: AnyObject {
    /// The app URL it resolves.
    var page: URL { get }
    /// The build channel's stream (the nonce its hello gave); nil without one.
    var nonce: String? { get }
    /// The plan the build channel's reports name: the generation every
    /// session last accepted, else the running plan's digest.
    var plan: String? { get }
    /// The loop's own status, a host the server no longer runs (`terminal`),
    /// and the first line of a save that did not apply (`refused`).
    var devStatus: DevLoopStatus? { get }
    var terminal: String? { get }
    var refused: String? { get }
    /// Close it: it applies nothing more.
    func close()
    /// Resolve the URL again (the dev menu's Reload).
    func reload()
    /// A build-channel report to the server (LLP 1122 D4).
    func report(_ body: [String: Any])
    /// Fetch the design the server's builds run, and apply it (LLP 1121).
    func syncDesign()
}

/// The slot is set before any session is made, and read on the main thread.
package enum PlanConnectionLink {
    /// The app's connection to a URL, telling the app's dev pills what it
    /// has to say; nil when the URL is no HTTP app URL.
    #if EXACT_INSPECT
    package static var open: ((ExactApp, String) -> (any PlanConnection)?)? = { PlanURL.open(for: $0, $1) }
    #else
    package static var open: ((ExactApp, String) -> (any PlanConnection)?)?
    #endif
}

extension ExactApp {
    /// A fact the build channel reports moved (LLP 1122 D4): its status is
    /// owed. The channel (BuildChannel.swift) is development's alone (LLP
    /// 1047.001 D6), so a production build compiles neither it nor its
    /// report, and here does nothing.
    func buildFactsMoved() {
        #if EXACT_INSPECT
        scheduleStatus()
        #endif
    }

    /// A session booted: the build channel's design, pin and report follow
    /// it; in production, nothing does.
    func buildSessionBooted(_ session: ExactSession) {
        #if EXACT_INSPECT
        liveBooted(session)
        #endif
    }
}
