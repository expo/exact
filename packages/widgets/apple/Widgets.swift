import ExactPackage
import Foundation

public final class WidgetsModule: ExactPackage {
    private let context: ExactPackageContext
    private var observer: NSObjectProtocol?

    public init(context: ExactPackageContext) {
        self.context = context
        observer = NotificationCenter.default.addObserver(forName: ExactWidgetPlacements.changed, object: nil, queue: .main) { [context] _ in
            context.changed("widgets.placedWidgets")
        }
    }
    deinit { observer.map(NotificationCenter.default.removeObserver) }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        guard name == "placedWidgets", let kind = args["kind"] as? String else { return reply(["error": "NotSupportedError"]) }
        context.placedWidgets.answer(kind: kind) { reply(["exact:answer": $0]) }
    }
}
