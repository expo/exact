// The author's pressed paint (LLP 1061 D7), on both platforms: while the
// press `pressed` tracks holds a node (the bit `-exact-press-scale` and the
// platform's dim use), it shows its `-exact-press-opacity` as a fraction of
// its own opacity, its active `background-color` in place of its
// background, and its active `color` as its text colour, which its
// subtree takes as CSS's `color` is taken (a descendant with a colour of its
// own keeps it). Eased in and back on the scale's curve and duration, at once
// under the agent's clock; a release before the press was ever on screen
// waits until it has eased in, as the scale's does. No runner round trip.
//
// The opacity is an additive Core Animation on `opacity`, as the platform's
// dim is (PressFeedback.swift), so it composes with every engine write. The
// colours stand in the views' styles for the keys they replace, the engine's
// values kept beside them (`pressPainted`, which an engine write passes
// through), and the change cross-fades.
//
// A node's `:active` (LLP 1121 D8) lowers to these rows, its opacity to
// `press_opacity_to`: an absolute target the press eases to from what shows
// (a hover look's included), not a share. Its `:hover` and `:focus-visible`
// looks stand in under the press while the pointer hovers it (the Mac, an
// iPad's pointer) or its focus is visible, at once, their opacity an additive
// layer opacity (`looked`). A look's `color` is its text colour and its
// subtree's, lent below as the press's colour is (`inkLooks`), under the
// press's own.
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// One view's part in a press's paint.
struct PressPaint {
    /// Whether this node's own paint is shown (or its release is held).
    var down = false
    /// When the paint last went down.
    var at: CFTimeInterval = 0
    /// A release that came before the press was on screen, held.
    var releaseHeld = false
    /// The opacity's progress, 0 at rest and 1 held, on the scale's curve.
    var ease = PressFeedback(from: 0, to: 0, start: 0)
    /// How much opacity a full press takes away: the node's own at the
    /// press, less its `-exact-press-opacity` share of it.
    var depth: Float = 0
    /// The views this press shows in its colours, itself included.
    var inked: [WeakNodeView] = []
    /// The keys a press (this node's or an ancestor's) replaces in this
    /// view's style, with the values the engine gave them; nil was absent.
    var rest: [String: BatchValue?] = [:]
    var overlay: NodeStyle = [:]
    /// The pressed node whose colour this view shows, while it shows one.
    var inkedBy = WeakNodeView()
    /// The engine's style before the hover and focus looks (`looked`).
    var lookBase: NodeStyle?
    /// Whether the pointer hovers it, for a `:hover` look (the Mac).
    var hovered = false
    /// Whether its focus is visible, for a `:focus-visible` look.
    var focusShown = false
    /// The opacity the looks that hold show, if one names it.
    var lookOpacity: Float?
    var lookMotionStates: UInt8 = 0
    /// The text colour the looks that hold show, if one names it, and the
    /// one the views below were last given (`inkLooks`).
    var lookInk: BatchValue?
    var lookInkLent: BatchValue?
    /// Whether the press `pressed` tracks has left the box (`pressFollows`).
    var outside = false
    /// As a node that was never pressed (nor looked) holds it.
    var atRest: Bool {
        !down && at == 0 && !releaseHeld && ease.rests(at: 0) && depth == 0 && inked.isEmpty && rest.isEmpty
            && overlay.isEmpty && inkedBy.view == nil && lookBase == nil && !hovered && !focusShown && lookOpacity == nil
            && lookInk == nil && lookInkLent == nil && !outside
    }
}

/// A view a press recoloured, held weakly: it may be destroyed or taken
/// for another node while held.
struct WeakNodeView { weak var view: NodeView? }

