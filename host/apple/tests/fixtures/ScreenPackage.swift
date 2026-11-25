import ExactPackage
import UIKit

final class ScreenPackage: ExactPackage {
    init(context: ExactPackageContext) {}
    func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        reply(["error": "NotSupportedError"])
    }
    func view(_ tag: String, id: String, props: [String: String], events: ExactPackageEvents) throws -> ExactPackageView {
        if tag == "probe-plain" { return PlainView() }
        return TableScreen()
    }
}

private final class PlainView: ExactPackageView {
    let view = UIView()
    func setProps(_ props: [String: String]) {}
}

private final class TableScreen: ExactPackageView {
    let table = AppearanceTable()
    var controller: UIViewController? { table }
    var view: UIView { table.view }
    func setProps(_ props: [String: String]) {}
}
