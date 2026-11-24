#if os(iOS)
import UIKit

/// Which steps the drag section's hint gives: an iPad, or an iPhone Duo while it is open, shows two apps side by
/// side, so a card drags straight across; an iPhone, or a closed Duo, shows one app at a time. An iPad is UIKit's pad
/// idiom. A Duo is an iPhone whose window's hinge interaction reports a hinge (iOS 27.1), and the hinge's status says
/// whether it is closed; an iPhone without one reports none.
final class DragDevice {
    /// What the `dragDevice` answer watches: said whenever `kind` changes
    static let topic = "gallery-drag-device"

    /// `pad`, `duo-open`, `duo-closed` or `phone`
    private(set) var kind = UIDevice.current.userInterfaceIdiom == .pad ? "pad" : "phone"
    /// Called on the main thread when `kind` changes
    var changed: () -> Void = {}

    /// Hears the hinge in a window the session has just been given, through an empty view of the module's own
    func watch(_ window: ExactWindow) {
        guard #available(iOS 27.1, *), kind != "pad", window.isNew, let host = window.window else { return }
        let ear = UIView(frame: .zero)
        ear.isUserInteractionEnabled = false
        ear.isAccessibilityElement = false
        ear.addInteraction(UIHingeInteraction { [weak self] _, update in
            // A view leaving the window reports no hinge, and the device has not changed
            guard let self, let hinge = update.hinge else { return }
            let now = hinge.status == .closed ? "duo-closed" : "duo-open"
            guard now != kind else { return }
            kind = now
            changed()
        })
        host.addSubview(ear)
        window.owns(view: ear, "Hears the hinge for the drag section's hint")
    }
}
#endif
