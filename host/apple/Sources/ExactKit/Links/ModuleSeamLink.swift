// @ref LLP 1047.001 D10 — the module seam's session-side families (cut list
// B2): notification actions and responses, push registration, host events
// and live activities. The core reaches them only through what this file
// declares. Without them a session declares no notification categories
// (it is ready at once), holds no background action, registers for no push,
// hears no host event and sends no activity. The slots' defaults are
// today's. The host executables install wakes, notification clicks and host
// events through `ExactApp`'s public calls in the families' files; they are
// the composition's, and import the families' targets once they move.

/// What a session asks of notifications (NotifyActions.swift).
package protocol NotificationHosting: AnyObject {
    /// Registration of the plan's categories, before data code runs; `ready` follows.
    func declareCategories(_ session: ExactSession, ready: @escaping () -> Void)
    /// The plan restarted: its background actions are replaced.
    func replaceBackgroundActions(_ session: ExactSession)
    /// Responses that arrived before the session could take them.
    func deliverResponses(_ session: ExactSession)
    /// A background action begins, unless it expired while queued.
    func beginBackgroundAction(_ session: ExactSession, _ id: String) -> Bool
    /// After work lands: a background action it ended lets the app go.
    func backgroundActionsApplied(_ session: ExactSession)
    /// The session ends: every background action it held lets the app go.
    func endBackgroundActions(_ session: ExactSession)
}

/// No notifications: categories are ready at once, and no background action begins.
package final class NoNotifications: NotificationHosting {
    package init() {}
    package func declareCategories(_ session: ExactSession, ready: @escaping () -> Void) { ready() }
    package func replaceBackgroundActions(_ session: ExactSession) {}
    package func deliverResponses(_ session: ExactSession) {}
    package func beginBackgroundAction(_ session: ExactSession, _ id: String) -> Bool { false }
    package func backgroundActionsApplied(_ session: ExactSession) {}
    package func endBackgroundActions(_ session: ExactSession) {}
}

/// What a session asks of its live activities (Activities.swift).
package protocol ActivityOutputting: AnyObject {
    /// An `activity` op from a batch.
    func enqueue(_ op: [String: Any])
    /// Once data has activated: what was queued is sent.
    func drain()
}

/// The slots are set before any session is made, and read on the main thread.
package enum ModuleSeamLink {
    /// Notification actions and responses.
    package static var notifications: (any NotificationHosting)? = BuiltInNotifications()
    /// Push registration and host events, listened for as a session is made.
    package static var listen: (() -> Void)? = { PushRegistration.listen(); HostEvents.listen() }
    /// A session ended: the host events queued for it go to the first live
    /// session, which hears that they arrived; with none, they stay held.
    package static var sessionEnded: ((String) -> Void)? = { HostEvents.ended($0) }
    /// Whether the app ships the activity arm, and a session's activity output.
    package static var activitiesPackaged: (() -> Bool)? = { ActivityLibrary.packaged }
    package static var makeActivities: ((ExactSession) -> any ActivityOutputting)? = { ActivityOutput($0) }
}

extension ExactSession {
    /// The session's notifications, or none.
    var notificationHost: any NotificationHosting { ModuleSeamLink.notifications ?? NoNotifications() }
}
