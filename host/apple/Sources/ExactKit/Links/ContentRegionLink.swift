// @ref LLP 1047.001 D10 — content regions (long documents a worker lays out
// and rasters, cut list C13) are a family the composition selects by a
// plan's region use. On macOS the session reaches its region controller
// through what this file declares; without the family it holds
// `NoContentRegions`, and no region is prepared. iOS has no region
// controller yet. The slot's default is today's controller.
#if os(macOS)
import AppKit

/// What a session asks of its content regions around each batch.
package protocol ContentRegionHosting: AnyObject {
    /// Before the batch is applied: the regions it asks for or retires.
    func prepare(_ batch: Batch)
    /// After it: what has finished is told to the runtime.
    func flush()
    /// The plan restarted.
    func reset()
    /// Whether `node` is a region's owner, whose text the region draws.
    func owns(_ node: NodeView) -> Bool
    /// An owner's geometry or appearance changed.
    func geometryChanged()
    /// Whether `view` is this session's region surface (the inspection walk).
    func ownsSurface(_ view: NSView) -> Bool
    /// What the agent reports of the regions.
    var diagnostics: [String: Any] { get }
}

/// No content regions.
package final class NoContentRegions: ContentRegionHosting {
    package init() {}
    package func prepare(_ batch: Batch) {}
    package func flush() {}
    package func reset() {}
    package func owns(_ node: NodeView) -> Bool { false }
    package func geometryChanged() {}
    package func ownsSurface(_ view: NSView) -> Bool { false }
    package var diagnostics: [String: Any] { [:] }
}

/// The slots are set before any session is made, and read on the main thread.
package enum ContentRegionLink {
    /// A session's content regions.
    package static var make: ((ExactSession) -> any ContentRegionHosting)? = { RegionController($0) }
    /// Whether `view` is any session's region surface: the mouse shows an
    /// I-beam over one.
    package static var isSurface: ((NSView) -> Bool)? = { $0 is RegionSurfaceMac }
}
#endif
