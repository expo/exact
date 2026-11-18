// The app composition (LLP 1047.001 D4, D10): the capabilities' Swift
// halves the app links, each imported and installed under the define
// `Package.swift` sets when the composition links it, and the updates the
// baked store level chooses (LLP 1030 D4, EXACT_APP_UPDATING).
import ExactKit
#if EXACT_APP_UPDATING
import ExactUpdates
#endif
#if EXACT_WATCH_SYNC && os(iOS)
import ExactWatchSync
#endif
#if EXACT_LINK_GROUPED_LISTS
import ExactGroupedLists
#endif
#if EXACT_LINK_MARKDOWN
import ExactMarkdown
#endif
#if EXACT_LINK_COLLECTIONS
import ExactCollections
#endif
#if EXACT_LINK_SURFACES
import ExactSurfaces
#endif
#if EXACT_LINK_DRAG
import ExactDrag
#endif
#if EXACT_LINK_MEDIA_PICKER
import ExactMediaPicker
#endif
#if EXACT_LINK_AUTH_SESSION
import ExactAuthSession
#endif
#if EXACT_LINK_NUMERALS
import ExactNumerals
#endif
#if EXACT_LINK_TIPS
import ExactTips
#endif
#if EXACT_LINK_PRESENCE
import ExactPresence
#endif
#if EXACT_LINK_EFFECTS
import ExactEffects
#endif
#if EXACT_LINK_SVG
import ExactSvg
#endif
#if EXACT_LINK_IMAGES
import ExactImages
#endif
#if EXACT_LINK_NATIVE_VIEWS
import ExactNativeViews
#endif
public enum ExactComposition {
    public static let app: ExactApp = {
        AgentLink.install()
        installCapabilities()
        let app = ExactApp.shared
        #if EXACT_WATCH_SYNC && os(iOS)
        app.replicateStore = ExactWatchSync.phone { line in app.sessions.forEach { $0.log(line) } }
        #endif
        #if EXACT_APP_UPDATING
        ExactUpdates.install(on: app)
        #endif
        return app
    }()

    /// Install the linked capabilities' Swift halves, in the roster's order,
    /// before any runtime or presenter exists: each sets its slots and is
    /// idempotent. An embedder calls the installers `capabilities.json` names,
    /// in its order; the tests call this as their bundle loads.
    public static func installCapabilities() {
        #if EXACT_LINK_GROUPED_LISTS
        ExactGroupedLists.install()
        #endif
        #if EXACT_LINK_MARKDOWN
        ExactMarkdown.install()
        #endif
        #if EXACT_LINK_COLLECTIONS
        ExactCollections.install()
        #endif
        #if EXACT_LINK_SURFACES
        ExactSurfaces.install()
        #endif
        #if EXACT_LINK_DRAG
        ExactDrag.install()
        #endif
        #if EXACT_LINK_MEDIA_PICKER
        ExactMediaPicker.install()
        #endif
        #if EXACT_LINK_AUTH_SESSION
        ExactAuthSession.install()
        #endif
        #if EXACT_LINK_NUMERALS
        ExactNumerals.install()
        #endif
        #if EXACT_LINK_TIPS
        ExactTips.install()
        #endif
        #if EXACT_LINK_PRESENCE
        ExactPresence.install()
        #endif
        #if EXACT_LINK_EFFECTS
        ExactEffects.install()
        #endif
        #if EXACT_LINK_SVG
        ExactSvg.install()
        #endif
        #if EXACT_LINK_IMAGES
        ExactImages.install()
        #endif
        #if EXACT_LINK_NATIVE_VIEWS
        ExactNativeViews.install()
        #endif
    }
}
