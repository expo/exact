// @ref LLP 1047.001 D10 — a windowed list (the roster's `Collections`) is a
// family the composition selects: its snapshot, fills, feedback and travel.
// The core reaches it only through what this file declares. The presenter
// always holds an object: without the family it holds `NoLists`, which owns
// no list and asks for nothing, so a plan with no list pays one call per
// hook. The family's target, `ExactCollections`, installs its host. Drag
// (`ExactDrag`) holds a list's pointer through the same protocol. ExactCore
// reaches the family's feedback export through `CoreExports`.
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// What the core asks of a presenter's windowed lists.
package protocol ListHosting: AnyObject {
    /// Whether `view` is a list this family windows, and their ids.
    func owns(_ view: UInt32) -> Bool
    var listIDs: [UInt32] { get }
    /// The snapshot the runner last sent for list `id`.
    func snapshot(_ id: UInt32) -> CollectionSnapshot?
    /// The list owning `descendant`, if any.
    func owningCollection(_ descendant: UInt32?) -> UInt32?
    /// List `id`'s scroll offset along its main axis, as the runner reads it.
    func mainOffset(_ id: UInt32) -> Double?
    /// The row wrapper at or above `descendant`: what a reorder ghost snapshots.
    func wrapper(of descendant: UInt32) -> NodeView?

    // The batch.
    func beginBatch(_ batch: Batch)
    func endBatch()
    func reset()
    func settle()
    func dataReady()
    func pinsChanged()
    func flushIfNeeded()

    // Fills.
    var fillPending: Set<UInt32> { get }
    var glimpses: Set<UInt32> { get }
    var fillLatency: [UInt32: Double] { get }
    @discardableResult func fillSlice(_ view: UInt32, limit: UInt32, glimpse: Bool) -> Int
    func fixedSpans(_ id: UInt32) -> [(first: Bool, last: Bool, start: Double, end: Double)]
    /// Whether list `id`'s mounted rows cover its port, and how many rows cover `ahead` points past it.
    func covers(_ id: UInt32) -> Bool
    func rowsToCover(_ id: UInt32, ahead: CGFloat) -> UInt32

    // Travel and interaction.
    var animating: Set<UInt32> { get }
    var correcting: Bool { get }
    var startOwed: Set<UInt32> { get }
    func animationTarget(_ id: UInt32) -> CGPoint?
    func drivesOffset(_ id: UInt32) -> Bool
    func userIntent(_ view: UInt32, travel: Bool)
    func changed(_ view: UInt32, user: Bool, sequenceAdvanced: Bool)
    func animationEnded(_ view: UInt32, dragging: Bool, atTarget: Bool?)
    func stopAnimation(_ view: UInt32)
    func landed(_ view: UInt32, at now: Double)
    func advanceScrollSequence(_ view: UInt32)
    func ancestorMoving(_ view: UInt32) -> Bool
    func gestureBegan(_ view: UInt32, at time: TimeInterval?)
    func gestureBegan(around view: @autoclosure () -> PlatformView?, at time: TimeInterval?)
    func pointer(_ view: UInt32?)
    func holdPointer(_ view: UInt32) -> UInt64
    func releaseInteractionLater(ifCurrent expected: UInt64?)

    // What the presenter tells it to do.
    var requestFill: (() -> Void)? { get set }
    var motion: ((UInt32) -> Double?)? { get set }
    var glimpse: ((UInt32) -> UInt32?)? { get set }
    var glimpsed: ((UInt32, TimeInterval, Int) -> Void)? { get set }
    var rescued: (() -> Void)? { get set }
    var onFill: ((UInt32, Data) -> Void)? { get set }
    var onFeedback: ((Data) -> Void)? { get set }
    var drain: (() -> Void)? { get set }
    var filling: (() -> Bool)? { get set }

    #if os(iOS) || os(tvOS)
    /// List `id`'s scroll ended: a travelled window returns to the ordinary one.
    func rested(_ id: UInt32)
    func readerMoves(_ id: UInt32)
    func watchContacts(in port: UIView) -> UIGestureRecognizer
    func offsetsMoved(_ id: UInt32, by dy: CGFloat)
    /// What list `id` draws through of its scroll view's insets, on each edge.
    func drawnThrough(_ id: UInt32) -> UIEdgeInsets?
    func lastRequests() -> [UInt32: UInt64]
    func lastRequest(_ id: UInt32) -> UInt64
    #else
    /// Whether `scroll`'s clip view may move: a list's scroller knob drag keeps the offset the reader saw.
    func admitsScroll(_ scroll: NSScrollView) -> Bool
    func pointerDown(_ view: UInt32?, event: NSEvent)
    func observeKnobDrags()
    func limitPrepared()
    func heard(_ event: NSEvent)
    #endif
}

extension ListHosting {
    func userIntent(_ view: UInt32) { userIntent(view, travel: false) }
    @discardableResult func fillSlice(_ view: UInt32, limit: UInt32) -> Int { fillSlice(view, limit: limit, glimpse: false) }
    func changed(_ view: UInt32, user: Bool = false) { changed(view, user: user, sequenceAdvanced: false) }
    func animationEnded(_ view: UInt32, dragging: Bool = false) { animationEnded(view, dragging: dragging, atTarget: nil) }
    func animationEnded(_ view: UInt32, atTarget: Bool?) { animationEnded(view, dragging: false, atTarget: atTarget) }
    func gestureBegan(_ view: UInt32) { gestureBegan(view, at: nil) }
    func gestureBegan(around view: @autoclosure () -> PlatformView?) { gestureBegan(around: view(), at: nil) }
    package func releaseInteractionLater() { releaseInteractionLater(ifCurrent: nil) }
}

