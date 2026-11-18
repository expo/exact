// A configured widget's intent and provider (app.json `configuration`), over Timeline.swift's.
import AppIntents
import Foundation
import WidgetKit

/// A configured widget's intent (app.json `configuration`): the generated
/// bundle declares one per entry, a parameter per row, and names its values.
protocol ExactConfigurationIntent: WidgetConfigurationIntent {
    #if os(watchOS)
    static var exactRecommendations: [AppIntentRecommendation<Self>] { get }
    #endif
    var exactConfiguration: [String: String] { get }
}

/// A configured widget's provider. The refresh receives the intent's values, so each
/// timeline is one configuration's, shared across the families that chose it, and
/// every entry carries the values for its render.
struct ExactIntentTimelineProvider<Intent: ExactConfigurationIntent>: AppIntentTimelineProvider {
    let kind: String
    private var base: ExactTimelineProvider { ExactTimelineProvider(kind: kind) }
    /// The intent's values as the refresh and the render decode them. Sorted keys give
    /// equal choices equal bytes, which key the kept timeline.
    private static func chosen(_ intent: Intent) -> Data? {
        try? JSONSerialization.data(withJSONObject: intent.exactConfiguration, options: [.sortedKeys])
    }
    private static func configured(_ entry: ExactWidgetEntry, _ chosen: Data?) -> ExactWidgetEntry {
        var entry = entry
        entry.configuration = chosen
        return entry
    }
    func placeholder(in context: Context) -> ExactWidgetEntry { base.placeholder(in: context) }
    func snapshot(for configuration: Intent, in context: Context) async -> ExactWidgetEntry {
        let chosen = Self.chosen(configuration)
        return await withCheckedContinuation { done in
            base.getSnapshot(in: context) { done.resume(returning: Self.configured($0, chosen)) }
        }
    }
    func timeline(for configuration: Intent, in context: Context) async -> Timeline<ExactWidgetEntry> {
        let chosen = Self.chosen(configuration)
        return await withCheckedContinuation { done in
            base.getTimeline(in: context, configuration: chosen) { timeline in
                done.resume(returning: Timeline(entries: timeline.entries.map { Self.configured($0, chosen) }, policy: timeline.policy))
            }
        }
    }
    #if os(watchOS)
    // A watch face offers a configured complication only through its recommendations.
    func recommendations() -> [AppIntentRecommendation<Intent>] {
        Intent.exactRecommendations
    }
    #endif
}
