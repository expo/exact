import Foundation
#if os(iOS) || os(tvOS)
import UIKit
#endif

extension ExactSession {
    /// The preference bits the runtime lays out with: the system's (or the
    /// scene's) appearance and the view's, with a display-mode override.
    func preferenceBits() -> UInt32 {
        #if os(iOS) || os(tvOS)
        // The scene owns system appearance; a window's app override does not.
        if let scene = view?.window?.windowScene {
            systemDark = scene.traitCollection.userInterfaceStyle == .dark
        }
        let dark = systemDark
        #else
        let dark = DisplayPreferences.systemDark
        #endif
        let bits = DisplayPreferences.bits(systemDark: dark, view: view)
        return displayModeOverride.map { (bits & ~(7 << 11)) | ($0 << 11) } ?? bits
    }
}
