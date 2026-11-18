import Foundation

/// A session's hatch diagnostics store (HatchDiagnostics.swift; LLP
/// 1075.003.000.001 §3.1–3.3): what the core asks of it.
package protocol HatchDiagnosing: AnyObject {
    func attach(_ session: ExactSession?)
    func timed<T>(_ scope: String, _ moment: String, site: Int?, counts: Bool, _ body: () -> T) -> T
    func ended(node: UInt32)
    func reset()
    /// A late frame's hatch calls in `(from, to]`, seconds on the media clock.
    func window(from: Double, to: Double) -> [String: Any]?
    /// `state.hatches` around `words`, as ElementHatches counts them.
    func state(words: [String: Any]) -> [String: Any]
    /// `perf hatches` for `session`, with the runner's `perf` tags.
    func perf(of session: ExactSession) -> [String: Any]
}

/// Hatch diagnostics are development-only, as `perf` is (LLP 1079 D1). A
/// production composition compiles no store (LLP 1047.001 D11): a session
/// has none, its hatch calls go untimed, and the module's table has no
/// diagnostics entry.
package enum HatchDiagnosticsLink {
    /// Whether this binary keeps diagnostics: a development composition, and
    /// not a production bake.
    package static let available: Bool = {
        #if EXACT_AGENT
        return !ExactEnv.productionBake
        #else
        return false
        #endif
    }()

    /// Whether hatch calls are timed now (main thread): from boot where
    /// frames are sampled from boot, else from `measurePerf`.
    nonisolated(unsafe) static package var measuring = available && FrameSampler.atBoot

    /// A new session's store, or nil in a production composition.
    static package func store() -> (any HatchDiagnosing)? {
        #if EXACT_AGENT
        return HatchDiagnostics()
        #else
        return nil
        #endif
    }

    /// The diagnostics' call table that the host's hatch table names, or nil
    /// where this binary keeps none.
    static package var table: UnsafeRawPointer? {
        #if EXACT_AGENT
        return available ? HatchDiagnostics.table : nil
        #else
        return nil
        #endif
    }
}
