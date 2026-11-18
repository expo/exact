// @ref LLP 1047.001 D10 — `wakelock="screen"` (LLP 1125 D11; WakeLock.swift)
// is a family: the core reaches the process's lock only through this slot.
// No roster row names it yet; its move adds one, by the `wakelock`
// property a plan binds. Without it no batch holds the lock, and the
// display dims as it otherwise would. The slot's default is today's.

/// The slot is set before any session is made, and read on the main thread.
package enum WakeLockLink {
    /// A holder (a session's presenter) holds the process's lock or lets it go.
    package static var hold: ((AnyObject, Bool) -> Void)? = { WakeLock.hold($0, $1) }
}

extension Presenter {
    /// The batch's word on the wake lock, when it says one.
    func applyWakeLock(_ batch: Batch) {
        if let held = batch.wakelock { WakeLockLink.hold?(self, held) }
    }
}
