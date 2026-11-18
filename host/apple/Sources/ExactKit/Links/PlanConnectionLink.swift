// @ref LLP 1047.001 D10 — the plan from a URL (LLP 1023; cut list C8): the
// app's one dev connection (LLP 1031 D11; PlanURL.swift), which resolves the
// app URL, fetches and verifies each generation, applies it to every session
// and subscribes to the server's edits. `ExactApp` holds it, and the core
// reaches it only through what this file declares. Its readers are the dev
// pill, the build channel, live design and the inspector. Only development
// paths connect: the host executables' (`EXACT_DEV_PLAN` naming a URL, a
// development link, `ExactDevelopmentLink`) and the dev menus'. Its move
// selects it in development, where every family links (D6), and settles
// which production store levels (D2) need it. Without it `connect` is
// refused on stderr and the app stays unconnected. The slot's default is
// today's.
import Foundation

/// What the dev loop said it is doing before it had a plan to give:
/// preparing (its producers' first build), or its start's error.
package enum DevLoopStatus: Equatable { case preparing(String), failed(String) }

/// What the window's dev pill says, the first that holds (LLP 1159.001 D2):
/// the loop's own status, a host whose native code the server no longer
/// runs, or a save that did not apply.
package enum DevNotice: Equatable { case loop(DevLoopStatus), stale, refused(String) }

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
    package static var open: ((ExactApp, String) -> (any PlanConnection)?)? = { PlanURL.open(for: $0, $1) }
}
