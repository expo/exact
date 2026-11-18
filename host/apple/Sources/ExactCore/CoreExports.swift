// @ref LLP 1047.001 D10 — the C exports a family owns that ExactCore calls.
// ExactCore reaches each through a slot here, never by name, so an app
// without the family links no call to an export its archive does not emit
// (D3). A family's target sets its slots from its installer; a nil slot
// answers what the family's absence means, written at each call site.
import CExact

package enum CoreExports {
    /// Which SVG modules a plan can need (bit 1: islands, bit 2: filters);
    /// absent, none.
    package static var svgIslands: ((ExactRuntime) -> UInt8)?
    /// A windowed list's feedback (its fills and reports); absent, the
    /// session sends none, and a call answers `Runtime.noFeedback`.
    package static var collectionFeedback: ((ExactRuntime, Int, Double) -> UInt32)?
    /// What UIKit covers of a list's port; absent, not told.
    package static var listCovered: ((ExactRuntime, UInt32, Double, Double) -> Void)?
    /// A row's index by key; absent, none.
    package static var listIndex: ((ExactRuntime, UInt32, UInt32) -> UInt32)?
    /// A list's text between two positions; absent, none.
    package static var listText: ((ExactRuntime, UInt32, UInt32, UInt32, UInt32, UInt32, UInt32, UInt32, UInt32, UInt32) -> UInt32)?
    /// A list's fixed lead or trail size, its snap position and its snap
    /// stop; absent, none (not finite).
    package static var collectionFixedSize: ((ExactRuntime, UInt32, UInt32, UInt32, UInt32) -> Double)?
    package static var collectionSnap: ((ExactRuntime, UInt32, Double, Double, Double, Double, Double) -> Double)?
    package static var collectionSnapStop: ((ExactRuntime, UInt32, Double, Double, Double, Double, Double) -> Double)?
    /// The events native extensions raised (LLP 1133.004 D6), notification
    /// actions among them; absent, the session hears none and does not ask.
    package static var moduleEvent: ((ExactRuntime, Int) -> UInt32)? = exact_module_event
    /// The activity arm's link; absent, the runtime links none.
    package static var linkActivities: ((ExactRuntime) -> Void)? = exact_link_activities
    /// The activity arm's snapshots and send completions; absent, no activity
    /// output exists to ask.
    package static var activities: ((ExactRuntime, Int) -> UInt32)? = exact_activities
}

extension Runtime {
    /// What a list's feedback answers when no list family is linked. Applied,
    /// it would clear running motion and timers, so the session does not ask.
    static let noFeedback = Batch(ops: [], timers: false, motion: false, clock: nil, error: nil)
}
