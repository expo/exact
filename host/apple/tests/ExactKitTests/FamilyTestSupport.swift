@testable import ExactKit
@testable import ExactImages
@testable import ExactCollections

// The built-in family implementations behind the core's link slots (LLP 1047.001 D10), for tests that read their internals.

extension ExactSession {
    /// The session's image loader as the built-in implementation, for tests
    /// that read its internals (`ExactImages` installs it).
    var loader: RasterLoader { rasters as! RasterLoader }
}

extension Presenter {
    /// The built-in windowed lists (`ListLink.make`'s default).
    var listHost: CollectionHost { collections as! CollectionHost }
}

#if os(iOS) || os(tvOS)
extension Presenter {
    /// The built-in swipe actions and row cells (`SwipeActionsLink`'s defaults), for tests that read their internals.
    var swipeHost: SwipeActionsHost { swipeActions as! SwipeActionsHost }
    var rowCellsHost: RowCells { rowCells as! RowCells }
}
#endif

extension Presenter {
    /// The built-in segment host (`ControlLink.makeSegments`' default).
    var segmentHost: SegmentHost { segments as! SegmentHost }
}

#if os(macOS)
extension ExactSession {
    /// The built-in content regions (`ContentRegionLink.make`'s default).
    var regionHost: RegionController { regions as! RegionController }
}
#endif

extension ExactSession {
    /// The built-in web views (`MediaLink.makeWeb`'s default).
    var webHost: WebViews { webviews as! WebViews }
}
