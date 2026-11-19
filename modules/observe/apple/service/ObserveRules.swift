// Observe's custom-event validation, copied from expo-app-metrics
// (`LogEvents/*Validation.swift`, `AttributeValidation.swift`) so events match upstream.
// Attributes are kept in key order up to the limit. The rest, plus blank and
// reserved keys, are dropped and counted.
import Foundation

enum ObserveRules {
    static let maxName = 256, maxBody = 4096, maxDisplayName = 128, maxAttributes = 128
    /// The log name of `Observe.reportFeedback`.
    static let feedbackName = "expo.feedback"

    static func name(_ raw: String) -> String? {
        let name = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, !name.hasPrefix("expo."), name.count <= maxName else {
            fputs("observe: dropped an event with an invalid name\n", stderr)
            return nil
        }
        return name
    }

    static func truncate(_ s: String, _ max: Int) -> String {
        s.count <= max ? s : String(s.prefix(max - 1)) + "…"
    }

    static func body(_ raw: String?) -> String? { raw.map { truncate($0, maxBody) } }

    static func displayName(_ raw: String?) -> String? {
        guard let s = raw?.trimmingCharacters(in: .whitespacesAndNewlines), !s.isEmpty else { return nil }
        return truncate(s, maxDisplayName)
    }

    /// Observe's feedback checks, from expo-observe's `reportFeedback.ts`: a non-empty id,
    /// a message or a rating, and a whole-number rating within a scale from 0 to 10.
    static func feedback(_ e: [String: Any]) -> (id: String, message: String?, rating: [String: Int]?)? {
        let id = e["id"] as? String ?? ""
        let message = (e["message"] as? String).flatMap { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0 }
        var rating: [String: Int]?
        if let value = e["rating"] as? Double {
            let whole = { (d: Double?) in d.flatMap { $0.rounded() == $0 && abs($0) < 1e15 ? Int($0) : nil } }
            guard let v = whole(value), let min = whole(e["min"] as? Double ?? 0), let max = whole(e["max"] as? Double ?? 10),
                  min >= 0, min < max, max <= 10, v >= min, v <= max else {
                fputs("observe: dropped feedback with a rating outside its scale\n", stderr)
                return nil
            }
            rating = ["value": v, "min": min, "max": max]
        }
        guard !id.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, message != nil || rating != nil else {
            fputs("observe: dropped feedback with no id, or with neither message nor rating\n", stderr)
            return nil
        }
        return (id, message, rating)
    }

    static func reserved(_ key: String) -> Bool {
        (key.hasPrefix("expo.") && key.count > 5) || key == "session.id" || key == "event.name"
    }

    static func attributes(_ raw: [String: Any]) -> (kept: [String: Any], dropped: Int) {
        var kept: [String: Any] = [:]
        var dropped = 0
        for (key, value) in raw.sorted(by: { $0.key < $1.key }) {
            let k = key.trimmingCharacters(in: .whitespacesAndNewlines)
            if k.isEmpty || reserved(k) || kept.count >= maxAttributes { dropped += 1; continue }
            kept[k] = value
        }
        return (kept, dropped)
    }
}
