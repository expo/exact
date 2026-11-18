// @ref LLP 1047.001 D10 — shared-element flights (LLP 1013.000) are a family
// the composition selects (the roster's pointer and motion row). The core
// reaches them only through what this file declares; without the family the
// presenter holds `NoFlights`, and nothing flies. The slot's default is
// today's flights, so every app still links them; their own target will
// install them instead. How a view shows while it flies (`FlightLook`), a
// flight's clip (`FlightClip`) and, on macOS, its slot (`FlightSlot`) are the
// core's: its image, grid, paint order and child order passes read them.
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The look an arriver shows while it flies: where its image is drawn,
/// in its own bounds (`applyImageLayer` defers to it).
package struct FlightLook {
    package var image: CGRect
    /// The leaver's decoded image, drawn while the arriver's own is still
    /// loading: a new node's raster lands a turn or more after the commit
    /// that hides the leaver, and a flight drawing nothing until then showed
    /// no photo at all for a frame or two on a device (LLP 1013.000 D4).
    package var stand: (any ImageLease)? = nil
    /// A view scaled whole in its clip (`FlightScale`): `applyTransform`
    /// keeps this scale while it flies, as a style (UIKit) or a layout
    /// (AppKit) would otherwise undo it.
    package var scale: CGFloat? = nil
    package init(image: CGRect, stand: (any ImageLease)? = nil, scale: CGFloat? = nil) {
        self.image = image; self.stand = stand; self.scale = scale
    }
}

#if os(macOS)
/// A flight's clip: the shown box of a view flying scaled (D4.4), which
/// paint order ranks as the view it holds.
package final class FlightClip: NSView {
    package override var isFlipped: Bool { true }
    package override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
/// A flying view's empty place in its parent, which child order keeps among
/// the nodes it orders.
package final class FlightSlot: NSView {
    package override var isFlipped: Bool { true }
    package override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
#else
/// A flight's clip: the shown box of a view flying scaled (D4.4), which
/// paint order ranks as the view it holds.
package final class FlightClip: UIView {
    package override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? { nil }
}
#endif

/// What the core asks of a presenter's flights.
package protocol FlightHosting: AnyObject {
    var isEmpty: Bool { get }
    func begin(_ op: BatchOp)
    func land(_ id: UInt32)
    func present(_ id: UInt32, _ progress: CGFloat)
    func forget(_ id: UInt32)
    /// After the batch: lift new flights, show every flight at its progress.
    func batchApplied()
    func landInside(_ view: PlatformView)
    func reset()
    func isFlying(_ view: NodeView) -> Bool
    /// Each flight's slot placed on the pixel grid.
    func regridSlots()
    /// The slot a flying `view` lands in, while it flies.
    func slot(of view: NodeView) -> PlatformView?
    /// Whether `view` is a flight's container or slot (the inspection walk).
    func owns(_ view: PlatformView) -> Bool
    #if os(macOS)
    func frame(_ id: UInt32, _ frame: NSRect) -> Bool
    #else
    func frame(_ op: BatchOp) -> Bool
    #endif
}

/// No flights: nothing lifts, every frame lands where laid out.
package final class NoFlights: FlightHosting {
    package init() {}
    package var isEmpty: Bool { true }
    package func begin(_ op: BatchOp) {}
    package func land(_ id: UInt32) {}
    package func present(_ id: UInt32, _ progress: CGFloat) {}
    package func forget(_ id: UInt32) {}
    package func batchApplied() {}
    package func landInside(_ view: PlatformView) {}
    package func reset() {}
    package func isFlying(_ view: NodeView) -> Bool { false }
    package func regridSlots() {}
    package func slot(of view: NodeView) -> PlatformView? { nil }
    package func owns(_ view: PlatformView) -> Bool { false }
    #if os(macOS)
    package func frame(_ id: UInt32, _ frame: NSRect) -> Bool { false }
    #else
    package func frame(_ op: BatchOp) -> Bool { false }
    #endif
}

/// The slots are set before any session is made, and read on the main thread.
package enum FlightLink {
    /// A presenter's flights.
    package static var make: ((Presenter) -> any FlightHosting)? = { BuiltInFlights($0) }
}
