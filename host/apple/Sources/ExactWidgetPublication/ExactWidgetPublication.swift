// The widgets family (LLP 1047.001 D10, the roster's `widgets`; LLP 1126 D8):
// the running app's publication of its widget and Live Activity entries. An
// app's composition links it when it has an entry, or when its plan sends
// Live Activities, whose sends name the published generation.
import Foundation
import ExactCore
import ExactKit

public enum ExactWidgetPublication {
    /// Set `WidgetLink`'s slot, before any session is made.
    public static func install() {
        WidgetLink.publication = Publication()
        WidgetPlacements.install()
    }
}

private struct Publication: WidgetPublishing {
    func publishAfterFirstPixel(_ assets: AssetResolver) { WidgetContainer.publishAfterFirstPixel(assets) }
    func observeStore(root: URL?, appID: String?, kinds: [String]?, foreground: [Notification.Name],
                      needsRefresh: @escaping () -> Bool, changed: @escaping (Bool, @escaping (Bool) -> Void) -> Void) -> AnyObject? {
        WidgetContainer.observeStore(root: root, appID: appID, kinds: kinds, foreground: foreground, needsRefresh: needsRefresh, changed: changed)
    }
    @MainActor func reloadAfterStoreChange(kinds: [String]?) { WidgetContainer.reloadAfterStoreChange(kinds: kinds) }
    var publicationFailure: String? { WidgetContainer.publicationFailure }
}
