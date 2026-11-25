import UIKit

final class ScreenModule: ExactModule {
    override class var views: [String: ExactNativeFactory] {
        ["app-table": ExactNativeFactory { _, events in ExactNativeScreen(screen: AppearanceTable(), events: events) },
         "reusable-table": ExactNativeFactory(reuse: true) { _, events in AppTable(screen: AppearanceTable(), events: events) },
         "clearing-table": ExactNativeFactory { _, events in ClearingTable(screen: AppearanceTable(), events: events) }]
    }
}

private final class AppTable: ExactNativeScreen {
    override func prepareForReuse() throws {}
}

/// An app's screen that lets the host clear its safe area through its controller (LLP 1123 D15).
private final class ClearingTable: ExactNativeScreen {
    override var safeArea: ExactNativeSafeArea? { ExactNativeSafeArea(adjustsAdditionalSafeAreaInsets: true) }
}

let exactModule: ExactModule.Type = ScreenModule.self
