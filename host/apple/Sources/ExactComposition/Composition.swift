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
#if EXACT_LINK_PICKER
import ExactPickers
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
#if EXACT_LINK_FLIGHTS
import ExactFlights
#endif
#if EXACT_LINK_STICKY
import ExactSticky
#endif
#if EXACT_LINK_IMAGES
import ExactImages
#endif
#if EXACT_LINK_NATIVE_VIEWS
import ExactNativeViews
#endif
#if EXACT_LINK_CONTROLS
import ExactControls
#endif
#if EXACT_LINK_NOTIFICATIONS
import ExactNotifications
#endif
#if EXACT_LINK_HOST_EVENTS
import ExactHostEventMailbox
#endif
#if EXACT_LINK_ACTIVITIES
import ExactActivities
#endif
#if EXACT_LINK_WAKES
import ExactWakeCarriers
#endif
#if EXACT_LINK_GRADIENTS
import ExactGradients
#endif
#if EXACT_LINK_TRACES
import ExactTraces
#endif
#if EXACT_LINK_WAKE_LOCK
import ExactWakeLock
#endif
#if EXACT_LINK_CONTENT_REGIONS
import ExactContentRegions
#endif
#if EXACT_LINK_VIDEO
import ExactVideo
#endif
#if EXACT_LINK_WEB_VIEWS
import ExactWebViews
#endif
#if EXACT_LINK_SOUND
import ExactSound
#endif
#if EXACT_LINK_MIDI
import ExactMIDI
#endif
#if EXACT_LINK_ANIMATIONS
import ExactAnimations
#endif
#if EXACT_LINK_NAVIGATION
import ExactNavigation
#endif
#if EXACT_LINK_TEXT_SELECTION
import ExactTextSelection
#endif
#if EXACT_LINK_WIDGETS
import ExactWidgetPublication
#endif
#if EXACT_LINK_TOOLS
import ExactToolDoor
#endif
#if EXACT_LINK_CONTENT_INDEX
import ExactIndexMirror
#endif
#if EXACT_LINK_DIALOGS
import ExactDialogs
#endif
#if EXACT_LINK_SHARE
import ExactSharing
#endif
#if EXACT_LINK_DRAG_OUT
import ExactDragOut
#endif
#if EXACT_LINK_DRAG_IN && os(iOS)
import ExactDragIn
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
        #if EXACT_LINK_PICKER
        ExactPickers.install()
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
        #if EXACT_LINK_FLIGHTS
        ExactFlights.install()
        #endif
        #if EXACT_LINK_STICKY
        ExactSticky.install()
        #endif
        #if EXACT_LINK_IMAGES
        ExactImages.install()
        #endif
        #if EXACT_LINK_NATIVE_VIEWS
        ExactNativeViews.install()
        #endif
        #if EXACT_LINK_CONTROLS
        ExactControls.install()
        #endif
        #if EXACT_LINK_NOTIFICATIONS
        ExactNotifications.install()
        #endif
        #if EXACT_LINK_HOST_EVENTS
        ExactHostEventMailbox.install()
        #endif
        #if EXACT_LINK_ACTIVITIES
        ExactActivities.install()
        #endif
        #if EXACT_LINK_WAKES
        ExactWakeCarriers.install()
        #endif
        #if EXACT_LINK_GRADIENTS
        ExactGradients.install()
        #endif
        #if EXACT_LINK_TRACES
        ExactTraces.install()
        #endif
        #if EXACT_LINK_WAKE_LOCK
        ExactWakeLock.install()
        #endif
        #if EXACT_LINK_CONTENT_REGIONS
        ExactContentRegions.install()
        #endif
        #if EXACT_LINK_VIDEO
        ExactVideo.install()
        #endif
        #if EXACT_LINK_WEB_VIEWS
        ExactWebViews.install()
        #endif
        #if EXACT_LINK_SOUND
        ExactSound.install()
        #endif
        #if EXACT_LINK_MIDI
        ExactMIDI.install()
        #endif
        #if EXACT_LINK_ANIMATIONS
        ExactAnimations.install()
        #endif
        #if EXACT_LINK_NAVIGATION
        ExactNavigation.install()
        #endif
        #if EXACT_LINK_TEXT_SELECTION
        ExactTextSelection.install()
        #endif
        #if EXACT_LINK_WIDGETS
        ExactWidgetPublication.install()
        #endif
        #if EXACT_LINK_TOOLS
        ExactToolDoor.install()
        #endif
        #if EXACT_LINK_CONTENT_INDEX
        ExactIndexMirror.install()
        #endif
        #if EXACT_LINK_DIALOGS
        ExactDialogs.install()
        #endif
        #if EXACT_LINK_SHARE
        ExactSharing.install()
        #endif
        #if EXACT_LINK_DRAG_OUT
        ExactDragOut.install()
        #endif
        // The Mac takes drops in its core (FileDragMac.swift), and tvOS has no drag
        #if EXACT_LINK_DRAG_IN && os(iOS)
        ExactDragIn.install()
        #endif
    }
}
