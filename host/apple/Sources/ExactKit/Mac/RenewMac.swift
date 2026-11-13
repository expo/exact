// @ref LLP 1078 — a virtualized list's row that leaves the window is rebound
// to the item the window needs next: the runner keeps its views and kernel
// nodes, writes only the props and styles that differ for the new item, and
// says `renew` for each of its views before them. A rebound row must be
// indistinguishable from a freshly mounted one, so the presenter drops here
// what it keeps by view beyond its props and style, as a new view has none
// of it:
// - input in progress: a press (the mouse's, the table's, a bubbled one),
//   the held pointer, a swipe, a drag, a hold the driver set, the pressed
//   paint and its ease; the `:hover` and `:focus-visible` looks;
// - the hover path: the view leaves it without a `leave`, as the old item's
//   node is gone; the next display frame's hit test (`followPointer`, which
//   the batch schedules) gives the new item its `enter` if the pointer is
//   over it;
// - presentation is not reset: it is the motion engine's, which keeps a
//   renewed node at rest with the values it shows and presents what the new
//   item changes, and forgets a moving one and presents it whole, as new;
// - a nested scroller's offset, back at its start without a `scroll` or
//   `scrollend` event, and what it kept of a scroll (anchors, a pending
//   offset, a collapsed one, a smooth scroll, the snap area it rested on, a
//   wheel's snap waiting and a snap's ease);
// - a picture: kept when the new item names the same source (a new view
//   would find it decoded), else dropped with its natural size the moment
//   the props name another (`renewSource`), never shown under the new item,
//   and a picture-in-picture window it floats in (closed, as at a destroy);
// - an `svg`'s scene and a box's CSS animations (the Rust host sends them
//   again whole), a flight (landed), a live region's last text;
// - the text selection's place for its paragraphs, an element hatch (it
//   hears `ended`, and `built` for the new item), and the appearance the
//   session last heard for the view's paint motion.
// The runner never rebinds a row holding focus, a press or a reorder's
// source, an editor, a control, a native, canvas, web or video view, or an
// inner list (LLP 1078 §2.3): none of their state is reset here. While
// VoiceOver or Switch Control runs no row is rebound: its cursor stays on the
// element it was on, never on a view that now shows another item, as on iOS
// (NodePoolIOS.swift).
import Foundation
import ExactCore
#if os(macOS)
import AppKit
#endif

/// Whether a session's runner rebinds list rows (LLP 1078). The Mac's
/// presenter resets what it keeps by view on `renew`; UIKit's pools its
/// views instead (NodePoolIOS.swift) and never asks. `EXACT_ROW_REUSE=0`
/// builds every row anew, as the Linux and Android hosts take it, to compare.
enum RowReuse {
    static func adopt(_ session: ExactSession) {
        #if os(macOS)
        guard ExactEnv.environment["EXACT_ROW_REUSE"] != "0" else { return }
        let workspace = NSWorkspace.shared
        let apply = { [weak session] in
            guard let session else { return }
            session.runtime.useRowReuse(!workspace.isVoiceOverEnabled && !workspace.isSwitchControlEnabled)
        }
        apply()
        session.presenter.rowReuseWatch = [
            workspace.observe(\.isVoiceOverEnabled) { _, _ in DispatchQueue.main.async(execute: apply) },
            workspace.observe(\.isSwitchControlEnabled) { _, _ in DispatchQueue.main.async(execute: apply) },
        ]
        #endif
    }
}

#if os(macOS)
extension Presenter {
    /// A `renew` op: `id`'s view now shows another list item.
    func renew(_ id: UInt32) {
        guard let view = views[id] else { return }
        renewing.insert(id)
        if let press = mouseChain.bubbledPress { press.cancel(in: view) }
        if pointerHeld == id { pointerHeld = nil }
        pointerPath.removeAll { $0 == id }
        mouseSwipe.retire(id)
        mouseLayoutPan.retire(id)
        mouseDrags.height.retire(id)
        mouseDrags.transform.retire(id)
        mouseDrags.reorder.retire(id)
        tables.renewed(view)
        if hoveredNodes.contains(where: { $0 === view }) {
            hoveredNodes.removeAll { $0 === view }
            hovered = hoveredNodes.first
        }
        lookHovered.removeAll { $0.view === view || $0.view == nil }
        if let run = hoveredInline, inlineOwners[run]?.owner == id { hoveredInline = nil }
        selection.renewed(view)
        elements.destroyed(id)
        elements.created(view)
        svg.forget(id)
        if let flight = flights[id] { landFlight(flight) }
        // The old item's image left the document: its floating window closes, as at a destroy.
        PictureInPictureHost.gone(view)
        if let session, session.viewDark.removeValue(forKey: id) != nil { session.noteAppearance(view) }
        view.renew()
    }