extension NodeView {
    /// Whether the text colour it shows is its own: written (a moving one
    /// included, `style.rs`), its own press's (LLP 1061 D7) or a look's that
    /// holds (LLP 1121 D8), not one an ancestor lends it. A platform field
    /// takes no other (LLP 1115 D4).
    var showsOwnTextColor: Bool {
        wrote("text_color") || (pressPaint.down && style["press_color"] != nil) || pressPaint.lookInk != nil
    }
    package var pressIsDown: Bool { holds("active") || pressed && !pressPaint.outside }
    /// The rows of the pressed paint (LLP 1061 D7).
    static let pressPaintRows = ["press_opacity", "press_opacity_to", "press_background_color", "press_color"]
    /// Whether the author wrote this node's pressed look: a press scale or
    /// any of the paint (any value, 1 included). It replaces the platform's
    /// unsaid press (LLP 1115.000 D6).
    var authorsPress: Bool { style["press_scale"] != nil || Self.pressPaintRows.contains { style[$0] != nil } }

    /// State look `state` (`hover`, `focus_visible`) style `s` carries (LLP 1121 D8).
    static func look(_ state: String, in s: NodeStyle) -> NodeStyle? {
        guard case .object(let looks)? = s["looks"], case .object(let look)? = looks[state] else { return nil }
        return look
    }

    /// How much opacity a full press takes away, by style `s`, from what
    /// shows (a look's, else the node's own): to `:active`'s absolute target,
    /// or the pressed paint's share of it, as the web and Linux take it.
    private func pressDepth(_ s: NodeStyle) -> Float {
        let shown = pressPaint.lookOpacity ?? ownOpacity
        if let to = s["press_opacity_to"]?.number {
            return (shown - Float(min(1, max(0, to)))) * filterOpacityFactor
        }
        return shown * (1 - Float(min(1, max(0, s["press_opacity"]?.number ?? 1)))) * filterOpacityFactor
    }

    /// The looks again, as they hold now: after a change the flags don't
    /// show (the node's `disabled`, a state held or let go).
    func relook() {
        HeldState.propsChanged() // a node disabled, or under a disabled one, holds no state
        refreshLookMotion()
        guard let base = pressPaint.lookBase else { return }
        restyleForPress(pressPainted(looked(base)))
        inkLooks()
        refreshPressOpacity()
    }

    /// AppKit's layer is optional, UIKit's not.
    private var paintLayer: CALayer? { layer }
    var filterOpacityFactor: Float { boxFilter?.opacity ?? 1 }
    /// The engine's opacity before the filter, retained even at `opacity(0)`.
    var ownOpacity: Float {
        #if os(macOS)
        Float(alphaValue)
        #else
        boxFilter?.unfilteredOpacity ?? (layer as? NodeLayer)?.ownOpacity ?? Float(alpha)
        #endif
    }

    /// Show the pressed paint (`down`) or ease it back; `instantly` drops it.
    func paintPress(_ down: Bool, instantly: Bool = false) {
        if down, pressPaint.releaseHeld { pressPaint.releaseHeld = false; return }
        guard down != pressPaint.down else { return }
        guard !down || Self.pressPaintRows.contains(where: { style[$0] != nil }) else { return }
        let now = CACurrentMediaTime()
        let still = instantly || ExactEnv.agentFreezes
        if !down, !still, now - pressPaint.at < PressFeedback.unseen {
            pressPaint.releaseHeld = true
            let at = pressPaint.at
            DispatchQueue.main.asyncAfter(deadline: .now() + PressFeedback.duration) { [weak self] in
                guard let self, pressPaint.releaseHeld, pressPaint.at == at else { return }
                pressPaint.releaseHeld = false
                paintPress(false)
            }
            return
        }
        pressPaint.releaseHeld = false
        pressPaint.down = down
        if down { pressPaint.at = now }
        fadePressOpacity(down, at: now, still: still)
        inkPress(down, still: still)
    }

    private func fadePressOpacity(_ down: Bool, at now: CFTimeInterval, still: Bool) {
        if down { pressPaint.depth = pressDepth(style) }
        // The clock moves whatever the depth, so a depth that comes later
        // takes the press's real progress.
        if still { pressPaint.ease = PressFeedback(from: down ? 1 : 0, to: down ? 1 : 0, start: now) } else { pressPaint.ease.aim(down ? 1 : 0, at: now) }
        installPressFade(pressPaint.depth, still: still)
    }

