// @ref LLP 1126.005 D4 — the record of a kind's placed widgets, which the
// widget extension writes to the App Group and the app answers from.
import Foundation

public enum WidgetPlacementRecords {
    public static let lag: TimeInterval = 120

    public struct Record: Codable, Equatable {
        public var parameters: [String]
        public var count: Int
        public var complete: Bool
        public var placements: [[String: String?]]
        public var unlisted: [String: Double]

        public init(parameters: [String], listed: [[String: String]?]?, asked: [String: String]?, after earlier: Record? = nil, now: Date = Date()) {
            self.parameters = parameters
            let decoded = (listed ?? []).compactMap { $0 }
            count = listed?.count ?? 0
            complete = listed != nil && decoded.count == count
            var rows: [String: [String: String?]] = [:]
            for values in decoded {
                rows[Record.key(values, parameters)] = Dictionary(uniqueKeysWithValues: parameters.map { ($0, values[$0]) })
            }
            placements = rows.keys.sorted().compactMap { rows[$0] }
            let stamp = now.timeIntervalSinceReferenceDate
            var waiting = (earlier?.parameters == parameters ? earlier?.unlisted : nil) ?? [:]
            if listed != nil {
                if let asked, waiting[Record.key(asked, parameters)] == nil { waiting[Record.key(asked, parameters)] = stamp }
                waiting = waiting.filter { rows[$0.key] == nil }
            }
            unlisted = waiting.filter { stamp - $0.value < WidgetPlacementRecords.lag }
        }

        func waits(at now: Date) -> Bool {
            unlisted.values.contains { now.timeIntervalSinceReferenceDate - $0 < WidgetPlacementRecords.lag }
        }

        private static func key(_ values: [String: String], _ parameters: [String]) -> String {
            let row = parameters.map { values[$0].map { $0 as Any } ?? NSNull() }
            return (try? JSONSerialization.data(withJSONObject: row)).map { String(decoding: $0, as: UTF8.self) } ?? ""
        }
    }

    public enum Answer: Equatable {
        case placed([[String: String?]])
        case unknown
    }

    public static func answer(_ record: Record?, parameters: [String]?, placed: Int, now: Date = Date()) -> Answer {
        let current = record.flatMap { $0.parameters == parameters ? $0 : nil }
        if placed == 0 { return current?.waits(at: now) == true ? .unknown : .placed([]) }
        guard let current, current.complete, current.count == placed, !current.waits(at: now) else { return .unknown }
        return .placed(current.placements)
    }

    private static let queue = DispatchQueue(label: "exact.widget-placements")
    private static func file(_ root: URL) -> URL { root.appendingPathComponent("widget-placements.json") }

    public static func read(root: URL?) -> [String: Record] {
        guard let root, let data = try? Data(contentsOf: file(root)) else { return [:] }
        return (try? JSONDecoder().decode([String: Record].self, from: data)) ?? [:]
    }

    public static func written(root: URL?) -> Date? {
        guard let root else { return nil }
        return (try? FileManager.default.attributesOfItem(atPath: file(root).path))?[.modificationDate] as? Date
    }

    @discardableResult
    public static func record(kind: String, parameters: [String], listed: [[String: String]?]?, asked: [String: String]?, root: URL?, appID: String?, now: Date = Date()) -> Bool {
        guard let root else { return false }
        return queue.sync {
            var all = read(root: root)
            let record = Record(parameters: parameters, listed: listed, asked: asked, after: all[kind], now: now)
            guard record != all[kind] else { return false }
            all[kind] = record
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys]
            guard let data = try? encoder.encode(all), (try? data.write(to: file(root), options: .atomic)) != nil else { return false }
            if let appID {
                CFNotificationCenterPostNotification(CFNotificationCenterGetDarwinNotifyCenter(), CFNotificationName((appID + ".widget-placements") as CFString), nil, nil, true)
            }
            return true
        }
    }
}
