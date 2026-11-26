// @ref LLP 1123 D16 — the window's root route says whether the window's content runs under its title bar and toolbar
// (`navigationFullSizeContent`, AppKit's `.fullSizeContentView`). Navigation reads it from the root route of the first
// root's selected stack, and the window's owner, which sets the window's style, takes it over its own choice.
#if os(macOS)
import AppKit
import ObjectiveC

extension Presenter {
    nonisolated(unsafe) private static var fullSizeKey: UInt8 = 0

    /// What the window's root route says of full-size content, `nil` where it says nothing: the owner's choice stands.
    /// A change tells the owner through `onViewportFit`, which reads the window's chrome again.
    package var rootFullSizeContent: Bool? {
        get { (objc_getAssociatedObject(self, &Self.fullSizeKey) as? NSNumber)?.boolValue }
        set {
            guard newValue != rootFullSizeContent else { return }
            objc_setAssociatedObject(self, &Self.fullSizeKey, newValue.map(NSNumber.init(value:)), .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
            onViewportFit?()
        }
    }
}
#endif
