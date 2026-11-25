import Foundation
import QuartzCore
import CExact
import ExactCore

#if os(iOS) || os(tvOS)
extension ExactSession {
    /// The initial view needs its layout policy before resources settle.
    var launchCoversViewport: Bool {
        if let cached = launchViewport, cached.plan == app.lastPlan { return cached.cover }
        let cover = runtime.on(busy: false) {
            let length = app.lastPlan.map { runtime.write($0) } ?? 0
            return exact_plan_covers_viewport(runtime.rt, length) != 0
        }
        launchViewport = (app.lastPlan, cover)
        return cover
    }
}
#endif

extension ExactSession {
    /// The location of the visit beneath the route keyed `key` on its stack,
    /// nil for a stack's root (`Runtime.locationBeneath`). For the router's
    /// top it is what the batch that made it the top carried
    /// (`Batch.beneath`), so main does not wait on the owner to learn
    /// whether Back goes (LLP 1072 T2).
    package func locationBeneath(_ key: String) -> String? {
        if let heard = routerBeneath, UInt64(key) == heard.visit { return heard.location.isEmpty ? nil : heard.location }
        return runtime.locationBeneath(key)
    }

    /// Releases completed background actions and tells launch and navigation TTI to read outstanding work
    /// again. Work that shows nothing new can end the last of it (a skipped
    /// batch, owner work landing, a canvas replay or an SVG picture done), so
    /// this follows every end of work, not only an apply.
    func workEnded() {
        workEndedAt = CACurrentMediaTime()
        notificationHost.backgroundActionsApplied(self)
        ExactLaunch.shared.workEnded(self)
        NavigationMarks.shared.workEnded(self)
    }
}

/// Supplied once per session launch and reused by every replacement runner.
struct LaunchPlace: Equatable {
    let locale: String
    let timeZone: String
    let seed: UInt64
    /// The Region's format with a forced hour cycle (`SessionFacts.currentFormatLocale`), else
    /// empty (the locale); read again when either changes (`formatLocaleChanged`).
    var formatLocale: String
    /// Under the agent, the Unix milliseconds at the clock's zero (LLP
    /// 1027.000.000 D3; default 2026-01-01T00:00:00Z); nil reads the machine.
    let epoch: Double?
    init(environment: [String: String] = ExactEnv.environment) {
        if environment["EXACT_AGENT"] == "1" {
            locale = environment["EXACT_AGENT_LOCALE"] ?? "en-US"
            timeZone = environment["EXACT_AGENT_TIME_ZONE"] ?? "UTC"
            seed = UInt32(environment["EXACT_AGENT_SEED"] ?? "1").map(UInt64.init) ?? 1 // below 2^32 (LLP 1116.005 D5); else 1, as exact_data::crypto takes it
            formatLocale = environment["EXACT_AGENT_FORMAT_LOCALE"] ?? ""
            epoch = Double(environment["EXACT_AGENT_EPOCH"] ?? "1767225600000") ?? 1_767_225_600_000
        } else {
            epoch = nil
            locale = SessionFacts.viewerLocale()
            timeZone = TimeZone.current.identifier
            seed = UInt64(UInt32.random(in: .min ... .max))
            formatLocale = SessionFacts.currentFormatLocale
        }
    }
}
