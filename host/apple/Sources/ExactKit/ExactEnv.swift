// The process facts every session reads (`ExactEnv`).
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif
import Foundation
import QuartzCore
/// The process facts every session reads: the agent drives the app
/// (LLP 1012 — the driver owns the clock), a smoke run prints and exits.
public enum ExactEnv {
    /// The process's environment. A production bake never enters agent mode
    /// (LLP 1069.007 D2, ruled) and takes no module or plan override (LLP 1024
    /// D5, LLP 1047.001 §9.13): its `EXACT_AGENT`, `EXACT_AGENT_*`,
    /// `EXACT_HATCHES`, `EXACT_MODULES`, `EXACT_PLAN` and `EXACT_DEV_PLAN`
    /// variables are dropped here, from the process too, before this host or
    /// its library reads one.
    public static let environment: [String: String] = sanitize(ProcessInfo.processInfo.environment, production: productionBake)
    static package func sanitize(_ raw: [String: String], production: Bool, unset: (String) -> Void = { unsetenv($0) }) -> [String: String] {
        var environment = raw
        let dropped = environment.keys.filter {
            ["EXACT_AGENT", "EXACT_HATCHES", "EXACT_MODULES", "EXACT_PLAN", "EXACT_DEV_PLAN"].contains($0)
                || $0.hasPrefix("EXACT_AGENT_")
        }.sorted()
        guard !dropped.isEmpty, production else { return environment }
        FileHandle.standardError.write(Data("exact: a production build ignores \(dropped.joined(separator: ", ")) (LLP 1069.007 D2, LLP 1024 D5)\n".utf8))
        for key in dropped { environment[key] = nil; unset(key) }
        return environment
    }
    /// Whether this binary was baked with `EXACT_UPDATE_TRUST=production` (`BakedCompatibility.production`).
    static package let productionBake = BakedCompatibility.production(BakedCompatibility.json)
    public static let agentMode = environment["EXACT_AGENT"] == "1"
    /// `EXACT_AGENT=live` (the driver's `--live`): an ordinary launch on the
    /// wall clock that the driver can still reach, which sends input and
    /// reads but never holds the clock (`Agent.clock` refuses it).
    public static let agentLive = environment["EXACT_AGENT"] == "live"
    /// Whether the launch serves the driver: agent mode or `--live`.
    public static let agentCarried = agentMode || agentLive
    /// `EXACT_AGENT_BIND` (`--bind`, LLP 1133.001 D7): extensions answered by their implementations under the agent.
    static package let agentBind = Set((environment["EXACT_AGENT_BIND"] ?? "").split(separator: ",").map(String.init))
    /// `EXACT_AGENT_TIMING=platform` (LLP 1035.003 D5, opt-in): under the
    /// agent carrier, UIKit's own transitions, presentations and keyboard
    /// animations keep their natural timing — the ordinary app with a
    /// socket, for observing a gesture's native motion. The default freezes
    /// them, which the smoke depends on.
    public static let agentTiming = environment["EXACT_AGENT_TIMING"] ?? "agent"
    /// Whether the agent's world is settled between calls: native animation applies at once. False under `platform` timing.
    public static let agentFreezes = agentMode && agentTiming != "platform"
    static let postsLiveWork = !agentFreezes && environment["EXACT_FILL_SYNC"] != "1"
    public static let smoke = environment["EXACT_SMOKE"] == "1"
    /// Milliseconds since the process's `main`: the session's clock and the
    /// startup stamps'. It starts at its first read, which an adapter makes
    /// the first statement of its `main`.
    public static let t0 = CACurrentMediaTime()
    public static func wall() -> Double { (CACurrentMediaTime() - t0) * 1000 }
    /// Startup stamps, milliseconds from `main`, in order (the smoke prints
    /// them). Without `EXACT_SMOKE` a stamp reads no clock and builds no label.
    nonisolated(unsafe) public static var stamps: [(String, Double)] = []
    public static func stamp(_ label: @autoclosure () -> String) {
        guard smoke else { return }
        stamps.append((label(), wall()))
    }
}
