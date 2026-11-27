import UIKit

final class ScreenModule: ExactModule {
    override class var views: [String: ExactNativeFactory] {
        ["app-table": ExactNativeFactory { _, events in ExactNativeScreen(screen: AppearanceTable(), events: events) },
         "reusable-table": ExactNativeFactory(reuse: true) { _, events in AppTable(screen: AppearanceTable(), events: events) },
         "clearing-table": ExactNativeFactory { _, events in ClearingTable(screen: AppearanceTable(), events: events) },
         "reusable-cleared": ExactNativeFactory(reuse: true) { _, events in ClearedView(events: events) }]
    }
}

private final class AppTable: ExactNativeScreen {
    override func prepareForReuse() throws {}
}

/// An app's screen that lets the host clear its safe area through its controller (LLP 1123 D15), which
/// counts its declaration's releases in its view's `accessibilityValue`.
private final class ClearingTable: ExactNativeScreen {
    override var safeArea: ExactNativeSafeArea? {
        ExactNativeSafeArea(adjustsAdditionalSafeAreaInsets: true, release: { [weak self] in
            guard let view = self?.screen.view else { return }
            view.accessibilityValue = String((Int(view.accessibilityValue ?? "") ?? 0) + 1)
        })
    }
}

/// A reusable view with no controller that lets the host clear its safe area (LLP 1123 D15).
private final class ClearedView: ExactNativeInstance {
    private let plain = UIView()
    override var view: ExactNativeView { plain }
    override var safeArea: ExactNativeSafeArea? { ExactNativeSafeArea(adjustsAdditionalSafeAreaInsets: true) }
    override func prepareForReuse() throws {}
}

let exactModule: ExactModule.Type = ScreenModule.self
