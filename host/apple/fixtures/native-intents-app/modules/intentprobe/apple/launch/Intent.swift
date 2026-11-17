import AppIntents
import ExactKit
import Darwin
@available(iOS 18.0, macOS 15.0, *)
struct NativeLaunchIntent: AppIntent {
    static let title: LocalizedStringResource = "Native launch probe"
    @MainActor func perform() async throws -> some IntentResult { .result() }
}
#if DEBUG
@available(iOS 18.0, macOS 15.0, *)
struct DevelopmentIntent: AppIntent {
    static let title: LocalizedStringResource = "Development probe"
    func perform() async throws -> some IntentResult { .result() }
}
#else
@available(iOS 18.0, macOS 15.0, *)
struct ProductionIntent: AppIntent {
    static let title: LocalizedStringResource = "Production probe"
    func perform() async throws -> some IntentResult { .result() }
}
#endif
struct IntentResolutionProbe: ExactLaunchPart {
    static let module = "intentprobe"
    static func launch(_ context: ExactLaunchContext) {
        guard let name = ProcessInfo.processInfo.environment["EXACT_PROBE_INTENT"] else { return }
        guard let type = _typeByName(name) as? any AppIntent.Type else {
            print("unresolved intent: \(name)")
            exit(1)
        }
        _ = type.init()
        print("resolved intent before any view: \(name)")
        exit(0)
    }
}
