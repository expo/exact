import Foundation

/// Prepare every participant before deciding whether the generation can commit.
/// Native image loads must start together, including sessions after a waiter.
func prepareGeneration<Session, Candidate>(_ sessions: [Session], prepare: (Session) -> Candidate?) -> [(Session, Candidate)]? {
    var candidates: [(Session, Candidate)] = []
    var ready = true
    for session in sessions {
        if let candidate = prepare(session) { candidates.append((session, candidate)) }
        else { ready = false }
    }
    return ready ? candidates : nil
}

/// The session generation that made a platform request (an authentication
/// sheet, a file panel, an open, a tool call, a frame's queued event). A
/// reload or plan replacement keeps the session but starts a new runner,
/// which numbers its calls and nodes from the start again, so an answer
/// that arrives after it would reach an unrelated call or node by the same
/// number. Delivery goes through `session`, which is nil once that
/// generation has ended or the session was destroyed. Main thread.
struct GenerationStamp {
    private weak var owner: ExactSession?
    let generation: Int
    init(_ session: ExactSession) { owner = session; generation = session.generation }
    /// The session, while it is alive and still runs the stamped generation.
    var session: ExactSession? {
        guard let owner, owner.state != .destroyed, owner.generation == generation else { return nil }
        return owner
    }
}
