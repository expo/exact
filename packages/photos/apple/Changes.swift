// `photoChanges`' token and answer (LLP 1133.019.000 D1), shared by the agent's library and PhotoKit's.
import Foundation

/// A `photoChanges` token, opaque to the app: base64 of JSON naming the backend that answered it and the view it
/// answered for (the access and the subtypes), then either nothing (a rescan-only view's starting point), a
/// checkpoint (`at`), or a walk's continuation (`at`, `end` and the last candidate answered, `after`). The agent's
/// tokens also name its store (`epoch`).
struct ChangeToken: Equatable {
    var backend: String
    var access: String
    var subtypes: [String]
    var epoch: String?
    var at: String?
    var end: String?
    var after: String?

    struct Malformed: Error {}

    static let refused: [String: Any] = ["error": "SyntaxError", "message": "not a token photoChanges answered here"]

    /// The view's subtypes: each word once, in order.
    static func subtypes(_ args: [String: Any]) -> [String] { Array(Set((args["subtypes"] as? [String]) ?? [])).sorted() }

    var rescan: Bool { at == nil }

    var text: String {
        var o: [String: Any] = ["v": 1, "backend": backend, "access": access, "subtypes": subtypes]
        for (k, v) in [("epoch", epoch), ("at", at), ("end", end), ("after", after)] { if let v { o[k] = v } }
        return ((try? JSONSerialization.data(withJSONObject: o, options: [.sortedKeys])) ?? Data()).base64EncodedString()
    }

    /// Nil for no token or an empty one. Throws `Malformed` for text no `backend` answered.
    static func parse(_ given: Any?, backend: String) throws -> ChangeToken? {
        guard let given = given as? String, !given.isEmpty else { return nil }
        guard let data = Data(base64Encoded: given), let o = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let v = o["v"] as? NSNumber, CFGetTypeID(v) != CFBooleanGetTypeID(), v.doubleValue == 1, o["backend"] as? String == backend,
              let access = o["access"] as? String, let subtypes = o["subtypes"] as? [String] else { throw Malformed() }
        func field(_ k: String) throws -> String? {
            guard let v = o[k] else { return nil }
            guard let s = v as? String, !s.isEmpty else { throw Malformed() }
            return s
        }
        let t = ChangeToken(backend: backend, access: access, subtypes: subtypes, epoch: try field("epoch"), at: try field("at"),
                            end: try field("end"), after: try field("after"))
        // A continuation has both its end and its last key; a rescan-only token has neither. The view is one this
        // module answers, and the text is exactly what its encoder writes for these fields.
        guard (t.end == nil) == (t.after == nil), !(t.rescan && t.end != nil),
              ["full", "limited", "none"].contains(t.access), t.subtypes == Array(Set(t.subtypes)).sorted(), t.subtypes.allSatisfy({ $0 == "screenshot" }),
              t.text == given else { throw Malformed() }
        return t
    }

    /// Whether `since` was answered for this view.
    func sameView(_ since: ChangeToken) -> Bool { access == since.access && subtypes == since.subtypes }

    /// `photoChanges`' answer.
    func answer(added: [[String: Any]] = [], updated: [[String: Any]] = [], deleted: [String] = [], more: Bool = false,
                expired: Bool = false) -> [String: Any] {
        ["added": added, "updated": updated, "deleted": deleted, "token": text, "more": more, "expired": expired]
    }

    /// `photoChanges`' limit: 1 to 500, 500 if none.
    static func limit(_ args: [String: Any]) -> Int { min(500, max(1, (args["limit"] as? NSNumber)?.intValue ?? 500)) }
}