    /// The press's opacity fade for `depth` as the press's ease describes it
    /// now: the same curve and start, so what shows is `-depth × progress` at
    /// every frame; held at its end once settled (always, on the agent's clock).
    private func installPressFade(_ depth: Float, still: Bool) {
        guard let layer = paintLayer else { return }
        let key = "pressOpacity", e = pressPaint.ease
        let settled = still || e.settled(at: CACurrentMediaTime())
        guard depth != 0, !(settled && e.to == 0) else { layer.removeAnimation(forKey: key); return }
        let fade = CABasicAnimation(keyPath: "opacity")
        fade.isAdditive = true
        fade.fillMode = .both
        if settled {
            // Held: what the dim's hold is (PressFeedback.swift `dimPress`).
            fade.fromValue = -depth * Float(e.to)
            fade.toValue = -depth * Float(e.to)
            fade.duration = .greatestFiniteMagnitude
            fade.isRemovedOnCompletion = false
        } else {
            fade.fromValue = -depth * Float(e.from)
            fade.toValue = -depth * Float(e.to)
            fade.beginTime = layer.convertTime(e.start, from: nil)
            fade.duration = PressFeedback.duration
            fade.timingFunction = PressFeedback.timing
            fade.isRemovedOnCompletion = e.to == 0
        }
        layer.add(fade, forKey: key)
    }

    /// The colours on (`down`) or off: this view's background, and the text
    /// colour of every view below that takes this one's.
    private func inkPress(_ down: Bool, still: Bool) {
        let changed: Bool
        if down {
            // Itself first, so a colour the engine gives it mid-press finds it.
            pressPaint.inked = [WeakNodeView(view: self)]
            let own = Self.pressOverlay(style)
            guard !own.isEmpty else { return }
            standInForPress(own)
            if let ink = own["text_color"] { inkBelow(self, ink: ink, inherited: pressPaint.rest["text_color"] ?? nil) }
            changed = true
        } else {
            changed = !pressPaint.inked.isEmpty
            endInked(pressPaint.inked.compactMap(\.view), released: true)
            pressPaint.inked = []
            // A look that holds lends its colour again.
            pressPaint.lookInkLent = nil
            inkLooks()
        }
        guard changed, !still, let layer = paintLayer else { return }
        let fade = CATransition()
        fade.type = .fade
        fade.duration = PressFeedback.duration
        fade.timingFunction = PressFeedback.timing
        layer.add(fade, forKey: kCATransition)
    }

    /// A CATransition holds the whole subtree's old pixels. End it when a
    /// batch changes that subtree, so stale content cannot fade over the new.
    func endPressCrossFade() { paintLayer?.removeAnimation(forKey: kCATransition) }

    /// The views under `view` whose text colour is the pressed node's: none
    /// written of their own, and either one this press already colours or
    /// the same value as the pressed node's (a control's own colour is not
    /// one it inherits). A view with a colour of its own keeps its subtree's
    /// too.
    private func inkBelow(_ view: PlatformView, ink: BatchValue, inherited: BatchValue?) {
        for child in view.subviews {
            guard let node = child as? NodeView else { inkBelow(child, ink: ink, inherited: inherited); continue }
            // (One an ancestor's press colours now takes this one's, if what it shows beneath is the same; not one
            // showing its own colour, a press's included, which is its subtree's boundary.)
            // A mode boundary (`-exact-mode`, LLP 1121 D7) writes the colour around it again in its own tuple, a
            // value of its own: it takes the colour too, and the views below it that take that value. A control's
            // colour is the platform's, never one it inherits.
            let boundary = node.wrote("mode") && node.kind != "control"
            guard !node.showsOwnTextColor, boundary || node.pressPaint.inkedBy.view === self || node.style["text_color"] == inherited
                || (node.pressPaint.inkedBy.view != nil && node.pressPaint.rest["text_color"] ?? nil == inherited) else { continue }
            let base = node.pressPaint.overlay["text_color"] == nil ? node.style["text_color"] : node.pressPaint.rest["text_color"] ?? nil
            if !pressPaint.inked.contains(where: { $0.view === node }) { pressPaint.inked.append(WeakNodeView(view: node)) }
            node.pressPaint.inkedBy = WeakNodeView(view: self)
            node.standInForPress(["text_color": ink])
            inkBelow(node, ink: ink, inherited: boundary ? base : inherited)
        }
    }

