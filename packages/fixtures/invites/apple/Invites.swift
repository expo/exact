// @fixture/invites on Apple: the session's one instance, made when the module
// loads (`"start": "load"`), takes what the host held for its session of the
// `cloudkit-share` host event, then takes each later one when the host says
// it arrived; `invites` answers what it took and how much was waiting at its
// start. `deliverInvite` delivers one as the system would, by the host's
// test entry. A real package casts each payload to `CKShare.Metadata`; this
// one's are strings.
import ExactPackage
import Foundation

public final class InvitesModule: ExactPackage {
    private static let event = "cloudkit-share"
    private let context: ExactPackageContext
    private var taken: [String] = []
    private var atStart = 0
    private var observer: NSObjectProtocol?

    public init(context: ExactPackageContext) {
        self.context = context
        take()
        atStart = taken.count
        observer = NotificationCenter.default.addObserver(forName: ExactHostEvents.arrived, object: nil, queue: nil) { [weak self] note in
            guard let self, note.userInfo?["event"] as? String == Self.event, note.userInfo?["session"] as? String == context.label else { return }
            DispatchQueue.main.async { self.take() }
        }
    }

    deinit { observer.map(NotificationCenter.default.removeObserver) }

    private func take() {
        let items = context.hostEvents.take(Self.event)
        guard !items.isEmpty else { return }
        taken += items.map { "\($0)" }
        context.changed("invites.invites")
    }

    private var answer: [String: Any] { ["taken": taken.joined(separator: "; "), "count": taken.count, "atStart": atStart] }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        switch name {
        case "invites": reply(answer)
        case "deliverInvite":
            let body: [String: Any] = ["event": Self.event, "payload": NSString(string: args["text"] as? String ?? ""), "session": context.label]
            NotificationCenter.default.post(name: ExactHostEvents.deliver, object: body as NSDictionary)
            DispatchQueue.main.async { reply(self.answer) }
        // Answered only under their grants; the runner answers the substitute otherwise.
        case "shareStatus", "sharePlayStatus": reply(["exact:answer": true])
        default: reply(["error": "NotSupportedError"])
        }
    }
}
