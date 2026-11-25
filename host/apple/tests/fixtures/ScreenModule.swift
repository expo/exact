import UIKit

final class ScreenModule: ExactModule {
    override class var views: [String: ExactNativeFactory] {
        ["app-table": ExactNativeFactory { _, events in ExactNativeScreen(screen: AppearanceTable(), events: events) },
         "reusable-table": ExactNativeFactory(reuse: true) { _, events in AppTable(screen: AppearanceTable(), events: events) }]
    }
}

private final class AppTable: ExactNativeScreen {
    override func prepareForReuse() throws {}
}

let exactModule: ExactModule.Type = ScreenModule.self
