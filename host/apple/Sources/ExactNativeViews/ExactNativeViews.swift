// The native module and hatches family (LLP 1047.001 D10, the roster's
// `native_views`): the app's module artifact and its process table, its
// native views, and the navigation, route, tab, toolbar, scope and element
// hatches with their clock and regions. An app's composition links this
// module when its plan has a module tag, a hatch or a package view, when it
// links the runner's module seam (`modules`, a manifest's `link` included),
// or when the app ships a module artifact, and installs it before any
// session is made;
// the core reaches it only through `NativeModuleLink`.
import ExactKit

public enum ExactNativeViews {
    /// Set `NativeModuleLink`'s slots, before any session is made.
    public static func install() {
        NativeModuleLink.make = { NativeViews() }
        NativeModuleLink.makeElements = { ElementHatches($0) }
    }
}
