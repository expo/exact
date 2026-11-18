// @ref LLP 1047.001 D10 — the binding core (LLP 1121 D12; Forma.swift): a
// design token resolved for a place, for code outside the plan. D6 counts it
// inspection code, development-only; a host screen or an embedder that calls
// the public `ExactForma` links it, and the native module's hatches answer a
// module's bindings through it. No roster row names it yet. The session
// tells it after each batch, only through this slot; without it nothing is
// asked again after a batch, and no observer hears a move. The slot's
// default is today's.

/// The slot is set before any session is made, and read on the main thread.
package enum FormaLink {
    /// After a batch: each place whose inputs moved is asked again, redrawn,
    /// and its observers told.
    package static var moved: ((ExactSession) -> Void)? = { Formas.moved($0) }
}