    /// The batch's `renew` ops are applied, and the props after them.
    func renewsApplied() { renewing.removeAll() }
}

extension NodeView {
    /// Props whose new value changes nothing a view paints. A row rebound to
    /// another item writes these on its wrapper and row (its key, its place
    /// in the set, a test id): redrawing those boxes for them was AppKit's
    /// display of two layers a row that showed what they showed before.
    static let unpaintedProps: Set<String> = ["testId", "listItemKey", "accessibilityPosInSet", "accessibilitySetSize"]
    /// Whether a props op may change what the view paints: a prop that goes
    /// is taken for one that may have.
    static func paints(set: [String: String], clear: [String]) -> Bool {
        !clear.isEmpty || set.keys.contains { !unpaintedProps.contains($0) }
    }

    /// What this view keeps beyond its props and style, as a new view has it.
    fileprivate func renew() {
        holdGone()
        lookState(hovered: false, focus: false)
        if pressed { pressed = false }
        keyboardPressed = false; contextClicked = false; inlinePressed = nil; svgPressed = nil
        // The engine's style with its looks stays this view's: the new item
        // restyles it only where its style differs.
        let base = pressPaint.lookBase
        press = PressFeedback(); dropPressPaint()
        pressPaint.lookBase = base
        refreshPressOpacity()
        applyTransform()
        liveText = nil
        renewScroll()
    }

    /// A nested scroller back at its start, as a new one is, without the
    /// `scroll` and `scrollend` its move would send the new item.
    private func renewScroll() {
        guard let sv = scroll else { return }
        cancelElementScroll()
        scrollEnd?.reset()
        pendingScrollTop = nil; pendingScrollLeft = nil
        beforeLayoutScroll = nil; hiddenScroll = nil; followedScroll = nil; scrollAnchor = nil
        lastScrollEvent = .zero
        // Its snapping is a new scroller's: rested on no area (a mandatory
        // one would follow the old item's), no wheel's snap waiting, no ease.
        sv.snapWheel?.work.cancel(); sv.snapWheel = nil
        sv.snapRested = [ScrollSnap.Rested(), ScrollSnap.Rested()]
        sv.snapDropsMomentum = false; sv.wheelAt = 0
        let clip = sv.contentView, easing = sv.snapSettling
        sv.snapSettling = false
        guard clip.bounds.origin != .zero || easing else { return }
        let center = NotificationCenter.default
        center.removeObserver(self, name: NSView.boundsDidChangeNotification, object: clip)
        if easing {
            // A snap's ease is an animation of the bounds: one of no length replaces it.
            NSAnimationContext.runAnimationGroup { context in context.duration = 0; clip.animator().setBoundsOrigin(.zero) }
        }
        clip.scroll(to: .zero)
        sv.reflectScrolledClipView(clip)
        center.addObserver(self, selector: #selector(clipScrolled), name: NSView.boundsDidChangeNotification, object: clip)
        presenter?.onScrolled?(id, 0, 0)
    }

    /// The props name another picture than the one this view shows: a view
    /// the batch renewed shows none until the new one loads, as a new view
    /// does, where any other keeps the old one meanwhile (`loadImage`).
    func renewSource() {
        guard let presenter, presenter.renewing.contains(id), raster != nil || image != nil || hasSymbolLeaf else { return }
        presenter.session?.rasters.cancel(id)
        raster = nil; image = nil
        clearSymbol()
        applyImageLayer()
        presenter.intrinsic(id, nil)
    }
}
#endif