    /// Show `values` in place of this view's own, keeping the engine's.
    private func standInForPress(_ values: NodeStyle) {
        var s = style
        for (key, value) in values {
            if pressPaint.rest[key] == nil { pressPaint.rest[key] = .some(s[key]) }
            s[key] = value
            pressPaint.overlay[key] = value
        }
        restyleForPress(s)
    }

    /// Views this press coloured, back to their own: only those it still
    /// colours (another press may have taken one since); one pressed itself
    /// since (a descendant pressed while this release waited out
    /// `PressFeedback.unseen`) keeps its own press's paint and loses only the
    /// text colour this one lent, where its own press writes none. Its own
    /// press `released` while the press that lent it colour still shows
    /// (held down, or its release waiting): that press colours it, and what
    /// it coloured, again (LLP 1061 D7, stage 9 G4). (Not when the colours
    /// are only reworked under a new style, nor as the view goes.)
    private func endInked(_ views: [NodeView], released: Bool = false) {
        let lender = released ? liveLender() : nil
        for view in views {
            if view === self { endPressStandIn() } else { view.endLent(by: self) }
        }
        lender?.inkAgain()
    }

    /// The colour `pressing` lent this view, back: all it stands in, or only
    /// that colour while its own press paints it (where that writes none).
    private func endLent(by pressing: NodeView) {
        guard pressPaint.inkedBy.view === pressing else { return }
        if !pressPaint.down { endPressStandIn() }
        else if Self.pressOverlay(style)["text_color"] == nil { endPressStandIn(only: ["text_color"]) }
        else { pressPaint.inkedBy = WeakNodeView() } // its own press colours it: nothing of `pressing`'s on it
    }

    /// The nearest node above that would colour this view now (`lent`), no
    /// colour written between.
    private func liveLender() -> NodeView? {
        var view = superview
        while let current = view {
            if let node = current as? NodeView {
                // (A press colour shows over the node's own written one; the written one stops what is above.)
                if node.lent != nil { return node }
                if node.wrote("text_color") { return nil }
            }
            view = current.superview
        }
        return nil
    }

    /// The colour it lends the views below that take its own, and the colour
    /// they take it in place of: its press's while that is down (or its
    /// release waiting), else that of a look that holds.
    private var lent: (ink: BatchValue, inherited: BatchValue?)? {
        if let ink = pressInk { return (ink, pressPaint.rest["text_color"] ?? nil) }
        return pressPaint.lookInk.map { ($0, pressPaint.lookBase?["text_color"]) }
    }

    /// Its press's colour while that is down (or its release waiting).
    private var pressInk: BatchValue? {
        pressPaint.down || pressPaint.releaseHeld ? Self.pressOverlay(style)["text_color"] : nil
    }

    /// The views a batch placed take the colour a node above lends now (a
    /// look's or a press's), as the views already there do: each lender inks
    /// its subtree again, once.
    static func inkPlaced(_ views: some Sequence<NodeView>) {
        var lenders = Set<ObjectIdentifier>()
        for view in views {
            // A colour lent by a node it is no longer below goes, with what it lent below.
            if let by = view.pressPaint.inkedBy.view, !view.isDescendant(of: by) {
                view.endLent(by: by)
                by.endInk(below: view, by: by)
            }
        }
        for view in views where !view.wrote("text_color") {
            guard let lender = view.liveLender(), lenders.insert(ObjectIdentifier(lender)).inserted else { continue }
            lender.inkAgain()
        }
    }

    /// A style that removed this press's colour can expose an ancestor's
    /// held colour, and one that changed its looks lends theirs. Re-ink after
    /// applying the style to avoid re-entering `presented` while it is
    /// building that style.
    func refreshPressInheritance() {
        if pressed || holds("active") { showPress(pressIsDown) }
        inkLooks()
        guard pressPaint.down, !showsOwnTextColor else { return }
        liveLender()?.inkAgain()
    }

    /// Its colour on the views below that take it again (`lent`).
    private func inkAgain() {
        guard let (ink, inherited) = lent else { return }
        inkBelow(self, ink: ink, inherited: inherited)
    }

