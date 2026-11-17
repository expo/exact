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

/// A platform request's weak session owner and the runtime lease it asked
/// under. Validation shares the posted-job door (LLP 1172.005 E11).
struct SessionLease {
    private weak var owner: ExactSession?
    private let runtime: RuntimeLease
    var generation: Int { runtime.generation }
    init(_ session: ExactSession) { owner = session; runtime = session.runtime.completionLease }
    var session: ExactSession? {
        guard let owner, owner.runtime.accepts(runtime) else { return nil }
        return owner
    }
}
