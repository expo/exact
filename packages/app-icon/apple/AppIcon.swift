// @exact/app-icon on iOS and iPadOS (LLP 1163.000 D3): `UIApplication`'s alternate icon. The
// roster is the build's, `ExactAlternateIcons` in Info.plist, in `alternate_icons`' order; `icon`
// is `alternateIconName` as UIKit reports it, which may name an icon an update removed, and
// none for the primary. iOS tells the person of a change in an alert of its own.
//
// The runner admits `setAppIcon` only from the app's own foreground code (`origin`, `foreground`);
// here it compares the raw `alternateIconName`, so choosing the primary always clears a removed
// alternate, and asks iOS nothing when the icon already shows.
import ExactPackage
import Foundation
#if canImport(UIKit)
import UIKit
#endif

public final class AppIconModule: ExactPackage {
    let context: ExactPackageContext

    public init(context: ExactPackageContext) { self.context = context }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        #if canImport(UIKit) && os(iOS)
        DispatchQueue.main.async {
            let app = UIApplication.shared
            switch name {
            case "appIcon": reply(Self.status(app))
            case "setAppIcon":
                let icon = args["icon"] as? String, status = Self.status(app)
                guard status["available"] as? Bool == true else { return reply(["error": "NotSupportedError"]) }
                if icon == app.alternateIconName { return reply(status) }
                if let icon, !Self.roster.contains(icon) {
                    return reply(["error": "NotFoundError", "message": "no alternate icon named \(icon)"])
                }
                app.setAlternateIconName(icon) { error in
                    DispatchQueue.main.async {
                        guard let error else {
                            self.context.log("setAppIcon: shows \(icon ?? "the primary icon")")
                            return reply(Self.status(app))
                        }
                        let ns = error as NSError
                        reply(["error": "UnknownError", "message": ns.localizedDescription, "apple": ["domain": ns.domain, "code": ns.code]])
                    }
                }
            default: reply(["error": "NotSupportedError"])
            }
        }
        #else
        reply(name == "appIcon" ? ["available": false, "icon": NSNull(), "icons": [String]()] : ["error": "NotSupportedError"])
        #endif
    }

    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] {
        ["error": "NotSupportedError"]
    }

    /// The alternates the build ships (host/apple/assets.mjs).
    static let roster = Bundle.main.object(forInfoDictionaryKey: "ExactAlternateIcons") as? [String] ?? []

    #if canImport(UIKit) && os(iOS)
    /// Available where iOS switches icons and the binary ships an alternate or one still shows.
    static func status(_ app: UIApplication) -> [String: Any] {
        let shown = app.alternateIconName
        let available = app.supportsAlternateIcons && (!roster.isEmpty || shown != nil)
        return ["available": available, "icon": shown ?? NSNull(), "icons": available ? roster : []]
    }
    #endif
}
