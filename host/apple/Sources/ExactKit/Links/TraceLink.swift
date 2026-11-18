// @ref LLP 1047.001 D10 — traces an app records (`startTrace_EXPERIMENTAL`,
// `stopTrace_EXPERIMENTAL`; TraceRecording.swift), roster `traces`, linked by
// a plan that calls them or a manifest that names them in `link`. The
// session's command dispatch reaches them only through this slot; without
// them a trace command is refused into the journal, as one that finds no
// recording is. Frame health's sampler records the frames (FrameSampler.swift,
// in every build, LLP 1079.000); the family is the commands and the report.
// The slot's default is today's.

/// The slot is set before any session is made, and read on the main thread.
package enum TraceLink {
    /// `startTrace_EXPERIMENTAL()` or `stopTrace_EXPERIMENTAL([maxBytes])`.
    package static var command: ((ExactSession, String, [Any]) -> Void)? = { $0.traceCommand($1, $2) }
}

extension ExactSession {
    /// A trace command from a batch.
    func trace(_ name: String, _ args: [Any]) {
        guard let command = TraceLink.command else { return log("\(name) refused: this build links no traces") }
        command(self, name, args)
    }
}
