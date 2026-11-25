import ExactPackage
import UIKit

final class ScreenPackage: ExactPackage {
    init(context: ExactPackageContext) {}
    func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        reply(["error": "NotSupportedError"])
    }
    func view(_ tag: String, id: String, props: [String: String], events: ExactPackageEvents) throws -> ExactPackageView {
        if tag == "probe-plain" { return PlainView() }
        if tag == "probe-cleared" { return ClearedView() }
        return TableScreen()
    }
}

/// A plain view lending its scroll view to the host (LLP 1123 D15): its layout clears 20 of any
/// side clearance, and its release says so in the scroll view's accessibility hint.
private final class PlainView: ExactPackageView {
    let view = UIView()
    let scroll = UIScrollView()
    init() { view.addSubview(scroll) }
    var safeArea: ExactPackageSafeArea? {
        let scroll = scroll
        return ExactPackageSafeArea(
            scrollViews: [.init(scroll, anchor: "list") { allowed in UIEdgeInsets(top: 0, left: min(allowed.left, 20), bottom: 0, right: min(allowed.right, 20)) }],
            safeAreaRegions: 1, release: { scroll.accessibilityHint = "released" })
    }
    func setProps(_ props: [String: String]) {}
}

/// A view with no controller that lets the host clear its safe area (LLP 1123 D15).
private final class ClearedView: ExactPackageView {
    let view = UIView()
    var safeArea: ExactPackageSafeArea? { ExactPackageSafeArea(adjustsAdditionalSafeAreaInsets: true) }
    func setProps(_ props: [String: String]) {}
}

private final class TableScreen: ExactPackageView {
    let table = AppearanceTable()
    var controller: UIViewController? { table }
    var view: UIView { table.view }
    func setProps(_ props: [String: String]) {}
}
