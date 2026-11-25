import UIKit

final class AppearanceTable: UITableViewController {
    let trace = UILabel()
    override func viewDidLoad() {
        super.viewDidLoad()
        tableView.tableFooterView = trace
    }
    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        trace.text = (trace.text ?? "") + "willAppear "
    }
    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        trace.text = (trace.text ?? "") + "didAppear "
        trace.accessibilityHint = "visible"
    }
    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        view.window?.layoutIfNeeded()
        trace.text = (trace.text ?? "") + "willDisappear "
    }
    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        trace.accessibilityHint = "hidden"
        trace.text = (trace.text ?? "") + "didDisappear "
    }
    override func didMove(toParent parent: UIViewController?) {
        super.didMove(toParent: parent)
        if parent == nil { view.window?.layoutIfNeeded() }
        if parent != nil { trace.accessibilityValue = nil }
        else if trace.accessibilityValue == nil { trace.accessibilityValue = view.superview == nil ? "view left first" : "controller left first" }
    }
}