    /// Its looks' colour on the views below that take its own, as its press
    /// lends its colour, or theirs back once no look gives one (and an
    /// ancestor's that shows, `liveLender`). Under its own press's colour that
    /// one shows; the release lends the look's (`inkPress`).
    private func inkLooks() {
        let ink = pressPaint.lookInk
        guard ink != pressPaint.lookInkLent, pressInk == nil else { return }
        pressPaint.lookInkLent = ink
        if ink != nil { inkAgain(); return }
        // The views it inked, also one a batch moved out from under it since.
        endInk(below: self, by: self)
        for view in pressPaint.inked.compactMap(\.view) where view !== self { view.endLent(by: self) }
        pressPaint.inked.removeAll { $0.view !== self }
        liveLender()?.inkAgain()
    }

    /// The engine's values again (only those of `keys`, when it says).
    func endPressStandIn(only keys: Set<String>? = nil) {
        if let keys {
            var s = style
            for key in keys where pressPaint.overlay[key] != nil {
                if let value = pressPaint.rest[key] ?? nil { s[key] = value } else { s.removeValue(forKey: key) }
                pressPaint.rest[key] = nil
                pressPaint.overlay[key] = nil
            }
            // No ink of its lender's left on it.
            pressPaint.inkedBy = WeakNodeView()
            restyleForPress(s)
            return
        }
        guard !pressPaint.overlay.isEmpty else { pressPaint.inkedBy = WeakNodeView(); return }
        var s = style
        for (key, value) in pressPaint.rest {
            if let value { s[key] = value } else { s.removeValue(forKey: key) }
        }
        pressPaint.rest = [:]
        pressPaint.overlay = [:]
        pressPaint.inkedBy = WeakNodeView()
        restyleForPress(s)
    }

    /// The views below `view` coloured by `pressing`'s press, back to their own.
    private func endInk(below view: PlatformView, by pressing: NodeView) {
        for child in view.subviews {
            if let node = child as? NodeView { node.endLent(by: pressing) }
            endInk(below: child, by: pressing)
        }
    }

    /// The press's opacity again when what it starts from or goes to moves
    /// (the node's own opacity, a look's, style `s`'s press rows): the fade on
    /// the same ease with the new depth; a press opacity no longer written goes
    /// at once. The look's opacity is held again first, a native button's too.
    func refreshPressOpacity(_ s: NodeStyle? = nil) {
        applyLookOpacity()
        if isNativeButton { refreshNativeActivePresentation(); return }
        #if os(iOS)
        refreshDimOpacity()
        #endif
        // Held, easing in or easing out: the fade is always `-depth ×
        // progress` on the press's own ease, so what shows moves with it, as
        // on the web and Linux (`shown + (to − shown) × progress`).
        guard pressPaint.down || pressPaint.ease.factor(at: CACurrentMediaTime()) > 0 else { return }
        let depth = pressDepth(s ?? style)
        guard depth != pressPaint.depth else { return }
        pressPaint.depth = depth
        installPressFade(depth, still: ExactEnv.agentFreezes)
    }

    /// The keys a node's own press shows, from its style's press rows.
    private static func pressOverlay(_ s: NodeStyle) -> NodeStyle {
        var own: NodeStyle = [:]
        if let fill = s["press_background_color"] { own["background_color"] = fill }
        if let ink = s["press_color"] { own["text_color"] = ink }
        return own
    }

    /// The style keys a state look stands in (its opacity is a layer's,
    /// `applyLookOpacity`; its text colour also its subtree's, `inkLooks`).
    static let lookRows = ["background_color", "text_color", "border_color_top", "border_color_right", "border_color_bottom", "border_color_left", "box_shadow"]

    /// An engine style as this view shows it: its `:hover` then its
    /// `:focus-visible` look over it while they hold (none on a disabled
    /// node), then the press's paint (`pressPainted`). The engine's is kept
    /// to show again when they change.
    func presented(_ s: NodeStyle) -> NodeStyle {
        pressPaint.lookBase = s["looks"] == nil ? nil : s
        #if os(iOS)
        lookHover(Self.look("hover", in: s) != nil) // a pointer's hover (LookHoverIOS.swift)
        #endif
        refreshLookMotion()
        let shown = pressPainted(looked(s))
        applyLookOpacity()
        return shown
    }

