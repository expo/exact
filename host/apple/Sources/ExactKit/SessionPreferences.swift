import Foundation
#if os(iOS) || os(tvOS)
import UIKit
#endif

extension ExactSession {
    /// The device's posture and the viewport segments a fold makes (LLP 1078 D4, D5): the view's reading,
    /// kept for the agent's `layout.env` and told to the kernel and the runner in one batch.
    /// Returns the batch's error, when the runtime refused the grid; the fold is kept only when it took it.
    @discardableResult func segments(_ fold: ViewportFold) -> String? {
        guard booted, state != .destroyed else { return nil }
        let batch = runtime.segments(fold)
        if batch.error == nil { presenter.fold = fold }
        apply(batch)
        return batch.error
    }
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
