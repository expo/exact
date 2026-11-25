// @ref LLP 1047.001 D10 — CSS animations (LLP 1055 D7, LLP 1055.001) are a
// family the composition selects. The host lowers a box's CSS animations,
// its transitions and its drag and motion tracks to one `animations` op, so
// a composition that links any of the roster's `animations`, `motion`,
// `drag`, `timelines` or `transitions` links this family (Package.swift's
// `played`). The core reaches the specs' reading and playing only through
// what this file declares. `ExactAnimations.install()` sets the slots;
// without it no box animates, and a plan that moves is refused at boot
// (`Unlinked(...)`). The SVG family plays its elements' animations with the
// same code, so it links this one.
import QuartzCore

/// What the core asks of a presenter's box animations.
package protocol BoxAnimating: AnyObject {
    /// `id`'s `animations` op, on `layer`.
    func animations(_ id: UInt32, _ payload: [String: Any], layer: CALayer?, clock: Double?)
    /// `visibility` hid or showed `id`'s box.
    func boxVisibility(_ id: UInt32, hidden: Bool, clock: Double?)
    /// An agent moved the clock.
    func seek(clock: Double?)
    /// Whether `id`'s view holds CSS animations.
    func animatesBox(_ id: UInt32) -> Bool
    /// Whether `id`'s layer plays transform animations (none while it flies).
    func playsTransform(_ id: UInt32, flying: Bool, clock: Double?) -> Bool
    func forget(_ id: UInt32)
    /// `id`'s box paints its background colour on `fill` (nil for its own layer), as a box's extension holds its
    /// picture (LLP 1123 D13): its background-colour animations play there.
    func setFill(_ id: UInt32, _ fill: CALayer?, clock: Double?)
}

/// No CSS animations: no box animates.
package final class NoBoxAnimations: BoxAnimating {
    package init() {}
    package func animations(_ id: UInt32, _ payload: [String: Any], layer: CALayer?, clock: Double?) {}
    package func setFill(_ id: UInt32, _ fill: CALayer?, clock: Double?) {}
    package func boxVisibility(_ id: UInt32, hidden: Bool, clock: Double?) {}
    package func seek(clock: Double?) {}
    package func animatesBox(_ id: UInt32) -> Bool { false }
    package func playsTransform(_ id: UInt32, flying: Bool, clock: Double?) -> Bool { false }
    package func forget(_ id: UInt32) {}
}

/// The slots are set before any session is made; `read` is called on the
/// owner as a batch is decoded, the others on the main thread.
package enum AnimationLink {
    /// A presenter's box animations.
    package static var make: (() -> any BoxAnimating)?
    /// An `animations` op's specs read once, off main, as the batch is
    /// decoded (`Batch.prepare`).
    package static var read: (([[String: Any]]) -> Any)?
}
