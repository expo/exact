import Foundation
#if os(iOS) || os(tvOS)
import UIKit
#endif

extension ExactSession {
    /// The view's appearance, for paint motion's `light-dark()` (LLP 1062).
    /// The platform's colours are reported first: before the first scheme
    /// that fills the kernel's table without motion, so nothing eases from a
    /// fallback to the platform's colour at startup (LLP 1095 D9).
    public func scheme(dark: Bool) {
        guard booted, state != .destroyed else { return }
        let first = schemeDark == nil
        schemeDark = dark
        reportColors()
        reportDeferredAppearances(first: first)
        // A subtree with a `color-scheme` of its own keeps its appearance when the session's changes, so no trait
        // callback says it: said here, after the scheme. Its views' appearance is the row's, already settled,
        // whatever the platform's walk has reached.
        for v in presenter.views.values where v.style["color_scheme"] != nil { noteAppearance(v) }
        apply(runtime.scheme(dark: dark))
    }

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
