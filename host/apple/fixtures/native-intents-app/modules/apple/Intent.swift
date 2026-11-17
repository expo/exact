import AppIntents
import UniformTypeIdentifiers
final class IntentModule: ExactModule {}
let exactModule: ExactModule.Type = IntentModule.self
struct NativeScreenshotIntent: AppIntent {
    static let title: LocalizedStringResource = "Native screenshot probe"
    @Parameter(title: "Screenshot", supportedContentTypes: [.image]) var image: IntentFile
    func perform() async throws -> some IntentResult { .result() }
}