    private func refreshLookMotion() {
        let states: UInt8 = disabled ? 0 : (pressPaint.hovered || holds("hover") ? 1 : 0) | (pressPaint.focusShown || holds("focus-visible") ? 2 : 0)
        guard states != pressPaint.lookMotionStates else { return }
        pressPaint.lookMotionStates = states
        let view = id
        // A style can be arriving inside a batch. Observe after that batch is applied.
        DispatchQueue.main.async { [weak self] in
            guard let self, id == view, let session = presenter?.session else { return }
            session.whenIdle(bound: true) { [weak self, weak session] in
                guard let self, id == view, let session else { return }
                session.applyUnlessEmpty(session.runtime.lookMotion(view, states: pressPaint.lookMotionStates, now: session.now()))
            }
        }
    }

    private func looked(_ s: NodeStyle) -> NodeStyle {
        pressPaint.lookOpacity = nil
        pressPaint.lookInk = nil
        guard !disabled else { return s }
        var s = s
        // The real state's, or one held beside it (HeldState.swift).
        for (state, on) in [("hover", pressPaint.hovered || holds("hover")), ("focus_visible", pressPaint.focusShown || holds("focus-visible"))] where on {
            guard let look = Self.look(state, in: s) else { continue }
            for key in Self.lookRows {
                // A row the look clears is null: CSS's `unset`, for `color` the colour around (inherited), for the
                // rest their initial value, none (no shadow).
                if case .null? = look[key] {
                    if key == "text_color", let around = nodeSuperview?.style["text_color"] { s[key] = around } else { s.removeValue(forKey: key) }
                } else if let value = look[key] { s[key] = value }
            }
            if let o = look["opacity"]?.number { pressPaint.lookOpacity = Float(min(1, max(0, o))) }
            if look["text_color"] != nil, let ink = s["text_color"] { pressPaint.lookInk = ink }
        }
        return s
    }

    /// The looks' opacity in place of the node's own, at once: an additive
    /// layer opacity, as the press's is, so it composes with every engine write.
    private func applyLookOpacity() {
        guard let layer = paintLayer else { return }
        guard let to = pressPaint.lookOpacity else { layer.removeAnimation(forKey: "lookOpacity"); return }
        let hold = CABasicAnimation(keyPath: "opacity")
        hold.isAdditive = true
        hold.fromValue = (to - ownOpacity) * filterOpacityFactor
        hold.toValue = hold.fromValue
        hold.duration = .greatestFiniteMagnitude
        hold.isRemovedOnCompletion = false
        hold.fillMode = .both
        layer.add(hold, forKey: "lookOpacity")
    }

    /// The pointer came over it or left (`hovered`), or its focus came to
    /// show or went (`focus`): its look shows at once, under the press's.
    func lookState(hovered: Bool? = nil, focus: Bool? = nil) {
        let was = (pressPaint.hovered, pressPaint.focusShown)
        if let hovered { pressPaint.hovered = hovered }
        if let focus { pressPaint.focusShown = focus }
        guard (pressPaint.hovered, pressPaint.focusShown) != was else { return }
        relook()
    }