/// No windowed lists: nothing is owned, nothing is asked.
final class NoLists: ListHosting {
    func owns(_ view: UInt32) -> Bool { false }
    var listIDs: [UInt32] { [] }
    func snapshot(_ id: UInt32) -> CollectionSnapshot? { nil }
    func owningCollection(_ descendant: UInt32?) -> UInt32? { nil }
    func mainOffset(_ id: UInt32) -> Double? { nil }
    func wrapper(of descendant: UInt32) -> NodeView? { nil }
    func beginBatch(_ batch: Batch) {}
    func endBatch() {}
    func reset() {}
    func settle() {}
    func dataReady() {}
    func pinsChanged() {}
    func flushIfNeeded() {}
    var fillPending: Set<UInt32> { [] }
    var glimpses: Set<UInt32> { [] }
    var fillLatency: [UInt32: Double] { [:] }
    func fillSlice(_ view: UInt32, limit: UInt32, glimpse: Bool) -> Int { 0 }
    func fixedSpans(_ id: UInt32) -> [(first: Bool, last: Bool, start: Double, end: Double)] { [] }
    package func covers(_ id: UInt32) -> Bool { false }
    func rowsToCover(_ id: UInt32, ahead: CGFloat) -> UInt32 { 0 }
    var animating: Set<UInt32> { [] }
    var correcting: Bool { false }
    var startOwed: Set<UInt32> { [] }
    func animationTarget(_ id: UInt32) -> CGPoint? { nil }
    func drivesOffset(_ id: UInt32) -> Bool { false }
    func userIntent(_ view: UInt32, travel: Bool) {}
    func changed(_ view: UInt32, user: Bool, sequenceAdvanced: Bool) {}
    func animationEnded(_ view: UInt32, dragging: Bool, atTarget: Bool?) {}
    func stopAnimation(_ view: UInt32) {}
    func landed(_ view: UInt32, at now: Double) {}
    func advanceScrollSequence(_ view: UInt32) {}
    func ancestorMoving(_ view: UInt32) -> Bool { false }
    func gestureBegan(_ view: UInt32, at time: TimeInterval?) {}
    func gestureBegan(around view: @autoclosure () -> PlatformView?, at time: TimeInterval?) {}
    func pointer(_ view: UInt32?) {}
    func holdPointer(_ view: UInt32) -> UInt64 { 0 }
    func releaseInteractionLater(ifCurrent expected: UInt64?) {}
    var requestFill: (() -> Void)?
    var motion: ((UInt32) -> Double?)?
    var glimpse: ((UInt32) -> UInt32?)?
    var glimpsed: ((UInt32, TimeInterval, Int) -> Void)?
    var rescued: (() -> Void)?
    var onFill: ((UInt32, Data) -> Void)?
    var onFeedback: ((Data) -> Void)?
    var drain: (() -> Void)?
    var filling: (() -> Bool)?
    #if os(iOS) || os(tvOS)
    func rested(_ id: UInt32) {}
    func readerMoves(_ id: UInt32) {}
    func watchContacts(in port: UIView) -> UIGestureRecognizer { UIGestureRecognizer() }
    func offsetsMoved(_ id: UInt32, by dy: CGFloat) {}
    func drawnThrough(_ id: UInt32) -> UIEdgeInsets? { nil }
    func lastRequests() -> [UInt32: UInt64] { [:] }
    func lastRequest(_ id: UInt32) -> UInt64 { 0 }
    #else
    func admitsScroll(_ scroll: NSScrollView) -> Bool { true }
    func pointerDown(_ view: UInt32?, event: NSEvent) {}
    func observeKnobDrags() {}
    func limitPrepared() {}
    func heard(_ event: NSEvent) {}
    #endif
}

#if os(iOS) || os(tvOS)
/// What the family asks of the presenter's scroll pump and element reveals.
extension Presenter {
    /// List `id`'s travel is over: the pump stops leading it.
    package func forgetTravel(_ id: UInt32) { scrollPump.forgetTravel(id) }
    /// How far ahead, in seconds, list `id`'s window builds while it moves.
    package func leadTime(_ id: UInt32) -> Double { scrollPump.leadTime(id) }
    /// A correction moved list `id`'s port by `delta`.
    package func shifted(_ id: UInt32, by delta: CGFloat) { scrollPump.shifted(id, by: delta) }
    /// Whether an element reveal moves `scroll`; if so it heads for where its target now is.
    package func retargetReveal(_ scroll: UIScrollView) -> Bool {
        guard elementScrolls.revealing(scroll) != nil else { return false }
        elementScrolls.retarget(scroll)
        return true
    }
}
#endif

/// The slots are set before any session is made, and read on the main thread.
package enum ListLink {
    /// A presenter's windowed lists.
    package static var make: ((Presenter) -> any ListHosting)?
}
