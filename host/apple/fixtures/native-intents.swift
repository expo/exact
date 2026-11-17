import AppIntents
import UniformTypeIdentifiers
@available(iOS 18.0, macOS 15.0, *)
struct NativeAction: AppIntent {
    static let title: LocalizedStringResource = "Native action"
    @Parameter(title: "Screenshot", supportedContentTypes: [.image]) var image: IntentFile
    static let openAppWhenRun = true
    static let isDiscoverable = false
    func perform() async throws -> some IntentResult { .result() }
}
struct ShortcutAction: AppIntent {
    static let title: LocalizedStringResource = "Shortcut action"
    @Parameter(title: "Count", default: 5) var count: Int
    @Parameter(title: "Ratio", default: 2.5) var ratio: Double
    @Parameter(title: "Enabled", default: false) var enabled: Bool
    @Parameter(title: "Name", default: "") var name: String
    @Parameter(title: "Optional count", default: 0) var optionalCount: Int?
    func perform() async throws -> some IntentResult { .result() }
}
@available(iOS 18.0, macOS 15.0, *)
struct NativeShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(intent: ShortcutAction(), phrases: ["Native in \(.applicationName)"], shortTitle: "Native", systemImageName: "app")
    }
}