    /// An engine style for this view while a press paints it: the engine's
    /// new values are kept for the release, and what the press shows is
    /// read again from it. The pressed node's own rows may have changed (a
    /// new colour goes to its subtree again); a descendant that now writes
    /// its own colour keeps it.
    func pressPainted(_ s: NodeStyle) -> NodeStyle {
        // A press easing out reads its depth from the new style too.
        if !pressPaint.down { refreshPressOpacity(s) }
        guard pressPaint.down || !pressPaint.overlay.isEmpty else { return s }
        var s = s
        var overlay = pressPaint.overlay
        if pressPaint.down {
            let lent = Self.pressOverlay(style)["text_color"] == nil ? pressPaint.overlay["text_color"] : nil
            overlay = Self.pressOverlay(s)
            // A colour a press above lends it, where its own press writes none, stays; a colour of its own now
            // ends that lender's colour below it too.
            if overlay["text_color"] == nil, let lent, let lender = pressPaint.inkedBy.view {
                if !Self.writes(s, "text_color"), pressPaint.lookInk == nil { overlay["text_color"] = lent }
                else { endInk(below: self, by: lender); pressPaint.inkedBy = WeakNodeView() }
            }
            refreshPressOpacity(s)
            if overlay["text_color"] != pressPaint.overlay["text_color"] {
                // The views it coloured keep taking its colour; new ones
                // holding its own take it too; the rest go back.
                let before = pressPaint.inked.compactMap(\.view).filter { $0 !== self }
                pressPaint.inked = [WeakNodeView(view: self)]
                if let ink = overlay["text_color"] { inkBelow(self, ink: ink, inherited: s["text_color"]) }
                let kept = Set(pressPaint.inked.compactMap { $0.view.map(ObjectIdentifier.init) })
                endInked(before.filter { !kept.contains(ObjectIdentifier($0)) })
            }
        } else if Self.writes(s, "text_color") || pressPaint.lookInk != nil, overlay["text_color"] != nil {
            // Its own colour now (a look's included), and what is below it takes that one.
            overlay["text_color"] = nil
            if let pressing = pressPaint.inkedBy.view { endInk(below: self, by: pressing); pressPaint.inkedBy = WeakNodeView() }
        }
        for key in pressPaint.overlay.keys where overlay[key] == nil { pressPaint.rest[key] = nil }
        for (key, value) in overlay {
            pressPaint.rest[key] = .some(s[key])
            s[key] = value
        }
        pressPaint.overlay = overlay
        return s
    }

    /// Whether style `s` says its author wrote row `key`, as `wrote` reads a view's own.
    private static func writes(_ s: NodeStyle, _ key: String) -> Bool {
        guard case .array(let rows)? = s["written"] else { return false }
        return rows.contains(.string(key))
    }

    private func restyleForPress(_ s: NodeStyle) {
        applyStyle(s)
        guard isParagraph else { return }
        invalidateText()
        #if os(macOS)
        needsDisplay = true
        #else
        setNeedsDisplay()
        #endif
    }

    /// A view taken for another node keeps nothing of a press, and the
    /// views its press recoloured go back to their own.
    func dropPressPaint() {
        holdGone()
        endInked(pressPaint.inked.compactMap(\.view).filter { $0 !== self })
        if pressPaint.lookInkLent != nil { endInk(below: self, by: self) }
        paintLayer?.removeAnimation(forKey: "pressOpacity")
        paintLayer?.removeAnimation(forKey: "lookOpacity")
        paintLayer?.removeAnimation(forKey: "nativeActiveOpacity")
        paintLayer?.removeAnimation(forKey: "nativeActiveScale")
        pressPaint = PressPaint()
    }
}


extension Presenter {
    /// Read sizes before applying the batch. Moving a snapshot with its node
    /// preserves the colour fade; changing the pixels it holds invalidates it.
    func pressPaintChanges(_ batch: Batch) -> [UInt32] {
        var changed: [UInt32] = []
        for op in batch.ops {
            switch op.op {
            case .props, .style, .children, .paragraph, .flow, .svg, .fieldContent: break
            case .frame:
                #if os(iOS) || os(tvOS)
                let size = views[op.id]?.layoutBounds.size ?? flats.leaves[op.id]?.layout.size
                #else
                let size = views[op.id]?.layoutBounds.size
                #endif
                if size == CGSize(width: op.w, height: op.h) {
                    // Its own snapshot moves with it; an ancestor's keeps
                    // the pixels at the child's old position.
                    var parent = views[op.id]?.superview
                    while let view = parent {
                        if let node = view as? NodeView { changed.append(node.id); break }
                        parent = view.superview
                    }
                    #if os(iOS) || os(tvOS)
                    if let host = flats.hostID(of: op.id) { changed.append(host) }
                    #endif
                    continue
                }
            default: continue
            }
            changed.append(op.id)
            #if os(iOS) || os(tvOS)
            if let host = flats.hostID(of: op.id) { changed.append(host) }
            #endif
        }
        return changed
    }
}
