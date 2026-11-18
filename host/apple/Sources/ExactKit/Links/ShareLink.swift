// @ref LLP 1047.001 D10 — sharing: `share(…)`, the system share sheet (LLP
// 1069.003, roster `share`), and a received share's claim from the share
// extension's inbox (LLP 1133.024). The core reaches both only through what
// this file declares. The two are selected by different inputs: sending by
// the plan's `share` command, receiving by the manifest's `share_target`,
// which no roster row names yet; the family's move adds that composition
// input, or leaves receiving in the core. Without the family a `share` is
// refused into the journal, which is all its outcome ever is (D2), and no
// share is received. The context menu's Share item presents `ShareSheet`
// directly; it belongs to the menus' move. The slot's default is today's.

/// What a session asks of sharing; one object serves every session.
package protocol ShareHosting: AnyObject {
    /// `share(title, text, url, files)`, from the input on `source`.
    func share(_ session: ExactSession, _ args: [Any], source: UInt32?)
    /// Before the boot: a received share becomes the launch location.
    func claimCold(_ session: ExactSession)
    /// Once storage is configured: the cold share's files land, and the
    /// inbox is claimed from now on.
    func land(_ session: ExactSession)
    #if EXACT_AGENT
    // What the agent asks: development only, so production strips them.
    /// The agent answered a share's hold: its real sheet closes.
    func answered(_ session: ExactSession, _ reply: [String: Any])
    /// The driver's `share:` form, delivered at the navigation root.
    func receive(_ session: ExactSession, _ request: [String: Any]) -> [String: Any]
    #if os(iOS)
    /// `back` with a real sheet up: the newest closes; false with none.
    func agentBack(_ session: ExactSession) -> Bool
    /// Whether a real sheet has yet to finish arriving.
    func arriving(_ session: ExactSession) -> Bool
    #endif
    #endif
}

/// No sharing: a `share` is refused, and nothing is received.
package final class NoShare: ShareHosting {
    package init() {}
    package func share(_ session: ExactSession, _ args: [Any], source: UInt32?) {
        session.log("share: refused: this build links no share sheet")
    }
    package func claimCold(_ session: ExactSession) {}
    package func land(_ session: ExactSession) {}
    #if EXACT_AGENT
    package func answered(_ session: ExactSession, _ reply: [String: Any]) {}
    package func receive(_ session: ExactSession, _ request: [String: Any]) -> [String: Any] {
        ["error": "share: this build links no share target"]
    }
    #if os(iOS)
    package func agentBack(_ session: ExactSession) -> Bool { false }
    package func arriving(_ session: ExactSession) -> Bool { false }
    #endif
    #endif
}

/// The slot is set before any session is made, and read on the main thread.
package enum ShareLink {
    package static var installed: (any ShareHosting)? = BuiltInShare()
}

extension ExactSession {
    /// Sharing, or none.
    var shareHost: any ShareHosting { ShareLink.installed ?? NoShare() }
}
