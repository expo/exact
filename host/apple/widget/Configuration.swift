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

/// A configured widget's provider. The refresh does not read the configuration,
/// so each timeline is the unconfigured provider's, shared across families and
/// configurations, with the intent's values stamped on every entry for its render.
struct ExactIntentTimelineProvider<Intent: ExactConfigurationIntent>: AppIntentTimelineProvider {
    let kind: String
    private var base: ExactTimelineProvider { ExactTimelineProvider(kind: kind) }
    private static func configured(_ entry: ExactWidgetEntry, _ intent: Intent) -> ExactWidgetEntry {
        var entry = entry
        entry.configuration = try? JSONSerialization.data(withJSONObject: intent.exactConfiguration, options: [.sortedKeys])
        return entry
    }
    func placeholder(in context: Context) -> ExactWidgetEntry { base.placeholder(in: context) }
    func snapshot(for configuration: Intent, in context: Context) async -> ExactWidgetEntry {
        await withCheckedContinuation { done in
            base.getSnapshot(in: context) { done.resume(returning: Self.configured($0, configuration)) }
        }
    }
    func timeline(for configuration: Intent, in context: Context) async -> Timeline<ExactWidgetEntry> {
        await withCheckedContinuation { done in
            base.getTimeline(in: context) { timeline in
                done.resume(returning: Timeline(entries: timeline.entries.map { Self.configured($0, configuration) }, policy: timeline.policy))
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
