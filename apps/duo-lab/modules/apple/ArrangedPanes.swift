#if os(iOS)
import UIKit
#endif

final class DuoArrangements {
    #if os(iOS)
    private var containers: [ObjectIdentifier: (controller: UIViewController, mode: String)] = [:]
    #endif

    func update(_ element: ExactElement) {
        #if os(iOS)
        guard element.hatch == .arrangedPanes else { return }
        element.reusable = true
        let key = ObjectIdentifier(element)
        let mode = element.data[.paneLayout] ?? "split"
        if #available(iOS 27.1, *) {
            if mode == "css" { element.arrange(using: nil); containers[key]?.mode = mode; return }
            let cached = containers[key]
            let controller = cached?.controller as? UIArrangementViewController ?? UIArrangementViewController()
            if cached?.mode != mode {
                // Animate a change on screen; one made off screen has nothing to animate
                let animated = controller.viewIfLoaded?.window != nil
                switch mode {
                case "horizontal": controller.updateArrangement(.split.axes(.horizontal), animated: animated)
                case "vertical": controller.updateArrangement(.split.axes(.vertical), animated: animated)
                case "overlay": controller.updateArrangement(.overlay, animated: animated)
                default: controller.updateArrangement(.split, animated: animated)
                }
            }
            if element.arrange(using: controller) { containers[key] = (controller, mode) }
        }
        #endif
    }

    func remove(_ element: ExactElement) {
        #if os(iOS)
        containers[ObjectIdentifier(element)] = nil
        #endif
    }
}
